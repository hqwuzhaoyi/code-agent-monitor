# Unified Agent Status Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Unify three separate AgentStatus enums into one, and add status synchronization to agents.json.

**Architecture:** Replace `manager::AgentStatus` (Running/Waiting/Stopped) with simplified enum (Processing/WaitingForInput/Unknown), delete `ai::types::AgentStatus` and `tui::AgentState`, update all imports, add `update_agent_status()` method for real-time sync.

**Tech Stack:** Rust, serde, ratatui (TUI)

---

## Task 1: Replace AgentStatus in manager.rs

**Files:**
- Modify: `src/agent_mod/manager.rs:62-68`

**Step 1: Replace the AgentStatus enum**

Replace lines 62-68:

```rust
/// Agent 统一状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// 正在处理中 - agent 正在执行任务
    Processing,
    /// 等待输入 - agent 空闲，等待用户响应
    WaitingForInput,
    /// 未知 - 无法确定状态
    Unknown,
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

impl AgentStatus {
    /// 是否应该发送通知
    pub fn should_notify(&self) -> bool {
        matches!(self, Self::WaitingForInput | Self::Unknown)
    }

    /// 获取 TUI 显示图标
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Processing => "🟢",
            Self::WaitingForInput => "🟡",
            Self::Unknown => "❓",
        }
    }

    /// 是否正在处理
    pub fn is_processing(&self) -> bool {
        matches!(self, Self::Processing)
    }

    /// 是否在等待输入
    pub fn is_waiting(&self) -> bool {
        matches!(self, Self::WaitingForInput)
    }
}
```

**Step 2: Update start_agent() to use Processing**

Find `status: AgentStatus::Running` and replace with `status: AgentStatus::Processing`.

**Step 3: Verify compilation**

Run: `cargo check`
Expected: Errors about `AgentStatus::Running/Waiting/Stopped` not found (expected, will fix in later tasks)

**Step 4: Commit**

```bash
git add src/agent_mod/manager.rs
git commit -m "refactor(agent): replace AgentStatus enum with unified version"
```

---

## Task 2: Add update_agent_status() method

**Files:**
- Modify: `src/agent_mod/manager.rs`

**Step 1: Add the update_agent_status method**

Add after `remove_agent()` method:

```rust
/// 更新 agent 状态
pub fn update_agent_status(&self, agent_id: &str, status: AgentStatus) -> Result<bool> {
    self.with_locked_agents_file(|agents_file| {
        if let Some(agent) = agents_file.agents.iter_mut().find(|a| a.agent_id == agent_id) {
            if agent.status != status {
                debug!(agent_id = %agent_id, old_status = ?agent.status, new_status = ?status, "Updating agent status");
                agent.status = status;
                return true;
            }
        }
        false
    })
}
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Still errors (will fix in next tasks)

**Step 3: Commit**

```bash
git add src/agent_mod/manager.rs
git commit -m "feat(agent): add update_agent_status method for real-time sync"
```

---

## Task 3: Delete AgentStatus from ai/types.rs

**Files:**
- Modify: `src/ai/types.rs:34-61`
- Modify: `src/ai/mod.rs:14`

**Step 1: Delete AgentStatus from types.rs**

Delete lines 30-61 (the entire AgentStatus section including impl blocks).

**Step 2: Update ai/mod.rs exports**

Change line 14 from:
```rust
pub use types::{AgentStatus, QuestionType, NotificationContent};
```
to:
```rust
pub use types::{QuestionType, NotificationContent};
```

**Step 3: Verify compilation**

Run: `cargo check`
Expected: Errors about AgentStatus not found in ai module

**Step 4: Commit**

```bash
git add src/ai/types.rs src/ai/mod.rs
git commit -m "refactor(ai): remove duplicate AgentStatus, use unified version"
```

---

## Task 4: Update ai/extractor.rs imports

**Files:**
- Modify: `src/ai/extractor.rs:10`

**Step 1: Update import**

Change line 10 from:
```rust
use crate::ai::types::{AgentStatus, NotificationContent, QuestionType};
```
to:
```rust
use crate::ai::types::{NotificationContent, QuestionType};
use crate::agent_mod::manager::AgentStatus;
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Fewer errors

