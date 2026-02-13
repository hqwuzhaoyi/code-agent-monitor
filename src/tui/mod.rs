//! TUI 仪表盘模块

mod app;
mod state;

pub use app::{App, AppResult, Tui, init_terminal, restore_terminal};
pub use state::{AgentItem, AgentState, NotificationItem, View};
