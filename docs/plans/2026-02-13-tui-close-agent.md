# TUI Close Agent Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add x/d key binding to close selected agent in TUI Dashboard

**Architecture:** Add `close_selected_agent()` method to App, handle x/d key in main loop (similar to Enter key pattern), update help bar

**Tech Stack:** Rust, ratatui, crossterm

---

### Task 1: Add close_selected_agent method to App

**Files:**
- Modify: `src/tui/app.rs:49-232` (App impl block)

**Step 1: Write the failing test**

Add to `src/tui/tests.rs`:

```rust
#[test]
fn test_close_selected_agent_returns_id() {
    let mut app = App::new();
    app.agents = vec![
        AgentItem {
            id: "cam-test-close".to_string(),
            agent_type: "claude".to_string(),
            project: "test".to_string(),
            state: AgentState::Running,
            started_at: chrono::Local::now(),
            tmux_session: Some("cam-test-close".to_string()),
        },
    ];

    // close_selected_agent should return the agent ID
    let result = app.close_selected_agent();
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some("cam-test-close".to_string()));
}

#[test]
fn test_close_selected_agent_empty_list() {
    let mut app = App::new();
    let result = app.close_selected_agent();
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_close_selected_agent --no-run 2>&1 | head -20`
Expected: Compilation error "no method named `close_selected_agent`"

**Step 3: Write minimal implementation**

Add to `src/tui/app.rs` after `attach_selected_tmux` method (around line 231):

```rust
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
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_close_selected_agent -v`
Expected: PASS (note: actual agent won't be stopped since no real tmux session)

**Step 5: Commit**

```bash
git add src/tui/app.rs src/tui/tests.rs
git commit -m "feat(tui): add close_selected_agent method"
```

---

### Task 2: Handle x/d key in main loop

**Files:**
- Modify: `src/tui/app.rs:260-323` (run function)

**Step 1: Add x/d key handling in run function**

In `src/tui/app.rs`, inside the `run` function, after the Enter key handling block (around line 299), add:

```rust
                    // 检查是否是 x 或 d 键（关闭 agent）
                    if key.code == crossterm::event::KeyCode::Char('x')
                        || key.code == crossterm::event::KeyCode::Char('d')
                    {
                        if !app.filter_mode && app.view == View::Dashboard {
                            let _ = app.close_selected_agent();
                            last_full_refresh = std::time::Instant::now();
                            continue;
                        }
                    }
```

**Step 2: Run cargo check**

Run: `cargo check`
Expected: No errors

**Step 3: Manual test**

Run: `cargo run --release -- tui`
- Press x or d on a selected agent
- Agent should be closed and removed from list

**Step 4: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): handle x/d key to close agent in main loop"
```

---

### Task 3: Update help bar

**Files:**
- Modify: `src/tui/ui.rs:82`

**Step 1: Update help text**

In `src/tui/ui.rs`, change line 82 from:

```rust
        let help = " [j/k] 移动  [Enter] tmux  [/] filter  [l] logs  [q] quit ";
```

to:

```rust
        let help = " [j/k] 移动  [Enter] tmux  [x] close  [/] filter  [l] logs  [q] quit ";
```

**Step 2: Run cargo check**

Run: `cargo check`
Expected: No errors

**Step 3: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat(tui): add close shortcut to help bar"
```

---

### Task 4: Final verification

**Step 1: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 2: Build release**

Run: `cargo build --release`
Expected: Build succeeds

**Step 3: Manual E2E test**

1. Start a mock agent: `cam start /tmp --agent-type mock`
2. Open TUI: `cargo run --release -- tui`
3. Verify agent appears in list
4. Press x or d
5. Verify agent is removed from list
6. Verify help bar shows `[x] close`

**Step 4: Final commit (if any fixes needed)**

```bash
git add -A
git commit -m "fix(tui): address any issues from E2E testing"
```
