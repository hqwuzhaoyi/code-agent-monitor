# Unified Agent Status Migration

## Overview

This migration consolidates three separate `AgentStatus` enums into a single unified enum, adds real-time status synchronization to `agents.json`, and updates all references across the codebase.

**Date:** 2026-02-14
**Branch:** `unified-status`
**Status:** ✅ Complete

## What Changed

### Before: Three Separate Enums

The codebase had three different status representations:

1. **`agent_mod::manager::AgentStatus`** (Running/Waiting/Stopped)
   ```rust
   pub enum AgentStatus {
       Running,
       Waiting,
       Stopped,
   }
   ```

2. **`ai::types::AgentStatus`** (Processing/WaitingForInput/Unknown)
   ```rust
   pub enum AgentStatus {
       Processing,
       WaitingForInput,
       Unknown,
   }
   ```

3. **`tui::AgentState`** (Running/Waiting/Idle)
   ```rust
   pub enum AgentState {
       Running,
       Waiting,
       Idle,
   }
   ```

This caused:
- Redundant type conversions between layers
- Inconsistent status semantics across modules
- Maintenance burden when adding new status types
- Confusion about which enum to use where

### After: Single Unified Enum

All modules now use a single `AgentStatus` enum from `agent_mod::manager`:

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

### New Feature: Real-Time Status Sync

Added `update_agent_status()` method to `AgentManager`:

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

The watcher now calls this method to sync status changes to `agents.json` in real-time.

## Why These Changes Were Made

### 1. Eliminate Redundancy

Three separate enums meant:
- Duplicate code for similar functionality
- Manual conversions between types (e.g., `AgentStatus::Running` → `AgentState::Running`)
- Risk of conversion bugs

### 2. Improve Maintainability

Single source of truth means:
- Add new status types in one place
- Consistent behavior across all modules
- Easier to understand the codebase

### 3. Enable Real-Time Status Tracking

Previously, agent status was only updated when agents started/stopped. Now:
- Watcher detects status changes using AI
- Status syncs to `agents.json` immediately
- TUI shows live status updates
- Notifications can be filtered by status

### 4. Better Semantics

The unified enum uses clearer names:
- `Processing` (not "Running") - agent is actively working
- `WaitingForInput` (not "Waiting") - agent needs user input
- `Unknown` (not "Stopped") - status cannot be determined

## Breaking Changes

### ⚠️ Incompatible `agents.json` Format

The new status values are **not compatible** with old `agents.json` files.

**Old format:**
```json
{
  "agents": [
    {
      "agent_id": "test-123",
      "status": "running"
    }
  ]
}
```

**New format:**
```json
{
  "agents": [
    {
      "agent_id": "test-123",
      "status": "processing"
    }
  ]
}
```

### Migration Required

**Before deploying this version:**

```bash
# Remove old agents.json
rm -f ~/.config/code-agent-monitor/agents.json

# The system will automatically create a new file with the correct format
```

**Why no backwards compatibility?**

This is a development tool with a small user base. Clean migration is simpler than maintaining compatibility code. Users can easily restart their agents.

## Files Changed

### Core Agent Module
- `src/agent_mod/manager.rs` - Replaced enum, added `update_agent_status()`
- `src/agent_mod/watcher.rs` - Added status sync logic

### AI Module
- `src/ai/types.rs` - Deleted duplicate `AgentStatus`
- `src/ai/mod.rs` - Removed `AgentStatus` from exports
- `src/ai/extractor.rs` - Updated imports
- `src/ai/quality.rs` - Updated imports

### Infrastructure
- `src/anthropic.rs` - Updated re-exports
- `src/infra/input.rs` - Updated imports

### TUI Module
- `src/tui/state.rs` - Deleted `AgentState`, use `AgentStatus`
- `src/tui/mod.rs` - Removed `AgentState` from exports
- `src/tui/app.rs` - Removed status conversion logic
- `src/tui/tests.rs` - Updated test code

