//! Inbox Watcher 模块 - 监控 Agent Teams inbox 目录变化
//!
//! 监控 `~/.claude/teams/{team-name}/inboxes/` 目录，
//! 检测新消息并根据过滤规则触发通知。

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, error, debug};

use crate::openclaw_notifier::OpenclawNotifier;
use super::bridge::{InboxMessage, SpecialMessage, TeamBridge};

/// 通知紧急程度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// 高优先级：权限请求、错误
    High,
    /// 中优先级：任务完成、空闲
    Medium,
    /// 低优先级：普通消息
    Low,
}

impl Urgency {
    /// 转换为字符串（用于 OpenclawNotifier）
    pub fn as_str(&self) -> &'static str {
        match self {
            Urgency::High => "HIGH",
            Urgency::Medium => "MEDIUM",
            Urgency::Low => "LOW",
        }
    }
}

/// 通知决策
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyDecision {
    /// 需要通知用户
    Notify {
        urgency: Urgency,
        summary: String,
    },
    /// 静默处理
    Silent,
}

/// Inbox Watcher - 监控 inbox 并触发通知
pub struct InboxWatcher {
    team_bridge: TeamBridge,
    notifier: OpenclawNotifier,
    /// 记录每个 inbox 文件的最后修改时间
    last_modified: HashMap<PathBuf, std::time::SystemTime>,
    /// 记录每个 inbox 的最后消息数量
    last_message_count: HashMap<PathBuf, usize>,
    /// 轮询间隔
    poll_interval: Duration,
}

impl InboxWatcher {
    /// 创建新的 InboxWatcher
    pub fn new(notifier: OpenclawNotifier) -> Self {
        Self {
            team_bridge: TeamBridge::new(),
            notifier,
            last_modified: HashMap::new(),
            last_message_count: HashMap::new(),
            poll_interval: Duration::from_secs(2),
        }
    }

    /// 创建带自定义 TeamBridge 的 InboxWatcher（用于测试）
    pub fn new_with_bridge(team_bridge: TeamBridge, notifier: OpenclawNotifier) -> Self {
        Self {
            team_bridge,
            notifier,
            last_modified: HashMap::new(),
            last_message_count: HashMap::new(),
            poll_interval: Duration::from_secs(2),
        }
    }

    /// 设置轮询间隔
    pub fn set_poll_interval(&mut self, interval: Duration) {
        self.poll_interval = interval;
    }

    /// 开始监控指定 Team（阻塞式）
    pub fn watch_team(&mut self, team: &str) -> Result<()> {
        info!(team = %team, "Starting inbox watch for team");

        loop {
            self.check_team_inboxes(team)?;
            std::thread::sleep(self.poll_interval);
        }
    }

    /// 开始监控所有 Teams（阻塞式）
    pub fn watch_all_teams(&mut self) -> Result<()> {
        info!("Starting inbox watch for all teams");

        loop {
            let teams = self.team_bridge.list_teams();
            for team in teams {
                if let Err(e) = self.check_team_inboxes(&team) {
                    error!(team = %team, error = %e, "Failed to check team inboxes");
                }
            }
            std::thread::sleep(self.poll_interval);
        }
    }

    /// 单次检查指定 Team 的所有 inbox
    pub fn check_team_inboxes(&mut self, team: &str) -> Result<Vec<(String, Vec<InboxMessage>)>> {
        let mut new_messages = Vec::new();

        // 获取 team 的 inboxes 目录
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("无法获取 home 目录"))?;
        let inboxes_dir = home.join(".claude").join("teams").join(team).join("inboxes");

        if !inboxes_dir.exists() {
            return Ok(new_messages);
        }

