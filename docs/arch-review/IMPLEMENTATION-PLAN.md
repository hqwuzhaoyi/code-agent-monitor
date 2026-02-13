# Arch-Review Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor CAM codebase to resolve circular dependencies, eliminate code duplication, and improve module organization.

**Architecture:** Parallel implementation using 4 git worktrees (P0-P3), each handling one priority level. Agent Team coordinates work, merging in priority order.

**Tech Stack:** Rust, cargo, git worktrees, Agent Teams

---

## Phase 0: Setup

### Task 0.1: Create Git Worktrees

**Files:**
- None (git operations only)

**Step 1: Create worktree branches**

```bash
git branch refactor/p0-foundation
git branch refactor/p1-core
git branch refactor/p2-modules
git branch refactor/p3-architecture
```

**Step 2: Create worktrees**

```bash
git worktree add ../code-agent-monitor-p0 refactor/p0-foundation
git worktree add ../code-agent-monitor-p1 refactor/p1-core
git worktree add ../code-agent-monitor-p2 refactor/p2-modules
git worktree add ../code-agent-monitor-p3 refactor/p3-architecture
```

**Step 3: Verify worktrees**

Run: `git worktree list`
Expected: 5 worktrees listed (main + 4 refactor branches)

---

## P0: Foundation (No Dependencies)

### Task P0.1: Create ai_types.rs - Types

**Files:**
- Create: `src/ai_types.rs`
- Test: `tests/ai_types_test.rs`

**Step 1: Write the failing test**

```rust
// tests/ai_types_test.rs
use code_agent_monitor::ai_types::{AgentStatus, QuestionType, NotificationContent};

#[test]
fn test_agent_status_default() {
    let status = AgentStatus::default();
    assert_eq!(status, AgentStatus::Unknown);
}

#[test]
fn test_question_type_default() {
    let qt = QuestionType::default();
    assert_eq!(qt, QuestionType::OpenEnded);
}

#[test]
fn test_notification_content_default() {
    let content = NotificationContent::default();
    assert_eq!(content.summary, "等待输入");
    assert!(content.options.is_empty());
}

#[test]
fn test_notification_content_confirmation() {
    let content = NotificationContent::confirmation("Allow access?");
    assert_eq!(content.question_type, QuestionType::Confirmation);
    assert_eq!(content.question, "Allow access?");
}

#[test]
fn test_notification_content_options() {
    let content = NotificationContent::options("Choose:", vec!["A".into(), "B".into()]);
    assert_eq!(content.question_type, QuestionType::Options);
    assert_eq!(content.options.len(), 2);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --test ai_types_test`
Expected: FAIL with "unresolved import"

**Step 3: Write ai_types.rs**

```rust
// src/ai_types.rs
//! Shared AI types - breaks circular dependency between anthropic and ai_quality

use serde::{Deserialize, Serialize};

/// Agent processing status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentStatus {
    /// Agent is actively processing
    Processing,
    /// Agent is waiting for user input
    WaitingForInput,
    /// Status cannot be determined
    #[default]
    Unknown,
}

/// Question type classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    /// Multiple choice options
    Options,
    /// Yes/No confirmation
    Confirmation,
    /// Open-ended question
    OpenEnded,
}

impl Default for QuestionType {
    fn default() -> Self {
        Self::OpenEnded
    }
}

/// Extracted notification content from terminal snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationContent {
    /// Question type
    pub question_type: QuestionType,
    /// Full question text
    pub question: String,
    /// Options list (only for Options type)
    pub options: Vec<String>,
    /// Brief summary (under 10 chars)
    pub summary: String,
    /// Reply hint for user
    pub reply_hint: String,
}

impl Default for NotificationContent {
    fn default() -> Self {
        Self {
            question_type: QuestionType::OpenEnded,
            question: String::new(),
            options: Vec::new(),
            summary: "等待输入".to_string(),
            reply_hint: String::new(),
        }
    }
}

impl NotificationContent {
    /// Create confirmation type content
    pub fn confirmation(question: &str) -> Self {
        Self {
            question_type: QuestionType::Confirmation,
            question: question.to_string(),
            options: Vec::new(),
            summary: "请求确认".to_string(),
            reply_hint: "y/n".to_string(),
        }
    }

    /// Create options type content
    pub fn options(question: &str, options: Vec<String>) -> Self {
        let hint = if options.len() <= 4 {
            (1..=options.len()).map(|i| i.to_string()).collect::<Vec<_>>().join("/")
        } else {
            "1-N".to_string()
        };
        Self {
            question_type: QuestionType::Options,
            question: question.to_string(),
            options,
            summary: "等待选择".to_string(),
            reply_hint: hint,
        }
    }

    /// Create open-ended question content
    pub fn open_ended(question: &str) -> Self {
        Self {
            question_type: QuestionType::OpenEnded,
            question: question.to_string(),
            options: Vec::new(),
            summary: "等待回复".to_string(),
            reply_hint: String::new(),
        }
    }
}
```

