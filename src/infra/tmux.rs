//! tmux 管理模块 - 封装 tmux 操作

use anyhow::{anyhow, Result};
use std::process::Command;
use tracing::{info, error, debug};

/// tmux 管理器
pub struct TmuxManager;

impl TmuxManager {
    pub fn new() -> Self {
        Self
    }

    /// 创建新的 tmux session 并运行命令
    pub fn create_session(&self, session_name: &str, working_dir: &str, command: &str) -> Result<()> {
        debug!(session = %session_name, working_dir = %working_dir, "Creating tmux session");

        let status = Command::new("tmux")
            .args([
                "new-session",
                "-d",           // detached
                "-s", session_name,
                "-c", working_dir,
                command,
            ])
            .status()?;

        if status.success() {
            info!(session = %session_name, "Tmux session created");
            Ok(())
        } else {
            error!(session = %session_name, "Failed to create tmux session");
            Err(anyhow!("Failed to create tmux session: {}", session_name))
        }
    }

    /// 检查 session 是否存在
    pub fn session_exists(&self, session_name: &str) -> bool {
        Command::new("tmux")
            .args(["has-session", "-t", session_name])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// 重命名 session
    pub fn rename_session(&self, old_name: &str, new_name: &str) -> Result<()> {
        debug!(old = %old_name, new = %new_name, "Renaming tmux session");

        let status = Command::new("tmux")
            .args(["rename-session", "-t", old_name, new_name])
            .status()?;

        if status.success() {
            info!(old = %old_name, new = %new_name, "Tmux session renamed");
            Ok(())
        } else {
            error!(old = %old_name, new = %new_name, "Failed to rename tmux session");
            Err(anyhow!("Failed to rename session {} to {}", old_name, new_name))
        }
    }

    /// 向 session 发送按键
    /// 使用 -l 标志确保文本被字面解释，避免 "Enter" 等特殊字符串被解释为按键
    pub fn send_keys(&self, session_name: &str, keys: &str) -> Result<()> {
        info!(session = %session_name, keys_len = keys.len(), "Sending keys to tmux session");

        // 使用 -l 标志发送字面文本，避免特殊字符被解释
        let status = Command::new("tmux")
            .args(["send-keys", "-t", session_name, "-l", keys])
            .status()?;

        if !status.success() {
            error!(session = %session_name, "Failed to send text to tmux");
            return Err(anyhow!("Failed to send keys to session: {}", session_name));
        }

        debug!(session = %session_name, "Text sent, now sending Enter");

        // 单独发送 Enter（不使用 -l，因为这里需要解释为按键）
        let status = Command::new("tmux")
            .args(["send-keys", "-t", session_name, "Enter"])
            .status()?;

        if status.success() {
            info!(session = %session_name, "Enter key sent successfully");
            Ok(())
        } else {
            error!(session = %session_name, "Failed to send Enter key");
            Err(anyhow!("Failed to send Enter to session: {}", session_name))
        }
    }

    /// 向 session 发送按键（不自动添加 Enter）
    /// 使用 -l 标志确保文本被字面解释
    pub fn send_keys_raw(&self, session_name: &str, keys: &str) -> Result<()> {
        let status = Command::new("tmux")
            .args(["send-keys", "-t", session_name, "-l", keys])
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("Failed to send keys to session: {}", session_name))
        }
    }

    /// 捕获 session 的终端输出
    pub fn capture_pane(&self, session_name: &str, lines: u32) -> Result<String> {
        let output = Command::new("tmux")
            .args([
                "capture-pane",
                "-t", session_name,
                "-p",           // print to stdout
                "-S", &format!("-{}", lines),  // start from N lines back
            ])
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(anyhow!("Failed to capture pane from session: {}", session_name))
        }
    }

    /// 终止 session
    pub fn kill_session(&self, session_name: &str) -> Result<()> {
        debug!(session = %session_name, "Killing tmux session");

        let status = Command::new("tmux")
            .args(["kill-session", "-t", session_name])
            .status()?;

        if status.success() {
            info!(session = %session_name, "Tmux session killed");
            Ok(())
        } else {
            error!(session = %session_name, "Failed to kill tmux session");
            Err(anyhow!("Failed to kill session: {}", session_name))
        }
    }

    /// 列出所有 tmux sessions
    pub fn list_sessions(&self) -> Result<Vec<String>> {
        let output = Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .output()?;

        if output.status.success() {
            let sessions: Vec<String> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect();
            Ok(sessions)
        } else {
            // tmux list-sessions fails if no sessions exist
            Ok(Vec::new())
        }
    }

    /// 列出所有 cam- 前缀的 session
    pub fn list_cam_sessions(&self) -> Result<Vec<String>> {
        let output = Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .output()?;

        if output.status.success() {
            let sessions: Vec<String> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|s| s.starts_with("cam-"))
                .map(|s| s.to_string())
                .collect();
            Ok(sessions)
        } else {
            // tmux list-sessions fails if no sessions exist
            Ok(Vec::new())
        }
    }
}

