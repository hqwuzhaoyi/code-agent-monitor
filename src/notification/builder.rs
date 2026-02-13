//! 通知系统构建器 - 自动检测并配置渠道

use super::channel::NotificationMessage;
use super::dispatcher::NotificationDispatcher;
use super::channels::dashboard::{DashboardChannel, DashboardConfig};
use super::channels::openclaw_message::{OpenclawMessageChannel, OpenclawMessageConfig};
use super::urgency::Urgency;
use anyhow::Result;
use std::fs;
use std::sync::Arc;
use tracing::info;

/// 通知系统构建器 - 自动检测并配置渠道
pub struct NotificationBuilder {
    openclaw_cmd: String,
    min_urgency: Urgency,
    dry_run: bool,
    enable_dashboard: bool,
}

impl NotificationBuilder {
    pub fn new() -> Self {
        Self {
            openclaw_cmd: Self::find_openclaw_path(),
            min_urgency: Urgency::Medium,
            dry_run: false,
            enable_dashboard: true,
        }
    }

    /// 设置最低 urgency
    pub fn min_urgency(mut self, urgency: Urgency) -> Self {
        self.min_urgency = urgency;
        self
    }

    /// 设置 dry-run 模式
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// 是否启用 Dashboard 渠道
    pub fn enable_dashboard(mut self, enable: bool) -> Self {
        self.enable_dashboard = enable;
        self
    }

    /// 设置 OpenClaw 命令路径
    pub fn openclaw_cmd(mut self, cmd: impl Into<String>) -> Self {
        self.openclaw_cmd = cmd.into();
        self
    }

    /// 构建 NotificationDispatcher（自动检测渠道）
    pub fn build(self) -> Result<NotificationDispatcher> {
        let mut dispatcher = NotificationDispatcher::new().with_dry_run(self.dry_run);

        // 从 OpenClaw 配置自动检测渠道
        if let Some(config) = self.detect_openclaw_config()? {
            let channels = config.get("channels");

            // 1. Telegram
            if let Some(chat_id) = channels.and_then(Self::extract_telegram_target) {
                info!(channel = "telegram", target = %chat_id, "Detected Telegram channel");
                let channel = OpenclawMessageChannel::new(OpenclawMessageConfig {
                    channel_type: "telegram".to_string(),
                    target: chat_id,
                    openclaw_cmd: self.openclaw_cmd.clone(),
                    min_urgency: self.min_urgency,
                });
                dispatcher.register_channel(Arc::new(channel));
            }

            // 2. WhatsApp
            if let Some(phone) = channels.and_then(|c| Self::extract_allow_from(c, "whatsapp")) {
                info!(channel = "whatsapp", target = %phone, "Detected WhatsApp channel");
                let channel = OpenclawMessageChannel::new(OpenclawMessageConfig {
                    channel_type: "whatsapp".to_string(),
                    target: phone,
                    openclaw_cmd: self.openclaw_cmd.clone(),
                    min_urgency: self.min_urgency,
                });
                dispatcher.register_channel(Arc::new(channel));
            }

            // 3. Discord
            if let Some(channel_id) = channels.and_then(|c| Self::extract_default_channel(c, "discord")) {
                info!(channel = "discord", target = %channel_id, "Detected Discord channel");
                let channel = OpenclawMessageChannel::new(OpenclawMessageConfig {
                    channel_type: "discord".to_string(),
                    target: channel_id,
                    openclaw_cmd: self.openclaw_cmd.clone(),
                    min_urgency: self.min_urgency,
                });
                dispatcher.register_channel(Arc::new(channel));
            }

            // 4. Slack
            if let Some(channel_id) = channels.and_then(|c| Self::extract_default_channel(c, "slack")) {
                info!(channel = "slack", target = %channel_id, "Detected Slack channel");
                let channel = OpenclawMessageChannel::new(OpenclawMessageConfig {
                    channel_type: "slack".to_string(),
                    target: channel_id,
                    openclaw_cmd: self.openclaw_cmd.clone(),
                    min_urgency: self.min_urgency,
                });
                dispatcher.register_channel(Arc::new(channel));
            }

            // 5. Signal
            if let Some(phone) = channels.and_then(|c| Self::extract_allow_from(c, "signal")) {
                info!(channel = "signal", target = %phone, "Detected Signal channel");
                let channel = OpenclawMessageChannel::new(OpenclawMessageConfig {
                    channel_type: "signal".to_string(),
                    target: phone,
                    openclaw_cmd: self.openclaw_cmd.clone(),
                    min_urgency: self.min_urgency,
                });
                dispatcher.register_channel(Arc::new(channel));
            }
        }

        // Dashboard（总是启用，除非明确禁用）
        if self.enable_dashboard {
            info!(channel = "dashboard", "Enabling Dashboard channel");
            let dashboard = DashboardChannel::new(DashboardConfig {
                openclaw_cmd: self.openclaw_cmd.clone(),
                min_urgency: Urgency::Medium,
            });
            dispatcher.register_channel(Arc::new(dashboard));
        }

        Ok(dispatcher)
    }

