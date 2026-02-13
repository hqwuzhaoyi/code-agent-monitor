//! TUI 渲染模块

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use crate::tui::{App, View};

/// 渲染主界面
pub fn render(app: &App, frame: &mut Frame) {
    match app.view {
        View::Dashboard => render_dashboard(app, frame),
        View::Logs => render_logs(app, frame),
    }
}

/// 渲染仪表盘视图
fn render_dashboard(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // 垂直分割: 状态栏 | 主区域 | 通知 | 快捷键
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // 状态栏
            Constraint::Min(10),    // 主区域
            Constraint::Length(5),  // 通知
            Constraint::Length(1),  // 快捷键
        ])
        .split(area);

    // 状态栏
    let status = format!(
        " CAM TUI │ Agents: {} │ ↻ {:?} ago",
        app.agents.len(),
        app.last_refresh.elapsed()
    );
    let status_bar = Paragraph::new(status).style(Style::default().bg(Color::Blue).fg(Color::White));
    frame.render_widget(status_bar, vertical[0]);

    // 主区域: 左右分割
    let main_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),  // Agent 列表
            Constraint::Percentage(70),  // 终端预览
        ])
        .split(vertical[1]);

    // Agent 列表
    render_agent_list(app, frame, main_area[0]);

    // 终端预览
    render_terminal_preview(app, frame, main_area[1]);

    // 通知区域
    render_notifications(app, frame, vertical[2]);

    // 快捷键栏
    let help = " [j/k] 移动  [Enter] 跳转 tmux  [l] 日志  [q] 退出 ";
    let help_bar = Paragraph::new(help).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(help_bar, vertical[3]);
}

/// 渲染 Agent 列表
fn render_agent_list(app: &App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app
        .agents
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let icon = agent.state.icon();
            let selected = if i == app.selected_index { "→ " } else { "  " };
            let duration = chrono::Local::now()
                .signed_duration_since(agent.started_at)
                .num_minutes();
            let text = format!(
                "{}{} {} \n   {} \n   [{:?}] {}m",
                selected, icon, agent.agent_type, agent.project,
                agent.state, duration
            );
            ListItem::new(text)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Agents "));
    frame.render_widget(list, area);
}

/// 渲染终端预览
fn render_terminal_preview(app: &App, frame: &mut Frame, area: Rect) {
    let preview = Paragraph::new(app.terminal_preview.as_str())
        .block(Block::default().borders(Borders::ALL).title(" Terminal Preview "));
    frame.render_widget(preview, area);
}

/// 渲染通知区域
fn render_notifications(app: &App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app
        .notifications
        .iter()
        .rev()
        .take(3)
        .map(|n| {
            let text = format!(
                "[{}] {}: {}",
                n.timestamp.format("%H:%M"),
                n.agent_id,
                n.message
            );
            ListItem::new(text)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Notifications "));
    frame.render_widget(list, area);
}

/// 渲染日志视图
fn render_logs(_app: &App, frame: &mut Frame) {
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" CAM Logs (Press Esc to return) ");
    let paragraph = Paragraph::new("日志视图待实现...")
        .block(block);
    frame.render_widget(paragraph, area);
}