**Step 4: Add module to lib.rs**

```rust
// Add to src/lib.rs after line 1
pub mod ai_types;
```

**Step 5: Run test to verify it passes**

Run: `cargo test --test ai_types_test`
Expected: PASS (5 tests)

**Step 6: Commit**

```bash
git add src/ai_types.rs tests/ai_types_test.rs src/lib.rs
git commit -m "feat: add ai_types module to break circular dependency"
```

---

### Task P0.2: Migrate anthropic.rs to use ai_types

**Files:**
- Modify: `src/anthropic.rs:44-115` (remove type definitions)
- Modify: `src/anthropic.rs:1-20` (add import)

**Step 1: Update imports in anthropic.rs**

Replace lines 44-115 (type definitions) with:

```rust
// Re-export from ai_types for backwards compatibility
pub use crate::ai_types::{AgentStatus, QuestionType, NotificationContent};
```

**Step 2: Run existing tests**

Run: `cargo test`
Expected: PASS (all existing tests should still work)

**Step 3: Commit**

```bash
git add src/anthropic.rs
git commit -m "refactor: migrate anthropic.rs to use ai_types"
```

---

### Task P0.3: Migrate ai_quality.rs to use ai_types

**Files:**
- Modify: `src/ai_quality.rs:9` (change import)

**Step 1: Update import**

Change line 9 from:
```rust
use crate::anthropic::{AgentStatus, NotificationContent, QuestionType};
```

To:
```rust
use crate::ai_types::{AgentStatus, NotificationContent, QuestionType};
```

**Step 2: Run tests**

Run: `cargo test`
Expected: PASS

**Step 3: Verify no circular dependency**

Run: `cargo build --release`
Expected: SUCCESS (circular dependency resolved)

**Step 4: Commit**

```bash
git add src/ai_quality.rs
git commit -m "refactor: migrate ai_quality.rs to use ai_types

Resolves circular dependency: anthropic <-> ai_quality"
```

---

### Task P0.4: Unify tmux operations

**Files:**
- Modify: `src/session.rs` (remove send_to_tmux)
- Modify: `src/conversation_state.rs` (remove send_to_tmux, use TmuxManager)

**Step 1: Check current duplicates**

Verify duplicate implementations exist:
- `src/session.rs` - `send_to_tmux` function
- `src/conversation_state.rs` - `send_to_tmux` function
- `src/tmux.rs` - `TmuxManager::send_keys`

**Step 2: Update session.rs to use TmuxManager**

Find the `send_to_tmux` function in session.rs and replace calls with:

```rust
use crate::tmux::TmuxManager;

// Replace direct tmux calls with:
let tmux = TmuxManager::new();
tmux.send_keys(session_name, input)?;
```

**Step 3: Update conversation_state.rs to use TmuxManager**

Same pattern - replace `send_to_tmux` with `TmuxManager::send_keys`.

**Step 4: Run tests**

