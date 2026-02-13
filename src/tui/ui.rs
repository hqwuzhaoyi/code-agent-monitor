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

    // 预先计算过滤后的 agents（避免重复计算）
    let filtered = app.filtered_agents();
    let filtered_count = filtered.len();
    let filter_text = app.filter_input.text();
    let is_filtering = !filter_text.is_empty();

    // 垂直分割: 状态栏 | 主区域 | 通知 | 底部栏
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // 状态栏
            Constraint::Min(10),    // 主区域
            Constraint::Length(5),  // 通知
            Constraint::Length(1),  // 底部栏（过滤输入或快捷键）
        ])
        .split(area);

    // 状态栏
    let elapsed = app.last_refresh.elapsed().as_secs();
    let refresh_text = if elapsed < 60 {
        format!("{}s", elapsed)
    } else {
        format!("{}m", elapsed / 60)
    };
    let status = if is_filtering {
        format!(
            " CAM TUI │ Showing {} of {} │ ↻ {} ago",
            filtered_count,
            app.agents.len(),
            refresh_text
        )
    } else {
        format!(
            " CAM TUI │ Agents: {} │ ↻ {} ago",
            app.agents.len(),
            refresh_text
        )
    };
    // 过滤模式时边框变色（类似 lazygit）
    let status_style = if app.filter_mode {
        Style::default().bg(Color::Cyan).fg(Color::Black)
    } else {
        Style::default().bg(Color::Blue).fg(Color::White)
    };
    let status_bar = Paragraph::new(status).style(status_style);
    frame.render_widget(status_bar, vertical[0]);

    // 主区域: 左右分割
    let main_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),  // Agent 列表
            Constraint::Percentage(70),  // 终端预览
        ])
        .split(vertical[1]);

    // Agent 列表（使用预先计算的 filtered）
    render_agent_list_with_filtered(app, frame, main_area[0], &filtered);

    // 终端预览
    render_terminal_preview(app, frame, main_area[1]);

    // 通知区域
    render_notifications(app, frame, vertical[2]);

    // 底部栏：过滤模式显示输入框，否则显示快捷键
    if app.filter_mode {
        let (before, after) = app.filter_input.split_at_cursor();
        let filter_bar = Paragraph::new(format!(" Filter: {}│{} ", before, after))
            .style(Style::default().bg(Color::Yellow).fg(Color::Black));
        frame.render_widget(filter_bar, vertical[3]);
    } else if is_filtering {
        let filter_bar = Paragraph::new(format!(" Filter: {} │ [Esc] clear │ [/] edit ", filter_text))
            .style(Style::default().bg(Color::DarkGray).fg(Color::Cyan));
        frame.render_widget(filter_bar, vertical[3]);
    } else {
        let help = " [j/k] 移动  [Enter] tmux  [/] filter  [l] logs  [q] quit ";
        let help_bar = Paragraph::new(help).style(Style::default().bg(Color::DarkGray));
        frame.render_widget(help_bar, vertical[3]);
    }
}

/// 渲染 Agent 列表（使用预先过滤的结果）
fn render_agent_list_with_filtered(app: &App, frame: &mut Frame, area: Rect, filtered: &[&crate::tui::AgentItem]) {
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let icon = agent.state.icon();
            let selected = if i == app.selected_index { "→ " } else { "  " };
            let duration = chrono::Local::now()
                .signed_duration_since(agent.started_at)
                .num_minutes();
            let text = format!(
                "{}{} {}\n   {} | {}\n   [{:?}] {}m",
                selected, icon, agent.id, agent.agent_type, agent.project,
                agent.state, duration
            );
            ListItem::new(text)
        })
        .collect();

    let filter_text = app.filter_input.text();
    let title = if filter_text.is_empty() {
        " Agents ".to_string()
    } else {
        format!(" Agents ({}) ", filtered.len())
    };

    // 过滤模式时边框变色
    let border_style = if app.filter_mode {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let list = List::new(items)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border_style));
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
fn render_logs(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // 状态栏
            Constraint::Min(5),     // 日志内容
            Constraint::Length(1),  // 快捷键
        ])
        .split(area);

    // 状态栏
    let status = format!(
        " CAM Logs │ Filter: {:?} │ Lines: {}",
        app.logs_state.filter,
        app.logs_state.lines.len()
    );
    let status_bar = Paragraph::new(status)
        .style(Style::default().bg(Color::Magenta).fg(Color::White));
    frame.render_widget(status_bar, vertical[0]);

    // 日志内容
    let filtered = app.logs_state.filtered_lines();
    let items: Vec<ListItem> = filtered
        .iter()
        .skip(app.logs_state.scroll_offset)
        .take(vertical[1].height as usize)
        .map(|line| {
            let style = if line.contains("ERROR") || line.contains("❌") {
                Style::default().fg(Color::Red)
            } else if line.contains("WARN") || line.contains("⚠") {
                Style::default().fg(Color::Yellow)
            } else if line.contains("INFO") || line.contains("✅") {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            ListItem::new(*line).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(list, vertical[1]);

    // 快捷键
    let help = " [j/k] 滚动  [f] 过滤级别  [G] 跳到最新  [Esc] 返回  [q] 退出 ";
    let help_bar = Paragraph::new(help).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(help_bar, vertical[2]);
}
