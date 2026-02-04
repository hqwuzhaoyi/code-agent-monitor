//! Watcher Daemon 模块 - 管理后台监控进程

use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Watcher Daemon 管理器
pub struct WatcherDaemon {
    /// 数据目录
    data_dir: PathBuf,
}

impl WatcherDaemon {
    /// 创建新的 daemon 管理器
    pub fn new() -> Self {
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude-monitor");

        let _ = fs::create_dir_all(&data_dir);

        Self { data_dir }
    }

    /// 创建用于测试的 daemon 管理器
    pub fn new_for_test() -> Self {
        let data_dir = std::env::temp_dir().join(format!("cam-daemon-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&data_dir);
        Self { data_dir }
    }

    /// 获取 PID 文件路径
    pub fn pid_file_path(&self) -> PathBuf {
        self.data_dir.join("watcher.pid")
    }

    /// 检查 watcher 是否在运行
    pub fn is_running(&self) -> bool {
        let pid_file = self.pid_file_path();
        if !pid_file.exists() {
            return false;
        }

        // 读取 PID 并检查进程是否存在
        if let Ok(content) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                return Self::process_exists(pid);
            }
        }

        false
    }

    /// 检查进程是否存在
    fn process_exists(pid: u32) -> bool {
        // 使用 kill -0 检查进程是否存在
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// 写入 PID 文件
    pub fn write_pid(&self, pid: u32) -> Result<()> {
        fs::write(self.pid_file_path(), pid.to_string())?;
        Ok(())
    }

    /// 读取 PID
    pub fn read_pid(&self) -> Result<Option<u32>> {
        let pid_file = self.pid_file_path();
        if !pid_file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&pid_file)?;
        Ok(content.trim().parse().ok())
    }

    /// 删除 PID 文件
    pub fn remove_pid(&self) -> Result<()> {
        let pid_file = self.pid_file_path();
        if pid_file.exists() {
            fs::remove_file(pid_file)?;
        }
        Ok(())
    }

    /// 启动 watcher（如果未运行）
    pub fn ensure_started(&self) -> Result<bool> {
        if self.is_running() {
            return Ok(false); // 已经在运行
        }

        // 查找 cam 可执行文件
        let cam_path = std::env::current_exe()?;

        // Fork 后台进程运行 cam watch-daemon
        let child = Command::new(&cam_path)
            .args(["watch-daemon"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        // 写入 PID
        self.write_pid(child.id())?;

        Ok(true) // 新启动
    }

    /// 停止 watcher
    pub fn stop(&self) -> Result<bool> {
        if let Some(pid) = self.read_pid()? {
            // 发送 SIGTERM
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .output();

            self.remove_pid()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl Default for WatcherDaemon {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_file_path() {
        let daemon = WatcherDaemon::new();
        let path = daemon.pid_file_path();
        assert!(path.to_string_lossy().contains(".claude-monitor"));
        assert!(path.to_string_lossy().ends_with("watcher.pid"));
    }

    #[test]
    fn test_is_running_when_no_pid_file() {
        let daemon = WatcherDaemon::new_for_test();
        assert!(!daemon.is_running());
    }

    #[test]
    fn test_write_and_read_pid() {
        let daemon = WatcherDaemon::new_for_test();
        let test_pid = std::process::id();

        daemon.write_pid(test_pid).unwrap();
        assert!(daemon.is_running());

        daemon.remove_pid().unwrap();
        assert!(!daemon.is_running());
    }
}
