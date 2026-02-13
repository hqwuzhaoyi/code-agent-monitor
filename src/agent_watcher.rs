//! Agent 监控模块 - 监控 Agent 状态、JSONL 事件和输入等待
//!
//! Note: This module is being gradually migrated to use components from `crate::watcher`.
//! See `crate::watcher::AgentMonitor` for tmux session monitoring.
//! See `crate::watcher::EventProcessor` for JSONL event processing.
//! See `crate::watcher::StabilityDetector` for terminal stability detection.

use crate::agent::{AgentManager, AgentRecord};
use crate::input_detector::{InputWaitDetector, InputWaitResult};
use crate::jsonl_parser::{JsonlEvent, JsonlParser};
use crate::tmux::TmuxManager;
// Import new watcher module for future migration
use crate::watcher::{AgentMonitor, EventProcessor, StabilityDetector};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, debug};

/// 通知动作
#[derive(Debug, Clone, PartialEq, Eq)]
enum NotifyAction {
    /// 发送通知
    Send,
    /// 发送提醒（用户长时间未响应）
    SendReminder,
    /// 被抑制（锁定期内或已发送）
    Suppressed,
}

/// 监控事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum WatchEvent {
    /// Agent 退出
    AgentExited {
        agent_id: String,
        project_path: String,
    },
    /// 工具调用
    ToolUse {
        agent_id: String,
        tool_name: String,
        tool_target: Option<String>,
        timestamp: Option<String>,
    },
    /// 工具调用批次（多个工具调用合并）
    ToolUseBatch {
        agent_id: String,
        tools: Vec<String>,
        timestamp: Option<String>,
    },
    /// 错误
    Error {
        agent_id: String,
        message: String,
        timestamp: Option<String>,
    },
    /// 等待输入
    WaitingForInput {
        agent_id: String,
        pattern_type: String,
        context: String,
    },
    /// Agent 恢复运行（从等待状态）
    AgentResumed {
        agent_id: String,
    },
}

/// Agent 状态快照
#[derive(Debug, Clone)]
pub struct AgentSnapshot {
    /// Agent 记录
    pub record: AgentRecord,
    /// 最近的工具调用
    pub recent_tools: Vec<JsonlEvent>,
    /// 最近的错误
    pub recent_errors: Vec<JsonlEvent>,
    /// 是否在等待输入
    pub waiting_for_input: Option<InputWaitResult>,
    /// 最后活动时间
    pub last_activity: Option<String>,
}

/// 通知锁定记录（时间窗口锁定方案）
#[derive(Debug, Clone)]
struct NotificationLock {
    /// 首次通知时间（Unix 时间戳秒）- 用于计算总超时
    first_notified_at: u64,
    /// 锁定开始时间（Unix 时间戳秒）
    locked_at: u64,
    /// 锁定时的内容指纹
    content_fingerprint: u64,
    /// 是否已发送提醒
    reminder_sent: bool,
}

/// Terminal stability state for AI call optimization
#[derive(Debug, Clone)]
struct StabilityState {
    /// Terminal content hash
    content_hash: u64,
    /// Timestamp when this hash was first seen (Unix seconds)
    first_seen_at: u64,
    /// Number of consecutive polls with same hash
    consecutive_count: u32,
    /// Whether AI check has been performed for this stable state
    ai_checked: bool,
}

impl StabilityState {
    fn new(hash: u64, now: u64) -> Self {
        Self {
            content_hash: hash,
            first_seen_at: now,
            consecutive_count: 1,
            ai_checked: false,
        }
    }

    /// Update state with new hash, returns true if content changed
    fn update(&mut self, hash: u64, now: u64) -> bool {
        if hash == self.content_hash {
            self.consecutive_count += 1;
            false
        } else {
            self.content_hash = hash;
            self.first_seen_at = now;
            self.consecutive_count = 1;
            self.ai_checked = false;
            true
        }
    }

    /// Check if terminal has been stable for threshold duration
    fn is_stable(&self, now: u64, threshold_secs: u64) -> bool {
        now.saturating_sub(self.first_seen_at) >= threshold_secs
    }

    /// Mark that AI check has been performed
    fn mark_ai_checked(&mut self) {
        self.ai_checked = true;
    }
}

