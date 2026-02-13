# Domain Restructure Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 按功能域重构 src/ 目录，根目录只保留 lib.rs 和 main.rs

**Architecture:** 分 8 个阶段迁移，每阶段创建一个功能域模块，通过 re-export 保持向后兼容。从低风险的 infra/ 开始，逐步迁移到高风险的 cli/。

**Tech Stack:** Rust, cargo, git

---

## Phase 0: Setup

### Task 0.1: Create Git Worktree

**Files:**
- None (git operations only)

**Step 1: Create branch**

```bash
git branch refactor/domain-restructure
```

**Step 2: Create worktree**

```bash
git worktree add ../code-agent-monitor-domain refactor/domain-restructure
```

**Step 3: Verify**

Run: `git worktree list`
Expected: 显示新 worktree

**Step 4: Change to worktree**

```bash
cd ../code-agent-monitor-domain
```

---

## Phase 1: Create infra/ Module

### Task 1.1: Create infra/mod.rs

**Files:**
- Create: `src/infra/mod.rs`

**Step 1: Create directory and mod.rs**

```rust
// src/infra/mod.rs
//! 基础设施层 - tmux、进程、终端、解析器

pub mod tmux;
pub mod process;
pub mod terminal;
pub mod jsonl;
pub mod input;

pub use tmux::TmuxManager;
pub use process::ProcessScanner;
pub use jsonl::{JsonlParser, JsonlEvent, format_tool_use, extract_tool_target_from_input};
pub use input::{InputWaitDetector, InputWaitResult, InputWaitPattern};
```

**Step 2: Run build to verify syntax**

Run: `cargo build 2>&1 | head -20`
Expected: FAIL with "file not found" (modules not yet moved)

---

### Task 1.2: Move tmux.rs

**Files:**
- Move: `src/tmux.rs` → `src/infra/tmux.rs`

**Step 1: Move file**

```bash
mv src/tmux.rs src/infra/tmux.rs
```

**Step 2: Update lib.rs - remove old module**

In `src/lib.rs`, change:
```rust
// Remove this line:
pub mod tmux;
```

**Step 3: Add infra module to lib.rs**

```rust
// Add this line:
pub mod infra;
```

**Step 4: Update re-export in lib.rs**

```rust
// Change:
pub use tmux::TmuxManager;
// To:
pub use infra::TmuxManager;
```

**Step 5: Run build**

Run: `cargo build 2>&1 | head -30`
Expected: May have import errors in other files

**Step 6: Fix imports in dependent files**

Search and replace `crate::tmux::` with `crate::infra::tmux::` in:
- src/agent.rs
- src/agent_watcher.rs
- src/session.rs
- src/conversation_state.rs

**Step 7: Run build again**

Run: `cargo build`
Expected: SUCCESS (or more import errors to fix)

**Step 8: Run tests**

Run: `cargo test`
Expected: PASS

**Step 9: Commit**

```bash
git add -A
git commit -m "refactor: move tmux.rs to infra/"
```

---

### Task 1.3: Move process.rs

**Files:**
- Move: `src/process.rs` → `src/infra/process.rs`

**Step 1: Move file**

```bash
mv src/process.rs src/infra/process.rs
```

**Step 2: Update lib.rs**

Remove `pub mod process;` and update re-export to `pub use infra::ProcessScanner;`

**Step 3: Fix imports**

Search `crate::process::` and replace with `crate::infra::process::`

**Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move process.rs to infra/"
```

---

### Task 1.4: Move terminal_utils.rs

**Files:**
- Move: `src/terminal_utils.rs` → `src/infra/terminal.rs`

**Step 1: Move and rename**

```bash
mv src/terminal_utils.rs src/infra/terminal.rs
```

**Step 2: Update lib.rs**

Remove `pub mod terminal_utils;`

**Step 3: Update infra/mod.rs**

Already has `pub mod terminal;`

**Step 4: Fix imports**

Search `crate::terminal_utils::` and replace with `crate::infra::terminal::`

**Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor: move terminal_utils.rs to infra/terminal.rs"
```

---

### Task 1.5: Move jsonl_parser.rs

**Files:**
- Move: `src/jsonl_parser.rs` → `src/infra/jsonl.rs`

