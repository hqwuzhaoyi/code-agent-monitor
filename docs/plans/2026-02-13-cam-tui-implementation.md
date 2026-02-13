# CAM TUI 仪表盘实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 CAM 添加 TUI 仪表盘，提供统一的 agent 监控界面

**Architecture:** 基于 Ratatui + Crossterm 构建异步 TUI，复用现有 infra/ 和 notification/ 模块，通过 Tokio select! 合并键盘、agent 状态、通知三个事件源

**Tech Stack:** Rust, Ratatui 0.28, Crossterm 0.28, Tokio

---

## Task 1: 添加依赖

**Files:**
- Modify: `Cargo.toml`

**Step 1: 添加 ratatui 和 crossterm 依赖**

```toml
# 在 [dependencies] 末尾添加
ratatui = "0.28"
crossterm = "0.28"
```

**Step 2: 验证依赖可以解析**

Run: `cargo check`
Expected: 编译通过，无错误

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add ratatui and crossterm for TUI"
```

---

## Task 2: 创建 TUI 模块骨架

**Files:**
- Create: `src/tui/mod.rs`
- Create: `src/tui/app.rs`
- Modify: `src/lib.rs`

**Step 1: 创建 tui/mod.rs**

```rust
//! TUI 仪表盘模块

mod app;

pub use app::{App, AppResult};
```

**Step 2: 创建 tui/app.rs 骨架**

```rust
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
```

**Step 3: 在 lib.rs 中注册模块**

在 `src/lib.rs` 的模块声明区域添加：

```rust
pub mod tui;
```

**Step 4: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 5: Commit**

```bash
git add src/tui/ src/lib.rs
git commit -m "feat(tui): add module skeleton"
```

---

## Task 3: 实现基础终端初始化和清理

**Files:**
- Modify: `src/tui/app.rs`

**Step 1: 添加终端初始化函数**

```rust
use std::io::{self, Stdout};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

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
```

**Step 2: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 3: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): add terminal init and restore"
```

---

## Task 4: 实现 Agent 数据结构

**Files:**
- Create: `src/tui/state.rs`
- Modify: `src/tui/mod.rs`

**Step 1: 创建 state.rs**

```rust
//! TUI 状态数据结构

use chrono::{DateTime, Local};

/// Agent 状态
#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    Running,
    Waiting,
    Idle,
    Error,
}

impl AgentState {
    pub fn icon(&self) -> &'static str {
        match self {
            AgentState::Running => "●",
            AgentState::Waiting => "◉",
            AgentState::Idle => "○",
            AgentState::Error => "✗",
        }
    }
}

/// Agent 信息（TUI 显示用）
#[derive(Debug, Clone)]
pub struct AgentItem {
    pub id: String,
    pub agent_type: String,
    pub project: String,
    pub state: AgentState,
    pub started_at: DateTime<Local>,
    pub tmux_session: Option<String>,
}

/// 通知条目
#[derive(Debug, Clone)]
pub struct NotificationItem {
    pub timestamp: DateTime<Local>,
    pub agent_id: String,
    pub message: String,
}

/// 当前视图
#[derive(Debug, Clone, PartialEq, Default)]
pub enum View {
    #[default]
    Dashboard,
    Logs,
}
```

**Step 2: 更新 mod.rs**

```rust
//! TUI 仪表盘模块

mod app;
mod state;

pub use app::{App, AppResult, Tui, init_terminal, restore_terminal};
pub use state::{AgentState, AgentItem, NotificationItem, View};
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 4: Commit**

```bash
git add src/tui/
git commit -m "feat(tui): add state data structures"
```

---

## Task 5: 扩展 App 状态

**Files:**
- Modify: `src/tui/app.rs`

**Step 1: 扩展 App 结构体**

```rust
use crate::tui::state::{AgentItem, NotificationItem, View};

/// TUI 应用状态
pub struct App {
    pub should_quit: bool,
    pub view: View,
    pub agents: Vec<AgentItem>,
    pub selected_index: usize,
    pub notifications: Vec<NotificationItem>,
    pub terminal_preview: String,
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

