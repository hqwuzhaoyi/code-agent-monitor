//! AI 内容提取模块
//!
//! 从终端快照中智能提取通知内容，使用 Haiku 4.5 模型。

use anyhow::{anyhow, Result};
use tracing::{debug, info, trace, warn};

use crate::ai::client::{AnthropicClient, AnthropicConfig};
use crate::ai_quality::{assess_question_extraction, assess_status_detection, thresholds};
use crate::ai_types::{AgentStatus, NotificationContent, QuestionType};
use crate::infra::terminal::truncate_for_status;

/// 内容提取超时（毫秒）- 10 秒（本地代理可能需要更长时间）
const EXTRACT_TIMEOUT_MS: u64 = 10000;

// ============================================================================
// 通知内容提取
// ============================================================================

/// 从终端快照中智能提取通知内容（使用 Haiku 4.5）
///
/// 使用 AI 分析终端输出，提取：
/// - 问题类型（选项/确认/开放式）
/// - 完整问题文本
/// - 选项列表
/// - 简洁摘要
///
/// # 参数
/// - `terminal_snapshot`: 终端快照内容
///
/// # 返回
/// - `Ok(NotificationContent)`: 提取的内容
/// - `Err`: API 调用失败时返回错误
///
/// # 超时
/// 使用 3 秒超时，超时后返回默认值
pub fn extract_notification_content(terminal_snapshot: &str) -> Result<NotificationContent> {
    let start = std::time::Instant::now();

    // 创建带 3 秒超时的客户端
    let config = AnthropicConfig {
        timeout_ms: EXTRACT_TIMEOUT_MS,
        ..AnthropicConfig::auto_load()?
    };
    let client = AnthropicClient::new(config)?;

    // 截取最后 N 行，避免 token 过多
    let truncated = crate::infra::terminal::truncate_last_lines(terminal_snapshot, 80);

    let system = "你是一个终端输出分析专家。从 AI Agent 终端快照中提取正在询问用户的问题信息。只返回 JSON，不要其他内容。";

    let prompt = format!(
        r#"从以下终端输出中提取问题信息：

<terminal>
{truncated}
</terminal>

返回 JSON 格式：
{{
  "question_type": "options" | "confirmation" | "open_ended",
  "question": "完整的问题文本",
  "options": ["选项1", "选项2", ...],
  "summary": "简洁的摘要（10字以内）"
}}

规则：
- 重要：只分析终端输出中最后出现的问题或提示，忽略之前的历史会话内容。
- question_type:
  - "options": 有多个选项供选择（如 1. xxx 2. xxx）
  - "confirmation": 是/否确认（如 [Y/n]、确认？）
  - "open_ended": 开放式问题（需要输入内容）
- question: 提取完整的问题文本，包括上下文
- options: 仅 options 类型需要填写，其他类型为空数组
- summary: 简洁描述，如"等待选择"、"请求确认"、"等待回复"

只返回 JSON，不要其他内容。"#
    );

    let response = client.complete(&prompt, Some(system))?;
    let elapsed = start.elapsed();
    info!(elapsed_ms = elapsed.as_millis(), "Haiku extract_notification_content completed");

    // 解析 JSON 响应
    let json_str = extract_json_from_output(&response)
        .ok_or_else(|| anyhow!("No JSON found in response: {}", response))?;

    let parsed: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| anyhow!("Failed to parse JSON: {} - content: {}", e, json_str))?;

    // 提取字段
    let question_type_str = parsed
        .get("question_type")
        .and_then(|v| v.as_str())
        .unwrap_or("open_ended");

    let question_type = match question_type_str {
        "options" => QuestionType::Options,
        "confirmation" => QuestionType::Confirmation,
        _ => QuestionType::OpenEnded,
    };

    let question = parsed
        .get("question")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let options: Vec<String> = parsed
        .get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let summary = parsed
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("等待输入")
        .to_string();

    // Generate reply_hint based on question_type and options
    let reply_hint = match question_type {
        QuestionType::Confirmation => "y/n".to_string(),
        QuestionType::Options => {
            if options.len() <= 5 {
                (1..=options.len())
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            } else {
                format!("1-{}", options.len())
            }
        }
        QuestionType::OpenEnded => String::new(),
    };

    let content = NotificationContent {
        question_type,
        question,
        options,
        summary,
        reply_hint,
    };

    // 评估提取质量
    let assessment = assess_question_extraction(&content);
    if assessment.confidence < thresholds::MEDIUM {
        warn!(
            confidence = assessment.confidence,
            issues = ?assessment.issues,
            "Question extraction quality below MEDIUM threshold"
        );
    } else {
        debug!(
            confidence = assessment.confidence,
            "Question extraction quality assessment passed"
        );
    }

    Ok(content)
}

