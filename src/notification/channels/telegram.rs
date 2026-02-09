//! Telegram 渠道（基于 OpenclawMessageChannel）

use super::openclaw_message::{OpenclawMessageChannel, OpenclawMessageConfig};
use crate::notification::urgency::Urgency;

/// Telegram 渠道配置
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    /// Chat ID
    pub chat_id: String,
    /// OpenClaw 命令路径
    pub openclaw_cmd: String,
    /// 最低发送 urgency
    pub min_urgency: Urgency,
}

/// Telegram 渠道
pub type TelegramChannel = OpenclawMessageChannel;

impl TelegramChannel {
    /// 创建 Telegram 渠道
    pub fn telegram(config: TelegramConfig) -> Self {
        Self::new(OpenclawMessageConfig {
            channel_type: "telegram".to_string(),
            target: config.chat_id,
            openclaw_cmd: config.openclaw_cmd,
            min_urgency: config.min_urgency,
        })
    }
}
