//! OpenClaw message send 通用渠道
//!
//! 支持所有 OpenClaw 支持的渠道：telegram, whatsapp, discord, slack, signal 等

use crate::notification::channel::{NotificationChannel, NotificationMessage, SendResult, urgency_meets_threshold};
use crate::notification::urgency::Urgency;
use anyhow::Result;
use std::process::Command;
use tracing::{info, error};

/// OpenClaw message send 渠道配置
#[derive(Debug, Clone)]
pub struct OpenclawMessageConfig {
    /// 渠道类型: telegram, whatsapp, discord, slack, signal 等
    pub channel_type: String,
    /// 目标（chat_id, phone number, channel id 等）
    pub target: String,
    /// OpenClaw 命令路径
    pub openclaw_cmd: String,
    /// 最低发送 urgency
    pub min_urgency: Urgency,
}

/// OpenClaw message send 通用渠道
pub struct OpenclawMessageChannel {
    config: OpenclawMessageConfig,
}

impl OpenclawMessageChannel {
    pub fn new(config: OpenclawMessageConfig) -> Self {
        Self { config }
    }

    /// 格式化消息（添加 agent_id 标记）
    fn format_message(&self, message: &NotificationMessage) -> String {
        if let Some(agent_id) = &message.agent_id {
            // 使用 markdown monospace 格式，方便用户点击复制
            format!("{} `{}`", message.content, agent_id)
        } else {
            message.content.clone()
        }
    }
}

impl NotificationChannel for OpenclawMessageChannel {
    fn name(&self) -> &str {
        &self.config.channel_type
    }

    fn should_send(&self, message: &NotificationMessage) -> bool {
        urgency_meets_threshold(message.urgency, self.config.min_urgency)
    }

    fn send(&self, message: &NotificationMessage) -> Result<SendResult> {
        if !self.should_send(message) {
            return Ok(SendResult::Skipped(format!(
                "urgency {:?} below threshold {:?}",
                message.urgency, self.config.min_urgency
            )));
        }

        let formatted = self.format_message(message);

        let output = Command::new(&self.config.openclaw_cmd)
            .args([
                "message", "send",
                "--channel", &self.config.channel_type,
                "--target", &self.config.target,
                "--message", &formatted,
            ])
            .output()?;

        if output.status.success() {
            info!(
                channel = %self.config.channel_type,
                target = %self.config.target,
                agent_id = ?message.agent_id,
                "Message sent successfully"
            );
            Ok(SendResult::Sent)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                channel = %self.config.channel_type,
                error = %stderr,
                "Failed to send message"
            );
            Ok(SendResult::Failed(stderr.to_string()))
        }
    }

    fn send_async(&self, message: &NotificationMessage) -> Result<()> {
        if !self.should_send(message) {
            return Ok(());
        }

        let formatted = self.format_message(message);

        // 使用 spawn() 异步发送，不阻塞调用方
        Command::new(&self.config.openclaw_cmd)
            .args([
                "message", "send",
                "--channel", &self.config.channel_type,
                "--target", &self.config.target,
                "--message", &formatted,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        Ok(())
    }
}
