use code_agent_monitor::{InputWaitDetector, InputWaitPattern};

#[test]
fn test_detect_chinese_confirmation() {
    let detector = InputWaitDetector::new();
    let output = "是否继续？[是/否]";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Confirmation));
}

#[test]
fn test_detect_chinese_input_prompt() {
    let detector = InputWaitDetector::new();
    let output = "请输入文件名：";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::ColonPrompt));
}

#[test]
fn test_detect_chinese_continue() {
    let detector = InputWaitDetector::new();
    let output = "是否继续执行？";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Continue));
}

#[test]
fn test_detect_chinese_press_enter() {
    let detector = InputWaitDetector::new();
    let output = "按回车继续";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::PressEnter));
}

#[test]
fn test_detect_chinese_permission() {
    let detector = InputWaitDetector::new();
    let output = "是否授权此操作？";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::PermissionRequest));
}

#[test]
fn test_detect_chinese_confirm_question() {
    let detector = InputWaitDetector::new();
    let output = "确认？";

    let result = detector.detect_immediate(output);

    assert!(result.is_waiting);
    assert_eq!(result.pattern_type, Some(InputWaitPattern::Confirmation));
}
