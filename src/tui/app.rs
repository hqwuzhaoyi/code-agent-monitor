//! TUI 应用状态和主循环

use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::{
    execute,
    event::{EnableMouseCapture, DisableMouseCapture},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use chrono::{DateTime, Local};

use crate::tui::logs::LogsState;
use crate::tui::search::SearchInput;
use crate::tui::state::{AgentItem, NotificationItem, View};
use crate::tui::terminal_stream::TerminalStream;
use crate::{AgentManager, TmuxManager};

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
    /// 终端流管理器
    pub terminal_stream: TerminalStream,
    /// 日志状态
    pub logs_state: LogsState,
    /// 过滤模式（类似 lazygit）
    pub filter_mode: bool,
    /// 过滤输入
    pub filter_input: SearchInput,
    /// 上次鼠标滚动时间（用于节流）
    pub last_scroll_time: std::time::Instant,
}

/// 鼠标滚动节流间隔（毫秒）- 限制滚动频率，确保每次滚动只移动一项
pub const SCROLL_THROTTLE_MS: u64 = 300;

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
            terminal_stream: TerminalStream::new(),
            logs_state: LogsState::new(),
            filter_mode: false,
            filter_input: SearchInput::new(),
            last_scroll_time: std::time::Instant::now(),
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
            View::Dashboard => {
                let _ = self.logs_state.load();
                View::Logs
            }
            View::Logs => View::Dashboard,
        };
    }

    /// 进入过滤模式
    pub fn enter_filter_mode(&mut self) {
        self.filter_mode = true;
    }

    /// 退出过滤模式（保留过滤结果）
    pub fn exit_filter_mode(&mut self) {
        self.filter_mode = false;
    }

    /// 清除过滤
    pub fn clear_filter(&mut self) {
        self.filter_input.clear();
        self.filter_mode = false;
        self.selected_index = 0;
    }

    /// 获取过滤后的 agents（实时过滤）
    pub fn filtered_agents(&self) -> Vec<&AgentItem> {
        let query = self.filter_input.text();
        if query.is_empty() {
            self.agents.iter().collect()
        } else {
            let query_lower = query.to_lowercase();
            self.agents
                .iter()
                .filter(|a| {
                    a.id.to_lowercase().contains(&query_lower)
                        || a.project.to_lowercase().contains(&query_lower)
                })
                .collect()
        }
    }

    /// 过滤输入变化时重置选择
    pub fn on_filter_change(&mut self) {
        self.selected_index = 0;
    }

    /// 刷新 agent 列表
    pub fn refresh_agents(&mut self) -> AppResult<()> {
        let agent_manager = AgentManager::new();

        let mut items = Vec::new();

        // 从 AgentManager 获取已注册的 agents
        if let Ok(agents) = agent_manager.list_agents() {
            for agent in agents {
                // 直接使用 AgentStatus
                let state = agent.status.clone();

                // 解析 RFC3339 格式的时间字符串
                let started_at = DateTime::parse_from_rfc3339(&agent.started_at)
                    .map(|dt| dt.with_timezone(&Local))
                    .unwrap_or_else(|_| Local::now());

                items.push(AgentItem {
                    id: agent.agent_id.clone(),
                    agent_type: format!("{:?}", agent.agent_type),
                    project: agent
                        .project_path
                        .split('/')
                        .last()
                        .unwrap_or(&agent.project_path)
                        .to_string(),
                    state,
                    started_at,
                    tmux_session: Some(agent.tmux_session.clone()),
                });
            }
        }

        // 按启动时间降序排序（最新在前）
        items.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        self.agents = items;
        self.last_refresh = std::time::Instant::now();

        // 更新终端预览
        let session_to_refresh = self.selected_agent()
            .and_then(|agent| agent.tmux_session.clone());
        if let Some(session) = session_to_refresh {
            self.refresh_terminal_preview(&session)?;
        }

        Ok(())
    }

    /// 刷新终端预览（优先使用 pipe-pane，降级到 capture-pane）
    pub fn refresh_terminal_preview(&mut self, tmux_session: &str) -> AppResult<()> {
        // 尝试从 pipe 文件读取
        if let Some(pipe_path) = self.terminal_stream.pipe_file() {
            if let Ok(content) = std::fs::read_to_string(pipe_path) {
                // 只保留最后 50 行
                let lines: Vec<&str> = content.lines().collect();
                let start = lines.len().saturating_sub(50);
                self.terminal_preview = lines[start..].join("\n");
                return Ok(());
            }
        }

        // 降级到 capture-pane
        let tmux = TmuxManager::new();
        if let Ok(output) = tmux.capture_pane(tmux_session, 30) {
            self.terminal_preview = output;
        }
        Ok(())
    }

    /// 切换选中 agent 时启动新的 pipe-pane
    pub fn switch_agent_stream(&mut self) {
        let session = self.selected_agent()
            .and_then(|agent| agent.tmux_session.clone());
        if let Some(session) = session {
            // 尝试启动 pipe-pane，失败则忽略（会降级到轮询）
            let _ = self.terminal_stream.start(&session);
        }
    }

    /// 跳转到选中 agent 的 tmux session
    pub fn attach_selected_tmux(&self) -> AppResult<Option<String>> {
        if let Some(agent) = self.selected_agent() {
            if let Some(ref session) = agent.tmux_session {
                return Ok(Some(session.clone()));
            }
        }
        Ok(None)
    }

    /// 关闭选中的 agent
    pub fn close_selected_agent(&mut self) -> AppResult<Option<String>> {
        let agent_id = match self.selected_agent() {
            Some(agent) => agent.id.clone(),
            None => return Ok(None),
        };

        let agent_manager = AgentManager::new();
        // 忽略错误（agent 可能已不存在）
        let _ = agent_manager.stop_agent(&agent_id);

        // 刷新列表
        let _ = self.refresh_agents();

        // 调整选中索引
        if self.selected_index > 0 && self.selected_index >= self.agents.len() {
            self.selected_index = self.agents.len().saturating_sub(1);
        }

        Ok(Some(agent_id))
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
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// 恢复终端
pub fn restore_terminal(terminal: &mut Tui) -> AppResult<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;
    Ok(())
}

