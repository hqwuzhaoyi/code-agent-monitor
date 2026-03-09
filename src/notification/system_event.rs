//! System Event payload 结构
//!
//! 定义发送给 OpenClaw 的结构化事件数据

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::notification::event::{NotificationEvent, NotificationEventType};
use crate::notification::progress::ProgressSnapshot;
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
    /// 任务进度快照（可选，有 team 任务数据时存在）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ProgressSnapshot>,
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
            progress: ProgressSnapshot::from_agent(&event.agent_id),
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

    /// 转换为纯文本通知消息（钉钉/Telegram 共用）
    pub fn to_telegram_message(&self) -> String {
        let emoji = match self.urgency.as_str() {
            "HIGH" => "🔴",
            "MEDIUM" => "🟡",
            _ => "🟢",
        };

        let event_label = match self.event_type.as_str() {
            "permission_request" => "权限请求",
            "waiting_for_input" => "等待输入",
            "error" => "错误",
            "agent_exited" => "Agent 退出",
            "notification" => "通知",
            _ => &self.event_type,
        };

        let event_desc = match self.event_type.as_str() {
            "permission_request" => {
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

                    let snapshot_tail = self.context.terminal_snapshot.as_ref().map(|snapshot| {
                        let lines: Vec<&str> = snapshot.lines().collect();
                        let start = lines.len().saturating_sub(30);
                        lines[start..].join("\n")
                    });

                    if let Some(tail) = snapshot_tail {
                        format!("{} {}\n\n{}", tool_name, cmd, tail)
                    } else {
                        format!("{} {}", tool_name, cmd)
                    }
                } else {
                    String::new()
                }
            }
            "waiting_for_input" => {
                if let Some(extracted) = &self.context.extracted_message {
                    extracted.clone()
                } else if let Some(snapshot) = &self.context.terminal_snapshot {
                    let lines: Vec<&str> = snapshot.lines().collect();
                    let start = lines.len().saturating_sub(30);
                    lines[start..].join("\n")
                } else {
                    String::new()
                }
            }
            "notification" => {
                if let EventData::Notification {
                    message,
                    notification_type,
                } = &self.event_data
                {
                    if message.is_empty() {
                        notification_type.clone()
                    } else {
                        message.clone()
                    }
                } else {
                    String::new()
                }
            }
            "error" => {
                if let EventData::Error { message } = &self.event_data {
                    message.clone()
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        };

        let project = self
            .project_path
            .as_deref()
            .unwrap_or("unknown");

        let action_hint = match self.event_type.as_str() {
            "permission_request" => "\n\n💡 回复 y 允许 / n 拒绝",
            "waiting_for_input" => "\n\n💡 回复你的选择或输入",
            _ => "",
        };

        let progress_block = if let Some(ref progress) = self.progress {
            let mut block = format!(
                "\n\n📊 进度: {}/{} ({}%)",
                progress.completed_tasks, progress.total_tasks, progress.completion_rate
            );
            if !progress.remaining_top3.is_empty() {
                block.push_str("\n剩余重点:");
                for (i, item) in progress.remaining_top3.iter().enumerate() {
                    block.push_str(&format!("\n  {}. {}", i + 1, item.subject));
                }
            }
            if progress.needs_confirmation {
                block.push_str(&format!(
                    "\n⏳ 待确认项: {}",
                    progress.pending_confirmations_count
                ));
            }
            block
        } else {
            String::new()
        };

        if event_desc.is_empty() {
            format!(
                "{} [CAM] {}\n{} | {}\n项目: {}{}{}",
                emoji, self.agent_id, event_label, self.urgency, project, progress_block, action_hint
            )
        } else {
            format!(
                "{} [CAM] {}\n{} | {}\n项目: {}\n\n{}{}{}",
                emoji, self.agent_id, event_label, self.urgency, project, event_desc, progress_block, action_hint
            )
        }
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

        // HIGH urgency uses red circle emoji
        assert!(
            msg.contains("🔴"),
            "HIGH urgency message should contain 🔴, got: {}",
            msg
        );
        assert!(msg.contains("cam-tg-1"));
        assert!(msg.contains("回复你的选择或输入"));
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
