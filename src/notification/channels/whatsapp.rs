//! WhatsApp 渠道（基于 OpenclawMessageChannel）

use super::openclaw_message::{OpenclawMessageChannel, OpenclawMessageConfig};
use crate::notification::urgency::Urgency;

/// WhatsApp 渠道配置
#[derive(Debug, Clone)]
pub struct WhatsAppConfig {
    /// Phone number (E.164 format)
    pub phone_number: String,
    /// OpenClaw 命令路径
    pub openclaw_cmd: String,
    /// 最低发送 urgency
    pub min_urgency: Urgency,
}

/// WhatsApp 渠道
pub type WhatsAppChannel = OpenclawMessageChannel;

impl WhatsAppChannel {
    /// 创建 WhatsApp 渠道
    pub fn whatsapp(config: WhatsAppConfig) -> Self {
        Self::new(OpenclawMessageConfig {
            channel_type: "whatsapp".to_string(),
            target: config.phone_number,
            openclaw_cmd: config.openclaw_cmd,
            min_urgency: config.min_urgency,
        })
    }
}
