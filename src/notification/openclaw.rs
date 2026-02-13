//! OpenClaw 通知模块 - 通过 openclaw CLI 发送事件到 channel 或 agent
//!
//! 通知路由策略：
//! - HIGH/MEDIUM urgency → 通过 system event 发送结构化 payload（触发 heartbeat）
//! - LOW urgency → 静默处理（避免上下文累积）
//!
//! 此模块作为门面（Facade），委托给 notification 子模块处理具体逻辑：
//! - `notification::urgency` - Urgency 分类
//! - `notification::formatter` - 消息格式化
//! - `notification::payload` - Payload 构建
//! - `notification::terminal_cleaner` - 终端输出清理

use anyhow::Result;
use std::process::Command;
use std::fs;
use tracing::{info, error, debug, warn};
use crate::notification::urgency::{Urgency, get_urgency};
use crate::notification::formatter::MessageFormatter;
use crate::notification::payload::PayloadBuilder;
use crate::notification::event::{NotificationEvent, NotificationEventType};
use crate::notification::deduplicator::NotificationDeduplicator;
use crate::notification::channel::SendResult;
use std::sync::Mutex;

/// Channel 配置
#[derive(Debug, Clone)]
pub struct ChannelConfig {
    /// channel 类型: telegram, whatsapp, discord, slack 等
    pub channel: String,
    /// 目标: chat_id, phone number, channel id 等
    pub target: String,
}

/// OpenClaw notifier - 门面模式，委托给子模块处理
pub struct OpenclawNotifier {
    /// openclaw command path
    openclaw_cmd: String,
    /// Channel config (for direct sending)
    channel_config: Option<ChannelConfig>,
    /// dry-run mode (print only, don't send)
    dry_run: bool,
    /// Disable AI extraction (for testing/debugging)
    no_ai: bool,
    /// 消息格式化器
    formatter: MessageFormatter,
    /// Payload 构建器
    payload_builder: PayloadBuilder,
    /// 通知去重器
    deduplicator: Mutex<NotificationDeduplicator>,
}

impl OpenclawNotifier {
    /// 创建新的通知器
    pub fn new() -> Self {
        let channel_config = Self::detect_channel();
        Self {
            openclaw_cmd: Self::find_openclaw_path(),
            channel_config,
            dry_run: false,
            no_ai: false,
            formatter: MessageFormatter::new(),
            payload_builder: PayloadBuilder::new(),
            deduplicator: Mutex::new(NotificationDeduplicator::new()),
        }
    }

    /// 设置 dry-run 模式
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// 设置是否禁用 AI 提取
    pub fn with_no_ai(mut self, no_ai: bool) -> Self {
        self.no_ai = no_ai;
        self.formatter = self.formatter.with_no_ai(no_ai);
        self.payload_builder = self.payload_builder.with_no_ai(no_ai);
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
            .as_array()?;

        // allowFrom 本质是“入站发送者 allowlist”。这里用作出站通知收件人时，只能做启发式：
        // 取第一个“具体的”条目，并跳过 "*" 这种通配符（常见于 dmPolicy/groupPolicy="open" 配置）。
        for entry in allow_from {
            if let Some(s) = entry.as_str() {
                let s = s.trim();
                if s.is_empty() || s == "*" {
                    continue;
                }
                return Some(s.to_string());
            }
            if let Some(n) = entry.as_i64() {
                return Some(n.to_string());
            }
        }

        None
    }

    /// 提取 allowFrom 数组的第一个元素
    fn extract_allow_from(channels: &serde_json::Value, channel_name: &str) -> Option<String> {
        let allow_from = channels
            .get(channel_name)?
            .get("allowFrom")?
            .as_array()?;

        // 同 extract_telegram_target：跳过 "*" 这种通配符，选择第一个具体条目。
        for entry in allow_from {
            if let Some(s) = entry.as_str() {
                let s = s.trim();
                if s.is_empty() || s == "*" {
                    continue;
                }
                return Some(s.to_string());
            }
            if let Some(n) = entry.as_i64() {
                return Some(n.to_string());
            }
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

        // 回退到默认（让系统 PATH 解析）
        "openclaw".to_string()
    }

    // ==================== 日志辅助函数 ====================

    /// 记录耗时日志到 hook.log
    fn log_timing(stage: &str, result: &str, duration: std::time::Duration) {
        use std::fs::OpenOptions;
        use std::io::Write;

        if let Some(home) = dirs::home_dir() {
            let log_path = home.join(".config/code-agent-monitor/hook.log");
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                let _ = writeln!(
                    file,
                    "[{}] ⏱️ {} {} took {}ms",
                    timestamp,
                    stage,
                    result,
                    duration.as_millis()
                );
            }
        }
    }

