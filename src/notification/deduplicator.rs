//! 通知去重器 - 防止短时间内发送重复通知
//!
//! 当 Hook 和 Watcher 同时检测到同一状态时，可能会产生重复通知。
//! 此模块通过内容哈希和时间窗口实现去重。

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

/// 通知去重器
pub struct NotificationDeduplicator {
    /// 最近发送的通知: agent_id -> (content_hash, timestamp)
    recent: HashMap<String, (u64, Instant)>,
    /// 去重窗口（默认 30 秒）
    window: Duration,
}

impl NotificationDeduplicator {
    /// 创建新的去重器，使用默认 30 秒窗口
    pub fn new() -> Self {
        Self {
            recent: HashMap::new(),
            window: Duration::from_secs(30),
        }
    }

    /// 设置去重窗口时长
    pub fn with_window(mut self, window: Duration) -> Self {
        self.window = window;
        self
    }

    /// 检查是否应该发送通知
    ///
    /// 返回 `true` 表示应该发送，`false` 表示应该去重跳过
    ///
    /// # 去重规则
    /// - 同一 agent_id 在窗口内发送相同内容会被去重
    /// - 不同 agent_id 的相同内容不会被去重
    /// - 窗口过期后可以重新发送
    pub fn should_send(&mut self, agent_id: &str, content: &str) -> bool {
        let hash = Self::hash_content(content);
        let now = Instant::now();

        // 清理过期记录
        self.cleanup_expired(now);

        if let Some((prev_hash, prev_time)) = self.recent.get(agent_id) {
            if *prev_hash == hash && now.duration_since(*prev_time) < self.window {
                return false; // 去重
            }
        }

        self.recent.insert(agent_id.to_string(), (hash, now));
        true
    }

    /// 计算内容哈希
    fn hash_content(content: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    /// 清理过期记录
    fn cleanup_expired(&mut self, now: Instant) {
        self.recent
            .retain(|_, (_, time)| now.duration_since(*time) < self.window);
    }
}

impl Default for NotificationDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_same_content_within_window_is_deduplicated() {
        let mut dedup = NotificationDeduplicator::new();
        let agent_id = "cam-test";
        let content = "等待确认: rm -rf /tmp/test";

        // 第一次应该发送
        assert!(dedup.should_send(agent_id, content));
        // 第二次相同内容应该被去重
        assert!(!dedup.should_send(agent_id, content));
        // 第三次仍然被去重
        assert!(!dedup.should_send(agent_id, content));
    }

    #[test]
    fn test_different_content_not_deduplicated() {
        let mut dedup = NotificationDeduplicator::new();
        let agent_id = "cam-test";

        assert!(dedup.should_send(agent_id, "内容 A"));
        assert!(dedup.should_send(agent_id, "内容 B"));
        assert!(dedup.should_send(agent_id, "内容 C"));
    }

    #[test]
    fn test_window_expiry_allows_resend() {
        // 使用 100ms 的短窗口便于测试
        let mut dedup = NotificationDeduplicator::new().with_window(Duration::from_millis(100));
        let agent_id = "cam-test";
        let content = "等待确认";

        // 第一次发送
        assert!(dedup.should_send(agent_id, content));
        // 窗口内被去重
        assert!(!dedup.should_send(agent_id, content));

        // 等待窗口过期
        sleep(Duration::from_millis(150));

        // 窗口过期后可以重新发送
        assert!(dedup.should_send(agent_id, content));
    }

    #[test]
    fn test_different_agents_same_content_not_deduplicated() {
        let mut dedup = NotificationDeduplicator::new();
        let content = "相同的通知内容";

        // 不同 agent 的相同内容应该都能发送
        assert!(dedup.should_send("cam-agent-1", content));
        assert!(dedup.should_send("cam-agent-2", content));
        assert!(dedup.should_send("cam-agent-3", content));

        // 但同一 agent 的相同内容会被去重
        assert!(!dedup.should_send("cam-agent-1", content));
        assert!(!dedup.should_send("cam-agent-2", content));
    }

    #[test]
    fn test_cleanup_expired_records() {
        let mut dedup = NotificationDeduplicator::new().with_window(Duration::from_millis(50));

        // 添加多个记录
        dedup.should_send("agent-1", "content-1");
        dedup.should_send("agent-2", "content-2");
        dedup.should_send("agent-3", "content-3");

        // 等待过期
        sleep(Duration::from_millis(100));

        // 触发清理（通过调用 should_send）
        dedup.should_send("agent-new", "new-content");

        // 验证旧记录已被清理（可以重新发送）
        assert!(dedup.should_send("agent-1", "content-1"));
        assert!(dedup.should_send("agent-2", "content-2"));
    }

    #[test]
    fn test_default_window_is_30_seconds() {
        let dedup = NotificationDeduplicator::new();
        assert_eq!(dedup.window, Duration::from_secs(30));
    }

    #[test]
    fn test_custom_window() {
        let dedup = NotificationDeduplicator::new().with_window(Duration::from_secs(60));
        assert_eq!(dedup.window, Duration::from_secs(60));
    }
}
