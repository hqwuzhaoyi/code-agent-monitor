//! 进程扫描模块 - 扫描系统中的 AI 编码代理进程

use serde::{Deserialize, Serialize};
use sysinfo::{System, Process, Pid};
use anyhow::Result;
use std::collections::HashMap;

/// 代理类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentType {
    ClaudeCode,
    OpenCode,
    Codex,
    GeminiCli,
    MistralVibe,
    Unknown,
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::ClaudeCode => write!(f, "Claude Code"),
            AgentType::OpenCode => write!(f, "OpenCode"),
            AgentType::Codex => write!(f, "Codex"),
            AgentType::GeminiCli => write!(f, "Gemini CLI"),
            AgentType::MistralVibe => write!(f, "Mistral Vibe"),
            AgentType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// 代理进程信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub pid: u32,
    pub agent_type: AgentType,
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub status: String,
    pub cpu_usage: f32,
    pub memory_mb: u64,
    pub start_time: u64,
}

/// 进程扫描器
pub struct ProcessScanner {
    system: System,
}

impl ProcessScanner {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        Self { system }
    }

    /// 刷新系统信息
    pub fn refresh(&mut self) {
        self.system.refresh_all();
    }

    /// 扫描所有 AI 代理进程
    pub fn scan_agents(&self) -> Result<Vec<AgentInfo>> {
        let mut agents = Vec::new();

        for (pid, process) in self.system.processes() {
            if let Some(agent_info) = self.parse_agent_process(pid, process) {
                agents.push(agent_info);
            }
        }

        Ok(agents)
    }

    /// 获取指定 PID 的代理信息
    pub fn get_agent_info(&self, pid: u32) -> Result<Option<AgentInfo>> {
        let pid = Pid::from_u32(pid);
        if let Some(process) = self.system.process(pid) {
            Ok(self.parse_agent_process(&pid, process))
        } else {
            Ok(None)
        }
    }

    /// 终止指定进程
    pub fn kill_agent(&self, pid: u32) -> Result<()> {
        let pid = Pid::from_u32(pid);
        if let Some(process) = self.system.process(pid) {
            process.kill();
            Ok(())
        } else {
            anyhow::bail!("进程 {} 不存在", pid)
        }
    }

    /// 解析进程信息，判断是否为 AI 代理
    fn parse_agent_process(&self, pid: &Pid, process: &Process) -> Option<AgentInfo> {
        let name = process.name().to_string_lossy().to_lowercase();
        let cmd: Vec<String> = process.cmd().iter().map(|s| s.to_string_lossy().to_string()).collect();
        let cmd_str = cmd.join(" ").to_lowercase();

        // 检测代理类型
        let agent_type = if name.contains("claude") || cmd_str.contains("claude") {
            AgentType::ClaudeCode
        } else if name.contains("opencode") || cmd_str.contains("opencode") {
            AgentType::OpenCode
        } else if name.contains("codex") || cmd_str.contains("codex") {
            AgentType::Codex
        } else if name.contains("gemini") || cmd_str.contains("gemini") {
            AgentType::GeminiCli
        } else if name.contains("mistral") || cmd_str.contains("mistral-vibe") {
            AgentType::MistralVibe
        } else {
            return None; // 不是 AI 代理进程
        };

        // 提取会话 ID
        let session_id = self.extract_session_id(&cmd);
        
        // 提取模型
        let model = self.extract_model(&cmd);

        // 获取工作目录
        let working_dir = process.cwd()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Some(AgentInfo {
            pid: pid.as_u32(),
            agent_type,
            command: process.name().to_string_lossy().to_string(),
            args: cmd,
            working_dir,
            session_id,
            model,
            status: format!("{:?}", process.status()),
            cpu_usage: process.cpu_usage(),
            memory_mb: process.memory() / 1024 / 1024,
            start_time: process.start_time(),
        })
    }

    /// 从命令行参数中提取会话 ID
    fn extract_session_id(&self, args: &[String]) -> Option<String> {
        for (i, arg) in args.iter().enumerate() {
            if arg == "--resume" || arg == "--session-id" || arg == "-r" {
                return args.get(i + 1).cloned();
            }
        }
        None
    }

    /// 从命令行参数中提取模型
    fn extract_model(&self, args: &[String]) -> Option<String> {
        for (i, arg) in args.iter().enumerate() {
            if arg == "--model" || arg == "-m" {
                return args.get(i + 1).cloned();
            }
        }
        None
    }
}

impl Default for ProcessScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_agents() {
        let scanner = ProcessScanner::new();
        let agents = scanner.scan_agents().unwrap();
        // 测试不会崩溃
        println!("Found {} agents", agents.len());
    }
}
