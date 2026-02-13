//! TUI 仪表盘模块

mod app;
mod event;
mod state;
mod ui;

pub use app::{App, AppResult, Tui, init_terminal, restore_terminal, run};
pub use event::{TuiEvent, poll_event, handle_key};
pub use state::{AgentItem, AgentState, NotificationItem, View};
pub use ui::render;
