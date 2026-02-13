# 统一通知去重机制实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 Watcher 的完整去重逻辑（30分钟锁定 + 提醒 + 2小时上限）迁移到 Deduplicator，实现跨进程持久化和文件锁保护。

**Architecture:** 重写 `notification/deduplicator.rs`，采用 Watcher 的时间窗口锁定方案，添加 fs2 文件锁实现并发安全。Watcher 改为调用统一的 Deduplicator。

**Tech Stack:** Rust, fs2 (文件锁), serde_json (持久化)

---

## Task 1: 添加 fs2 依赖

**Files:**
- Modify: `Cargo.toml`

**Step 1: 添加 fs2 依赖**

在 `[dependencies]` 部分添加：

```toml
fs2 = "0.4"
```

**Step 2: 验证依赖可用**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo check`
Expected: 编译成功，无错误

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add fs2 dependency for file locking"
```

---

## Task 2: 定义新的数据结构和常量

**Files:**
- Modify: `src/notification/deduplicator.rs:1-50`

**Step 1: 写测试 - NotificationLock 序列化**

在 `deduplicator.rs` 的 `#[cfg(test)] mod tests` 中添加：

```rust
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
```

**Step 2: 运行测试验证失败**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test test_notification_lock_serialization -- --nocapture`
Expected: FAIL - `NotificationLock` 未定义

**Step 3: 实现数据结构**

替换 `deduplicator.rs` 开头的结构体定义（第 23-36 行）：

```rust
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

/// 常量
impl NotificationDeduplicator {
    /// 锁定时长：30 分钟
    const LOCK_DURATION_SECS: u64 = 1800;
    /// 提醒延迟：锁定结束后 30 分钟
    const REMINDER_DELAY_SECS: u64 = 1800;
    /// 最大通知时限：2 小时后停止发送
    const MAX_NOTIFICATION_DURATION_SECS: u64 = 7200;
}
```

**Step 4: 运行测试验证通过**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test test_notification_lock_serialization -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src/notification/deduplicator.rs
git commit -m "feat(dedup): add NotificationLock struct and NotifyAction enum"
```

---

## Task 3: 实现内容指纹计算

**Files:**
- Modify: `src/notification/deduplicator.rs`

**Step 1: 写测试 - 内容指纹**

```rust
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
fn test_content_fingerprint_ignores_animation() {
    let content1 = "Question?\nFlowing...\nMore text";
    let content2 = "Question?\nBrewing...\nMore text";
    let fp1 = NotificationDeduplicator::content_fingerprint(content1);
    let fp2 = NotificationDeduplicator::content_fingerprint(content2);
    assert_eq!(fp1, fp2);
}
```

**Step 2: 运行测试验证失败**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test test_content_fingerprint -- --nocapture`
Expected: FAIL - 方法未定义

**Step 3: 实现内容指纹**

```rust
impl NotificationDeduplicator {
    /// 计算内容指纹
    fn content_fingerprint(content: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let normalized = Self::normalize_content(content);
        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        hasher.finish()
    }

    /// 规范化内容（移除噪声）
    fn normalize_content(content: &str) -> String {
        content
            .lines()
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
}
```

**Step 4: 运行测试验证通过**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test test_content_fingerprint -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src/notification/deduplicator.rs
git commit -m "feat(dedup): add content fingerprint calculation"
```

---

## Task 4: 实现文件锁保护的持久化

**Files:**
- Modify: `src/notification/deduplicator.rs`

**Step 1: 写测试 - 文件锁**

```rust
#[test]
fn test_file_lock_load_save_roundtrip() {
    use std::fs;
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
    fs::write(&path, &content).unwrap();

    // 加载
    let loaded_content = fs::read_to_string(&path).unwrap();
    let loaded_state: DedupState = serde_json::from_str(&loaded_content).unwrap();

    assert_eq!(loaded_state.locks.len(), 1);
    assert!(loaded_state.locks.contains_key("agent-1"));
}
```

**Step 2: 运行测试验证通过**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test test_file_lock_load_save_roundtrip -- --nocapture`
Expected: PASS（基础序列化已工作）

**Step 3: 添加 fs2 导入和文件锁方法**

在文件顶部添加：

```rust
use fs2::FileExt;
use std::io::{Read, Write};
```

实现带文件锁的加载和保存：

```rust
impl NotificationDeduplicator {
    /// 从磁盘加载状态（带共享锁）
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