**Step 3: Commit**

```bash
git add src/ai/extractor.rs
git commit -m "refactor(ai): update AgentStatus import in extractor"
```

---

## Task 5: Update ai/quality.rs imports

**Files:**
- Modify: `src/ai/quality.rs:9`

**Step 1: Update import**

Change line 9 from:
```rust
use crate::ai::types::{AgentStatus, NotificationContent, QuestionType};
```
to:
```rust
use crate::ai::types::{NotificationContent, QuestionType};
use crate::agent_mod::manager::AgentStatus;
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Fewer errors

**Step 3: Commit**

```bash
git add src/ai/quality.rs
git commit -m "refactor(ai): update AgentStatus import in quality"
```

---

## Task 6: Update anthropic.rs re-exports

**Files:**
- Modify: `src/anthropic.rs:15`

**Step 1: Update re-export**

Change line 15 from:
```rust
pub use crate::ai::types::{AgentStatus, NotificationContent, QuestionType};
```
to:
```rust
pub use crate::ai::types::{NotificationContent, QuestionType};
pub use crate::agent_mod::manager::AgentStatus;
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Fewer errors

**Step 3: Commit**

```bash
git add src/anthropic.rs
git commit -m "refactor(anthropic): update AgentStatus re-export"
```

---

## Task 7: Delete AgentState from tui/state.rs

**Files:**
- Modify: `src/tui/state.rs:6-24`
- Modify: `src/tui/state.rs:32`

**Step 1: Delete AgentState enum**

Delete lines 6-24 (the entire AgentState enum and impl block).

**Step 2: Update AgentItem struct**

Change line 32 (after deletion, will be around line 8):
```rust
pub state: AgentState,
```
to:
```rust
pub state: AgentStatus,
```

**Step 3: Add import at top of file**

Add after line 3:
```rust
use crate::agent_mod::manager::AgentStatus;
```

**Step 4: Verify compilation**

Run: `cargo check`
Expected: Errors in tui/app.rs about AgentState

**Step 5: Commit**

```bash
git add src/tui/state.rs
git commit -m "refactor(tui): remove AgentState, use unified AgentStatus"
```

---

## Task 8: Update tui/app.rs

**Files:**
- Modify: `src/tui/app.rs:156-160`

**Step 1: Update status mapping**

Replace lines 156-160:
```rust
let state = match agent.status {
    AgentStatus::Running => AgentState::Running,
    AgentStatus::Waiting => AgentState::Waiting,
    AgentStatus::Stopped => AgentState::Idle,
};
```
with:
```rust
let state = agent.status.clone();
```

**Step 2: Remove AgentState import if present**

Remove any `use super::state::AgentState;` or similar import.

**Step 3: Update icon usage in ui.rs if needed**

The `AgentStatus::icon()` method is already implemented, so TUI rendering should work.

**Step 4: Verify compilation**

Run: `cargo check`
Expected: PASS (all errors resolved)

**Step 5: Commit**

```bash
git add src/tui/app.rs
git commit -m "refactor(tui): use AgentStatus directly, remove conversion"
```

---

## Task 9: Final verification and test

**Step 1: Run full build**

Run: `cargo build --release`
Expected: PASS

**Step 2: Run tests**

Run: `cargo test`
Expected: PASS (or identify failing tests to fix)

**Step 3: Manual test TUI**

Run: `./target/release/cam tui`
Expected: Agent list shows with correct icons (🟢/🟡/❓)

**Step 4: Final commit**

```bash
git add -A
git commit -m "feat(status): complete unified AgentStatus migration"
```

---

## Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | Replace AgentStatus enum | manager.rs |
| 2 | Add update_agent_status() | manager.rs |
| 3 | Delete ai::types::AgentStatus | types.rs, mod.rs |
| 4 | Update extractor.rs imports | extractor.rs |
| 5 | Update quality.rs imports | quality.rs |
| 6 | Update anthropic.rs re-exports | anthropic.rs |
| 7 | Delete tui::AgentState | state.rs |
| 8 | Update tui/app.rs | app.rs |
| 9 | Final verification | - |

**Total estimated changes:** ~100 lines
