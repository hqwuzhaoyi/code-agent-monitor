//! 输入等待检测器集成测试
//!
//! 这些测试需要 Anthropic API key，使用 #[ignore] 标记。
//! 运行: cargo test --test input_detector_test -- --ignored

use code_agent_monitor::{InputWaitDetector, InputWaitPattern};

// =========================================================================
// 集成测试 - 需要 API（使用 #[ignore] 标记）
// 运行: cargo test --test input_detector_test -- --ignored
// =========================================================================

#[test]
#[ignore = "requires Anthropic API key"]
fn test_detect_chinese_confirmation() {
    let detector = InputWaitDetector::new();
    let output = "是否继续？[是/否]";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    // AI 判断时统一返回 Other 类型
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Other));
}

#[test]
#[ignore = "requires Anthropic API key"]
fn test_detect_chinese_input_prompt() {
    let detector = InputWaitDetector::new();
    let output = "请输入文件名：";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Other));
}

#[test]
#[ignore = "requires Anthropic API key"]
fn test_detect_chinese_continue() {
    let detector = InputWaitDetector::new();
    let output = "是否继续执行？";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Other));
}

#[test]
#[ignore = "requires Anthropic API key"]
fn test_detect_chinese_press_enter() {
    let detector = InputWaitDetector::new();
    let output = "按回车继续";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Other));
}

#[test]
#[ignore = "requires Anthropic API key"]
fn test_detect_chinese_permission() {
    let detector = InputWaitDetector::new();
    let output = "是否授权此操作？";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Other));
}

#[test]
#[ignore = "requires Anthropic API key"]
fn test_detect_chinese_confirm_question() {
    let detector = InputWaitDetector::new();
    let output = "确认？";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Other));
}

#[test]
#[ignore = "requires Anthropic API key"]
fn test_detect_claude_code_yes_no_format() {
    let detector = InputWaitDetector::new();
    // Claude Code 实际使用的格式
    let output = "Write to /tmp/test.txt? [Y]es / [N]o / [A]lways / [D]on't ask";

    let result = detector.detect_immediate(output);

    assert!(
        result.is_waiting,
        "Should detect Claude Code [Y]es / [N]o format"
    );
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Other));
}

#[test]
#[ignore = "requires Anthropic API key"]
fn test_detect_claude_code_always_format() {
    let detector = InputWaitDetector::new();
    let output = "Run bash command? [Y]es / [N]o / [A]lways";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting, "Should detect [A]lways format");
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Other));
}
