//! 统一通知事件结构
//!
//! 定义 Hook 和 Watcher 共用的事件数据结构，解决数据格式不一致问题。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 统一的通知事件结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    /// Agent ID
    pub agent_id: String,
    /// 事件类型
    pub event_type: NotificationEventType,
    /// 项目路径（用于提取项目名）
    pub project_path: Option<String>,
    /// 终端快照
    pub terminal_snapshot: Option<String>,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 去重键（由 watcher 生成，用于通知去重）
    pub dedup_key: Option<String>,
    /// 跳过去重（强制发送）
    #[serde(default)]
    pub skip_dedup: bool,
}

/// 事件类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationEventType {
    /// 等待用户输入（Watcher 检测）
    WaitingForInput {
        pattern_type: String,
        /// 是否需要关键决策
        is_decision_required: bool,
    },
    /// 权限请求
    PermissionRequest {
        tool_name: String,
        tool_input: Value,
    },
    /// 通知（Hook 触发）
    Notification {
        notification_type: String,
        message: String,
    },
    /// Agent 退出
    AgentExited,
    /// 错误
    Error { message: String },
    /// 停止
    Stop,
    /// 会话开始
    SessionStart,
    /// 会话结束
    SessionEnd,
}

impl NotificationEvent {
    /// 创建新的事件
    pub fn new(agent_id: impl Into<String>, event_type: NotificationEventType) -> Self {
        Self {
            agent_id: agent_id.into(),
            event_type,
            project_path: None,
            terminal_snapshot: None,
            timestamp: Utc::now(),
            dedup_key: None,
            skip_dedup: false,
        }
    }

    /// 获取项目名（从路径提取）
    pub fn project_name(&self) -> &str {
        self.project_path
            .as_ref()
            .and_then(|p| p.rsplit('/').next())
            .unwrap_or(&self.agent_id)
    }

    /// 判断是否需要用户回复
    pub fn needs_reply(&self) -> bool {
        match &self.event_type {
            NotificationEventType::WaitingForInput { .. } => true,
            NotificationEventType::PermissionRequest { .. } => true,
            NotificationEventType::Notification {
                notification_type, ..
            } => notification_type == "permission_prompt" || notification_type == "idle_prompt",
            _ => false,
        }
    }
}

/// 事件构建器
#[derive(Debug, Default)]
pub struct NotificationEventBuilder {
    agent_id: Option<String>,
    event_type: Option<NotificationEventType>,
    project_path: Option<String>,
    terminal_snapshot: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    dedup_key: Option<String>,
}

impl NotificationEventBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 Agent ID
    pub fn agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    /// 设置事件类型
    pub fn event_type(mut self, event_type: NotificationEventType) -> Self {
        self.event_type = Some(event_type);
        self
    }

    /// 设置项目路径
    pub fn project_path(mut self, path: impl Into<String>) -> Self {
        self.project_path = Some(path.into());
        self
    }

    /// 设置终端快照
    pub fn terminal_snapshot(mut self, snapshot: impl Into<String>) -> Self {
        self.terminal_snapshot = Some(snapshot.into());
        self
    }

    /// 设置时间戳
    pub fn timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// 设置去重键
    pub fn dedup_key(mut self, key: impl Into<String>) -> Self {
        self.dedup_key = Some(key.into());
        self
    }

    /// 构建事件
    pub fn build(self) -> Result<NotificationEvent, &'static str> {
        let agent_id = self.agent_id.ok_or("agent_id is required")?;
        let event_type = self.event_type.ok_or("event_type is required")?;

        Ok(NotificationEvent {
            agent_id,
            event_type,
            project_path: self.project_path,
            terminal_snapshot: self.terminal_snapshot,
            timestamp: self.timestamp.unwrap_or_else(Utc::now),
            dedup_key: self.dedup_key,
            skip_dedup: false,
        })
    }
}

/// 便捷构造函数
impl NotificationEvent {
    /// 创建等待输入事件
    pub fn waiting_for_input(agent_id: impl Into<String>, pattern_type: impl Into<String>) -> Self {
        Self::waiting_for_input_with_decision(agent_id, pattern_type, false)
    }

    /// 创建等待输入事件（带决策标记）
    pub fn waiting_for_input_with_decision(
        agent_id: impl Into<String>,
        pattern_type: impl Into<String>,
        is_decision_required: bool,
    ) -> Self {
        Self::new(
            agent_id,
            NotificationEventType::WaitingForInput {
                pattern_type: pattern_type.into(),
                is_decision_required,
            },
        )
    }

