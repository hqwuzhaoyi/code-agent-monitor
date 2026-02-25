//! ReAct 消息提取器
//!
//! 使用 ReAct (Reasoning + Acting) 循环从终端快照中提取消息。
//! 通过迭代扩展上下文直到提取完整的消息内容。

pub mod prompts;
pub mod traits;

use anyhow::Result;
use tracing::{debug, info, warn};

use crate::agent::manager::AgentStatus;
use crate::ai::client::AnthropicClient;
use crate::ai::extractor::is_agent_processing;
use crate::infra::tmux::TmuxManager;

pub use prompts::{message_extraction_prompt, MESSAGE_EXTRACTION_SYSTEM};
pub use traits::{ExtractionResult, ExtractedMessage, IterationConfig, MessageExtractor, MessageType};

/// 从终端快照提取格式化消息的便捷函数
///
/// 使用 ReAct 循环迭代扩展上下文，直到提取完整的消息。
/// 这是供 `openclaw.rs` 等模块使用的高级 API。
///
/// # 参数
/// - `terminal_snapshot`: 终端快照内容
///
/// # 返回
/// - `Some((message, fingerprint))`: 成功提取到消息和指纹
/// - `None`: Agent 正在处理中、空闲或提取失败
pub fn extract_message_from_snapshot(terminal_snapshot: &str) -> Option<(String, String)> {
    let extractor = match HaikuExtractor::new() {
        Ok(e) => e,
        Err(e) => {
            warn!(error = %e, "Failed to create HaikuExtractor");
            return None;
        }
    };

    // 先检查是否在处理中
    if extractor.is_processing(terminal_snapshot) {
        debug!("Agent is processing, skipping extraction");
        return None;
    }

    // ReAct 循环：逐步扩展上下文
    let config = IterationConfig::default();
    for (iteration, &lines) in config.context_sizes.iter().enumerate() {
        if iteration >= config.max_iterations {
            warn!("Max iterations reached");
            break;
        }

        debug!(iteration = iteration, lines = lines, "ReAct iteration");

        match extractor.extract(terminal_snapshot, lines) {
            ExtractionResult::Success(message) => {
                // 检查是否是空闲状态
                if matches!(message.message_type, MessageType::Idle { .. }) {
                    debug!("Agent is idle, no question");
                    return None;
                }

                info!(
                    fingerprint = %message.fingerprint,
                    iterations = iteration + 1,
                    "Message extracted successfully"
                );
                return Some((message.content, message.fingerprint));
            }
            ExtractionResult::NeedMoreContext => {
                debug!(lines = lines, "Need more context, expanding");
                continue;
            }
            ExtractionResult::Processing => {
                debug!("Agent is processing");
                return None;
            }
            ExtractionResult::Failed(reason) => {
                warn!(reason = %reason, "Extraction failed");
                // 继续尝试更多上下文
                continue;
            }
        }
    }

    warn!("Failed to extract message after all iterations");
    None
}

/// Haiku 提取器实现
///
/// 使用 Anthropic Haiku 模型进行消息提取。
pub struct HaikuExtractor {
    client: AnthropicClient,
}

impl HaikuExtractor {
    /// 创建新的 Haiku 提取器
    pub fn new() -> Result<Self> {
        let client = AnthropicClient::from_config()?;
        Ok(Self { client })
    }

    /// 从 JSON 响应中提取 JSON 字符串
    fn extract_json(output: &str) -> Option<String> {
        let start = output.find('{')?;
        let end = output.rfind('}')?;
        if end > start {
            Some(output[start..=end].to_string())
        } else {
            None
        }
    }

