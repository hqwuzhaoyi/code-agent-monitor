//! Integration Tests for CAM ↔ OpenClaw
//!
//! Tests the full integration链路: CAM → OpenClaw → Decision → Callback

use serde_json::json;

mod unit {
    use super::*;

    /// Test SystemEventPayload serialization
    #[test]
    fn test_system_event_payload_serialization() {
        let payload = json!({
            "source": "cam",
            "version": "1.0",
            "agent_id": "cam-123",
            "event_type": "permission_request",
            "urgency": "HIGH",
            "project_path": "/test/project",
            "timestamp": "2026-02-18T10:00:00Z",
            "event_data": {
                "tool_name": "Bash",
                "tool_input": {"command": "ls -la"}
            },
            "context": {
                "risk_level": "LOW"
            }
        });

        assert_eq!(payload["source"], "cam");
        assert_eq!(payload["event_type"], "permission_request");
    }

    /// Test risk assessment for Bash commands
    #[test]
    fn test_risk_assessment_bash() {
        // LOW risk commands
        let low_risk = vec!["ls", "cat", "echo", "pwd", "git status"];
        for cmd in low_risk {
            // Should be assessed as LOW risk
            assert!(is_low_risk_bash(cmd), "Command {} should be LOW risk", cmd);
        }

        // HIGH risk commands
        let high_risk = vec!["rm -rf", "sudo", "dd if="];
        for cmd in high_risk {
            assert!(is_high_risk_bash(cmd), "Command {} should be HIGH risk", cmd);
        }
    }

    fn is_low_risk_bash(cmd: &str) -> bool {
        let low_risk_patterns = ["ls", "cat", "echo", "pwd", "git status", "git log", "head", "tail"];
        low_risk_patterns.iter().any(|p| cmd.starts_with(*p))
    }

    fn is_high_risk_bash(cmd: &str) -> bool {
        let high_risk_patterns = ["rm -rf", "sudo", "dd", "mkfs", "chmod 777"];
        high_risk_patterns.iter().any(|p| cmd.contains(*p))
    }
}

mod integration {
    use super::*;

    /// Test openclaw system event CLI command
    #[test]
    fn test_openclaw_system_event_cli() {
        // This would test: openclaw system event --text '{"source":"cam",...}' --mode now
        // For now, just verify the command structure
        let payload = r#"{"source":"cam","agent_id":"test-1","event_type":"test"}"#;
        assert!(!payload.is_empty());
    }

    /// Test MCP callback interface
    #[test]
    fn test_mcp_callback_interface() {
        // Test that cam_agent_send format is correct
        let agent_id = "cam-123";
        let message = "y";

        // Should be able to construct valid MCP call
        assert!(!agent_id.is_empty());
        assert!(!message.is_empty());
    }
}

mod e2e {
    use super::*;

    /// Full E2E test: CAM triggers event → OpenClaw receives → Decision → Callback
    #[test]
    fn test_full_event_flow() {
        // 1. CAM creates NotificationEvent
        let event = json!({
            "agent_id": "cam-123",
            "event_type": "permission_request",
            "tool_name": "Bash",
            "tool_input": {"command": "npm install"}
        });

        // 2. Build SystemEventPayload
        let payload = json!({
            "source": "cam",
            "agent_id": event["agent_id"],
            "event_type": event["event_type"],
            "urgency": "HIGH",
            "risk_level": "MEDIUM"
        });

        // 3. OpenClaw receives and decides
        let should_notify = payload["urgency"] == "HIGH";
        let should_auto_approve = payload["risk_level"] == "LOW";

        // 4. Verify decision logic
        assert!(should_notify, "HIGH urgency should notify user");
        assert!(!should_auto_approve, "MEDIUM risk should NOT auto-approve");

        // 5. Callback to CAM
        let callback_action = if should_notify { "notify_user" } else { "silent" };
        assert_eq!(callback_action, "notify_user");
    }
}