Run: `cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add src/session.rs src/conversation_state.rs
git commit -m "refactor: unify tmux operations through TmuxManager

Removes duplicate send_to_tmux implementations"
```

---

## P1: Core Refactoring (Depends on P0)

### Task P1.1: Create watcher module structure

**Files:**
- Create: `src/watcher/mod.rs`
- Create: `src/watcher/agent_monitor.rs`
- Create: `src/watcher/event_processor.rs`
- Create: `src/watcher/stability.rs`

**Step 1: Create directory and mod.rs**

```rust
// src/watcher/mod.rs
//! Agent watching subsystem - monitors agent state and processes events

mod agent_monitor;
mod event_processor;
mod stability;

pub use agent_monitor::AgentMonitor;
pub use event_processor::EventProcessor;
pub use stability::{StabilityState, StabilityDetector};
```

**Step 2: Create agent_monitor.rs skeleton**

```rust
// src/watcher/agent_monitor.rs
//! Agent lifecycle monitoring - tmux session health checks

use anyhow::Result;
use crate::agent::AgentRecord;
use crate::tmux::TmuxManager;

/// Monitors agent tmux sessions for health
pub struct AgentMonitor {
    tmux: TmuxManager,
}

impl AgentMonitor {
    pub fn new() -> Self {
        Self { tmux: TmuxManager::new() }
    }

    /// Check if agent's tmux session is still alive
    pub fn is_alive(&self, agent: &AgentRecord) -> bool {
        self.tmux.session_exists(&agent.tmux_session)
    }

    /// Capture current terminal content
    pub fn capture_terminal(&self, agent: &AgentRecord, lines: u32) -> Result<String> {
        self.tmux.capture_pane(&agent.tmux_session, lines)
    }
}
```

**Step 3: Create event_processor.rs skeleton**

```rust
// src/watcher/event_processor.rs
//! JSONL event processing - parses and transforms agent events

use crate::jsonl_parser::{JsonlParser, JsonlEvent};

/// Processes JSONL events from agent logs
pub struct EventProcessor {
    parser: JsonlParser,
}

impl EventProcessor {
    pub fn new(log_path: &str) -> Self {
        Self {
            parser: JsonlParser::new(log_path),
        }
    }

    /// Read new events since last check
    pub fn read_new_events(&mut self) -> Vec<JsonlEvent> {
        self.parser.read_new_events().unwrap_or_default()
    }
}
```

**Step 4: Create stability.rs skeleton**

```rust
// src/watcher/stability.rs
//! Terminal stability detection - determines when content has settled

use std::collections::HashMap;

/// Tracks terminal content stability
#[derive(Debug, Clone)]
pub struct StabilityState {
    /// Content fingerprint (hash)
    pub fingerprint: u64,
    /// Consecutive stable checks
    pub stable_count: u32,
    /// Last check timestamp
    pub last_check: std::time::Instant,
}

impl Default for StabilityState {
    fn default() -> Self {
        Self {
            fingerprint: 0,
            stable_count: 0,
            last_check: std::time::Instant::now(),
        }
    }
}

/// Detects when terminal content has stabilized
pub struct StabilityDetector {
    states: HashMap<String, StabilityState>,
    threshold: u32,
}

impl StabilityDetector {
    pub fn new(threshold: u32) -> Self {
        Self {
            states: HashMap::new(),
            threshold,
        }
    }

    /// Check if content is stable
    pub fn is_stable(&mut self, agent_id: &str, content: &str) -> bool {
        let fingerprint = Self::hash_content(content);
        let state = self.states.entry(agent_id.to_string()).or_default();

        if state.fingerprint == fingerprint {
            state.stable_count += 1;
        } else {
            state.fingerprint = fingerprint;
            state.stable_count = 1;
        }
        state.last_check = std::time::Instant::now();

        state.stable_count >= self.threshold
    }

    fn hash_content(content: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }
}
```

**Step 5: Add watcher module to lib.rs**