**Step 1: Move and rename**

```bash
mv src/jsonl_parser.rs src/infra/jsonl.rs
```

**Step 2: Update lib.rs**

Remove `pub mod jsonl_parser;` and update re-exports.

**Step 3: Fix imports**

Search `crate::jsonl_parser::` and replace with `crate::infra::jsonl::`

**Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move jsonl_parser.rs to infra/jsonl.rs"
```

---

### Task 1.6: Move input_detector.rs

**Files:**
- Move: `src/input_detector.rs` → `src/infra/input.rs`

**Step 1: Move and rename**

```bash
mv src/input_detector.rs src/infra/input.rs
```

**Step 2: Update lib.rs**

Remove `pub mod input_detector;` and update re-exports.

**Step 3: Fix imports**

Search `crate::input_detector::` and replace with `crate::infra::input::`

**Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move input_detector.rs to infra/input.rs"
```

---

## Phase 2: Create agent/ Module

### Task 2.1: Create agent/mod.rs

**Files:**
- Create: `src/agent/mod.rs`

**Step 1: Create directory and mod.rs**

```rust
// src/agent/mod.rs
//! Agent 生命周期管理 - 启动、监控、停止

pub mod manager;
pub mod watcher;
pub mod daemon;

pub use manager::{AgentManager, AgentRecord, AgentType, AgentStatus, StartAgentRequest, StartAgentResponse};
pub use watcher::{AgentWatcher, WatchEvent, AgentSnapshot, format_watch_event};
pub use daemon::WatcherDaemon;
```

**Step 2: Commit skeleton**

```bash
git add src/agent/mod.rs
git commit -m "feat: add agent/ module skeleton"
```

---

### Task 2.2: Move agent.rs to agent/manager.rs

**Files:**
- Move: `src/agent.rs` → `src/agent/manager.rs`

**Step 1: Move file**

```bash
mv src/agent.rs src/agent/manager.rs
```

**Step 2: Update lib.rs**

Remove `pub mod agent;`, add `pub mod agent;` (now points to directory)

**Step 3: Fix imports in agent/manager.rs**

Update internal imports to use `crate::infra::` for tmux, process etc.

**Step 4: Fix imports in other files**

Search `crate::agent::` - most should still work via re-export, but check:
- src/agent_watcher.rs
- src/mcp.rs
- src/team/orchestrator.rs

**Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor: move agent.rs to agent/manager.rs"
```

---

### Task 2.3: Move agent_watcher.rs to agent/watcher.rs

**Files:**
- Move: `src/agent_watcher.rs` → `src/agent/watcher.rs`

**Step 1: Move file**

```bash
mv src/agent_watcher.rs src/agent/watcher.rs
```

**Step 2: Update lib.rs**

Remove `pub mod agent_watcher;`

**Step 3: Fix imports**

Search `crate::agent_watcher::` and replace with `crate::agent::watcher::`

**Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move agent_watcher.rs to agent/watcher.rs"
```

---

### Task 2.4: Move watcher_daemon.rs to agent/daemon.rs

**Files:**
- Move: `src/watcher_daemon.rs` → `src/agent/daemon.rs`

**Step 1: Move file**

```bash
mv src/watcher_daemon.rs src/agent/daemon.rs
```

**Step 2: Update lib.rs**

Remove `pub mod watcher_daemon;`

**Step 3: Fix imports**

Search `crate::watcher_daemon::` and replace with `crate::agent::daemon::`

**Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move watcher_daemon.rs to agent/daemon.rs"
```

---

### Task 2.5: Remove old watcher/ module

**Files:**
- Delete: `src/watcher/` directory

**Step 1: Check if watcher/ is used**

```bash
grep -r "crate::watcher::" src/
```

If used, merge functionality into agent/watcher.rs first.

**Step 2: Remove directory**

```bash
rm -rf src/watcher/
```

**Step 3: Update lib.rs**

Remove `pub mod watcher;`

**Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: remove redundant watcher/ module"
```

---

## Phase 3: Create session/ Module

### Task 3.1: Create session/mod.rs

**Files:**
- Create: `src/session/mod.rs`

**Step 1: Create directory and mod.rs**

