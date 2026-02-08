//! Code Agent Monitor - 监控和管理 AI 编码代理进程

pub mod process;
pub mod session;
pub mod mcp;
pub mod notify;
pub mod tmux;
pub mod agent;
pub mod jsonl_parser;
pub mod input_detector;
pub mod agent_watcher;
pub mod throttle;
pub mod openclaw_notifier;
pub mod watcher_daemon;
pub mod team_discovery;
pub mod task_list;

pub use process::ProcessScanner;
pub use session::{SessionManager, SessionFilter};
pub use mcp::McpServer;
pub use notify::{Watcher, Notifier, NotifyEvent};
pub use tmux::TmuxManager;
pub use agent::{AgentManager, AgentRecord, AgentType, AgentStatus, StartAgentRequest, StartAgentResponse};
pub use jsonl_parser::{JsonlParser, JsonlEvent, format_tool_use, extract_tool_target_from_input};
pub use input_detector::{InputWaitDetector, InputWaitResult, InputWaitPattern};
pub use agent_watcher::{AgentWatcher, WatchEvent, AgentSnapshot, format_watch_event};
pub use throttle::{NotifyThrottle, ThrottledEvent, MergedNotification};
pub use openclaw_notifier::{OpenclawNotifier, SendResult};
pub use watcher_daemon::WatcherDaemon;
pub use team_discovery::{TeamConfig, TeamMember, discover_teams, get_team_members, get_active_team_members};
pub use task_list::{Task, TaskStatus, list_tasks, get_task, update_task_status, list_team_names};
