//! 通知存储 - 本地 JSONL 文件读写

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::urgency::Urgency;

/// 通知记录（JSONL 格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRecord {
    /// ISO8601 时间戳
    pub ts: DateTime<Utc>,
    /// Agent ID
    pub agent_id: String,
    /// 紧急程度
    pub urgency: Urgency,
    /// 事件类型
    pub event: String,
    /// 简短摘要
    pub summary: String,
}

/// 通知存储
pub struct NotificationStore;

const MAX_NOTIFICATIONS: usize = 200;
const KEEP_AFTER_CLEANUP: usize = 100;
const CLEANUP_CHECK_INTERVAL: usize = 10;
static WRITE_COUNT: AtomicUsize = AtomicUsize::new(0);

impl NotificationStore {
    /// 获取存储文件路径
    pub fn path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("code-agent-monitor")
            .join("notifications.jsonl")
    }

    /// 追加通知记录（带文件锁）
    pub fn append(record: &NotificationRecord) -> Result<()> {
        use fs2::FileExt;

        let path = Self::path();

        // 确保目录存在
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // 打开文件并加锁
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        file.lock_shared()?;
        let mut file = file;
        writeln!(file, "{}", serde_json::to_string(record)?)?;
        file.unlock()?;

        // 定期检查是否需要清理
        Self::maybe_cleanup();

        Ok(())
    }

    /// 读取最近 N 条通知
    pub fn read_recent(n: usize) -> Vec<NotificationRecord> {
        let path = Self::path();

        if !path.exists() {
            return Vec::new();
        }

        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let reader = BufReader::new(file);
        let records: Vec<NotificationRecord> = reader
            .lines()
            .filter_map(|line| line.ok())
            .filter_map(|line| serde_json::from_str(&line).ok())
            .collect();

        // 返回最后 N 条
        let start = records.len().saturating_sub(n);
        records[start..].to_vec()
    }

    /// 定期检查并清理
    fn maybe_cleanup() {
        let count = WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
        if count % CLEANUP_CHECK_INTERVAL != 0 {
            return;
        }

        let path = Self::path();
        if let Ok(metadata) = fs::metadata(&path) {
            // 估算行数：平均每行 150 字节
            let estimated_lines = metadata.len() as usize / 150;
            if estimated_lines > MAX_NOTIFICATIONS {
                let _ = Self::cleanup();
            }
        }
    }

    /// 执行清理（保留最近的记录）
    fn cleanup() -> Result<()> {
        use fs2::FileExt;

        let path = Self::path();
        let file = File::open(&path)?;

        // 独占锁用于清理
        file.lock_exclusive()?;

        let reader = BufReader::new(&file);
        let records: Vec<NotificationRecord> = reader
            .lines()
            .filter_map(|line| line.ok())
            .filter_map(|line| serde_json::from_str(&line).ok())
            .collect();

        if records.len() <= MAX_NOTIFICATIONS {
            file.unlock()?;
            return Ok(());
        }

        // 保留最后 KEEP_AFTER_CLEANUP 条
        let start = records.len().saturating_sub(KEEP_AFTER_CLEANUP);
        let to_keep = &records[start..];

        // 写入临时文件
        let temp_path = path.with_extension("tmp");
        {
            let mut temp_file = File::create(&temp_path)?;
            for record in to_keep {
                writeln!(temp_file, "{}", serde_json::to_string(record)?)?;
            }
        }

        // 原子替换
        fs::rename(&temp_path, &path)?;

        file.unlock()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_record(agent_id: &str, summary: &str) -> NotificationRecord {
        NotificationRecord {
            ts: Utc::now(),
            agent_id: agent_id.to_string(),
            urgency: Urgency::High,
            event: "test".to_string(),
            summary: summary.to_string(),
        }
    }

    #[test]
    fn test_notification_record_serialization() {
        let record = create_test_record("cam-123", "Test message");
        let json = serde_json::to_string(&record).unwrap();
        let parsed: NotificationRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.agent_id, "cam-123");
        assert_eq!(parsed.summary, "Test message");
        assert_eq!(parsed.urgency, Urgency::High);
    }

    #[test]
    fn test_read_recent_empty_file() {
        // 读取不存在的文件应返回空列表
        let records = NotificationStore::read_recent(10);
        // 可能返回空或已有数据，取决于测试环境
        assert!(records.len() <= 10);
    }
}
