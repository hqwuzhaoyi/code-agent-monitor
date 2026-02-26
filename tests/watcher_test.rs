//! Tests for the watcher module

use code_agent_monitor::agent::{AgentMonitor, StabilityDetector};

#[test]
fn test_stability_detector_is_stable_same_content() {
    // Given: a stability detector with threshold 3
    let mut detector = StabilityDetector::new(3);
    let agent_id = "test-agent";
    let content = "Hello, world!";

    // When: checking stability with same content multiple times
    assert!(!detector.is_stable(agent_id, content)); // count = 1
    assert!(!detector.is_stable(agent_id, content)); // count = 2
    assert!(detector.is_stable(agent_id, content)); // count = 3, stable!
    assert!(detector.is_stable(agent_id, content)); // count = 4, still stable
}

#[test]
fn test_stability_detector_is_stable_different_content() {
    // Given: a stability detector with threshold 3
    let mut detector = StabilityDetector::new(3);
    let agent_id = "test-agent";

    // When: checking stability with different content
    assert!(!detector.is_stable(agent_id, "content 1")); // count = 1
    assert!(!detector.is_stable(agent_id, "content 2")); // count = 1 (reset)
    assert!(!detector.is_stable(agent_id, "content 3")); // count = 1 (reset)

    // Then: never becomes stable because content keeps changing
}

#[test]
fn test_stability_detector_clear() {
    // Given: a stability detector with some state
    let mut detector = StabilityDetector::new(2);
    let agent_id = "test-agent";
    let content = "Hello";

    detector.is_stable(agent_id, content); // count = 1

    // When: clearing the state
    detector.clear(agent_id);

    // Then: count resets
    assert!(!detector.is_stable(agent_id, content)); // count = 1 again
    assert!(detector.is_stable(agent_id, content)); // count = 2, stable
}

#[test]
fn test_stability_detector_multiple_agents() {
    // Given: a stability detector tracking multiple agents
    let mut detector = StabilityDetector::new(2);

    // When: checking stability for different agents
    assert!(!detector.is_stable("agent-1", "content A")); // agent-1: count = 1
    assert!(!detector.is_stable("agent-2", "content B")); // agent-2: count = 1
    assert!(detector.is_stable("agent-1", "content A")); // agent-1: count = 2, stable
    assert!(detector.is_stable("agent-2", "content B")); // agent-2: count = 2, stable

    // Then: each agent has independent state
}

#[test]
fn test_agent_monitor_creation() {
    // Given/When: creating an AgentMonitor
    let monitor = AgentMonitor::new();
    let default_monitor = AgentMonitor::default();

    // Then: both should be created successfully (no panic)
    // We can't easily test is_alive without a real tmux session,
    // but we can verify the struct is created
    drop(monitor);
    drop(default_monitor);
}
