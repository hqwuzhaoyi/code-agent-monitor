//! 终端实时流模块 - 使用 tmux pipe-pane

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

/// 终端流管理器
pub struct TerminalStream {
    current_pane: Option<String>,
    pipe_file: Option<PathBuf>,
}

impl TerminalStream {
    pub fn new() -> Self {
        Self {
            current_pane: None,
            pipe_file: None,
        }
    }

    /// 开始监听指定 tmux session
    pub fn start(&mut self, session: &str) -> Result<PathBuf> {
        // 先停止旧的
        self.stop();

        let pipe_path = PathBuf::from(format!("/tmp/cam-tui-{}.log", session.replace(':', "-")));

        // 清空旧文件
        let _ = std::fs::remove_file(&pipe_path);

        // 启动 pipe-pane
        let status = Command::new("tmux")
            .args([
                "pipe-pane",
                "-t",
                session,
                &format!("cat >> {}", pipe_path.display()),
            ])
            .status()?;

        if status.success() {
            self.current_pane = Some(session.to_string());
            self.pipe_file = Some(pipe_path.clone());
            Ok(pipe_path)
        } else {
            anyhow::bail!("Failed to start pipe-pane for {}", session)
        }
    }

    /// 停止当前监听
    pub fn stop(&mut self) {
        if let Some(ref session) = self.current_pane {
            // 关闭 pipe-pane
            let _ = Command::new("tmux")
                .args(["pipe-pane", "-t", session])
                .status();
        }

        // 清理文件
        if let Some(ref path) = self.pipe_file {
            let _ = std::fs::remove_file(path);
        }

        self.current_pane = None;
        self.pipe_file = None;
    }

    /// 获取当前 pipe 文件路径
    pub fn pipe_file(&self) -> Option<&PathBuf> {
        self.pipe_file.as_ref()
    }
}

impl Drop for TerminalStream {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Default for TerminalStream {
    fn default() -> Self {
        Self::new()
    }
}
