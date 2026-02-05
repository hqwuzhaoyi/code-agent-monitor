//! Agent 管理模块 - Agent 生命周期管理

use crate::tmux::TmuxManager;
use crate::watcher_daemon::WatcherDaemon;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Agent 类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Claude,
    OpenCode,
    Codex,
    Mock,  // 用于测试
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Claude => write!(f, "claude"),
            AgentType::OpenCode => write!(f, "opencode"),
            AgentType::Codex => write!(f, "codex"),
            AgentType::Mock => write!(f, "mock"),
        }
    }
}

impl std::str::FromStr for AgentType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "claude" => Ok(AgentType::Claude),
            "opencode" => Ok(AgentType::OpenCode),
            "codex" => Ok(AgentType::Codex),
            "mock" => Ok(AgentType::Mock),
            _ => Err(anyhow!("Unknown agent type: {}", s)),
        }
    }
}

/// Agent 状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Running,
    Waiting,
    Stopped,
}

/// Agent 记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub agent_id: String,
    pub agent_type: AgentType,
    pub project_path: String,
    pub tmux_session: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonl_path: Option<String>,
    #[serde(default)]
    pub jsonl_offset: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_output_hash: Option<String>,
    pub started_at: String,
    pub status: AgentStatus,
}

/// 启动 Agent 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartAgentRequest {
    pub project_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_prompt: Option<String>,
}

/// 启动 Agent 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartAgentResponse {
    pub agent_id: String,
    pub tmux_session: String,
}

/// agents.json 结构
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AgentsFile {
    agents: Vec<AgentRecord>,
}

/// Agent 管理器
pub struct AgentManager {
    pub tmux: TmuxManager,
    data_dir: PathBuf,
}

impl AgentManager {
    pub fn new() -> Self {
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude-monitor");

        // 确保目录存在
        let _ = fs::create_dir_all(&data_dir);

        Self {
            tmux: TmuxManager::new(),
            data_dir,
        }
    }

