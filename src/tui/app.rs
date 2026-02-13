//! TUI 应用状态和主循环

use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use chrono::{DateTime, Local};

use crate::tui::state::{AgentItem, AgentState, NotificationItem, View};
use crate::tui::terminal_stream::TerminalStream;
use crate::{AgentManager, AgentStatus, TmuxManager};

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
            terminal_stream: TerminalStream::new(),
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

    /// 刷新 agent 列表
    pub fn refresh_agents(&mut self) -> AppResult<()> {
        let agent_manager = AgentManager::new();

        let mut items = Vec::new();

        // 从 AgentManager 获取已注册的 agents
        if let Ok(agents) = agent_manager.list_agents() {
            for agent in agents {
                // 根据 AgentRecord 的 status 字段确定状态
                let state = match agent.status {
                    AgentStatus::Running => AgentState::Running,
                    AgentStatus::Waiting => AgentState::Waiting,
                    AgentStatus::Stopped => AgentState::Idle,
                };

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

use crate::tui::{render, poll_event, handle_key, TuiEvent};
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