    pub fn next_agent(&mut self) {
        if !self.agents.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.agents.len();
        }
    }

    pub fn prev_agent(&mut self) {
        if !self.agents.is_empty() {
            self.selected_index = self.selected_index.checked_sub(1).unwrap_or(self.agents.len() - 1);
        }
    }

    pub fn selected_agent(&self) -> Option<&AgentItem> {
        self.agents.get(self.selected_index)
    }

    pub fn toggle_view(&mut self) {
        self.view = match self.view {
            View::Dashboard => View::Logs,
            View::Logs => View::Dashboard,
        };
    }
}
```

**Step 2: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 3: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): extend App state with agents and notifications"
```

---

## Task 6: 实现基础 UI 渲染

**Files:**
- Create: `src/tui/ui.rs`
- Modify: `src/tui/mod.rs`

**Step 1: 创建 ui.rs**

```rust
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
                "{}{} {} \n   {} \n   [{}] {}m",
                selected, icon, agent.agent_type, agent.project,
                format!("{:?}", agent.state).to_uppercase(), duration
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
fn render_logs(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" CAM Logs (Press Esc to return) ");
    let paragraph = Paragraph::new("日志视图待实现...")
        .block(block);
    frame.render_widget(paragraph, area);
}
```

**Step 2: 更新 mod.rs**

```rust
//! TUI 仪表盘模块

mod app;
mod state;
mod ui;

pub use app::{App, AppResult, Tui, init_terminal, restore_terminal};
pub use state::{AgentState, AgentItem, NotificationItem, View};
pub use ui::render;
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 4: Commit**

```bash
git add src/tui/
git commit -m "feat(tui): add basic UI rendering"
```

---

## Task 7: 实现事件处理

**Files:**
- Create: `src/tui/event.rs`
- Modify: `src/tui/mod.rs`

**Step 1: 创建 event.rs**

```rust
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
```

**Step 2: 更新 mod.rs**

```rust
//! TUI 仪表盘模块

mod app;
mod state;
mod ui;
mod event;

pub use app::{App, AppResult, Tui, init_terminal, restore_terminal};
pub use state::{AgentState, AgentItem, NotificationItem, View};
pub use ui::render;
pub use event::{TuiEvent, poll_event, handle_key};
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 4: Commit**

```bash
git add src/tui/
git commit -m "feat(tui): add event handling"
```

---

## Task 8: 实现主循环

**Files:**
- Modify: `src/tui/app.rs`

**Step 1: 添加 run 函数**

在 `app.rs` 末尾添加：

```rust
use crate::tui::{render, poll_event, handle_key, TuiEvent};
use std::time::Duration;

/// 运行 TUI 主循环
pub fn run(terminal: &mut Tui, app: &mut App) -> AppResult<()> {
    while !app.should_quit {
        // 渲染
        terminal.draw(|frame| render(app, frame))?;

        // 处理事件（100ms 超时）
        if let Some(event) = poll_event(Duration::from_millis(100))? {
            match event {
                TuiEvent::Key(key) => handle_key(app, key),
                TuiEvent::Tick => {}
            }
        }
    }
    Ok(())
}
```

**Step 2: 更新 mod.rs 导出**

```rust
pub use app::{App, AppResult, Tui, init_terminal, restore_terminal, run};
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 4: Commit**

```bash
git add src/tui/
git commit -m "feat(tui): add main loop"
```

---

## Task 9: 添加 CLI 子命令

**Files:**
- Modify: `src/main.rs`

**Step 1: 添加 Tui 子命令定义**

在 `Commands` enum 中添加：

```rust
    /// 启动 TUI 仪表盘
    Tui {
        /// 空闲刷新间隔（毫秒）
        #[arg(long, default_value = "10000")]
        refresh_interval: u64,

        /// 不显示通知流
        #[arg(long)]
        no_notifications: bool,
    },
