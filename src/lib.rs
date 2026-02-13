//! Code Agent Monitor - 监控和管理 AI 编码代理进程

pub mod ai;
pub mod ai_types;
pub mod cli;
pub mod infra;  // 新增
pub mod session;
pub mod mcp;
pub mod notify;
pub mod agent;
pub mod agent_watcher;
pub mod openclaw_notifier;
pub mod watcher_daemon;
pub mod team;
pub mod task_list;
pub mod conversation_state;
pub mod anthropic;
pub mod notification;
pub mod ai_quality;
pub mod watcher;
pub mod mcp_new;

// Re-exports from infra (backwards compatibility)
pub use infra::{TmuxManager, ProcessScanner};
pub use infra::jsonl::{JsonlParser, JsonlEvent, format_tool_use, extract_tool_target_from_input};
pub use infra::input::{InputWaitDetector, InputWaitResult, InputWaitPattern};

pub use session::{SessionManager, SessionFilter};
pub use mcp::McpServer;
pub use notify::{Watcher, Notifier, NotifyEvent};
pub use agent::{AgentManager, AgentRecord, AgentType, AgentStatus, StartAgentRequest, StartAgentResponse};
pub use agent_watcher::{AgentWatcher, WatchEvent, AgentSnapshot, format_watch_event};
pub use notification::{NotifyThrottle, ThrottledEvent, MergedNotification};
pub use openclaw_notifier::OpenclawNotifier;
pub use notification::SendResult;
pub use watcher_daemon::WatcherDaemon;
pub use team::{TeamConfig, TeamMember, TeamBridge, InboxMessage, SpecialMessage, AgentId, InboxWatcher, NotifyDecision, TeamOrchestrator, SpawnResult, TeamProgress, discover_teams, get_team_members, get_active_team_members};
pub use task_list::{Task, TaskStatus, list_tasks, get_task, update_task_status, list_team_names};
pub use notification::{NotificationSummarizer, RiskLevel, PermissionSummary, ErrorSummary, CompletionSummary};
pub use conversation_state::{ConversationStateManager, ConversationState, PendingConfirmation, ConfirmationType, AgentContext, ReplyResult};
pub use anthropic::{AnthropicClient, AnthropicConfig, extract_question_with_haiku};
pub use notification::event::{NotificationEvent, NotificationEventType, NotificationEventBuilder};
pub use notification::deduplicator::NotificationDeduplicator;
