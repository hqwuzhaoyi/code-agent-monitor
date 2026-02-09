//! 通知渠道 trait 定义

use anyhow::Result;
use serde::{Deserialize, Serialize};
use super::urgency::Urgency;

/// 通知消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationMessage {
    /// 消息内容（已格式化）
    pub content: String,
    /// Agent ID（用于回复路由）
    pub agent_id: Option<String>,
    /// 紧急程度
    pub urgency: Urgency,
    /// 结构化 payload（可选，用于 Dashboard）
    pub payload: Option<serde_json::Value>,
    /// 消息元数据
    pub metadata: MessageMetadata,
}

impl NotificationMessage {
    /// 创建简单消息
    pub fn new(content: impl Into<String>, urgency: Urgency) -> Self {
        Self {
            content: content.into(),
            agent_id: None,
            urgency,
            payload: None,
            metadata: MessageMetadata::default(),
        }
    }

    /// 设置 agent_id
    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    /// 设置 payload
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }

    /// 设置元数据
    pub fn with_metadata(mut self, metadata: MessageMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// 消息元数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// 事件类型
    pub event_type: String,
    /// 项目路径
    pub project: Option<String>,
    /// 时间戳
    pub timestamp: Option<String>,
}

/// 发送结果
#[derive(Debug, Clone, PartialEq)]
pub enum SendResult {
    /// 发送成功
    Sent,
    /// 跳过（不符合渠道过滤条件）
    Skipped(String),
    /// 发送失败
    Failed(String),
}

/// 通知渠道 trait
pub trait NotificationChannel: Send + Sync {
    /// 渠道名称（用于日志和配置）
    fn name(&self) -> &str;

    /// 是否应该发送此消息（根据 urgency 等条件过滤）
    fn should_send(&self, message: &NotificationMessage) -> bool;

    /// 同步发送消息
    fn send(&self, message: &NotificationMessage) -> Result<SendResult>;

    /// 异步发送消息（spawn 后立即返回）
    fn send_async(&self, message: &NotificationMessage) -> Result<()>;
}

/// 检查 urgency 是否满足最低要求
pub fn urgency_meets_threshold(message_urgency: Urgency, min_urgency: Urgency) -> bool {
    match (message_urgency, min_urgency) {
        (Urgency::High, _) => true,
        (Urgency::Medium, Urgency::High) => false,
        (Urgency::Medium, _) => true,
        (Urgency::Low, Urgency::Low) => true,
        (Urgency::Low, _) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urgency_meets_threshold() {
        // High 总是满足
        assert!(urgency_meets_threshold(Urgency::High, Urgency::High));
        assert!(urgency_meets_threshold(Urgency::High, Urgency::Medium));
        assert!(urgency_meets_threshold(Urgency::High, Urgency::Low));

        // Medium 满足 Medium 和 Low
        assert!(!urgency_meets_threshold(Urgency::Medium, Urgency::High));
        assert!(urgency_meets_threshold(Urgency::Medium, Urgency::Medium));
        assert!(urgency_meets_threshold(Urgency::Medium, Urgency::Low));

        // Low 只满足 Low
        assert!(!urgency_meets_threshold(Urgency::Low, Urgency::High));
        assert!(!urgency_meets_threshold(Urgency::Low, Urgency::Medium));
        assert!(urgency_meets_threshold(Urgency::Low, Urgency::Low));
    }

    #[test]
    fn test_notification_message_builder() {
        let msg = NotificationMessage::new("test", Urgency::High)
            .with_agent_id("cam-123")
            .with_payload(serde_json::json!({"type": "test"}));

        assert_eq!(msg.content, "test");
        assert_eq!(msg.agent_id, Some("cam-123".to_string()));
        assert_eq!(msg.urgency, Urgency::High);
        assert!(msg.payload.is_some());
    }
}