        // 遍历所有 inbox 文件
        for entry in std::fs::read_dir(&inboxes_dir)?.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "json") {
                let member = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(String::from)
                    .unwrap_or_default();

                // 检查文件是否有变化
                if let Ok(metadata) = std::fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        let last_mod = self.last_modified.get(&path).copied();

                        if last_mod.is_none() || last_mod.unwrap() < modified {
                            // 文件有变化，读取新消息
                            if let Ok(messages) = self.team_bridge.read_inbox(team, &member) {
                                let last_count = self.last_message_count.get(&path).copied().unwrap_or(0);

                                if messages.len() > last_count {
                                    // 有新消息
                                    let new_msgs: Vec<_> = messages[last_count..].to_vec();

                                    // 处理新消息
                                    self.process_new_messages(team, &member, &new_msgs)?;

                                    new_messages.push((member.clone(), new_msgs));
                                }

                                // 更新记录
                                self.last_modified.insert(path.clone(), modified);
                                self.last_message_count.insert(path, messages.len());
                            }
                        }
                    }
                }
            }
        }

        Ok(new_messages)
    }

    /// 处理新消息
    fn process_new_messages(
        &self,
        team: &str,
        member: &str,
        messages: &[InboxMessage],
    ) -> Result<()> {
        for msg in messages {
            let decision = self.should_notify(msg);

            match decision {
                NotifyDecision::Notify { urgency, summary } => {
                    info!(
                        team = %team,
                        member = %member,
                        urgency = ?urgency,
                        summary = %summary,
                        "Sending notification"
                    );

                    // 构建通知 context（包含消息详情）
                    let context = serde_json::json!({
                        "type": "cam_team_notification",
                        "version": "1.0",
                        "urgency": urgency.as_str(),
                        "team": team,
                        "member": member,
                        "summary": summary,
                        "message": {
                            "from": msg.from,
                            "text": msg.text,
                            "timestamp": msg.timestamp.to_rfc3339()
                        },
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    });

                    // 根据 urgency 决定 event_type
                    let event_type = match urgency {
                        Urgency::High => "permission_request",
                        Urgency::Medium => "AgentExited",
                        Urgency::Low => "session_start",
                    };

                    // 发送通知 (agent_id, event_type, pattern_or_path, context)
                    if let Err(e) = self.notifier.send_event(
                        &format!("{}@{}", member, team),
                        event_type,
                        &summary,
                        &context.to_string(),
                    ) {
                        error!(
                            team = %team,
                            member = %member,
                            error = %e,
                            "Failed to send notification"
                        );
                    }
                }
                NotifyDecision::Silent => {
                    // 静默处理
                    debug!(team = %team, member = %member, "Message silently processed");
                }
            }
        }

        Ok(())
    }

    /// 判断是否需要通知用户
    pub fn should_notify(&self, message: &InboxMessage) -> NotifyDecision {
        // 尝试解析为特殊消息
        if let Ok(special) = serde_json::from_str::<SpecialMessage>(&message.text) {
            return match special {
                SpecialMessage::PermissionRequest { tool, .. } => NotifyDecision::Notify {
                    urgency: Urgency::High,
                    summary: format!("{} 请求执行 {}", message.from, tool),
                },
                SpecialMessage::TaskAssignment { subject, .. } => NotifyDecision::Notify {
                    urgency: Urgency::Medium,
                    summary: format!("任务分配: {}", subject),
                },
                SpecialMessage::IdleNotification { .. } => {
                    // 普通空闲通知不需要通知用户
                    NotifyDecision::Silent
                }
                SpecialMessage::ShutdownApproved { .. } => {
                    // 关闭确认不需要通知用户
                    NotifyDecision::Silent
                }
            };
        }

        // 检查是否包含错误关键词
        let text_lower = message.text.to_lowercase();
        if text_lower.contains("error") || text_lower.contains("错误") || text_lower.contains("失败") {
            return NotifyDecision::Notify {
                urgency: Urgency::High,
                summary: format!("{}: {}", message.from, truncate_text(&message.text, 50)),
            };
        }

        // 检查是否包含完成关键词
        if text_lower.contains("完成") || text_lower.contains("completed") || text_lower.contains("done") {
            return NotifyDecision::Notify {
                urgency: Urgency::Medium,
                summary: format!("{} 完成任务", message.from),
            };
        }

        // 检查是否有 summary 字段（表示重要消息）
        if message.summary.is_some() {
            return NotifyDecision::Notify {
                urgency: Urgency::Low,
                summary: message.summary.clone().unwrap_or_default(),
            };
        }

        // 默认静默
        NotifyDecision::Silent
    }

    /// 获取等待中的权限请求
    pub fn get_pending_permission_requests(&self, team: &str) -> Result<Vec<PendingPermissionRequest>> {
        let mut requests = Vec::new();

        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("无法获取 home 目录"))?;
        let inboxes_dir = home.join(".claude").join("teams").join(team).join("inboxes");

        if !inboxes_dir.exists() {
            return Ok(requests);
        }

        for entry in std::fs::read_dir(&inboxes_dir)?.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "json") {
                let member = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(String::from)
                    .unwrap_or_default();

                if let Ok(messages) = self.team_bridge.read_inbox(team, &member) {
                    for msg in messages {
                        if !msg.read {
                            if let Ok(SpecialMessage::PermissionRequest { tool, input }) =
                                serde_json::from_str(&msg.text)
                            {
                                requests.push(PendingPermissionRequest {
                                    team: team.to_string(),
                                    member: member.clone(),
                                    tool,
                                    input,
                                    timestamp: msg.timestamp,
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(requests)
    }
}

/// 等待中的权限请求
#[derive(Debug, Clone)]
pub struct PendingPermissionRequest {
    pub team: String,
    pub member: String,
    pub tool: String,
    pub input: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 截断文本
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_message(from: &str, text: &str) -> InboxMessage {
        InboxMessage {
            from: from.to_string(),
            text: text.to_string(),
            summary: None,
            timestamp: Utc::now(),
            color: None,
            read: false,
        }
    }

    #[test]
    fn test_should_notify_permission_request() {
        let notifier = OpenclawNotifier::new();
        let watcher = InboxWatcher::new(notifier);

        let json = r#"{"type":"permission_request","tool":"Bash","input":{"command":"ls"}}"#;
        let msg = create_test_message("developer", json);

        let decision = watcher.should_notify(&msg);

        match decision {
            NotifyDecision::Notify { urgency, summary } => {
                assert_eq!(urgency, Urgency::High);
                assert!(summary.contains("Bash"));
            }
            _ => panic!("Expected Notify decision"),
        }
    }

    #[test]
    fn test_should_notify_task_assignment() {
        let notifier = OpenclawNotifier::new();
        let watcher = InboxWatcher::new(notifier);

        let json = r#"{"type":"task_assignment","task_id":"1","subject":"Fix bug"}"#;
        let msg = create_test_message("team-lead", json);

        let decision = watcher.should_notify(&msg);

        match decision {
            NotifyDecision::Notify { urgency, summary } => {
                assert_eq!(urgency, Urgency::Medium);
                assert!(summary.contains("Fix bug"));
            }
            _ => panic!("Expected Notify decision"),
        }
    }

    #[test]
    fn test_should_not_notify_idle() {
        let notifier = OpenclawNotifier::new();
        let watcher = InboxWatcher::new(notifier);

        let json = r#"{"type":"idle_notification","idle_reason":"available"}"#;
        let msg = create_test_message("developer", json);

        let decision = watcher.should_notify(&msg);
        assert_eq!(decision, NotifyDecision::Silent);
    }

    #[test]
    fn test_should_not_notify_shutdown_approved() {
        let notifier = OpenclawNotifier::new();
        let watcher = InboxWatcher::new(notifier);

        let json = r#"{"type":"shutdown_approved","request_id":"123"}"#;
        let msg = create_test_message("developer", json);

        let decision = watcher.should_notify(&msg);
        assert_eq!(decision, NotifyDecision::Silent);
    }

    #[test]
    fn test_should_notify_error_message() {
        let notifier = OpenclawNotifier::new();
        let watcher = InboxWatcher::new(notifier);

        let msg = create_test_message("developer", "Error: compilation failed");

        let decision = watcher.should_notify(&msg);

        match decision {
            NotifyDecision::Notify { urgency, .. } => {
                assert_eq!(urgency, Urgency::High);
            }
            _ => panic!("Expected Notify decision for error"),
        }
    }

    #[test]
    fn test_should_notify_chinese_error() {
        let notifier = OpenclawNotifier::new();
        let watcher = InboxWatcher::new(notifier);

        let msg = create_test_message("developer", "任务执行失败，请检查配置");

        let decision = watcher.should_notify(&msg);

        match decision {
            NotifyDecision::Notify { urgency, .. } => {
                assert_eq!(urgency, Urgency::High);
            }
            _ => panic!("Expected Notify decision for Chinese error"),
        }
    }

    #[test]
    fn test_should_notify_completion() {
        let notifier = OpenclawNotifier::new();
        let watcher = InboxWatcher::new(notifier);

        let msg = create_test_message("developer", "任务已完成，代码已提交");

        let decision = watcher.should_notify(&msg);

        match decision {
            NotifyDecision::Notify { urgency, .. } => {
                assert_eq!(urgency, Urgency::Medium);
            }
            _ => panic!("Expected Notify decision for completion"),
        }
    }

    #[test]
    fn test_should_notify_with_summary() {
        let notifier = OpenclawNotifier::new();
        let watcher = InboxWatcher::new(notifier);

        let mut msg = create_test_message("developer", "Some regular message");
        msg.summary = Some("Important update".to_string());

        let decision = watcher.should_notify(&msg);

        match decision {
            NotifyDecision::Notify { urgency, summary } => {
                assert_eq!(urgency, Urgency::Low);
                assert_eq!(summary, "Important update");
            }
            _ => panic!("Expected Notify decision for message with summary"),
        }
    }

    #[test]
    fn test_should_not_notify_regular_message() {
        let notifier = OpenclawNotifier::new();
        let watcher = InboxWatcher::new(notifier);

        let msg = create_test_message("developer", "Just a regular status update");

        let decision = watcher.should_notify(&msg);
        assert_eq!(decision, NotifyDecision::Silent);
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 10), "short");
        assert_eq!(truncate_text("this is a long text", 10), "this is a ...");
    }
}
