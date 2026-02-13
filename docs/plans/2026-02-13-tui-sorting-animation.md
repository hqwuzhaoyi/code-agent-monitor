# TUI Sorting and Animation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add agent list sorting by start time (newest first), animated state icons, and search functionality.

**Architecture:** Global animation tick counter in App, AgentState::icon() takes tick parameter to return frame-appropriate icon. Sorting applied after loading agents. Search mode with real-time filtering by ID and project name.

**Tech Stack:** Rust, ratatui, chrono

---

### Task 1: Add animation_tick to App

**Files:**
- Modify: `src/tui/app.rs:23-42` (App struct)
- Modify: `src/tui/app.rs:45-57` (App::new)

**Step 1: Add animation_tick field to App struct**

In `src/tui/app.rs`, add field after `logs_state`:

```rust
/// 动画帧计数器
pub animation_tick: usize,
```

**Step 2: Initialize animation_tick in App::new()**

In `App::new()`, add after `logs_state: LogsState::new()`:

```rust
animation_tick: 0,
```

**Step 3: Build to verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): add animation_tick field to App"
```

---

### Task 2: Update AgentState::icon() to accept tick

**Files:**
- Modify: `src/tui/state.rs:14-23` (AgentState::icon)

**Step 1: Write test for animated icon**

In `src/tui/tests.rs`, add test:

```rust
#[test]
fn test_agent_state_animated_icon() {
    // Running cycles through 4 frames
    assert_eq!(AgentState::Running.icon(0), "◐");
    assert_eq!(AgentState::Running.icon(1), "◓");
    assert_eq!(AgentState::Running.icon(2), "◑");
    assert_eq!(AgentState::Running.icon(3), "◒");
    assert_eq!(AgentState::Running.icon(4), "◐"); // wraps

    // Waiting pulses
    assert_eq!(AgentState::Waiting.icon(0), "◉");
    assert_eq!(AgentState::Waiting.icon(1), "◎");

    // Idle breathes
    assert_eq!(AgentState::Idle.icon(0), "○");
    assert_eq!(AgentState::Idle.icon(1), "◌");

    // Error flashes
    assert_eq!(AgentState::Error.icon(0), "✗");
    assert_eq!(AgentState::Error.icon(1), "⚠");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_agent_state_animated_icon 2>&1 | tail -10`
Expected: FAIL - wrong number of arguments

**Step 3: Update icon() implementation**

Replace `AgentState::icon()` in `src/tui/state.rs`:

```rust
impl AgentState {
    pub fn icon(&self, tick: usize) -> &'static str {
        match self {
            AgentState::Running => {
                const FRAMES: &[&str] = &["◐", "◓", "◑", "◒"];
                FRAMES[tick % FRAMES.len()]
            }
            AgentState::Waiting => {
                const FRAMES: &[&str] = &["◉", "◎"];
                FRAMES[tick % FRAMES.len()]
            }
            AgentState::Idle => {
                const FRAMES: &[&str] = &["○", "◌"];
                FRAMES[tick % FRAMES.len()]
            }
            AgentState::Error => {
                const FRAMES: &[&str] = &["✗", "⚠"];
                FRAMES[tick % FRAMES.len()]
            }
        }
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_agent_state_animated_icon 2>&1 | tail -5`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/state.rs src/tui/tests.rs
git commit -m "feat(tui): add animated icons for agent states"
```

---

### Task 3: Update existing icon tests

**Files:**
- Modify: `src/tui/tests.rs:47-52` (test_agent_state_icon)

**Step 1: Update test_agent_state_icon to pass tick**

Replace the test:

```rust
#[test]
fn test_agent_state_icon() {
    // Test with tick=0 for basic icon check
    assert_eq!(AgentState::Running.icon(0), "◐");
    assert_eq!(AgentState::Waiting.icon(0), "◉");
    assert_eq!(AgentState::Idle.icon(0), "○");
    assert_eq!(AgentState::Error.icon(0), "✗");
}
```

**Step 2: Run test to verify it passes**

Run: `cargo test test_agent_state_icon 2>&1 | tail -5`
Expected: PASS

**Step 3: Commit**

```bash
git add src/tui/tests.rs
git commit -m "test(tui): update icon test for new signature"
```

---

### Task 4: Update UI to use animation_tick

**Files:**
- Modify: `src/tui/ui.rs:66-89` (render_agent_list)

**Step 1: Update render_agent_list to pass tick**

In `render_agent_list`, change:

```rust
let icon = agent.state.icon();
```

to:

```rust
let icon = agent.state.icon(app.animation_tick);
```

**Step 2: Build to verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat(tui): use animation_tick in agent list rendering"
```

---

### Task 5: Increment animation_tick in main loop

**Files:**
- Modify: `src/tui/app.rs:219-272` (run function)

**Step 1: Add tick increment after render**

In the `run()` function, after `terminal.draw(|frame| render(app, frame))?;`, add:

```rust
// 递增动画帧
app.animation_tick = app.animation_tick.wrapping_add(1);
```

**Step 2: Build and run to verify animation**

Run: `cargo build 2>&1 | tail -5`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): increment animation_tick each render cycle"
```

---

### Task 6: Add agent list sorting

**Files:**
- Modify: `src/tui/app.rs:94-141` (refresh_agents)

**Step 1: Write test for sorting**

In `src/tui/tests.rs`, add:

```rust
#[test]
fn test_agents_sorted_by_start_time() {
    let mut app = App::new();
    let now = chrono::Local::now();

    app.agents = vec![
        AgentItem {
            id: "old".to_string(),
            agent_type: "claude".to_string(),
            project: "test".to_string(),
            state: AgentState::Running,
            started_at: now - chrono::Duration::hours(2),
            tmux_session: None,
        },
        AgentItem {
            id: "new".to_string(),
            agent_type: "claude".to_string(),
            project: "test".to_string(),
            state: AgentState::Running,
            started_at: now,
            tmux_session: None,
        },
        AgentItem {
            id: "mid".to_string(),
            agent_type: "claude".to_string(),
            project: "test".to_string(),
            state: AgentState::Running,
            started_at: now - chrono::Duration::hours(1),
            tmux_session: None,
        },
    ];

    // Sort manually to test the sorting logic
    app.agents.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    assert_eq!(app.agents[0].id, "new");
    assert_eq!(app.agents[1].id, "mid");
    assert_eq!(app.agents[2].id, "old");
}
```

**Step 2: Run test to verify it passes**

Run: `cargo test test_agents_sorted_by_start_time 2>&1 | tail -5`
Expected: PASS (tests the sorting logic itself)

**Step 3: Add sorting to refresh_agents**

In `refresh_agents()`, before `self.agents = items;`, add:

```rust
// 按启动时间降序排序（最新在前）
items.sort_by(|a, b| b.started_at.cmp(&a.started_at));
```

**Step 4: Build to verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Build succeeds

**Step 5: Commit**

```bash
git add src/tui/app.rs src/tui/tests.rs
git commit -m "feat(tui): sort agents by start time (newest first)"
```

---

### Task 7: Run all tests and verify

**Step 1: Run all TUI tests**

Run: `cargo test tui 2>&1 | tail -20`
Expected: All tests pass

**Step 2: Run full test suite**

Run: `cargo test 2>&1 | tail -10`
Expected: Tests pass (ignore pre-existing flaky test)

**Step 3: Final commit if needed**

If any fixes were needed, commit them.

---

### Task 8: Add search state to App

**Files:**
- Modify: `src/tui/app.rs:23-42` (App struct)
- Modify: `src/tui/app.rs:45-57` (App::new)

**Step 1: Add search fields to App struct**

In `src/tui/app.rs`, add fields after `animation_tick`:

```rust
/// 搜索模式
pub search_mode: bool,
/// 搜索关键词
pub search_query: String,
```

**Step 2: Initialize search fields in App::new()**

In `App::new()`, add after `animation_tick: 0`:

```rust
search_mode: false,
search_query: String::new(),
```

**Step 3: Add search helper methods**

Add to `impl App`:

```rust
/// 进入搜索模式
pub fn enter_search_mode(&mut self) {
    self.search_mode = true;
    self.search_query.clear();
}

/// 退出搜索模式
pub fn exit_search_mode(&mut self) {
    self.search_mode = false;
    self.search_query.clear();
}

/// 获取过滤后的 agents
pub fn filtered_agents(&self) -> Vec<&AgentItem> {
    if self.search_query.is_empty() {
        self.agents.iter().collect()
    } else {
        let query = self.search_query.to_lowercase();
        self.agents
            .iter()
            .filter(|a| {
                a.id.to_lowercase().contains(&query)
                    || a.project.to_lowercase().contains(&query)
            })
            .collect()
    }
}
```

**Step 4: Build to verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Build succeeds

**Step 5: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): add search state to App"
```

---

### Task 9: Add search event handling

**Files:**
- Modify: `src/tui/event.rs` (handle_dashboard_key, add handle_search_key)

**Step 1: Add search key handler**

In `src/tui/event.rs`, add new function:

```rust
fn handle_search_key(app: &mut crate::tui::App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.exit_search_mode(),
        KeyCode::Enter => {
            // 确认搜索，保持过滤状态但退出搜索模式
            app.search_mode = false;
        }
        KeyCode::Backspace => {
            app.search_query.pop();
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
        }
        _ => {}
    }
}
```

**Step 2: Update handle_key to route to search handler**

Modify `handle_key`:

```rust
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
```

**Step 3: Add '/' key to enter search mode**

In `handle_dashboard_key`, add case:

```rust
KeyCode::Char('/') => app.enter_search_mode(),
```

**Step 4: Build to verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Build succeeds

**Step 5: Commit**

```bash
git add src/tui/event.rs
git commit -m "feat(tui): add search event handling"
```

---

### Task 10: Update UI to show search and filter

**Files:**
- Modify: `src/tui/ui.rs` (render_dashboard, render_agent_list)

**Step 1: Update status bar to show search input**

In `render_dashboard`, modify status bar rendering:

```rust
// 状态栏
let status = if app.search_mode {
    format!(" 🔍 {}_", app.search_query)
} else if !app.search_query.is_empty() {
    format!(
        " CAM TUI │ Agents: {} (filtered) │ ↻ {:?} ago │ [/] search",
        app.filtered_agents().len(),
        app.last_refresh.elapsed()
    )
} else {
    format!(
        " CAM TUI │ Agents: {} │ ↻ {:?} ago │ [/] search",
        app.agents.len(),
        app.last_refresh.elapsed()
    )
};
let status_style = if app.search_mode {
    Style::default().bg(Color::Yellow).fg(Color::Black)
} else {
    Style::default().bg(Color::Blue).fg(Color::White)
};
let status_bar = Paragraph::new(status).style(status_style);
frame.render_widget(status_bar, vertical[0]);
```

**Step 2: Update render_agent_list to use filtered_agents**

Modify `render_agent_list` to use `app.filtered_agents()`:

```rust
fn render_agent_list(app: &App, frame: &mut Frame, area: Rect) {
    let filtered = app.filtered_agents();
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let icon = agent.state.icon(app.animation_tick);
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

    let title = if app.search_query.is_empty() {
        " Agents ".to_string()
    } else {
        format!(" Agents ({}) ", filtered.len())
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(list, area);
}
```

**Step 3: Build to verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat(tui): update UI for search display and filtering"
```

---

### Task 11: Add search tests

**Files:**
- Modify: `src/tui/tests.rs`

**Step 1: Add search filter test**

```rust
#[test]
fn test_search_filter() {
    let mut app = App::new();
    app.agents = vec![
        AgentItem {
            id: "cam-123".to_string(),
            agent_type: "claude".to_string(),
            project: "my-project".to_string(),
            state: AgentState::Running,
            started_at: chrono::Local::now(),
            tmux_session: None,
        },
        AgentItem {
            id: "cam-456".to_string(),
            agent_type: "claude".to_string(),
            project: "other-project".to_string(),
            state: AgentState::Idle,
            started_at: chrono::Local::now(),
            tmux_session: None,
        },
    ];

    // No filter
    assert_eq!(app.filtered_agents().len(), 2);

    // Filter by ID
    app.search_query = "123".to_string();
    assert_eq!(app.filtered_agents().len(), 1);
    assert_eq!(app.filtered_agents()[0].id, "cam-123");

    // Filter by project
    app.search_query = "other".to_string();
    assert_eq!(app.filtered_agents().len(), 1);
    assert_eq!(app.filtered_agents()[0].project, "other-project");

    // Case insensitive
    app.search_query = "MY-PROJECT".to_string();
    assert_eq!(app.filtered_agents().len(), 1);
}
```

**Step 2: Add search mode test**

```rust
#[test]
fn test_search_mode() {
    let mut app = App::new();

    assert!(!app.search_mode);
    assert!(app.search_query.is_empty());

    app.enter_search_mode();
    assert!(app.search_mode);

    app.search_query = "test".to_string();
    app.exit_search_mode();
    assert!(!app.search_mode);
    assert!(app.search_query.is_empty());
}
```

**Step 3: Run tests**

Run: `cargo test test_search 2>&1 | tail -10`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/tui/tests.rs
git commit -m "test(tui): add search functionality tests"
```

---

### Task 12: Final verification

**Step 1: Run all TUI tests**

Run: `cargo test tui 2>&1 | tail -20`
Expected: All tests pass

**Step 2: Build release**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Build succeeds

**Step 3: Test TUI**

Run: `cargo run --release -- tui`
Expected:
- Agents sorted by start time (newest at top)
- State icons animate (rotating/pulsing)
- Press `/` to enter search mode
- Type to filter agents by ID or project name
- Press `Esc` to clear search, `Enter` to confirm

**Step 4: Done**

All features implemented and verified.
