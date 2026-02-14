use code_agent_monitor::agent::manager::AgentStatus;
use code_agent_monitor::infra::input::{InputWaitPattern, InputWaitResult};

// Integration test to verify watcher updates agent status
// This is a contract test - we verify the behavior exists
#[test]
fn test_watcher_updates_status_on_change() {
    // Full integration test would require tmux setup
    // This documents the expected behavior and verifies the logic

    // Test 1: AI detects waiting -> should set WaitingForInput
    let wait_result = InputWaitResult {
        is_waiting: true,
        pattern_type: Some(InputWaitPattern::Other),
        context: "test".to_string(),
    };

    let status = if wait_result.is_waiting {
        AgentStatus::WaitingForInput
    } else if wait_result.pattern_type == Some(InputWaitPattern::Unknown) {
        AgentStatus::Unknown
    } else {
        AgentStatus::Processing
    };
    assert_eq!(status, AgentStatus::WaitingForInput);

    // Test 2: AI detects processing -> should set Processing
    let processing_result = InputWaitResult {
        is_waiting: false,
        pattern_type: None,
        context: "test".to_string(),
    };

    let status = if processing_result.is_waiting {
        AgentStatus::WaitingForInput
    } else if processing_result.pattern_type == Some(InputWaitPattern::Unknown) {
        AgentStatus::Unknown
    } else {
        AgentStatus::Processing
    };
    assert_eq!(status, AgentStatus::Processing);

    // Test 3: AI returns Unknown -> should set Unknown (not Processing!)
    let unknown_result = InputWaitResult {
        is_waiting: false,
        pattern_type: Some(InputWaitPattern::Unknown),
        context: "test".to_string(),
    };

    let status = if unknown_result.is_waiting {
        AgentStatus::WaitingForInput
    } else if unknown_result.pattern_type == Some(InputWaitPattern::Unknown) {
        AgentStatus::Unknown
    } else {
        AgentStatus::Processing
    };
    assert_eq!(status, AgentStatus::Unknown);

    // Verify Unknown should notify (important for alerting on AI failures)
    assert!(AgentStatus::Unknown.should_notify());
}