```rust
// src/session/mod.rs
//! 会话管理 - Claude Code 会话和对话状态

pub mod manager;
pub mod state;

pub use manager::{SessionManager, SessionFilter};
pub use state::{ConversationStateManager, ConversationState, PendingConfirmation, ConfirmationType, AgentContext, ReplyResult};
```

**Step 2: Commit skeleton**

```bash
git add src/session/mod.rs
git commit -m "feat: add session/ module skeleton"
```

---

### Task 3.2: Move session.rs to session/manager.rs

**Files:**
- Move: `src/session.rs` → `src/session/manager.rs`

**Step 1: Move file**

```bash
mv src/session.rs src/session/manager.rs
```

**Step 2: Update lib.rs**

Change `pub mod session;` to point to new module, update re-exports.

**Step 3: Fix imports**

Search `crate::session::` and update as needed.

**Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move session.rs to session/manager.rs"
```

---

### Task 3.3: Move conversation_state.rs to session/state.rs

**Files:**
- Move: `src/conversation_state.rs` → `src/session/state.rs`

**Step 1: Move file**

```bash
mv src/conversation_state.rs src/session/state.rs
```

**Step 2: Update lib.rs**

Remove `pub mod conversation_state;`

**Step 3: Fix imports**

Search `crate::conversation_state::` and replace with `crate::session::state::`

**Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move conversation_state.rs to session/state.rs"
```

---

## Phase 4: Consolidate ai/ Module

### Task 4.1: Move ai_types.rs to ai/types.rs

**Files:**
- Move: `src/ai_types.rs` → `src/ai/types.rs`

**Step 1: Move file**

```bash
mv src/ai_types.rs src/ai/types.rs
```

**Step 2: Update ai/mod.rs**

Add `pub mod types;` and re-exports.

**Step 3: Update lib.rs**

Remove `pub mod ai_types;`

**Step 4: Fix imports**

Search `crate::ai_types::` and replace with `crate::ai::types::`

**Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor: move ai_types.rs to ai/types.rs"
```

---

### Task 4.2: Move ai_quality.rs to ai/quality.rs

**Files:**
- Move: `src/ai_quality.rs` → `src/ai/quality.rs`

**Step 1: Move file**

```bash
mv src/ai_quality.rs src/ai/quality.rs
```

**Step 2: Update ai/mod.rs**

Add `pub mod quality;`

**Step 3: Update lib.rs**

Remove `pub mod ai_quality;`

**Step 4: Fix imports**

Search `crate::ai_quality::` and replace with `crate::ai::quality::`

**Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor: move ai_quality.rs to ai/quality.rs"
```

---

### Task 4.3: Remove anthropic.rs

**Files:**
- Delete: `src/anthropic.rs`

**Step 1: Check usage**

```bash
grep -r "crate::anthropic::" src/
```

**Step 2: Update imports to use ai/ module**

Replace `crate::anthropic::` with appropriate `crate::ai::` imports.

**Step 3: Remove file**

```bash
rm src/anthropic.rs
```

**Step 4: Update lib.rs**

Remove `pub mod anthropic;` and update re-exports.

**Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor: remove anthropic.rs, use ai/ module"
```

---

## Phase 5: Consolidate mcp/ Module

### Task 5.1: Merge mcp.rs into mcp/mod.rs

**Files:**
- Modify: `src/mcp.rs` → `src/mcp/mod.rs`
- Delete: `src/mcp_new/`

**Step 1: Create mcp/ directory**

```bash
mkdir -p src/mcp
```

**Step 2: Move mcp.rs to mcp/mod.rs**

```bash
mv src/mcp.rs src/mcp/mod.rs
```

**Step 3: Move mcp_new/ contents**

```bash
mv src/mcp_new/types.rs src/mcp/types.rs
mv src/mcp_new/tools src/mcp/tools
rm -rf src/mcp_new
```

**Step 4: Update mcp/mod.rs**

Add module declarations for types and tools.

**Step 5: Update lib.rs**

Remove `pub mod mcp_new;`

**Step 6: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 7: Commit**

```bash
git add -A
git commit -m "refactor: consolidate mcp/ module"
```

---

## Phase 6: Consolidate notification/ Module

### Task 6.1: Move openclaw_notifier.rs to notification/notifier.rs

**Files:**
- Move: `src/openclaw_notifier.rs` → `src/notification/notifier.rs`

**Step 1: Move file**

```bash
mv src/openclaw_notifier.rs src/notification/notifier.rs
```

**Step 2: Update notification/mod.rs**

Add `pub mod notifier;` and `pub use notifier::OpenclawNotifier;`

**Step 3: Update lib.rs**

Remove `pub mod openclaw_notifier;`

**Step 4: Fix imports**

Search `crate::openclaw_notifier::` and replace with `crate::notification::notifier::`

**Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor: move openclaw_notifier.rs to notification/notifier.rs"
```