    /// 检测 OpenClaw 配置
    fn detect_openclaw_config(&self) -> Result<Option<serde_json::Value>> {
        let config_path = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?
            .join(".openclaw/openclaw.json");

        if !config_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&config_path)?;
        let config: serde_json::Value = serde_json::from_str(&content)?;
        Ok(Some(config))
    }

    /// 提取 Telegram target
    fn extract_telegram_target(channels: &serde_json::Value) -> Option<String> {
        let allow_from = channels.get("telegram")?.get("allowFrom")?.as_array()?;

        for entry in allow_from {
            if let Some(s) = entry.as_str() {
                let s = s.trim();
                if !s.is_empty() && s != "*" {
                    return Some(s.to_string());
                }
            }
            if let Some(n) = entry.as_i64() {
                return Some(n.to_string());
            }
        }
        None
    }

    /// 提取 allowFrom 字段
    fn extract_allow_from(channels: &serde_json::Value, channel_name: &str) -> Option<String> {
        let allow_from = channels.get(channel_name)?.get("allowFrom")?.as_array()?;

        for entry in allow_from {
            if let Some(s) = entry.as_str() {
                let s = s.trim();
                if !s.is_empty() && s != "*" {
                    return Some(s.to_string());
                }
            }
        }
        None
    }

    /// 提取 defaultChannel 字段
    fn extract_default_channel(channels: &serde_json::Value, channel_name: &str) -> Option<String> {
        channels
            .get(channel_name)?
            .get("defaultChannel")?
            .as_str()
            .map(|s| s.to_string())
    }

    /// 查找 openclaw 可执行文件路径
    fn find_openclaw_path() -> String {
        // 优先使用 PATH 中的 openclaw
        if let Ok(output) = std::process::Command::new("which").arg("openclaw").output() {
            if output.status.success() {
                if let Ok(path) = String::from_utf8(output.stdout) {
                    let path = path.trim();
                    if !path.is_empty() {
                        return path.to_string();
                    }
                }
            }
        }

        // Hook 环境可能没有完整 PATH，检查常见位置
        if let Some(home) = dirs::home_dir() {
            let volta_path = home.join(".volta/bin/openclaw");
            if volta_path.exists() {
                return volta_path.to_string_lossy().to_string();
            }

            let local_bin = home.join(".local/bin/openclaw");
            if local_bin.exists() {
                return local_bin.to_string_lossy().to_string();
            }
        }

        // 检查系统路径
        for path in &["/usr/local/bin/openclaw", "/opt/homebrew/bin/openclaw"] {
            if std::path::Path::new(path).exists() {
                return path.to_string();
            }
        }

        // 回退到默认
        "openclaw".to_string()
    }
}

impl Default for NotificationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 便捷函数：创建消息并发送
pub fn send_notification(
    content: impl Into<String>,
    urgency: Urgency,
    agent_id: Option<&str>,
    payload: Option<serde_json::Value>,
) -> Result<()> {
    let dispatcher = NotificationBuilder::new().build()?;

    let mut message = NotificationMessage::new(content, urgency);
    if let Some(id) = agent_id {
        message = message.with_agent_id(id);
    }
    if let Some(p) = payload {
        message = message.with_payload(p);
    }

    dispatcher.send_async(&message)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_default() {
        let builder = NotificationBuilder::new();
        assert!(!builder.dry_run);
        assert!(builder.enable_dashboard);
    }

    #[test]
    fn test_builder_chain() {
        let builder = NotificationBuilder::new()
            .min_urgency(Urgency::High)
            .dry_run(true)
            .enable_dashboard(false);

        assert!(builder.dry_run);
        assert!(!builder.enable_dashboard);
    }

    #[test]
    fn test_extract_telegram_target() {
        let channels = serde_json::json!({
            "telegram": {
                "allowFrom": ["123456789"]
            }
        });
        let target = NotificationBuilder::extract_telegram_target(&channels);
        assert_eq!(target, Some("123456789".to_string()));
    }

    #[test]
    fn test_extract_telegram_target_number() {
        let channels = serde_json::json!({
            "telegram": {
                "allowFrom": [123456789]
            }
        });
        let target = NotificationBuilder::extract_telegram_target(&channels);
        assert_eq!(target, Some("123456789".to_string()));
    }

    #[test]
    fn test_extract_telegram_target_skips_wildcard() {
        let channels = serde_json::json!({
            "telegram": {
                "allowFrom": ["*", "123456789"]
            }
        });
        let target = NotificationBuilder::extract_telegram_target(&channels);
        assert_eq!(target, Some("123456789".to_string()));
    }
}
