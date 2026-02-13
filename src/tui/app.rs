//! TUI 应用状态和主循环

use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use crate::tui::state::{AgentItem, NotificationItem, View};

pub type AppResult<T> = Result<T>;
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// TUI 应用状态
pub struct App {
    /// 是否退出
    pub should_quit: bool,
    /// 当前视图
    pub view: View,
    /// Agent 列表
    pub agents: Vec<AgentItem>,
    /// 当前选中的 agent 索引
    pub selected_index: usize,
    /// 通知列表
    pub notifications: Vec<NotificationItem>,
    /// 终端预览内容
    pub terminal_preview: String,
    /// 上次刷新时间
    pub last_refresh: std::time::Instant,
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            view: View::Dashboard,
            agents: Vec::new(),
            selected_index: 0,
            notifications: Vec::new(),
            terminal_preview: String::new(),
            last_refresh: std::time::Instant::now(),
        }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// 选择下一个 agent
    pub fn next_agent(&mut self) {
        if !self.agents.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.agents.len();
        }
    }

    /// 选择上一个 agent
    pub fn prev_agent(&mut self) {
        if !self.agents.is_empty() {
            self.selected_index = self.selected_index.checked_sub(1).unwrap_or(self.agents.len() - 1);
        }
    }

    /// 获取当前选中的 agent
    pub fn selected_agent(&self) -> Option<&AgentItem> {
        self.agents.get(self.selected_index)
    }

    /// 切换视图
    pub fn toggle_view(&mut self) {
        self.view = match self.view {
            View::Dashboard => View::Logs,
            View::Logs => View::Dashboard,
        };
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// 初始化终端
pub fn init_terminal() -> AppResult<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// 恢复终端
pub fn restore_terminal(terminal: &mut Tui) -> AppResult<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