```

**Step 2: 添加命令处理**

在 `match cli.command` 中添加：

```rust
        Commands::Tui { refresh_interval: _, no_notifications: _ } => {
            use code_agent_monitor::tui::{App, init_terminal, restore_terminal, run};

            let mut terminal = init_terminal()?;
            let mut app = App::new();

            let result = run(&mut terminal, &mut app);

            restore_terminal(&mut terminal)?;

            result?;
        }
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 4: 手动测试**

Run: `cargo run -- tui`
Expected: 显示空的 TUI 界面，按 q 退出

**Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat(tui): add tui subcommand"
```

---

## Task 10: 集成 Agent 数据加载

**Files:**
- Modify: `src/tui/app.rs`

**Step 1: 添加 refresh_agents 方法**

```rust
use crate::{AgentManager, ProcessScanner};
use crate::tui::state::AgentState;
use chrono::{Local, TimeZone};

impl App {
    /// 刷新 agent 列表
    pub fn refresh_agents(&mut self) -> AppResult<()> {
        let agent_manager = AgentManager::new();
        let scanner = ProcessScanner::new();

        let mut items = Vec::new();

        // 从 AgentManager 获取已注册的 agents
        if let Ok(agents) = agent_manager.list_agents() {
            for agent in agents {
                // 检查进程是否还在运行
                let is_running = scanner.get_agent_info(agent.pid.unwrap_or(0) as u32)
                    .ok()
                    .flatten()
                    .is_some();

                let state = if !is_running {
                    AgentState::Idle
                } else {
                    // TODO: 检测是否等待输入
                    AgentState::Running
                };

                items.push(AgentItem {
                    id: agent.agent_id.clone(),
                    agent_type: format!("{:?}", agent.agent_type),
                    project: agent.project_path.split('/').last().unwrap_or(&agent.project_path).to_string(),
                    state,
                    started_at: Local.timestamp_opt(agent.started_at as i64, 0)
                        .single()
                        .unwrap_or_else(Local::now),
                    tmux_session: Some(agent.tmux_session.clone()),
                });
            }
        }

        self.agents = items;
        self.last_refresh = std::time::Instant::now();

        // 更新终端预览
        if let Some(agent) = self.selected_agent() {
            if let Some(ref session) = agent.tmux_session {
                self.refresh_terminal_preview(session)?;
            }
        }

        Ok(())
    }

    /// 刷新终端预览
    pub fn refresh_terminal_preview(&mut self, tmux_session: &str) -> AppResult<()> {
        use crate::TmuxManager;
        let tmux = TmuxManager::new();
        if let Ok(output) = tmux.capture_pane(tmux_session, 30) {
            self.terminal_preview = output;
        }
        Ok(())
    }
}
```

**Step 2: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 3: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): integrate agent data loading"
```

---

## Task 11: 更新主循环加载数据

**Files:**
- Modify: `src/tui/app.rs`

**Step 1: 修改 run 函数添加初始加载和定时刷新**

```rust
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
                    let prev_selected = app.selected_index;
                    handle_key(app, key);
                    // 如果选择变化，刷新终端预览
                    if prev_selected != app.selected_index {
                        if let Some(agent) = app.selected_agent() {
                            if let Some(ref session) = agent.tmux_session {
                                let _ = app.refresh_terminal_preview(session);
                            }
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
```

**Step 2: 更新 main.rs 调用**

```rust
        Commands::Tui { refresh_interval, no_notifications: _ } => {
            use code_agent_monitor::tui::{App, init_terminal, restore_terminal, run};

            let mut terminal = init_terminal()?;
            let mut app = App::new();

            let result = run(&mut terminal, &mut app, refresh_interval);

            restore_terminal(&mut terminal)?;

            result?;
        }
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 4: Commit**

```bash
git add src/tui/ src/main.rs
git commit -m "feat(tui): add periodic refresh"
```

---

## Task 12: 实现 tmux 跳转

**Files:**
- Modify: `src/tui/event.rs`
- Modify: `src/tui/app.rs`

**Step 1: 添加 attach_tmux 方法到 App**

```rust
impl App {
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
```

**Step 2: 修改 run 函数处理 tmux attach**

```rust
/// 运行 TUI 主循环
pub fn run(terminal: &mut Tui, app: &mut App, refresh_interval_ms: u64) -> AppResult<()> {
    // 初始加载
    let _ = app.refresh_agents();

    let refresh_interval = Duration::from_millis(refresh_interval_ms);
    let mut last_full_refresh = std::time::Instant::now();

    while !app.should_quit {
        // 渲染
        terminal.draw(|frame| render(app, frame))?;

        // 处理事件
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

                            if let Err(e) = status {
                                // 可以添加错误提示
                                eprintln!("tmux attach failed: {}", e);
                            }
                            continue;
                        }
                    }

