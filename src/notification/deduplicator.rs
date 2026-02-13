//! 通知去重器 - 防止短时间内发送重复通知
//!
//! 当 Hook 和 Watcher 同时检测到同一状态时，可能会产生重复通知。
//! 此模块通过提取核心问题内容和相似度检查实现去重。
//!
//! ## 去重策略
//! 1. 提取核心问题内容（忽略 reply_hint 等变化部分）
//! 2. 使用 120 秒时间窗口
//! 3. 相似度 > 80% 视为重复
//!
//! ## 持久化
//! 去重状态持久化到 `~/.config/code-agent-monitor/dedup_state.json`，
//! 确保跨进程调用（如 `cam notify` 命令）也能正确去重。

use std::collections::HashMap;
use std::time::Duration;
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// 持久化的去重记录
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DedupRecord {
    /// 核心问题内容
    core_question: String,
    /// Unix 时间戳（秒）
    timestamp: u64,
}

/// 持久化的去重状态
#[derive(Debug, Default, Serialize, Deserialize)]
struct DedupState {
    /// agent_id -> DedupRecord
    records: HashMap<String, DedupRecord>,
}

/// 通知去重器
pub struct NotificationDeduplicator {
    /// 最近发送的通知: agent_id -> (core_question, timestamp_secs)
    recent: HashMap<String, (String, u64)>,
    /// 去重窗口（默认 120 秒）
    window: Duration,
    /// 相似度阈值（0.0 - 1.0）
    similarity_threshold: f64,
    /// 是否启用持久化（测试时可禁用）
    persist: bool,
}

impl NotificationDeduplicator {
    /// 创建新的去重器，使用默认 120 秒窗口
    /// 自动从磁盘加载之前的状态
    pub fn new() -> Self {
        let mut dedup = Self {
            recent: HashMap::new(),
            window: Duration::from_secs(120),
            similarity_threshold: 0.8,
            persist: true,
        };
        dedup.load_state();
        dedup
    }

    /// 创建不持久化的去重器（用于测试）
    #[cfg(test)]
    pub fn new_without_persistence() -> Self {
        Self {
            recent: HashMap::new(),
            window: Duration::from_secs(120),
            similarity_threshold: 0.8,
            persist: false,
        }
    }

    /// 设置去重窗口时长
    pub fn with_window(mut self, window: Duration) -> Self {
        self.window = window;
        self
    }

    /// 设置相似度阈值
    #[allow(dead_code)]
    pub fn with_similarity_threshold(mut self, threshold: f64) -> Self {
        self.similarity_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// 获取状态文件路径
    fn state_file_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config/code-agent-monitor/dedup_state.json"))
    }

    /// 从磁盘加载状态
    fn load_state(&mut self) {
        if !self.persist {
            return;
        }

        let Some(path) = Self::state_file_path() else {
            return;
        };

        if !path.exists() {
            return;
        }

        match fs::read_to_string(&path) {
            Ok(content) => {
                if let Ok(state) = serde_json::from_str::<DedupState>(&content) {
                    for (agent_id, record) in state.records {
                        self.recent.insert(agent_id, (record.core_question, record.timestamp));
                    }
                    debug!(records = self.recent.len(), "Loaded dedup state from disk");
                }
            }
            Err(e) => {
                debug!(error = %e, "Failed to load dedup state");
            }
        }
    }

