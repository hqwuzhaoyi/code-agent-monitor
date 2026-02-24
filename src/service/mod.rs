//! Service management for CAM watcher daemon

mod launchd;

pub use launchd::{LaunchdService, ServiceStatus};