### Tests
- `tests/unified_status_test.rs` - New tests for unified enum
- `tests/watcher_status_sync_test.rs` - Contract test for sync behavior
- `tests/ai_types_test.rs` - Updated imports

## How to Verify the Migration

### 1. Compilation Check

```bash
cargo check
```

Expected: ✅ No errors

### 2. Run Tests

```bash
cargo test
```

Expected: ✅ All tests pass

Key tests:
- `test_unified_status_variants` - Verifies enum structure
- `test_status_icons` - Verifies TUI icons
- `test_agent_status_is_processing` - Verifies helper methods
- `test_agent_status_is_waiting` - Verifies helper methods

### 3. Build Release Binary

```bash
cargo build --release
```

Expected: ✅ Clean build

### 4. Manual Testing: Status Sync

```bash
# Clean old data
rm -f ~/.config/code-agent-monitor/agents.json

# Start an agent
./target/release/cam start test-agent

# Check initial status (should be "processing")
cat ~/.config/code-agent-monitor/agents.json | jq '.agents[] | {agent_id, status}'

# Wait for agent to become idle (type something and let it respond)
# Check status again (should change to "waiting_for_input")
cat ~/.config/code-agent-monitor/agents.json | jq '.agents[] | {agent_id, status}'
```

Expected output:
```json
{
  "agent_id": "test-agent",
  "status": "processing"
}
```

After agent becomes idle:
```json
{
  "agent_id": "test-agent",
  "status": "waiting_for_input"
}
```

### 5. Manual Testing: TUI Display

```bash
./target/release/cam tui
```

Expected:
- Agent list shows with correct icons:
  - 🟢 for `Processing`
  - 🟡 for `WaitingForInput`
  - ❓ for `Unknown`
- Status updates in real-time as agents change state

### 6. Verify Import Paths

```bash
# All AgentStatus imports should point to agent_mod::manager
rg "use.*AgentStatus" src/
```

Expected: All imports use `crate::agent_mod::manager::AgentStatus` or `crate::agent::manager::AgentStatus`

## Testing Approach

### Test-Driven Development (TDD)

This migration followed strict TDD:

1. **Write failing tests first** (`tests/unified_status_test.rs`)
2. **Implement minimal code** to make tests pass
3. **Refactor** while keeping tests green
4. **Commit frequently** for easy rollback

### Test Coverage

| Test File | Purpose | Status |
|-----------|---------|--------|
| `unified_status_test.rs` | Core enum behavior | ✅ Pass |
| `ai_types_test.rs` | AI types integration | ✅ Pass |
| `watcher_status_sync_test.rs` | Contract test for sync | ✅ Pass |
| `tui/tests.rs` | TUI integration | ✅ Pass |

### Manual Testing

Manual tests verified:
- ✅ Status sync works in real-time
- ✅ TUI displays correct icons
- ✅ Notifications filter by status
- ✅ No compilation errors
- ✅ Clean build

## Implementation Timeline

| Task | Description | Commits |
|------|-------------|---------|
| 0 | Pre-migration cleanup | Manual step |
| 1 | Write failing tests | `c0dad1d` |
| 2 | Replace AgentStatus enum | `3be7a70` |
| 3 | Fix Running references | `7fa0f81` |
| 4 | Add update_agent_status() | `98957ad` |
| 5 | Delete ai::types::AgentStatus | `e9a95ec` |
| 6 | Update extractor.rs imports | `e307f3d` |
| 7 | Update quality.rs imports | `0571849` |
| 8 | Update anthropic.rs re-exports | `1324cb9` |
| 9 | Update infra/input.rs imports | `3cb374b` |
| 10 | Delete tui::AgentState | `96c1a91` |
| 11 | Update tui/mod.rs exports | `c4bc695` |
| 12 | Update tui/app.rs | `f2423cb` |
| 13 | Update tui/tests.rs | `895ebf8` |
| 14 | Wire status sync into watcher | `3255e67` |
| 15 | Final verification | Manual testing |

