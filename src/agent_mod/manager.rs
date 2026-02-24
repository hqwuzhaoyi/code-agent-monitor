//! Agent 管理模块 - Agent 生命周期管理

use crate::infra::tmux::TmuxManager;
use crate::agent::daemon::WatcherDaemon;
use crate::agent::adapter::get_adapter;
use anyhow::{anyhow, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn, debug};

/// 全局计数器，确保 agent_id 唯一性（即使在同一毫秒内）
static AGENT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Agent 类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Claude,
    OpenCode,
    Codex,
    GeminiCli,
    MistralVibe,
    Mock,     // 用于测试
    Unknown,  // 未知类型（进程扫描时使用）
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Claude => write!(f, "claude"),
            AgentType::OpenCode => write!(f, "opencode"),
            AgentType::Codex => write!(f, "codex"),
            AgentType::GeminiCli => write!(f, "gemini-cli"),
            AgentType::MistralVibe => write!(f, "mistral-vibe"),
            AgentType::Mock => write!(f, "mock"),
            AgentType::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for AgentType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "claude" | "claude-code" | "claudecode" => Ok(AgentType::Claude),
            "opencode" => Ok(AgentType::OpenCode),
            "codex" => Ok(AgentType::Codex),
            "gemini" | "gemini-cli" | "geminicli" => Ok(AgentType::GeminiCli),
            "mistral" | "mistral-vibe" | "mistralvibe" => Ok(AgentType::MistralVibe),
            "mock" => Ok(AgentType::Mock),
            "unknown" => Ok(AgentType::Unknown),
            _ => Err(anyhow!("Unknown agent type: {}", s)),
        }
    }
}

/// Agent 统一状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// 正在处理中 - agent 正在执行任务
    Processing,
    /// 等待输入 - agent 空闲，等待用户响应
    WaitingForInput,
    /// 等待输入且需要关键决策 - 需要用户做重要决定
    DecisionRequired,
    /// 未知 - 无法确定状态
    Unknown,
    /// 运行中 - 兼容旧数据，等同于 Processing
    #[serde(alias = "running")]
    Running,
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

impl AgentStatus {
    /// 是否应该发送通知
    pub fn should_notify(&self) -> bool {
        matches!(self, Self::WaitingForInput | Self::DecisionRequired | Self::Unknown)
    }

