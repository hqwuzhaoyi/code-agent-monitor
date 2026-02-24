// src/agent_mod/adapter/codex.rs
//! Codex CLI 适配器
//!
//! TODO: 由 teammate 实现

use super::*;
use crate::agent::AgentType;

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Codex
    }

    fn get_command(&self) -> &str {
        "codex"
    }

    fn get_resume_command(&self, session_id: &str) -> String {
        format!("codex --resume {}", session_id)
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        DetectionStrategy::HookWithPolling
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: true,
            hook_events: vec![],
            mcp_support: true,
            json_output: true,
        }
    }

    fn paths(&self) -> AgentPaths {
        AgentPaths {
            config: None,
            sessions: None,
            logs: None,
        }
    }

    fn is_installed(&self) -> bool {
        false
    }

    fn parse_hook_event(&self, _payload: &str) -> Option<HookEvent> {
        None
    }

    fn detect_ready(&self, _terminal_output: &str) -> bool {
        false
    }
}
