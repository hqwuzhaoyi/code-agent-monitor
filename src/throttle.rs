//! 通知限流模块 - 通知去重和合并

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 通知事件
#[derive(Debug, Clone)]
pub enum ThrottledEvent {
    /// 工具调用
    ToolUse {
        agent_id: String,
        tool: String,
        target: Option<String>,
    },
    /// 错误
    Error {
        agent_id: String,
        message: String,
    },
    /// 等待输入
    WaitingForInput {
        agent_id: String,
        context: String,
    },
}

/// 合并后的通知
#[derive(Debug, Clone)]
pub struct MergedNotification {
    /// 通知消息
    pub message: String,
    /// 事件数量
    pub event_count: usize,
    /// 时间戳
    pub timestamp: Instant,
}

/// 通知限流器
pub struct NotifyThrottle {
    /// 工具调用合并窗口（秒）
    tool_merge_window: Duration,
    /// 错误去重窗口（秒）
    error_dedupe_window: Duration,
    /// 等待输入防抖窗口（秒）
    input_wait_debounce: Duration,
    /// 待处理的工具调用
    pending_tools: HashMap<String, Vec<(String, Option<String>, Instant)>>,
    /// 最近的错误（用于去重）
    recent_errors: HashMap<String, Instant>,
    /// 最近的等待输入通知
    recent_input_waits: HashMap<String, Instant>,
}

impl NotifyThrottle {
    /// 创建新的限流器
    pub fn new() -> Self {
        Self {
            tool_merge_window: Duration::from_secs(3),
            error_dedupe_window: Duration::from_secs(300), // 5 分钟
            input_wait_debounce: Duration::from_secs(10),
            pending_tools: HashMap::new(),
            recent_errors: HashMap::new(),
            recent_input_waits: HashMap::new(),
        }
    }

    /// 创建带自定义窗口的限流器
    pub fn with_windows(
        tool_merge_window: Duration,
        error_dedupe_window: Duration,
        input_wait_debounce: Duration,
    ) -> Self {
        Self {
            tool_merge_window,
            error_dedupe_window,
            input_wait_debounce,
            pending_tools: HashMap::new(),
            recent_errors: HashMap::new(),
            recent_input_waits: HashMap::new(),
        }
    }

    /// 推送事件
    pub fn push(&mut self, event: ThrottledEvent) {
        self.push_with_time(event, Instant::now());
    }

    /// 推送事件（带时间戳，用于测试）
    pub fn push_with_time(&mut self, event: ThrottledEvent, time: Instant) {
        match event {
            ThrottledEvent::ToolUse { agent_id, tool, target } => {
                self.pending_tools
                    .entry(agent_id)
                    .or_insert_with(Vec::new)
                    .push((tool, target, time));
            }
            ThrottledEvent::Error { agent_id, message } => {
                let key = format!("{}:{}", agent_id, message);
                self.recent_errors.insert(key, time);
            }
            ThrottledEvent::WaitingForInput { agent_id, context: _ } => {
                self.recent_input_waits.insert(agent_id, time);
            }
        }
    }

    /// 刷新并获取合并后的通知
    pub fn flush(&mut self) -> Vec<MergedNotification> {
        let now = Instant::now();
        let mut notifications = Vec::new();

        // 处理工具调用合并
        let agent_ids: Vec<String> = self.pending_tools.keys().cloned().collect();
        for agent_id in agent_ids {
            if let Some(tools) = self.pending_tools.get(&agent_id) {
                // 检查是否超过合并窗口
                if let Some(first) = tools.first() {
                    if now.duration_since(first.2) >= self.tool_merge_window {
                        // 合并工具调用
                        let tools_list = self.pending_tools.remove(&agent_id).unwrap();
                        let formatted: Vec<String> = tools_list.iter()
                            .map(|(tool, target, _)| {
                                if let Some(t) = target {
                                    format!("{} {}", tool, t)
                                } else {
                                    tool.clone()
                                }
                            })
                            .collect();

                        let message = format!("🔧 {} 执行: {}", agent_id, formatted.join(", "));
                        notifications.push(MergedNotification {
                            message,
                            event_count: formatted.len(),
                            timestamp: now,
                        });
                    }
                }
            }
        }

        notifications
    }

    /// 检查错误是否应该被去重
    pub fn should_dedupe_error(&self, agent_id: &str, message: &str) -> bool {
        let key = format!("{}:{}", agent_id, message);
        if let Some(last_time) = self.recent_errors.get(&key) {
            Instant::now().duration_since(*last_time) < self.error_dedupe_window
        } else {
            false
        }
    }

    /// 检查等待输入通知是否应该被防抖
    pub fn should_debounce_input_wait(&self, agent_id: &str) -> bool {
        if let Some(last_time) = self.recent_input_waits.get(agent_id) {
            Instant::now().duration_since(*last_time) < self.input_wait_debounce
        } else {
            false
        }
    }

    /// 记录错误（用于去重）
    pub fn record_error(&mut self, agent_id: &str, message: &str) {
        let key = format!("{}:{}", agent_id, message);
        self.recent_errors.insert(key, Instant::now());
    }

    /// 记录等待输入通知（用于防抖）
    pub fn record_input_wait(&mut self, agent_id: &str) {
        self.recent_input_waits.insert(agent_id.to_string(), Instant::now());
    }

