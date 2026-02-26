//! 通知抽象层 - 统一管理所有通知渠道
//!
//! # 设计目标
//! 1. 统一接口：所有渠道实现 `NotificationChannel` trait
//! 2. 渠道解耦：每个渠道独立实现，互不影响
//! 3. 灵活路由：`NotificationDispatcher` 根据配置决定发送到哪些渠道
//! 4. 异步发送：所有渠道支持异步发送，不阻塞调用方
//!
//! # 使用示例
//! ```ignore
//! use code_agent_monitor::notification::{NotificationBuilder, NotificationMessage, Urgency};
//!
//! let dispatcher = NotificationBuilder::new()
//!     .min_urgency(Urgency::Medium)
//!     .build()?;
//!
//! let message = NotificationMessage::new("Hello", Urgency::High);
//! dispatcher.send_sync(&message)?;
//! ```

pub mod builder;
pub mod channel;
pub mod channels;
pub mod dedup_key;
pub mod deduplicator;
pub mod dispatcher;
pub mod event;
pub mod openclaw;
pub mod payload;
pub mod store;
pub mod summarizer;
pub mod system_event;
pub mod terminal_cleaner;
pub mod throttle;
pub mod urgency;
pub mod watcher;
pub mod webhook;

#[cfg(test)]
mod system_event_test;

pub use builder::NotificationBuilder;
pub use channel::{MessageMetadata, NotificationChannel, NotificationMessage, SendResult};
pub use dedup_key::{generate_dedup_key, normalize_terminal_content};
pub use deduplicator::{NotificationDeduplicator, NotifyAction};
pub use dispatcher::NotificationDispatcher;
pub use event::{NotificationEvent, NotificationEventBuilder, NotificationEventType};
pub use openclaw::OpenclawNotifier;
pub use payload::PayloadBuilder;
pub use store::{NotificationRecord, NotificationStore};
pub use summarizer::{
    CompletionSummary, ErrorSummary, NotificationSummarizer, PermissionSummary, RiskLevel,
};
pub use system_event::SystemEventPayload;
pub use terminal_cleaner::is_processing;
pub use throttle::{MergedNotification, NotifyThrottle, ThrottledEvent};
pub use urgency::{get_urgency, Urgency};
pub use watcher::{Notifier, NotifyEvent, Watcher};
pub use webhook::{
    load_webhook_config_from_file, WebhookClient, WebhookConfig, WebhookPayload, WebhookResponse,
};