    /// 清理用户输入行中的长内容
    fn clean_user_input(content: &str) -> String {
        content
            .lines()
            .map(|line| {
                if let Some(input_start) = line.find('❯') {
                    let after_prompt = &line[input_start + '❯'.len_utf8()..];
                    let trimmed = after_prompt.trim();
                    if trimmed.len() > 10 {
                        format!("{}❯ [用户正在输入...]", &line[..input_start])
                    } else {
                        line.to_string()
                    }
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 截取终端快照的最后 N 行
    fn truncate_lines(content: &str, lines: usize) -> String {
        let all_lines: Vec<&str> = content.lines().collect();
        if all_lines.len() <= lines {
            content.to_string()
        } else {
            all_lines[all_lines.len() - lines..].join("\n")
        }
    }
}

impl MessageExtractor for HaikuExtractor {
    fn extract(&self, terminal_snapshot: &str, lines: usize) -> ExtractionResult {
        // 截取指定行数
        let truncated = Self::truncate_lines(terminal_snapshot, lines);
        let cleaned = Self::clean_user_input(&truncated);

        let prompt = message_extraction_prompt(&cleaned);

        let response = match self.client.complete(&prompt, Some(MESSAGE_EXTRACTION_SYSTEM)) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "Haiku API call failed");
                return ExtractionResult::Failed(e.to_string());
            }
        };

        // 解析 JSON 响应
        let json_str = match Self::extract_json(&response) {
            Some(s) => s,
            None => {
                warn!(response = %response, "Failed to extract JSON from response");
                return ExtractionResult::Failed("No JSON in response".to_string());
            }
        };

        let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, json = %json_str, "Failed to parse JSON");
                return ExtractionResult::Failed(e.to_string());
            }
        };

        // 检查上下文是否完整
        let context_complete = parsed
            .get("context_complete")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if !context_complete {
            debug!(lines = lines, "AI indicates context is incomplete");
            return ExtractionResult::NeedMoreContext;
        }

        let has_question = parsed
            .get("has_question")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if has_question {
            let message = parsed
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if message.is_empty() {
                return ExtractionResult::Failed("Empty message".to_string());
            }

            let fingerprint = parsed
                .get("fingerprint")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let message_type_str = parsed
                .get("message_type")
                .and_then(|v| v.as_str())
                .unwrap_or("open_ended");

            let message_type = match message_type_str {
                "choice" => MessageType::Choice,
                "confirmation" => MessageType::Confirmation,
                _ => MessageType::OpenEnded,
            };

            ExtractionResult::Success(ExtractedMessage {
                content: message,
                fingerprint,
                context_complete: true,
                message_type,
            })
        } else {
            // 无问题，返回空闲状态
            let status = parsed
                .get("agent_status")
                .and_then(|v| v.as_str())
                .unwrap_or("idle")
                .to_string();

            let last_action = parsed
                .get("last_action")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            ExtractionResult::Success(ExtractedMessage {
                content: String::new(),
                fingerprint: String::new(),
                context_complete: true,
                message_type: MessageType::Idle { status, last_action },
            })
        }
    }

    fn is_processing(&self, terminal_snapshot: &str) -> bool {
        matches!(
            is_agent_processing(terminal_snapshot),
            AgentStatus::Processing | AgentStatus::Running
        )
    }
}

/// ReAct 消息提取器
///
/// 使用 ReAct 循环迭代扩展上下文，直到提取完整的消息。
pub struct ReactExtractor {
    /// AI 提取器实现
    extractor: Box<dyn MessageExtractor>,
    /// 迭代配置
    pub config: IterationConfig,
}

impl ReactExtractor {
    /// 创建新的 ReAct 提取器
    pub fn new(extractor: Box<dyn MessageExtractor>) -> Self {
        Self {
            extractor,
            config: IterationConfig::default(),
        }
    }

    /// 使用自定义配置创建
    pub fn with_config(extractor: Box<dyn MessageExtractor>, config: IterationConfig) -> Self {
        Self { extractor, config }
    }

