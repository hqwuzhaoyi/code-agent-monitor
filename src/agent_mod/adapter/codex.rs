// src/agent_mod/adapter/codex.rs
//! Codex CLI 适配器
//!
//! Codex CLI 使用 `notify` 配置发送 `agent-turn-complete` 事件。
//! 由于只有 turn-complete 事件，需要配合轮询检测其他状态。

use super::*;
use crate::agent::AgentType;
use std::path::PathBuf;

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Codex
    }

    fn get_command(&self) -> &str {
        "codex"
    }

    fn get_resume_command(&self, session_id: &str) -> String {
        if !session_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            panic!("Invalid session_id format: only alphanumeric, hyphen, and underscore allowed");
        }
        format!("codex --resume {}", session_id)
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        // Codex 只有 turn-complete 事件，需要轮询补充其他状态检测
        DetectionStrategy::HookWithPolling
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: true,
            hook_events: vec!["agent-turn-complete".into()],
            mcp_support: true,
            json_output: true,
        }
    }

    fn paths(&self) -> AgentPaths {
        let home = dirs::home_dir().unwrap_or_else(|| {
            tracing::warn!("Could not determine home directory, using current directory");
            PathBuf::from(".")
        });
        AgentPaths {
            config: Some(home.join(".codex/config.toml")),
            sessions: Some(home.join(".codex/sessions")),
            logs: None,
        }
    }

    fn is_installed(&self) -> bool {
        which::which("codex").is_ok()
    }

    fn parse_hook_event(&self, payload: &str) -> Option<HookEvent> {
        // Codex notify payload 作为 JSON 传递
        let value: serde_json::Value = serde_json::from_str(payload).ok()?;
        let event_type = value.get("type")?.as_str()?;

        match event_type {
            "agent-turn-complete" => {
                let thread_id = value
                    .get("thread-id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let turn_id = value
                    .get("turn-id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let cwd = value
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                Some(HookEvent::TurnComplete {
                    thread_id,
                    turn_id,
                    cwd,
                })
            }
            _ => None,
        }
    }

    fn detect_ready(&self, terminal_output: &str) -> bool {
        // 排除信任确认界面
        if terminal_output.contains("Do you trust the contents of this directory?")
            || terminal_output.contains("1. Yes, continue")
        {
            return false;
        }

        // 检测正常就绪状态
        terminal_output.contains(">_ OpenAI Codex")
            || terminal_output.contains("? for shortcuts")
            || terminal_output.contains("context left")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type() {
        let adapter = CodexAdapter;
        assert_eq!(adapter.agent_type(), AgentType::Codex);
    }

    #[test]
    fn test_get_command() {
        let adapter = CodexAdapter;
        assert_eq!(adapter.get_command(), "codex");
    }

    #[test]
    fn test_get_resume_command() {
        let adapter = CodexAdapter;
        assert_eq!(
            adapter.get_resume_command("abc123"),
            "codex --resume abc123"
        );
    }

    #[test]
    fn test_detection_strategy() {
        let adapter = CodexAdapter;
        assert_eq!(
            adapter.detection_strategy(),
            DetectionStrategy::HookWithPolling
        );
    }

    #[test]
    fn test_capabilities() {
        let adapter = CodexAdapter;
        let caps = adapter.capabilities();
        assert!(caps.native_hooks);
        assert!(caps.hook_events.contains(&"agent-turn-complete".to_string()));
        assert!(caps.mcp_support);
        assert!(caps.json_output);
    }

    #[test]
    fn test_paths() {
        let adapter = CodexAdapter;
        let paths = adapter.paths();
        assert!(paths.config.is_some());
        assert!(paths.sessions.is_some());
        let config = paths.config.unwrap();
        assert!(config.to_string_lossy().contains(".codex/config.toml"));
    }

    #[test]
    fn test_parse_turn_complete() {
        let adapter = CodexAdapter;
        let payload = r#"{
            "type": "agent-turn-complete",
            "thread-id": "019c8eda-8d98-7ca3-bdd6-8bdbb1a80f1f",
            "turn-id": "019c8eda-955d-7853-84a0-4ed91b90014d",
            "cwd": "/tmp/project"
        }"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::TurnComplete {
                thread_id,
                turn_id,
                cwd,
            } => {
                assert!(thread_id.starts_with("019c8eda"));
                assert!(turn_id.starts_with("019c8eda"));
                assert_eq!(cwd, "/tmp/project");
            }
            _ => panic!("Expected TurnComplete"),
        }
    }

    #[test]
    fn test_parse_turn_complete_minimal() {
        let adapter = CodexAdapter;
        let payload = r#"{"type": "agent-turn-complete"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::TurnComplete {
                thread_id,
                turn_id,
                cwd,
            } => {
                assert_eq!(thread_id, "");
                assert_eq!(turn_id, "");
                assert_eq!(cwd, "");
            }
            _ => panic!("Expected TurnComplete"),
        }
    }

    #[test]
    fn test_parse_unknown_event() {
        let adapter = CodexAdapter;
        let payload = r#"{"type": "unknown-event"}"#;
        assert!(adapter.parse_hook_event(payload).is_none());
    }

    #[test]
    fn test_parse_invalid_json() {
        let adapter = CodexAdapter;
        assert!(adapter.parse_hook_event("not json").is_none());
        assert!(adapter.parse_hook_event("{}").is_none());
    }

    #[test]
    fn test_detect_ready_normal_state() {
        let adapter = CodexAdapter;
        let normal = r#"╭───────────────────────────────────────────────────╮
│ >_ OpenAI Codex (v0.104.0)                        │
│                                                   │
│ model:     gpt-5.3-codex xhigh   /model to change │
│ directory: /tmp/test                              │
╰───────────────────────────────────────────────────╯

› Find and fix a bug in @filename

  ? for shortcuts                                            100% context left"#;

        assert!(adapter.detect_ready(normal));
    }

    #[test]
    fn test_detect_ready_excludes_trust_dialog() {
        let adapter = CodexAdapter;
        let trust_dialog = r#"> You are in /tmp/test

  Do you trust the contents of this directory? Working with untrusted contents
  comes with higher risk of prompt injection.

› 1. Yes, continue
  2. No, quit

  Press enter to continue"#;

        assert!(!adapter.detect_ready(trust_dialog));
    }

    #[test]
    fn test_detect_ready_partial_matches() {
        let adapter = CodexAdapter;
        // 只有 >_ OpenAI Codex 也应该匹配
        assert!(adapter.detect_ready(">_ OpenAI Codex"));
        // 只有 ? for shortcuts 也应该匹配
        assert!(adapter.detect_ready("? for shortcuts"));
        // 只有 context left 也应该匹配
        assert!(adapter.detect_ready("50% context left"));
        // 无关内容不应该匹配
        assert!(!adapter.detect_ready("Loading..."));
        assert!(!adapter.detect_ready("Some random output"));
    }

    #[test]
    fn test_get_resume_command_with_hyphen_underscore() {
        let adapter = CodexAdapter;
        assert_eq!(
            adapter.get_resume_command("session-123_abc"),
            "codex --resume session-123_abc"
        );
    }

    #[test]
    #[should_panic(expected = "Invalid session_id format")]
    fn test_get_resume_command_rejects_shell_injection() {
        let adapter = CodexAdapter;
        adapter.get_resume_command("abc; rm -rf /");
    }

    #[test]
    #[should_panic(expected = "Invalid session_id format")]
    fn test_get_resume_command_rejects_spaces() {
        let adapter = CodexAdapter;
        adapter.get_resume_command("abc def");
    }
}
