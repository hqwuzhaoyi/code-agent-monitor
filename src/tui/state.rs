//! TUI 状态数据结构

use chrono::{DateTime, Local};

/// Agent 状态
#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    Running,
    Waiting,
    Idle,
    Error,
}

impl AgentState {
    /// 返回动画图标，tick 除以 5 来降低动画速度
    pub fn icon(&self, tick: usize) -> &'static str {
        let frame = tick / 5; // 每5帧切换一次动画
        match self {
            AgentState::Running => {
                const FRAMES: &[&str] = &["◐", "◓", "◑", "◒"];
                FRAMES[frame % FRAMES.len()]
            }
            AgentState::Waiting => {
                const FRAMES: &[&str] = &["◉", "◎"];
                FRAMES[frame % FRAMES.len()]
            }
            AgentState::Idle => {
                const FRAMES: &[&str] = &["○", "◌"];
                FRAMES[frame % FRAMES.len()]
            }
            AgentState::Error => {
                const FRAMES: &[&str] = &["✗", "⚠"];
                FRAMES[frame % FRAMES.len()]
            }
        }
    }
}

/// Agent 信息（TUI 显示用）
#[derive(Debug, Clone)]
pub struct AgentItem {
    pub id: String,
    pub agent_type: String,
    pub project: String,
    pub state: AgentState,
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
