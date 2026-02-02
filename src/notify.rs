//! 通知模块 - 监控代理状态并发送通知

use crate::process::{AgentInfo, ProcessScanner};
use crate::session::SessionManager;
use anyhow::Result;
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

/// 通知事件类型
#[derive(Debug, Clone)]
pub enum NotifyEvent {
    /// 代理启动
    AgentStarted(AgentInfo),
    /// 代理退出
    AgentExited { pid: u32, agent_type: String, working_dir: String },
    /// 代理状态变化
    AgentStatusChanged { pid: u32, old_status: String, new_status: String },
}

/// 通知器
pub struct Notifier {
    /// 是否使用 Clawdbot 发送通知
    use_clawdbot: bool,
    /// Clawdbot 命令路径
    clawdbot_cmd: String,
}

impl Notifier {
    pub fn new(use_clawdbot: bool) -> Self {
        // 尝试查找 clawdbot 路径
        let clawdbot_cmd = Self::find_clawdbot_path();
        Self {
            use_clawdbot,
            clawdbot_cmd,
        }
    }

    /// 查找 clawdbot 可执行文件路径
    fn find_clawdbot_path() -> String {
        let possible_paths = [
            "/Users/admin/.volta/bin/clawdbot",
            "/usr/local/bin/clawdbot",
            "/opt/homebrew/bin/clawdbot",
            "clawdbot",
        ];
        
        for path in possible_paths {
            if std::path::Path::new(path).exists() || path == "clawdbot" {
                return path.to_string();
            }
        }
        
        "clawdbot".to_string()
    }

    /// 发送通知
    pub fn notify(&self, event: &NotifyEvent) -> Result<()> {
        let message = match event {
            NotifyEvent::AgentStarted(agent) => {
                format!("🚀 代理启动: {} (PID: {}) 在 {}", 
                    agent.agent_type, agent.pid, agent.working_dir)
            }
            NotifyEvent::AgentExited { pid, agent_type, working_dir } => {
                format!("✅ 代理退出: {} (PID: {}) 在 {}", 
                    agent_type, pid, working_dir)
            }
            NotifyEvent::AgentStatusChanged { pid, old_status, new_status } => {
                format!("📊 代理状态变化: PID {} 从 {} 变为 {}", 
                    pid, old_status, new_status)
            }
        };

        self.notify_text(&message)
    }

    /// 发送自定义文本通知
    pub fn notify_text(&self, message: &str) -> Result<()> {
        if self.use_clawdbot {
            self.send_via_clawdbot(message)?;
        } else {
            println!("[通知] {}", message);
        }

        Ok(())
    }

    /// 通过 Clawdbot 发送通知
    fn send_via_clawdbot(&self, message: &str) -> Result<()> {
        // 使用 clawdbot message send 发送 Telegram 消息
        let result = Command::new(&self.clawdbot_cmd)
            .args(["message", "send", "--channel", "telegram", "--target", "1440537501", "--message", message])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("Clawdbot 通知失败: {}", stderr);
                    // 回退到控制台输出
                    println!("[通知] {}", message);
                }
            }
            Err(e) => {
                eprintln!("无法执行 Clawdbot: {}", e);
                // 回退到控制台输出
                println!("[通知] {}", message);
            }
        }

        Ok(())
    }
}

/// 监控器 - 持续监控代理进程状态
pub struct Watcher {
    /// 轮询间隔（秒）
    interval_secs: u64,
    /// 通知器
    notifier: Notifier,
    /// 上次扫描的代理状态
    last_agents: HashMap<u32, AgentInfo>,
}

impl Watcher {
    pub fn new(interval_secs: u64, use_clawdbot: bool) -> Self {
        Self {
            interval_secs,
            notifier: Notifier::new(use_clawdbot),
            last_agents: HashMap::new(),
        }
    }

    /// 开始监控
    pub async fn watch(&mut self) -> Result<()> {
        println!("🔍 开始监控代理进程 (间隔: {}秒)...", self.interval_secs);
        println!("按 Ctrl+C 停止\n");

        // 初始扫描
        let scanner = ProcessScanner::new();
        let agents = scanner.scan_agents()?;
        for agent in agents {
            self.last_agents.insert(agent.pid, agent);
        }
        println!("初始发现 {} 个代理进程\n", self.last_agents.len());

        loop {
            sleep(Duration::from_secs(self.interval_secs)).await;
            
            let scanner = ProcessScanner::new();
            let current_agents = scanner.scan_agents()?;
            let current_map: HashMap<u32, AgentInfo> = current_agents
                .into_iter()
                .map(|a| (a.pid, a))
                .collect();

            // 检测新启动的代理
            for (pid, agent) in &current_map {
                if !self.last_agents.contains_key(pid) {
                    self.notifier.notify(&NotifyEvent::AgentStarted(agent.clone()))?;
                }
            }

            // 检测退出的代理
            for (pid, agent) in &self.last_agents {
                if !current_map.contains_key(pid) {
                    self.notifier.notify(&NotifyEvent::AgentExited {
                        pid: *pid,
                        agent_type: format!("{:?}", agent.agent_type),
                        working_dir: agent.working_dir.clone(),
                    })?;

                    // 尝试获取该项目最新会话的最后一条消息
                    let manager = SessionManager::new();
                    if let Ok(Some(session)) = manager.get_latest_session_by_project(&agent.working_dir) {
                        if let Ok(messages) = manager.get_session_logs(&session.id, 1) {
                            if let Some(last) = messages.last() {
                                let preview = if last.content.len() > 500 {
                                    format!("{}...", &last.content[..500])
                                } else {
                                    last.content.clone()
                                };
                                let text = format!(
                                    "📝 最新消息 ({})\n会话: {}\n{}",
                                    last.role, session.id, preview
                                );
                                let _ = self.notifier.notify_text(&text);
                            }
                        }
                    }
                }
            }

            // 更新状态
            self.last_agents = current_map;
        }
    }
}