    /// 创建用于测试的 AgentManager
    pub fn new_for_test() -> Self {
        let data_dir = std::env::temp_dir().join(format!("cam-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&data_dir);

        Self {
            tmux: TmuxManager::new(),
            data_dir,
        }
    }

    /// 获取 agents.json 路径
    fn agents_file_path(&self) -> PathBuf {
        self.data_dir.join("agents.json")
    }

    /// 读取 agents.json
    fn read_agents_file(&self) -> Result<AgentsFile> {
        let path = self.agents_file_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(AgentsFile::default())
        }
    }

    /// 写入 agents.json
    fn write_agents_file(&self, file: &AgentsFile) -> Result<()> {
        let path = self.agents_file_path();
        let content = serde_json::to_string_pretty(file)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// 生成 agent_id
    fn generate_agent_id(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("cam-{}", timestamp)
    }

    /// 获取 agent 启动命令
    fn get_agent_command(&self, agent_type: &AgentType, resume_session: Option<&str>) -> String {
        match agent_type {
            AgentType::Claude => {
                if let Some(session_id) = resume_session {
                    format!("claude --resume {}", session_id)
                } else {
                    "claude".to_string()
                }
            }
            AgentType::OpenCode => "opencode".to_string(),
            AgentType::Codex => "codex".to_string(),
            AgentType::Mock => "sleep 3600".to_string(),  // 测试用
        }
    }

    /// 启动 Agent
    pub fn start_agent(&self, request: StartAgentRequest) -> Result<StartAgentResponse> {
        let agent_type: AgentType = request.agent_type
            .as_deref()
            .unwrap_or("claude")
            .parse()?;

        let agent_id = self.generate_agent_id();
        let tmux_session = agent_id.clone();

        let command = self.get_agent_command(&agent_type, request.resume_session.as_deref());

        // 创建 tmux session
        self.tmux.create_session(&tmux_session, &request.project_path, &command)?;

        // 如果有初始 prompt，等待 Claude Code 就绪后发送
        if let Some(prompt) = &request.initial_prompt {
            // 循环检测 Claude Code 是否显示 > 提示符
            let max_attempts = 30; // 最多等待 30 秒
            let mut ready = false;
            for _ in 0..max_attempts {
                std::thread::sleep(std::time::Duration::from_secs(1));
                if let Ok(output) = self.tmux.capture_pane(&tmux_session, 20) {
                    // 检测 Claude Code 就绪的标志：使用正则匹配行首 > 提示符
                    let claude_prompt_re = regex::Regex::new(r"(?m)^>\s*$").unwrap();
                    if claude_prompt_re.is_match(&output) || output.contains("Welcome to") || output.contains("Claude Code") {
                        ready = true;
                        // 额外等待 1 秒确保完全就绪
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        break;
                    }
                }
            }
            if ready {
                self.tmux.send_keys(&tmux_session, prompt)?;
            } else {
                eprintln!("警告: Claude Code 未能在 30 秒内就绪，初始 prompt 未发送");
            }
        }

        // 保存到 agents.json
        let record = AgentRecord {
            agent_id: agent_id.clone(),
            agent_type,
            project_path: request.project_path,
            tmux_session: tmux_session.clone(),
            session_id: request.resume_session,
            jsonl_path: None,
            jsonl_offset: 0,
            last_output_hash: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            status: AgentStatus::Running,
        };

        let mut file = self.read_agents_file()?;
        file.agents.push(record);
        self.write_agents_file(&file)?;

        // 确保 watcher daemon 在运行
        let daemon = WatcherDaemon::new();
        if let Ok(started) = daemon.ensure_started() {
            if started {
                eprintln!("已启动 watcher daemon");
            }
        }

        Ok(StartAgentResponse {
            agent_id,
            tmux_session,
        })
    }

    /// 使用自定义命令启动 agent（用于测试）
    pub fn start_agent_with_command(&self, project_path: String, command: &str) -> Result<StartAgentResponse> {
        let agent_id = self.generate_agent_id();
        let tmux_session = agent_id.clone();

        // 创建 tmux session
        self.tmux.create_session(&tmux_session, &project_path, command)?;

        // 保存到 agents.json
        let record = AgentRecord {
            agent_id: agent_id.clone(),
            agent_type: AgentType::Mock,
            project_path,
            tmux_session: tmux_session.clone(),
            session_id: None,
            jsonl_path: None,
            jsonl_offset: 0,
            last_output_hash: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            status: AgentStatus::Running,
        };

        let mut file = self.read_agents_file()?;
        file.agents.push(record);
        self.write_agents_file(&file)?;

        Ok(StartAgentResponse {
            agent_id,
            tmux_session,
        })
    }

    /// 停止 Agent
    pub fn stop_agent(&self, agent_id: &str) -> Result<()> {
        let mut file = self.read_agents_file()?;

        // 找到 agent
        let agent = file.agents.iter()
            .find(|a| a.agent_id == agent_id)
            .ok_or_else(|| anyhow!("Agent not found: {}", agent_id))?
            .clone();

        // 终止 tmux session
        let _ = self.tmux.kill_session(&agent.tmux_session);

        // 从记录中删除
        file.agents.retain(|a| a.agent_id != agent_id);
        self.write_agents_file(&file)?;

        Ok(())
    }

    /// 向 Agent 发送输入
    pub fn send_input(&self, agent_id: &str, input: &str) -> Result<()> {
        let file = self.read_agents_file()?;

        let agent = file.agents.iter()
            .find(|a| a.agent_id == agent_id)
            .ok_or_else(|| anyhow!("Agent not found: {}", agent_id))?;

        self.tmux.send_keys(&agent.tmux_session, input)?;

        Ok(())
    }

    /// 获取 Agent 日志
    pub fn get_logs(&self, agent_id: &str, lines: u32) -> Result<String> {
        let file = self.read_agents_file()?;

        let agent = file.agents.iter()
            .find(|a| a.agent_id == agent_id)
            .ok_or_else(|| anyhow!("Agent not found: {}", agent_id))?;

        self.tmux.capture_pane(&agent.tmux_session, lines)
    }

    /// 列出所有 Agent（过滤已死亡的）
    pub fn list_agents(&self) -> Result<Vec<AgentRecord>> {
        let mut file = self.read_agents_file()?;
        let mut changed = false;

        // 过滤已死亡的 session
        let live_agents: Vec<AgentRecord> = file.agents.iter()
            .filter(|a| {
                let alive = self.tmux.session_exists(&a.tmux_session);
                if !alive {
                    changed = true;
                }
                alive
            })
            .cloned()
            .collect();

        // 如果有变化，更新文件
        if changed {
            file.agents = live_agents.clone();
            self.write_agents_file(&file)?;
        }

        Ok(live_agents)
    }

    /// 获取单个 Agent
    pub fn get_agent(&self, agent_id: &str) -> Result<Option<AgentRecord>> {
        let agents = self.list_agents()?;
        Ok(agents.into_iter().find(|a| a.agent_id == agent_id))
    }

    /// 通过 session_id 查找 Agent
    pub fn find_agent_by_session_id(&self, session_id: &str) -> Result<Option<AgentRecord>> {
        let agents = self.list_agents()?;
        Ok(agents.into_iter().find(|a| a.session_id.as_deref() == Some(session_id)))
    }

    /// 通过 cwd 更新 Agent 的 session_id
    /// 用于在 SessionStart hook 触发时建立 session_id 与 agent_id 的映射
    pub fn update_session_id_by_cwd(&self, cwd: &str, session_id: &str) -> Result<bool> {
        let mut file = self.read_agents_file()?;
        let mut updated = false;

        // 使用规范化路径进行比较（解析符号链接）
        let cwd_canonical = canonicalize_path(cwd);

        // 检查是否有多个匹配的 agent（潜在的歧义）
        let matching_agents: Vec<_> = file.agents.iter()
            .filter(|a| canonicalize_path(&a.project_path) == cwd_canonical && a.session_id.is_none())
            .collect();

        if matching_agents.len() > 1 {
            eprintln!("警告: 发现 {} 个 agent 匹配路径 {}，将使用第一个匹配", matching_agents.len(), cwd);
        }

        for agent in &mut file.agents {
            let agent_path_canonical = canonicalize_path(&agent.project_path);
            if agent_path_canonical == cwd_canonical && agent.session_id.is_none() {
                agent.session_id = Some(session_id.to_string());
                updated = true;
                break;
            }
        }

        if updated {
            self.write_agents_file(&file)?;
        }

        Ok(updated)
    }

    /// 通过 cwd 查找 Agent
    pub fn find_agent_by_cwd(&self, cwd: &str) -> Result<Option<AgentRecord>> {
        let agents = self.list_agents()?;
        let cwd_canonical = canonicalize_path(cwd);
        Ok(agents.into_iter().find(|a| canonicalize_path(&a.project_path) == cwd_canonical))
    }
}

/// 规范化路径，解析符号链接
/// 如果解析失败（路径不存在等），进行基本的路径规范化
fn canonicalize_path(path: &str) -> String {
    fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| {
            // 基本路径规范化：处理 . 和 .. 以及多余的斜杠
            let path = path.trim_end_matches('/');
            let mut components: Vec<&str> = Vec::new();
            for component in path.split('/') {
                match component {
                    "" | "." => continue,
                    ".." => { components.pop(); }
                    c => components.push(c),
                }
            }
            if path.starts_with('/') {
                format!("/{}", components.join("/"))
            } else {
                components.join("/")
            }
        })
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup_test_agents(manager: &AgentManager) {
        if let Ok(agents) = manager.list_agents() {
            for agent in agents {
                let _ = manager.stop_agent(&agent.agent_id);
            }
        }
    }

    #[test]
    fn test_start_agent_creates_tmux_session() {
        // Given: AgentManager
        let manager = AgentManager::new_for_test();
        cleanup_test_agents(&manager);

        // When: 启动一个 mock agent
        let result = manager.start_agent(StartAgentRequest {
            project_path: "/tmp".to_string(),
            agent_type: Some("mock".to_string()),
            resume_session: None,
            initial_prompt: None,
        });

        // Then: 返回 agent_id，tmux session 存在
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.agent_id.starts_with("cam-"));
        assert!(manager.tmux.session_exists(&response.tmux_session));

        // Cleanup
        manager.stop_agent(&response.agent_id).unwrap();
    }

