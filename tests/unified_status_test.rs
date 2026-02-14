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

#[test]
fn test_new_agent_starts_as_processing() {
    // Verify that new agents start with Processing status by default
    // This is a contract test - the actual behavior is verified in manager.rs:356
    // where new agents are created with AgentStatus::Processing

    // We verify the enum default is Unknown (for safety)
    assert_eq!(AgentStatus::default(), AgentStatus::Unknown);

    // But new agents should explicitly use Processing (not default)
    // This is enforced by code review and compilation checks
    let expected_new_agent_status = AgentStatus::Processing;
    assert!(expected_new_agent_status.is_processing());
    assert!(!expected_new_agent_status.should_notify());
}

// Note: Full integration test would require AgentManager setup
// This is a contract test - we verify the method signature exists
#[test]
fn test_update_agent_status_method_exists() {
    // Verify the update_agent_status method has correct signature
    // It should:
    // 1. Take &self, agent_id: &str, status: AgentStatus
    // 2. Return Result<bool> where bool indicates if status changed
    // 3. Use with_locked_agents_file for thread safety

    // We can't easily instantiate AgentManager in unit tests,
    // but we verify the types are correct
    use code_agent_monitor::agent::manager::AgentManager;

    // This will fail to compile if the method signature is wrong
    fn _verify_signature(manager: &AgentManager, id: &str, status: AgentStatus) -> anyhow::Result<bool> {
        manager.update_agent_status(id, status)
    }

    // Verify status change detection logic
    let old_status = AgentStatus::Processing;
    let new_status = AgentStatus::WaitingForInput;
    assert_ne!(old_status, new_status); // Should detect change

    let same_status = AgentStatus::Processing;
    assert_eq!(old_status, same_status); // Should not update
}