```rust
// Add to src/lib.rs
pub mod watcher;
```

**Step 6: Run build**

Run: `cargo build`
Expected: SUCCESS

**Step 7: Commit**

```bash
git add src/watcher/ src/lib.rs
git commit -m "feat: add watcher module structure

Prepares for agent_watcher.rs split"
```

---

### Task P1.2: Migrate agent_watcher.rs to use watcher module

**Files:**
- Modify: `src/agent_watcher.rs` (use new watcher modules)

**Step 1: Update imports**

Add to agent_watcher.rs:
```rust
use crate::watcher::{AgentMonitor, EventProcessor, StabilityDetector};
```

**Step 2: Replace inline implementations with module calls**

Gradually replace:
- tmux session checks → `AgentMonitor::is_alive`
- JSONL parsing → `EventProcessor::read_new_events`
- Stability tracking → `StabilityDetector::is_stable`

**Step 3: Run tests**

Run: `cargo test`
Expected: PASS

**Step 4: Commit**

```bash
git add src/agent_watcher.rs
git commit -m "refactor: migrate agent_watcher to use watcher module"
```

---

## P2: Module Restructuring (Depends on P1)

### Task P2.1: Create ai module structure

**Files:**
- Create: `src/ai/mod.rs`
- Create: `src/ai/client.rs`
- Create: `src/ai/extractor.rs`

**Step 1: Create ai/mod.rs**

```rust
// src/ai/mod.rs
//! AI subsystem - Anthropic API client and content extraction

mod client;
mod extractor;

pub use client::{AnthropicClient, AnthropicConfig};
pub use extractor::{extract_question_with_haiku, extract_notification_content, is_agent_processing};
```

**Step 2: Move client code to ai/client.rs**

Extract from anthropic.rs:
- `AnthropicConfig` struct and impl
- `AnthropicClient` struct and impl
- API constants
- HTTP request/response types

**Step 3: Move extraction code to ai/extractor.rs**

Extract from anthropic.rs:
- `extract_question_with_haiku`
- `extract_notification_content`
- `is_agent_processing`
- Related helper functions

**Step 4: Update lib.rs**

```rust
pub mod ai;
// Keep anthropic.rs as re-export for backwards compatibility
```

**Step 5: Run tests**

Run: `cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add src/ai/ src/lib.rs
git commit -m "feat: create ai module structure

Splits anthropic.rs into client and extractor"
```

---

### Task P2.2: Create mcp module structure

**Files:**
- Create: `src/mcp/mod.rs`
- Create: `src/mcp/types.rs`
- Create: `src/mcp/tools/mod.rs`
- Create: `src/mcp/tools/agent.rs`
- Create: `src/mcp/tools/session.rs`
- Create: `src/mcp/tools/team.rs`
- Create: `src/mcp/tools/task.rs`

**Step 1: Create mcp/mod.rs**

```rust
// src/mcp/mod.rs
//! MCP Server - Model Context Protocol implementation

mod types;
mod tools;

pub use types::{McpRequest, McpResponse, McpError};
pub use tools::handle_tool_call;

use anyhow::Result;

/// MCP Server
pub struct McpServer {
    // ... existing fields
}

impl McpServer {
    // ... existing methods, delegating to tools module
}
```

**Step 2: Create mcp/types.rs**

Extract from mcp.rs:
- `McpRequest`, `McpResponse`, `McpError`
- Tool definitions
- JSON-RPC types

**Step 3: Create mcp/tools/mod.rs**

```rust
// src/mcp/tools/mod.rs
mod agent;
mod session;
mod team;
mod task;

pub use agent::*;
pub use session::*;
pub use team::*;
pub use task::*;

pub fn handle_tool_call(name: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    match name {
        n if n.starts_with("agent_") => agent::handle(n, params),
        n if n.starts_with("session_") => session::handle(n, params),
        n if n.starts_with("team_") => team::handle(n, params),
        n if n.starts_with("task_") => task::handle(n, params),
        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}
```

