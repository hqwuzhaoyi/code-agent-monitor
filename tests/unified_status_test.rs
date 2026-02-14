use code_agent_monitor::agent::manager::AgentStatus;

#[test]
fn test_unified_status_variants() {
    // Test that only three variants exist
    let processing = AgentStatus::Processing;
    let waiting = AgentStatus::WaitingForInput;
    let unknown = AgentStatus::Unknown;

    assert!(processing.is_processing());
    assert!(!processing.is_waiting());
    assert!(processing.should_notify() == false);

    assert!(waiting.is_waiting());
    assert!(!waiting.is_processing());
    assert!(waiting.should_notify());

    assert!(unknown.should_notify());
    assert_eq!(unknown.icon(), "❓");
}

#[test]
fn test_status_default() {
    let status = AgentStatus::default();
    assert_eq!(status, AgentStatus::Unknown);
}

#[test]
fn test_status_icons() {
    assert_eq!(AgentStatus::Processing.icon(), "🟢");
    assert_eq!(AgentStatus::WaitingForInput.icon(), "🟡");
    assert_eq!(AgentStatus::Unknown.icon(), "❓");
}
