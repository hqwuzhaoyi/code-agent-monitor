//! Agent 生命周期管理 - 启动、监控、停止

pub mod adapter;
pub mod daemon;
pub mod event_processor;
pub mod extractor;
pub mod manager;
pub mod monitor;
pub mod stability;
pub mod watcher;

pub use daemon::WatcherDaemon;
pub use event_processor::EventProcessor;
pub use extractor::{
    extract_message_from_snapshot, ExtractedMessage, ExtractionResult, HaikuExtractor,
    IterationConfig, MessageType, ReactExtractor,
};
pub use manager::{
    AgentManager, AgentRecord, AgentStatus, AgentType, StartAgentRequest, StartAgentResponse,
};
pub use monitor::AgentMonitor;
pub use stability::{StabilityDetector, StabilityState};
pub use watcher::{format_watch_event, AgentSnapshot, AgentWatcher, WatchEvent};

// Adapter exports
pub use adapter::{
    get_adapter, AgentAdapter, AgentCapabilities, AgentPaths, DetectionStrategy, HookEvent,
};