**Step 4: Create tool handler files**

Each file handles its domain's tools:
- `agent.rs`: agent_list, agent_info, agent_kill, etc.
- `session.rs`: session_list, session_resume, etc.
- `team.rs`: team_create, team_spawn, team_progress, etc.
- `task.rs`: task_list, task_create, task_update, etc.

**Step 5: Run tests**

Run: `cargo test`
Expected: PASS

**Step 6: Commit**

```bash
git add src/mcp/
git commit -m "feat: create mcp module structure

Splits mcp.rs into types and tool handlers"
```

---

## P3: Architecture (Depends on P2)

### Task P3.1: Create cli module structure

**Files:**
- Create: `src/cli/mod.rs`
- Create: `src/cli/commands/mod.rs`
- Create: `src/cli/commands/list.rs`
- Create: `src/cli/commands/session.rs`
- Create: `src/cli/commands/agent.rs`
- Create: `src/cli/commands/team.rs`
- Create: `src/cli/commands/notify.rs`
- Create: `src/cli/commands/daemon.rs`
- Create: `src/cli/output.rs`

**Step 1: Create cli/mod.rs**

```rust
// src/cli/mod.rs
//! CLI command handling

mod commands;
mod output;

pub use commands::*;
pub use output::*;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cam")]
#[command(about = "Code Agent Monitor - manage AI coding agents")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    // ... existing commands
}
```

**Step 2: Create command handler files**

Each file handles its command group.

**Step 3: Update main.rs**

```rust
// src/main.rs
use code_agent_monitor::cli::{Cli, Commands};
use clap::Parser;

fn main() {
    let cli = Cli::parse();
    // ... delegate to cli module
}
```

**Step 4: Run tests**

Run: `cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add src/cli/ src/main.rs
git commit -m "feat: create cli module structure

Splits main.rs into command handlers"
```

---

### Task P3.2: Consolidate notification module

**Files:**
- Move: `src/notification_summarizer.rs` → `src/notification/summarizer.rs`
- Move: `src/throttle.rs` → `src/notification/throttle.rs`
- Modify: `src/notification/mod.rs`
- Modify: `src/lib.rs`

**Step 1: Move files**

```bash
mv src/notification_summarizer.rs src/notification/summarizer.rs
mv src/throttle.rs src/notification/throttle.rs
```

**Step 2: Update notification/mod.rs**

```rust
// Add to src/notification/mod.rs
pub mod summarizer;
pub mod throttle;

pub use summarizer::*;
pub use throttle::*;
```

**Step 3: Update lib.rs imports**

Remove old module declarations, add re-exports from notification.

**Step 4: Run tests**

Run: `cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add src/notification/ src/lib.rs
git commit -m "refactor: consolidate notification module

Moves summarizer and throttle into notification/"
```

---

## Merge Strategy

### Task M.1: Merge P0 to main

```bash
cd /path/to/main
git merge refactor/p0-foundation
cargo build --release
cargo test
```

### Task M.2: Rebase and merge P1

```bash
cd ../code-agent-monitor-p1
git rebase main
# Resolve conflicts if any
cd ../code-agent-monitor
git merge refactor/p1-core
cargo build --release
cargo test
```

### Task M.3: Rebase and merge P2

```bash
cd ../code-agent-monitor-p2
git rebase main
cd ../code-agent-monitor
git merge refactor/p2-modules
cargo build --release
cargo test
```

### Task M.4: Rebase and merge P3

```bash
cd ../code-agent-monitor-p3
git rebase main
cd ../code-agent-monitor
git merge refactor/p3-architecture
cargo build --release
cargo test
cargo clippy
```

---

## Success Criteria

- [ ] `cargo build --release` passes
- [ ] `cargo test` passes
- [ ] `cargo clippy` clean
- [ ] No circular dependencies
- [ ] All files < 800 lines
- [ ] No duplicate tmux implementations
- [ ] Unified question struct (NotificationContent)