/// Hook event tracker for cross-process coordination
#[derive(Debug, Clone, Default)]
struct HookEventTracker {
    /// Last hook event timestamp per agent (Unix seconds)
    last_hook_times: HashMap<String, u64>,
}

impl HookEventTracker {
    /// Record a hook event for an agent (used by tests)
    #[allow(dead_code)]
    fn record_hook(&mut self, agent_id: &str, now: u64) {
        self.last_hook_times.insert(agent_id.to_string(), now);
    }

    /// Check if agent is within quiet period (recent hook event)
    fn is_in_quiet_period(&self, agent_id: &str, now: u64, quiet_secs: u64) -> bool {
        self.last_hook_times
            .get(agent_id)
            .map(|&last_time| now.saturating_sub(last_time) < quiet_secs)
            .unwrap_or(false)
    }

    /// Clear tracking for an agent
    fn clear(&mut self, agent_id: &str) {
        self.last_hook_times.remove(agent_id);
    }
}

/// Agent 监控器
pub struct AgentWatcher {
    /// Agent 管理器
    agent_manager: AgentManager,
    /// tmux 管理器
    tmux: TmuxManager,
    /// 输入等待检测器
    input_detector: InputWaitDetector,
    /// 每个 agent 的 JSONL 解析器
    jsonl_parsers: HashMap<String, JsonlParser>,
    /// 每个 agent 的通知锁定状态
    notification_locks: HashMap<String, NotificationLock>,
    /// 每个 agent 的上次等待状态（用于检测恢复）
    last_waiting_state: HashMap<String, bool>,
    /// 每个 agent 的终端稳定性状态
    stability_states: HashMap<String, StabilityState>,
    /// Hook 事件追踪器
    hook_tracker: HookEventTracker,
    /// New watcher module agent monitor (for gradual migration)
    agent_monitor: AgentMonitor,
}

impl AgentWatcher {
    /// 通知锁定时长（30 分钟）
    const LOCK_DURATION_SECS: u64 = 1800;
    /// 提醒延迟（锁定结束后 30 分钟发送提醒）
    const REMINDER_DELAY_SECS: u64 = 1800;
    /// 最大通知时限（2 小时后停止发送）
    const MAX_NOTIFICATION_DURATION_SECS: u64 = 7200;
    /// Terminal stability threshold (seconds)
    const STABILITY_THRESHOLD_SECS: u64 = 6;
    /// Hook quiet period - skip AI check if hook event within this window (seconds)
    const HOOK_QUIET_PERIOD_SECS: u64 = 10;

    /// 创建新的监控器
    pub fn new() -> Self {
        Self {
            agent_manager: AgentManager::new(),
            tmux: TmuxManager::new(),
            input_detector: InputWaitDetector::new(),
            jsonl_parsers: HashMap::new(),
            notification_locks: HashMap::new(),
            last_waiting_state: HashMap::new(),
            stability_states: HashMap::new(),
            hook_tracker: HookEventTracker::default(),
            agent_monitor: AgentMonitor::new(),
        }
    }

    /// 创建用于测试的监控器
    pub fn new_for_test() -> Self {
        Self {
            agent_manager: AgentManager::new_for_test(),
            tmux: TmuxManager::new(),
            input_detector: InputWaitDetector::new(),
            jsonl_parsers: HashMap::new(),
            notification_locks: HashMap::new(),
            last_waiting_state: HashMap::new(),
            stability_states: HashMap::new(),
            hook_tracker: HookEventTracker::default(),
            agent_monitor: AgentMonitor::new(),
        }
    }

    /// Check if agent is alive using new watcher module
    /// This method demonstrates the migration path to the new watcher module
    pub fn is_agent_alive(&self, agent: &AgentRecord) -> bool {
        self.agent_monitor.is_alive(agent)
    }

    /// 获取当前 Unix 时间戳（秒）
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// 计算内容指纹
    fn content_fingerprint(content: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        // 规范化内容：移除动画字符和时间相关内容
        let normalized = Self::normalize_content(content);

        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        hasher.finish()
    }