impl Default for TmuxManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// 全局计数器，确保测试 session 名称唯一
    static TEST_SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// 生成唯一的测试 session 名称
    fn unique_session_name(prefix: &str) -> String {
        let counter = TEST_SESSION_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("{}-{}-{}", prefix, std::process::id(), counter)
    }

    #[test]
    fn test_create_session() {
        // Given: 一个不存在的 session 名
        let manager = TmuxManager::new();
        let session_name = unique_session_name("cam-test");

        // When: 创建 session 运行 echo 命令
        let result = manager.create_session(&session_name, "/tmp", "sleep 60");

        // Then: 返回成功，session 存在
        assert!(result.is_ok());
        assert!(manager.session_exists(&session_name));

        // Cleanup
        manager.kill_session(&session_name).unwrap();
    }

    #[test]
    fn test_send_keys() {
        // Given: 一个运行中的 session
        let manager = TmuxManager::new();
        let session_name = unique_session_name("cam-test");
        manager.create_session(&session_name, "/tmp", "cat").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));

        // When: 发送输入
        let result = manager.send_keys(&session_name, "hello");

        // Then: 返回成功
        assert!(result.is_ok());

        // Cleanup
        manager.kill_session(&session_name).unwrap();
    }

    #[test]
    fn test_capture_pane() {
        // Given: 一个有输出的 session
        let manager = TmuxManager::new();
        let session_name = unique_session_name("cam-test");
        manager.create_session(&session_name, "/tmp", "echo 'test output'; sleep 60").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));

        // When: 捕获输出
        let output = manager.capture_pane(&session_name, 50).unwrap();

        // Then: 包含预期内容
        assert!(output.contains("test output"));

        // Cleanup
        manager.kill_session(&session_name).unwrap();
    }

    #[test]
    fn test_session_exists_false_for_nonexistent() {
        // Given: 一个不存在的 session 名
        let manager = TmuxManager::new();

        // When/Then: 返回 false
        assert!(!manager.session_exists("nonexistent-session-xyz"));
    }

    #[test]
    fn test_list_sessions() {
        // Given: 创建两个 session
        let manager = TmuxManager::new();
        let session1 = unique_session_name("cam-test-list");
        let session2 = unique_session_name("cam-test-list");
        manager.create_session(&session1, "/tmp", "sleep 60").unwrap();
        manager.create_session(&session2, "/tmp", "sleep 60").unwrap();

        // When: 列出 cam- 前缀的 session
        let sessions = manager.list_cam_sessions().unwrap();

        // Then: 包含这两个
        assert!(sessions.contains(&session1));
        assert!(sessions.contains(&session2));

        // Cleanup
        manager.kill_session(&session1).unwrap();
        manager.kill_session(&session2).unwrap();
    }
}
