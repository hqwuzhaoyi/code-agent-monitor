//! Dashboard 渠道（通过 system event 发送结构化 payload）

use crate::notification::channel::{NotificationChannel, NotificationMessage, SendResult, urgency_meets_threshold};
use crate::notification::urgency::Urgency;
use anyhow::Result;
use std::process::Command;
use tracing::{info, error};

/// Dashboard 渠道配置
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// OpenClaw 命令路径
    pub openclaw_cmd: String,
    /// 最低发送 urgency
    pub min_urgency: Urgency,
}

/// Dashboard 渠道（通过 system event 发送结构化 payload）
pub struct DashboardChannel {
    config: DashboardConfig,
}

impl DashboardChannel {
    pub fn new(config: DashboardConfig) -> Self {
        Self { config }
    }
}

impl NotificationChannel for DashboardChannel {
    fn name(&self) -> &str {
        "dashboard"
    }

    fn should_send(&self, message: &NotificationMessage) -> bool {
        // Dashboard 只接收有 payload 的消息
        if message.payload.is_none() {
            return false;
        }

        urgency_meets_threshold(message.urgency, self.config.min_urgency)
    }

    fn send(&self, message: &NotificationMessage) -> Result<SendResult> {
        if !self.should_send(message) {
            return Ok(SendResult::Skipped("no payload or urgency too low".to_string()));
        }

        let payload = message.payload.as_ref().unwrap();
        let payload_text = payload.to_string();

        let output = Command::new(&self.config.openclaw_cmd)
            .args([
                "system", "event",
                "--text", &payload_text,
                "--mode", "now",
            ])
            .output()?;

        if output.status.success() {
            info!(
                channel = "dashboard",
                agent_id = ?message.agent_id,
                "System event sent successfully"
            );
            Ok(SendResult::Sent)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                channel = "dashboard",
                error = %stderr,
                "Failed to send system event"
            );
            Ok(SendResult::Failed(stderr.to_string()))
        }
    }

    fn send_async(&self, message: &NotificationMessage) -> Result<()> {
        if !self.should_send(message) {
            return Ok(());
        }

        let payload = message.payload.as_ref().unwrap();
        let payload_text = payload.to_string();

        // 使用 spawn() 异步发送，不阻塞调用方
        Command::new(&self.config.openclaw_cmd)
            .args([
                "system", "event",
                "--text", &payload_text,
                "--mode", "now",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        Ok(())
    }
}
