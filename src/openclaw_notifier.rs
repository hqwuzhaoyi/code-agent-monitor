//! OpenClaw 通知模块 - 通过 openclaw CLI 发送事件到 channel 或 agent
//!
//! 通知路由策略：
//! - HIGH/MEDIUM urgency → 直接发送到 channel（绕过 Agent 决策）
//! - LOW urgency → 发送给 Agent（让 Agent 汇总或选择性转发）

use anyhow::Result;
use std::process::Command;
use std::fs;

/// Channel 配置
#[derive(Debug, Clone)]
pub struct ChannelConfig {
    /// channel 类型: telegram, whatsapp, discord, slack 等
    pub channel: String,
    /// 目标: chat_id, phone number, channel id 等
    pub target: String,
}

/// OpenClaw 通知器
pub struct OpenclawNotifier {
    /// openclaw 命令路径
    openclaw_cmd: String,
    /// 目标 session id（用于发送给 Agent）
    session_id: String,
    /// Channel 配置（用于直接发送）
    channel_config: Option<ChannelConfig>,
    /// 是否为 dry-run 模式（只打印不发送）
    dry_run: bool,
}

impl OpenclawNotifier {
    /// 创建新的通知器
    pub fn new() -> Self {
        let channel_config = Self::detect_channel();
        Self {
            openclaw_cmd: Self::find_openclaw_path(),
            session_id: "main".to_string(),
            channel_config,
            dry_run: false,
        }
    }

    /// 创建指定 session 的通知器
    pub fn with_session(session_id: &str) -> Self {
        let channel_config = Self::detect_channel();
        Self {
            openclaw_cmd: Self::find_openclaw_path(),
            session_id: session_id.to_string(),
            channel_config,
            dry_run: false,
        }
    }

    /// 设置 dry-run 模式
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// 从 OpenClaw 配置自动检测 channel
    /// 按优先级检测: telegram > whatsapp > discord > slack > 其他
    fn detect_channel() -> Option<ChannelConfig> {
        let config_path = dirs::home_dir()?.join(".openclaw/openclaw.json");
        let content = fs::read_to_string(&config_path).ok()?;
        let config: serde_json::Value = serde_json::from_str(&content).ok()?;
        let channels = config.get("channels")?;

        // 按优先级尝试检测各个 channel
        // 1. Telegram
        if let Some(target) = Self::extract_telegram_target(channels) {
            return Some(ChannelConfig {
                channel: "telegram".to_string(),
                target,
            });
        }

        // 2. WhatsApp
        if let Some(target) = Self::extract_allow_from(channels, "whatsapp") {
            return Some(ChannelConfig {
                channel: "whatsapp".to_string(),
                target,
            });
        }

        // 3. Discord
        if let Some(target) = Self::extract_default_channel(channels, "discord") {
            return Some(ChannelConfig {
                channel: "discord".to_string(),
                target,
            });
        }

        // 4. Slack
        if let Some(target) = Self::extract_default_channel(channels, "slack") {
            return Some(ChannelConfig {
                channel: "slack".to_string(),
                target,
            });
        }

        // 5. Signal
        if let Some(target) = Self::extract_allow_from(channels, "signal") {
            return Some(ChannelConfig {
                channel: "signal".to_string(),
                target,
            });
        }

        None
    }

    /// 提取 Telegram target (chat_id)
    fn extract_telegram_target(channels: &serde_json::Value) -> Option<String> {
        let allow_from = channels
            .get("telegram")?
            .get("allowFrom")?
            .as_array()?
            .first()?;

        // 处理字符串或数字类型
        if let Some(s) = allow_from.as_str() {
            return Some(s.to_string());
        }
        if let Some(n) = allow_from.as_i64() {
            return Some(n.to_string());
        }
        None
    }

    /// 提取 allowFrom 数组的第一个元素
    fn extract_allow_from(channels: &serde_json::Value, channel_name: &str) -> Option<String> {
        let allow_from = channels
            .get(channel_name)?
            .get("allowFrom")?
            .as_array()?
            .first()?;

        if let Some(s) = allow_from.as_str() {
            return Some(s.to_string());
        }
        if let Some(n) = allow_from.as_i64() {
            return Some(n.to_string());
        }
        None
    }

    /// 提取 defaultChannel
    fn extract_default_channel(channels: &serde_json::Value, channel_name: &str) -> Option<String> {
        channels
            .get(channel_name)?
            .get("defaultChannel")?
            .as_str()
            .map(|s| s.to_string())
    }

    /// 查找 openclaw 可执行文件路径
    fn find_openclaw_path() -> String {
        let possible_paths = [
            "/Users/admin/.volta/bin/openclaw",
            "/opt/homebrew/bin/openclaw",
            "/usr/local/bin/openclaw",
            "openclaw",
        ];

        for path in possible_paths {
            if std::path::Path::new(path).exists() || path == "openclaw" {
                return path.to_string();
            }
        }

        "openclaw".to_string()
    }