        let Some(path) = Self::state_file_path() else {
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
}
```

**Step 4: 运行测试验证通过**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test dedup -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src/notification/deduplicator.rs
git commit -m "feat(dedup): add file locking for concurrent safety"
```

---

## Task 5: 实现核心去重逻辑

**Files:**
- Modify: `src/notification/deduplicator.rs`

**Step 1: 写测试 - 首次通知**

```rust
#[test]
fn test_should_send_first_notification() {
    let mut dedup = NotificationDeduplicator::new_without_persistence();
    let action = dedup.should_send("agent-1", "Question?");
    assert_eq!(action, NotifyAction::Send);
}
```

**Step 2: 写测试 - 锁定期内相同内容**

```rust
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
```

**Step 3: 写测试 - 锁定期内内容变化**

```rust
#[test]
fn test_should_send_different_content_resets_lock() {
    let mut dedup = NotificationDeduplicator::new_without_persistence();

    let action1 = dedup.should_send("agent-1", "Question A?");
    assert_eq!(action1, NotifyAction::Send);

    // 内容变化，应该发送
    let action2 = dedup.should_send("agent-1", "Question B?");
    assert_eq!(action2, NotifyAction::Send);
}
```

**Step 4: 运行测试验证失败**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test test_should_send -- --nocapture`
Expected: FAIL - 方法签名不匹配

**Step 5: 实现核心逻辑**

更新 `NotificationDeduplicator` 结构体和 `should_send` 方法：

```rust
/// 通知去重器
pub struct NotificationDeduplicator {
    /// agent_id -> NotificationLock
    locks: HashMap<String, NotificationLock>,
    /// 是否启用持久化
    persist: bool,
}

impl NotificationDeduplicator {
    pub fn new() -> Self {
        let mut dedup = Self {
            locks: HashMap::new(),
            persist: true,
        };
        dedup.load_state();
        dedup
    }

    #[cfg(test)]
    pub fn new_without_persistence() -> Self {
        Self {
            locks: HashMap::new(),
            persist: false,
        }
    }

    /// 获取当前 Unix 时间戳（秒）
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// 清理过期记录（超过 2 小时的）
    fn cleanup_expired(&mut self, now: u64) {
        self.locks.retain(|_, lock| {
            now.saturating_sub(lock.first_notified_at) < Self::MAX_NOTIFICATION_DURATION_SECS
        });
    }

