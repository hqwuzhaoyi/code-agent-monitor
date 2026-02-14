//! 输入等待检测模块 - 检测 Agent 是否在等待用户输入
//!
//! 使用 AI 判断 Agent 状态，避免硬编码特定工具的模式。
//! 参考 CLAUDE.md "避免硬编码 AI 工具特定模式" 原则。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::agent::manager::AgentStatus;
use crate::anthropic::is_agent_processing;
use crate::infra::terminal::truncate_for_status;

/// 输入等待检测结果
#[derive(Debug, Clone)]
pub struct InputWaitResult {
    /// 是否在等待输入
    pub is_waiting: bool,
    /// 匹配到的模式类型
    pub pattern_type: Option<InputWaitPattern>,
    /// 终端输出的最后几行（上下文）
    pub context: String,
}

/// 等待输入的模式类型
///
/// 注意：使用 AI 判断时，无法区分具体类型，统一返回 `Other`。
/// 保留这些类型是为了向后兼容。
#[derive(Debug, Clone, PartialEq)]
pub enum InputWaitPattern {
    /// Claude Code 的 > 提示符
    ClaudePrompt,
    /// 确认提示 [Y/n] 或 [y/N]
    Confirmation,
    /// 按回车继续
    PressEnter,
    /// 继续执行提示
    Continue,
    /// 冒号结尾的提示
    ColonPrompt,
    /// 权限请求
    PermissionRequest,
    /// 其他等待模式（AI 判断时使用此类型）
    Other,
}

/// 输入等待检测器
///
/// 使用 AI 判断 Agent 是否在等待用户输入，而不是硬编码正则模式。
/// 这样可以兼容不同的 AI 编码工具（Claude Code、Codex、OpenCode 等）。
pub struct InputWaitDetector {
    /// 每个 session 的上次输出
    last_outputs: HashMap<String, String>,
    /// 每个 session 的上次变化时间
    last_change_times: HashMap<String, Instant>,
    /// 空闲检测阈值（秒）
    idle_threshold: Duration,
}

impl InputWaitDetector {
    /// 创建新的检测器
    pub fn new() -> Self {
        Self::with_idle_threshold(Duration::from_secs(3))
    }

    /// 创建带自定义空闲阈值的检测器
    pub fn with_idle_threshold(idle_threshold: Duration) -> Self {
        Self {
            last_outputs: HashMap::new(),
            last_change_times: HashMap::new(),
            idle_threshold,
        }
    }

    /// 检测是否在等待输入
    ///
    /// 使用 AI 判断 Agent 状态，而不是硬编码正则模式。
    /// 只有在输出空闲（超过阈值时间没有变化）时才调用 AI 判断。
    pub fn detect(&mut self, session_name: &str, output: &str) -> InputWaitResult {
        let now = Instant::now();

        // 检查输出是否有变化
        let is_idle = if let Some(last_output) = self.last_outputs.get(session_name) {
            if output == last_output {
                // 输出没变化，检查是否超过空闲阈值
                if let Some(last_change) = self.last_change_times.get(session_name) {
                    now.duration_since(*last_change) >= self.idle_threshold
                } else {
                    false
                }
            } else {
                // 输出有变化，更新记录
                self.last_outputs.insert(session_name.to_string(), output.to_string());
                self.last_change_times.insert(session_name.to_string(), now);
                false
            }
        } else {
            // 首次检测，记录输出
            self.last_outputs.insert(session_name.to_string(), output.to_string());
            self.last_change_times.insert(session_name.to_string(), now);
            false
        };

        // 如果不是空闲状态，不检测等待模式
        if !is_idle {
            return InputWaitResult {
                is_waiting: false,
                pattern_type: None,
                context: String::new(),
            };
        }

        // 空闲状态，使用 AI 判断
        self.detect_immediate(output)
    }

    /// 立即检测（不考虑空闲时间）
    ///
    /// 使用 AI 判断 Agent 是否在等待用户输入。
    /// 这种方式比硬编码模式更灵活，可以兼容不同的 AI 编码工具。
    pub fn detect_immediate(&self, output: &str) -> InputWaitResult {
        let context = truncate_for_status(output);

        match is_agent_processing(&context) {
            AgentStatus::WaitingForInput => InputWaitResult {
                is_waiting: true,
                pattern_type: Some(InputWaitPattern::Other),
                context,
            },
            AgentStatus::Processing | AgentStatus::Unknown => InputWaitResult {
                is_waiting: false,
                pattern_type: None,
                context,
            },
        }
    }

