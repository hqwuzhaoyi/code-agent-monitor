//! 事件处理模块

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::time::Duration;
use anyhow::Result;

/// TUI 事件
#[derive(Debug)]
pub enum TuiEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Tick,
}

/// 轮询事件
pub fn poll_event(timeout: Duration) -> Result<Option<TuiEvent>> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => return Ok(Some(TuiEvent::Key(key))),
            Event::Mouse(mouse) => return Ok(Some(TuiEvent::Mouse(mouse))),
            _ => {} // 忽略其他事件（如 Resize）
        }
    }
    Ok(None)
}

/// 处理按键事件
pub fn handle_key(app: &mut crate::tui::App, key: KeyEvent) {
    if app.filter_mode {
        handle_filter_key(app, key);
        return;
    }
    match app.view {
        crate::tui::View::Dashboard => handle_dashboard_key(app, key),
        crate::tui::View::Logs => handle_logs_key(app, key),
    }
}

fn handle_filter_key(app: &mut crate::tui::App, key: KeyEvent) {
    match key.code {
        // 退出过滤模式（保留过滤结果）
        KeyCode::Enter => app.exit_filter_mode(),
        // 取消过滤（清除）
        KeyCode::Esc => app.clear_filter(),

        // 光标移动
        KeyCode::Left => app.filter_input.move_left(),
        KeyCode::Right => app.filter_input.move_right(),
        KeyCode::Home => app.filter_input.move_home(),
        KeyCode::End => app.filter_input.move_end(),

        // 文本编辑
        KeyCode::Backspace => {
            app.filter_input.backspace();
            app.on_filter_change();
        }
        KeyCode::Delete => {
            app.filter_input.delete();
            app.on_filter_change();
        }

        // 导航（在过滤模式下也可以用上下键选择）
        KeyCode::Up => app.prev_agent(),
        KeyCode::Down => app.next_agent(),

        // 字符输入
        KeyCode::Char(c) => {
            app.filter_input.insert(c);
            app.on_filter_change();
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
        KeyCode::Char('/') => app.enter_filter_mode(),
        KeyCode::Esc => {
            // Esc 清除过滤
            if !app.filter_input.is_empty() {
                app.clear_filter();
            }
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.quit(),
        KeyCode::Enter => {
            // Enter 跳转 tmux（在 run 函数中处理）
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

/// 处理鼠标事件（带节流）
pub fn handle_mouse(app: &mut crate::tui::App, mouse: MouseEvent) -> bool {
    use std::time::Duration;
    use crate::tui::app::SCROLL_THROTTLE_MS;

    // 节流：忽略过于频繁的滚动事件
    if app.last_scroll_time.elapsed() < Duration::from_millis(SCROLL_THROTTLE_MS) {
        return false;
    }

    // 返回 true 表示选择发生变化，需要刷新终端预览
    let result = match mouse.kind {
        MouseEventKind::ScrollDown => {
            app.last_scroll_time = std::time::Instant::now();
            match app.view {
                crate::tui::View::Dashboard => {
                    app.next_agent();
                    true
                }
                crate::tui::View::Logs => {
                    app.logs_state.scroll_down();
                    false
                }
            }
        }
        MouseEventKind::ScrollUp => {
            app.last_scroll_time = std::time::Instant::now();
            match app.view {
                crate::tui::View::Dashboard => {
                    app.prev_agent();
                    true
                }
                crate::tui::View::Logs => {
                    app.logs_state.scroll_up();
                    false
                }
            }
        }
        _ => false, // 忽略其他鼠标事件（点击、拖拽等）
    };

    result
}