    /// 规范化内容（移除噪声）
    fn normalize_content(content: &str) -> String {
        content
            .lines()
            // 移除包含动画指示器的行
            .filter(|line| {
                !line.contains("Flowing")
                    && !line.contains("Brewing")
                    && !line.contains("Thinking")
                    && !line.contains("Running…")
                    && !line.contains("tokens")
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    /// 检查是否应该发送通知（时间窗口锁定方案）
    fn should_send_notification(&mut self, agent_id: &str, content: &str) -> NotifyAction {
        let now = Self::current_timestamp();
        let fingerprint = Self::content_fingerprint(content);

        if let Some(lock) = self.notification_locks.get_mut(agent_id) {
            let elapsed = now.saturating_sub(lock.locked_at);
            let total_elapsed = now.saturating_sub(lock.first_notified_at);

            // 超过 2 小时，停止发送任何通知
            if total_elapsed >= Self::MAX_NOTIFICATION_DURATION_SECS {
                debug!(
                    agent_id = %agent_id,
                    total_elapsed_secs = total_elapsed,
                    "Max notification duration exceeded, suppressing"
                );
                return NotifyAction::Suppressed;
            }

            // 锁定期内
            if elapsed < Self::LOCK_DURATION_SECS {
                // 内容不同，发送新通知并重新锁定
                if fingerprint != lock.content_fingerprint {
                    debug!(
                        agent_id = %agent_id,
                        elapsed_secs = elapsed,
                        "Different content detected, sending new notification"
                    );
                    lock.locked_at = now;
                    lock.content_fingerprint = fingerprint;
                    lock.reminder_sent = false;
                    // 注意：不重置 first_notified_at，保持总时间计算
                    return NotifyAction::Send;
                }
                // 相同内容，抑制
                return NotifyAction::Suppressed;
            }

            // 锁定期结束，检查是否需要发送提醒
            if elapsed >= Self::LOCK_DURATION_SECS + Self::REMINDER_DELAY_SECS {
                // 内容相同且未发送提醒
                if fingerprint == lock.content_fingerprint && !lock.reminder_sent {
                    debug!(
                        agent_id = %agent_id,
                        elapsed_secs = elapsed,
                        "Sending reminder notification"
                    );
                    lock.reminder_sent = true;
                    return NotifyAction::SendReminder;
                }

                // 内容不同，发送新通知并重新锁定
                if fingerprint != lock.content_fingerprint {
                    lock.locked_at = now;
                    lock.content_fingerprint = fingerprint;
                    lock.reminder_sent = false;
                    return NotifyAction::Send;
                }

                // 已发送过提醒，抑制
                return NotifyAction::Suppressed;
            }

            // 锁定期刚结束，等待提醒时机
            if fingerprint == lock.content_fingerprint {
                return NotifyAction::Suppressed;
            }

            // 内容不同，发送新通知
            lock.locked_at = now;
            lock.content_fingerprint = fingerprint;
            lock.reminder_sent = false;
            return NotifyAction::Send;
        }

        // 首次通知，创建锁定
        self.notification_locks.insert(agent_id.to_string(), NotificationLock {
            first_notified_at: now,
            locked_at: now,
            content_fingerprint: fingerprint,
            reminder_sent: false,
        });

        NotifyAction::Send
    }

    /// 清除 agent 的通知锁定（当 agent 恢复运行时调用）
    fn clear_notification_lock(&mut self, agent_id: &str) {
        self.notification_locks.remove(agent_id);
    }

    /// Determine if AI check should be performed (used by tests)
    #[allow(dead_code)]
    fn should_check_ai(
        &self,
        agent_id: &str,
        stability: &StabilityState,
        now: u64,
        content_changed: bool,
    ) -> bool {
        // Condition 1: Content just changed - wait for stability
        if content_changed {
            return false;
        }

        // Condition 2: Already checked for this stable state
        if stability.ai_checked {
            return false;
        }

        // Condition 3: Not stable long enough
        if !stability.is_stable(now, Self::STABILITY_THRESHOLD_SECS) {
            return false;
        }

        // Condition 4: Recent hook event - let hook flow handle it
        if self.hook_tracker.is_in_quiet_period(agent_id, now, Self::HOOK_QUIET_PERIOD_SECS) {
            return false;
        }

        true
    }

    /// Get skip reason for debug logging (used by tests)
    #[allow(dead_code)]
    fn skip_reason(&self, agent_id: &str, stability: &StabilityState, now: u64) -> &'static str {
        if stability.ai_checked {
            "already_checked"
        } else if !stability.is_stable(now, Self::STABILITY_THRESHOLD_SECS) {
            "not_stable_yet"
        } else if self.hook_tracker.is_in_quiet_period(agent_id, now, Self::HOOK_QUIET_PERIOD_SECS) {
            "recent_hook_event"
        } else {
            "unknown"
        }
    }

    /// Load hook events from file (cross-process coordination)
    fn load_hook_events(&mut self) {
        let hook_file = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".config/code-agent-monitor")
            .join("last_hook_events.json");

        if hook_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&hook_file) {
                if let Ok(events) = serde_json::from_str::<HashMap<String, u64>>(&content) {
                    for (agent_id, timestamp) in events {
                        self.hook_tracker.last_hook_times.insert(agent_id, timestamp);
                    }
                }
            }
        }
    }

    /// 执行一次轮询，返回检测到的事件
    pub fn poll_once(&mut self) -> Result<Vec<WatchEvent>> {
        let mut events = Vec::new();

        // Load latest hook events for coordination
        self.load_hook_events();

        // 获取所有活跃的 agent
        let agents = self.agent_manager.list_agents()?;
        debug!(agent_count = agents.len(), "Polling agents");
        for agent in &agents {
            debug!(agent_id = %agent.agent_id, "  - checking agent");
        }

        // 检查每个 agent
        for agent in &agents {
            // 1. 检查 tmux session 是否存活
            if !self.tmux.session_exists(&agent.tmux_session) {
                info!(agent_id = %agent.agent_id, "Agent tmux session exited");
                events.push(WatchEvent::AgentExited {
                    agent_id: agent.agent_id.clone(),
                    project_path: agent.project_path.clone(),
                });
                self.cleanup_agent(&agent.agent_id);
                continue;
            }

            // 2. 解析 JSONL 新事件
            if let Some(ref jsonl_path) = agent.jsonl_path {
                let parser = self.jsonl_parsers
                    .entry(agent.agent_id.clone())
                    .or_insert_with(|| {
                        let mut p = JsonlParser::new(jsonl_path);
                        p.set_position(agent.jsonl_offset);
                        p
                    });

                if let Ok(new_events) = parser.read_new_events() {
                    for event in new_events {
                        match &event {
                            JsonlEvent::ToolUse { tool_name, input, timestamp, .. } => {
                                let tool_target = crate::jsonl_parser::extract_tool_target_from_input(tool_name, input);
                                events.push(WatchEvent::ToolUse {
                                    agent_id: agent.agent_id.clone(),
                                    tool_name: tool_name.clone(),
                                    tool_target,
                                    timestamp: timestamp.clone(),
                                });
                            }
                            JsonlEvent::Error { message, timestamp } => {
                                events.push(WatchEvent::Error {
                                    agent_id: agent.agent_id.clone(),
                                    message: message.clone(),
                                    timestamp: timestamp.clone(),
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }

            // 3. 检测输入等待状态（带稳定性检测优化）
            if let Ok(output) = self.tmux.capture_pane(&agent.tmux_session, 50) {
                let now = Self::current_timestamp();
                let content_hash = Self::content_fingerprint(&output);
                let agent_id = agent.agent_id.clone();

                // Update stability state
                let stability = self.stability_states
                    .entry(agent_id.clone())
                    .or_insert_with(|| StabilityState::new(content_hash, now));
                let content_changed = stability.update(content_hash, now);

                // Extract stability info for decision making
                let ai_checked = stability.ai_checked;
                let is_stable = stability.is_stable(now, Self::STABILITY_THRESHOLD_SECS);

                // Check if AI detection should be performed
                let in_quiet_period = self.hook_tracker.is_in_quiet_period(&agent_id, now, Self::HOOK_QUIET_PERIOD_SECS);

                let should_check = !content_changed && !ai_checked && is_stable && !in_quiet_period;

                if !should_check {
                    let skip_reason = if ai_checked {
                        "already_checked"
                    } else if !is_stable {
                        "not_stable_yet"
                    } else if in_quiet_period {
                        "recent_hook_event"
                    } else {
                        "content_changed"
                    };

                    debug!(
                        agent_id = %agent_id,
                        reason = skip_reason,
                        "Skipping AI check (stability optimization)"
                    );

                    // Still need to track waiting state for resume detection
                    let was_waiting = self.last_waiting_state.get(&agent_id).copied().unwrap_or(false);
                    if was_waiting {
                        // Content changed while waiting - might have resumed
                        // Will be detected on next stable check
                    }
                    continue;
                }

                // Perform AI detection
                let wait_result = self.input_detector.detect_immediate(&output);

                // Mark AI checked
                if let Some(stability) = self.stability_states.get_mut(&agent_id) {
                    stability.mark_ai_checked();
                }

                let was_waiting = self.last_waiting_state.get(&agent_id).copied().unwrap_or(false);

                debug!(
                    agent_id = %agent_id,
                    is_waiting = wait_result.is_waiting,
                    pattern = ?wait_result.pattern_type,
                    was_waiting = was_waiting,
                    "Input wait detection (AI called)"
                );

                if wait_result.is_waiting {
                    // 检查是否应该发送通知
                    let action = self.should_send_notification(&agent_id, &wait_result.context);

                    match action {
                        NotifyAction::Send => {
                            let pattern_type = wait_result.pattern_type
                                .as_ref()
                                .map(|p| format!("{:?}", p))
                                .unwrap_or_else(|| "Unknown".to_string());

                            info!(
                                agent_id = %agent_id,
                                pattern_type = %pattern_type,
                                "Agent waiting for input, sending notification"
                            );

                            events.push(WatchEvent::WaitingForInput {
                                agent_id: agent_id.clone(),
                                pattern_type,
                                context: wait_result.context.clone(),
                            });
                        }
                        NotifyAction::SendReminder => {
                            let pattern_type = wait_result.pattern_type
                                .as_ref()
                                .map(|p| format!("{:?}", p))
                                .unwrap_or_else(|| "Unknown".to_string());

                            info!(
                                agent_id = %agent_id,
                                pattern_type = %pattern_type,
                                "Agent still waiting, sending reminder"
                            );

                            events.push(WatchEvent::WaitingForInput {
                                agent_id: agent_id.clone(),
                                pattern_type: format!("{} (提醒)", pattern_type),
                                context: wait_result.context.clone(),
                            });
                        }
                        NotifyAction::Suppressed => {
                            debug!(
                                agent_id = %agent_id,
                                "Notification suppressed (within lock window)"
                            );
                        }
                    }
                } else {
                    // 不在等待状态
                    if was_waiting {
                        info!(agent_id = %agent_id, "Agent resumed from waiting state");
                        self.clear_notification_lock(&agent_id);
                        events.push(WatchEvent::AgentResumed {
                            agent_id: agent_id.clone(),
                        });
                    }
                }

                self.last_waiting_state.insert(agent_id, wait_result.is_waiting);
            }
        }

        if !events.is_empty() {
            info!(event_count = events.len(), "Poll generated events");
            for event in &events {
                info!(event = ?event, "  - event");
            }
        }

        Ok(events)
    }

    /// 获取 agent 的当前状态快照
    pub fn get_agent_snapshot(&mut self, agent_id: &str) -> Result<Option<AgentSnapshot>> {
        let agent = match self.agent_manager.get_agent(agent_id)? {
            Some(a) => a,
            None => return Ok(None),
        };

        // 获取最近的工具调用和错误
        let (recent_tools, recent_errors) = if let Some(ref jsonl_path) = agent.jsonl_path {
            let mut parser = JsonlParser::new(jsonl_path);
            let tools = parser.get_recent_tool_calls(5).unwrap_or_default();
            let errors = parser.get_recent_errors(3).unwrap_or_default();
            (tools, errors)
        } else {
            (Vec::new(), Vec::new())
        };

        // 检测输入等待状态
        let waiting_for_input = if let Ok(output) = self.tmux.capture_pane(&agent.tmux_session, 50) {
            let result = self.input_detector.detect_immediate(&output);
            if result.is_waiting {
                Some(result)
            } else {
                None
            }
        } else {
            None
        };

        // 获取最后活动时间
        let last_activity = recent_tools.last()
            .and_then(|e| {
                if let JsonlEvent::ToolUse { timestamp, .. } = e {
                    timestamp.clone()
                } else {
                    None
                }
            });

        Ok(Some(AgentSnapshot {
            record: agent,
            recent_tools,
            recent_errors,
            waiting_for_input,
            last_activity,
        }))
    }

    /// 获取所有 agent 的状态快照
    pub fn get_all_snapshots(&mut self) -> Result<Vec<AgentSnapshot>> {
        let agents = self.agent_manager.list_agents()?;
        let mut snapshots = Vec::new();

        for agent in agents {
            if let Some(snapshot) = self.get_agent_snapshot(&agent.agent_id)? {
                snapshots.push(snapshot);
            }
        }

        Ok(snapshots)
    }

    /// 清理 agent 相关状态
    fn cleanup_agent(&mut self, agent_id: &str) {
        self.jsonl_parsers.remove(agent_id);
        self.notification_locks.remove(agent_id);
        self.last_waiting_state.remove(agent_id);
        self.input_detector.clear_session(agent_id);
        self.stability_states.remove(agent_id);
        self.hook_tracker.clear(agent_id);
    }

    /// 获取 agent 管理器引用
    pub fn agent_manager(&self) -> &AgentManager {
        &self.agent_manager
    }

    /// 获取 agent 管理器可变引用
    pub fn agent_manager_mut(&mut self) -> &mut AgentManager {
        &mut self.agent_manager
    }

    /// 轮询一次并只返回关键事件（退出、错误、等待输入）
    pub fn poll_critical_events(&mut self) -> Result<Vec<WatchEvent>> {
        let all_events = self.poll_once()?;

        Ok(all_events
            .into_iter()
            .filter(|e| matches!(
                e,
                WatchEvent::AgentExited { .. } |
                WatchEvent::Error { .. } |
                WatchEvent::WaitingForInput { .. }
            ))
            .collect())
    }
}

impl Default for AgentWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// 格式化 WatchEvent 为人类可读的通知消息
pub fn format_watch_event(event: &WatchEvent) -> String {
    match event {
        WatchEvent::AgentExited { agent_id, project_path } => {
            format!("✅ Agent 退出: {} ({})", agent_id, project_path)
        }
        WatchEvent::ToolUse { agent_id, tool_name, tool_target, .. } => {
            if let Some(target) = tool_target {
                format!("🔧 {} 执行: {} {}", agent_id, tool_name, target)
            } else {
                format!("🔧 {} 执行: {}", agent_id, tool_name)
            }
        }
        WatchEvent::ToolUseBatch { agent_id, tools, .. } => {
            format!("🔧 {} 执行: {}", agent_id, tools.join(", "))
        }
        WatchEvent::Error { agent_id, message, .. } => {
            let preview = if message.len() > 100 {
                format!("{}...", &message[..97])
            } else {
                message.clone()
            };
            format!("❌ {} 错误: {}", agent_id, preview)
        }
        WatchEvent::WaitingForInput { agent_id, pattern_type, context } => {
            let preview = if context.len() > 200 {
                format!("{}...", &context[..197])
            } else {
                context.clone()
            };
            format!("⏸️ {} 等待输入 ({}):\n{}", agent_id, pattern_type, preview)
        }
        WatchEvent::AgentResumed { agent_id } => {
            format!("▶️ {} 继续执行", agent_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_watch_event_agent_exited() {
        let event = WatchEvent::AgentExited {
            agent_id: "cam-123".to_string(),
            project_path: "/workspace/myapp".to_string(),
        };

        let formatted = format_watch_event(&event);
        assert!(formatted.contains("cam-123"));
        assert!(formatted.contains("退出"));
    }

    #[test]
    fn test_format_watch_event_tool_use() {
        let event = WatchEvent::ToolUse {
            agent_id: "cam-123".to_string(),
            tool_name: "Edit".to_string(),
            tool_target: Some("main.rs".to_string()),
            timestamp: None,
        };

        let formatted = format_watch_event(&event);
        assert!(formatted.contains("Edit"));
        assert!(formatted.contains("main.rs"));
    }

    #[test]
    fn test_format_watch_event_waiting() {
        let event = WatchEvent::WaitingForInput {
            agent_id: "cam-123".to_string(),
            pattern_type: "Confirmation".to_string(),
            context: "Continue? [Y/n]".to_string(),
        };

        let formatted = format_watch_event(&event);
        assert!(formatted.contains("等待输入"));
        assert!(formatted.contains("Confirmation"));
    }

    #[test]
    fn test_poll_critical_events_filters() {
        // 这个测试验证过滤逻辑的正确性
        let events = vec![
            WatchEvent::ToolUse {
                agent_id: "cam-123".to_string(),
                tool_name: "Read".to_string(),
                tool_target: None,
                timestamp: None,
            },
            WatchEvent::AgentExited {
                agent_id: "cam-123".to_string(),
                project_path: "/tmp".to_string(),
            },
            WatchEvent::AgentResumed {
                agent_id: "cam-123".to_string(),
            },
            WatchEvent::Error {
                agent_id: "cam-123".to_string(),
                message: "error".to_string(),
                timestamp: None,
            },
        ];

        let critical: Vec<_> = events
            .into_iter()
            .filter(|e| matches!(
                e,
                WatchEvent::AgentExited { .. } |
                WatchEvent::Error { .. } |
                WatchEvent::WaitingForInput { .. }
            ))
            .collect();

        assert_eq!(critical.len(), 2);
    }

    // === StabilityState tests ===

    #[test]
    fn test_stability_state_new() {
        let state = StabilityState::new(12345, 1000);
        assert_eq!(state.content_hash, 12345);
        assert_eq!(state.first_seen_at, 1000);
        assert_eq!(state.consecutive_count, 1);
        assert!(!state.ai_checked);
    }

    #[test]
    fn test_stability_state_update_same_hash() {
        let mut state = StabilityState::new(12345, 1000);
        let changed = state.update(12345, 1001);
        assert!(!changed);
        assert_eq!(state.consecutive_count, 2);
        assert_eq!(state.first_seen_at, 1000); // unchanged
    }

    #[test]
    fn test_stability_state_update_different_hash() {
        let mut state = StabilityState::new(12345, 1000);
        state.ai_checked = true;
        let changed = state.update(67890, 1002);
        assert!(changed);
        assert_eq!(state.consecutive_count, 1);
        assert_eq!(state.first_seen_at, 1002);
        assert!(!state.ai_checked); // reset
    }

    #[test]
    fn test_stability_state_is_stable() {
        let state = StabilityState::new(12345, 1000);
        assert!(!state.is_stable(1005, 6)); // 5 secs, not stable
        assert!(state.is_stable(1006, 6));  // 6 secs, stable
        assert!(state.is_stable(1010, 6));  // 10 secs, stable
    }

    // === HookEventTracker tests ===

    #[test]
    fn test_hook_tracker_record_and_check() {
        let mut tracker = HookEventTracker::default();
        tracker.record_hook("agent-1", 1000);

        // Within quiet period (10 secs)
        assert!(tracker.is_in_quiet_period("agent-1", 1005, 10));
        // After quiet period
        assert!(!tracker.is_in_quiet_period("agent-1", 1015, 10));
        // Different agent
        assert!(!tracker.is_in_quiet_period("agent-2", 1005, 10));
    }

    #[test]
    fn test_hook_tracker_clear() {
        let mut tracker = HookEventTracker::default();
        tracker.record_hook("agent-1", 1000);
        assert!(tracker.is_in_quiet_period("agent-1", 1005, 10));

        tracker.clear("agent-1");
        assert!(!tracker.is_in_quiet_period("agent-1", 1005, 10));
    }

    // === should_check_ai tests ===

    #[test]
    fn test_should_check_ai_content_changed() {
        let watcher = AgentWatcher::new_for_test();
        let state = StabilityState::new(12345, 1000);
        // Content just changed - should NOT check
        assert!(!watcher.should_check_ai("agent-1", &state, 1000, true));
    }

    #[test]
    fn test_should_check_ai_already_checked() {
        let watcher = AgentWatcher::new_for_test();
        let mut state = StabilityState::new(12345, 1000);
        state.ai_checked = true;
        // Already checked - should NOT check
        assert!(!watcher.should_check_ai("agent-1", &state, 1010, false));
    }

    #[test]
    fn test_should_check_ai_not_stable() {
        let watcher = AgentWatcher::new_for_test();
        let state = StabilityState::new(12345, 1000);
        // Only 3 seconds stable - should NOT check (threshold is 6)
        assert!(!watcher.should_check_ai("agent-1", &state, 1003, false));
    }

    #[test]
    fn test_should_check_ai_all_conditions_met() {
        let watcher = AgentWatcher::new_for_test();
        let state = StabilityState::new(12345, 1000);
        // 10 seconds stable, not checked, no recent hook - should check
        assert!(watcher.should_check_ai("agent-1", &state, 1010, false));
    }
}
