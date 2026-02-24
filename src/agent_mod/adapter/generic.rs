// src/agent_mod/adapter/generic.rs
//! 通用适配器
//!
//! TODO: 由 teammate 实现

use super::*;
use crate::agent::AgentType;

pub struct GenericAdapter {
    agent_type: AgentType,
    command: String,
}

impl GenericAdapter {
    pub fn new(agent_type: AgentType) -> Self {
        let command = "echo".to_string();
        Self { agent_type, command }
    }
}

impl AgentAdapter for GenericAdapter {
    fn agent_type(&self) -> AgentType {
        self.agent_type.clone()
    }

    fn get_command(&self) -> &str {
        &self.command
    }

    fn get_resume_command(&self, _session_id: &str) -> String {
        self.command.clone()
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        DetectionStrategy::PollingOnly
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: false,
            hook_events: vec![],
            mcp_support: false,
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
