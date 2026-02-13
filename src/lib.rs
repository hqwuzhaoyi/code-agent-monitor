//! Code Agent Monitor - 监控和管理 AI 编码代理进程

pub mod ai;
pub mod cli;
pub mod infra;
#[path = "session_mod/mod.rs"]
pub mod session;
#[path = "mcp_mod/mod.rs"]
pub mod mcp;
#[path = "agent_mod/mod.rs"]
pub mod agent;
pub mod team;
pub mod task_list;
pub mod anthropic;
pub mod notification;

// Re-exports from infra (backwards compatibility)
pub use infra::{TmuxManager, ProcessScanner};
pub use infra::jsonl::{JsonlParser, JsonlEvent, format_tool_use, extract_tool_target_from_input};
pub use infra::input::{InputWaitDetector, InputWaitResult, InputWaitPattern};

// Re-exports from agent (backwards compatibility)
pub use agent::{AgentManager, AgentRecord, AgentType, AgentStatus, StartAgentRequest, StartAgentResponse};
pub use agent::{AgentWatcher, WatchEvent, AgentSnapshot, format_watch_event};
pub use agent::WatcherDaemon;

// Re-exports from session (backwards compatibility)
pub use session::{SessionManager, SessionFilter};
pub use session::{ConversationStateManager, ConversationState, PendingConfirmation, ConfirmationType, AgentContext, ReplyResult};

// Re-exports from mcp (backwards compatibility)
pub use mcp::McpServer;
pub use mcp::{McpError, McpRequest, McpResponse, McpTool};

// Re-exports from notification (backwards compatibility)
pub use notification::{Watcher, Notifier, NotifyEvent};
pub use notification::{NotifyThrottle, ThrottledEvent, MergedNotification};
pub use notification::OpenclawNotifier;
pub use notification::SendResult;
pub use notification::{NotificationSummarizer, RiskLevel, PermissionSummary, ErrorSummary, CompletionSummary};
pub use notification::event::{NotificationEvent, NotificationEventType, NotificationEventBuilder};
pub use notification::deduplicator::NotificationDeduplicator;

pub use team::{TeamConfig, TeamMember, TeamBridge, InboxMessage, SpecialMessage, AgentId, InboxWatcher, NotifyDecision, TeamOrchestrator, SpawnResult, TeamProgress, discover_teams, get_team_members, get_active_team_members};
pub use task_list::{Task, TaskStatus, list_tasks, get_task, update_task_status, list_team_names};
pub use anthropic::{AnthropicClient, AnthropicConfig, extract_question_with_haiku};
