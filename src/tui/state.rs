//! TUI 状态数据结构

use crate::notification::Urgency;
use crate::AgentStatus;
use chrono::{DateTime, Local};

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

/// 当前焦点区域
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Focus {
    #[default]
    AgentList,
    Notifications,
}

/// 通知条目
#[derive(Debug, Clone)]
pub struct NotificationItem {
    pub timestamp: DateTime<Local>,
    pub agent_id: String,
    pub message: String,
    pub urgency: Urgency,
    /// 事件类型
    pub event_type: String,
    /// 项目路径
    pub project: Option<String>,
    /// 事件详情
    pub event_detail: Option<serde_json::Value>,
    /// 终端快照
    pub terminal_snapshot: Option<String>,
    /// 风险等级
    pub risk_level: Option<String>,
}

/// 当前视图
#[derive(Debug, Clone, PartialEq, Default)]
pub enum View {
    #[default]
    Dashboard,
    Logs,
}