    /// 清理过期的记录
    pub fn cleanup(&mut self) {
        let now = Instant::now();

        // 清理过期的错误记录
        self.recent_errors.retain(|_, time| {
            now.duration_since(*time) < self.error_dedupe_window
        });

        // 清理过期的等待输入记录
        self.recent_input_waits.retain(|_, time| {
            now.duration_since(*time) < self.input_wait_debounce
        });
    }

    /// 清除指定 agent 的所有状态
    pub fn clear_agent(&mut self, agent_id: &str) {
        self.pending_tools.remove(agent_id);
        self.recent_input_waits.remove(agent_id);

        // 清除该 agent 的错误记录
        let prefix = format!("{}:", agent_id);
        self.recent_errors.retain(|key, _| !key.starts_with(&prefix));
    }
}

impl Default for NotifyThrottle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_consecutive_tool_calls() {
        let mut throttle = NotifyThrottle::with_windows(
            Duration::from_millis(100),
            Duration::from_secs(300),
            Duration::from_secs(10),
        );

        // 推送 3 个工具调用
        throttle.push(ThrottledEvent::ToolUse {
            agent_id: "cam-123".to_string(),
            tool: "Edit".to_string(),
            target: Some("a.rs".to_string()),
        });
        throttle.push(ThrottledEvent::ToolUse {
            agent_id: "cam-123".to_string(),
            tool: "Edit".to_string(),
            target: Some("b.rs".to_string()),
        });
        throttle.push(ThrottledEvent::ToolUse {
            agent_id: "cam-123".to_string(),
            tool: "Read".to_string(),
            target: Some("c.rs".to_string()),
        });

        // 等待合并窗口
        std::thread::sleep(Duration::from_millis(150));

        // 刷新
        let events = throttle.flush();

        // 应该合并为一条
        assert_eq!(events.len(), 1);
        assert!(events[0].message.contains("Edit a.rs"));
        assert!(events[0].message.contains("Edit b.rs"));
        assert!(events[0].message.contains("Read c.rs"));
        assert_eq!(events[0].event_count, 3);
    }

    #[test]
    fn test_dedupe_same_error() {
        let mut throttle = NotifyThrottle::new();

        // 记录第一个错误
        throttle.record_error("cam-123", "Permission denied");

        // 同一错误应该被去重
        assert!(throttle.should_dedupe_error("cam-123", "Permission denied"));

        // 不同错误不应该被去重
        assert!(!throttle.should_dedupe_error("cam-123", "File not found"));

        // 不同 agent 的相同错误不应该被去重
        assert!(!throttle.should_dedupe_error("cam-456", "Permission denied"));
    }

    #[test]
    fn test_error_dedupe_expires() {
        let mut throttle = NotifyThrottle::with_windows(
            Duration::from_secs(3),
            Duration::from_millis(100), // 100ms 去重窗口
            Duration::from_secs(10),
        );

        // 记录错误
        throttle.record_error("cam-123", "Permission denied");

        // 立即检查，应该被去重
        assert!(throttle.should_dedupe_error("cam-123", "Permission denied"));

        // 等待超过去重窗口
        std::thread::sleep(Duration::from_millis(150));

        // 现在不应该被去重
        assert!(!throttle.should_dedupe_error("cam-123", "Permission denied"));
    }

    #[test]
    fn test_input_wait_debounce() {
        let mut throttle = NotifyThrottle::with_windows(
            Duration::from_secs(3),
            Duration::from_secs(300),
            Duration::from_millis(100), // 100ms 防抖窗口
        );

        // 记录等待输入
        throttle.record_input_wait("cam-123");

        // 立即检查，应该被防抖
        assert!(throttle.should_debounce_input_wait("cam-123"));

        // 等待超过防抖窗口
        std::thread::sleep(Duration::from_millis(150));

        // 现在不应该被防抖
        assert!(!throttle.should_debounce_input_wait("cam-123"));
    }

    #[test]
    fn test_clear_agent() {
        let mut throttle = NotifyThrottle::new();

        // 添加一些状态
        throttle.push(ThrottledEvent::ToolUse {
            agent_id: "cam-123".to_string(),
            tool: "Edit".to_string(),
            target: None,
        });
        throttle.record_error("cam-123", "Error");
        throttle.record_input_wait("cam-123");

        // 清除
        throttle.clear_agent("cam-123");

        // 验证状态已清除
        assert!(!throttle.pending_tools.contains_key("cam-123"));
        assert!(!throttle.recent_input_waits.contains_key("cam-123"));
        assert!(!throttle.should_dedupe_error("cam-123", "Error"));
    }

    #[test]
    fn test_cleanup_expired_records() {
        let mut throttle = NotifyThrottle::with_windows(
            Duration::from_secs(3),
            Duration::from_millis(50),
            Duration::from_millis(50),
        );

        // 添加记录
        throttle.record_error("cam-123", "Error");
        throttle.record_input_wait("cam-123");

        // 等待过期
        std::thread::sleep(Duration::from_millis(100));

        // 清理
        throttle.cleanup();

        // 验证已清理
        assert!(throttle.recent_errors.is_empty());
        assert!(throttle.recent_input_waits.is_empty());
    }
}
