# CAM 领域重构设计

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:writing-plans to create implementation plan after design approval.

**Goal:** 按功能域重构 src/ 目录结构，对标经典 Rust 项目（eza、starship）

**Architecture:** 6 个功能域 + 1 个基础设施层，根目录只保留 lib.rs 和 main.rs

**Tech Stack:** Rust, cargo, git

---

## 当前问题

- src/ 根目录 19 个 .rs 文件，过于扁平
- 相关功能分散（如 agent.rs + agent_watcher.rs + watcher_daemon.rs）
- 与经典 Rust 项目结构差距大

## 目标结构

```
src/
├── lib.rs              # 库入口，模块导出
├── main.rs             # CLI 入口
│
├── agent/              # Agent 生命周期管理
│   ├── mod.rs          # 导出
│   ├── manager.rs      ← agent.rs (AgentManager, AgentRecord)
│   ├── watcher.rs      ← agent_watcher.rs (AgentWatcher)
│   └── daemon.rs       ← watcher_daemon.rs (WatcherDaemon)
│
├── session/            # 会话管理
│   ├── mod.rs          # 导出
│   ├── manager.rs      ← session.rs (SessionManager)
│   └── state.rs        ← conversation_state.rs (ConversationStateManager)
│
├── notification/       # 通知系统（已有，保持）
│   ├── mod.rs
│   ├── notifier.rs     ← openclaw_notifier.rs (移入)
│   ├── formatter.rs
│   ├── deduplicator.rs
│   ├── summarizer.rs
│   ├── throttle.rs
│   ├── event.rs
│   ├── payload.rs
│   ├── urgency.rs
│   ├── channel.rs
│   ├── dispatcher.rs
│   ├── builder.rs
│   ├── terminal_cleaner.rs
│   └── channels/
│
├── team/               # Team 编排（已有，保持）
│   ├── mod.rs
│   ├── bridge.rs
│   ├── discovery.rs
│   ├── orchestrator.rs
│   ├── inbox_watcher.rs
│   └── task.rs         ← task_list.rs (移入)
│
├── ai/                 # AI 集成（已有，扩展）
│   ├── mod.rs
│   ├── client.rs
│   ├── extractor.rs
│   ├── quality.rs      ← ai_quality.rs (移入)
│   └── types.rs        ← ai_types.rs (移入)
│
├── mcp/                # MCP Server（合并）
│   ├── mod.rs          ← mcp.rs 主逻辑
│   ├── types.rs        ← mcp_new/types.rs
│   └── tools/          ← mcp_new/tools/
│       ├── mod.rs
│       ├── agent.rs
│       ├── session.rs
│       ├── team.rs
│       └── task.rs
│
├── cli/                # CLI 命令（扩展）
│   ├── mod.rs
│   ├── output.rs
│   └── commands/       # 从 main.rs 拆出
│       ├── mod.rs
│       ├── list.rs
│       ├── session.rs
│       ├── agent.rs
│       ├── team.rs
│       ├── notify.rs
│       └── daemon.rs
│
└── infra/              # 基础设施
    ├── mod.rs
    ├── tmux.rs         ← tmux.rs
    ├── process.rs      ← process.rs
    ├── terminal.rs     ← terminal_utils.rs
    ├── jsonl.rs        ← jsonl_parser.rs
    └── input.rs        ← input_detector.rs
```

## 文件迁移清单

| 原位置 | 新位置 | 说明 |
|--------|--------|------|
| src/agent.rs | src/agent/manager.rs | Agent 管理 |
| src/agent_watcher.rs | src/agent/watcher.rs | Agent 监控 |
| src/watcher_daemon.rs | src/agent/daemon.rs | 后台守护 |
| src/session.rs | src/session/manager.rs | 会话管理 |
| src/conversation_state.rs | src/session/state.rs | 对话状态 |
| src/openclaw_notifier.rs | src/notification/notifier.rs | 通知门面 |
| src/task_list.rs | src/team/task.rs | 任务列表 |
| src/ai_types.rs | src/ai/types.rs | AI 类型 |
| src/ai_quality.rs | src/ai/quality.rs | AI 质量 |
| src/mcp.rs | src/mcp/mod.rs | MCP 主逻辑 |
| src/mcp_new/* | src/mcp/* | 合并 |
| src/tmux.rs | src/infra/tmux.rs | tmux 操作 |
| src/process.rs | src/infra/process.rs | 进程扫描 |
| src/terminal_utils.rs | src/infra/terminal.rs | 终端工具 |
| src/jsonl_parser.rs | src/infra/jsonl.rs | JSONL 解析 |
| src/input_detector.rs | src/infra/input.rs | 输入检测 |
| src/anthropic.rs | 删除 | 已迁移到 ai/ |
| src/notify.rs | 删除 | 旧版，已废弃 |
| src/watcher/* | 删除 | 合并到 agent/ |

## 删除/合并清单

| 文件 | 处理 | 原因 |
|------|------|------|
| src/anthropic.rs | 删除 | 功能已在 ai/client.rs + ai/extractor.rs |
| src/notify.rs | 删除 | 旧版通知，已被 notification/ 替代 |
| src/watcher/ | 合并到 agent/ | 功能重叠 |
| src/mcp_new/ | 合并到 mcp/ | 统一 MCP 模块 |

## 向后兼容

通过 lib.rs 的 re-export 保持 API 兼容：

```rust
// src/lib.rs
pub mod agent;
pub mod session;
pub mod notification;
pub mod team;
pub mod ai;
pub mod mcp;
pub mod cli;
pub mod infra;

// Re-exports for backwards compatibility
pub use agent::{AgentManager, AgentRecord, AgentWatcher, WatcherDaemon};
pub use session::{SessionManager, ConversationStateManager};
pub use notification::OpenclawNotifier;
// ... 其他 re-export
```

## 实施优先级

| 阶段 | 任务 | 风险 |
|------|------|------|
| P0 | 创建 infra/ 模块 | 低 |
| P1 | 创建 agent/ 模块 | 中 |
| P2 | 创建 session/ 模块 | 低 |
| P3 | 整合 ai/ 模块 | 低 |
| P4 | 整合 mcp/ 模块 | 中 |
| P5 | 整合 notification/ | 低 |
| P6 | 整合 team/ | 低 |
| P7 | 拆分 cli/commands/ | 高 |
| P8 | 清理废弃文件 | 低 |

## 成功标准

- [ ] src/ 根目录只有 lib.rs 和 main.rs
- [ ] 所有模块按功能域组织
- [ ] cargo build --release 通过
- [ ] cargo test 通过
- [ ] 向后兼容（re-export 保持）