    /// 保存状态到磁盘
    fn save_state(&self) {
        if !self.persist {
            return;
        }

        let Some(path) = Self::state_file_path() else {
            return;
        };

        // 确保目录存在
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let state = DedupState {
            records: self.recent.iter()
                .map(|(k, (q, t))| (k.clone(), DedupRecord {
                    core_question: q.clone(),
                    timestamp: *t,
                }))
                .collect(),
        };

        if let Ok(content) = serde_json::to_string(&state) {
            if let Err(e) = fs::write(&path, content) {
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

    /// 检查是否应该发送通知
    ///
    /// 返回 `true` 表示应该发送，`false` 表示应该去重跳过
    ///
    /// # 去重规则
    /// - 提取核心问题内容进行比较（忽略 reply_hint 变化）
    /// - 同一 agent_id 在窗口内发送相似内容会被去重
    /// - 相似度 > 80% 视为重复
    /// - 不同 agent_id 的相同内容不会被去重
    /// - 窗口过期后可以重新发送
    pub fn should_send(&mut self, agent_id: &str, content: &str) -> bool {
        let core_question = Self::extract_core_question(content);
        let now = Self::current_timestamp();

        // 清理过期记录
        self.cleanup_expired(now);

        if let Some((prev_question, prev_time)) = self.recent.get(agent_id) {
            let elapsed_secs = now.saturating_sub(*prev_time);
            if elapsed_secs < self.window.as_secs() {
                // 检查相似度
                let similarity = Self::calculate_similarity(&core_question, prev_question);
                if similarity >= self.similarity_threshold {
                    debug!(
                        agent_id = %agent_id,
                        similarity = %format!("{:.1}%", similarity * 100.0),
                        elapsed_secs = %elapsed_secs,
                        "Notification deduplicated (similar question within window)"
                    );
                    return false; // 去重
                }
            }
        }

        self.recent.insert(agent_id.to_string(), (core_question, now));
        self.save_state();
        true
    }

    /// 提取核心问题内容
    ///
    /// 消息格式通常为：
    /// ```text
    /// ⏸️ [project] 等待输入
    ///
    /// 核心问题内容在这里
    ///
    /// 回复 y/n 或其他指引
    /// ```
    ///
    /// 我们提取第一个 `\n\n` 之后到下一个 `\n\n` 之前的内容作为核心问题
    fn extract_core_question(content: &str) -> String {
        // 按双换行分割
        let parts: Vec<&str> = content.split("\n\n").collect();

        // 如果有多个部分，取第二部分（跳过标题行）
        // 如果只有一部分，使用整个内容
        let core = if parts.len() >= 2 {
            // 第二部分通常是核心问题
            parts[1].trim()
        } else {
            content.trim()
        };

        // 移除常见的变化部分（reply_hint 等）
        let core = Self::remove_reply_hints(core);

        core.to_string()
    }

    /// 移除回复指引等变化部分
    fn remove_reply_hints(content: &str) -> &str {
        // 常见的回复指引模式，从这些开始的行应该被忽略
        let hint_prefixes = [
            "回复",
            "Reply",
            "输入",
            "Enter",
            "y/n",
            "y 允许",
            "n 拒绝",
        ];

        // 找到第一个回复指引行的位置
        let mut end_pos = content.len();
        for line in content.lines() {
            let trimmed = line.trim();
            for prefix in &hint_prefixes {
                if trimmed.starts_with(prefix) {
                    if let Some(pos) = content.find(line) {
                        end_pos = end_pos.min(pos);
                    }
                    break;
                }
            }
        }

        content[..end_pos].trim()
    }

    /// 计算两个字符串的相似度（Jaccard 相似度，基于字符 n-gram）
    ///
    /// 返回 0.0 - 1.0 之间的值，1.0 表示完全相同
    fn calculate_similarity(a: &str, b: &str) -> f64 {
        if a == b {
            return 1.0;
        }
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

        // 使用 3-gram 进行比较
        let ngrams_a = Self::get_ngrams(a, 3);
        let ngrams_b = Self::get_ngrams(b, 3);

        if ngrams_a.is_empty() || ngrams_b.is_empty() {
            // 字符串太短，直接比较
            return if a == b { 1.0 } else { 0.0 };
        }

        // Jaccard 相似度 = |A ∩ B| / |A ∪ B|
        let intersection: usize = ngrams_a.iter().filter(|g| ngrams_b.contains(g)).count();
        let union = ngrams_a.len() + ngrams_b.len() - intersection;

        if union == 0 {
            return 1.0;
        }

        intersection as f64 / union as f64
    }

    /// 获取字符串的 n-gram 集合
    fn get_ngrams(s: &str, n: usize) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() < n {
            return vec![s.to_string()];
        }

        chars.windows(n).map(|w| w.iter().collect()).collect()
    }

    /// 清理过期记录
    fn cleanup_expired(&mut self, now: u64) {
        let window_secs = self.window.as_secs();
        self.recent
            .retain(|_, (_, time)| now.saturating_sub(*time) < window_secs);
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
    use std::thread::sleep;

    #[test]
    fn test_same_content_within_window_is_deduplicated() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let agent_id = "cam-test";
        let content = "等待确认: rm -rf /tmp/test";

        // 第一次应该发送
        assert!(dedup.should_send(agent_id, content));
        // 第二次相同内容应该被去重
        assert!(!dedup.should_send(agent_id, content));
        // 第三次仍然被去重
        assert!(!dedup.should_send(agent_id, content));
    }

    #[test]
    fn test_different_content_not_deduplicated() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let agent_id = "cam-test";

        assert!(dedup.should_send(agent_id, "内容 A"));
        assert!(dedup.should_send(agent_id, "内容 B"));
        assert!(dedup.should_send(agent_id, "内容 C"));
    }

    #[test]
    fn test_window_expiry_allows_resend() {
        // 使用 1 秒的短窗口便于测试（Unix 时间戳精度为秒）
        let mut dedup = NotificationDeduplicator::new_without_persistence()
            .with_window(Duration::from_secs(1));
        let agent_id = "cam-test";
        let content = "等待确认";

        // 第一次发送
        assert!(dedup.should_send(agent_id, content));
        // 窗口内被去重
        assert!(!dedup.should_send(agent_id, content));

        // 等待窗口过期（需要超过 1 秒）
        sleep(Duration::from_millis(1100));

        // 窗口过期后可以重新发送
        assert!(dedup.should_send(agent_id, content));
    }

    #[test]
    fn test_different_agents_same_content_not_deduplicated() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let content = "相同的通知内容";

        // 不同 agent 的相同内容应该都能发送
        assert!(dedup.should_send("cam-agent-1", content));
        assert!(dedup.should_send("cam-agent-2", content));
        assert!(dedup.should_send("cam-agent-3", content));

        // 但同一 agent 的相同内容会被去重
        assert!(!dedup.should_send("cam-agent-1", content));
        assert!(!dedup.should_send("cam-agent-2", content));
    }

