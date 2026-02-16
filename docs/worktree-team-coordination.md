# Multi-Worktree Development Team - Dedup Fix Project

## Team Structure

### Worktrees Created

| Worktree | Branch | Purpose | Agent |
|----------|--------|---------|-------|
| `.worktrees/dedup-fix` | `feature/dedup-fix` | Core implementation | dedup-key-developer |
| `.worktrees/testing` | `feature/testing` | Test suite | test-engineer |
| `.worktrees/review` | `feature/review` | Code review | code-reviewer |
| `.worktrees/docs` | `feature/docs` | Documentation | tech-writer |
| Main repo | `main` | Coordination | team-lead (you) |

### Team Members

1. **plan-reviewer** - Analyzing v2 plan issues and proposing fixes
2. **dedup-key-developer** - Implementing unified dedup key generator
3. **test-engineer** - Writing comprehensive test suite
4. **tech-writer** - Creating documentation
5. **code-reviewer** - Performing code review

## Task Breakdown

### Task #1: Implement Unified Dedup Key Generator
- **Owner**: dedup-key-developer
- **Worktree**: `.worktrees/dedup-fix`
- **Status**: In Progress
- **Deliverable**: `src/notification/dedup_key.rs`

### Task #2: Update Watcher to Use Unified Key
- **Owner**: TBD (blocked by Task #1)
- **Worktree**: `.worktrees/dedup-fix`
- **Status**: Pending
- **Deliverable**: Modified `src/agent_mod/watcher.rs`

### Task #3: Update Hook Path to Use Unified Key
- **Owner**: TBD (blocked by Task #1)
- **Worktree**: `.worktrees/dedup-fix`
- **Status**: Pending
- **Deliverable**: Modified `src/notification/openclaw.rs`

### Task #4: Write Unit Tests
- **Owner**: test-engineer
- **Worktree**: `.worktrees/testing`
- **Status**: In Progress
- **Deliverable**: `tests/dedup_key_tests.rs`

### Task #5: Write Integration Tests
- **Owner**: test-engineer
- **Worktree**: `.worktrees/testing`
- **Status**: In Progress
- **Deliverable**: Updated `tests/integration_test.rs`

### Task #6: Review Code Changes
- **Owner**: code-reviewer
- **Worktree**: `.worktrees/review`
- **Status**: Waiting
- **Deliverable**: `docs/dedup-fix/review-report.md`

### Task #7: Write Documentation
- **Owner**: tech-writer
- **Worktree**: `.worktrees/docs`
- **Status**: In Progress
- **Deliverable**: `docs/dedup-fix/*`

### Task #8: Review v2 Plan Critical Issues
- **Owner**: plan-reviewer
- **Worktree**: Main repo
- **Status**: In Progress
- **Deliverable**: Issue analysis and v3 proposal

## Workflow

```
┌─────────────────────────────────────────────────────────────┐
│                    Parallel Development                      │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  plan-reviewer          dedup-key-developer                 │
│  ├─ Analyze issues      ├─ Implement dedup_key.rs          │
│  └─ Propose v3 fixes    └─ Run unit tests                  │
│                                                              │
│  test-engineer          tech-writer                         │
│  ├─ Write unit tests    ├─ Write README                    │
│  ├─ Write integration   ├─ Write implementation guide      │
│  └─ Run test suite      └─ Write testing guide             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                    Sequential Integration                    │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. Integrate dedup_key.rs into watcher (Task #2)          │
│  2. Integrate dedup_key.rs into hooks (Task #3)            │
│  3. Run full test suite                                     │
│  4. Code review (Task #6)                                   │
│  5. Address review feedback                                 │
│  6. Final verification                                      │
│                                                              │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                    Merge and Deploy                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. Merge feature/dedup-fix → main                          │
│  2. Merge feature/testing → main                            │
│  3. Merge feature/docs → main                               │
│  4. Build release: cargo build --release                    │
│  5. Deploy: cp target/release/cam plugins/cam/bin/cam       │
│  6. Restart services                                        │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Communication Protocol

### Agent → Team Lead
- Use SendMessage with summary of progress
- Report blockers immediately
- Request clarification when needed

### Team Lead → Agents
- Coordinate task dependencies
- Resolve blockers
- Approve/reject proposals

### Agent → Agent
- Minimal direct communication
- Use shared files for coordination
- Team lead mediates conflicts

## Progress Tracking

Check progress:
```bash
cam team-progress worktree-dev-team
```

List tasks:
```bash
# In Claude Code
TaskList
```

## Worktree Management

List all worktrees:
```bash
git worktree list
```

Switch to a worktree:
```bash
cd .worktrees/dedup-fix
cd .worktrees/testing
cd .worktrees/review
cd .worktrees/docs
```

Remove worktree when done:
```bash
git worktree remove .worktrees/dedup-fix
```

## Critical Issues Being Addressed

Based on plan-reviewer's analysis:

### HIGH Priority
1. **Dedup state sync** - Watcher and notifier hold separate in-memory deduplicators
2. **Terminal snapshot timing** - Should use existing snapshot from NotificationEvent

### MEDIUM Priority
3. **Integration point** - openclaw handles NotificationEvent, not WatchEvent
4. **Hook tracking** - Use existing HookEventTracker structure
5. **Test paths** - Use Rust integration tests, not shell scripts

### LOW Priority
6. **Documentation accuracy** - Fix expiry time (2 hours, not 30 minutes)

## Success Criteria

- [ ] All tasks completed
- [ ] All tests passing
- [ ] Code review approved
- [ ] Documentation complete
- [ ] No duplicate notifications in testing
- [ ] Backward compatible
- [ ] Performance acceptable

## Next Steps

1. Wait for agents to complete initial tasks
2. Review plan-reviewer's findings
3. Integrate dedup_key.rs into watcher and hooks
4. Run full test suite
5. Address code review feedback
6. Merge and deploy

## Team Shutdown

When all work is complete:
```bash
# Shutdown team
cam team-shutdown worktree-dev-team

# Or via SendMessage
SendMessage type=shutdown_request to each agent
```
