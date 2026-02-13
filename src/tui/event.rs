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
    if app.search_mode {
        handle_search_key(app, key);
        return;
    }
    match app.view {
        crate::tui::View::Dashboard => handle_dashboard_key(app, key),
        crate::tui::View::Logs => handle_logs_key(app, key),
    }
}

fn handle_search_key(app: &mut crate::tui::App, key: KeyEvent) {
    match (key.code, key.modifiers) {
        // 退出搜索
        (KeyCode::Esc, _) => app.exit_search_mode(),

        // 确认搜索
        (KeyCode::Enter, _) => app.confirm_search(),

        // 光标移动
        (KeyCode::Left, _) => app.search_input.move_left(),
        (KeyCode::Right, _) => app.search_input.move_right(),
        (KeyCode::Home, _) => app.search_input.move_home(),
        (KeyCode::End, _) => app.search_input.move_end(),
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => app.search_input.move_home(),
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => app.search_input.move_end(),

        // 文本编辑
        (KeyCode::Backspace, _) => app.search_input.backspace(),
        (KeyCode::Delete, _) => app.search_input.delete(),
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => app.search_input.clear(),
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => app.search_input.delete_word(),

        // 导航匹配项（用上下方向键 + Ctrl）
        (KeyCode::Down, KeyModifiers::CONTROL) => app.next_agent(),
        (KeyCode::Up, KeyModifiers::CONTROL) => app.prev_agent(),

        // 字符输入（包括 j/k）
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            app.search_input.insert(c);
        }
        _ => {}
    }
}

fn handle_dashboard_key(app: &mut crate::tui::App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('j') | KeyCode::Down => app.next_agent(),
        KeyCode::Char('k') | KeyCode::Up => app.prev_agent(),
        KeyCode::Char('l') => app.toggle_view(),
        KeyCode::Char('/') => app.enter_search_mode(),
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
        KeyCode::Char('j') | KeyCode::Down => app.logs_state.scroll_down(),
        KeyCode::Char('k') | KeyCode::Up => app.logs_state.scroll_up(),
        KeyCode::Char('G') => app.logs_state.scroll_to_bottom(),
        KeyCode::Char('f') => app.logs_state.toggle_filter(),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.quit(),
        _ => {}
    }
}