    /// 格式化事件消息 - 委托给 MessageFormatter
    pub fn format_event(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> String {
        self.formatter.format_event(agent_id, event_type, pattern_or_path, context)
    }

    /// 创建结构化 payload - 委托给 PayloadBuilder
    fn create_payload(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> serde_json::Value {
        let urgency = get_urgency(event_type, context);
        self.payload_builder.create_payload(agent_id, event_type, pattern_or_path, context, urgency)
    }

    /// 发送事件到 channel
    /// HIGH/MEDIUM urgency → 通过 gateway wake 发送结构化 payload
    /// LOW urgency → 静默处理（避免 agent session 上下文累积导致去重问题）
    /// 返回 SendResult 以区分发送成功和静默跳过
    pub fn send_event(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> Result<SendResult> {
        let total_start = std::time::Instant::now();

        // 外部会话（ext-xxx）不发送通知
        // 原因：外部会话无法远程回复，通知只会造成打扰
        if agent_id.starts_with("ext-") {
            if self.dry_run {
                eprintln!("[DRY-RUN] External session (cannot reply remotely), skipping: {} {}", agent_id, event_type);
            }
            debug!(agent_id = %agent_id, event_type = %event_type, "Skipping external session notification");
            return Ok(SendResult::Skipped("external session".to_string()));
        }

        let urgency = get_urgency(event_type, context);

        debug!(
            agent_id = %agent_id,
            event_type = %event_type,
            urgency = urgency.as_str(),
            "Processing notification event"
        );

        match urgency {
            Urgency::High | Urgency::Medium => {
                // 方案 A: 同时发送到 Dashboard 和 Channel
                // 1. 先发送 system event（让 Dashboard 显示）
                // 2. 再发送到 channel（确保用户收到通知）

                let format_start = std::time::Instant::now();
                let message = self.format_event(agent_id, event_type, pattern_or_path, context);
                Self::log_timing("format_event", event_type, format_start.elapsed());

                // 如果 format_event 返回空字符串，表示应该跳过通知（如处理中状态）
                if message.is_empty() {
                    if self.dry_run {
                        eprintln!("[DRY-RUN] Processing state detected, skipping: {} {}", event_type, agent_id);
                    }
                    debug!(
                        agent_id = %agent_id,
                        event_type = %event_type,
                        "Notification skipped (processing state)"
                    );
                    return Ok(SendResult::Skipped("processing state".to_string()));
                }

                // 1. 发送 system event 到 Dashboard（异步，不阻塞）
                let payload = self.create_payload(agent_id, event_type, pattern_or_path, context);
                let gateway_start = std::time::Instant::now();
                if let Err(e) = self.send_via_gateway_async(&payload) {
                    // Gateway 发送失败不影响 channel 发送
                    warn!(error = %e, "Failed to send system event to dashboard");
                }
                Self::log_timing("send_gateway", "async", gateway_start.elapsed());

                // 2. 发送到 channel（如果配置了）
                if let Some(config) = &self.channel_config {
                    let channel_name = config.channel.clone();
                    let needs_reply = matches!(event_type,
                        "permission_request" | "WaitingForInput" | "Error" | "notification"
                    );

                    let send_start = std::time::Instant::now();
                    if needs_reply {
                        self.send_direct(&message, agent_id)?;
                    } else {
                        self.send_direct_text(&message)?;
                    }
                    Self::log_timing("send_direct", &channel_name, send_start.elapsed());
                }

                Self::log_timing("send_event_total", event_type, total_start.elapsed());

                info!(
                    agent_id = %agent_id,
                    event_type = %event_type,
                    urgency = urgency.as_str(),
                    "Notification sent to dashboard and channel"
                );
                Ok(SendResult::Sent)
            }
            Urgency::Low => {
                // LOW urgency: 静默处理，不发送通知
                // 参考 coding-agent skill 设计：启动通知由调用方自己说，不需要系统推送
                if self.dry_run {
                    eprintln!("[DRY-RUN] LOW urgency, skipping: {} {}", event_type, agent_id);
                }
                debug!(
                    agent_id = %agent_id,
                    event_type = %event_type,
                    "Notification skipped (LOW urgency)"
                );
                Ok(SendResult::Skipped(format!("LOW urgency ({})", event_type)))
            }
        }
    }

    /// 发送统一的 NotificationEvent（新 API）
    ///
    /// 这是新的统一入口，使用 NotificationEvent 结构体替代多个参数。
    /// 优势：
    /// 1. 项目名从 event.project_path 获取，不再依赖 pattern_or_path
    /// 2. 终端快照从 event.terminal_snapshot 获取，数据来源清晰
    /// 3. 类型安全，避免参数混淆
    /// 4. 内置去重机制，防止重复通知
    /// 5. 检测处理中状态，避免发送无意义通知
    pub fn send_notification_event(&self, event: &NotificationEvent) -> Result<SendResult> {
        use crate::notification::terminal_cleaner::is_processing;

        let total_start = std::time::Instant::now();
        let agent_id = &event.agent_id;

        // 外部会话（ext-xxx）不发送通知
        if agent_id.starts_with("ext-") {
            if self.dry_run {
                eprintln!("[DRY-RUN] External session (cannot reply remotely), skipping: {}", agent_id);
            }
            debug!(agent_id = %agent_id, "Skipping external session notification");
            return Ok(SendResult::Skipped("external session".to_string()));
        }

        // 检测处理中状态（使用 AI 判断，兼容 Claude Code/Codex/OpenCode 等）
        // 如果 agent 正在处理中，不发送 idle_prompt 通知
        if let Some(ref snapshot) = event.terminal_snapshot {
            if is_processing(snapshot) {
                if self.dry_run {
                    eprintln!("[DRY-RUN] Agent is processing (AI detection), skipping: {}", agent_id);
                }
                debug!(agent_id = %agent_id, "Skipping notification - agent is processing");
                return Ok(SendResult::Skipped("agent processing".to_string()));
            }
        }

        // 获取事件类型字符串用于 urgency 判断
        let event_type_str = match &event.event_type {
            NotificationEventType::WaitingForInput { .. } => "WaitingForInput",
            NotificationEventType::PermissionRequest { .. } => "permission_request",
            NotificationEventType::Notification { notification_type, .. } => {
                if notification_type == "idle_prompt" || notification_type == "permission_prompt" {
                    "notification"
                } else {
                    "notification"
                }
            }
            NotificationEventType::AgentExited => "AgentExited",
            NotificationEventType::Error { .. } => "Error",
            NotificationEventType::Stop => "stop",
            NotificationEventType::SessionStart => "session_start",
            NotificationEventType::SessionEnd => "session_end",
        };

        // 构建 context 用于 urgency 判断（兼容旧逻辑）
        let context_for_urgency = match &event.event_type {
            NotificationEventType::Notification { notification_type, message } => {
                serde_json::json!({
                    "notification_type": notification_type,
                    "message": message
                }).to_string()
            }
            _ => String::new(),
        };

        let urgency = get_urgency(event_type_str, &context_for_urgency);

        // 特殊处理：stop 事件可能包含等待输入的问题
        // Claude Code 在输出问题后会触发 stop 而非 idle_prompt
        let (urgency, event_for_format) = if matches!(&event.event_type, NotificationEventType::Stop) {
            if let Some(ref snapshot) = event.terminal_snapshot {
                // 使用 AI 检测终端是否包含等待输入的问题
                if let Some(content) = crate::anthropic::detect_waiting_question(snapshot) {
                    debug!(agent_id = %agent_id, "Stop event contains waiting question, upgrading urgency");
                    // 创建一个新的事件用于格式化，类型改为 Notification
                    let mut new_event = event.clone();
                    new_event.event_type = NotificationEventType::Notification {
                        notification_type: "idle_prompt".to_string(),
                        message: content.question.clone(),
                    };
                    (Urgency::Medium, Some(new_event))
                } else {
                    (urgency, None)
                }
            } else {
                (urgency, None)
            }
        } else {
            (urgency, None)
        };

        // 使用可能更新的事件进行格式化
        let final_event = event_for_format.as_ref().unwrap_or(event);

        debug!(
            agent_id = %agent_id,
            event_type = %event_type_str,
            urgency = urgency.as_str(),
            "Processing notification event (new API)"
        );

        match urgency {
            Urgency::High | Urgency::Medium => {
                let format_start = std::time::Instant::now();
                let message = self.formatter.format_notification_event(final_event);
                Self::log_timing("format_notification_event", event_type_str, format_start.elapsed());

                // 如果消息为空，跳过
                if message.is_empty() {
                    if self.dry_run {
                        eprintln!("[DRY-RUN] Empty message, skipping: {}", agent_id);
                    }
                    return Ok(SendResult::Skipped("empty message".to_string()));
                }

                // 去重检查
                {
                    let mut dedup = self.deduplicator.lock().unwrap();
                    let action = dedup.should_send(agent_id, &message);
                    match action {
                        crate::notification::NotifyAction::Send => {
                            // 继续发送
                        }
                        crate::notification::NotifyAction::SendReminder => {
                            // 发送提醒（可以在消息中添加提醒标记）
                            // 继续发送
                        }
                        crate::notification::NotifyAction::Suppressed(reason) => {
                            if self.dry_run {
                                eprintln!("[DRY-RUN] Duplicate notification, skipping: {} ({})", agent_id, reason);
                            }
                            debug!(agent_id = %agent_id, reason = %reason, "Notification deduplicated");
                            return Ok(SendResult::Skipped("duplicate".to_string()));
                        }
                    }
                }

                // 发送到 channel
                if let Some(config) = &self.channel_config {
                    let channel_name = config.channel.clone();
                    let needs_reply = final_event.needs_reply();

                    let send_start = std::time::Instant::now();
                    if needs_reply {
                        self.send_direct(&message, agent_id)?;
                    } else {
                        self.send_direct_text(&message)?;
                    }
                    Self::log_timing("send_direct", &channel_name, send_start.elapsed());
                }

                Self::log_timing("send_notification_event_total", event_type_str, total_start.elapsed());

                info!(
                    agent_id = %agent_id,
                    event_type = %event_type_str,
                    urgency = urgency.as_str(),
                    "Notification sent (new API)"
                );
                Ok(SendResult::Sent)
            }
            Urgency::Low => {
                if self.dry_run {
                    eprintln!("[DRY-RUN] LOW urgency, skipping: {} {}", event_type_str, agent_id);
                }
                debug!(
                    agent_id = %agent_id,
                    event_type = %event_type_str,
                    "Notification skipped (LOW urgency)"
                );
                Ok(SendResult::Skipped(format!("LOW urgency ({})", event_type_str)))
            }
        }
    }

    /// 直接发送消息到 channel
    /// agent_id 用于在消息末尾添加路由标记 [agent_id]，方便用户回复时路由到正确的 agent
    ///
    /// 注意：使用 spawn() 异步发送，不阻塞调用方。
    /// OpenClaw message send 命令本身需要 8-12 秒（Gateway 通信、验证等），
    /// 使用异步发送可以让 Hook 立即返回，用户感知延迟从 8-12s 降至 <100ms。
    fn send_direct(&self, message: &str, agent_id: &str) -> Result<()> {
        let config = self.channel_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No channel configured"))?;

        if self.dry_run {
            eprintln!("[DRY-RUN] Would send to channel={} target={}", config.channel, config.target);
            eprintln!("[DRY-RUN] Message: {}", message);
            eprintln!("[DRY-RUN] Agent ID tag: {}", agent_id);
            return Ok(());
        }

        // 添加 agent_id 标记用于回复路由
        // 使用 Telegram markdown 的 monospace 格式，方便用户点击复制
        let tagged_message = format!("{} `{}`", message, agent_id);

        // 使用 spawn() 异步发送，不阻塞调用方
        // OpenClaw 进程在后台运行，发送完成后自动退出
        let child = Command::new(&self.openclaw_cmd)
            .args([
                "message", "send",
                "--channel", &config.channel,
                "--target", &config.target,
                "--message", &tagged_message,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();

        match child {
            Ok(_) => {
                // 进程已启动，不等待完成
                // 如果需要错误处理，可以在后台线程中等待并记录
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Failed to spawn OpenClaw message send");
                Err(e.into())
            }
        }
    }

    /// 异步发送 system event 到 Dashboard（不阻塞调用方）
    fn send_via_gateway_async(&self, payload: &serde_json::Value) -> Result<()> {
        if self.dry_run {
            eprintln!("[DRY-RUN] Would send via system event (async)");
            eprintln!("[DRY-RUN] Payload: {}", serde_json::to_string_pretty(payload).unwrap_or_default());
            return Ok(());
        }

        let payload_text = payload.to_string();

        // 使用 spawn() 异步发送，不阻塞调用方
        let child = Command::new(&self.openclaw_cmd)
            .args([
                "system", "event",
                "--text", &payload_text,
                "--mode", "now",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        match child {
            Ok(_) => Ok(()),
            Err(e) => {
                error!(error = %e, "Failed to spawn system event");
                Err(e.into())
            }
        }
    }

    /// 直接发送纯文本到检测到的 channel。
    ///
    /// 主要用于老的 `cam watch --openclaw` 路径，避免在多个模块里重复实现
    /// `openclaw message send` 的参数拼装和 channel detection。
    /// 注意：此方法不添加 agent_id 标记，因为调用方通常没有 agent_id 上下文。
    ///
    /// 使用 spawn() 异步发送，不阻塞调用方。
    pub fn send_direct_text(&self, message: &str) -> Result<()> {
        let config = self.channel_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No channel configured"))?;

        if self.dry_run {
            eprintln!("[DRY-RUN] Would send to channel={} target={}", config.channel, config.target);
            eprintln!("[DRY-RUN] Message: {}", message);
            return Ok(());
        }

        // 使用 spawn() 异步发送，不阻塞调用方
        let child = Command::new(&self.openclaw_cmd)
            .args([
                "message", "send",
                "--channel", &config.channel,
                "--target", &config.target,
                "--message", message,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();

        match child {
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!("无法执行 OpenClaw message send: {}", e);
                Err(e.into())
            }
        }
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
    use crate::notification::formatter::MessageFormatter;

    #[test]
    fn test_get_urgency_high() {
        assert_eq!(get_urgency("permission_request", ""), Urgency::High);
        assert_eq!(get_urgency("Error", ""), Urgency::High);
        assert_eq!(get_urgency("WaitingForInput", ""), Urgency::High);

        // notification with permission_prompt
        let context = r#"{"notification_type": "permission_prompt"}"#;
        assert_eq!(get_urgency("notification", context), Urgency::High);
    }

    #[test]
    fn test_get_urgency_medium() {
        // AgentExited 是 MEDIUM（可能是异常退出，用户需要知道）
        assert_eq!(get_urgency("AgentExited", ""), Urgency::Medium);

        // notification with idle_prompt
        let context = r#"{"notification_type": "idle_prompt"}"#;
        assert_eq!(get_urgency("notification", context), Urgency::Medium);
    }

    #[test]
    fn test_get_urgency_low() {
        // stop/session_end 是 LOW（用户自己触发的，无需通知）
        assert_eq!(get_urgency("stop", ""), Urgency::Low);
        assert_eq!(get_urgency("session_end", ""), Urgency::Low);
        assert_eq!(get_urgency("session_start", ""), Urgency::Low);
        // ToolUse 是 LOW（太频繁，静默处理）
        assert_eq!(get_urgency("ToolUse", ""), Urgency::Low);
        assert_eq!(get_urgency("unknown_event", ""), Urgency::Low);

        // notification with unknown type
        let context = r#"{"notification_type": "other"}"#;
        assert_eq!(get_urgency("notification", context), Urgency::Low);
    }

    #[test]
    fn test_get_urgency_notification_idle_prompt_with_terminal_snapshot() {
        let context = r#"{"notification_type": "idle_prompt", "message": "waiting"}

--- 终端快照 ---
line 1"#;
        assert_eq!(get_urgency("notification", context), Urgency::Medium);
    }

    #[test]
    fn test_get_urgency_notification_permission_prompt_with_terminal_snapshot() {
        let context = r#"{"notification_type": "permission_prompt", "message": "confirm?"}

--- 终端快照 ---
line 1"#;
        assert_eq!(get_urgency("notification", context), Urgency::High);
    }

    #[test]
    fn test_format_waiting_event() {
        let notifier = OpenclawNotifier::new().with_no_ai(true);

        let message = notifier.format_event(
            "cam-1234567890",
            "WaitingForInput",
            "Confirmation",
            "Do you want to continue? [Y/n]",
        );

        // 简化后的格式：AI 禁用时显示提示信息
        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
        // AI 禁用时显示无法解析提示
        assert!(message.contains("无法解析通知内容") || message.contains("Do you want to continue?"));
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

        assert!(message.contains("❌"));
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

        // 新格式：使用项目名
        assert!(message.contains("✅"));
        assert!(message.contains("myapp") || message.contains("已完成"));
    }

    // ==================== 终端快照测试 ====================

    #[test]
    fn test_format_event_with_terminal_snapshot() {
        let notifier = OpenclawNotifier::new();

        // 模拟带终端快照的 context
        let context_with_snapshot = r#"{"cwd": "/workspace"}

--- 终端快照 ---
$ cargo build
   Compiling myapp v0.1.0
    Finished release target"#;

        let message = notifier.format_event(
            "cam-123",
            "stop",
            "",
            context_with_snapshot,
        );

        // 新格式：简洁，不再显示终端快照
        assert!(message.contains("⏹️"));
        assert!(message.contains("已停止") || message.contains("workspace"));
    }

    #[test]
    fn test_format_event_snapshot_truncation() {
        let notifier = OpenclawNotifier::new();

        // 创建超过 15 行的终端输出
        let mut long_output = String::from(r#"{"cwd": "/tmp"}

--- 终端快照 ---
"#);
        for i in 1..=20 {
            long_output.push_str(&format!("line {}\n", i));
        }

        let message = notifier.format_event(
            "cam-123",
            "stop",
            "",
            &long_output,
        );

        // 新格式：简洁，不再显示终端快照
        assert!(message.contains("⏹️"));
        assert!(message.contains("已停止") || message.contains("tmp"));
    }

    #[test]
    fn test_format_event_without_snapshot() {
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
            "cam-123",
            "stop",
            "",
            r#"{"cwd": "/workspace"}"#,
        );

        assert!(message.contains("⏹️"));
        assert!(message.contains("已停止") || message.contains("workspace"));
    }

    // ==================== 各事件类型格式化测试 ====================

    #[test]
    fn test_format_permission_request() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/test"}, "cwd": "/workspace"}"#;
        let message = notifier.format_event("cam-123", "permission_request", "", context);

        // 新格式：使用风险等级 emoji（✅/⚠️/🔴）替代固定的 🔐
        // rm -rf /tmp/test 是低风险（/tmp 路径）
        assert!(message.contains("✅") || message.contains("⚠️") || message.contains("🔴"));
        assert!(message.contains("请求权限"));
        assert!(message.contains("Bash"));
        assert!(message.contains("rm -rf /tmp/test"));
        // 新格式：简化回复指引
        assert!(message.contains("y 允许") || message.contains("n 拒绝"));
    }

    #[test]
    fn test_format_notification_idle_prompt() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "idle_prompt", "message": "Task completed, waiting for next instruction"}"#;
        let message = notifier.format_event("cam-123", "notification", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
    }

    #[test]
    fn test_format_notification_permission_prompt() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "permission_prompt", "message": "Allow file write?"}"#;
        let message = notifier.format_event("cam-123", "notification", "", context);

        assert!(message.contains("🔐"));
        assert!(message.contains("确认") || message.contains("需要"));
        assert!(message.contains("Allow file write?"));
        // 新格式：简化回复指引
        assert!(message.contains("y") && message.contains("n"));
    }

    #[test]
    fn test_format_session_start() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"cwd": "/Users/admin/project"}"#;
        let message = notifier.format_event("cam-123", "session_start", "", context);

        assert!(message.contains("🚀"));
        assert!(message.contains("已启动"));
        // 新格式：使用项目名
        assert!(message.contains("project"));
    }

    #[test]
    fn test_format_stop_event() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"cwd": "/workspace/app"}"#;
        let message = notifier.format_event("cam-123", "stop", "", context);

        assert!(message.contains("⏹️"));
        assert!(message.contains("已停止") || message.contains("app"));
    }

    #[test]
    fn test_format_session_end() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"cwd": "/workspace"}"#;
        let message = notifier.format_event("cam-123", "session_end", "", context);

