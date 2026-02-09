//! 通知分发器 - 管理多个渠道并路由消息

use super::channel::{NotificationChannel, NotificationMessage, SendResult};
use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn};

/// 通知分发器 - 管理多个渠道并路由消息
pub struct NotificationDispatcher {
    /// 所有注册的渠道
    channels: Vec<Arc<dyn NotificationChannel>>,
    /// 是否为 dry-run 模式
    dry_run: bool,
}

impl NotificationDispatcher {
    /// 创建新的分发器
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
            dry_run: false,
        }
    }

    /// 设置 dry-run 模式
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// 注册渠道
    pub fn register_channel(&mut self, channel: Arc<dyn NotificationChannel>) {
        info!(channel = channel.name(), "Registering notification channel");
        self.channels.push(channel);
    }

    /// 同步发送消息到所有渠道
    pub fn send_sync(&self, message: &NotificationMessage) -> Result<Vec<(String, SendResult)>> {
        let mut results = Vec::new();

        for channel in &self.channels {
            let name = channel.name().to_string();

            if self.dry_run {
                eprintln!("[DRY-RUN] Would send to channel: {}", name);
                results.push((name, SendResult::Skipped("dry-run".to_string())));
                continue;
            }

            let result = match channel.send(message) {
                Ok(r) => r,
                Err(e) => {
                    warn!(channel = %name, error = %e, "Channel send failed");
                    SendResult::Failed(e.to_string())
                }
            };

            results.push((name, result));
        }

        Ok(results)
    }

    /// 异步发送消息到所有渠道（spawn 后立即返回）
    pub fn send_async(&self, message: &NotificationMessage) -> Result<()> {
        for channel in &self.channels {
            if self.dry_run {
                eprintln!("[DRY-RUN] Would send async to channel: {}", channel.name());
                continue;
            }

            if let Err(e) = channel.send_async(message) {
                warn!(channel = channel.name(), error = %e, "Channel async send failed");
            }
        }

        Ok(())
    }

    /// 获取已注册的渠道数量
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// 获取已注册的渠道名称
    pub fn channel_names(&self) -> Vec<&str> {
        self.channels.iter().map(|c| c.name()).collect()
    }
}

impl Default for NotificationDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::urgency::Urgency;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 测试用的 mock 渠道
    struct MockChannel {
        name: String,
        send_count: AtomicUsize,
    }

    impl MockChannel {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                send_count: AtomicUsize::new(0),
            }
        }

        fn get_send_count(&self) -> usize {
            self.send_count.load(Ordering::SeqCst)
        }
    }

    impl NotificationChannel for MockChannel {
        fn name(&self) -> &str {
            &self.name
        }

        fn should_send(&self, _message: &NotificationMessage) -> bool {
            true
        }

        fn send(&self, _message: &NotificationMessage) -> Result<SendResult> {
            self.send_count.fetch_add(1, Ordering::SeqCst);
            Ok(SendResult::Sent)
        }

        fn send_async(&self, _message: &NotificationMessage) -> Result<()> {
            self.send_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn test_dispatcher_register_channel() {
        let mut dispatcher = NotificationDispatcher::new();
        assert_eq!(dispatcher.channel_count(), 0);

        dispatcher.register_channel(Arc::new(MockChannel::new("test")));
        assert_eq!(dispatcher.channel_count(), 1);
        assert_eq!(dispatcher.channel_names(), vec!["test"]);
    }

    #[test]
    fn test_dispatcher_send_sync() {
        let mut dispatcher = NotificationDispatcher::new();
        let channel = Arc::new(MockChannel::new("test"));
        dispatcher.register_channel(channel.clone());

        let message = NotificationMessage::new("test", Urgency::High);
        let results = dispatcher.send_sync(&message).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "test");
        assert_eq!(results[0].1, SendResult::Sent);
        assert_eq!(channel.get_send_count(), 1);
    }

    #[test]
    fn test_dispatcher_dry_run() {
        let mut dispatcher = NotificationDispatcher::new().with_dry_run(true);
        let channel = Arc::new(MockChannel::new("test"));
        dispatcher.register_channel(channel.clone());

        let message = NotificationMessage::new("test", Urgency::High);
        let results = dispatcher.send_sync(&message).unwrap();

        assert_eq!(results[0].1, SendResult::Skipped("dry-run".to_string()));
        assert_eq!(channel.get_send_count(), 0); // 不应该实际发送
    }
}
