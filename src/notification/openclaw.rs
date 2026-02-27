//! OpenClaw 通知模块 - 通过 openclaw CLI 发送事件到 channel 或 agent
//!
//! 通知路由策略：
//! - HIGH/MEDIUM urgency → 通过 system event 发送结构化 payload（触发 heartbeat）
//! - LOW urgency → 静默处理（避免上下文累积）
//!
//! 此模块作为门面（Facade），委托给 notification 子模块处理具体逻辑：
//! - `notification::urgency` - Urgency 分类
//! - `notification::payload` - Payload 构建
//! - `notification::terminal_cleaner` - 终端输出清理
//! - `notification::system_event` - System Event 结构化数据

use crate::agent::extractor::extract_message_from_snapshot;
use crate::infra::terminal::truncate_for_status;
use crate::notification::channel::SendResult;
use crate::notification::dedup_key::generate_dedup_key;
use crate::notification::deduplicator::NotificationDeduplicator;
use crate::notification::event::{NotificationEvent, NotificationEventType};
use crate::notification::payload::PayloadBuilder;
use crate::notification::store::{NotificationRecord, NotificationStore};
use crate::notification::urgency::{get_urgency, Urgency};
use crate::notification::webhook::{WebhookClient, WebhookConfig};
use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::Command;
use std::sync::Mutex;
use tracing::{debug, error, info, warn};

/// 记录到 hook.log
fn log_to_hook_file(message: &str) {
    let log_path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".config/code-agent-monitor/hook.log");

    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = writeln!(file, "[{}] {}", timestamp, message);
    }
}

/// Convert NotificationEventType to a string for dedup key generation
/// Used when terminal_snapshot is not available
fn event_type_to_string(event_type: &NotificationEventType) -> String {
    match event_type {
        NotificationEventType::WaitingForInput {
            pattern_type,
            is_decision_required,
        } => {
            format!(
                "waiting_for_input:{}:{}",
                pattern_type, is_decision_required
            )
        }
        NotificationEventType::PermissionRequest { tool_name, .. } => {
            format!("permission_request:{}", tool_name)
        }
        NotificationEventType::Notification {
            notification_type,
            message,
        } => {
            format!("notification:{}:{}", notification_type, message)
        }
        NotificationEventType::AgentExited => "agent_exited".to_string(),
        NotificationEventType::Error { message } => format!("error:{}", message),
        NotificationEventType::Stop => "stop".to_string(),
        NotificationEventType::SessionStart => "session_start".to_string(),
        NotificationEventType::SessionEnd => "session_end".to_string(),
    }
}

/// OpenClaw notifier - 门面模式，委托给子模块处理
pub struct OpenclawNotifier {
    /// openclaw command path
    openclaw_cmd: String,
    /// dry-run mode (print only, don't send)
    dry_run: bool,
    /// Disable AI extraction (for testing/debugging)
    no_ai: bool,
    /// Webhook client (可选，用于 HTTP 触发)
    webhook_client: Option<WebhookClient>,
    /// Optional defaults for webhook delivery routing
    webhook_default_channel: Option<String>,
    webhook_default_to: Option<String>,
    /// Payload 构建器
    payload_builder: PayloadBuilder,
    /// 通知去重器
    deduplicator: Mutex<NotificationDeduplicator>,
}

impl OpenclawNotifier {
    /// 创建新的通知器
    pub fn new() -> Self {
        Self {
            openclaw_cmd: Self::find_openclaw_path(),
            dry_run: false,
            no_ai: false,
            webhook_client: None,
            webhook_default_channel: None,
            webhook_default_to: None,
            payload_builder: PayloadBuilder::new(),
            deduplicator: Mutex::new(NotificationDeduplicator::new()),
        }
    }