use crate::tui::{render, poll_event, handle_key, handle_mouse, TuiEvent};
use std::time::Duration;

/// 运行 TUI 主循环
pub fn run(terminal: &mut Tui, app: &mut App, refresh_interval_ms: u64) -> AppResult<()> {
    // 初始加载
    let _ = app.refresh_agents();

    let refresh_interval = Duration::from_millis(refresh_interval_ms);
    let mut last_full_refresh = std::time::Instant::now();

    while !app.should_quit {
        // 渲染
        terminal.draw(|frame| render(app, frame))?;

        // 处理事件（100ms 超时）
        if let Some(event) = poll_event(Duration::from_millis(100))? {
            match event {
                TuiEvent::Key(key) => {
                    // 检查是否是 Enter 键
                    if key.code == crossterm::event::KeyCode::Enter {
                        if let Ok(Some(session)) = app.attach_selected_tmux() {
                            // 暂时恢复终端
                            restore_terminal(terminal)?;

                            // 执行 tmux attach
                            let status = std::process::Command::new("tmux")
                                .args(["attach-session", "-t", &session])
                                .status();

                            // 重新初始化终端
                            *terminal = init_terminal()?;

                            // 刷新数据
                            let _ = app.refresh_agents();
                            last_full_refresh = std::time::Instant::now();

                            if let Err(e) = status {
                                eprintln!("tmux attach failed: {}", e);
                            }
                            continue;
                        }
                    }
                    // 检查是否是 x 或 d 键（关闭 agent）
                    if key.code == crossterm::event::KeyCode::Char('x')
                        || key.code == crossterm::event::KeyCode::Char('d')
                    {
                        if !app.filter_mode && app.view == View::Dashboard {
                            let _ = app.close_selected_agent();
                            // 清空终端预览，避免显示已关闭 agent 的内容
                            app.terminal_preview.clear();
                            // 切换到新选中的 agent 的流
                            app.switch_agent_stream();
                            // 强制完整重绘
                            terminal.clear()?;
                            last_full_refresh = std::time::Instant::now();
                            continue;
                        }
                    }
                    let prev_selected = app.selected_index;
                    handle_key(app, key);
                    // 如果选择变化，切换 pipe-pane 并刷新终端预览
                    if prev_selected != app.selected_index {
                        app.switch_agent_stream();
                        let session_to_refresh = app.selected_agent()
                            .and_then(|agent| agent.tmux_session.clone());
                        if let Some(session) = session_to_refresh {
                            let _ = app.refresh_terminal_preview(&session);
                        }
                    }
                }
                TuiEvent::Mouse(mouse) => {
                    // 处理鼠标事件
                    let prev_selected = app.selected_index;
                    let selection_changed = handle_mouse(app, mouse);
                    // 如果选择变化，切换 pipe-pane 并刷新终端预览
                    if selection_changed && prev_selected != app.selected_index {
                        app.switch_agent_stream();
                        let session_to_refresh = app.selected_agent()
                            .and_then(|agent| agent.tmux_session.clone());
                        if let Some(session) = session_to_refresh {
                            let _ = app.refresh_terminal_preview(&session);
                        }
                    }
                }
                TuiEvent::Tick => {}
            }
        }

        // 定时全量刷新
        if last_full_refresh.elapsed() >= refresh_interval {
            let _ = app.refresh_agents();
            last_full_refresh = std::time::Instant::now();
        }
    }
    Ok(())
}