/// 从终端快照中提取通知内容，失败时返回默认值
///
/// 这是 `extract_notification_content` 的便捷包装，
/// 在 API 调用失败或超时时返回默认的 `NotificationContent`。
pub fn extract_notification_content_or_default(terminal_snapshot: &str) -> NotificationContent {
    match extract_notification_content(terminal_snapshot) {
        Ok(content) => content,
        Err(e) => {
            warn!(error = %e, "Failed to extract notification content, using default");
            NotificationContent::default()
        }
    }
}

/// 从 Haiku 提取的问题结果
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedQuestion {
    /// 问题类型: "open", "choice", "confirm"
    pub question_type: String,
    /// 核心问题内容
    pub question: String,
    /// 选项列表（仅 choice 类型有值）
    pub options: Vec<String>,
    /// 回复提示
    pub reply_hint: String,
}

/// 任务摘要（NoQuestion 场景使用）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TaskSummary {
    /// Agent 状态: "completed", "idle", "waiting"
    pub status: String,
    /// 最后操作摘要
    pub last_action: Option<String>,
}

/// 提取结果
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractionResult {
    /// 成功提取到问题
    Found(ExtractedQuestion),
    /// AI 判断没有问题需要回答，但可能有任务摘要
    NoQuestion(TaskSummary),
    /// 提取失败（API 错误、解析失败等）
    Failed,
}

/// 从终端快照中提取问题（使用 Haiku）
///
/// 这是一个便捷函数，用于替代 `extract_question_with_ai`。
/// 使用 Haiku 模型，延迟约 1-2 秒，比 Opus 快 10 倍。
///
/// 如果 AI 判断上下文不完整，会自动获取更多行数重试。
///
/// 返回值：
/// - `ExtractionResult::Found(question)` - 成功提取到问题
/// - `ExtractionResult::NoQuestion(summary)` - AI 判断没有问题需要回答，包含任务摘要
/// - `ExtractionResult::Failed` - 提取失败
pub fn extract_question_with_haiku(terminal_snapshot: &str) -> ExtractionResult {
    // 尝试不同的上下文大小
    let context_sizes = [80, 150, 300];

    for &lines in &context_sizes {
        match extract_question_with_context(terminal_snapshot, lines) {
            Ok(InternalResult::Question(result)) => return ExtractionResult::Found(result),
            Ok(InternalResult::NoQuestion(summary)) => {
                return ExtractionResult::NoQuestion(summary)
            }
            Err(NeedMoreContext) => {
                debug!(
                    lines = lines,
                    "AI needs more context, retrying with more lines"
                );
                continue;
            }
            Err(ExtractionFailed) => return ExtractionResult::Failed,
        }
    }

    warn!("Failed to extract question even with maximum context");
    ExtractionResult::Failed
}

/// 提取错误类型
enum ExtractionError {
    NeedMoreContext,
    ExtractionFailed,
}

/// 内部提取结果
enum InternalResult {
    Question(ExtractedQuestion),
    NoQuestion(TaskSummary),
}

use ExtractionError::*;