    /// 检查是否应该发送通知
    pub fn should_send(&mut self, agent_id: &str, content: &str) -> NotifyAction {
        let now = Self::current_timestamp();
        let fingerprint = Self::content_fingerprint(content);

        // 清理过期记录
        self.cleanup_expired(now);

        if let Some(lock) = self.locks.get_mut(agent_id) {
            let elapsed = now.saturating_sub(lock.locked_at);
            let total_elapsed = now.saturating_sub(lock.first_notified_at);

            // 1. 超过 2 小时，停止发送
            if total_elapsed >= Self::MAX_NOTIFICATION_DURATION_SECS {
                return NotifyAction::Suppressed("max duration exceeded".into());
            }

            // 2. 锁定期内
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

            // 3. 提醒时机（锁定结束后 30 分钟）
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

            // 4. 锁定期结束但未到提醒时机
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
```

**Step 6: 运行测试验证通过**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test test_should_send -- --nocapture`
Expected: PASS

**Step 7: Commit**

```bash
git add src/notification/deduplicator.rs
git commit -m "feat(dedup): implement unified dedup logic with 30min lock + reminder"
```

---

## Task 6: 添加提醒和超时测试

**Files:**
- Modify: `src/notification/deduplicator.rs`

**Step 1: 写测试 - 提醒发送**

```rust
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
```

**Step 2: 写测试 - 2小时超时**

```rust
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
```

**Step 3: 运行测试验证通过**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test test_should_send_reminder -- --nocapture && cargo test test_should_send_suppressed_after_max -- --nocapture`
Expected: PASS

**Step 4: Commit**

```bash
git add src/notification/deduplicator.rs
git commit -m "test(dedup): add reminder and max duration tests"
```

---

## Task 7: 更新 Watcher 使用统一 Deduplicator

**Files:**
- Modify: `src/agent_mod/watcher.rs`

**Step 1: 移除 Watcher 内部的去重逻辑**

删除 `watcher.rs` 中的以下内容：
- `NotifyAction` 枚举（第 19-28 行）
- `NotificationLock` 结构体（第 85-96 行）
- `notification_locks` 字段（第 185 行）
- `should_send_notification` 方法（第 283-367 行）
- `clear_notification_lock` 方法（第 369-372 行）
- 相关常量（第 197-202 行）

**Step 2: 添加 Deduplicator 导入和字段**

在 `watcher.rs` 顶部添加：

```rust
use crate::notification::deduplicator::{NotificationDeduplicator, NotifyAction};
```

在 `AgentWatcher` 结构体中添加：

```rust
/// 通知去重器
deduplicator: NotificationDeduplicator,
```

**Step 3: 更新构造函数**

在 `new()` 和 `new_for_test()` 中初始化：

```rust
deduplicator: NotificationDeduplicator::new(),
// 或测试版本
deduplicator: NotificationDeduplicator::new_without_persistence(),
```

**Step 4: 更新 poll_once 中的调用**

将 `self.should_send_notification(&agent_id, &wait_result.context)` 改为：

```rust
let action = self.deduplicator.should_send(&agent_id, &wait_result.context);
```

将 `self.clear_notification_lock(&agent_id)` 改为：

```rust
self.deduplicator.clear_lock(&agent_id);
```

**Step 5: 更新 cleanup_agent**

将 `self.notification_locks.remove(agent_id)` 改为：

```rust
self.deduplicator.clear_lock(agent_id);
```

**Step 6: 运行测试验证**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test watcher -- --nocapture`
Expected: PASS

**Step 7: Commit**

```bash
git add src/agent_mod/watcher.rs src/notification/deduplicator.rs
git commit -m "refactor(watcher): use unified NotificationDeduplicator"
```

---

## Task 8: 更新 notification 模块导出

**Files:**
- Modify: `src/notification/mod.rs`

**Step 1: 确保 NotifyAction 被导出**

检查 `src/notification/mod.rs`，确保包含：

```rust
pub use deduplicator::{NotificationDeduplicator, NotifyAction};
```

**Step 2: 运行完整测试**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add src/notification/mod.rs
git commit -m "feat(notification): export NotifyAction from deduplicator"
```

---

## Task 9: 清理旧代码和测试

**Files:**
- Modify: `src/notification/deduplicator.rs`

**Step 1: 删除旧的测试和结构**

删除以下不再需要的内容：
- `DedupRecord` 结构体
- 旧的 `DedupState` 结构体（使用新的 `locks` 字段版本）
- `window` 和 `similarity_threshold` 字段
- `with_window` 和 `with_similarity_threshold` 方法
- `extract_core_question` 和 `remove_reply_hints` 方法
- `calculate_similarity` 和 `get_ngrams` 方法
- 所有基于相似度的旧测试

**Step 2: 运行测试验证**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test dedup -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add src/notification/deduplicator.rs
git commit -m "refactor(dedup): remove legacy similarity-based dedup code"
```

---

## Task 10: 端到端验证

**Files:**
- None (验证步骤)

**Step 1: 构建 release 版本**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo build --release`
Expected: 编译成功

**Step 2: 运行所有测试**

Run: `cd /Users/admin/workspace/code-agent-monitor && cargo test`
Expected: 所有测试通过

**Step 3: 手动测试 Hook 模式**

Run: `echo '{"cwd": "/tmp"}' | ./target/release/cam notify --event stop --agent-id test-agent --dry-run`
Expected: 正常输出，无错误

**Step 4: 检查状态文件格式**

Run: `cat ~/.config/code-agent-monitor/dedup_state.json`
Expected: JSON 格式包含 `locks` 字段

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: unified notification deduplication with 30min lock + reminder + 2hr max"
```

---

## 验收标准

1. ✅ 首次通知 → Send
2. ✅ 锁定期内相同内容 → Suppressed
3. ✅ 锁定期内内容变化 → Send + 重置锁定
4. ✅ 锁定期结束后相同内容 → Suppressed（等待提醒）
5. ✅ 提醒时机到达 → SendReminder
6. ✅ 提醒后相同内容 → Suppressed
7. ✅ 2小时后 → Suppressed（永久）
8. ✅ 并发写入安全（文件锁）
9. ✅ agent 恢复后清除锁定
