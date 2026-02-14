//! TUI 状态数据结构

use chrono::{DateTime, Local};
use crate::AgentStatus;

/// Agent 信息（TUI 显示用）
#[derive(Debug, Clone)]
pub struct AgentItem {
    pub id: String,
    pub agent_type: String,
    pub project: String,
    pub state: AgentStatus,
    pub started_at: DateTime<Local>,
    pub tmux_session: Option<String>,
}

/// 通知条目
#[derive(Debug, Clone)]
pub struct NotificationItem {
    pub timestamp: DateTime<Local>,
    pub agent_id: String,
    pub message: String,
}

/// 当前视图
#[derive(Debug, Clone, PartialEq, Default)]
pub enum View {
    #[default]
    Dashboard,
    Logs,
}