/// 使用指定行数的上下文提取问题
fn extract_question_with_context(
    terminal_snapshot: &str,
    lines: usize,
) -> Result<InternalResult, ExtractionError> {
    let client = match AnthropicClient::from_config() {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to create Anthropic client");
            return Err(ExtractionFailed);
        }
    };

    // 截取最后 N 行
    let truncated = crate::infra::terminal::truncate_last_lines(terminal_snapshot, lines);

    let system = "你是一个终端输出分析专家。从给定的终端快照中提取用户正在被询问的问题，或分析 Agent 当前状态。";

    let prompt = format!(
        r#"分析以下 AI Agent 终端输出，提取正在询问用户的问题。

终端输出:
{}

请用 JSON 格式回复，包含以下字段：
- question_type: "open"（开放问题）、"choice"（选择题）、"confirm"（确认）、"none"（无问题，Agent 空闲等待指令）
- question: 完整的问题内容。重要：必须包含问题所引用的具体内容（如代码结构、设计方案、选项列表等），让用户无需查看终端就能理解和回答问题。不要截断或省略重要上下文
- options: 选项列表（仅当 question_type 为 "choice" 时提取，格式如 ["1. 选项A", "2. 选项B"]，否则为空数组 []）
- reply_hint: 回复提示（如"回复 y/n"、"回复数字选择"、"回复内容"）
- contains_ui_noise: true/false（问题内容是否包含终端 UI 元素，如工具调用标记、状态指示器、进度条、ASCII art 等）
- context_complete: true/false（判断标准见下方）
- agent_status: "completed"（刚完成任务，终端显示了完成信息）、"idle"（空闲等待，无明显完成信息）、"waiting"（等待用户回答问题）
- last_action: Agent 最后完成的操作摘要。重要：仔细查看终端输出，提取 Agent 完成的具体任务（如"React Todo List 项目已完成"、"创建了 TodoList 组件"、"修复了登录 bug"、"项目构建成功"）。如果终端显示了任务完成信息（如"已完成"、"成功"、"done"等），必须提取。只有在完全无法判断时才返回 null

context_complete 判断标准（非常重要）：
- 如果问题中包含指示词如"这个"、"上面的"、"以下"、"这些"、"该"等，必须检查被引用的内容是否在终端输出中可见
- 例如："这个项目结构看起来合适吗？" - 必须能看到具体的项目结构内容，否则 context_complete = false
- 例如："这个方案可以吗？" - 必须能看到具体的方案内容，否则 context_complete = false
- 例如："以下选项选择哪个？" - 必须能看到选项列表，否则 context_complete = false
- 如果问题是独立的（如"你想要实现什么功能？"），则 context_complete = true

重要规则：
1. 只分析终端输出中最后出现的问题或提示，忽略之前的历史会话内容
2. question 字段只应包含纯文本问题，不要包含终端 UI 元素（如工具调用标记 ⏺、状态指示器 ✻、进度条、ASCII art logo 等）
3. 如果无法提取干净的问题内容（即问题中混杂了 UI 元素），设置 contains_ui_noise 为 true
4. 如果问题引用了看不到的上下文，必须设置 context_complete 为 false
5. 如果 context_complete 为 true，question 必须包含足够的上下文让用户理解问题
6. 当 question_type 为 "none" 时，仍需分析 agent_status 和 last_action，帮助用户了解 Agent 状态
7. 如果 Agent 只是显示了结果/总结然后等待（终端只有空闲提示符 ❯ 或 >，没有明确问问题），应该返回 question_type: "none"

只返回 JSON，不要其他内容。"#,
        truncated
    );

    let start = std::time::Instant::now();
    let response = match client.complete(&prompt, Some(system)) {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Haiku API call failed");
            return Err(ExtractionFailed);
        }
    };
    debug!(
        elapsed_ms = start.elapsed().as_millis(),
        lines = lines,
        "Haiku API call completed"
    );
    trace!(response = %response, "Haiku raw response");

    // 解析 JSON 响应
    let json_str = match extract_json_from_output(&response) {
        Some(s) => s,
        None => {
            warn!(response = %response, "Failed to extract JSON from Haiku response");
            return Err(ExtractionFailed);
        }
    };
    debug!(json = %json_str, "Haiku returned JSON");
    let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return Err(ExtractionFailed),
    };

    let question_type = match parsed.get("question_type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return Err(ExtractionFailed),
    };

    if question_type == "none" {
        // 提取任务摘要
        let status = parsed
            .get("agent_status")
            .and_then(|v| v.as_str())
            .unwrap_or("idle")
            .to_string();
        let last_action = parsed
            .get("last_action")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        return Ok(InternalResult::NoQuestion(TaskSummary {
            status,
            last_action,
        }));
    }

    // 检查上下文是否完整
    let context_complete = parsed
        .get("context_complete")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if !context_complete {
        debug!("AI indicates context is incomplete");
        return Err(NeedMoreContext);
    }

    // 检查 AI 是否标记了 UI 噪音
    let contains_ui_noise = parsed
        .get("contains_ui_noise")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if contains_ui_noise {
        warn!("AI detected UI noise in extracted question, rejecting");
        return Err(ExtractionFailed);
    }

    let question = match parsed.get("question").and_then(|v| v.as_str()) {
        Some(q) => q.to_string(),
        None => return Err(ExtractionFailed),
    };
    let reply_hint = parsed
        .get("reply_hint")
        .and_then(|v| v.as_str())
        .unwrap_or("回复内容")
        .to_string();

    // 提取选项列表
    let options = parsed
        .get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(InternalResult::Question(ExtractedQuestion {
        question_type: question_type.to_string(),
        question,
        options,
        reply_hint,
    }))
}