---

### Task 6.2: Remove notify.rs

**Files:**
- Delete: `src/notify.rs`

**Step 1: Check usage**

```bash
grep -r "crate::notify::" src/
```

**Step 2: Update or remove usages**

If still used, migrate to notification/ module.

**Step 3: Remove file**

```bash
rm src/notify.rs
```

**Step 4: Update lib.rs**

Remove `pub mod notify;`

**Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor: remove deprecated notify.rs"
```

---

## Phase 7: Consolidate team/ Module

### Task 7.1: Move task_list.rs to team/task.rs

**Files:**
- Move: `src/task_list.rs` → `src/team/task.rs`

**Step 1: Move file**

```bash
mv src/task_list.rs src/team/task.rs
```

**Step 2: Update team/mod.rs**

Add `pub mod task;` and re-exports.

**Step 3: Update lib.rs**

Remove `pub mod task_list;`

**Step 4: Fix imports**

Search `crate::task_list::` and replace with `crate::team::task::`

**Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor: move task_list.rs to team/task.rs"
```

---

## Phase 8: Final Cleanup

### Task 8.1: Update lib.rs re-exports

**Files:**
- Modify: `src/lib.rs`

**Step 1: Clean up lib.rs**

Final lib.rs should look like:

```rust
//! Code Agent Monitor - 监控和管理 AI 编码代理进程

pub mod infra;
pub mod agent;
pub mod session;
pub mod ai;
pub mod mcp;
pub mod notification;
pub mod team;
pub mod cli;

// Re-exports for backwards compatibility
pub use infra::{TmuxManager, ProcessScanner, JsonlParser, JsonlEvent, InputWaitDetector, InputWaitResult, InputWaitPattern};
pub use agent::{AgentManager, AgentRecord, AgentType, AgentStatus, AgentWatcher, WatchEvent, WatcherDaemon};
pub use session::{SessionManager, SessionFilter, ConversationStateManager, ConversationState, PendingConfirmation};
pub use ai::{AnthropicClient, AnthropicConfig};
pub use mcp::McpServer;
pub use notification::{OpenclawNotifier, SendResult, NotificationEvent};
pub use team::{TeamConfig, TeamMember, TeamBridge, TeamOrchestrator};
```

**Step 2: Build and test**

Run: `cargo build && cargo test`
Expected: PASS

**Step 3: Commit**

```bash
git add -A
git commit -m "refactor: finalize lib.rs re-exports"
```

---

### Task 8.2: Verify root directory is clean

**Step 1: Check src/ root**

```bash
ls src/*.rs
```

Expected: Only `lib.rs` and `main.rs`

**Step 2: Run full test suite**

```bash
cargo test
cargo clippy
```

Expected: PASS

**Step 3: Final commit**

```bash
git add -A
git commit -m "refactor: complete domain restructure"
```

---

## Merge Strategy

### Task M.1: Merge to main

```bash
cd /path/to/main
git merge refactor/domain-restructure
cargo build --release
cargo test
git push origin main
```

### Task M.2: Cleanup worktree

```bash
git worktree remove ../code-agent-monitor-domain
git branch -d refactor/domain-restructure
```

---

## Success Criteria

- [ ] src/ 根目录只有 lib.rs 和 main.rs
- [ ] 6 个功能域模块: agent/, session/, ai/, mcp/, notification/, team/
- [ ] 1 个基础设施模块: infra/
- [ ] 1 个 CLI 模块: cli/
- [ ] cargo build --release 通过
- [ ] cargo test 通过
- [ ] 向后兼容（re-export 保持 API 不变）
