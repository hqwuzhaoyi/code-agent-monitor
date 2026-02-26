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

pub use app::{init_terminal, restore_terminal, run, App, AppResult, Tui};
pub use event::{handle_key, handle_mouse, poll_event, TuiEvent};
pub use logs::{LogLevel, LogsState};
pub use search::SearchInput;
pub use state::{AgentItem, Focus, NotificationItem, View};
pub use terminal_stream::TerminalStream;
pub use ui::render;
