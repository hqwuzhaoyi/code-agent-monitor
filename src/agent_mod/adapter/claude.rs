// src/agent_mod/adapter/claude.rs
//! Claude Code 适配器
//!
//! TODO: 由 teammate 实现

use super::*;
use crate::agent::AgentType;

pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Claude
    }

    fn get_command(&self) -> &str {
        "claude"
    }

    fn get_resume_command(&self, session_id: &str) -> String {
        format!("claude --resume {}", session_id)
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
