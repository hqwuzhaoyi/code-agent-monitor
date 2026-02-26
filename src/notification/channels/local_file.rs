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
        let record = NotificationRecord {
            ts: Utc::now(),
            agent_id: message
                .agent_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            urgency: message.urgency,
            event: message.metadata.event_type.clone(),
            summary: truncate_summary(&message.content, 100),
        };

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
}