    #[test]
    fn test_start_agent_persists_to_agents_json() {
        // Given: AgentManager
        let manager = AgentManager::new_for_test();
        cleanup_test_agents(&manager);

        // When: 启动 agent
        let response = manager.start_agent(StartAgentRequest {
            project_path: "/tmp".to_string(),
            agent_type: Some("mock".to_string()),
            resume_session: None,
            initial_prompt: None,
        }).unwrap();

        // Then: agents.json 包含该记录
        let agents = manager.list_agents().unwrap();
        assert!(agents.iter().any(|a| a.agent_id == response.agent_id));

        // Cleanup
        manager.stop_agent(&response.agent_id).unwrap();
    }

    #[test]
    fn test_stop_agent_kills_tmux_and_removes_record() {
        // Given: 一个运行中的 agent
        let manager = AgentManager::new_for_test();
        cleanup_test_agents(&manager);

        let response = manager.start_agent(StartAgentRequest {
            project_path: "/tmp".to_string(),
            agent_type: Some("mock".to_string()),
            resume_session: None,
            initial_prompt: None,
        }).unwrap();

        // When: 停止 agent
        let result = manager.stop_agent(&response.agent_id);

        // Then: 成功，tmux session 不存在，记录已删除
        assert!(result.is_ok());
        assert!(!manager.tmux.session_exists(&response.tmux_session));
        let agents = manager.list_agents().unwrap();
        assert!(!agents.iter().any(|a| a.agent_id == response.agent_id));
    }

