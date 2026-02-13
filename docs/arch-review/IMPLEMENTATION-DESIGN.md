# Arch-Review Implementation Design

## Overview

Parallel implementation of architecture refactoring identified in `docs/arch-review/` using Agent Teams and git worktrees.

## Approach

4 git worktrees organized by priority level, with an Agent Team coordinating parallel work. Each priority level builds on the previous, minimizing merge conflicts.

## Team Structure

| Agent | Worktree | Branch | Responsibilities |
|-------|----------|--------|------------------|
| team-lead | main | main | Coordination, merge management |
| p0-agent | code-agent-monitor-p0 | refactor/p0-foundation | ai_types, tmux unification |
| p1-agent | code-agent-monitor-p1 | refactor/p1-core | agent_watcher split, formatter |
| p2-agent | code-agent-monitor-p2 | refactor/p2-modules | anthropic split, mcp split |
| p3-agent | code-agent-monitor-p3 | refactor/p3-architecture | main.rs split, consolidation |

## P0: Foundation (No Dependencies)

### Task 1: Create ai_types.rs

Extract shared types to break circular dependency between `anthropic.rs` and `ai_quality.rs`:

```rust
// src/ai_types.rs
pub enum AgentStatus { Processing, WaitingForInput, Unknown }
pub enum QuestionType { YesNo, MultipleChoice, FreeForm, Unknown }
pub struct NotificationContent { ... }
pub struct QuestionContent { ... }  // Unified from NotificationContent + ExtractedQuestion
```

### Task 2: Unify tmux operations

Remove duplicate `send_to_tmux` implementations:
- Delete `session.rs:237-249`
- Delete `conversation_state.rs:331-353`
- Use `tmux::TmuxManager::send_keys()` everywhere

### Task 3: Unify question struct

Merge `NotificationContent` and `ExtractedQuestion` into `QuestionContent`:

```rust
pub struct QuestionContent {
    pub question_type: QuestionType,
    pub question: String,
    pub options: Vec<String>,
    pub summary: String,
    pub reply_hint: String,
}
```

## P1: Core Refactoring (Depends on P0)

### Task 1: Split agent_watcher.rs

```
src/watcher/
├── mod.rs              # AgentWatcher struct, poll_once orchestration
├── agent_monitor.rs    # tmux session monitoring, cleanup
├── event_processor.rs  # JSONL parsing, event transformation
└── stability.rs        # Terminal stability detection, hook coordination
```

### Task 2: Decouple formatter from AI

Change formatter to accept pre-extracted content:

```rust
// Before: formatter calls AI internally
pub fn format_notification_event(&self, event: &NotificationEvent) -> String

// After: caller extracts, formatter formats
pub fn format_notification_event(
    &self,
    event: &NotificationEvent,
    extracted: Option<&QuestionContent>
) -> String
```

## P2: Module Restructuring (Depends on P1)

### Task 1: Split anthropic.rs

```
src/ai/
├── mod.rs              # Re-exports
├── client.rs           # AnthropicClient, config loading (~300 lines)
└── extractor.rs        # Question/status extraction (~500 lines)
```

### Task 2: Split mcp.rs

```
src/mcp/
├── mod.rs              # McpServer, request routing
├── types.rs            # McpRequest, McpResponse, McpError
└── tools/
    ├── mod.rs          # Tool registration
    ├── agent.rs        # agent_* tools
    ├── session.rs      # session_* tools
    ├── team.rs         # team_* tools
    └── task.rs         # task_* tools
```

## P3: Architecture (Depends on P2)

### Task 1: Split main.rs

```
src/cli/
├── mod.rs              # Cli struct, Commands enum
├── output.rs           # JSON/table formatting
└── commands/
    ├── mod.rs
    ├── list.rs         # list, info
    ├── session.rs      # sessions, resume, logs
    ├── agent.rs        # kill, watch
    ├── team.rs         # team-* commands
    ├── notify.rs       # notify command
    └── daemon.rs       # watch-daemon, serve
```

### Task 2: Consolidate notification module

```
src/notification/
├── summarizer.rs       # ← from notification_summarizer.rs
├── throttle.rs         # ← from throttle.rs
└── legacy/
    └── notify.rs       # ← from notify.rs (deprecated)
```

## TDD Testing Strategy

Each worktree writes unit tests for new modules:

| Priority | Test File | Coverage |
|----------|-----------|----------|
| P0 | tests/ai_types_test.rs | Type serialization, enum variants |
| P1 | tests/watcher_test.rs | Event processing, stability |
| P2 | tests/ai_test.rs, tests/mcp_test.rs | Client, extraction, tools |
| P3 | tests/cli_test.rs | Command parsing |

## Merge Strategy

1. **P0 → main**: Foundation types, no conflicts expected
2. **P1 → main**: Rebase on P0, resolve watcher imports
3. **P2 → main**: Rebase on P1, resolve ai/mcp imports
4. **P3 → main**: Rebase on P2, final architecture

Each merge requires:
- `cargo build --release` passes
- `cargo test` passes
- Team-lead code review

## Success Criteria

- [ ] No circular dependencies (anthropic ↔ ai_quality resolved)
- [ ] No duplicate code (tmux ops, question structs unified)
- [ ] All files < 800 lines
- [ ] All tests passing
- [ ] cargo clippy clean
