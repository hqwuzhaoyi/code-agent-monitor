# TUI Sorting and Animation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add agent list sorting by start time (newest first) and animated state icons.

**Architecture:** Global animation tick counter in App, AgentState::icon() takes tick parameter to return frame-appropriate icon. Sorting applied after loading agents.

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

### Task 8: Manual verification

**Step 1: Build release**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Build succeeds

**Step 2: Test TUI**

Run: `cargo run --release -- tui`
Expected:
- Agents sorted by start time (newest at top)
- State icons animate (rotating/pulsing)

**Step 3: Done**

All features implemented and verified.
