//! System Event payload 结构
//!
//! 定义发送给 OpenClaw 的结构化事件数据

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::notification::event::{NotificationEvent, NotificationEventType};
use crate::notification::summarizer::NotificationSummarizer;
use crate::notification::urgency::Urgency;

/// System Event Payload - 发送给 OpenClaw 的结构化数据
///
/// NOTE: OpenClaw Gateway 使用 camelCase 字段名。
/// 使用 `#[serde(rename_all = "camelCase")]` 确保序列化时转换为 camelCase。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
///
/// NOTE: 使用 camelCase 以匹配 OpenClaw Gateway 期望的格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase")]
pub enum EventData {
    PermissionRequest {
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(rename = "toolInput")]
        tool_input: Value,
    },
    WaitingForInput {
        #[serde(rename = "patternType")]
        pattern_type: String,
        #[serde(rename = "isDecisionRequired")]
        is_decision_required: bool,
    },
    Notification {
        #[serde(rename = "notificationType")]
        notification_type: String,
        message: String,
    },
    Error {
        message: String,
    },
    Empty {},
}

/// 上下文信息
///
/// NOTE: 使用 camelCase 以匹配 OpenClaw Gateway 期望的格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventContext {
    /// 终端快照
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_snapshot: Option<String>,
    /// AI 提取的格式化消息（包含完整问题和选项）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extracted_message: Option<String>,
    /// 问题指纹（用于去重）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question_fingerprint: Option<String>,
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
            NotificationEventType::PermissionRequest {
                tool_name,
                tool_input,
            } => EventData::PermissionRequest {
                tool_name: tool_name.clone(),
                tool_input: tool_input.clone(),
            },
            NotificationEventType::WaitingForInput {
                pattern_type,
                is_decision_required,
            } => EventData::WaitingForInput {
                pattern_type: pattern_type.clone(),
                is_decision_required: *is_decision_required,
            },
            NotificationEventType::Notification {
                notification_type,
                message,
            } => EventData::Notification {
                notification_type: notification_type.clone(),
                message: message.clone(),
            },
            NotificationEventType::Error { message } => EventData::Error {
                message: message.clone(),
            },
            _ => EventData::Empty {},
        };

        // 计算风险等级
        let risk_level = match &event.event_type {
            NotificationEventType::PermissionRequest {
                tool_name,
                tool_input,
            } => {
                let input_str = tool_input.to_string();
                assess_risk_level(tool_name, &input_str).to_string()
            }
            // WaitingForInput 需要用户交互
            // 如果是需要关键决策，设为 HIGH
            NotificationEventType::WaitingForInput {
                is_decision_required,
                ..
            } => {
                if *is_decision_required {
                    "HIGH".to_string()
                } else {
                    "MEDIUM".to_string()
                }
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
                extracted_message: None,
                question_fingerprint: None,
                risk_level,
            },
        }
    }

    /// 转换为 JSON Value
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    /// 升级 is_decision_required 标志（AI 提取器检测到需要用户决策）
    ///
    /// 当 AI 提取器判断需要用户决策时，升级 event_data 和 risk_level
    pub fn set_decision_required(&mut self, value: bool) {
        if value {
            if let EventData::WaitingForInput {
                ref mut is_decision_required,
                ..
            } = self.event_data
            {
                *is_decision_required = true;
            }
            self.context.risk_level = "HIGH".to_string();
        }
    }

    /// 设置 AI 提取的消息和指纹
    ///
    /// 在发送通知前调用，避免在 from_event 中重复调用 AI
    pub fn set_extracted_message(&mut self, message: String, fingerprint: String) {
        self.context.extracted_message = Some(message);
        self.context.question_fingerprint = Some(fingerprint);
    }

    /// 转换为 Telegram 消息格式
    pub fn to_telegram_message(&self) -> String {
        let emoji = match self.urgency.as_str() {
            "HIGH" => "⚠️",
            "MEDIUM" => "💬",
            _ => "ℹ️",
        };

        let event_desc = match self.event_type.as_str() {
            "permission_request" => {
                // 优先使用 AI 提取的消息
                if let Some(extracted) = &self.context.extracted_message {
                    extracted.clone()
                } else if let EventData::PermissionRequest {
                    tool_name,
                    tool_input,
                } = &self.event_data
                {
                    let cmd = tool_input
                        .get("command")
                        .or_else(|| tool_input.get("file_path"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    // Fallback: 截取终端最后 30 行
                    let snapshot_tail = self.context.terminal_snapshot.as_ref().map(|snapshot| {
                        let lines: Vec<&str> = snapshot.lines().collect();
                        let start = lines.len().saturating_sub(30);
                        lines[start..].join("\n")
                    });

                    if let Some(tail) = snapshot_tail {
                        format!("执行: {} {}\n\n{}", tool_name, cmd, tail)
                    } else {
                        format!("执行: {} {}", tool_name, cmd)
                    }
                } else {
                    "请求权限".to_string()
                }
            }
            "waiting_for_input" => {
                // 优先使用 AI 提取的消息
                if let Some(extracted) = &self.context.extracted_message {
                    extracted.clone()
                } else if let Some(snapshot) = &self.context.terminal_snapshot {
                    // Fallback: 截取终端最后 30 行
                    let lines: Vec<&str> = snapshot.lines().collect();
                    let start = lines.len().saturating_sub(30);
                    let preview = lines[start..].join("\n");
                    format!("等待输入\n\n{}", preview)
                } else {
                    "等待输入".to_string()
                }
            }
            "notification" => {
                // Show the actual notification message
                if let EventData::Notification {
                    message,
                    notification_type,
                } = &self.event_data
                {
                    format!("{}: {}", notification_type, message)
                } else {
                    "通知".to_string()
                }
            }
            "error" => {
                if let EventData::Error { message } = &self.event_data {
                    format!("错误: {}", message)
                } else {
                    "发生错误".to_string()
                }
            }
            "agent_exited" => "Agent 已退出".to_string(),
            _ => self.event_type.clone(),
        };

        let risk = self.context.risk_level.as_str();

        let risk_emoji = match risk {
            "HIGH" => "🔴",
            "MEDIUM" => "🟡",
            "LOW" => "🟢",
            _ => "⚪",
        };

        let action_hint = match self.event_type.as_str() {
            "permission_request" => "回复 y 允许 / n 拒绝",
            "waiting_for_input" => "回复你的选择或输入内容",
            _ => "无需回复",
        };

        format!(
            "{} *CAM* {}\n\n{}\n\n风险: {} {}\n\n{}",
            emoji, self.agent_id, event_desc, risk_emoji, risk, action_hint
        )
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
        assert_eq!(
            assess_risk_level("Bash", r#"{"command": "rm -rf /"}"#),
            "HIGH"
        );
    }

    #[test]
    fn test_assess_risk_level_write() {
        assert_eq!(
            assess_risk_level("Write", r#"{"file_path": "/tmp/test.txt"}"#),
            "LOW"
        );
        assert_eq!(
            assess_risk_level("Write", r#"{"file_path": "/etc/passwd"}"#),
            "HIGH"
        );
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
        // 验证 camelCase 序列化
        assert!(
            json.get("eventType").is_some(),
            "should use camelCase: eventType"
        );
        assert!(
            json.get("agentId").is_some(),
            "should use camelCase: agentId"
        );
        assert!(
            json.get("projectPath").is_some(),
            "should use camelCase: projectPath"
        );
        assert!(
            json.get("eventData").is_some(),
            "should use camelCase: eventData"
        );
        // 验证不存在 snake_case
        assert!(
            json.get("event_type").is_none(),
            "should NOT use snake_case: event_type"
        );
        assert!(
            json.get("agent_id").is_none(),
            "should NOT use snake_case: agent_id"
        );
    }

    #[test]
    fn test_decision_required_risk_level_high() {
        let event = NotificationEvent::waiting_for_input_with_decision(
            "cam-decision-1",
            "Choice",
            true,
        );

        let payload = SystemEventPayload::from_event(&event, Urgency::High);

        assert_eq!(payload.context.risk_level, "HIGH");
    }

    #[test]
    fn test_decision_not_required_risk_level_medium() {
        let event = NotificationEvent::waiting_for_input_with_decision(
            "cam-decision-2",
            "Confirmation",
            false,
        );

        let payload = SystemEventPayload::from_event(&event, Urgency::Medium);

        assert_eq!(payload.context.risk_level, "MEDIUM");
    }

    #[test]
    fn test_decision_required_in_event_data_json() {
        let event = NotificationEvent::waiting_for_input_with_decision(
            "cam-json-1",
            "Choice",
            true,
        );

        let payload = SystemEventPayload::from_event(&event, Urgency::High);
        let json = serde_json::to_string(&payload).unwrap();

        assert!(
            json.contains("\"isDecisionRequired\":true"),
            "JSON should contain isDecisionRequired:true, got: {}",
            json
        );
    }

    #[test]
    fn test_decision_not_required_in_event_data_json() {
        let event = NotificationEvent::waiting_for_input_with_decision(
            "cam-json-2",
            "Confirmation",
            false,
        );

        let payload = SystemEventPayload::from_event(&event, Urgency::Medium);
        let json = serde_json::to_string(&payload).unwrap();

        assert!(
            json.contains("\"isDecisionRequired\":false"),
            "JSON should contain isDecisionRequired:false, got: {}",
            json
        );
    }

    #[test]
    fn test_set_decision_required_upgrades_payload() {
        let event = NotificationEvent::waiting_for_input_with_decision(
            "cam-upgrade",
            "Confirmation",
            false,
        );

        let mut payload = SystemEventPayload::from_event(&event, Urgency::Medium);

        // Initially MEDIUM risk
        assert_eq!(payload.context.risk_level, "MEDIUM");
        if let EventData::WaitingForInput {
            is_decision_required,
            ..
        } = &payload.event_data
        {
            assert!(!is_decision_required);
        } else {
            panic!("Expected WaitingForInput event data");
        }

        // Upgrade via set_decision_required
        payload.set_decision_required(true);

        // Now should be HIGH risk
        assert_eq!(payload.context.risk_level, "HIGH");
        if let EventData::WaitingForInput {
            is_decision_required,
            ..
        } = &payload.event_data
        {
            assert!(is_decision_required);
        } else {
            panic!("Expected WaitingForInput event data after upgrade");
        }
    }

    #[test]
    fn test_telegram_message_decision_high_urgency() {
        let event = NotificationEvent::waiting_for_input_with_decision(
            "cam-tg-1",
            "Choice",
            true,
        );

        let payload = SystemEventPayload::from_event(&event, Urgency::High);
        let msg = payload.to_telegram_message();

        // HIGH urgency should use warning emoji
        assert!(
            msg.contains("⚠️"),
            "HIGH urgency telegram message should contain ⚠️ emoji, got: {}",
            msg
        );
        // Risk level should show HIGH with red circle
        assert!(
            msg.contains("🔴") && msg.contains("HIGH"),
            "Decision-required message should show HIGH risk with 🔴, got: {}",
            msg
        );
        // Should contain agent_id
        assert!(msg.contains("cam-tg-1"));
        // Should contain action hint for waiting_for_input
        assert!(msg.contains("回复你的选择或输入内容"));
    }

    #[test]
    fn test_permission_request_includes_terminal_tail_in_message() {
        let mut event = NotificationEvent::permission_request(
            "cam-123",
            "Bash",
            serde_json::json!({"command": "echo hi"}),
        );
        event.terminal_snapshot = Some(
            (1..=50)
                .map(|i| format!("line {}", i))
                .collect::<Vec<_>>()
                .join("\n"),
        );

        let payload = SystemEventPayload::from_event(&event, Urgency::High);
        let msg = payload.to_telegram_message();

        // Tail should include the last line, and (by construction) omit the first.
        assert!(msg.contains("line 50"));
        assert!(!msg.contains("line 1\nline 2\nline 3"));
    }
}