    /// 使用 webhook 配置创建通知器
    pub fn with_webhook(config: WebhookConfig) -> Result<Self, String> {
        let webhook_default_channel = config.default_channel.clone();
        let webhook_default_to = config.default_to.clone();
        let webhook_client = WebhookClient::new(config)?;
        Ok(Self {
            openclaw_cmd: Self::find_openclaw_path(),
            dry_run: false,
            no_ai: false,
            webhook_client: Some(webhook_client),
            webhook_default_channel,
            webhook_default_to,
            payload_builder: PayloadBuilder::new(),
            deduplicator: Mutex::new(NotificationDeduplicator::new()),
        })
    }

    /// 设置 dry-run 模式
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// 设置是否禁用 AI 提取
    pub fn with_no_ai(mut self, no_ai: bool) -> Self {
        self.no_ai = no_ai;
        self.payload_builder = self.payload_builder.with_no_ai(no_ai);
        self
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

    /// 创建结构化 payload - 委托给 PayloadBuilder
    fn create_payload(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> serde_json::Value {
        let urgency = get_urgency(event_type, context);
        self.payload_builder
            .create_payload(agent_id, event_type, pattern_or_path, context, urgency)
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
        // 外部会话（ext-xxx）不发送通知
        // 原因：外部会话无法远程回复，通知只会造成打扰
        if agent_id.starts_with("ext-") {
            if self.dry_run {
                eprintln!(
                    "[DRY-RUN] External session (cannot reply remotely), skipping: {} {}",
                    agent_id, event_type
                );
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
                // 发送 system event 到 Dashboard（异步，不阻塞）
                let payload = self.create_payload(agent_id, event_type, pattern_or_path, context);
                if let Err(e) = self.send_via_gateway_async(&payload) {
                    warn!(error = %e, "Failed to send system event to dashboard");
                }

                info!(
                    agent_id = %agent_id,
                    event_type = %event_type,
                    urgency = urgency.as_str(),
                    "Notification sent to dashboard"
                );
                Ok(SendResult::Sent)
            }
            Urgency::Low => {
                // LOW urgency: 静默处理，不发送通知
                if self.dry_run {
                    eprintln!(
                        "[DRY-RUN] LOW urgency, skipping: {} {}",
                        event_type, agent_id
                    );
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
    /// 委托模式：只发送 system event，由 OpenClaw Agent 决定如何处理
    pub fn send_notification_event(&self, event: &NotificationEvent) -> Result<SendResult> {
        self.send_system_event_only(event)
    }

    /// 只发送 system event（新架构）
    ///
    /// 不再发送 message send，所有决策由 OpenClaw Agent 处理
    pub fn send_system_event_only(&self, event: &NotificationEvent) -> Result<SendResult> {
        use crate::notification::system_event::SystemEventPayload;
        use crate::notification::terminal_cleaner::is_processing;

        let agent_id = &event.agent_id;

        // 外部会话不发送通知
        if agent_id.starts_with("ext-") {
            debug!(agent_id = %agent_id, "Skipping external session notification");
            return Ok(SendResult::Skipped("external session".to_string()));
        }

        // 检测处理中状态
        if let Some(ref snapshot) = event.terminal_snapshot {
            if is_processing(snapshot) {
                debug!(agent_id = %agent_id, "Skipping notification - agent is processing");
                return Ok(SendResult::Skipped("agent processing".to_string()));
            }
        }

        // 计算 urgency
        let event_type_str = match &event.event_type {
            NotificationEventType::WaitingForInput { .. } => "WaitingForInput",
            NotificationEventType::PermissionRequest { .. } => "permission_request",
            NotificationEventType::Notification {
                notification_type, ..
            } => {
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

        let context_for_urgency = match &event.event_type {
            NotificationEventType::Notification {
                notification_type,
                message,
            } => serde_json::json!({
                "notification_type": notification_type,
                "message": message
            })
            .to_string(),
            _ => String::new(),
        };

        let urgency = get_urgency(event_type_str, &context_for_urgency);

        // LOW urgency 静默处理
        if matches!(urgency, Urgency::Low) {
            debug!(agent_id = %agent_id, event_type = %event_type_str, "Notification skipped (LOW urgency)");
            return Ok(SendResult::Skipped(format!(
                "LOW urgency ({})",
                event_type_str
            )));
        }

        // 去重检查
        let dedup_key = if let Some(ref key) = event.dedup_key {
            key.clone()
        } else if let Some(snapshot) = event.terminal_snapshot.as_deref() {
            let truncated = truncate_for_status(snapshot);
            generate_dedup_key(&truncated)
        } else {
            let fallback_content =
                format!("{}:{}", event_type_to_string(&event.event_type), agent_id);
            generate_dedup_key(&fallback_content)
        };

        if !event.skip_dedup {
            let mut dedup = self.deduplicator.lock().unwrap();
            let action = dedup.should_send(agent_id, &dedup_key);
            if let crate::notification::NotifyAction::Suppressed(reason) = action {
                debug!(agent_id = %agent_id, reason = %reason, "Notification deduplicated");
                return Ok(SendResult::Skipped("duplicate".to_string()));
            }
        }

        // 构建并发送 system event
        let mut payload = SystemEventPayload::from_event(event, urgency);

        // 对于需要用户输入的事件，使用 ReAct 提取器提取格式化消息
        // 只在确定要发送时才调用，避免浪费 API 调用
        if !self.no_ai {
            if let Some(snapshot) = &event.terminal_snapshot {
                if matches!(
                    event.event_type,
                    NotificationEventType::WaitingForInput { .. }
                        | NotificationEventType::PermissionRequest { .. }
                ) {
                    match extract_message_from_snapshot(snapshot) {
                        Some((message, fingerprint, _is_decision_required)) => {
                            // 检查是否是错误消息，如果是则升级为 Error 事件
                            if message.starts_with("ERROR: ") {
                                let error_msg = message.strip_prefix("ERROR: ").unwrap_or(&message).to_string();
                                info!(
                                    agent_id = %agent_id,
                                    error = %error_msg,
                                    "AI extractor detected terminal error, upgrading to Error event"
                                );
                                let error_event = NotificationEvent::new(
                                    agent_id.clone(),
                                    NotificationEventType::Error { message: error_msg },
                                )
                                .with_terminal_snapshot(snapshot.clone())
                                .with_skip_dedup(event.skip_dedup);
                                if let Some(ref project) = event.project_path {
                                    return self.send_system_event_only(&error_event.with_project_path(project));
                                }
                                return self.send_system_event_only(&error_event);
                            }
                            debug!(
                                agent_id = %agent_id,
                                fingerprint = %fingerprint,
                                "ReAct extracted formatted message"
                            );
                            payload.set_extracted_message(message, fingerprint);
                        }
                        None => {
                            debug!(
                                agent_id = %agent_id,
                                "ReAct extraction returned None (processing/idle/failed)"
                            );
                        }
                    }
                }
            }
        }

        if self.dry_run {
            eprintln!("[DRY-RUN] Would send system event:");
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&payload).unwrap_or_default()
            );
            return Ok(SendResult::Sent);
        }

        // If a webhook is configured, prefer it (single-channel delivery).
        // This is especially important for reply-required events so OpenClaw hooks/skills can run.
        self.send_via_gateway_async(&payload.to_json())?;

        // 记录详细的发送内容到 hook.log
        log_to_hook_file(&format!(
            "📤 Webhook sent: agent={} event={} urgency={}",
            agent_id,
            event_type_str,
            urgency.as_str()
        ));
        if let Some(ref extracted) = payload.context.extracted_message {
            log_to_hook_file(&format!(
                "   extracted_message: {}",
                extracted.replace('\n', "\\n")
            ));
        }
        if let Some(ref fp) = payload.context.question_fingerprint {
            log_to_hook_file(&format!("   fingerprint: {}", fp));
        }

        // 记录到本地文件（供 TUI 显示）
        let summary = match &event.event_type {
            NotificationEventType::PermissionRequest { tool_name, .. } => {
                format!("Permission: {}", tool_name)
            }
            NotificationEventType::WaitingForInput { pattern_type, .. } => {
                format!("Waiting: {}", pattern_type)
            }
            NotificationEventType::Notification {
                notification_type,
                message,
            } => {
                if message.is_empty() {
                    notification_type.clone()
                } else {
                    message.chars().take(80).collect()
                }
            }
            NotificationEventType::Error { message } => {
                format!("Error: {}", message.chars().take(60).collect::<String>())
            }
            NotificationEventType::AgentExited => "Agent exited".to_string(),
            NotificationEventType::Stop => "Stopped".to_string(),
            NotificationEventType::SessionStart => "Session started".to_string(),
            NotificationEventType::SessionEnd => "Session ended".to_string(),
        };

        // Build event_detail JSON from event type
        let event_detail = match &event.event_type {
            NotificationEventType::PermissionRequest {
                tool_name,
                tool_input,
            } => Some(serde_json::json!({
                "tool_name": tool_name,
                "tool_input": tool_input,
            })),
            NotificationEventType::WaitingForInput {
                pattern_type,
                is_decision_required,
            } => Some(serde_json::json!({
                "pattern_type": pattern_type,
                "is_decision_required": is_decision_required,
            })),
            NotificationEventType::Notification {
                notification_type,
                message,
            } => Some(serde_json::json!({
                "notification_type": notification_type,
                "message": message,
            })),
            NotificationEventType::Error { message } => Some(serde_json::json!({
                "message": message,
            })),
            _ => None,
        };

        // Use extracted_message as risk_level hint if available
        let risk_level = payload
            .context
            .extracted_message
            .as_ref()
            .and_then(|_| Some("AI_EXTRACTED".to_string()));

        let record = NotificationRecord {
            ts: chrono::Utc::now(),
            agent_id: agent_id.clone(),
            urgency,
            event: event_type_str.to_string(),
            summary,
            project: event.project_path.clone(),
            event_detail,
            terminal_snapshot: event.terminal_snapshot.clone(),
            risk_level,
        };
        if let Err(e) = NotificationStore::append(&record) {
            warn!(error = %e, "Failed to write notification to local file");
        }

        info!(
            agent_id = %agent_id,
            event_type = %event_type_str,
            urgency = urgency.as_str(),
            "System event sent to OpenClaw"
        );

        Ok(SendResult::Sent)
    }

    /// 发送 system event 到 Gateway 并等待 Agent 处理
    ///
    /// 使用 --expect-final 等待 Agent 完成处理，确保通知被发送到用户
    fn send_via_gateway_async(&self, payload: &serde_json::Value) -> Result<()> {
        // 如果配置了 webhook client，优先使用 webhook
        if let Some(ref _client) = self.webhook_client {
            return self.send_via_webhook(payload);
        }

        if self.dry_run {
            eprintln!("[DRY-RUN] Would send via system event");
            eprintln!(
                "[DRY-RUN] Payload: {}",
                serde_json::to_string_pretty(payload).unwrap_or_default()
            );
            return Ok(());
        }

        let payload_text = payload.to_string();

        // 使用阻塞执行，确保获取失败原因
        // 超时设置为 60 秒，足够 Agent 处理并发送通知
        let output = Command::new(&self.openclaw_cmd)
            .args([
                "system",
                "event",
                "--text",
                &payload_text,
                "--mode",
                "now",
                "--expect-final",
                "--timeout",
                "60000",
            ])
            .output();

        match output {
            Ok(out) => {
                if out.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    error!(status = ?out.status, stderr = %stderr, "System event command failed");
                    Err(anyhow::anyhow!("System event command failed: {}", stderr))
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to run system event");
                Err(e.into())
            }
        }
    }

    /// 通过 Webhook 发送通知 (推荐方案)
    fn send_via_webhook(&self, payload: &serde_json::Value) -> anyhow::Result<()> {
        if let Some(ref client) = self.webhook_client {
            // 从 payload 中提取消息内容，优先使用格式化消息
            // NOTE: SystemEventPayload 使用 camelCase (eventType)，旧版 PayloadBuilder 使用 snake_case (event_type)
            let message =
                if payload.get("eventType").is_some() || payload.get("event_type").is_some() {
                    // 这是 SystemEventPayload 格式，使用格式化消息
                    use crate::notification::system_event::SystemEventPayload;
                    if let Ok(sep) = serde_json::from_value::<SystemEventPayload>(payload.clone()) {
                        let mut msg = sep.to_telegram_message();

                        // For reply-required events, include raw JSON so hooks/skills (and humans) have full context.
                        if matches!(
                            sep.event_type.as_str(),
                            "permission_request" | "waiting_for_input"
                        ) {
                            let raw = serde_json::to_string_pretty(payload).unwrap_or_default();
                            let max_chars = 3500usize;
                            let raw_trunc: String = raw.chars().take(max_chars).collect();
                            msg.push_str("\n\n---\nraw_event_json:\n```json\n");
                            msg.push_str(&raw_trunc);
                            if raw.len() > max_chars {
                                msg.push_str("\n... (truncated)");
                            }
                            msg.push_str("\n```\n");
                        }

                        msg
                    } else {
                        payload
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Agent notification")
                            .to_string()
                    }
                } else {
                    payload
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Agent notification")
                        .to_string()
                };

            // 支持 camelCase (agentId) 和 snake_case (agent_id)
            let agent_id = payload
                .get("agentId")
                .or_else(|| payload.get("agent_id"))
                .and_then(|m| m.as_str())
                .map(String::from);

            let agent_id_for_log = agent_id.clone();

            // 使用阻塞版本发送（避免在 async runtime 中创建新 runtime）
            let result = client.send_notification_blocking(
                message,
                agent_id,
                self.webhook_default_channel.clone(),
                self.webhook_default_to.clone(),
            );

            match result {
                Ok(resp) => {
                    if resp.ok {
                        info!(agent_id = ?agent_id_for_log, "Webhook notification sent successfully");
                        Ok(())
                    } else {
                        anyhow::bail!("Webhook failed: {:?}", resp.error)
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to send webhook notification");
                    anyhow::bail!("Webhook error: {}", e)
                }
            }
        } else {
            anyhow::bail!("Webhook client not configured")
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
        assert!(payload["event"]["tool_input"]["command"]
            .as_str()
            .unwrap()
            .contains("rm -rf"));
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

        let payload = notifier.create_payload(
            "cam-789",
            "WaitingForInput",
            "Confirmation",
            "Continue? [Y/n]",
        );

        assert_eq!(payload["urgency"], "HIGH");
        assert_eq!(payload["event_type"], "WaitingForInput");
        assert_eq!(payload["event"]["pattern_type"], "Confirmation");
        assert_eq!(payload["event"]["prompt"], "Continue? [Y/n]");
        assert!(payload["summary"]
            .as_str()
            .unwrap()
            .contains("Confirmation"));
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
        assert!(payload["terminal_snapshot"]
            .as_str()
            .unwrap()
            .contains("cargo build"));
    }

    #[test]
    fn test_create_payload_snapshot_truncation() {
        let notifier = OpenclawNotifier::new();

        // 创建超过 15 行的终端输出
        let mut long_output = String::from(
            r#"{"cwd": "/tmp"}

--- 终端快照 ---
"#,
        );
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

    // ==================== Stop 事件 urgency 升级测试 ====================

    #[test]
    fn test_stop_event_with_question_upgrades_urgency() {
        // 测试 stop 事件包含问题时，urgency 应该被提升
        let notifier = OpenclawNotifier::new().with_dry_run(true).with_no_ai(true);

        // 创建一个包含问题的 stop 事件
        let event = NotificationEvent::new("cam-test".to_string(), NotificationEventType::Stop)
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
        let event = NotificationEvent::new("cam-test".to_string(), NotificationEventType::Stop)
            .with_project_path("/workspace/test")
            .with_terminal_snapshot("Task completed successfully.\n\n❯ ");

        let result = notifier.send_notification_event(&event);

        // 应该被跳过（LOW urgency）
        assert!(matches!(result, Ok(SendResult::Skipped(_))));
    }

    // ==================== Empty snapshot dedup key tests ====================

    #[test]
    fn test_event_type_to_string_produces_unique_keys() {
        // Different event types should produce different strings
        let error_event = NotificationEventType::Error {
            message: "test error".to_string(),
        };
        let permission_event = NotificationEventType::PermissionRequest {
            tool_name: "Bash".to_string(),
            tool_input: serde_json::json!({"command": "ls"}),
        };
        let stop_event = NotificationEventType::Stop;

        let error_str = event_type_to_string(&error_event);
        let permission_str = event_type_to_string(&permission_event);
        let stop_str = event_type_to_string(&stop_event);

        // All should be different
        assert_ne!(error_str, permission_str);
        assert_ne!(error_str, stop_str);
        assert_ne!(permission_str, stop_str);

        // Verify format
        assert!(error_str.starts_with("error:"));
        assert!(permission_str.starts_with("permission_request:"));
        assert_eq!(stop_str, "stop");
    }

    #[test]
    fn test_different_events_without_snapshot_get_different_dedup_keys() {
        // Events without terminal_snapshot should still get unique dedup keys
        // based on event type and message content
        let message1 = "Error occurred";
        let message2 = "Permission needed";

        let error_event = NotificationEventType::Error {
            message: "API error".to_string(),
        };
        let permission_event = NotificationEventType::PermissionRequest {
            tool_name: "Bash".to_string(),
            tool_input: serde_json::json!({"command": "rm -rf /tmp"}),
        };

        // Generate fallback keys (simulating what happens when snapshot is None)
        let fallback1 = format!("{}:{}", event_type_to_string(&error_event), message1);
        let fallback2 = format!("{}:{}", event_type_to_string(&permission_event), message2);

        let key1 = generate_dedup_key(&fallback1);
        let key2 = generate_dedup_key(&fallback2);

        // Keys should be different
        assert_ne!(
            key1, key2,
            "Different events without snapshots should have different dedup keys"
        );
    }

    #[test]
    fn test_same_event_type_different_message_different_keys() {
        // Same event type but different messages should get different keys
        let event_type = NotificationEventType::Error {
            message: "error1".to_string(),
        };

        let fallback1 = format!(
            "{}:{}",
            event_type_to_string(&event_type),
            "First error message"
        );
        let fallback2 = format!(
            "{}:{}",
            event_type_to_string(&event_type),
            "Second error message"
        );

        let key1 = generate_dedup_key(&fallback1);
        let key2 = generate_dedup_key(&fallback2);

        assert_ne!(
            key1, key2,
            "Same event type with different messages should have different keys"
        );
    }

    // ==================== Passed dedup_key preference tests ====================

    #[test]
    fn test_event_with_dedup_key_uses_passed_key() {
        // When event has dedup_key set, it should be used instead of generating one
        let event = NotificationEvent::new(
            "cam-test".to_string(),
            NotificationEventType::WaitingForInput {
                pattern_type: "Confirmation".to_string(),
                is_decision_required: false,
            },
        )
        .with_terminal_snapshot("Some terminal content")
        .with_dedup_key("watcher-generated-key-123");

        // Verify the dedup_key is set
        assert_eq!(
            event.dedup_key,
            Some("watcher-generated-key-123".to_string())
        );
    }
}
