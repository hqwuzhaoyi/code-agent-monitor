//! Agent 生命周期管理 - 启动、监控、停止

pub mod manager;
pub mod watcher;
pub mod daemon;
pub mod monitor;
pub mod event_processor;
pub mod stability;
pub mod adapter;

pub use manager::{AgentManager, AgentRecord, AgentType, AgentStatus, StartAgentRequest, StartAgentResponse};
pub use watcher::{AgentWatcher, WatchEvent, AgentSnapshot, format_watch_event};
pub use daemon::WatcherDaemon;
pub use monitor::AgentMonitor;
pub use event_processor::EventProcessor;
pub use stability::{StabilityState, StabilityDetector};

// Adapter exports
pub use adapter::{
    AgentAdapter, AgentCapabilities, AgentPaths, DetectionStrategy, HookEvent, get_adapter,
};