                    let prev_selected = app.selected_index;
                    handle_key(app, key);
                    if prev_selected != app.selected_index {
                        if let Some(agent) = app.selected_agent() {
                            if let Some(ref session) = agent.tmux_session {
                                let _ = app.refresh_terminal_preview(session);
                            }
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
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 4: 手动测试**

Run: `cargo run -- tui`
Expected: 按 Enter 可以跳转到 tmux，Ctrl+b d 返回 TUI

**Step 5: Commit**

```bash
git add src/tui/
git commit -m "feat(tui): implement tmux attach on Enter"
```

---

## Task 13: 实现 pipe-pane 实时流

**Files:**
- Create: `src/tui/terminal_stream.rs`
- Modify: `src/tui/mod.rs`

**Step 1: 创建 terminal_stream.rs**

```rust
//! 终端实时流模块 - 使用 tmux pipe-pane

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

/// 终端流管理器
pub struct TerminalStream {
    current_pane: Option<String>,
    pipe_file: Option<PathBuf>,
}

impl TerminalStream {
    pub fn new() -> Self {
        Self {
            current_pane: None,
            pipe_file: None,
        }
    }

    /// 开始监听指定 tmux session
    pub fn start(&mut self, session: &str) -> Result<PathBuf> {
        // 先停止旧的
        self.stop();

        let pipe_path = PathBuf::from(format!("/tmp/cam-tui-{}.log", session.replace(':', "-")));

        // 清空旧文件
        let _ = std::fs::remove_file(&pipe_path);

        // 启动 pipe-pane
        let status = Command::new("tmux")
            .args([
                "pipe-pane",
                "-t", session,
                &format!("cat >> {}", pipe_path.display()),
            ])
            .status()?;

        if status.success() {
            self.current_pane = Some(session.to_string());
            self.pipe_file = Some(pipe_path.clone());
            Ok(pipe_path)
        } else {
            anyhow::bail!("Failed to start pipe-pane for {}", session)
        }
    }

    /// 停止当前监听
    pub fn stop(&mut self) {
        if let Some(ref session) = self.current_pane {
            // 关闭 pipe-pane
            let _ = Command::new("tmux")
                .args(["pipe-pane", "-t", session])
                .status();
        }

        // 清理文件
        if let Some(ref path) = self.pipe_file {
            let _ = std::fs::remove_file(path);
        }

        self.current_pane = None;
        self.pipe_file = None;
    }

    /// 获取当前 pipe 文件路径
    pub fn pipe_file(&self) -> Option<&PathBuf> {
        self.pipe_file.as_ref()
    }
}

impl Drop for TerminalStream {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Default for TerminalStream {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: 更新 mod.rs**

```rust
//! TUI 仪表盘模块

mod app;
mod state;
mod ui;
mod event;
mod terminal_stream;

pub use app::{App, AppResult, Tui, init_terminal, restore_terminal, run};
pub use state::{AgentState, AgentItem, NotificationItem, View};
pub use ui::render;
pub use event::{TuiEvent, poll_event, handle_key};
pub use terminal_stream::TerminalStream;
```

**Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 4: Commit**

```bash
git add src/tui/
git commit -m "feat(tui): add terminal stream with pipe-pane"
```

---

## Task 14: 集成 pipe-pane 到主循环

**Files:**
- Modify: `src/tui/app.rs`

**Step 1: 在 App 中添加 TerminalStream**

```rust
use crate::tui::terminal_stream::TerminalStream;

pub struct App {
    // ... 现有字段
    pub terminal_stream: TerminalStream,
}

impl App {
    pub fn new() -> Self {
        Self {
            // ... 现有初始化
            terminal_stream: TerminalStream::new(),
        }
    }
}
```

**Step 2: 修改 refresh_terminal_preview 使用 pipe-pane**

```rust
impl App {
    /// 刷新终端预览（优先使用 pipe-pane，降级到 capture-pane）
    pub fn refresh_terminal_preview(&mut self, tmux_session: &str) -> AppResult<()> {
        use crate::TmuxManager;

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
        if let Some(agent) = self.selected_agent() {
            if let Some(ref session) = agent.tmux_session {
                // 尝试启动 pipe-pane，失败则忽略（会降级到轮询）
                let _ = self.terminal_stream.start(session);
            }
        }
    }
}
```

**Step 3: 在主循环中调用**

在 `run` 函数中，当选择变化时调用 `switch_agent_stream`：

```rust
if prev_selected != app.selected_index {
    app.switch_agent_stream();
    if let Some(agent) = app.selected_agent() {
        if let Some(ref session) = agent.tmux_session {
            let _ = app.refresh_terminal_preview(session);
        }
    }
}
```

**Step 4: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 5: Commit**

```bash
git add src/tui/
git commit -m "feat(tui): integrate pipe-pane into main loop"
```

---

## Task 15: 实现日志视图

**Files:**
- Create: `src/tui/logs.rs`
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/mod.rs`

**Step 1: 创建 logs.rs**

```rust
//! 日志视图模块

use std::path::PathBuf;
use std::collections::VecDeque;
use anyhow::Result;

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LogLevel {
    #[default]
    All,
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    pub fn next(&self) -> Self {
        match self {
            LogLevel::All => LogLevel::Error,
            LogLevel::Error => LogLevel::Warn,
            LogLevel::Warn => LogLevel::Info,
            LogLevel::Info => LogLevel::Debug,
            LogLevel::Debug => LogLevel::All,
        }
    }

    pub fn matches(&self, line: &str) -> bool {
        match self {
            LogLevel::All => true,
            LogLevel::Error => line.contains("ERROR"),
            LogLevel::Warn => line.contains("WARN") || line.contains("ERROR"),
            LogLevel::Info => line.contains("INFO") || line.contains("WARN") || line.contains("ERROR"),
            LogLevel::Debug => true,
        }
    }
}

/// 日志状态
pub struct LogsState {
    pub lines: VecDeque<String>,
    pub filter: LogLevel,
    pub scroll_offset: usize,
    pub search_query: String,
}

impl LogsState {
    pub fn new() -> Self {
        Self {
            lines: VecDeque::with_capacity(1000),
            filter: LogLevel::All,
            scroll_offset: 0,
            search_query: String::new(),
        }
    }

    /// 加载日志文件
    pub fn load(&mut self) -> Result<()> {
        let log_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config/code-agent-monitor/hook.log");

        if log_path.exists() {
            let content = std::fs::read_to_string(&log_path)?;
            self.lines.clear();
            for line in content.lines().rev().take(500) {
                self.lines.push_front(line.to_string());
            }
        }
        Ok(())
    }

    /// 获取过滤后的行
    pub fn filtered_lines(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|line| self.filter.matches(line))
            .filter(|line| {
                self.search_query.is_empty() || line.contains(&self.search_query)
            })
            .map(|s| s.as_str())
            .collect()
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_to_bottom(&mut self) {
        let filtered = self.filtered_lines();
        self.scroll_offset = filtered.len().saturating_sub(20);
    }

    pub fn toggle_filter(&mut self) {
        self.filter = self.filter.next();
    }
}

impl Default for LogsState {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: 更新 App 添加 LogsState**

在 `app.rs` 中：

```rust
use crate::tui::logs::LogsState;

pub struct App {
    // ... 现有字段
    pub logs_state: LogsState,
}

impl App {
    pub fn new() -> Self {
        Self {
            // ... 现有初始化
            logs_state: LogsState::new(),
        }
    }
}
```

**Step 3: 更新 ui.rs 的 render_logs 函数**

```rust
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
            let style = if line.contains("ERROR") {
                Style::default().fg(Color::Red)
            } else if line.contains("WARN") {
                Style::default().fg(Color::Yellow)
            } else if line.contains("INFO") {
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
```

**Step 4: 更新 event.rs 处理日志视图按键**

```rust
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
```

**Step 5: 更新 mod.rs**

```rust
mod logs;
pub use logs::{LogsState, LogLevel};
```

**Step 6: 在切换到日志视图时加载日志**

在 `App::toggle_view` 中：

```rust
pub fn toggle_view(&mut self) {
    self.view = match self.view {
        View::Dashboard => {
            let _ = self.logs_state.load();
            View::Logs
        }
        View::Logs => View::Dashboard,
    };
}
```

**Step 7: 验证编译**

Run: `cargo check`
Expected: 编译通过

**Step 8: Commit**

```bash
git add src/tui/
git commit -m "feat(tui): implement logs view"
```

---

## Task 16: 添加测试

**Files:**
- Create: `src/tui/tests.rs`
- Modify: `src/tui/mod.rs`

**Step 1: 创建 tests.rs**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_navigation() {
        let mut app = App::new();
        app.agents = vec![
            AgentItem {
                id: "1".to_string(),
                agent_type: "claude".to_string(),
                project: "test".to_string(),
                state: AgentState::Running,
                started_at: chrono::Local::now(),
                tmux_session: None,
            },
            AgentItem {
                id: "2".to_string(),
                agent_type: "claude".to_string(),
                project: "test2".to_string(),
                state: AgentState::Idle,
                started_at: chrono::Local::now(),
                tmux_session: None,
            },
        ];

        assert_eq!(app.selected_index, 0);
        app.next_agent();
        assert_eq!(app.selected_index, 1);
        app.next_agent();
        assert_eq!(app.selected_index, 0); // wrap around
        app.prev_agent();
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_view_toggle() {
        let mut app = App::new();
        assert_eq!(app.view, View::Dashboard);
        app.toggle_view();
        assert_eq!(app.view, View::Logs);
        app.toggle_view();
        assert_eq!(app.view, View::Dashboard);
    }

    #[test]
    fn test_agent_state_icon() {
        assert_eq!(AgentState::Running.icon(), "●");
        assert_eq!(AgentState::Waiting.icon(), "◉");
        assert_eq!(AgentState::Idle.icon(), "○");
        assert_eq!(AgentState::Error.icon(), "✗");
    }

    #[test]
    fn test_log_level_filter() {
        let level = LogLevel::Error;
        assert!(level.matches("2024-01-01 ERROR something"));
        assert!(!level.matches("2024-01-01 INFO something"));

        let level = LogLevel::All;
        assert!(level.matches("anything"));
    }
}
```

**Step 2: 更新 mod.rs**

```rust
#[cfg(test)]
mod tests;
```

**Step 3: 运行测试**

Run: `cargo test tui`
Expected: 所有测试通过

**Step 4: Commit**

```bash
git add src/tui/
git commit -m "test(tui): add unit tests"
```

---

## 完成检查清单

- [ ] Task 1: 添加依赖
- [ ] Task 2: 创建 TUI 模块骨架
- [ ] Task 3: 实现基础终端初始化和清理
- [ ] Task 4: 实现 Agent 数据结构
- [ ] Task 5: 扩展 App 状态
- [ ] Task 6: 实现基础 UI 渲染
- [ ] Task 7: 实现事件处理
- [ ] Task 8: 实现主循环
- [ ] Task 9: 添加 CLI 子命令
- [ ] Task 10: 集成 Agent 数据加载
- [ ] Task 11: 更新主循环加载数据
- [ ] Task 12: 实现 tmux 跳转
- [ ] Task 13: 实现 pipe-pane 实时流
- [ ] Task 14: 集成 pipe-pane 到主循环
- [ ] Task 15: 实现日志视图
- [ ] Task 16: 添加测试
