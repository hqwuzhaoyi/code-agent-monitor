//! Code Agent Monitor - 监控和管理 AI 编码代理进程

pub mod process;
pub mod session;
pub mod mcp;
pub mod notify;
pub mod tmux;
pub mod agent;

pub use process::ProcessScanner;
pub use session::{SessionManager, SessionFilter};
pub use mcp::McpServer;
pub use notify::{Watcher, Notifier, NotifyEvent};
pub use tmux::TmuxManager;
pub use agent::{AgentManager, AgentRecord, AgentType, AgentStatus, StartAgentRequest, StartAgentResponse};
