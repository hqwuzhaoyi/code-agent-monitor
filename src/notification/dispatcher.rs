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

    // =========================================================================
    // TDD Tests for Retry Logic with Exponential Backoff
    // =========================================================================
    // These tests define the expected behavior for retry logic on async sends.
    // Failed sends should be retried with exponential backoff.

    /// Retry configuration for async sends
    #[derive(Debug, Clone)]
    pub struct RetryConfig {
        /// Maximum number of retry attempts
        pub max_retries: u32,
        /// Initial backoff duration in milliseconds
        pub initial_backoff_ms: u64,
        /// Maximum backoff duration in milliseconds
        pub max_backoff_ms: u64,
        /// Backoff multiplier (e.g., 2.0 for exponential)
        pub backoff_multiplier: f64,
    }

    impl Default for RetryConfig {
        fn default() -> Self {
            Self {
                max_retries: 3,
                initial_backoff_ms: 100,
                max_backoff_ms: 5000,
                backoff_multiplier: 2.0,
            }
        }
    }

    #[test]
    #[ignore = "TDD: needs implementation of send_async_with_retry() on NotificationChannel"]
    fn test_async_send_retries_on_transient_failure() {
        // When send_async fails with a transient error (network timeout, rate limit),
        // it should automatically retry with exponential backoff.
        //
        // Expected behavior:
        // - First attempt fails -> wait 100ms -> retry
        // - Second attempt fails -> wait 200ms -> retry
        // - Third attempt fails -> wait 400ms -> retry
        // - Fourth attempt fails -> give up, log error
        //
        // Expected method signature:
        //   fn send_async_with_retry(&self, message: &NotificationMessage, config: &RetryConfig) -> Result<()>;
        todo!("Implement send_async_with_retry() with exponential backoff")
    }

    #[test]
    #[ignore = "TDD: needs implementation of send_async_with_retry() on NotificationChannel"]
    fn test_async_send_does_not_retry_on_permanent_failure() {
        // Permanent failures (invalid config, authentication error) should not be retried.
        //
        // Expected behavior:
        // - Detect permanent failure (e.g., 401 Unauthorized, invalid chat_id)
        // - Return immediately without retry
        // - Log the permanent failure for debugging
        todo!("Implement permanent failure detection in retry logic")
    }

    #[test]
    #[ignore = "TDD: needs implementation of send_async_with_retry() on NotificationChannel"]
    fn test_async_send_respects_max_backoff() {
        // Backoff should be capped at max_backoff_ms to prevent excessive delays.
        //
        // With config: initial=100ms, multiplier=2.0, max=500ms
        // - Attempt 1: wait 100ms
        // - Attempt 2: wait 200ms
        // - Attempt 3: wait 400ms
        // - Attempt 4: wait 500ms (capped, not 800ms)
        todo!("Implement max backoff cap in retry logic")
    }

    #[test]
    #[ignore = "TDD: needs implementation of dispatcher retry support"]
    fn test_dispatcher_send_async_with_retry_config() {
        // The dispatcher should support configurable retry behavior.
        //
        // Expected method:
        //   fn with_retry_config(self, config: RetryConfig) -> Self;
        //   fn send_async(&self, message: &NotificationMessage) -> Result<()>;
        //
        // When retry_config is set, send_async should use retry logic.
        todo!("Implement retry config on NotificationDispatcher")
    }

    /// Mock channel that fails N times before succeeding
    struct FailingMockChannel {
        name: String,
        failures_remaining: std::sync::atomic::AtomicU32,
        send_attempts: std::sync::atomic::AtomicU32,
    }

    impl FailingMockChannel {
        fn new(name: &str, fail_count: u32) -> Self {
            Self {
                name: name.to_string(),
                failures_remaining: std::sync::atomic::AtomicU32::new(fail_count),
                send_attempts: std::sync::atomic::AtomicU32::new(0),
            }
        }

        fn get_attempt_count(&self) -> u32 {
            self.send_attempts.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl NotificationChannel for FailingMockChannel {
        fn name(&self) -> &str {
            &self.name
        }

        fn should_send(&self, _message: &NotificationMessage) -> bool {
            true
        }

        fn send(&self, _message: &NotificationMessage) -> Result<SendResult> {
            self.send_attempts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let remaining = self
                .failures_remaining
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            if remaining > 0 {
                Ok(SendResult::Failed("transient error".to_string()))
            } else {
                Ok(SendResult::Sent)
            }
        }

        fn send_async(&self, message: &NotificationMessage) -> Result<()> {
            let _ = self.send(message);
            Ok(())
        }
    }

    #[test]
    #[ignore = "TDD: needs implementation of retry logic"]
    fn test_retry_succeeds_after_transient_failures() {
        // Using FailingMockChannel to verify retry behavior
        let channel = Arc::new(FailingMockChannel::new("test", 2)); // Fail twice, then succeed
        let mut dispatcher = NotificationDispatcher::new();
        dispatcher.register_channel(channel.clone());

        // With retry config allowing 3 retries, this should eventually succeed
        let message = NotificationMessage::new("test", Urgency::High);

        // Expected: 3 attempts total (2 failures + 1 success)
        // dispatcher.send_with_retry(&message, &RetryConfig::default()).unwrap();
        // assert_eq!(channel.get_attempt_count(), 3);

        todo!("Implement and verify retry logic")
    }
}
