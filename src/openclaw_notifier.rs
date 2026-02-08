//! OpenClaw 通知模块 - 通过 openclaw CLI 发送事件到 channel 或 agent
//!
//! 通知路由策略：
//! - HIGH/MEDIUM urgency → 通过 system event 发送结构化 payload（触发 heartbeat）
//! - LOW urgency → 静默处理（避免上下文累积）
//!
//! Payload 格式：
//! ```json
//! {
//!   "type": "cam_notification",
//!   "version": "1.0",
//!   "urgency": "HIGH",
//!   "event_type": "permission_request",
//!   "agent_id": "cam-xxx",
//!   "project": "/path/to/project",
//!   "event": { ... },
//!   "summary": "简短摘要"
//! }
//! ```

use anyhow::Result;
use std::process::Command;
use std::fs;
use chrono::Utc;

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
        // 分离终端快照和原始 context
        let (raw_context, terminal_snapshot) = if let Some(idx) = context.find("\n\n--- 终端快照 ---\n") {
            let (before, after) = context.split_at(idx);
            let snapshot = after.trim_start_matches("\n\n--- 终端快照 ---\n");
            (before, Some(snapshot))
        } else {
            (context, None)
        };

        // 尝试解析 JSON context 获取更多信息
        let json: Option<serde_json::Value> = serde_json::from_str(raw_context).ok();

        // 格式化终端快照（截取最后 15 行，避免消息过长）
        let snapshot_section = terminal_snapshot.map(|s| {
            let lines: Vec<&str> = s.lines().collect();
            let display_lines = if lines.len() > 15 {
                lines[lines.len() - 15..].join("\n")
            } else {
                s.to_string()
            };
            format!("\n\n📸 终端快照:\n```\n{}\n```", display_lines)
        }).unwrap_or_default();

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
                    "🔐 [CAM] {} 请求权限\n\n工具: {}\n目录: {}\n参数:\n```\n{}\n```{}\n\n请回复:\n{} 1 = 允许\n{} 2 = 允许并记住\n{} 3 = 拒绝",
                    agent_id, tool_name, cwd, tool_input, snapshot_section, agent_id, agent_id, agent_id
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
                    format!("⏸️ [CAM] {} 等待输入\n\n{}{}", agent_id, message, snapshot_section)
                } else if notification_type == "permission_prompt" {
                    format!(
                        "🔐 [CAM] {} 需要权限确认\n\n{}{}\n\n请回复:\n{} 1 = 允许\n{} 2 = 允许并记住\n{} 3 = 拒绝",
                        agent_id, message, snapshot_section, agent_id, agent_id, agent_id
                    )
                } else {
                    format!("📢 [CAM] {} 通知\n\n{}{}", agent_id, message, snapshot_section)
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
                format!("✅ [CAM] {} 已停止\n\n目录: {}{}", agent_id, cwd, snapshot_section)
            }
            "WaitingForInput" => {
                format!(
                    "⏸️ [CAM] {} 等待输入\n\n类型: {}\n上下文: {}{}",
                    agent_id, pattern_or_path, raw_context, snapshot_section
                )
            }
            "Error" => {
                format!(
                    "❌ [CAM] {} 发生错误\n\n错误信息:\n---\n{}\n---{}\n\n请问如何处理？",
                    agent_id, raw_context, snapshot_section
                )
            }
            "AgentExited" => {
                format!(
                    "✅ [CAM] {} 已退出\n\n项目: {}{}",
                    agent_id, pattern_or_path, snapshot_section
                )
            }
            "ToolUse" => {
                // pattern_or_path = tool_name, raw_context = tool_target
                let target_info = if raw_context.is_empty() {
                    String::new()
                } else {
                    format!(" → {}", raw_context)
                };
                format!("🔧 [CAM] {} 执行: {}{}", agent_id, pattern_or_path, target_info)
            }
            _ => format!("[CAM] {} - {}: {}{}", agent_id, event_type, raw_context, snapshot_section),
        }
    }

    /// 判断事件是否需要用户关注（用于提示 OpenClaw agent）
    ///
    /// 20 个 AI 并行时的关注优先级:
    /// - HIGH: 必须立即响应（权限请求、错误）→ 阻塞任务进度
    /// - MEDIUM: 需要知道（完成、空闲）→ 可以分配新任务
    /// - LOW: 可选（启动）→ 通常不需要通知
    pub fn get_urgency(event_type: &str, context: &str) -> &'static str {
        // `cam notify` 会把终端快照追加到 JSON context 后面，导致直接解析失败。
        // 这里先剥离快照部分，保证 urgency 判断稳定。
        let raw_context = if let Some(idx) = context.find("\n\n--- 终端快照 ---\n") {
            &context[..idx]
        } else {
            context
        };

        match event_type {
            // 权限请求必须转发 - 阻塞任务进度
            "permission_request" => "HIGH",
            // notification 类型需要检查具体类型
            "notification" => {
                let json: Option<serde_json::Value> = serde_json::from_str(raw_context).ok();
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
            // Agent 停止/完成/退出 - 需要知道，可以分配新任务
            "stop" | "session_end" | "AgentExited" => "MEDIUM",
            // 启动通知 - 可选
            "session_start" => "LOW",
            // 工具调用 - 跟踪信息
            "ToolUse" => "MEDIUM",
            // 其他
            _ => "LOW",
        }
    }

    /// 创建结构化 payload 用于 gateway wake
    fn create_payload(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> serde_json::Value {
        let urgency = Self::get_urgency(event_type, context);

        // 分离终端快照和原始 context
        let (raw_context, terminal_snapshot) = if let Some(idx) = context.find("\n\n--- 终端快照 ---\n") {
            let (before, after) = context.split_at(idx);
            let snapshot = after.trim_start_matches("\n\n--- 终端快照 ---\n");
            (before, Some(snapshot.to_string()))
        } else {
            (context, None)
        };

        // 尝试解析 JSON context
        let json: Option<serde_json::Value> = serde_json::from_str(raw_context).ok();

        // 提取项目路径
        let project = json.as_ref()
            .and_then(|j| j.get("cwd"))
            .and_then(|v| v.as_str())
            .unwrap_or(pattern_or_path);

        // 构建 event 对象
        let event = self.build_event_object(event_type, pattern_or_path, &json, raw_context);

        // 生成简短摘要
        let summary = self.generate_summary(event_type, &json, pattern_or_path);

        let mut payload = serde_json::json!({
            "type": "cam_notification",
            "version": "1.0",
            "urgency": urgency,
            "event_type": event_type,
            "agent_id": agent_id,
            "project": project,
            "timestamp": Utc::now().to_rfc3339(),
            "event": event,
            "summary": summary
        });

        // 添加终端快照（如果有）
        if let Some(snapshot) = terminal_snapshot {
            // 截取最后 15 行
            let lines: Vec<&str> = snapshot.lines().collect();
            let truncated = if lines.len() > 15 {
                lines[lines.len() - 15..].join("\n")
            } else {
                snapshot
            };
            payload["terminal_snapshot"] = serde_json::Value::String(truncated);
        }

        payload
    }

    /// 构建 event 对象
    fn build_event_object(
        &self,
        event_type: &str,
        pattern_or_path: &str,
        json: &Option<serde_json::Value>,
        raw_context: &str,
    ) -> serde_json::Value {
        match event_type {
            "permission_request" => {
                let tool_name = json.as_ref()
                    .and_then(|j| j.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let tool_input = json.as_ref()
                    .and_then(|j| j.get("tool_input"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                serde_json::json!({
                    "tool_name": tool_name,
                    "tool_input": tool_input
                })
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

                serde_json::json!({
                    "notification_type": notification_type,
                    "message": message
                })
            }
            "WaitingForInput" => {
                serde_json::json!({
                    "pattern_type": pattern_or_path,
                    "prompt": raw_context
                })
            }
            "Error" => {
                serde_json::json!({
                    "message": raw_context
                })
            }
            "AgentExited" => {
                serde_json::json!({
                    "project_path": pattern_or_path
                })
            }
            "ToolUse" => {
                serde_json::json!({
                    "tool_name": pattern_or_path,
                    "tool_target": raw_context
                })
            }
            _ => {
                serde_json::json!({
                    "raw_context": raw_context
                })
            }
        }
    }

    /// 生成简短摘要
    fn generate_summary(
        &self,
        event_type: &str,
        json: &Option<serde_json::Value>,
        pattern_or_path: &str,
    ) -> String {
        match event_type {
            "permission_request" => {
                let tool_name = json.as_ref()
                    .and_then(|j| j.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format!("请求执行 {} 工具", tool_name)
            }
            "notification" => {
                let notification_type = json.as_ref()
                    .and_then(|j| j.get("notification_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match notification_type {
                    "idle_prompt" => "等待用户输入".to_string(),
                    "permission_prompt" => "需要权限确认".to_string(),
                    _ => "通知".to_string()
                }
            }
            "WaitingForInput" => format!("等待输入: {}", pattern_or_path),
            "Error" => "发生错误".to_string(),
            "AgentExited" => "Agent 已退出".to_string(),
            "ToolUse" => format!("执行工具: {}", pattern_or_path),
            "stop" | "session_end" => "会话已结束".to_string(),
            "session_start" => "会话已启动".to_string(),
            _ => event_type.to_string()
        }
    }

    /// 发送事件到 channel
    /// HIGH/MEDIUM urgency → 通过 gateway wake 发送结构化 payload
    /// LOW urgency → 静默处理（避免 agent session 上下文累积导致去重问题）
    pub fn send_event(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> Result<()> {
        let urgency = Self::get_urgency(event_type, context);

        match urgency {
            "HIGH" | "MEDIUM" => {
                // 直接发送到 Telegram（不经过 system event，因为 Agent 可能不处理 cam_notification）
                if self.channel_config.is_some() {
                    let message = self.format_event(agent_id, event_type, pattern_or_path, context);
                    return self.send_direct(&message);
                }

                // 如果没有 channel 配置，尝试 system event
                let payload = self.create_payload(agent_id, event_type, pattern_or_path, context);
                self.send_via_gateway_wake_payload(&payload)
            }
            _ => {
                // LOW urgency: 静默处理，不发送通知
                // 参考 coding-agent skill 设计：启动通知由调用方自己说，不需要系统推送
                if self.dry_run {
                    eprintln!("[DRY-RUN] LOW urgency, skipping: {} {}", event_type, agent_id);
                }
                Ok(())
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

        // 添加发送方式标识
        let tagged_message = format!("{}\n\n📡 via direct", message);

        let result = Command::new(&self.openclaw_cmd)
            .args([
                "message", "send",
                "--channel", &config.channel,
                "--target", &config.target,
                "--message", &tagged_message,
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

    /// 通过 system event 发送结构化 payload
    /// 参考 coding-agent skill 设计：一次性事件，触发 heartbeat
    fn send_via_gateway_wake_payload(&self, payload: &serde_json::Value) -> Result<()> {
        if self.dry_run {
            eprintln!("[DRY-RUN] Would send via system event");
            eprintln!("[DRY-RUN] Payload: {}", serde_json::to_string_pretty(payload).unwrap_or_default());
            return Ok(());
        }

        let payload_text = payload.to_string();

        let result = Command::new(&self.openclaw_cmd)
            .args([
                "system", "event",
                "--text", &payload_text,
                "--mode", "now",
            ])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("System event 发送失败: {}", stderr);
                    return Err(anyhow::anyhow!("System event failed: {}", stderr));
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("无法执行 system event: {}", e);
                Err(e.into())
            }
        }
    }

    /// 通过 system event 发送通知（旧接口，保留兼容性）
    /// 参考 coding-agent skill 设计：一次性事件，触发 heartbeat
    #[allow(dead_code)]
    fn send_via_gateway_wake(&self, message: &str) -> Result<()> {
        if self.dry_run {
            eprintln!("[DRY-RUN] Would send via system event");
            eprintln!("[DRY-RUN] Message: {}", message);
            return Ok(());
        }

        let result = Command::new(&self.openclaw_cmd)
            .args([
                "system", "event",
                "--text", message,
                "--mode", "now",
            ])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("System event 发送失败: {}", stderr);
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("无法执行 system event: {}", e);
                Err(e.into())
            }
        }
    }

    /// 发送消息给 Agent (已废弃，保留兼容性)
    #[allow(dead_code)]
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

    /// 为 Agent 包装消息（添加元数据）- 已废弃
    #[allow(dead_code)]
    fn wrap_for_agent(&self, message: &str, urgency: &str, event_type: &str, agent_id: &str) -> String {
        format!(
            "{}\n\n---\n[CAM_META] urgency={} event_type={} agent_id={}",
            message, urgency, event_type, agent_id
        )
    }

    /// 发送消息到 clawdbot (已废弃，保留兼容性)
    #[allow(dead_code)]
    pub fn send_message(&self, message: &str) -> Result<()> {
        self.send_to_agent(message)
    }

    /// 直接发送纯文本到检测到的 channel。
    ///
    /// 主要用于老的 `cam watch --openclaw` 路径，避免在多个模块里重复实现
    /// `openclaw message send` 的参数拼装和 channel detection。
    pub fn send_direct_text(&self, message: &str) -> Result<()> {
        self.send_direct(message)
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
        assert_eq!(OpenclawNotifier::get_urgency("AgentExited", ""), "MEDIUM");
        assert_eq!(OpenclawNotifier::get_urgency("ToolUse", ""), "MEDIUM");

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
    fn test_get_urgency_notification_idle_prompt_with_terminal_snapshot() {
        let context = r#"{"notification_type": "idle_prompt", "message": "waiting"}

--- 终端快照 ---
line 1"#;
        assert_eq!(OpenclawNotifier::get_urgency("notification", context), "MEDIUM");
    }

    #[test]
    fn test_get_urgency_notification_permission_prompt_with_terminal_snapshot() {
        let context = r#"{"notification_type": "permission_prompt", "message": "confirm?"}

--- 终端快照 ---
line 1"#;
        assert_eq!(OpenclawNotifier::get_urgency("notification", context), "HIGH");
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

        assert!(message.contains("已停止"));
        assert!(message.contains("📸 终端快照"));
        assert!(message.contains("cargo build"));
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

        // 应该只包含最后 15 行
        assert!(message.contains("line 20"));
        assert!(message.contains("line 6"));
        assert!(!message.contains("line 5\n")); // line 5 应该被截断
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

        assert!(message.contains("已停止"));
        assert!(!message.contains("📸 终端快照"));
    }

    // ==================== 各事件类型格式化测试 ====================

    #[test]
    fn test_format_permission_request() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/test"}, "cwd": "/workspace"}"#;
        let message = notifier.format_event("cam-123", "permission_request", "", context);

        assert!(message.contains("🔐"));
        assert!(message.contains("请求权限"));
        assert!(message.contains("Bash"));
        assert!(message.contains("rm -rf /tmp/test"));
        assert!(message.contains("/workspace"));
        assert!(message.contains("请回复"));
        assert!(message.contains("cam-123 1"));
        assert!(message.contains("cam-123 2"));
        assert!(message.contains("cam-123 3"));
    }

    #[test]
    fn test_format_notification_idle_prompt() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "idle_prompt", "message": "Task completed, waiting for next instruction"}"#;
        let message = notifier.format_event("cam-123", "notification", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
        assert!(message.contains("Task completed"));
    }

    #[test]
    fn test_format_notification_permission_prompt() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "permission_prompt", "message": "Allow file write?"}"#;
        let message = notifier.format_event("cam-123", "notification", "", context);

        assert!(message.contains("🔐"));
        assert!(message.contains("权限确认"));
        assert!(message.contains("Allow file write?"));
        assert!(message.contains("请回复"));
        assert!(message.contains("cam-123 1"));
        assert!(message.contains("cam-123 2"));
        assert!(message.contains("cam-123 3"));
    }

    #[test]
    fn test_format_session_start() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"cwd": "/Users/admin/project"}"#;
        let message = notifier.format_event("cam-123", "session_start", "", context);

        assert!(message.contains("🚀"));
        assert!(message.contains("已启动"));
        assert!(message.contains("/Users/admin/project"));
    }

    #[test]
    fn test_format_stop_event() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"cwd": "/workspace/app"}"#;
        let message = notifier.format_event("cam-123", "stop", "", context);

        assert!(message.contains("✅"));
        assert!(message.contains("已停止"));
        assert!(message.contains("/workspace/app"));
    }

    #[test]
    fn test_format_session_end() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"cwd": "/workspace"}"#;
        let message = notifier.format_event("cam-123", "session_end", "", context);

        assert!(message.contains("✅"));
        assert!(message.contains("已停止"));
    }

    #[test]
    fn test_format_agent_exited_with_snapshot() {
        let notifier = OpenclawNotifier::new();

        let context = r#"

--- 终端快照 ---
All tests passed!
Build successful."#;

        let message = notifier.format_event("cam-123", "AgentExited", "/myproject", context);

        assert!(message.contains("已退出"));
        assert!(message.contains("/myproject"));
        assert!(message.contains("📸 终端快照"));
        assert!(message.contains("All tests passed"));
    }

    #[test]
    fn test_format_tool_use() {
        let notifier = OpenclawNotifier::new();

        // 带 target 的工具调用
        let message = notifier.format_event("cam-123", "ToolUse", "Edit", "src/main.rs");
        assert!(message.contains("🔧"));
        assert!(message.contains("cam-123"));
        assert!(message.contains("Edit"));
        assert!(message.contains("src/main.rs"));

        // 不带 target 的工具调用
        let message2 = notifier.format_event("cam-456", "ToolUse", "Read", "");
        assert!(message2.contains("🔧"));
        assert!(message2.contains("Read"));
        assert!(!message2.contains("→"));
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

    // ==================== Wrap for Agent 测试 ====================

    #[test]
    fn test_wrap_for_agent_low_urgency() {
        let notifier = OpenclawNotifier::new();
        let wrapped = notifier.wrap_for_agent("Session started", "LOW", "session_start", "cam-456");

        assert!(wrapped.contains("Session started"));
        assert!(wrapped.contains("urgency=LOW"));
        assert!(wrapped.contains("event_type=session_start"));
        assert!(wrapped.contains("agent_id=cam-456"));
    }

    #[test]
    fn test_wrap_for_agent_contains_separator() {
        let notifier = OpenclawNotifier::new();
        let wrapped = notifier.wrap_for_agent("Test", "HIGH", "Error", "cam-789");

        // 应该包含分隔符
        assert!(wrapped.contains("---"));
        assert!(wrapped.contains("[CAM_META]"));
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

        let context = r#"{"cwd": "/workspace"}

--- 终端快照 ---
$ cargo build
   Compiling myapp v0.1.0
    Finished release target"#;

        let payload = notifier.create_payload("cam-123", "stop", "", context);

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

    #[test]
    fn test_generate_summary() {
        let notifier = OpenclawNotifier::new();

        // permission_request
        let json: Option<serde_json::Value> = serde_json::from_str(r#"{"tool_name": "Write"}"#).ok();
        assert!(notifier.generate_summary("permission_request", &json, "").contains("Write"));

        // Error
        assert_eq!(notifier.generate_summary("Error", &None, ""), "发生错误");

        // AgentExited
        assert_eq!(notifier.generate_summary("AgentExited", &None, ""), "Agent 已退出");

        // WaitingForInput
        assert!(notifier.generate_summary("WaitingForInput", &None, "Confirmation").contains("Confirmation"));
    }
}
