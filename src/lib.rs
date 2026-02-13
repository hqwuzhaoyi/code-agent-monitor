//! Code Agent Monitor - 监控和管理 AI 编码代理进程

pub mod ai;
pub mod ai_types;
pub mod cli;
pub mod infra;
pub mod session;
pub mod mcp;
pub mod notify;
#[path = "agent_mod/mod.rs"]
pub mod agent;  // 指向新目录
pub mod openclaw_notifier;
pub mod team;
pub mod task_list;
pub mod conversation_state;
pub mod anthropic;
pub mod notification;
pub mod ai_quality;
pub mod mcp_new;

// Re-exports from infra (backwards compatibility)
pub use infra::{TmuxManager, ProcessScanner};
pub use infra::jsonl::{JsonlParser, JsonlEvent, format_tool_use, extract_tool_target_from_input};
pub use infra::input::{InputWaitDetector, InputWaitResult, InputWaitPattern};

// Re-exports from agent (backwards compatibility)
pub use agent::{AgentManager, AgentRecord, AgentType, AgentStatus, StartAgentRequest, StartAgentResponse};
pub use agent::{AgentWatcher, WatchEvent, AgentSnapshot, format_watch_event};
pub use agent::WatcherDaemon;

pub use session::{SessionManager, SessionFilter};
pub use mcp::McpServer;
pub use notify::{Watcher, Notifier, NotifyEvent};
pub use notification::{NotifyThrottle, ThrottledEvent, MergedNotification};
pub use openclaw_notifier::OpenclawNotifier;
pub use notification::SendResult;
pub use team::{TeamConfig, TeamMember, TeamBridge, InboxMessage, SpecialMessage, AgentId, InboxWatcher, NotifyDecision, TeamOrchestrator, SpawnResult, TeamProgress, discover_teams, get_team_members, get_active_team_members};
pub use task_list::{Task, TaskStatus, list_tasks, get_task, update_task_status, list_team_names};
pub use notification::{NotificationSummarizer, RiskLevel, PermissionSummary, ErrorSummary, CompletionSummary};
pub use conversation_state::{ConversationStateManager, ConversationState, PendingConfirmation, ConfirmationType, AgentContext, ReplyResult};
pub use anthropic::{AnthropicClient, AnthropicConfig, extract_question_with_haiku};
pub use notification::event::{NotificationEvent, NotificationEventType, NotificationEventBuilder};
pub use notification::deduplicator::NotificationDeduplicator;