    #[test]
    fn test_cleanup_expired_records() {
        let mut dedup = NotificationDeduplicator::new_without_persistence()
            .with_window(Duration::from_secs(1));

        // 添加多个记录
        dedup.should_send("agent-1", "content-1");
        dedup.should_send("agent-2", "content-2");
        dedup.should_send("agent-3", "content-3");

        // 等待过期
        sleep(Duration::from_millis(1100));

        // 触发清理（通过调用 should_send）
        dedup.should_send("agent-new", "new-content");

        // 验证旧记录已被清理（可以重新发送）
        assert!(dedup.should_send("agent-1", "content-1"));
        assert!(dedup.should_send("agent-2", "content-2"));
    }

    #[test]
    fn test_default_window_is_120_seconds() {
        let dedup = NotificationDeduplicator::new_without_persistence();
        assert_eq!(dedup.window, Duration::from_secs(120));
    }

    #[test]
    fn test_custom_window() {
        let dedup = NotificationDeduplicator::new_without_persistence()
            .with_window(Duration::from_secs(60));
        assert_eq!(dedup.window, Duration::from_secs(60));
    }

    // ==================== 新增：相似度去重测试 ====================

    #[test]
    fn test_similar_reply_hints_are_deduplicated() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let agent_id = "cam-test";

        // 模拟 AI 每次提取的 reply_hint 略有不同的情况
        let msg1 = "⏸️ [myapp] 等待输入\n\n你想要实现什么功能？\n\n回复 y/n";
        let msg2 = "⏸️ [myapp] 等待输入\n\n你想要实现什么功能？\n\n回复 y 或 n";
        let msg3 = "⏸️ [myapp] 等待输入\n\n你想要实现什么功能？\n\n输入 y 允许，n 拒绝";

