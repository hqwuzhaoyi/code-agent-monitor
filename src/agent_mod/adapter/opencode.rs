// src/agent_mod/adapter/opencode.rs
//! OpenCode 适配器
//!
//! OpenCode 是一个支持完整 Plugin 事件系统的 AI 编码工具。
//! 支持的事件：session.created, session.idle, session.error, permission.asked, permission.replied

use super::*;
use crate::agent::AgentType;
use std::path::PathBuf;

pub struct OpenCodeAdapter;

impl AgentAdapter for OpenCodeAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::OpenCode
    }

    fn get_command(&self) -> &str {
        "opencode"
    }

    fn get_resume_command(&self, session_id: &str) -> String {
        if !session_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            panic!("Invalid session_id format: only alphanumeric, hyphen, and underscore allowed");
        }
        format!("opencode --session {}", session_id)
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        // OpenCode Plugin 系统完整，可以纯 Hook
        DetectionStrategy::HookOnly
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: true,
            hook_events: vec![
                "session.created".into(),
                "session.idle".into(),
                "session.error".into(),
                "permission.asked".into(),
                "permission.replied".into(),
                "tool.execute.before".into(),
                "tool.execute.after".into(),
            ],
            mcp_support: true,
            json_output: false,
        }
    }

    fn paths(&self) -> AgentPaths {
        let home = dirs::home_dir().unwrap_or_else(|| {
            tracing::warn!("Could not determine home directory, using current directory");
            PathBuf::from(".")
        });
        AgentPaths {
            config: Some(home.join(".config/opencode/opencode.json")),
            sessions: Some(home.join(".config/opencode/sessions")),
            logs: None,
        }
    }

    fn is_installed(&self) -> bool {
        which::which("opencode").is_ok()
    }

    fn parse_hook_event(&self, payload: &str) -> Option<HookEvent> {
        let value: serde_json::Value = serde_json::from_str(payload).ok()?;
        let event_type = value.get("type")?.as_str()?;
        let cwd = value
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        match event_type {
            "session.created" => Some(HookEvent::SessionStart {
                session_id: value
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                cwd,
            }),
            "session.idle" => Some(HookEvent::WaitingForInput {
                context: "idle".into(),
                is_decision: false,
                cwd,
            }),
            "session.error" => Some(HookEvent::Error {
                message: value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error")
                    .to_string(),
                cwd,
            }),
            "permission.asked" => Some(HookEvent::PermissionRequest {
                tool: value
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                action: value
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                cwd,
            }),
            "permission.replied" => Some(HookEvent::PermissionReplied {
                tool: value
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                approved: value
                    .get("approved")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            }),
            "tool.execute.before" => Some(HookEvent::Custom {
                event_type: "tool.execute.before".into(),
                payload: value.clone(),
            }),
            "tool.execute.after" => {
                let tool = value
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let success = value
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let duration_ms = value
                    .get("duration_ms")
                    .and_then(|v| v.as_u64());
                Some(HookEvent::ToolExecuted {
                    tool,
                    success,
                    duration_ms,
                })
            }
            _ => Some(HookEvent::Custom {
                event_type: event_type.to_string(),
                payload: value.clone(),
            }),
        }
    }

    fn detect_ready(&self, terminal_output: &str) -> bool {
        terminal_output.contains("opencode") || terminal_output.contains("Ready")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type() {
        let adapter = OpenCodeAdapter;
        assert_eq!(adapter.agent_type(), AgentType::OpenCode);
    }

    #[test]
    fn test_get_command() {
        let adapter = OpenCodeAdapter;
        assert_eq!(adapter.get_command(), "opencode");
    }

    #[test]
    fn test_get_resume_command() {
        let adapter = OpenCodeAdapter;
        assert_eq!(
            adapter.get_resume_command("abc123"),
            "opencode --session abc123"
        );
    }

    #[test]
    fn test_detection_strategy() {
        let adapter = OpenCodeAdapter;
        assert_eq!(adapter.detection_strategy(), DetectionStrategy::HookOnly);
    }

    #[test]
    fn test_capabilities() {
        let adapter = OpenCodeAdapter;
        let caps = adapter.capabilities();
        assert!(caps.native_hooks);
        assert!(caps.mcp_support);
        assert!(!caps.json_output);
        assert!(caps.hook_events.contains(&"session.created".to_string()));
        assert!(caps.hook_events.contains(&"session.idle".to_string()));
        assert!(caps.hook_events.contains(&"session.error".to_string()));
        assert!(caps.hook_events.contains(&"permission.asked".to_string()));
        assert!(caps.hook_events.contains(&"permission.replied".to_string()));
    }

    #[test]
    fn test_paths() {
        let adapter = OpenCodeAdapter;
        let paths = adapter.paths();
        assert!(paths.config.is_some());
        assert!(paths.sessions.is_some());
        assert!(paths.logs.is_none());

        let config_path = paths.config.unwrap();
        assert!(config_path.to_string_lossy().contains("opencode"));
    }

    #[test]
    fn test_parse_session_created() {
        let adapter = OpenCodeAdapter;
        let payload = r#"{"type":"session.created","session_id":"sess-123","cwd":"/tmp/project"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::SessionStart { session_id, cwd } => {
                assert_eq!(session_id, "sess-123");
                assert_eq!(cwd, "/tmp/project");
            }
            _ => panic!("Expected SessionStart"),
        }
    }

    #[test]
    fn test_parse_session_idle() {
        let adapter = OpenCodeAdapter;
        let payload = r#"{"type":"session.idle","cwd":"/tmp/project"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::WaitingForInput {
                context,
                is_decision,
                cwd,
            } => {
                assert_eq!(context, "idle");
                assert!(!is_decision);
                assert_eq!(cwd, "/tmp/project");
            }
            _ => panic!("Expected WaitingForInput"),
        }
    }

    #[test]
    fn test_parse_session_error() {
        let adapter = OpenCodeAdapter;
        let payload = r#"{"type":"session.error","message":"Connection failed","cwd":"/tmp"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::Error { message, cwd } => {
                assert_eq!(message, "Connection failed");
                assert_eq!(cwd, "/tmp");
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_parse_permission_asked() {
        let adapter = OpenCodeAdapter;
        let payload =
            r#"{"type":"permission.asked","tool":"Bash","action":"rm -rf /tmp/test","cwd":"/tmp"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::PermissionRequest { tool, action, cwd } => {
                assert_eq!(tool, "Bash");
                assert_eq!(action, "rm -rf /tmp/test");
                assert_eq!(cwd, "/tmp");
            }
            _ => panic!("Expected PermissionRequest"),
        }
    }

    #[test]
    fn test_parse_permission_replied() {
        let adapter = OpenCodeAdapter;
        let payload = r#"{"type":"permission.replied","tool":"Bash","approved":true}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::PermissionReplied { tool, approved } => {
                assert_eq!(tool, "Bash");
                assert!(approved);
            }
            _ => panic!("Expected PermissionReplied"),
        }
    }

    #[test]
    fn test_parse_permission_replied_denied() {
        let adapter = OpenCodeAdapter;
        let payload = r#"{"type":"permission.replied","tool":"Write","approved":false}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::PermissionReplied { tool, approved } => {
                assert_eq!(tool, "Write");
                assert!(!approved);
            }
            _ => panic!("Expected PermissionReplied"),
        }
    }

    #[test]
    fn test_parse_tool_execute_after() {
        let adapter = OpenCodeAdapter;
        let payload =
            r#"{"type":"tool.execute.after","tool":"Read","success":true,"duration_ms":150}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::ToolExecuted {
                tool,
                success,
                duration_ms,
            } => {
                assert_eq!(tool, "Read");
                assert!(success);
                assert_eq!(duration_ms, Some(150));
            }
            _ => panic!("Expected ToolExecuted"),
        }
    }

    #[test]
    fn test_parse_custom_event() {
        let adapter = OpenCodeAdapter;
        let payload = r#"{"type":"custom.event","data":"test"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::Custom {
                event_type,
                payload,
            } => {
                assert_eq!(event_type, "custom.event");
                assert_eq!(payload.get("data").unwrap().as_str().unwrap(), "test");
            }
            _ => panic!("Expected Custom"),
        }
    }

    #[test]
    fn test_parse_invalid_json() {
        let adapter = OpenCodeAdapter;
        let payload = "not valid json";
        assert!(adapter.parse_hook_event(payload).is_none());
    }

    #[test]
    fn test_parse_missing_type() {
        let adapter = OpenCodeAdapter;
        let payload = r#"{"session_id":"abc","cwd":"/tmp"}"#;
        assert!(adapter.parse_hook_event(payload).is_none());
    }

    #[test]
    fn test_detect_ready() {
        let adapter = OpenCodeAdapter;
        assert!(adapter.detect_ready("opencode v1.0.0"));
        assert!(adapter.detect_ready("Ready for input"));
        assert!(!adapter.detect_ready("Loading..."));
        assert!(!adapter.detect_ready(""));
    }

    #[test]
    fn test_get_resume_command_with_hyphen_underscore() {
        let adapter = OpenCodeAdapter;
        assert_eq!(
            adapter.get_resume_command("session-123_abc"),
            "opencode --session session-123_abc"
        );
    }

    #[test]
    #[should_panic(expected = "Invalid session_id format")]
    fn test_get_resume_command_rejects_shell_injection() {
        let adapter = OpenCodeAdapter;
        adapter.get_resume_command("abc; rm -rf /");
    }

    #[test]
    #[should_panic(expected = "Invalid session_id format")]
    fn test_get_resume_command_rejects_spaces() {
        let adapter = OpenCodeAdapter;
        adapter.get_resume_command("abc def");
    }
}
