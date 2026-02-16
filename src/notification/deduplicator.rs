//! 通知去重器 - 防止短时间内发送重复通知
//!
//! ## 去重策略（统一时间窗口锁定方案）
//! 1. 首次通知 → 发送，锁定 30 分钟
//! 2. 锁定期内相同内容 → 抑制
//! 3. 锁定期内内容变化 → 发送新通知，重置锁定
//! 4. 锁定期结束后 30 分钟 → 发送提醒（如果内容相同）
//! 5. 2 小时后 → 停止发送任何通知
//!
//! ## 持久化
//! 去重状态持久化到 `~/.config/code-agent-monitor/dedup_state.json`，
//! 使用 fs2 文件锁确保跨进程并发安全。

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tracing::debug;
use fs2::FileExt;
use std::io::{Read, Write};

/// 通知动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyAction {
    /// 发送通知
    Send,
    /// 发送提醒（用户长时间未响应）
    SendReminder,
    /// 被抑制
    Suppressed(String),
}

/// 通知锁定记录
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotificationLock {
    /// 首次通知时间（Unix 时间戳秒）- 用于计算总超时
    first_notified_at: u64,
    /// 锁定开始时间（Unix 时间戳秒）- 用于计算锁定窗口
    locked_at: u64,
    /// 内容指纹（hash）
    content_fingerprint: u64,
    /// 是否已发送提醒
    reminder_sent: bool,
}

/// 持久化状态
#[derive(Debug, Default, Serialize, Deserialize)]
struct DedupState {
    /// agent_id -> NotificationLock
    locks: HashMap<String, NotificationLock>,
}

/// 通知去重器
pub struct NotificationDeduplicator {
    /// agent_id -> NotificationLock
    locks: HashMap<String, NotificationLock>,
    /// 是否启用持久化
    persist: bool,
    /// 自定义状态文件路径（用于测试）
    #[cfg(test)]
    custom_state_path: Option<PathBuf>,
}

impl NotificationDeduplicator {
    /// 锁定时长：30 分钟
    const LOCK_DURATION_SECS: u64 = 1800;
    /// 提醒延迟：锁定结束后 30 分钟
    const REMINDER_DELAY_SECS: u64 = 1800;
    /// 最大通知时限：2 小时后停止发送
    const MAX_NOTIFICATION_DURATION_SECS: u64 = 7200;

    /// 创建新的去重器，自动从磁盘加载之前的状态
    pub fn new() -> Self {
        let mut dedup = Self {
            locks: HashMap::new(),
            persist: true,
            #[cfg(test)]
            custom_state_path: None,
        };
        dedup.load_state();
        dedup
    }

    /// 创建不持久化的去重器（用于测试）
    #[cfg(test)]
    pub fn new_without_persistence() -> Self {
        Self {
            locks: HashMap::new(),
            persist: false,
            custom_state_path: None,
        }
    }

    /// 创建使用自定义状态文件路径的去重器（用于测试跨进程行为）
    #[cfg(test)]
    pub fn new_with_state_path(path: PathBuf) -> Self {
        let mut dedup = Self {
            locks: HashMap::new(),
            persist: true,
            custom_state_path: Some(path),
        };
        dedup.load_state();
        dedup
    }