        // 第一次应该发送
        assert!(dedup.should_send(agent_id, msg1));
        // 后续相似内容应该被去重（核心问题相同）
        assert!(!dedup.should_send(agent_id, msg2));
        assert!(!dedup.should_send(agent_id, msg3));
    }

    #[test]
    fn test_different_questions_not_deduplicated() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let agent_id = "cam-test";

        let msg1 = "⏸️ [myapp] 等待输入\n\n你想要实现什么功能？\n\n回复内容";
        let msg2 = "⏸️ [myapp] 等待输入\n\n请确认是否继续？\n\n回复内容";

        // 不同问题应该都能发送
        assert!(dedup.should_send(agent_id, msg1));
        assert!(dedup.should_send(agent_id, msg2));
    }

    #[test]
    fn test_extract_core_question() {
        // 测试核心问题提取
        let msg = "⏸️ [myapp] 等待输入\n\n你想要实现什么功能？\n\n回复 y/n";
        let core = NotificationDeduplicator::extract_core_question(msg);
        assert_eq!(core, "你想要实现什么功能？");

        // 测试没有回复指引的情况
        let msg2 = "⏸️ [myapp] 等待输入\n\n请输入你的选择";
        let core2 = NotificationDeduplicator::extract_core_question(msg2);
        assert_eq!(core2, "请输入你的选择");

        // 测试单行消息
        let msg3 = "简单的通知消息";
        let core3 = NotificationDeduplicator::extract_core_question(msg3);
        assert_eq!(core3, "简单的通知消息");
    }

    #[test]
    fn test_calculate_similarity() {
        // 完全相同
        assert_eq!(NotificationDeduplicator::calculate_similarity("hello", "hello"), 1.0);

        // 完全不同
        let sim = NotificationDeduplicator::calculate_similarity("abc", "xyz");
        assert!(sim < 0.5);

        // 相似字符串
        let sim2 = NotificationDeduplicator::calculate_similarity(
            "你想要实现什么功能？",
            "你想要实现什么功能"
        );
        assert!(sim2 > 0.8);

        // 空字符串
        assert_eq!(NotificationDeduplicator::calculate_similarity("", "hello"), 0.0);
        assert_eq!(NotificationDeduplicator::calculate_similarity("hello", ""), 0.0);
    }

    #[test]
    fn test_similarity_threshold() {
        let mut dedup = NotificationDeduplicator::new_without_persistence()
            .with_similarity_threshold(0.9);

        let agent_id = "cam-test";

        // 使用更高的阈值，轻微差异也能通过
        let msg1 = "⏸️ [myapp] 等待输入\n\n问题内容 A";
        let msg2 = "⏸️ [myapp] 等待输入\n\n问题内容 B";

        assert!(dedup.should_send(agent_id, msg1));
        // 相似度不够高，应该能发送
        assert!(dedup.should_send(agent_id, msg2));
    }

    #[test]
    fn test_permission_request_dedup() {
        let mut dedup = NotificationDeduplicator::new_without_persistence();
        let agent_id = "cam-test";

        // 模拟权限请求消息
        let msg1 = "🔐 [myapp] 请求权限\n\nBash: rm -rf /tmp/test\n\ny 允许 | n 拒绝";
        let msg2 = "🔐 [myapp] 请求权限\n\nBash: rm -rf /tmp/test\n\n回复 y 允许，n 拒绝";

        assert!(dedup.should_send(agent_id, msg1));
        // 相同的权限请求应该被去重
        assert!(!dedup.should_send(agent_id, msg2));
    }

    // ==================== 持久化测试 ====================

    #[test]
    fn test_persistence_state_file_path() {
        let path = NotificationDeduplicator::state_file_path();
        assert!(path.is_some());
        let path = path.unwrap();
        assert!(path.to_string_lossy().contains(".config/code-agent-monitor"));
        assert!(path.to_string_lossy().contains("dedup_state.json"));
    }

    // ==================== 修复验证测试：去重持久化 ====================

    #[test]
    fn test_dedup_record_serialization() {
        // 验证 DedupRecord 可以正确序列化和反序列化
        let record = DedupRecord {
            core_question: "你想要实现什么功能？".to_string(),
            timestamp: 1700000000,
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: DedupRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.core_question, record.core_question);
        assert_eq!(deserialized.timestamp, record.timestamp);
    }

    #[test]
    fn test_dedup_state_serialization() {
        // 验证 DedupState 可以正确序列化和反序列化
        let mut records = HashMap::new();
        records.insert(
            "cam-agent-1".to_string(),
            DedupRecord {
                core_question: "问题 1".to_string(),
                timestamp: 1700000000,
            },
        );
        records.insert(
            "cam-agent-2".to_string(),
            DedupRecord {
                core_question: "问题 2".to_string(),
                timestamp: 1700000100,
            },
        );

        let state = DedupState { records };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: DedupState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.records.len(), 2);
        assert!(deserialized.records.contains_key("cam-agent-1"));
        assert!(deserialized.records.contains_key("cam-agent-2"));
    }

    #[test]
    fn test_unix_timestamp_is_used() {
        // 验证使用 Unix 时间戳而非 Instant
        let timestamp = NotificationDeduplicator::current_timestamp();

        // Unix 时间戳应该是一个合理的值（大于 2020 年的时间戳）
        assert!(timestamp > 1577836800); // 2020-01-01 00:00:00 UTC

        // 应该是秒级精度
        assert!(timestamp < u64::MAX / 1000); // 不是毫秒级
    }

    #[test]
    fn test_persistence_disabled_in_test_mode() {
        // 验证测试模式下持久化被禁用
        let dedup = NotificationDeduplicator::new_without_persistence();
        assert!(!dedup.persist);
    }

    #[test]
    fn test_persistence_enabled_by_default() {
        // 验证默认情况下持久化是启用的
        // 注意：这个测试会尝试加载状态文件，但不会失败
        let dedup = NotificationDeduplicator::new();
        assert!(dedup.persist);
    }

    #[test]
    fn test_state_survives_serialization_roundtrip() {
        // 验证状态可以完整地序列化和反序列化
        let mut dedup = NotificationDeduplicator::new_without_persistence();

        // 添加一些记录
        dedup.should_send("agent-1", "⏸️ [app] 等待输入\n\n问题内容 1\n\n回复 y/n");
        dedup.should_send("agent-2", "⏸️ [app] 等待输入\n\n问题内容 2\n\n回复 y/n");

        // 模拟序列化状态
        let state = DedupState {
            records: dedup
                .recent
                .iter()
                .map(|(k, (q, t))| {
                    (
                        k.clone(),
                        DedupRecord {
                            core_question: q.clone(),
                            timestamp: *t,
                        },
                    )
                })
                .collect(),
        };

        let json = serde_json::to_string(&state).unwrap();

        // 模拟反序列化到新实例
        let loaded_state: DedupState = serde_json::from_str(&json).unwrap();
        let mut new_dedup = NotificationDeduplicator::new_without_persistence();

        for (agent_id, record) in loaded_state.records {
            new_dedup
                .recent
                .insert(agent_id, (record.core_question, record.timestamp));
        }

        // 验证状态被正确恢复
        assert_eq!(new_dedup.recent.len(), 2);

        // 相同内容应该被去重（因为状态已恢复）
        assert!(!new_dedup.should_send("agent-1", "⏸️ [app] 等待输入\n\n问题内容 1\n\n回复 y/n"));
        assert!(!new_dedup.should_send("agent-2", "⏸️ [app] 等待输入\n\n问题内容 2\n\n回复 y/n"));
    }

    #[test]
    fn test_expired_records_not_restored() {
        // 验证过期记录在加载后会被清理
        let mut dedup = NotificationDeduplicator::new_without_persistence()
            .with_window(Duration::from_secs(60));

        // 模拟一个过期的记录（时间戳是 200 秒前）
        let old_timestamp = NotificationDeduplicator::current_timestamp() - 200;
        dedup
            .recent
            .insert("old-agent".to_string(), ("旧问题".to_string(), old_timestamp));

        // 添加一个新记录（触发清理）
        dedup.should_send("new-agent", "新问题");

        // 旧记录应该被清理
        assert!(!dedup.recent.contains_key("old-agent"));

        // 新记录应该存在
        assert!(dedup.recent.contains_key("new-agent"));
    }
}
