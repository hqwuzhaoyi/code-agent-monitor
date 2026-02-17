//! System Event payload 结构
//!
//! 定义发送给 OpenClaw 的结构化事件数据

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::notification::event::{NotificationEvent, NotificationEventType};
use crate::notification::urgency::Urgency;
use crate::notification::summarizer::NotificationSummarizer;

/// System Event Payload - 发送给 OpenClaw 的结构化数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEventPayload {
    /// 来源标识
    pub source: String,
    /// 版本号
    pub version: String,
    /// Agent ID
    pub agent_id: String,
    /// 事件类型
    pub event_type: String,
    /// 紧急程度
    pub urgency: String,
    /// 项目路径
    pub project_path: Option<String>,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 事件数据（根据 event_type 不同而不同）
    pub event_data: EventData,
    /// 上下文信息
    pub context: EventContext,
}

/// 事件数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventData {
    PermissionRequest {
        tool_name: String,
        tool_input: Value,
    },
    WaitingForInput {
        pattern_type: String,
    },
    Notification {
        notification_type: String,
        message: String,
    },
    Error {
        message: String,
    },
    Empty {},
}

/// 上下文信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventContext {
    /// 终端快照
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_snapshot: Option<String>,
    /// 风险等级
    pub risk_level: String,
}

/// 评估风险等级（返回字符串形式）
pub fn assess_risk_level(tool_name: &str, tool_input: &str) -> &'static str {
    let summarizer = NotificationSummarizer::new();

    // 解析 tool_input 为 JSON
    let input_value: Value = serde_json::from_str(tool_input).unwrap_or(Value::Null);

    let risk = match tool_name {
        "Bash" => {
            let command = input_value
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            summarizer.assess_bash_risk(command)
        }
        "Write" | "Edit" | "Read" => {
            let path = input_value
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let operation = if tool_name == "Read" { "read" } else { "write" };
            summarizer.assess_file_risk(path, operation)
        }
        _ => crate::notification::summarizer::RiskLevel::Low,
    };

    match risk {
        crate::notification::summarizer::RiskLevel::Low => "LOW",
        crate::notification::summarizer::RiskLevel::Medium => "MEDIUM",
        crate::notification::summarizer::RiskLevel::High => "HIGH",
    }
}

impl SystemEventPayload {
    /// 从 NotificationEvent 构建 payload
    pub fn from_event(event: &NotificationEvent, urgency: Urgency) -> Self {
        let event_type_str = match &event.event_type {
            NotificationEventType::WaitingForInput { .. } => "waiting_for_input",
            NotificationEventType::PermissionRequest { .. } => "permission_request",
            NotificationEventType::Notification { .. } => "notification",
            NotificationEventType::AgentExited => "agent_exited",
            NotificationEventType::Error { .. } => "error",
            NotificationEventType::Stop => "stop",
            NotificationEventType::SessionStart => "session_start",
            NotificationEventType::SessionEnd => "session_end",
        };

        let event_data = match &event.event_type {
            NotificationEventType::PermissionRequest { tool_name, tool_input } => {
                EventData::PermissionRequest {
                    tool_name: tool_name.clone(),
                    tool_input: tool_input.clone(),
                }
            }
            NotificationEventType::WaitingForInput { pattern_type } => {
                EventData::WaitingForInput {
                    pattern_type: pattern_type.clone(),
                }
            }
            NotificationEventType::Notification { notification_type, message } => {
                EventData::Notification {
                    notification_type: notification_type.clone(),
                    message: message.clone(),
                }
            }
            NotificationEventType::Error { message } => {
                EventData::Error {
                    message: message.clone(),
                }
            }
            _ => EventData::Empty {},
        };

        // 计算风险等级
        let risk_level = match &event.event_type {
            NotificationEventType::PermissionRequest { tool_name, tool_input } => {
                let input_str = tool_input.to_string();
                assess_risk_level(tool_name, &input_str).to_string()
            }
            _ => "LOW".to_string(),
        };

        Self {
            source: "cam".to_string(),
            version: "1.0".to_string(),
            agent_id: event.agent_id.clone(),
            event_type: event_type_str.to_string(),
            urgency: urgency.as_str().to_string(),
            project_path: event.project_path.clone(),
            timestamp: event.timestamp,
            event_data,
            context: EventContext {
                terminal_snapshot: event.terminal_snapshot.clone(),
                risk_level,
            },
        }
    }

    /// 转换为 JSON Value
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assess_risk_level_bash_low() {
        assert_eq!(assess_risk_level("Bash", r#"{"command": "ls -la"}"#), "LOW");
    }

    #[test]
    fn test_assess_risk_level_bash_high() {
        assert_eq!(assess_risk_level("Bash", r#"{"command": "rm -rf /"}"#), "HIGH");
    }

    #[test]
    fn test_assess_risk_level_write() {
        assert_eq!(assess_risk_level("Write", r#"{"file_path": "/tmp/test.txt"}"#), "LOW");
        assert_eq!(assess_risk_level("Write", r#"{"file_path": "/etc/passwd"}"#), "HIGH");
    }

    #[test]
    fn test_system_event_payload_from_event() {
        let event = NotificationEvent::permission_request(
            "cam-123",
            "Bash",
            serde_json::json!({"command": "ls -la"}),
        );

        let payload = SystemEventPayload::from_event(&event, Urgency::High);

        assert_eq!(payload.source, "cam");
        assert_eq!(payload.version, "1.0");
        assert_eq!(payload.agent_id, "cam-123");
        assert_eq!(payload.event_type, "permission_request");
        assert_eq!(payload.urgency, "HIGH");
        assert_eq!(payload.context.risk_level, "LOW");
    }

    #[test]
    fn test_system_event_payload_to_json() {
        let event = NotificationEvent::error("cam-456", "Test error");
        let payload = SystemEventPayload::from_event(&event, Urgency::High);

        let json = payload.to_json();
        assert_eq!(json["source"], "cam");
        assert_eq!(json["event_type"], "error");
    }
}