    /// 获取 TUI 显示图标
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Processing | Self::Running => "🟢",
            Self::WaitingForInput => "🟡",
            Self::DecisionRequired => "⚠️",
            Self::Unknown => "❓",
        }
    }

    /// 是否正在处理
    pub fn is_processing(&self) -> bool {
        matches!(self, Self::Processing | Self::Running)
    }

    /// 是否在等待输入
    pub fn is_waiting(&self) -> bool {
        matches!(self, Self::WaitingForInput | Self::DecisionRequired)
    }
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
    /// 可选：指定 agent_id，用于外部系统（如 OpenClaw）传入自定义 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// 可选：指定 tmux session 名称，用于外部系统传入已存在的 session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmux_session: Option<String>,
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
            .join(".config/code-agent-monitor");

        // 确保目录存在
        let _ = fs::create_dir_all(&data_dir);

        Self {
            tmux: TmuxManager::new(),
            data_dir,
        }
    }

    /// 创建用于测试的 AgentManager（每次调用创建独立的数据目录）
    pub fn new_for_test() -> Self {
        let counter = AGENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let data_dir = std::env::temp_dir().join(format!("cam-test-{}-{}", std::process::id(), counter));
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

    /// 获取锁文件路径
    fn lock_file_path(&self) -> PathBuf {
        self.data_dir.join("agents.json.lock")
    }

    /// 读取 agents.json（内部使用，不加锁）
    fn read_agents_file_internal(&self) -> Result<AgentsFile> {
        let path = self.agents_file_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(AgentsFile::default())
        }
    }

    /// 写入 agents.json（内部使用，不加锁）
    fn write_agents_file_internal(&self, file: &AgentsFile) -> Result<()> {
        let path = self.agents_file_path();
        let content = serde_json::to_string_pretty(file)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// 在文件锁保护下执行 agents.json 的读-改-写操作
    /// 使用阻塞锁，如果其他进程持有锁，会等待直到锁释放
    fn with_locked_agents_file<F, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce(&mut AgentsFile) -> Result<T>,
    {
        // 确保锁文件存在
        let lock_path = self.lock_file_path();
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lock_path)?;

        // 获取排他锁（阻塞等待）
        lock_file.lock_exclusive()?;

        // 读取、修改、写入
        let result = (|| {
            let mut file = self.read_agents_file_internal()?;
            let result = operation(&mut file)?;
            self.write_agents_file_internal(&file)?;
            Ok(result)
        })();

        // 释放锁（drop 时自动释放，但显式解锁更清晰）
        let _ = lock_file.unlock();

        result
    }

    /// 在文件锁保护下只读 agents.json
    fn with_locked_agents_file_read<F, T>(&self, operation: F) -> Result<T>
    where
        F: FnOnce(&AgentsFile) -> Result<T>,
    {
        let lock_path = self.lock_file_path();
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lock_path)?;

        // 获取共享锁（允许多个读者）
        lock_file.lock_shared()?;

        let result = (|| {
            let file = self.read_agents_file_internal()?;
            operation(&file)
        })();

        let _ = lock_file.unlock();

        result
    }

    /// 读取 agents.json（公开接口，加锁）
    fn read_agents_file(&self) -> Result<AgentsFile> {
        self.with_locked_agents_file_read(|file| Ok(file.clone()))
    }

    /// 生成 agent_id
    fn generate_agent_id(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let counter = AGENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("cam-{}-{}", timestamp, counter)
    }

    /// 获取 agent 启动命令
    #[deprecated(note = "Use get_adapter().get_command() instead")]
    fn get_agent_command(&self, agent_type: &AgentType, resume_session: Option<&str>) -> String {
        let adapter = get_adapter(agent_type);
        if let Some(session_id) = resume_session {
            adapter.get_resume_command(session_id)
        } else {
            adapter.get_command().to_string()
        }
    }

    /// 启动 Agent
    pub fn start_agent(&self, request: StartAgentRequest) -> Result<StartAgentResponse> {
        let agent_type: AgentType = request.agent_type
            .as_deref()
            .unwrap_or("claude")
            .parse()?;

        // 使用传入的 agent_id，或生成新的
        let agent_id = request.agent_id
            .clone()
            .unwrap_or_else(|| self.generate_agent_id());

        // 使用传入的 tmux_session，或使用 agent_id
        let tmux_session = request.tmux_session
            .clone()
            .unwrap_or_else(|| agent_id.clone());

        info!(
            agent_id = %agent_id,
            agent_type = %agent_type,
            project_path = %request.project_path,
            tmux_session = %tmux_session,
            "Starting agent"
        );

        // Use adapter to get command
        let adapter = get_adapter(&agent_type);
        let command = if let Some(ref session_id) = request.resume_session {
            adapter.get_resume_command(session_id)
        } else {
            adapter.get_command().to_string()
        };

        // 检查 tmux session 是否已存在
        let session_exists = self.tmux.session_exists(&tmux_session);

        if !session_exists {
            // 创建 tmux session
            self.tmux.create_session(&tmux_session, &request.project_path, &command)?;
        } else {
            info!(tmux_session = %tmux_session, "Tmux session already exists, reusing");
        }

        // 立即保存到 agents.json（先于 Claude Code hook 触发）
        // 这样 session_start hook 触发时能正确匹配到 agent
        let record = AgentRecord {
            agent_id: agent_id.clone(),
            agent_type,
            project_path: request.project_path.clone(),
            tmux_session: tmux_session.clone(),
            session_id: request.resume_session,
            jsonl_path: None,
            jsonl_offset: 0,
            last_output_hash: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            status: AgentStatus::Processing,
        };

        self.with_locked_agents_file(|file| {
            file.agents.push(record);
            Ok(())
        })?;

        // 如果有初始 prompt，等待 Claude Code 就绪后发送
        if let Some(prompt) = &request.initial_prompt {
            // 循环检测 Claude Code 是否显示提示符
            let max_attempts = 30; // 最多等待 30 秒
            let mut ready = false;
            for _ in 0..max_attempts {
                std::thread::sleep(std::time::Duration::from_secs(1));
                if let Ok(output) = self.tmux.capture_pane(&tmux_session, 30) {
                    // 检测 Claude Code 就绪的标志：
                    // - ❯ (U+276F) 是 Claude Code 的提示符
                    // - > (U+003E) 是旧版本的提示符
                    // - "Welcome to" 或 "Claude Code" 表示启动完成
                    let claude_prompt_re = regex::Regex::new(r"(?m)^[❯>]\s*$").unwrap();
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
                debug!(agent_id = %agent_id, "Initial prompt sent");
            } else {
                warn!(agent_id = %agent_id, "Claude Code not ready within 30s, initial prompt not sent");
            }
        }

        // 确保 watcher daemon 在运行
        let daemon = WatcherDaemon::new();
        if let Ok(started) = daemon.ensure_started() {
            if started {
                info!("Watcher daemon started");
            }
        }

        info!(agent_id = %agent_id, tmux_session = %tmux_session, "Agent started successfully");

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
            status: AgentStatus::Processing,
        };

        self.with_locked_agents_file(|file| {
            file.agents.push(record);
            Ok(())
        })?;

        Ok(StartAgentResponse {
            agent_id,
            tmux_session,
        })
    }

    /// 停止 Agent
    pub fn stop_agent(&self, agent_id: &str) -> Result<()> {
        info!(agent_id = %agent_id, "Stopping agent");

        // 在锁保护下查找 agent 并获取 tmux_session
        let tmux_session = self.with_locked_agents_file(|file| {
            let agent = file.agents.iter()
                .find(|a| a.agent_id == agent_id)
                .ok_or_else(|| anyhow!("Agent not found: {}", agent_id))?;
            let session = agent.tmux_session.clone();

            // 从记录中删除
            file.agents.retain(|a| a.agent_id != agent_id);
            Ok(session)
        })?;

        // 终止 tmux session（在锁外执行，避免长时间持有锁）
        let _ = self.tmux.kill_session(&tmux_session);

        info!(agent_id = %agent_id, "Agent stopped successfully");

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
        self.with_locked_agents_file(|file| {
            // 过滤已死亡的 session
            let live_agents: Vec<AgentRecord> = file.agents.iter()
                .filter(|a| self.tmux.session_exists(&a.tmux_session))
                .cloned()
                .collect();

            // 更新文件（只保留存活的）
            file.agents = live_agents.clone();
            Ok(live_agents)
        })
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
        let cwd_canonical = canonicalize_path(cwd);
        let session_id_owned = session_id.to_string();

        self.with_locked_agents_file(|file| {
            // 检查是否有多个匹配的 agent（潜在的歧义）
            let matching_count = file.agents.iter()
                .filter(|a| canonicalize_path(&a.project_path) == cwd_canonical && a.session_id.is_none())
                .count();

            if matching_count > 1 {
                eprintln!("警告: 发现 {} 个 agent 匹配路径 {}，将使用第一个匹配", matching_count, cwd);
            }

            let mut updated = false;
            for agent in &mut file.agents {
                let agent_path_canonical = canonicalize_path(&agent.project_path);
                if agent_path_canonical == cwd_canonical && agent.session_id.is_none() {
                    agent.session_id = Some(session_id_owned.clone());
                    updated = true;
                    break;
                }
            }

            Ok(updated)
        })
    }

    /// 通过 cwd 查找 Agent
    pub fn find_agent_by_cwd(&self, cwd: &str) -> Result<Option<AgentRecord>> {
        let agents = self.list_agents()?;
        let cwd_canonical = canonicalize_path(cwd);
        Ok(agents.into_iter().find(|a| canonicalize_path(&a.project_path) == cwd_canonical))
    }

    /// 注册外部（非 CAM 管理）的 Claude Code 会话
    /// 用于支持直接运行 claude 命令的场景
    pub fn register_external_session(&self, session_id: &str, cwd: &str) -> Result<String> {
        // 生成 agent_id: ext-{session_id前8位}
        let short_id = &session_id[..8.min(session_id.len())];
        let agent_id = format!("ext-{}", short_id);
        let agent_id_clone = agent_id.clone();

        let record = AgentRecord {
            agent_id: agent_id.clone(),
            agent_type: AgentType::Claude,
            project_path: cwd.to_string(),
            tmux_session: String::new(), // 无 tmux 管理
            session_id: Some(session_id.to_string()),
            jsonl_path: None,
            jsonl_offset: 0,
            last_output_hash: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            status: AgentStatus::Processing,
        };

        self.with_locked_agents_file(|file| {
            // 检查是否已存在
            if file.agents.iter().any(|a| a.agent_id == agent_id) {
                return Ok(());
            }
            file.agents.push(record);
            Ok(())
        })?;

        Ok(agent_id_clone)
    }

    /// 移除 Agent 记录（不终止 tmux session）
    /// 用于清理外部会话记录
    pub fn remove_agent(&self, agent_id: &str) -> Result<()> {
        let agent_id_owned = agent_id.to_string();
        self.with_locked_agents_file(|file| {
            file.agents.retain(|a| a.agent_id != agent_id_owned);
            Ok(())
        })
    }

    /// 更新 agent 状态
    pub fn update_agent_status(&self, agent_id: &str, status: AgentStatus) -> Result<bool> {
        self.with_locked_agents_file(|agents_file| {
            if let Some(agent) = agents_file.agents.iter_mut().find(|a| a.agent_id == agent_id) {
                if agent.status != status {
                    debug!(agent_id = %agent_id, old_status = ?agent.status, new_status = ?status, "Updating agent status");
                    agent.status = status;
                    return Ok(true);
                }
            }
            Ok(false)
        })
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
            agent_id: None,
            tmux_session: None,
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
            agent_id: None,
            tmux_session: None,
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
            agent_id: None,
            tmux_session: None,
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
            agent_id: None,
            tmux_session: None,
        }).unwrap();

        // 手动 kill tmux (模拟意外退出)
        manager.tmux.kill_session(&response.tmux_session).unwrap();

        // When: 列出 agents
        let agents = manager.list_agents().unwrap();

        // Then: 不包含已死亡的 agent
        assert!(!agents.iter().any(|a| a.agent_id == response.agent_id));
    }

    #[test]
    fn test_register_external_session() {
        // Given: AgentManager with clean state
        let manager = AgentManager::new_for_test();
        // Clean up any existing agents file
        let _ = std::fs::remove_file(manager.agents_file_path());

        // When: 注册外部会话
        let session_id = "862c4b15-f02a-45d6-b349-995d4d848765";
        let cwd = "/Users/admin/workspace/myproject";
        let result = manager.register_external_session(session_id, cwd);

        // Then: 返回 ext-{前8位}
        assert!(result.is_ok());
        let agent_id = result.unwrap();
        assert_eq!(agent_id, "ext-862c4b15");

        // 验证记录已保存（使用 read_agents_file 而非 list_agents，因为外部会话无 tmux）
        let file = manager.read_agents_file().unwrap();
        let agent = file.agents.iter().find(|a| a.agent_id == agent_id);
        assert!(agent.is_some(), "Agent should be found in agents.json");
        let agent = agent.unwrap();
        assert_eq!(agent.project_path, cwd);
        assert_eq!(agent.session_id, Some(session_id.to_string()));
        assert!(agent.tmux_session.is_empty()); // 无 tmux 管理

        // Cleanup
        let _ = manager.remove_agent(&agent_id);
    }

    #[test]
    fn test_register_external_session_idempotent() {
        // Given: AgentManager with clean state
        let manager = AgentManager::new_for_test();
        // Clean up any existing agents file
        let _ = std::fs::remove_file(manager.agents_file_path());

        let session_id = "test1234-f02a-45d6-b349-995d4d848765";
        let cwd = "/tmp/test";
        let agent_id = manager.register_external_session(session_id, cwd).unwrap();

        // When: 再次注册相同的会话
        let result = manager.register_external_session(session_id, cwd);

        // Then: 返回相同的 agent_id，不创建重复记录
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), agent_id);

        let file = manager.read_agents_file().unwrap();
        let count = file.agents.iter().filter(|a| a.agent_id == agent_id).count();
        assert_eq!(count, 1);

        // Cleanup
        let _ = manager.remove_agent(&agent_id);
    }

    #[test]
    fn test_remove_agent() {
        // Given: AgentManager with clean state
        let manager = AgentManager::new_for_test();
        // Clean up any existing agents file
        let _ = std::fs::remove_file(manager.agents_file_path());

        let session_id = "remove12-f02a-45d6-b349-995d4d848765";
        let agent_id = manager.register_external_session(session_id, "/tmp").unwrap();

        // When: 移除记录
        let result = manager.remove_agent(&agent_id);

        // Then: 成功，记录已删除
        assert!(result.is_ok());
        let file = manager.read_agents_file().unwrap();
        assert!(!file.agents.iter().any(|a| a.agent_id == agent_id));
    }
}
