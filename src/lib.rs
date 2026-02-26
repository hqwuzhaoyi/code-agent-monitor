//! Code Agent Monitor - 监控和管理 AI 编码代理进程

#[path = "agent_mod/mod.rs"]
pub mod agent;
pub mod ai;
pub mod anthropic;
pub mod cli;
pub mod infra;
#[path = "mcp_mod/mod.rs"]
pub mod mcp;
pub mod notification;
pub mod service;
#[path = "session_mod/mod.rs"]
pub mod session;
pub mod team;
pub mod tui;

// Re-exports from infra (backwards compatibility)
pub use infra::input::{InputWaitDetector, InputWaitPattern, InputWaitResult};
pub use infra::jsonl::{extract_tool_target_from_input, format_tool_use, JsonlEvent, JsonlParser};
pub use infra::{truncate_str, ProcessScanner, TmuxManager};

// Re-exports from agent (backwards compatibility)
pub use agent::WatcherDaemon;
pub use agent::{format_watch_event, AgentSnapshot, AgentWatcher, WatchEvent};
pub use agent::{
    AgentManager, AgentRecord, AgentStatus, AgentType, StartAgentRequest, StartAgentResponse,
};

// Re-exports from session (backwards compatibility)
pub use session::{
    AgentContext, BatchFilter, BatchReplyResult, ConfirmationType, ConversationState,
    ConversationStateManager, PendingConfirmation, ReplyResult,
};
pub use session::{SessionFilter, SessionManager};

// Re-exports from mcp (backwards compatibility)
pub use mcp::McpServer;
pub use mcp::{McpError, McpRequest, McpResponse, McpTool};

// Re-exports from notification (backwards compatibility)
pub use notification::deduplicator::NotificationDeduplicator;
pub use notification::event::{NotificationEvent, NotificationEventBuilder, NotificationEventType};
pub use notification::OpenclawNotifier;
pub use notification::SendResult;
pub use notification::{
    CompletionSummary, ErrorSummary, NotificationSummarizer, PermissionSummary, RiskLevel,
};
pub use notification::{MergedNotification, NotifyThrottle, ThrottledEvent};
pub use notification::{Notifier, NotifyEvent, Watcher};

// Re-exports from team (backwards compatibility)
pub use team::{
    discover_teams, get_active_team_members, get_team_members, AgentId, InboxMessage, InboxWatcher,
    NotifyDecision, SpawnResult, SpecialMessage, TeamBridge, TeamConfig, TeamMember,
    TeamOrchestrator, TeamProgress,
};
pub use team::{get_task, list_tasks, list_team_names, update_task_status, Task, TaskStatus};

pub use anthropic::{extract_question_with_haiku, AnthropicClient, AnthropicConfig};

// Re-exports from service
pub use service::{LaunchdService, ServiceStatus};