    /// 获取状态文件路径
    fn state_file_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config/code-agent-monitor/dedup_state.json"))
    }

    /// 获取实例的状态文件路径（支持自定义路径用于测试）
    fn get_state_path(&self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(ref path) = self.custom_state_path {
            return Some(path.clone());
        }
        Self::state_file_path()
    }

    /// 从磁盘加载状态（带共享锁）
    fn load_state(&mut self) {
        if !self.persist {
            return;
        }

        let Some(path) = self.get_state_path() else {
            return;
        };

        if !path.exists() {
            return;
        }

        match std::fs::File::open(&path) {
            Ok(mut file) => {
                if file.lock_shared().is_err() {
                    debug!("Failed to acquire shared lock for reading");
                    return;
                }

                let mut content = String::new();
                if file.read_to_string(&mut content).is_ok() {
                    if let Ok(state) = serde_json::from_str::<DedupState>(&content) {
                        self.locks = state.locks;
                        debug!(records = self.locks.len(), "Loaded dedup state from disk");
                    }
                }

                let _ = file.unlock();
            }
            Err(e) => {
                debug!(error = %e, "Failed to open dedup state file");
            }
        }
    }

    /// 保存状态到磁盘（带排他锁）
    fn save_state(&self) {
        if !self.persist {
            return;
        }

        let Some(path) = self.get_state_path() else {
            return;
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let state = DedupState {
            locks: self.locks.clone(),
        };

        match std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
        {
            Ok(mut file) => {
                if file.lock_exclusive().is_err() {
                    debug!("Failed to acquire exclusive lock for writing");
                    return;
                }

                if let Ok(content) = serde_json::to_string(&state) {
                    let _ = file.write_all(content.as_bytes());
                }

                let _ = file.unlock();
            }
            Err(e) => {
                debug!(error = %e, "Failed to save dedup state");
            }
        }
    }

    /// 获取当前 Unix 时间戳（秒）
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// 计算内容指纹
    ///
    /// Uses the unified dedup_key module for consistent normalization across
    /// all notification paths (watcher, hook).
    fn content_fingerprint(content: &str) -> u64 {
        use super::dedup_key::{normalize_terminal_content, hash_content};
        let normalized = normalize_terminal_content(content);
        hash_content(&normalized)
    }

    /// 清理过期记录（超过 2 小时的）
    fn cleanup_expired(&mut self, now: u64) {
        self.locks.retain(|_, lock| {
            now.saturating_sub(lock.first_notified_at) < Self::MAX_NOTIFICATION_DURATION_SECS
        });
    }

    /// 检查是否应该发送通知
    ///
    /// IMPORTANT: Reloads state from disk before checking to enable cross-process deduplication.
    /// Multiple cam processes (watcher, hook) share state via the persisted file.
    pub fn should_send(&mut self, agent_id: &str, content: &str) -> NotifyAction {
        // Reload state from disk to see updates from other processes
        self.load_state();

        let now = Self::current_timestamp();
        let fingerprint = Self::content_fingerprint(content);

        // 先检查当前 agent 是否超过最大时限（在清理之前）
        if let Some(lock) = self.locks.get(agent_id) {
            let total_elapsed = now.saturating_sub(lock.first_notified_at);
            if total_elapsed >= Self::MAX_NOTIFICATION_DURATION_SECS {
                // 超过 2 小时，停止发送并清理记录
                self.locks.remove(agent_id);
                self.save_state();
                return NotifyAction::Suppressed("max duration exceeded".into());
            }
        }

        // 清理其他过期记录
        self.cleanup_expired(now);

        if let Some(lock) = self.locks.get_mut(agent_id) {
            let elapsed = now.saturating_sub(lock.locked_at);

            // 锁定期内
            if elapsed < Self::LOCK_DURATION_SECS {
                if fingerprint != lock.content_fingerprint {
                    // 内容变化，发送新通知并重置锁定
                    lock.locked_at = now;
                    lock.content_fingerprint = fingerprint;
                    lock.reminder_sent = false;
                    self.save_state();
                    return NotifyAction::Send;
                }
                // 相同内容，抑制
                return NotifyAction::Suppressed("within lock window".into());
            }

            // 提醒时机（锁定结束后 30 分钟）
            if elapsed >= Self::LOCK_DURATION_SECS + Self::REMINDER_DELAY_SECS {
                if fingerprint == lock.content_fingerprint && !lock.reminder_sent {
                    lock.reminder_sent = true;
                    self.save_state();
                    return NotifyAction::SendReminder;
                }
                if fingerprint != lock.content_fingerprint {
                    // 内容变化，发送新通知
                    lock.locked_at = now;
                    lock.content_fingerprint = fingerprint;
                    lock.reminder_sent = false;
                    self.save_state();
                    return NotifyAction::Send;
                }
                // 已发送提醒，抑制
                return NotifyAction::Suppressed("reminder already sent".into());
            }

            // 锁定期结束但未到提醒时机
            if fingerprint == lock.content_fingerprint {
                return NotifyAction::Suppressed("waiting for reminder window".into());
            }
            // 内容变化，发送新通知
            lock.locked_at = now;
            lock.content_fingerprint = fingerprint;
            lock.reminder_sent = false;
            self.save_state();
            return NotifyAction::Send;
        }

        // 首次通知
        self.locks.insert(agent_id.to_string(), NotificationLock {
            first_notified_at: now,
            locked_at: now,
            content_fingerprint: fingerprint,
            reminder_sent: false,
        });
        self.save_state();
        NotifyAction::Send
    }

    /// 清除 agent 的锁定（当 agent 恢复运行时调用）
    pub fn clear_lock(&mut self, agent_id: &str) {
        self.locks.remove(agent_id);
        self.save_state();
    }
}