**Total changes:** ~150 lines across 15 files

## Code Examples

### Before: Status Conversion Hell

```rust
// In tui/app.rs - manual conversion required
let state = match agent.status {
    AgentStatus::Running => AgentState::Running,
    AgentStatus::Waiting => AgentState::Waiting,
    AgentStatus::Stopped => AgentState::Idle,
};
```

### After: Direct Usage

```rust
// In tui/app.rs - no conversion needed
let state = agent.status.clone();
```

### Before: Multiple Imports

```rust
// Different modules used different enums
use crate::ai::types::AgentStatus;        // In AI module
use crate::agent_mod::manager::AgentStatus; // In manager
use crate::tui::AgentState;               // In TUI
```

### After: Single Import

```rust
// All modules use the same enum
use crate::agent_mod::manager::AgentStatus;
```

### New: Status Sync in Watcher

```rust
// In watcher.rs - detect and sync status changes
let current_status = if is_stable && !ai_checked {
    match is_agent_processing(&output) {
        crate::anthropic::AgentStatus::Processing => {
            stability.ai_checked = true;
            AgentStatus::Processing
        }
        crate::anthropic::AgentStatus::WaitingForInput => {
            stability.ai_checked = true;
            AgentStatus::WaitingForInput
        }
        crate::anthropic::AgentStatus::Unknown => {
            AgentStatus::Unknown
        }
    }
} else {
    agent.status.clone()
};

// Update status if changed
if current_status != agent.status {
    if let Err(e) = self.agent_manager.update_agent_status(&agent_id, current_status.clone()) {
        error!(agent_id = %agent_id, error = %e, "Failed to update agent status");
    }
}
```

## Future Maintainers: Key Points

### Adding a New Status

To add a new status variant:

1. Add to enum in `src/agent_mod/manager.rs`:
   ```rust
   pub enum AgentStatus {
       Processing,
       WaitingForInput,
       Unknown,
       YourNewStatus,  // Add here
   }
   ```

2. Update helper methods:
   ```rust
   pub fn icon(&self) -> &'static str {
       match self {
           Self::Processing => "🟢",
           Self::WaitingForInput => "🟡",
           Self::Unknown => "❓",
           Self::YourNewStatus => "🔵",  // Add here
       }
   }
   ```

3. Update tests in `tests/unified_status_test.rs`

4. That's it! No need to update multiple enums or conversion logic.

### Status Sync Behavior

The watcher syncs status every time it detects a change:
- Uses AI (`is_agent_processing()`) to detect current status
- Only syncs when status actually changes (avoids unnecessary writes)
- Logs status changes at DEBUG level
- Errors are logged but don't crash the watcher

### Serialization Format

Status values are serialized as `snake_case`:
- `Processing` → `"processing"`
- `WaitingForInput` → `"waiting_for_input"`
- `Unknown` → `"unknown"`

This is controlled by `#[serde(rename_all = "snake_case")]`.

## Rollback Plan

If issues are found:

```bash
# Revert to previous commit
git revert HEAD~15..HEAD

# Or checkout previous branch
git checkout main

# Clean up new agents.json
rm -f ~/.config/code-agent-monitor/agents.json

# Rebuild
cargo build --release
```

## Related Documentation

- [Implementation Plan](plans/2026-02-14-unified-agent-status-impl-v2.md) - Detailed task breakdown
- [Development Guide](development.md) - Project structure and build process
- [Testing Guide](testing.md) - Test scenarios and E2E testing

## Questions?

For questions or issues:
1. Check the implementation plan for detailed task breakdown
2. Review git history: `git log --oneline --since="2 hours ago"`
3. Run tests: `cargo test --verbose`
4. Check logs: `tail -f ~/.config/code-agent-monitor/hook.log`

---

**Migration completed successfully on 2026-02-14** ✅
