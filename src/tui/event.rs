//! 事件处理模块

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;
use anyhow::Result;

/// TUI 事件
#[derive(Debug)]
pub enum TuiEvent {
    Key(KeyEvent),
    Tick,
}

/// 轮询事件
pub fn poll_event(timeout: Duration) -> Result<Option<TuiEvent>> {
    if event::poll(timeout)? {
        if let Event::Key(key) = event::read()? {
            return Ok(Some(TuiEvent::Key(key)));
        }
    }
    Ok(None)
}

/// 处理按键事件
pub fn handle_key(app: &mut crate::tui::App, key: KeyEvent) {
    match app.view {
        crate::tui::View::Dashboard => handle_dashboard_key(app, key),
        crate::tui::View::Logs => handle_logs_key(app, key),
    }
}

fn handle_dashboard_key(app: &mut crate::tui::App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('j') | KeyCode::Down => app.next_agent(),
        KeyCode::Char('k') | KeyCode::Up => app.prev_agent(),
        KeyCode::Char('l') => app.toggle_view(),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.quit(),
        KeyCode::Enter => {
            // 跳转到 tmux 将在后续 task 实现
        }
        _ => {}
    }
}

fn handle_logs_key(app: &mut crate::tui::App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.quit(),
        KeyCode::Esc => app.toggle_view(),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.quit(),
        _ => {}
    }
}
