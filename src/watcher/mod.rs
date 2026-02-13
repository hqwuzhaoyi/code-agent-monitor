//! Agent watching subsystem - monitors agent state and processes events

mod agent_monitor;
mod event_processor;
mod stability;

pub use agent_monitor::AgentMonitor;
pub use event_processor::EventProcessor;
pub use stability::{StabilityState, StabilityDetector};