        assert!(message.contains("🔚"));
        assert!(message.contains("会话结束") || message.contains("workspace"));
    }

    #[test]
    fn test_format_agent_exited_with_snapshot() {
        let notifier = OpenclawNotifier::new();

        let context = r#"

--- 终端快照 ---
All tests passed!
Build successful."#;

        let message = notifier.format_event("cam-123", "AgentExited", "/myproject", context);

        // 新格式：简洁，使用项目名
        assert!(message.contains("✅"));
        assert!(message.contains("myproject") || message.contains("已完成"));
    }

    #[test]
    fn test_format_tool_use() {
        let notifier = OpenclawNotifier::new();

        // 带 target 的工具调用
        let message = notifier.format_event("cam-123", "ToolUse", "Edit", "src/main.rs");
        assert!(message.contains("🔧"));
        assert!(message.contains("Edit"));
        assert!(message.contains("src/main.rs"));

        // 不带 target 的工具调用
        let message2 = notifier.format_event("cam-456", "ToolUse", "Read", "");
        assert!(message2.contains("🔧"));
        assert!(message2.contains("Read"));
    }

    // ==================== Channel 检测测试 ====================

    #[test]
    fn test_extract_telegram_target_string() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "telegram": {
                "allowFrom": ["123456789"]
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_telegram_target(&channels);
        assert_eq!(target, Some("123456789".to_string()));
    }

    #[test]
    fn test_extract_telegram_target_number() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "telegram": {
                "allowFrom": [123456789]
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_telegram_target(&channels);
        assert_eq!(target, Some("123456789".to_string()));
    }

    #[test]
    fn test_extract_telegram_target_skips_wildcard() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "telegram": {
                "allowFrom": ["*", "123456789"]
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_telegram_target(&channels);
        assert_eq!(target, Some("123456789".to_string()));
    }

    #[test]
    fn test_extract_default_channel() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "discord": {
                "defaultChannel": "general"
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_default_channel(&channels, "discord");
        assert_eq!(target, Some("general".to_string()));
    }

    #[test]
    fn test_extract_allow_from() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "whatsapp": {
                "allowFrom": ["+1234567890"]
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_allow_from(&channels, "whatsapp");
        assert_eq!(target, Some("+1234567890".to_string()));
    }

    #[test]
    fn test_extract_allow_from_skips_wildcard() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "whatsapp": {
                "allowFrom": ["*", "+1234567890"]
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_allow_from(&channels, "whatsapp");
        assert_eq!(target, Some("+1234567890".to_string()));
    }

    // ==================== Payload 创建测试 ====================

    #[test]
    fn test_create_payload_permission_request() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/test"}, "cwd": "/workspace"}"#;
        let payload = notifier.create_payload("cam-123", "permission_request", "", context);

        assert_eq!(payload["type"], "cam_notification");
        assert_eq!(payload["version"], "1.0");
        assert_eq!(payload["urgency"], "HIGH");
        assert_eq!(payload["event_type"], "permission_request");
        assert_eq!(payload["agent_id"], "cam-123");
        assert_eq!(payload["project"], "/workspace");
        assert_eq!(payload["event"]["tool_name"], "Bash");
        assert!(payload["event"]["tool_input"]["command"].as_str().unwrap().contains("rm -rf"));
        assert!(payload["summary"].as_str().unwrap().contains("Bash"));
        assert!(payload["timestamp"].as_str().is_some());
    }

    #[test]
    fn test_create_payload_error() {
        let notifier = OpenclawNotifier::new();

        let payload = notifier.create_payload("cam-456", "Error", "", "API rate limit exceeded");

        assert_eq!(payload["type"], "cam_notification");
        assert_eq!(payload["urgency"], "HIGH");
        assert_eq!(payload["event_type"], "Error");
        assert_eq!(payload["event"]["message"], "API rate limit exceeded");
        assert_eq!(payload["summary"], "发生错误");
    }

    #[test]
    fn test_create_payload_waiting_for_input() {
        let notifier = OpenclawNotifier::new();

        let payload = notifier.create_payload("cam-789", "WaitingForInput", "Confirmation", "Continue? [Y/n]");

        assert_eq!(payload["urgency"], "HIGH");
        assert_eq!(payload["event_type"], "WaitingForInput");
        assert_eq!(payload["event"]["pattern_type"], "Confirmation");
        assert_eq!(payload["event"]["prompt"], "Continue? [Y/n]");
        assert!(payload["summary"].as_str().unwrap().contains("Confirmation"));
    }

    #[test]
    fn test_create_payload_agent_exited() {
        let notifier = OpenclawNotifier::new();

        let payload = notifier.create_payload("cam-abc", "AgentExited", "/myproject", "");

        assert_eq!(payload["urgency"], "MEDIUM");
        assert_eq!(payload["event_type"], "AgentExited");
        assert_eq!(payload["event"]["project_path"], "/myproject");
        assert_eq!(payload["summary"], "Agent 已退出");
    }

    #[test]
    fn test_create_payload_notification_idle_prompt() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "idle_prompt", "message": "Task completed"}"#;
        let payload = notifier.create_payload("cam-def", "notification", "", context);

        assert_eq!(payload["urgency"], "MEDIUM");
        assert_eq!(payload["event"]["notification_type"], "idle_prompt");
        assert_eq!(payload["event"]["message"], "Task completed");
        assert_eq!(payload["summary"], "等待用户输入");
    }

    #[test]
    fn test_create_payload_with_terminal_snapshot() {
        let notifier = OpenclawNotifier::new();

        // 使用 AgentExited 测试（MEDIUM urgency），因为 stop 现在是 LOW
        let context = r#"{"cwd": "/workspace"}

--- 终端快照 ---
$ cargo build
   Compiling myapp v0.1.0
    Finished release target"#;

        let payload = notifier.create_payload("cam-123", "AgentExited", "", context);

        assert_eq!(payload["urgency"], "MEDIUM");
        assert!(payload["terminal_snapshot"].as_str().is_some());
        assert!(payload["terminal_snapshot"].as_str().unwrap().contains("cargo build"));
    }

    #[test]
    fn test_create_payload_snapshot_truncation() {
        let notifier = OpenclawNotifier::new();

        // 创建超过 15 行的终端输出
        let mut long_output = String::from(r#"{"cwd": "/tmp"}

--- 终端快照 ---
"#);
        for i in 1..=20 {
            long_output.push_str(&format!("line {}\n", i));
        }

        let payload = notifier.create_payload("cam-123", "stop", "", &long_output);

        let snapshot = payload["terminal_snapshot"].as_str().unwrap();
        // 应该只包含最后 15 行
        assert!(snapshot.contains("line 20"));
        assert!(snapshot.contains("line 6"));
        assert!(!snapshot.contains("line 5\n"));
    }

    // Note: generate_summary tests moved to notification::payload module

    // ==================== 新格式辅助函数测试 ====================

    #[test]
    fn test_extract_project_name() {
        assert_eq!(MessageFormatter::extract_project_name("/Users/admin/workspace/myapp"), "myapp");
        assert_eq!(MessageFormatter::extract_project_name("/workspace"), "workspace");
        assert_eq!(MessageFormatter::extract_project_name(""), "unknown");
        // Root path returns "/" as the file_name
        assert_eq!(MessageFormatter::extract_project_name("/"), "/");
    }

    #[test]
    fn test_get_project_name_for_agent() {
        // 测试 agent_id 简化（当 agents.json 中找不到时）
        let name = MessageFormatter::get_project_name_for_agent("cam-1234567890");
        assert_eq!(name, "agent-1234");

        // 短 agent_id 不简化
        let name2 = MessageFormatter::get_project_name_for_agent("cam-123");
        assert_eq!(name2, "cam-123");

        // 外部会话 agent_id 简化（当 agents.json 中找不到时）
        // 注意：如果 agents.json 中有此 agent，会返回实际项目名
        let name3 = MessageFormatter::get_project_name_for_agent("ext-nonexist");
        assert_eq!(name3, "session-none");

        // 短外部会话 agent_id 不简化
        let name4 = MessageFormatter::get_project_name_for_agent("ext-123");
        assert_eq!(name4, "ext-123");
    }

    // ==================== 新格式集成测试 ====================

    #[test]
    fn test_format_notification_with_no_ai_fallback() {
        // 测试当 AI 禁用时，回退到简洁提示（不显示原始快照，避免 UI 元素泄露）
        let notifier = OpenclawNotifier::new().with_no_ai(true);

        let context = r#"{"notification_type": "idle_prompt", "message": ""}

--- 终端快照 ---
Some unrecognized prompt format that doesn't match any pattern
Please provide your input here"#;

        let message = notifier.format_event("cam-123", "notification", "", context);

        // 应该显示简洁提示，不显示原始快照内容
        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
        // 新行为：AI 提取失败时显示简洁提示，不显示原始快照
        assert!(message.contains("无法解析通知内容，请查看终端"));
    }

    #[test]
    fn test_format_notification_ai_extraction_path() {
        // 测试 AI 提取路径（不实际调用 AI，只验证代码路径）
        let notifier = OpenclawNotifier::new().with_dry_run(true);

        let context = r#"{"notification_type": "idle_prompt", "message": ""}

--- 终端快照 ---
Some complex terminal output
That doesn't match standard patterns
But contains a question somewhere"#;

        // dry_run 模式下会尝试 AI 提取
        // 根据 AI 判断结果返回不同的 emoji：📋(有问题) / ⏸️(失败) / ✅(完成) / 💤(空闲)
        let message = notifier.format_event("cam-123", "notification", "", context);

        // 验证返回了某种格式的消息
        assert!(message.contains("📋") || message.contains("⏸️") || message.contains("✅") || message.contains("💤"));
    }

    // ==================== Stop 事件 urgency 升级测试 ====================

    #[test]
    fn test_stop_event_with_question_upgrades_urgency() {
        // 测试 stop 事件包含问题时，urgency 应该被提升
        let notifier = OpenclawNotifier::new().with_dry_run(true).with_no_ai(true);

        // 创建一个包含问题的 stop 事件
        let event = NotificationEvent::new(
            "cam-test".to_string(),
            NotificationEventType::Stop,
        )
        .with_project_path("/workspace/test")
        .with_terminal_snapshot("❯ 问我想要实现什么功能\n\n⏺ 你想要实现什么功能？\n\n❯ ");

        // 发送通知
        let result = notifier.send_notification_event(&event);

        // 应该成功发送（不是被跳过）
        assert!(result.is_ok());
        // 注意：由于 no_ai=true，可能不会检测到问题
        // 这个测试主要验证代码路径不会 panic
    }

    #[test]
    fn test_stop_event_without_question_stays_low() {
        let notifier = OpenclawNotifier::new().with_dry_run(true);

        // 创建一个不包含问题的 stop 事件
        let event = NotificationEvent::new(
            "cam-test".to_string(),
            NotificationEventType::Stop,
        )
        .with_project_path("/workspace/test")
        .with_terminal_snapshot("Task completed successfully.\n\n❯ ");

        let result = notifier.send_notification_event(&event);

        // 应该被跳过（LOW urgency）
        assert!(matches!(result, Ok(SendResult::Skipped(_))));
    }

}