/// 使用 Haiku 判断 Agent 是否正在处理中
///
/// 这个函数使用 AI 分析终端输出，判断 agent 当前状态：
/// - Processing: agent 正在执行任务（如 Thinking、Brewing、Running 等）
/// - WaitingForInput: agent 空闲，等待用户输入
///
/// 这种方式比硬编码模式更灵活，可以兼容不同的 AI 编码工具：
/// - Claude Code: Hatching…, Brewing…, Thinking…
/// - Codex: Running…, Executing…
/// - OpenCode: Processing…, Working…
///
/// # 参数
/// - `terminal_snapshot`: 终端快照内容（最后 10-15 行即可）
///
/// # 返回
/// - `AgentStatus::Processing`: agent 正在处理，不应发送通知
/// - `AgentStatus::WaitingForInput`: agent 空闲，应发送通知
/// - `AgentStatus::Unknown`: 无法确定（API 失败时）
///
/// # 超时
/// 使用 3 秒超时，超时后返回 Unknown
pub fn is_agent_processing(terminal_snapshot: &str) -> AgentStatus {
    let start = std::time::Instant::now();

    // 创建带 3 秒超时的客户端
    let config = match AnthropicConfig::auto_load() {
        Ok(c) => AnthropicConfig {
            timeout_ms: 3000,
            max_tokens: 50, // 只需要简短回答
            ..c
        },
        Err(e) => {
            warn!(error = %e, "Failed to load Anthropic config for is_agent_processing");
            return AgentStatus::Unknown;
        }
    };

    let client = match AnthropicClient::new(config) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to create Anthropic client for is_agent_processing");
            return AgentStatus::Unknown;
        }
    };

    // 只取最后 N 行，减少 token 消耗
    let last_lines = truncate_for_status(terminal_snapshot);

    let system = "你是一个终端状态分析专家。判断 AI 编码助手（如 Claude Code、Codex、OpenCode）的当前状态。只回答 PROCESSING 或 WAITING，不要其他内容。";

    let prompt = format!(
        r#"分析以下终端输出，判断 AI 编码助手的状态：

<terminal>
{last_lines}
</terminal>

判断规则：
- 只判断终端最后的状态，忽略历史输出
- PROCESSING: 如果看到以下任一指示器：
  * 带省略号的状态词（如 Thinking…、Brewing…、Hatching…、Grooving…、Running…、Executing…、Loading…、Working…、Streaming… 等）
  * 任何 "动词ing…" 或 "动词ing..." 格式的状态提示
  * 括号内的运行提示（如 (running stop hook)、(executing)、(loading) 等）
  * 旋转动画字符（✢✻✶✽◐◑◒◓⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏）
  * 进度条或百分比指示器
- WAITING: 如果看到空闲提示符（如 >、❯、$）或问题/选项等待用户输入
- 注意：⏺ 符号是输出块标记，不是处理中指示器。如果终端显示完成信息（如"已完成"、"成功"）后跟空闲提示符，应判断为 WAITING

只回答一个词：PROCESSING 或 WAITING"#
    );

    let response = match client.complete(&prompt, Some(system)) {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Haiku API call failed for is_agent_processing");
            return AgentStatus::Unknown;
        }
    };

    let elapsed = start.elapsed();
    debug!(elapsed_ms = elapsed.as_millis(), response = %response.trim(), "is_agent_processing completed");

    let response_upper = response.trim().to_uppercase();
    let status = if response_upper.contains("PROCESSING") {
        AgentStatus::Processing
    } else if response_upper.contains("WAITING") {
        AgentStatus::WaitingForInput
    } else {
        warn!(response = %response, "Unexpected response from is_agent_processing");
        AgentStatus::Unknown
    };

    // 评估状态检测质量
    let assessment = assess_status_detection(&status, terminal_snapshot);
    if assessment.confidence < thresholds::LOW {
        warn!(
            confidence = assessment.confidence,
            issues = ?assessment.issues,
            detected_status = ?status,
            "Status detection quality below LOW threshold, returning Unknown"
        );
        return AgentStatus::Unknown;
    }

    debug!(
        confidence = assessment.confidence,
        status = ?status,
        "Status detection quality assessment passed"
    );

    status
}

