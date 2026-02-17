//! System Event 单元测试

#[cfg(test)]
mod tests {
    use crate::notification::system_event::*;
    use crate::notification::event::{NotificationEvent, NotificationEventType};
    use crate::notification::urgency::Urgency;
    use serde_json::json;

    #[test]
    fn test_payload_from_permission_request() {
        let event = NotificationEvent::permission_request(
            "cam-test",
            "Bash",
            json!({"command": "npm install"}),
        ).with_project_path("/workspace/myapp");

        let payload = SystemEventPayload::from_event(&event, Urgency::High);

        assert_eq!(payload.source, "cam");
        assert_eq!(payload.version, "1.0");
        assert_eq!(payload.agent_id, "cam-test");
        assert_eq!(payload.event_type, "permission_request");
        assert_eq!(payload.urgency, "HIGH");
        assert_eq!(payload.project_path, Some("/workspace/myapp".to_string()));

        // 验证 event_data
        let json = payload.to_json();
        assert_eq!(json["event_data"]["tool_name"], "Bash");
    }

    #[test]
    fn test_payload_from_waiting_for_input() {
        let event = NotificationEvent::waiting_for_input("cam-test", "ClaudePrompt");

        let payload = SystemEventPayload::from_event(&event, Urgency::High);

        assert_eq!(payload.event_type, "waiting_for_input");
        let json = payload.to_json();
        assert_eq!(json["event_data"]["pattern_type"], "ClaudePrompt");
    }

    #[test]
    fn test_payload_from_error() {
        let event = NotificationEvent::error("cam-test", "Connection failed");

        let payload = SystemEventPayload::from_event(&event, Urgency::High);

        assert_eq!(payload.event_type, "error");
        let json = payload.to_json();
        assert_eq!(json["event_data"]["message"], "Connection failed");
    }

    #[test]
    fn test_payload_serialization() {
        let event = NotificationEvent::agent_exited("cam-test");
        let payload = SystemEventPayload::from_event(&event, Urgency::Medium);

        let json_str = serde_json::to_string(&payload).unwrap();
        assert!(json_str.contains("\"source\":\"cam\""));
        assert!(json_str.contains("\"event_type\":\"agent_exited\""));
    }

    #[test]
    fn test_risk_level_assessment() {
        // 高风险命令
        let event = NotificationEvent::permission_request(
            "cam-test",
            "Bash",
            json!({"command": "rm -rf /"}),
        );
        let payload = SystemEventPayload::from_event(&event, Urgency::High);
        assert_eq!(payload.context.risk_level, "HIGH");

        // 低风险命令
        let event = NotificationEvent::permission_request(
            "cam-test",
            "Bash",
            json!({"command": "ls -la"}),
        );
        let payload = SystemEventPayload::from_event(&event, Urgency::High);
        assert_eq!(payload.context.risk_level, "LOW");
    }
}
