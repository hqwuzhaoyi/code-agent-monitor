// src/agent_mod/adapter/claude.rs
//! Claude Code 适配器

use super::*;
use crate::agent::AgentType;
use regex::Regex;
use std::path::PathBuf;
use std::sync::LazyLock;

/// 静态编译的提示符正则表达式，避免每次调用都编译
static PROMPT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^[❯>]\s*$").expect("Invalid prompt regex"));

pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Claude
    }

    fn get_command(&self) -> &str {
        "claude"
    }

    fn get_resume_command(&self, session_id: &str) -> String {
        if !session_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            panic!("Invalid session_id format: only alphanumeric, hyphen, and underscore allowed");
        }
        format!("claude --resume {}", session_id)
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        DetectionStrategy::HookOnly
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: true,
            hook_events: vec![
                "session_start".into(),
                "stop".into(),
                "notification".into(),
                "PreToolUse".into(),
                "PostToolUse".into(),
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
            config: Some(home.join(".claude/settings.json")),
            sessions: Some(home.join(".claude/projects")),
            logs: None,
        }
    }

    fn is_installed(&self) -> bool {
        which::which("claude").is_ok()
    }

    fn parse_hook_event(&self, payload: &str) -> Option<HookEvent> {
        let value: serde_json::Value = serde_json::from_str(payload).ok()?;
        let event_type = value.get("event")?.as_str()?;
        let cwd = value
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let session_id = value
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        match event_type {
            "session_start" => Some(HookEvent::SessionStart {
                session_id: session_id.unwrap_or_default(),
                cwd,
            }),
            "stop" => Some(HookEvent::SessionEnd { session_id, cwd }),
            "notification" => {
                let notification_type = value
                    .get("notification_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match notification_type {
                    "idle_prompt" => Some(HookEvent::WaitingForInput {
                        context: "idle".into(),
                        is_decision: false,
                        cwd,
                    }),
                    _ => None,
                }
            }
            "PreToolUse" => {
                let tool = value
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                Some(HookEvent::PermissionRequest {
                    tool,
                    action: "execute".into(),
                    cwd,
                })
            }
            _ => None,
        }
    }

    fn detect_ready(&self, terminal_output: &str) -> bool {
        // Claude Code 就绪状态检测：
        // 1. 提示符 ❯ 或 > 在行首
        // 2. Welcome 消息
        // 3. Claude Code 标识
        PROMPT_RE.is_match(terminal_output)
            || terminal_output.contains("Welcome to")
            || terminal_output.contains("Claude Code")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_command() {
        let adapter = ClaudeAdapter;
        assert_eq!(adapter.get_command(), "claude");
    }

    #[test]
    fn test_get_resume_command() {
        let adapter = ClaudeAdapter;
        assert_eq!(
            adapter.get_resume_command("abc123"),
            "claude --resume abc123"
        );
    }

    #[test]
    fn test_detection_strategy() {
        let adapter = ClaudeAdapter;
        assert_eq!(adapter.detection_strategy(), DetectionStrategy::HookOnly);
    }

    #[test]
    fn test_capabilities() {
        let adapter = ClaudeAdapter;
        let caps = adapter.capabilities();
        assert!(caps.native_hooks);
        assert!(caps.mcp_support);
        assert!(!caps.json_output);
        assert!(caps.hook_events.contains(&"session_start".to_string()));
        assert!(caps.hook_events.contains(&"PreToolUse".to_string()));
    }

    #[test]
    fn test_paths() {
        let adapter = ClaudeAdapter;
        let paths = adapter.paths();
        assert!(paths.config.is_some());
        assert!(paths.sessions.is_some());
        assert!(paths.logs.is_none());

        let config = paths.config.unwrap();
        assert!(config.to_string_lossy().contains(".claude/settings.json"));
    }

    #[test]
    fn test_detect_ready_with_prompt() {
        let adapter = ClaudeAdapter;
        assert!(adapter.detect_ready("Some output\n❯ "));
        assert!(adapter.detect_ready("Output\n❯\n"));
        assert!(adapter.detect_ready("Line\n> "));
    }

    #[test]
    fn test_detect_ready_with_welcome() {
        let adapter = ClaudeAdapter;
        assert!(adapter.detect_ready("Welcome to Claude Code\n❯"));
        assert!(adapter.detect_ready("Welcome to the session"));
    }

    #[test]
    fn test_detect_ready_negative() {
        let adapter = ClaudeAdapter;
        assert!(!adapter.detect_ready("Loading..."));
        assert!(!adapter.detect_ready("Processing request"));
        assert!(!adapter.detect_ready(""));
    }

    #[test]
    fn test_parse_session_start() {
        let adapter = ClaudeAdapter;
        let payload = r#"{"event":"session_start","session_id":"abc","cwd":"/tmp"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::SessionStart { session_id, cwd } => {
                assert_eq!(session_id, "abc");
                assert_eq!(cwd, "/tmp");
            }
            _ => panic!("Expected SessionStart"),
        }
    }

    #[test]
    fn test_parse_stop() {
        let adapter = ClaudeAdapter;
        let payload = r#"{"event":"stop","session_id":"xyz","cwd":"/home/user"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::SessionEnd { session_id, cwd } => {
                assert_eq!(session_id, Some("xyz".to_string()));
                assert_eq!(cwd, "/home/user");
            }
            _ => panic!("Expected SessionEnd"),
        }
    }

    #[test]
    fn test_parse_notification_idle() {
        let adapter = ClaudeAdapter;
        let payload =
            r#"{"event":"notification","notification_type":"idle_prompt","cwd":"/project"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::WaitingForInput {
                context,
                is_decision,
                cwd,
            } => {
                assert_eq!(context, "idle");
                assert!(!is_decision);
                assert_eq!(cwd, "/project");
            }
            _ => panic!("Expected WaitingForInput"),
        }
    }

    #[test]
    fn test_parse_pre_tool_use() {
        let adapter = ClaudeAdapter;
        let payload = r#"{"event":"PreToolUse","tool_name":"Bash","cwd":"/workspace"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::PermissionRequest { tool, action, cwd } => {
                assert_eq!(tool, "Bash");
                assert_eq!(action, "execute");
                assert_eq!(cwd, "/workspace");
            }
            _ => panic!("Expected PermissionRequest"),
        }
    }

    #[test]
    fn test_parse_invalid_json() {
        let adapter = ClaudeAdapter;
        assert!(adapter.parse_hook_event("not json").is_none());
        assert!(adapter.parse_hook_event("{}").is_none());
        assert!(adapter.parse_hook_event(r#"{"event":"unknown"}"#).is_none());
    }

    #[test]
    fn test_get_resume_command_with_hyphen_underscore() {
        let adapter = ClaudeAdapter;
        assert_eq!(
            adapter.get_resume_command("session-123_abc"),
            "claude --resume session-123_abc"
        );
    }

    #[test]
    #[should_panic(expected = "Invalid session_id format")]
    fn test_get_resume_command_rejects_shell_injection() {
        let adapter = ClaudeAdapter;
        adapter.get_resume_command("abc; rm -rf /");
    }

    #[test]
    #[should_panic(expected = "Invalid session_id format")]
    fn test_get_resume_command_rejects_spaces() {
        let adapter = ClaudeAdapter;
        adapter.get_resume_command("abc def");
    }
}
