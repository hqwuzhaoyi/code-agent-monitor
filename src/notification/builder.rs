//! 通知系统构建器 - 自动检测并配置渠道

use super::channel::NotificationMessage;
use super::channels::dashboard::{DashboardChannel, DashboardConfig};
use super::channels::local_file::LocalFileChannel;
use super::dispatcher::NotificationDispatcher;
use super::urgency::Urgency;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

/// 通知系统构建器 - 自动检测并配置渠道
pub struct NotificationBuilder {
    openclaw_cmd: String,
    dry_run: bool,
    enable_dashboard: bool,
}

impl NotificationBuilder {
    pub fn new() -> Self {
        Self {
            openclaw_cmd: Self::find_openclaw_path(),
            dry_run: false,
            enable_dashboard: true,
        }
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

    /// 构建 NotificationDispatcher
    pub fn build(self) -> Result<NotificationDispatcher> {
        let mut dispatcher = NotificationDispatcher::new().with_dry_run(self.dry_run);

        // Dashboard（总是启用，除非明确禁用）
        if self.enable_dashboard {
            info!(channel = "dashboard", "Enabling Dashboard channel");
            let dashboard = DashboardChannel::new(DashboardConfig {
                openclaw_cmd: self.openclaw_cmd.clone(),
                min_urgency: Urgency::Medium,
            });
            dispatcher.register_channel(Arc::new(dashboard));
        }

        // LocalFile（总是启用，记录所有通知）
        info!(channel = "local_file", "Enabling LocalFile channel");
        dispatcher.register_channel(Arc::new(LocalFileChannel::new()));

        Ok(dispatcher)
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
            .dry_run(true)
            .enable_dashboard(false);

        assert!(builder.dry_run);
        assert!(!builder.enable_dashboard);
    }
}