    /// 清除 session 的状态
    pub fn clear_session(&mut self, session_name: &str) {
        self.last_outputs.remove(session_name);
        self.last_change_times.remove(session_name);
    }
}

impl Default for InputWaitDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // 单元测试 - 不需要 API
    // =========================================================================

    #[test]
    fn test_idle_detection_first_call() {
        let mut detector = InputWaitDetector::with_idle_threshold(Duration::from_millis(100));
        let session = "test-session";
        let output = "Some output";

        // 首次检测，不应该是等待状态（还没空闲）
        let result = detector.detect(session, output);
        assert!(!result.is_waiting);
        assert!(result.context.is_empty());
    }

    #[test]
    fn test_output_change_resets_idle() {
        let mut detector = InputWaitDetector::with_idle_threshold(Duration::from_millis(100));
        let session = "test-session";

        // 首次检测
        detector.detect(session, "output 1");

        // 等待超过阈值
        std::thread::sleep(Duration::from_millis(150));

        // 输出变化，应该重置空闲计时，不触发 AI 检测
        let result = detector.detect(session, "output 2");
        assert!(!result.is_waiting);
        assert!(result.context.is_empty());
    }

    #[test]
    fn test_clear_session() {
        let mut detector = InputWaitDetector::new();
        let session = "test-session";

        detector.detect(session, "some output");
        assert!(detector.last_outputs.contains_key(session));

        detector.clear_session(session);
        assert!(!detector.last_outputs.contains_key(session));
    }

    #[test]
    fn test_input_wait_pattern_equality() {
        assert_eq!(InputWaitPattern::Other, InputWaitPattern::Other);
        assert_ne!(InputWaitPattern::Other, InputWaitPattern::Confirmation);
    }

    #[test]
    fn test_input_wait_result_clone() {
        let result = InputWaitResult {
            is_waiting: true,
            pattern_type: Some(InputWaitPattern::Other),
            context: "test context".to_string(),
        };
        let cloned = result.clone();
        assert_eq!(cloned.is_waiting, result.is_waiting);
        assert_eq!(cloned.pattern_type, result.pattern_type);
        assert_eq!(cloned.context, result.context);
    }

    // =========================================================================
    // 集成测试 - 需要 API（使用 #[ignore] 标记）
    // 运行: cargo test --package cam -- --ignored
    // =========================================================================

    #[test]
    #[ignore = "requires Anthropic API key"]
    fn test_detect_immediate_waiting() {
        let detector = InputWaitDetector::new();
        // Claude Code 风格的等待输入提示
        let output = "Some output\n>\n";

        let result = detector.detect_immediate(output);

        // AI 应该判断为等待输入
        assert!(result.is_waiting);
        assert_eq!(result.pattern_type, Some(InputWaitPattern::Other));
    }

    #[test]
    #[ignore = "requires Anthropic API key"]
    fn test_detect_immediate_processing() {
        let detector = InputWaitDetector::new();
        // 处理中的状态
        let output = "Thinking...\n✢ Processing your request";

        let result = detector.detect_immediate(output);

        // AI 应该判断为处理中
        assert!(!result.is_waiting);
        assert_eq!(result.pattern_type, None);
    }

    #[test]
    #[ignore = "requires Anthropic API key"]
    fn test_detect_confirmation_prompt() {
        let detector = InputWaitDetector::new();
        let output = "Do you want to continue? [Y/n]";

        let result = detector.detect_immediate(output);

        // AI 应该判断为等待输入
        assert!(result.is_waiting);
    }

    #[test]
    #[ignore = "requires Anthropic API key"]
    fn test_idle_then_detect() {
        let mut detector = InputWaitDetector::with_idle_threshold(Duration::from_millis(100));
        let session = "test-session";
        let output = ">\n";

        // 首次检测，不应该是等待状态（还没空闲）
        let result1 = detector.detect(session, output);
        assert!(!result1.is_waiting);

        // 等待超过阈值
        std::thread::sleep(Duration::from_millis(150));

        // 再次检测，应该触发 AI 判断
        let result2 = detector.detect(session, output);
        // AI 应该判断为等待输入
        assert!(result2.is_waiting);
    }
}