    /// 格式化事件消息
    pub fn format_event(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> String {
        // 尝试解析 JSON context 获取更多信息
        let json: Option<serde_json::Value> = serde_json::from_str(context).ok();

        match event_type {
            "permission_request" => {
                // 提取工具名和输入
                let tool_name = json.as_ref()
                    .and_then(|j| j.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let tool_input = json.as_ref()
                    .and_then(|j| j.get("tool_input"))
                    .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                    .unwrap_or_default();
                let cwd = json.as_ref()
                    .and_then(|j| j.get("cwd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                format!(
                    "🔐 [CAM] {} 请求权限\n\n工具: {}\n目录: {}\n参数:\n```\n{}\n```\n\n请回复: 1=允许, 2=允许并记住, 3=拒绝",
                    agent_id, tool_name, cwd, tool_input
                )
            }
            "notification" => {
                let message = json.as_ref()
                    .and_then(|j| j.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let notification_type = json.as_ref()
                    .and_then(|j| j.get("notification_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if notification_type == "idle_prompt" {
                    format!("⏸️ [CAM] {} 等待输入\n\n{}", agent_id, message)
                } else if notification_type == "permission_prompt" {
                    format!("🔐 [CAM] {} 需要权限确认\n\n{}\n\n请回复: 1=允许, 2=允许并记住, 3=拒绝", agent_id, message)
                } else {
                    format!("📢 [CAM] {} 通知\n\n{}", agent_id, message)
                }
            }
            "session_start" => {
                let cwd = json.as_ref()
                    .and_then(|j| j.get("cwd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("🚀 [CAM] {} 已启动\n\n目录: {}", agent_id, cwd)
            }
            "session_end" | "stop" => {
                let cwd = json.as_ref()
                    .and_then(|j| j.get("cwd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("✅ [CAM] {} 已停止\n\n目录: {}", agent_id, cwd)
            }
            "WaitingForInput" => {
                format!(
                    "⏸️ [CAM] {} 等待输入\n\n类型: {}\n上下文:\n---\n{}\n---\n\n请问如何响应？",
                    agent_id, pattern_or_path, context
                )
            }
            "Error" => {
                format!(
                    "❌ [CAM] {} 发生错误\n\n错误信息:\n---\n{}\n---\n\n请问如何处理？",
                    agent_id, context
                )
            }
            "AgentExited" => {
                let last_output = if context.is_empty() {
                    String::new()
                } else {
                    format!("\n\n最后输出:\n---\n{}\n---", context)
                };
                format!(
                    "✅ [CAM] {} 已退出\n\n项目: {}{}",
                    agent_id, pattern_or_path, last_output
                )
            }
            _ => format!("[CAM] {} - {}: {}", agent_id, event_type, context),
        }
    }

    /// 判断事件是否需要用户关注（用于提示 OpenClaw agent）
    ///
    /// 20 个 AI 并行时的关注优先级:
    /// - HIGH: 必须立即响应（权限请求、错误）→ 阻塞任务进度
    /// - MEDIUM: 需要知道（完成、空闲）→ 可以分配新任务
    /// - LOW: 可选（启动）→ 通常不需要通知
    fn get_urgency(event_type: &str, context: &str) -> &'static str {
        match event_type {
            // 权限请求必须转发 - 阻塞任务进度
            "permission_request" => "HIGH",
            // notification 类型需要检查具体类型
            "notification" => {
                let json: Option<serde_json::Value> = serde_json::from_str(context).ok();
                let notification_type = json.as_ref()
                    .and_then(|j| j.get("notification_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match notification_type {
                    "permission_prompt" => "HIGH",  // 权限确认
                    "idle_prompt" => "MEDIUM",      // 空闲等待
                    _ => "LOW"
                }
            }
            // 错误必须转发 - 需要干预
            "Error" => "HIGH",
            // 等待输入必须转发
            "WaitingForInput" => "HIGH",
            // Agent 停止/完成 - 需要知道，可以分配新任务
            "stop" | "session_end" => "MEDIUM",
            // 启动通知 - 可选
            "session_start" => "LOW",
            // 其他
            _ => "LOW",
        }
    }

    /// 发送事件到 channel 或 agent
    /// HIGH/MEDIUM urgency → 直接发送到 channel
    /// LOW urgency → 发送给 Agent
    pub fn send_event(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> Result<()> {
        let message = self.format_event(agent_id, event_type, pattern_or_path, context);
        let urgency = Self::get_urgency(event_type, context);

        match urgency {
            "HIGH" | "MEDIUM" => {
                if self.channel_config.is_some() {
                    // 直接发送到 channel
                    self.send_direct(&message)
                } else {
                    // Fallback: 没有配置 channel，发给 Agent
                    let wrapped = self.wrap_for_agent(&message, urgency, event_type, agent_id);
                    self.send_to_agent(&wrapped)
                }
            }
            _ => {
                // LOW urgency: 发给 Agent
                let wrapped = self.wrap_for_agent(&message, urgency, event_type, agent_id);
                self.send_to_agent(&wrapped)
            }
        }
    }

    /// 直接发送消息到 channel
    fn send_direct(&self, message: &str) -> Result<()> {
        let config = self.channel_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No channel configured"))?;

        if self.dry_run {
            eprintln!("[DRY-RUN] Would send to channel={} target={}", config.channel, config.target);
            eprintln!("[DRY-RUN] Message: {}", message);
            return Ok(());
        }

        let result = Command::new(&self.openclaw_cmd)
            .args([
                "message", "send",
                "--channel", &config.channel,
                "--target", &config.target,
                "--message", message,
            ])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("OpenClaw 直接发送失败: {}", stderr);
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("无法执行 OpenClaw message send: {}", e);
                Err(e.into())
            }
        }
    }

    /// 发送消息给 Agent
    fn send_to_agent(&self, message: &str) -> Result<()> {
        if self.dry_run {
            eprintln!("[DRY-RUN] Would send to agent session={}", self.session_id);
            eprintln!("[DRY-RUN] Message: {}", message);
            return Ok(());
        }

        let result = Command::new(&self.openclaw_cmd)
            .args([
                "agent",
                "--session-id",
                &self.session_id,
                "--message",
                message,
            ])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("OpenClaw Agent 发送失败: {}", stderr);
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("无法执行 OpenClaw agent: {}", e);
                Err(e.into())
            }
        }
    }

    /// 为 Agent 包装消息（添加元数据）
    fn wrap_for_agent(&self, message: &str, urgency: &str, event_type: &str, agent_id: &str) -> String {
        format!(
            "{}\n\n---\n[CAM_META] urgency={} event_type={} agent_id={}",
            message, urgency, event_type, agent_id
        )
    }

    /// 发送消息到 clawdbot (保留兼容性)
    pub fn send_message(&self, message: &str) -> Result<()> {
        self.send_to_agent(message)
    }
}

impl Default for OpenclawNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_urgency_high() {
        assert_eq!(OpenclawNotifier::get_urgency("permission_request", ""), "HIGH");
        assert_eq!(OpenclawNotifier::get_urgency("Error", ""), "HIGH");
        assert_eq!(OpenclawNotifier::get_urgency("WaitingForInput", ""), "HIGH");

        // notification with permission_prompt
        let context = r#"{"notification_type": "permission_prompt"}"#;
        assert_eq!(OpenclawNotifier::get_urgency("notification", context), "HIGH");
    }

    #[test]
    fn test_get_urgency_medium() {
        assert_eq!(OpenclawNotifier::get_urgency("stop", ""), "MEDIUM");
        assert_eq!(OpenclawNotifier::get_urgency("session_end", ""), "MEDIUM");

        // notification with idle_prompt
        let context = r#"{"notification_type": "idle_prompt"}"#;
        assert_eq!(OpenclawNotifier::get_urgency("notification", context), "MEDIUM");
    }

    #[test]
    fn test_get_urgency_low() {
        assert_eq!(OpenclawNotifier::get_urgency("session_start", ""), "LOW");
        assert_eq!(OpenclawNotifier::get_urgency("unknown_event", ""), "LOW");

        // notification with unknown type
        let context = r#"{"notification_type": "other"}"#;
        assert_eq!(OpenclawNotifier::get_urgency("notification", context), "LOW");
    }

    #[test]
    fn test_wrap_for_agent() {
        let notifier = OpenclawNotifier::new();
        let wrapped = notifier.wrap_for_agent("Test message", "HIGH", "Error", "cam-123");

        assert!(wrapped.contains("Test message"));
        assert!(wrapped.contains("[CAM_META]"));
        assert!(wrapped.contains("urgency=HIGH"));
        assert!(wrapped.contains("event_type=Error"));
        assert!(wrapped.contains("agent_id=cam-123"));
    }

    #[test]
    fn test_format_waiting_event() {
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
            "cam-1234567890",
            "WaitingForInput",
            "Confirmation",
            "Do you want to continue? [Y/n]",
        );

        assert!(message.contains("cam-1234567890"));
        assert!(message.contains("等待输入"));
        assert!(message.contains("[Y/n]"));
    }

    #[test]
    fn test_format_error_event() {
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
            "cam-1234567890",
            "Error",
            "",
            "API rate limit exceeded",
        );

        assert!(message.contains("错误"));
        assert!(message.contains("API rate limit"));
    }

    #[test]
    fn test_format_exited_event() {
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
            "cam-1234567890",
            "AgentExited",
            "/workspace/myapp",
            "",
        );

        assert!(message.contains("已退出"));
        assert!(message.contains("/workspace/myapp"));
    }
}
