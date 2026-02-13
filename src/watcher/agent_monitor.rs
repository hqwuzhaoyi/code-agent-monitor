//! Agent lifecycle monitoring - tmux session health checks

use anyhow::Result;
use crate::agent::AgentRecord;
use crate::infra::tmux::TmuxManager;

/// Monitors agent tmux sessions for health
pub struct AgentMonitor {
    tmux: TmuxManager,
}

impl AgentMonitor {
    pub fn new() -> Self {
        Self { tmux: TmuxManager::new() }
    }

    /// Check if agent's tmux session is still alive
    pub fn is_alive(&self, agent: &AgentRecord) -> bool {
        self.tmux.session_exists(&agent.tmux_session)
    }

    /// Capture current terminal content
    pub fn capture_terminal(&self, agent: &AgentRecord, lines: u32) -> Result<String> {
        self.tmux.capture_pane(&agent.tmux_session, lines)
    }
}

impl Default for AgentMonitor {
    fn default() -> Self {
        Self::new()
    }
}