    #[test]
    fn test_send_input_to_agent() {
        // Given: 一个运行 cat 的 agent
        let manager = AgentManager::new_for_test();
        cleanup_test_agents(&manager);

        let response = manager.start_agent_with_command(
            "/tmp".to_string(),
            "cat",
        ).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));

        // When: 发送输入
        let result = manager.send_input(&response.agent_id, "hello world");

        // Then: 成功
        assert!(result.is_ok());

        // Verify: 输出包含发送的内容
        std::thread::sleep(std::time::Duration::from_millis(300));
        let logs = manager.get_logs(&response.agent_id, 50).unwrap();
        assert!(logs.contains("hello world"));

        // Cleanup
        manager.stop_agent(&response.agent_id).unwrap();
    }

    #[test]
    fn test_list_agents_filters_dead_sessions() {
        // Given: 一个已手动 kill 的 tmux session
        let manager = AgentManager::new_for_test();
        cleanup_test_agents(&manager);

        let response = manager.start_agent(StartAgentRequest {
            project_path: "/tmp".to_string(),
            agent_type: Some("mock".to_string()),
            resume_session: None,
            initial_prompt: None,
        }).unwrap();

        // 手动 kill tmux (模拟意外退出)
        manager.tmux.kill_session(&response.tmux_session).unwrap();

        // When: 列出 agents
        let agents = manager.list_agents().unwrap();

        // Then: 不包含已死亡的 agent
        assert!(!agents.iter().any(|a| a.agent_id == response.agent_id));
    }
}
