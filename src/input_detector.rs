//! 输入等待检测模块 - 检测 Agent 是否在等待用户输入

use regex::Regex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

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
    /// 其他等待模式
    Other,
}

/// 输入等待检测器
pub struct InputWaitDetector {
    /// 每个 session 的上次输出
    last_outputs: HashMap<String, String>,
    /// 每个 session 的上次变化时间
    last_change_times: HashMap<String, Instant>,
    /// 空闲检测阈值（秒）
    idle_threshold: Duration,
    /// 等待输入的正则模式
    patterns: Vec<(Regex, InputWaitPattern)>,
}

impl InputWaitDetector {
    /// 创建新的检测器
    pub fn new() -> Self {
        Self::with_idle_threshold(Duration::from_secs(3))
    }

    /// 创建带自定义空闲阈值的检测器
    pub fn with_idle_threshold(idle_threshold: Duration) -> Self {
        let patterns = vec![
            // Claude Code 的提示符（支持 > 和 ❯）
            (Regex::new(r"(?m)^[>❯]\s*$").unwrap(), InputWaitPattern::ClaudePrompt),
            // Claude Code 的 ❯ 提示符（Unicode U+276F）
            (Regex::new(r"❯\s*$").unwrap(), InputWaitPattern::ClaudePrompt),
            // 确认提示 - 英文
            (Regex::new(r"\[Y/n\]").unwrap(), InputWaitPattern::Confirmation),
            (Regex::new(r"\[y/N\]").unwrap(), InputWaitPattern::Confirmation),
            (Regex::new(r"\[yes/no\]").unwrap(), InputWaitPattern::Confirmation),
            // Claude Code 实际格式: [Y]es / [N]o / [A]lways / [D]on't ask
            (Regex::new(r"\[Y\]es\s*/\s*\[N\]o").unwrap(), InputWaitPattern::Confirmation),
            (Regex::new(r"\[A\]lways").unwrap(), InputWaitPattern::Confirmation),
            // 确认提示 - 中文
            (Regex::new(r"\[是/否\]").unwrap(), InputWaitPattern::Confirmation),
            (Regex::new(r"确认[？?]").unwrap(), InputWaitPattern::Confirmation),
            // 按回车继续 - 英文
            (Regex::new(r"(?i)press enter").unwrap(), InputWaitPattern::PressEnter),
            (Regex::new(r"(?i)press any key").unwrap(), InputWaitPattern::PressEnter),
            // 按回车继续 - 中文
            (Regex::new(r"按.*继续").unwrap(), InputWaitPattern::PressEnter),
            (Regex::new(r"回车继续").unwrap(), InputWaitPattern::PressEnter),
            // 继续执行提示 - 英文
            (Regex::new(r"(?i)continue\?").unwrap(), InputWaitPattern::Continue),
            (Regex::new(r"(?i)proceed\?").unwrap(), InputWaitPattern::Continue),
            // 继续执行提示 - 中文
            (Regex::new(r"是否继续").unwrap(), InputWaitPattern::Continue),
            (Regex::new(r"继续执行[？?]").unwrap(), InputWaitPattern::Continue),
            // 权限请求 - 英文
            (Regex::new(r"(?i)allow this action").unwrap(), InputWaitPattern::PermissionRequest),
            (Regex::new(r"(?i)do you want to").unwrap(), InputWaitPattern::PermissionRequest),
            // 权限请求 - 中文
            (Regex::new(r"允许.*操作").unwrap(), InputWaitPattern::PermissionRequest),
            (Regex::new(r"是否授权").unwrap(), InputWaitPattern::PermissionRequest),
            // 冒号结尾的提示（最后一行以冒号结尾）- 英文和中文
            (Regex::new(r"[：:]\s*$").unwrap(), InputWaitPattern::ColonPrompt),
        ];

        Self {
            last_outputs: HashMap::new(),
            last_change_times: HashMap::new(),
            idle_threshold,
            patterns,
        }
    }

    /// 检测是否在等待输入
    ///
    /// 返回 Some(InputWaitResult) 如果检测到等待输入，否则返回 None
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

        // 获取最后几行作为上下文
        let context = Self::get_last_lines(output, 30);

        // 检测等待模式
        for (pattern, pattern_type) in &self.patterns {
            if pattern.is_match(&context) {
                return InputWaitResult {
                    is_waiting: true,
                    pattern_type: Some(pattern_type.clone()),
                    context,
                };
            }
        }

