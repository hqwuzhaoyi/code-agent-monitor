//! TUI 仪表盘模块

mod app;
mod event;
mod logs;
mod search;
mod state;
mod terminal_stream;
mod ui;

#[cfg(test)]
mod tests;

pub use app::{App, AppResult, Tui, init_terminal, restore_terminal, run};
pub use event::{TuiEvent, poll_event, handle_key, handle_mouse};
pub use logs::{LogsState, LogLevel};
pub use search::SearchInput;
pub use state::{AgentItem, NotificationItem, View};
pub use terminal_stream::TerminalStream;
pub use ui::render;