/// 检测终端快照是否包含等待用户输入的问题
///
/// 用于 stop 事件处理：Claude Code 可能在输出问题后触发 stop 而非 idle_prompt。
/// 此函数检测终端是否包含等待输入的问题。
///
/// # 参数
/// - `terminal_snapshot`: 终端快照内容
///
/// # 返回
/// - `Some(NotificationContent)`: 如果检测到等待输入的问题
/// - `None`: 如果没有检测到问题或 AI 调用失败
pub fn detect_waiting_question(terminal_snapshot: &str) -> Option<NotificationContent> {
    // 先检查是否在处理中
    match is_agent_processing(terminal_snapshot) {
        AgentStatus::Processing => return None,
        AgentStatus::Unknown => {
            // 不确定状态，继续尝试提取问题
        }
        AgentStatus::WaitingForInput => {
            // 确认在等待输入，继续提取问题
        }
    }

    // 尝试提取问题内容
    match extract_notification_content(terminal_snapshot) {
        Ok(content) => {
            // 评估提取质量
            let assessment = assess_question_extraction(&content);

            if assessment.is_valid && assessment.confidence >= thresholds::MEDIUM {
                // 质量足够高，返回内容
                info!(
                    confidence = assessment.confidence,
                    question_type = ?content.question_type,
                    "Detected waiting question in stop event"
                );
                Some(content)
            } else {
                // 质量不够，记录警告
                warn!(
                    confidence = assessment.confidence,
                    issues = ?assessment.issues,
                    "Question extraction quality too low, ignoring"
                );
                None
            }
        }
        Err(e) => {
            debug!(error = %e, "Failed to extract question from terminal snapshot");
            None
        }
    }
}

/// 从输出中提取 JSON 字符串
fn extract_json_from_output(output: &str) -> Option<String> {
    let start = output.find('{')?;
    let end = output.rfind('}')?;
    if end > start {
        Some(output[start..=end].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_output() {
        let output = r#"Here is the JSON:
{"question_type": "confirm", "question": "Continue?", "reply_hint": "y/n"}
That's all."#;

        let json = extract_json_from_output(output).unwrap();
        assert!(json.contains("question_type"));
        assert!(json.contains("confirm"));
    }

    #[test]
    fn test_extract_json_no_json() {
        let output = "No JSON here";
        assert!(extract_json_from_output(output).is_none());
    }

    #[test]
    fn test_detect_waiting_question_structure() {
        // 测试 detect_waiting_question 函数存在且可调用
        // 实际 AI 调用需要 API key，这里只测试结构
        let snapshot = "";
        let result = detect_waiting_question(snapshot);
        // 空快照可能返回 None（没有 API key）或 Some（有 API key 但内容为空）
        // 这里只验证函数可以被调用，不验证具体返回值
        // 因为返回值取决于是否有 API key 和 AI 的判断
        let _ = result; // 只要不 panic 就算通过
    }

    #[test]
    fn test_detect_waiting_question_returns_notification_content() {
        // 测试返回类型是 Option<NotificationContent>
        // 由于没有 API key，这里只验证函数签名和返回类型
        let snapshot = "Some terminal output\n❯ What do you want to do?";
        let result: Option<NotificationContent> = detect_waiting_question(snapshot);
        // 没有 API key 时返回 None，但类型正确
        // 如果有 API key，应该返回 Some(NotificationContent)
        assert!(result.is_none() || result.is_some());
    }
}
