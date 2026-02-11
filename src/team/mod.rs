//! Team 模块 - Agent Teams 编排和管理
//!
//! 提供 Agent Teams 的创建、管理、消息传递和任务编排功能。
//!
//! ## 子模块
//!
//! - `discovery` - Team 配置发现和成员管理
//! - `bridge` - Team 文件系统操作（创建/删除/inbox 读写）
//! - `orchestrator` - Agent 编排和任务分配
//! - `inbox_watcher` - Inbox 目录监控和通知触发
//!
//! ## 数据存储
//!
//! Team 数据存储在 `~/.claude/teams/{team-name}/` 目录：
//! - `config.json` - Team 配置和成员列表
//! - `inboxes/{member-name}.json` - 成员 inbox 消息

pub mod discovery;
pub mod bridge;
pub mod orchestrator;
pub mod inbox_watcher;

// Re-export commonly used types
pub use discovery::{TeamConfig, TeamMember, discover_teams, get_team_members, get_active_team_members};
pub use bridge::{TeamBridge, InboxMessage, SpecialMessage, AgentId};
pub use orchestrator::{TeamOrchestrator, SpawnResult, TeamProgress};
pub use inbox_watcher::{InboxWatcher, Urgency, NotifyDecision};
