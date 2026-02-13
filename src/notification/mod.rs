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

pub mod channel;
pub mod dispatcher;
pub mod channels;
pub mod builder;
pub mod urgency;
pub mod payload;
pub mod terminal_cleaner;
pub mod formatter;
pub mod deduplicator;
pub mod event;
pub mod summarizer;
pub mod throttle;

pub use channel::{NotificationChannel, NotificationMessage, SendResult, MessageMetadata};
pub use dispatcher::NotificationDispatcher;
pub use builder::NotificationBuilder;
pub use urgency::{Urgency, get_urgency};
pub use payload::PayloadBuilder;
pub use terminal_cleaner::is_processing;
pub use formatter::{MessageFormatter, msg};
pub use deduplicator::NotificationDeduplicator;
pub use event::{NotificationEvent, NotificationEventType, NotificationEventBuilder};
pub use summarizer::{NotificationSummarizer, RiskLevel, PermissionSummary, ErrorSummary, CompletionSummary};
pub use throttle::{NotifyThrottle, ThrottledEvent, MergedNotification};
