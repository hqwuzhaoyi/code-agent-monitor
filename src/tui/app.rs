//! TUI 应用状态和主循环

use anyhow::Result;

pub type AppResult<T> = Result<T>;

/// TUI 应用状态
pub struct App {
    /// 是否退出
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self { should_quit: false }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
