// src/agent_mod/adapter/opencode.rs
//! OpenCode 适配器
//!
//! TODO: 由 teammate 实现

use super::*;
use crate::agent::AgentType;

pub struct OpenCodeAdapter;

impl AgentAdapter for OpenCodeAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::OpenCode
    }

    fn get_command(&self) -> &str {
        "opencode"
    }

    fn get_resume_command(&self, session_id: &str) -> String {
        format!("opencode --session {}", session_id)
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        DetectionStrategy::HookOnly
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: true,
            hook_events: vec![],
            mcp_support: true,
            json_output: false,
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