impl Default for NotificationDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Task 2: 数据结构测试 ====================

    #[test]
    fn test_notification_lock_serialization() {
        let lock = NotificationLock {
            first_notified_at: 1700000000,
            locked_at: 1700000100,
            content_fingerprint: 12345678901234567890,
            reminder_sent: false,
        };

        let json = serde_json::to_string(&lock).unwrap();
        let deserialized: NotificationLock = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.first_notified_at, lock.first_notified_at);
        assert_eq!(deserialized.locked_at, lock.locked_at);
        assert_eq!(deserialized.content_fingerprint, lock.content_fingerprint);
        assert_eq!(deserialized.reminder_sent, lock.reminder_sent);
    }

    // ==================== Task 3: 内容指纹测试 ====================

    #[test]
    fn test_content_fingerprint_same_content() {
        let fp1 = NotificationDeduplicator::content_fingerprint("hello world");
        let fp2 = NotificationDeduplicator::content_fingerprint("hello world");
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_content_fingerprint_different_content() {
        let fp1 = NotificationDeduplicator::content_fingerprint("hello");
        let fp2 = NotificationDeduplicator::content_fingerprint("world");
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn test_content_fingerprint_tool_agnostic() {
        // Tool-specific patterns (Flowing, Brewing, etc.) are NOT filtered
        // per CLAUDE.md guidelines - CAM must be compatible with multiple AI tools
        // Noise filtering is done by AI extraction layer (src/anthropic.rs)
        let content1 = "Question?\nFlowing...\nMore text";
        let content2 = "Question?\nBrewing...\nMore text";
        let fp1 = NotificationDeduplicator::content_fingerprint(content1);
        let fp2 = NotificationDeduplicator::content_fingerprint(content2);
        // Different tool-specific content should produce different fingerprints
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn test_content_fingerprint_ignores_ansi_and_timestamps() {
        // ANSI codes and timestamps ARE filtered (tool-agnostic)
        let content1 = "\x1b[32m[10:30:00]\x1b[0m Question?";
        let content2 = "\x1b[31m[11:45:30]\x1b[0m Question?";
        let fp1 = NotificationDeduplicator::content_fingerprint(content1);
        let fp2 = NotificationDeduplicator::content_fingerprint(content2);
        assert_eq!(fp1, fp2);
    }

    // ==================== Task 4: 文件锁测试 ====================

    #[test]
    fn test_file_lock_load_save_roundtrip() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("test_dedup.json");

        // 创建初始状态
        let mut locks = HashMap::new();
        locks.insert("agent-1".to_string(), NotificationLock {
            first_notified_at: 1000,
            locked_at: 1000,
            content_fingerprint: 12345,
            reminder_sent: false,
        });
        let state = DedupState { locks };

        // 保存
        let content = serde_json::to_string(&state).unwrap();
        std::fs::write(&path, &content).unwrap();

        // 加载
        let loaded_content = std::fs::read_to_string(&path).unwrap();
        let loaded_state: DedupState = serde_json::from_str(&loaded_content).unwrap();

        assert_eq!(loaded_state.locks.len(), 1);
        assert!(loaded_state.locks.contains_key("agent-1"));
    }

    // ==================== Task 5: 核心去重逻辑测试 ====================

    #[test]
    fn test_should_send_first_notification() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let action = dedup.should_send("agent-1", "Question?");
        assert_eq!(action, NotifyAction::Send);
    }

    #[test]
    fn test_should_send_suppressed_within_lock() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();

        // 首次发送
        let action1 = dedup.should_send("agent-1", "Question?");
        assert_eq!(action1, NotifyAction::Send);

        // 锁定期内相同内容
        let action2 = dedup.should_send("agent-1", "Question?");
        assert!(matches!(action2, NotifyAction::Suppressed(_)));
    }

    #[test]
    fn test_should_send_different_content_resets_lock() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();

        let action1 = dedup.should_send("agent-1", "Question A?");
        assert_eq!(action1, NotifyAction::Send);

        // 内容变化，应该发送
        let action2 = dedup.should_send("agent-1", "Question B?");
        assert_eq!(action2, NotifyAction::Send);
    }

    // ==================== Task 6: 提醒和超时测试 ====================

    #[test]
    fn test_should_send_reminder_after_delay() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let agent_id = "agent-1";
        let content = "Question?";

        // 首次发送
        assert_eq!(dedup.should_send(agent_id, content), NotifyAction::Send);

        // 模拟时间流逝：锁定期 + 提醒延迟 = 60 分钟
        if let Some(lock) = dedup.locks.get_mut(agent_id) {
            lock.locked_at -= 3600; // 1 小时前
            lock.first_notified_at -= 3600;
        }

        // 应该发送提醒
        let action = dedup.should_send(agent_id, content);
        assert_eq!(action, NotifyAction::SendReminder);

        // 再次调用应该被抑制
        let action2 = dedup.should_send(agent_id, content);
        assert!(matches!(action2, NotifyAction::Suppressed(_)));
    }

    #[test]
    fn test_should_send_suppressed_after_max_duration() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let agent_id = "agent-1";
        let content = "Question?";

        // 首次发送
        assert_eq!(dedup.should_send(agent_id, content), NotifyAction::Send);

        // 模拟时间流逝：超过 2 小时
        if let Some(lock) = dedup.locks.get_mut(agent_id) {
            lock.first_notified_at -= 7201; // 超过 2 小时
            lock.locked_at -= 7201;
        }

        // 应该被永久抑制
        let action = dedup.should_send(agent_id, content);
        assert!(matches!(action, NotifyAction::Suppressed(reason) if reason.contains("max duration")));
    }

    // ==================== 其他测试 ====================

    #[test]
    fn test_different_agents_not_deduplicated() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let content = "Same question?";

        // 不同 agent 的相同内容应该都能发送
        assert_eq!(dedup.should_send("agent-1", content), NotifyAction::Send);
        assert_eq!(dedup.should_send("agent-2", content), NotifyAction::Send);
        assert_eq!(dedup.should_send("agent-3", content), NotifyAction::Send);
    }

    #[test]
    fn test_clear_lock() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let agent_id = "agent-1";
        let content = "Question?";

        // 首次发送
        assert_eq!(dedup.should_send(agent_id, content), NotifyAction::Send);

        // 锁定期内被抑制
        assert!(matches!(dedup.should_send(agent_id, content), NotifyAction::Suppressed(_)));

        // 清除锁定
        dedup.clear_lock(agent_id);

        // 可以再次发送
        assert_eq!(dedup.should_send(agent_id, content), NotifyAction::Send);
    }

    #[test]
    fn test_persistence_disabled_in_test_mode() {
        let dedup = NotificationDeduplicator::new_without_persistence();
        assert!(!dedup.persist);
    }

    #[test]
    fn test_persistence_enabled_by_default() {
        let dedup = NotificationDeduplicator::new();
        assert!(dedup.persist);
    }

    #[test]
    fn test_state_file_path() {
        let path = NotificationDeduplicator::state_file_path();
        assert!(path.is_some());
        let path = path.unwrap();
        assert!(path.to_string_lossy().contains(".config/code-agent-monitor"));
        assert!(path.to_string_lossy().contains("dedup_state.json"));
    }

    // ==================== Cross-process state sync tests ====================

    #[test]
    fn test_cross_process_state_sync() {
        use tempfile::tempdir;
        use std::env;

        // Create a temp directory for test state
        let dir = tempdir().unwrap();
        let state_path = dir.path().join("dedup_state.json");

        // Simulate process 1: creates a lock
        let mut locks1 = HashMap::new();
        locks1.insert("agent-1".to_string(), NotificationLock {
            first_notified_at: NotificationDeduplicator::current_timestamp(),
            locked_at: NotificationDeduplicator::current_timestamp(),
            content_fingerprint: NotificationDeduplicator::content_fingerprint("Question?"),
            reminder_sent: false,
        });
        let state1 = DedupState { locks: locks1 };
        let content = serde_json::to_string(&state1).unwrap();
        std::fs::write(&state_path, &content).unwrap();

        // Simulate process 2: reads the state and should see the lock
        let loaded_content = std::fs::read_to_string(&state_path).unwrap();
        let loaded_state: DedupState = serde_json::from_str(&loaded_content).unwrap();

        assert!(loaded_state.locks.contains_key("agent-1"));
        assert_eq!(
            loaded_state.locks.get("agent-1").unwrap().content_fingerprint,
            NotificationDeduplicator::content_fingerprint("Question?")
        );
    }

    #[test]
    fn test_should_send_reloads_state() {
        // This test verifies that should_send() calls load_state() internally.
        // We can't easily test cross-process behavior in unit tests, but we can
        // verify the method signature and that it doesn't panic with persistence enabled.
        let mut dedup = NotificationDeduplicator::new();

        // First call should work (creates lock)
        let action1 = dedup.should_send("test-agent", "Test question?");
        assert_eq!(action1, NotifyAction::Send);

        // Second call should be suppressed (lock exists)
        let action2 = dedup.should_send("test-agent", "Test question?");
        assert!(matches!(action2, NotifyAction::Suppressed(_)));

        // Clean up
        dedup.clear_lock("test-agent");
    }

    // ==================== Enhanced cross-process deduplication tests ====================

    #[test]
    fn test_cross_process_dedup_watcher_then_hook() {
        use tempfile::tempdir;

        // Create a temp directory for isolated test state
        let dir = tempdir().unwrap();
        let state_path = dir.path().join("dedup_state.json");

        // === Simulate WATCHER process ===
        // Watcher detects a question and creates a lock
        {
            let mut watcher_dedup = NotificationDeduplicator::new_with_state_path(state_path.clone());

            // First notification from watcher - should send
            let action = watcher_dedup.should_send("agent-1", "Do you want to proceed?");
            assert_eq!(action, NotifyAction::Send, "Watcher should send first notification");

            // Verify state file was created
            assert!(state_path.exists(), "State file should be created after first notification");
        }
        // Watcher dedup instance dropped here, simulating process end

        // === Simulate HOOK process (new instance) ===
        // Hook receives same event and should see the existing lock
        {
            let mut hook_dedup = NotificationDeduplicator::new_with_state_path(state_path.clone());

            // Same notification from hook - should be suppressed
            let action = hook_dedup.should_send("agent-1", "Do you want to proceed?");
            assert!(
                matches!(action, NotifyAction::Suppressed(_)),
                "Hook should suppress duplicate notification, got: {:?}",
                action
            );
        }
    }

    #[test]
    fn test_cross_process_dedup_different_content_resets() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let state_path = dir.path().join("dedup_state.json");

        // === Process 1: Creates initial lock ===
        {
            let mut dedup1 = NotificationDeduplicator::new_with_state_path(state_path.clone());
            let action = dedup1.should_send("agent-1", "Question A?");
            assert_eq!(action, NotifyAction::Send);
        }

        // === Process 2: Different content should send and reset lock ===
        {
            let mut dedup2 = NotificationDeduplicator::new_with_state_path(state_path.clone());
            let action = dedup2.should_send("agent-1", "Question B?");
            assert_eq!(action, NotifyAction::Send, "Different content should send");
        }

        // === Process 3: Original content should now be suppressed (lock was reset to B) ===
        {
            let mut dedup3 = NotificationDeduplicator::new_with_state_path(state_path.clone());
            let action = dedup3.should_send("agent-1", "Question B?");
            assert!(
                matches!(action, NotifyAction::Suppressed(_)),
                "Same content as last should be suppressed"
            );
        }
    }

    #[test]
    fn test_cross_process_dedup_multiple_agents_independent() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let state_path = dir.path().join("dedup_state.json");

        // === Process 1: Lock agent-1 ===
        {
            let mut dedup = NotificationDeduplicator::new_with_state_path(state_path.clone());
            let action = dedup.should_send("agent-1", "Question for agent 1?");
            assert_eq!(action, NotifyAction::Send);
        }

        // === Process 2: agent-2 should not be affected by agent-1's lock ===
        {
            let mut dedup = NotificationDeduplicator::new_with_state_path(state_path.clone());
            let action = dedup.should_send("agent-2", "Question for agent 2?");
            assert_eq!(action, NotifyAction::Send, "Different agent should not be affected");
        }

        // === Process 3: agent-1 still locked ===
        {
            let mut dedup = NotificationDeduplicator::new_with_state_path(state_path.clone());
            let action = dedup.should_send("agent-1", "Question for agent 1?");
            assert!(
                matches!(action, NotifyAction::Suppressed(_)),
                "agent-1 should still be locked"
            );
        }
    }

    #[test]
    fn test_cross_process_clear_lock_propagates() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let state_path = dir.path().join("dedup_state.json");

        // === Process 1: Create lock ===
        {
            let mut dedup = NotificationDeduplicator::new_with_state_path(state_path.clone());
            dedup.should_send("agent-1", "Question?");
        }

        // === Process 2: Clear the lock ===
        {
            let mut dedup = NotificationDeduplicator::new_with_state_path(state_path.clone());
            dedup.clear_lock("agent-1");
        }

        // === Process 3: Should be able to send again ===
        {
            let mut dedup = NotificationDeduplicator::new_with_state_path(state_path.clone());
            let action = dedup.should_send("agent-1", "Question?");
            assert_eq!(action, NotifyAction::Send, "Should send after lock cleared");
        }
    }
}
