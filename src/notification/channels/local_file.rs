//! 本地文件渠道 - 将所有通知写入 JSONL 文件

use anyhow::Result;
use chrono::Utc;
use tracing::{debug, warn};

use crate::notification::channel::{NotificationChannel, NotificationMessage, SendResult};
use crate::notification::store::{NotificationRecord, NotificationStore};

/// 本地文件渠道 - 记录所有通知到本地文件
pub struct LocalFileChannel;

impl LocalFileChannel {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalFileChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationChannel for LocalFileChannel {
    fn name(&self) -> &str {
        "local_file"
    }

    fn should_send(&self, _message: &NotificationMessage) -> bool {
        // 记录所有通知
        true
    }

    fn send(&self, message: &NotificationMessage) -> Result<SendResult> {
        let record = build_record(message);

        match NotificationStore::append(&record) {
            Ok(()) => {
                debug!(
                    channel = "local_file",
                    agent_id = ?message.agent_id,
                    "Notification recorded to local file"
                );
                Ok(SendResult::Sent)
            }
            Err(e) => {
                warn!(
                    channel = "local_file",
                    error = %e,
                    "Failed to write notification to local file"
                );
                Ok(SendResult::Failed(e.to_string()))
            }
        }
    }

    fn send_async(&self, message: &NotificationMessage) -> Result<()> {
        // 本地文件写入很快，直接同步执行
        let _ = self.send(message);
        Ok(())
    }
}

/// 从 NotificationMessage 构建 NotificationRecord
fn build_record(message: &NotificationMessage) -> NotificationRecord {
    let (project, event_detail, terminal_snapshot, risk_level) =
        if let Some(ref payload) = message.payload {
            (
                payload
                    .get("project")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                payload.get("event").cloned(),
                payload
                    .get("terminal_snapshot")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                payload
                    .get("risk_level")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            )
        } else {
            (message.metadata.project.clone(), None, None, None)
        };

    NotificationRecord {
        ts: Utc::now(),
        agent_id: message
            .agent_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        urgency: message.urgency,
        event: message.metadata.event_type.clone(),
        summary: truncate_summary(&message.content, 100),
        project,
        event_detail,
        terminal_snapshot,
        risk_level,
    }
}

/// 截断摘要到指定长度
fn truncate_summary(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::urgency::Urgency;

    #[test]
    fn test_local_file_channel_should_send_always_true() {
        let channel = LocalFileChannel::new();
        let message = NotificationMessage::new("test", Urgency::Low);
        assert!(channel.should_send(&message));
    }

    #[test]
    fn test_truncate_summary() {
        assert_eq!(truncate_summary("short", 10), "short");
        assert_eq!(truncate_summary("this is a long message", 10), "this is...");
    }

    #[test]
    fn test_build_record_extracts_payload_fields() {
        let payload = serde_json::json!({
            "type": "cam_notification",
            "project": "/workspace/myproject",
            "event": {"tool_name": "Bash", "tool_input": {"command": "ls"}},
            "terminal_snapshot": "$ ls\nfile1",
            "risk_level": "LOW"
        });
        let message = NotificationMessage::new("test content", Urgency::High)
            .with_agent_id("cam-123")
            .with_payload(payload)
            .with_metadata(crate::notification::channel::MessageMetadata {
                event_type: "permission_request".to_string(),
                project: Some("/workspace/myproject".to_string()),
                timestamp: None,
            });

        let record = build_record(&message);
        assert_eq!(record.project, Some("/workspace/myproject".to_string()));
        assert_eq!(record.risk_level, Some("LOW".to_string()));
        assert!(record.event_detail.is_some());
        assert!(record.terminal_snapshot.is_some());
    }

    #[test]
    fn test_build_record_without_payload() {
        let message = NotificationMessage::new("test", Urgency::Low);
        let record = build_record(&message);
        assert!(record.project.is_none());
        assert!(record.event_detail.is_none());
        assert!(record.terminal_snapshot.is_none());
        assert!(record.risk_level.is_none());
    }
}