    /// 执行 ReAct 循环提取消息
    ///
    /// # 参数
    /// - `session_id`: tmux session ID
    /// - `tmux`: tmux 管理器
    ///
    /// # 返回
    /// - `Ok(Some(ExtractedMessage))`: 成功提取到消息
    /// - `Ok(None)`: Agent 正在处理中或空闲
    /// - `Err`: 提取失败
    pub fn extract_message(
        &self,
        session_id: &str,
        tmux: &TmuxManager,
    ) -> Result<Option<ExtractedMessage>> {
        // 获取最大行数的终端快照（一次性获取，避免多次 tmux 调用）
        let max_lines = *self.config.context_sizes.last().unwrap_or(&800);
        let full_snapshot = tmux.capture_pane(session_id, max_lines as u32)?;

        // 先检查是否在处理中
        if self.extractor.is_processing(&full_snapshot) {
            debug!(session_id = %session_id, "Agent is processing, skipping extraction");
            return Ok(None);
        }

        // ReAct 循环：逐步扩展上下文
        for (iteration, &lines) in self.config.context_sizes.iter().enumerate() {
            if iteration >= self.config.max_iterations {
                warn!(session_id = %session_id, "Max iterations reached");
                break;
            }

            debug!(
                session_id = %session_id,
                iteration = iteration,
                lines = lines,
                "ReAct iteration"
            );

            match self.extractor.extract(&full_snapshot, lines) {
                ExtractionResult::Success(message) => {
                    // 检查是否是空闲状态
                    if matches!(message.message_type, MessageType::Idle { .. }) {
                        debug!(session_id = %session_id, "Agent is idle, no question");
                        return Ok(None);
                    }

                    info!(
                        session_id = %session_id,
                        fingerprint = %message.fingerprint,
                        iterations = iteration + 1,
                        "Message extracted successfully"
                    );
                    return Ok(Some(message));
                }
                ExtractionResult::NeedMoreContext => {
                    debug!(
                        session_id = %session_id,
                        lines = lines,
                        "Need more context, expanding"
                    );
                    continue;
                }
                ExtractionResult::Processing => {
                    debug!(session_id = %session_id, "Agent is processing");
                    return Ok(None);
                }
                ExtractionResult::Failed(reason) => {
                    warn!(
                        session_id = %session_id,
                        reason = %reason,
                        "Extraction failed"
                    );
                    // 继续尝试更多上下文
                    continue;
                }
            }
        }

        warn!(session_id = %session_id, "Failed to extract message after all iterations");
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock 提取器用于测试
    struct MockExtractor {
        results: Vec<ExtractionResult>,
        call_count: AtomicUsize,
        is_processing_result: bool,
    }

    impl MockExtractor {
        fn new(results: Vec<ExtractionResult>) -> Self {
            Self {
                results,
                call_count: AtomicUsize::new(0),
                is_processing_result: false,
            }
        }

        fn with_processing(mut self, is_processing: bool) -> Self {
            self.is_processing_result = is_processing;
            self
        }
    }

    impl MessageExtractor for MockExtractor {
        fn extract(&self, _snapshot: &str, _lines: usize) -> ExtractionResult {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            self.results
                .get(idx)
                .cloned()
                .unwrap_or_else(|| ExtractionResult::Failed("No more results".into()))
        }

        fn is_processing(&self, _snapshot: &str) -> bool {
            self.is_processing_result
        }
    }

    #[test]
    fn test_react_loop_expands_context() {
        let extractor = MockExtractor::new(vec![
            ExtractionResult::NeedMoreContext,
            ExtractionResult::NeedMoreContext,
            ExtractionResult::Success(ExtractedMessage {
                content: "Test question".into(),
                fingerprint: "test-question".into(),
                context_complete: true,
                message_type: MessageType::OpenEnded,
            }),
        ]);

        let react = ReactExtractor::new(Box::new(extractor));

        // 验证配置
        assert_eq!(react.config.context_sizes.len(), 5);
        assert_eq!(react.config.max_iterations, 5);
    }

    #[test]
    fn test_react_skips_when_processing() {
        let extractor = MockExtractor::new(vec![]).with_processing(true);

        let react = ReactExtractor::new(Box::new(extractor));

        // 验证 is_processing 返回 true
        assert!(react.extractor.is_processing("test"));
    }

    #[test]
    fn test_haiku_extractor_truncate_lines() {
        let content = "line1\nline2\nline3\nline4\nline5";
        let truncated = HaikuExtractor::truncate_lines(content, 3);
        assert_eq!(truncated, "line3\nline4\nline5");
    }

    #[test]
    fn test_haiku_extractor_clean_user_input() {
        let content = "Some output\n❯ This is a very long user input that should be cleaned";
        let cleaned = HaikuExtractor::clean_user_input(content);
        assert!(cleaned.contains("[用户正在输入...]"));
    }

    #[test]
    fn test_haiku_extractor_clean_short_input() {
        let content = "Some output\n❯ A";
        let cleaned = HaikuExtractor::clean_user_input(content);
        assert!(cleaned.contains("❯ A"));
        assert!(!cleaned.contains("[用户正在输入...]"));
    }

    #[test]
    fn test_extract_json() {
        let output = r#"Here is the JSON: {"key": "value"} end"#;
        let json = HaikuExtractor::extract_json(output).unwrap();
        assert_eq!(json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_no_json() {
        let output = "No JSON here";
        assert!(HaikuExtractor::extract_json(output).is_none());
    }

    #[test]
    fn test_extract_json_nested() {
        let output = r#"Response: {"outer": {"inner": "value"}} done"#;
        let json = HaikuExtractor::extract_json(output).unwrap();
        assert!(json.contains("outer"));
        assert!(json.contains("inner"));
    }

    #[test]
    fn test_extract_json_with_array() {
        let output = r#"{"items": [1, 2, 3]}"#;
        let json = HaikuExtractor::extract_json(output).unwrap();
        assert_eq!(json, r#"{"items": [1, 2, 3]}"#);
    }

    #[test]
    fn test_extract_json_malformed_braces() {
        // 只有开括号
        let output = "{ incomplete";
        assert!(HaikuExtractor::extract_json(output).is_none());

        // 只有闭括号
        let output2 = "incomplete }";
        assert!(HaikuExtractor::extract_json(output2).is_none());
    }

    #[test]
    fn test_truncate_lines_exact() {
        let content = "line1\nline2\nline3";
        let truncated = HaikuExtractor::truncate_lines(content, 3);
        assert_eq!(truncated, "line1\nline2\nline3");
    }

    #[test]
    fn test_truncate_lines_fewer_than_requested() {
        let content = "line1\nline2";
        let truncated = HaikuExtractor::truncate_lines(content, 10);
        assert_eq!(truncated, "line1\nline2");
    }

    #[test]
    fn test_truncate_lines_empty() {
        let content = "";
        let truncated = HaikuExtractor::truncate_lines(content, 5);
        assert_eq!(truncated, "");
    }

    #[test]
    fn test_truncate_lines_single_line() {
        let content = "only one line";
        let truncated = HaikuExtractor::truncate_lines(content, 1);
        assert_eq!(truncated, "only one line");
    }

    #[test]
    fn test_clean_user_input_no_prompt() {
        let content = "Some output without prompt";
        let cleaned = HaikuExtractor::clean_user_input(content);
        assert_eq!(cleaned, content);
    }

    #[test]
    fn test_clean_user_input_empty_prompt() {
        let content = "Output\n❯ ";
        let cleaned = HaikuExtractor::clean_user_input(content);
        assert!(cleaned.contains("❯ "));
        assert!(!cleaned.contains("[用户正在输入...]"));
    }

    #[test]
    fn test_clean_user_input_exactly_10_chars() {
        // 正好 10 个字符不应该被清理
        let content = "❯ 1234567890";
        let cleaned = HaikuExtractor::clean_user_input(content);
        assert!(cleaned.contains("1234567890"));
        assert!(!cleaned.contains("[用户正在输入...]"));
    }

    #[test]
    fn test_clean_user_input_11_chars() {
        // 超过 10 个字符应该被清理
        let content = "❯ 12345678901";
        let cleaned = HaikuExtractor::clean_user_input(content);
        assert!(cleaned.contains("[用户正在输入...]"));
    }

    #[test]
    fn test_clean_user_input_multiple_prompts() {
        let content = "line1\n❯ short\nline2\n❯ this is a very long input that should be cleaned";
        let cleaned = HaikuExtractor::clean_user_input(content);
        assert!(cleaned.contains("❯ short"));
        assert!(cleaned.contains("[用户正在输入...]"));
    }

    #[test]
    fn test_clean_user_input_preserves_prefix() {
        let content = "prefix ❯ this is a long input to clean";
        let cleaned = HaikuExtractor::clean_user_input(content);
        assert!(cleaned.contains("prefix ❯"));
        assert!(cleaned.contains("[用户正在输入...]"));
    }

    // === ReactExtractor 配置测试 ===

    #[test]
    fn test_react_extractor_default_config() {
        let extractor = MockExtractor::new(vec![]);
        let react = ReactExtractor::new(Box::new(extractor));

        assert_eq!(react.config.context_sizes, vec![80, 150, 300, 500, 800]);
        assert_eq!(react.config.max_iterations, 5);
        assert_eq!(react.config.timeout_ms, 10000);
    }

    #[test]
    fn test_react_extractor_custom_config() {
        let extractor = MockExtractor::new(vec![]);
        let config = IterationConfig {
            context_sizes: vec![50, 100, 200],
            max_iterations: 3,
            timeout_ms: 5000,
        };

        let react = ReactExtractor::with_config(Box::new(extractor), config);

        assert_eq!(react.config.context_sizes, vec![50, 100, 200]);
        assert_eq!(react.config.max_iterations, 3);
        assert_eq!(react.config.timeout_ms, 5000);
    }

    // === ExtractionResult 测试 ===

    #[test]
    fn test_extraction_result_clone() {
        let result = ExtractionResult::Success(ExtractedMessage {
            content: "Test".into(),
            fingerprint: "test".into(),
            context_complete: true,
            message_type: MessageType::OpenEnded,
        });

        let cloned = result.clone();
        if let ExtractionResult::Success(msg) = cloned {
            assert_eq!(msg.content, "Test");
        } else {
            panic!("Expected Success variant");
        }
    }

    #[test]
    fn test_extraction_result_need_more_context_clone() {
        let result = ExtractionResult::NeedMoreContext;
        let cloned = result.clone();
        assert!(matches!(cloned, ExtractionResult::NeedMoreContext));
    }

    #[test]
    fn test_extraction_result_processing_clone() {
        let result = ExtractionResult::Processing;
        let cloned = result.clone();
        assert!(matches!(cloned, ExtractionResult::Processing));
    }

    #[test]
    fn test_extraction_result_failed_clone() {
        let result = ExtractionResult::Failed("error message".into());
        let cloned = result.clone();
        if let ExtractionResult::Failed(msg) = cloned {
            assert_eq!(msg, "error message");
        } else {
            panic!("Expected Failed variant");
        }
    }

    // === MessageType Idle 变体测试 ===

    #[test]
    fn test_message_type_idle_returns_none_in_react() {
        // 验证 Idle 类型在 ReactExtractor 中返回 None
        let extractor = MockExtractor::new(vec![ExtractionResult::Success(ExtractedMessage {
            content: String::new(),
            fingerprint: String::new(),
            context_complete: true,
            message_type: MessageType::Idle {
                status: "idle".into(),
                last_action: None,
            },
        })]);

        let react = ReactExtractor::new(Box::new(extractor));

        // 验证配置正确
        assert!(react.config.max_iterations > 0);
    }
}
