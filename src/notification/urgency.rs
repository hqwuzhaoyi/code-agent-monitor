//! Urgency classification for notifications
//!
//! This module provides urgency level classification for CAM notifications.
//! The urgency level determines how notifications are routed:
//! - HIGH: Must be forwarded immediately (permission requests, errors)
//! - MEDIUM: User should know (agent exited, idle prompt)
//! - LOW: Optional/silent (session start, tool use)

/// Urgency level for notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Urgency {
    High,
    Medium,
    Low,
}

impl std::fmt::Display for Urgency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Urgency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Urgency::High => "HIGH",
            Urgency::Medium => "MEDIUM",
            Urgency::Low => "LOW",
        }
    }
}

/// Classify urgency based on event type and context
///
/// Priority for 20 parallel AIs:
/// - HIGH: Must respond immediately (permission requests, errors) -> blocks task progress
/// - MEDIUM: Need to know (completed, idle) -> can assign new tasks
/// - LOW: Optional (startup) -> usually no notification needed
pub fn get_urgency(event_type: &str, context: &str) -> Urgency {
    // `cam notify` appends terminal snapshot to JSON context, causing parse failure.
    // Strip snapshot part first to ensure stable urgency classification.
    let raw_context = if let Some(idx) = context.find("\n\n--- 终端快照 ---\n") {
        &context[..idx]
    } else {
        context
    };

    match event_type {
        // Permission request must be forwarded - blocks task progress
        "permission_request" => Urgency::High,
        // notification type needs to check specific type
        "notification" => {
            let json: Option<serde_json::Value> = serde_json::from_str(raw_context).ok();
            let notification_type = json.as_ref()
                .and_then(|j| j.get("notification_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match notification_type {
                "permission_prompt" => Urgency::High,  // Permission confirmation
                "idle_prompt" => Urgency::Medium,      // Idle waiting
                _ => Urgency::Low
            }
        }
        // Error must be forwarded - needs intervention
        "Error" => Urgency::High,
        // Waiting for input must be forwarded
        "WaitingForInput" => Urgency::High,
        // Agent abnormal exit - need to know (might be crash or killed)
        "AgentExited" => Urgency::Medium,
        // stop/session_end - user triggered stop, no notification needed (user already knows)
        "stop" | "session_end" => Urgency::Low,
        // Startup notification - optional
        "session_start" => Urgency::Low,
        // Tool call - too frequent, silent processing
        "ToolUse" => Urgency::Low,
        // Others
        _ => Urgency::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_urgency_high() {
        assert_eq!(get_urgency("permission_request", ""), Urgency::High);
        assert_eq!(get_urgency("Error", ""), Urgency::High);
        assert_eq!(get_urgency("WaitingForInput", ""), Urgency::High);

        // notification with permission_prompt
        let context = r#"{"notification_type": "permission_prompt"}"#;
        assert_eq!(get_urgency("notification", context), Urgency::High);
    }

    #[test]
    fn test_get_urgency_medium() {
        // AgentExited is MEDIUM (might be abnormal exit, user needs to know)
        assert_eq!(get_urgency("AgentExited", ""), Urgency::Medium);

        // notification with idle_prompt
        let context = r#"{"notification_type": "idle_prompt"}"#;
        assert_eq!(get_urgency("notification", context), Urgency::Medium);
    }

    #[test]
    fn test_get_urgency_low() {
        // stop/session_end is LOW (user triggered, no notification needed)
        assert_eq!(get_urgency("stop", ""), Urgency::Low);
        assert_eq!(get_urgency("session_end", ""), Urgency::Low);
        assert_eq!(get_urgency("session_start", ""), Urgency::Low);
        // ToolUse is LOW (too frequent, silent processing)
        assert_eq!(get_urgency("ToolUse", ""), Urgency::Low);
        assert_eq!(get_urgency("unknown_event", ""), Urgency::Low);

        // notification with unknown type
        let context = r#"{"notification_type": "other"}"#;
        assert_eq!(get_urgency("notification", context), Urgency::Low);
    }

    #[test]
    fn test_get_urgency_notification_idle_prompt_with_terminal_snapshot() {
        let context = r#"{"notification_type": "idle_prompt", "message": "waiting"}

--- 终端快照 ---
line 1"#;
        assert_eq!(get_urgency("notification", context), Urgency::Medium);
    }

    #[test]
    fn test_get_urgency_notification_permission_prompt_with_terminal_snapshot() {
        let context = r#"{"notification_type": "permission_prompt", "message": "confirm?"}

--- 终端快照 ---
line 1"#;
        assert_eq!(get_urgency("notification", context), Urgency::High);
    }

    #[test]
    fn test_urgency_display() {
        assert_eq!(format!("{}", Urgency::High), "HIGH");
        assert_eq!(format!("{}", Urgency::Medium), "MEDIUM");
        assert_eq!(format!("{}", Urgency::Low), "LOW");
    }

    #[test]
    fn test_urgency_as_str() {
        assert_eq!(Urgency::High.as_str(), "HIGH");
        assert_eq!(Urgency::Medium.as_str(), "MEDIUM");
        assert_eq!(Urgency::Low.as_str(), "LOW");
    }
}