    /// 创建权限请求事件
    pub fn permission_request(
        agent_id: impl Into<String>,
        tool_name: impl Into<String>,
        tool_input: Value,
    ) -> Self {
        Self::new(
            agent_id,
            NotificationEventType::PermissionRequest {
                tool_name: tool_name.into(),
                tool_input,
            },
        )
    }

    /// 创建通知事件
    pub fn notification(
        agent_id: impl Into<String>,
        notification_type: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(
            agent_id,
            NotificationEventType::Notification {
                notification_type: notification_type.into(),
                message: message.into(),
            },
        )
    }

    /// 创建 Agent 退出事件
    pub fn agent_exited(agent_id: impl Into<String>) -> Self {
        Self::new(agent_id, NotificationEventType::AgentExited)
    }

    /// 创建错误事件
    pub fn error(agent_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            agent_id,
            NotificationEventType::Error {
                message: message.into(),
            },
        )
    }

    /// 创建停止事件
    pub fn stop(agent_id: impl Into<String>) -> Self {
        Self::new(agent_id, NotificationEventType::Stop)
    }

    /// 创建会话开始事件
    pub fn session_start(agent_id: impl Into<String>) -> Self {
        Self::new(agent_id, NotificationEventType::SessionStart)
    }

    /// 创建会话结束事件
    pub fn session_end(agent_id: impl Into<String>) -> Self {
        Self::new(agent_id, NotificationEventType::SessionEnd)
    }

    /// 设置项目路径（链式调用）
    pub fn with_project_path(mut self, path: impl Into<String>) -> Self {
        self.project_path = Some(path.into());
        self
    }

    /// 设置终端快照（链式调用）
    pub fn with_terminal_snapshot(mut self, snapshot: impl Into<String>) -> Self {
        self.terminal_snapshot = Some(snapshot.into());
        self
    }

    /// 设置去重键（链式调用）
    pub fn with_dedup_key(mut self, key: impl Into<String>) -> Self {
        self.dedup_key = Some(key.into());
        self
    }

    /// 跳过去重（链式调用）
    pub fn with_skip_dedup(mut self, skip: bool) -> Self {
        self.skip_dedup = skip;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_event() {
        let event = NotificationEvent::new(
            "cam-123",
            NotificationEventType::WaitingForInput {
                pattern_type: "Confirmation".to_string(),
                is_decision_required: false,
            },
        );

        assert_eq!(event.agent_id, "cam-123");
        assert!(matches!(
            event.event_type,
            NotificationEventType::WaitingForInput { .. }
        ));
    }

    #[test]
    fn test_waiting_for_input() {
        let event = NotificationEvent::waiting_for_input("cam-456", "ClaudePrompt");

        assert_eq!(event.agent_id, "cam-456");
        if let NotificationEventType::WaitingForInput {
            pattern_type,
            is_decision_required,
        } = &event.event_type
        {
            assert_eq!(pattern_type, "ClaudePrompt");
            assert!(!is_decision_required);
        } else {
            panic!("Expected WaitingForInput event type");
        }
    }

    #[test]
    fn test_permission_request() {
        let tool_input = serde_json::json!({"command": "rm -rf /tmp/test"});
        let event = NotificationEvent::permission_request("cam-789", "Bash", tool_input.clone());

        assert_eq!(event.agent_id, "cam-789");
        if let NotificationEventType::PermissionRequest {
            tool_name,
            tool_input: input,
        } = &event.event_type
        {
            assert_eq!(tool_name, "Bash");
            assert_eq!(input, &tool_input);
        } else {
            panic!("Expected PermissionRequest event type");
        }
    }

    #[test]
    fn test_notification() {
        let event = NotificationEvent::notification("cam-abc", "idle_prompt", "Task completed");

        if let NotificationEventType::Notification {
            notification_type,
            message,
        } = &event.event_type
        {
            assert_eq!(notification_type, "idle_prompt");
            assert_eq!(message, "Task completed");
        } else {
            panic!("Expected Notification event type");
        }
    }

    #[test]
    fn test_agent_exited() {
        let event = NotificationEvent::agent_exited("cam-def");
        assert!(matches!(
            event.event_type,
            NotificationEventType::AgentExited
        ));
    }

    #[test]
    fn test_error() {
        let event = NotificationEvent::error("cam-ghi", "API rate limit exceeded");

        if let NotificationEventType::Error { message } = &event.event_type {
            assert_eq!(message, "API rate limit exceeded");
        } else {
            panic!("Expected Error event type");
        }
    }

    #[test]
    fn test_project_name() {
        let event = NotificationEvent::waiting_for_input("cam-123", "Confirmation")
            .with_project_path("/Users/admin/workspace/my-project");

        assert_eq!(event.project_name(), "my-project");
    }

    #[test]
    fn test_project_name_fallback() {
        let event = NotificationEvent::waiting_for_input("cam-123", "Confirmation");
        // No project_path set, should fallback to agent_id
        assert_eq!(event.project_name(), "cam-123");
    }

    #[test]
    fn test_needs_reply() {
        // WaitingForInput needs reply
        let event1 = NotificationEvent::waiting_for_input("cam-1", "Confirmation");
        assert!(event1.needs_reply());

        // PermissionRequest needs reply
        let event2 = NotificationEvent::permission_request("cam-2", "Bash", serde_json::json!({}));
        assert!(event2.needs_reply());

        // idle_prompt notification needs reply
        let event3 = NotificationEvent::notification("cam-3", "idle_prompt", "waiting");
        assert!(event3.needs_reply());

        // permission_prompt notification needs reply
        let event4 = NotificationEvent::notification("cam-4", "permission_prompt", "confirm?");
        assert!(event4.needs_reply());

        // AgentExited does not need reply
        let event5 = NotificationEvent::agent_exited("cam-5");
        assert!(!event5.needs_reply());

        // Stop does not need reply
        let event6 = NotificationEvent::stop("cam-6");
        assert!(!event6.needs_reply());
    }

    #[test]
    fn test_with_terminal_snapshot() {
        let event = NotificationEvent::waiting_for_input("cam-123", "Confirmation")
            .with_terminal_snapshot("$ cargo build\n   Compiling...");

        assert_eq!(
            event.terminal_snapshot,
            Some("$ cargo build\n   Compiling...".to_string())
        );
    }

    #[test]
    fn test_with_dedup_key() {
        let event = NotificationEvent::waiting_for_input("cam-123", "Confirmation")
            .with_dedup_key("abc123");

        assert_eq!(event.dedup_key, Some("abc123".to_string()));
    }

    #[test]
    fn test_builder_with_dedup_key() {
        let event = NotificationEventBuilder::new()
            .agent_id("cam-builder")
            .event_type(NotificationEventType::AgentExited)
            .dedup_key("dedup-key-123")
            .build()
            .unwrap();

        assert_eq!(event.dedup_key, Some("dedup-key-123".to_string()));
    }

    #[test]
    fn test_builder() {
        let event = NotificationEventBuilder::new()
            .agent_id("cam-builder")
            .event_type(NotificationEventType::Error {
                message: "test error".to_string(),
            })
            .project_path("/workspace/test")
            .terminal_snapshot("error output")
            .build()
            .unwrap();

        assert_eq!(event.agent_id, "cam-builder");
        assert_eq!(event.project_path, Some("/workspace/test".to_string()));
        assert_eq!(event.terminal_snapshot, Some("error output".to_string()));
    }

    #[test]
    fn test_builder_missing_agent_id() {
        let result = NotificationEventBuilder::new()
            .event_type(NotificationEventType::AgentExited)
            .build();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "agent_id is required");
    }

    #[test]
    fn test_builder_missing_event_type() {
        let result = NotificationEventBuilder::new().agent_id("cam-123").build();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "event_type is required");
    }

    #[test]
    fn test_serialization() {
        let event = NotificationEvent::permission_request(
            "cam-ser",
            "Write",
            serde_json::json!({"file_path": "/tmp/test.txt"}),
        )
        .with_project_path("/workspace/project");

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: NotificationEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.agent_id, "cam-ser");
        assert_eq!(
            deserialized.project_path,
            Some("/workspace/project".to_string())
        );
    }

    #[test]
    fn test_event_type_serialization() {
        let event_type = NotificationEventType::WaitingForInput {
            pattern_type: "Confirmation".to_string(),
            is_decision_required: false,
        };

        let json = serde_json::to_string(&event_type).unwrap();
        assert!(json.contains("waiting_for_input"));
        assert!(json.contains("Confirmation"));

        let deserialized: NotificationEventType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, event_type);
    }
}