        // 空闲但没有匹配到特定模式
        InputWaitResult {
            is_waiting: false,
            pattern_type: None,
            context,
        }
    }

    /// 立即检测（不考虑空闲时间）
    pub fn detect_immediate(&self, output: &str) -> InputWaitResult {
        let context = Self::get_last_lines(output, 30);

        for (pattern, pattern_type) in &self.patterns {
            if pattern.is_match(&context) {
                return InputWaitResult {
                    is_waiting: true,
                    pattern_type: Some(pattern_type.clone()),
                    context,
                };
            }
        }

        InputWaitResult {
            is_waiting: false,
            pattern_type: None,
            context,
        }
    }

    /// 清除 session 的状态
    pub fn clear_session(&mut self, session_name: &str) {
        self.last_outputs.remove(session_name);
        self.last_change_times.remove(session_name);
    }

    /// 获取最后 N 行
    fn get_last_lines(text: &str, n: usize) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let start = if lines.len() > n { lines.len() - n } else { 0 };
        lines[start..].join("\n")
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

    #[test]
    fn test_detect_claude_prompt() {
        let detector = InputWaitDetector::new();
        let output = "Some output\n>\n";

        let result = detector.detect_immediate(output);

        assert!(result.is_waiting);
        assert_eq!(result.pattern_type, Some(InputWaitPattern::ClaudePrompt));
    }

    #[test]
    fn test_detect_claude_prompt_unicode() {
        let detector = InputWaitDetector::new();
        // Claude Code 使用 ❯ (U+276F) 作为提示符
        let output = "Some output\n❯ \n";

        let result = detector.detect_immediate(output);

        assert!(result.is_waiting);
        assert_eq!(result.pattern_type, Some(InputWaitPattern::ClaudePrompt));
    }

    #[test]
    fn test_detect_claude_prompt_unicode_end_of_line() {
        let detector = InputWaitDetector::new();
        // 实际 Claude Code 终端输出格式
        let output = "选择一个选项：\n1. 选项一\n2. 选项二\n❯ ";

        let result = detector.detect_immediate(output);

        assert!(result.is_waiting);
        assert_eq!(result.pattern_type, Some(InputWaitPattern::ClaudePrompt));
    }

    #[test]
    fn test_detect_confirmation_yn() {
        let detector = InputWaitDetector::new();
        let output = "Do you want to continue? [Y/n]";

        let result = detector.detect_immediate(output);

        assert!(result.is_waiting);
        assert_eq!(result.pattern_type, Some(InputWaitPattern::Confirmation));
    }

    #[test]
    fn test_detect_confirmation_yN() {
        let detector = InputWaitDetector::new();
        let output = "Are you sure? [y/N]";

        let result = detector.detect_immediate(output);

        assert!(result.is_waiting);
        assert_eq!(result.pattern_type, Some(InputWaitPattern::Confirmation));
    }

    #[test]
    fn test_detect_press_enter() {
        let detector = InputWaitDetector::new();
        let output = "Press Enter to continue...";

        let result = detector.detect_immediate(output);

        assert!(result.is_waiting);
        assert_eq!(result.pattern_type, Some(InputWaitPattern::PressEnter));
    }

    #[test]
    fn test_detect_continue_prompt() {
        let detector = InputWaitDetector::new();
        let output = "Would you like to continue?";

        let result = detector.detect_immediate(output);

        assert!(result.is_waiting);
        assert_eq!(result.pattern_type, Some(InputWaitPattern::Continue));
    }

    #[test]
    fn test_detect_permission_request() {
        let detector = InputWaitDetector::new();
        let output = "Do you want to allow this action?";

        let result = detector.detect_immediate(output);

        assert!(result.is_waiting);
        assert_eq!(result.pattern_type, Some(InputWaitPattern::PermissionRequest));
    }

    #[test]
    fn test_detect_colon_prompt() {
        let detector = InputWaitDetector::new();
        let output = "Enter your name:";

        let result = detector.detect_immediate(output);

        assert!(result.is_waiting);
        assert_eq!(result.pattern_type, Some(InputWaitPattern::ColonPrompt));
    }

    #[test]
    fn test_no_wait_pattern() {
        let detector = InputWaitDetector::new();
        let output = "Processing files...\nDone!";

        let result = detector.detect_immediate(output);

        assert!(!result.is_waiting);
        assert_eq!(result.pattern_type, None);
    }

    #[test]
    fn test_idle_detection() {
        let mut detector = InputWaitDetector::with_idle_threshold(Duration::from_millis(100));
        let session = "test-session";
        let output = ">\n";

        // 首次检测，不应该是等待状态（还没空闲）
        let result1 = detector.detect(session, output);
        assert!(!result1.is_waiting);

        // 等待超过阈值
        std::thread::sleep(Duration::from_millis(150));

        // 再次检测，应该是等待状态
        let result2 = detector.detect(session, output);
        assert!(result2.is_waiting);
    }

    #[test]
    fn test_output_change_resets_idle() {
        let mut detector = InputWaitDetector::with_idle_threshold(Duration::from_millis(100));
        let session = "test-session";

        // 首次检测
        detector.detect(session, "output 1");

        // 等待超过阈值
        std::thread::sleep(Duration::from_millis(150));

        // 输出变化，应该重置空闲计时
        let result = detector.detect(session, "output 2\n>");
        assert!(!result.is_waiting);
    }

    #[test]
    fn test_get_last_lines() {
        let text = "line1\nline2\nline3\nline4\nline5\nline6";
        let last_3 = InputWaitDetector::get_last_lines(text, 3);
        assert_eq!(last_3, "line4\nline5\nline6");
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
}
