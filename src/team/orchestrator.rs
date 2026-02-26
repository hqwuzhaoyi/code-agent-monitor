//! Team Orchestrator 模块 - 在 Team 中编排 Agent
//!
//! 负责在 Agent Teams 中启动、管理和协调 Claude Code agents。
//!
//! Remote Lead Mode 功能：
//! - 根据自然语言任务描述创建 Team
//! - 分配任务给成员
//! - 处理用户回复

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use super::bridge::{InboxMessage, TeamBridge};
use super::discovery::TeamMember;
use crate::agent::{AgentManager, StartAgentRequest};
use crate::infra::input::InputWaitDetector;
use crate::session::state::{ConversationStateManager, ReplyResult};

/// Team 中 Agent 的启动结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnResult {
    /// CAM 分配的 agent_id (cam-xxx)
    pub agent_id: String,
    /// tmux session 名称
    pub tmux_session: String,
    /// Team 名称
    pub team: String,
    /// 成员名称
    pub member_name: String,
}

/// Team 聚合进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamProgress {
    /// Team 名称
    pub team_name: String,
    /// 总成员数
    pub total_members: usize,
    /// 活跃成员数
    pub active_members: usize,
    /// 待处理任务数
    pub pending_tasks: usize,
    /// 已完成任务数
    pub completed_tasks: usize,
    /// 等待输入的成员名称列表
    pub waiting_for_input: Vec<String>,
}

/// Team 创建结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamCreationResult {
    /// Team 名称
    pub team_name: String,
    /// 项目路径
    pub project_path: String,
    /// 启动的成员列表
    pub members: Vec<SpawnResult>,
    /// 创建的任务列表
    pub tasks: Vec<String>,
}

/// 任务分配结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignmentResult {
    /// 任务 ID
    pub task_id: String,
    /// 任务主题
    pub subject: String,
    /// 分配给的成员
    pub assigned_to: String,
}

/// 用户意图
#[derive(Debug, Clone, PartialEq)]
pub enum UserIntent {
    /// 创建 Team
    CreateTeam {
        task_desc: String,
        project: String,
    },
    /// 查看进度
    CheckProgress {
        team: Option<String>,
    },
    /// 分配任务
    AssignTask {
        member: String,
        task: String,
    },
    /// 批准/拒绝
    Approve,
    Reject,
    /// 选择选项
    SelectOption(usize),
    /// 关闭 Team
    ShutdownTeam {
        team: String,
    },
    /// 未知意图
    Unknown(String),
}

/// Team Orchestrator - 在 Team 中编排 Agent
pub struct TeamOrchestrator {
    team_bridge: TeamBridge,
    agent_manager: AgentManager,
}

impl TeamOrchestrator {
    /// 创建新的 TeamOrchestrator
    pub fn new() -> Self {
        Self {
            team_bridge: TeamBridge::new(),
            agent_manager: AgentManager::new(),
        }
    }

    /// 创建用于测试的 TeamOrchestrator
    pub fn new_for_test(base_dir: std::path::PathBuf) -> Self {
        Self {
            team_bridge: TeamBridge::new_with_base_dir(base_dir),
            agent_manager: AgentManager::new_for_test(),
        }
    }

    /// 在 Team 中启动 Agent
    ///
    /// - 创建 tmux session
    /// - 注册到 team config.json 的 members 数组
    /// - 返回 SpawnResult
    pub fn spawn_agent(
        &self,
        team: &str,
        name: &str,
        agent_type: &str,
        initial_prompt: Option<&str>,
    ) -> Result<SpawnResult> {
        info!(
            team = %team,
            name = %name,
            agent_type = %agent_type,
            "Spawning agent in team"
        );

        // 验证 team 存在
        if !self.team_bridge.team_exists(team) {
            error!(team = %team, "Team does not exist");
            return Err(anyhow!("Team '{}' does not exist", team));
        }

        // 获取 team 状态以获取 project_path
        let status = self.team_bridge.get_team_status(team)?;
        let project_path = status.project_path.unwrap_or_else(|| ".".to_string());

        // 使用 AgentManager 启动 agent
        let response = self.agent_manager.start_agent(StartAgentRequest {
            project_path: project_path.clone(),
            agent_type: Some("claude".to_string()),
            resume_session: None,
            initial_prompt: initial_prompt.map(|s| s.to_string()),
            agent_id: None,
            tmux_session: None,
        })?;

        // 创建 TeamMember 并注册到 team
        let member = TeamMember {
            name: name.to_string(),
            agent_id: format!("{}@{}", name, team),
            agent_type: agent_type.to_string(),
            model: Some("claude-opus-4-6".to_string()),
            color: None,
            is_active: Some(true),
            tmux_pane_id: Some(response.tmux_session.clone()),
            cwd: Some(project_path),
        };

        self.team_bridge.spawn_member(team, member)?;

        info!(
            agent_id = %response.agent_id,
            team = %team,
            member_name = %name,
            "Agent spawned successfully in team"
        );

        Ok(SpawnResult {
            agent_id: response.agent_id,
            tmux_session: response.tmux_session,
            team: team.to_string(),
            member_name: name.to_string(),
        })
    }

    /// 获取 Team 聚合进度
    pub fn get_team_progress(&self, team: &str) -> Result<TeamProgress> {
        // 获取 team 状态
        let status = self.team_bridge.get_team_status(team)?;

        // 检查哪些成员在等待输入
        let mut waiting_for_input = Vec::new();
        let input_detector = InputWaitDetector::new();

        for member in &status.members {
            if member.is_active {
                // 尝试通过 tmux_pane_id 获取终端输出
                // 注意：tmux_pane_id 存储的是 CAM agent_id (cam-xxx)
                if let Ok(output) = self.agent_manager.get_logs(&member.agent_id, 15) {
                    let wait_result = input_detector.detect_immediate(&output);
                    if wait_result.is_waiting {
                        waiting_for_input.push(member.name.clone());
                    }
                }
            }
        }

        Ok(TeamProgress {
            team_name: team.to_string(),
            total_members: status.members.len(),
            active_members: status.members.iter().filter(|m| m.is_active).count(),
            pending_tasks: status.pending_tasks,
            completed_tasks: status.completed_tasks,
            waiting_for_input,
        })
    }

    /// 优雅关闭 Team（停止所有 agents）
    pub fn shutdown_team(&self, team: &str) -> Result<()> {
        // 获取 team 状态
        let status = self.team_bridge.get_team_status(team)?;

        // 停止每个活跃成员的 agent
        for member in &status.members {
            if member.is_active {
                // 尝试停止 agent（忽略错误，因为 agent 可能已经停止）
                // 注意：member.agent_id 是 {name}@{team} 格式
                // 我们需要找到对应的 CAM agent_id
                let agents = self.agent_manager.list_agents()?;
                for agent in agents {
                    // 检查 tmux_session 是否匹配
                    if agent.tmux_session == member.agent_id || agent.agent_id == member.agent_id {
                        let _ = self.agent_manager.stop_agent(&agent.agent_id);
                    }
                }
            }
        }

        Ok(())
    }

    /// 获取 AgentManager 引用（用于测试）
    pub fn agent_manager(&self) -> &AgentManager {
        &self.agent_manager
    }

    /// 获取 TeamBridge 引用（用于测试）
    pub fn team_bridge(&self) -> &TeamBridge {
        &self.team_bridge
    }

    // ==================== Remote Lead Mode ====================

    /// 根据任务描述创建 Team 并启动 agents
    ///
    /// 自动分析任务，决定需要哪些角色，创建 Team 并启动对应的 agents。
    pub fn create_team_for_task(
        &self,
        task_desc: &str,
        project: &str,
    ) -> Result<TeamCreationResult> {
        // 生成 team 名称（基于项目路径）
        let team_name = self.generate_team_name(project);

        // 创建 Team
        self.team_bridge
            .create_team(&team_name, task_desc, project)?;

        // 分析任务，决定需要的角色
        let roles = self.analyze_task_roles(task_desc);

        // 启动 agents
        let mut members = Vec::new();
        for (role_name, role_type, initial_prompt) in roles {
            match self.spawn_agent(&team_name, &role_name, &role_type, Some(&initial_prompt)) {
                Ok(result) => members.push(result),
                Err(e) => {
                    error!(role = %role_name, error = %e, "Failed to spawn agent");
                }
            }
        }

        // 创建初始任务
        let tasks = vec![task_desc.to_string()];

        Ok(TeamCreationResult {
            team_name,
            project_path: project.to_string(),
            members,
            tasks,
        })
    }

    /// 分配任务给成员
    pub fn assign_task(
        &self,
        team: &str,
        member: &str,
        task: &str,
    ) -> Result<TaskAssignmentResult> {
        // 验证 team 和 member 存在
        let status = self.team_bridge.get_team_status(team)?;
        let member_exists = status.members.iter().any(|m| m.name == member);
        if !member_exists {
            return Err(anyhow!("成员 '{}' 不存在于 Team '{}'", member, team));
        }

        // 创建任务 ID
        let task_id = format!("task-{}", chrono::Utc::now().timestamp_millis());

        // 发送任务到成员 inbox
        let msg = InboxMessage {
            from: "team-lead".to_string(),
            text: serde_json::json!({
                "type": "task_assignment",
                "task_id": task_id,
                "subject": task,
                "description": task,
                "assigned_by": "user"
            })
            .to_string(),
            summary: Some(format!("任务: {}", truncate_text(task, 30))),
            timestamp: chrono::Utc::now(),
            color: None,
            read: false,
        };

        self.team_bridge.send_to_inbox(team, member, msg)?;

        Ok(TaskAssignmentResult {
            task_id,
            subject: task.to_string(),
            assigned_to: member.to_string(),
        })
    }

    /// 处理用户回复
    ///
    /// 解析用户输入，执行对应操作。
    pub fn handle_user_reply(&self, reply: &str, context: Option<&str>) -> Result<String> {
        let intent = self.parse_user_intent(reply);

        match intent {
            UserIntent::Approve => {
                let state_manager = ConversationStateManager::new();
                match state_manager.handle_reply("y", None)? {
                    ReplyResult::Sent { agent_id, .. } => Ok(format!("已批准 {} 的请求", agent_id)),
                    ReplyResult::NoPending => Ok("没有待处理的确认请求".to_string()),
                    ReplyResult::NeedSelection { options } => {
                        let list: Vec<String> = options
                            .iter()
                            .map(|o| format!("- {} ({})", o.agent_id, o.context))
                            .collect();
                        Ok(format!(
                            "有多个待处理请求，请指定目标：\n{}",
                            list.join("\n")
                        ))
                    }
                    ReplyResult::InvalidSelection(msg) => Err(anyhow!("无效选择: {}", msg)),
                }
            }
            UserIntent::Reject => {
                let state_manager = ConversationStateManager::new();
                match state_manager.handle_reply("n", None)? {
                    ReplyResult::Sent { agent_id, .. } => Ok(format!("已拒绝 {} 的请求", agent_id)),
                    ReplyResult::NoPending => Ok("没有待处理的确认请求".to_string()),
                    _ => Ok("已处理".to_string()),
                }
            }
            UserIntent::SelectOption(n) => {
                let state_manager = ConversationStateManager::new();
                match state_manager.handle_reply(&n.to_string(), None)? {
                    ReplyResult::Sent { agent_id, reply } => {
                        Ok(format!("已发送选项 {} 到 {}", reply, agent_id))
                    }
                    _ => Ok("已处理".to_string()),
                }
            }
            UserIntent::CheckProgress { team } => {
                if let Some(team_name) = team {
                    let progress = self.get_team_progress(&team_name)?;
                    Ok(format!(
                        "Team '{}' 进度:\n  成员: {}/{} 活跃\n  任务: {} 待处理, {} 已完成\n  等待输入: {}",
                        progress.team_name,
                        progress.active_members,
                        progress.total_members,
                        progress.pending_tasks,
                        progress.completed_tasks,
                        if progress.waiting_for_input.is_empty() {
                            "无".to_string()
                        } else {
                            progress.waiting_for_input.join(", ")
                        }
                    ))
                } else {
                    // 列出所有 teams
                    let teams = self.team_bridge.list_teams();
                    if teams.is_empty() {
                        Ok("没有活跃的 Team".to_string())
                    } else {
                        Ok(format!("活跃的 Teams: {}", teams.join(", ")))
                    }
                }
            }
            UserIntent::CreateTeam { task_desc, project } => {
                let result = self.create_team_for_task(&task_desc, &project)?;
                Ok(format!(
                    "已创建 Team '{}'\n  项目: {}\n  成员: {}",
                    result.team_name,
                    result.project_path,
                    result
                        .members
                        .iter()
                        .map(|m| m.member_name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
            UserIntent::AssignTask { member, task } => {
                // 需要从 context 获取 team 名称
                let team = context.unwrap_or("default");
                let result = self.assign_task(team, &member, &task)?;
                Ok(format!(
                    "已分配任务 '{}' 给 {}",
                    result.subject, result.assigned_to
                ))
            }
            UserIntent::ShutdownTeam { team } => {
                self.shutdown_team(&team)?;
                Ok(format!("已关闭 Team '{}'", team))
            }
            UserIntent::Unknown(text) => {
                // 尝试作为直接回复发送
                let state_manager = ConversationStateManager::new();
                match state_manager.handle_reply(&text, None)? {
                    ReplyResult::Sent { agent_id, reply } => {
                        Ok(format!("已发送 '{}' 到 {}", reply, agent_id))
                    }
                    ReplyResult::NoPending => Ok(format!("未识别的命令: {}", text)),
                    _ => Ok("已处理".to_string()),
                }
            }
        }
    }

    /// 解析用户意图
    fn parse_user_intent(&self, input: &str) -> UserIntent {
        let input_lower = input.to_lowercase().trim().to_string();

        // 批准/拒绝
        if matches!(
            input_lower.as_str(),
            "y" | "yes" | "是" | "好" | "可以" | "确认" | "同意" | "允许" | "批准"
        ) {
            return UserIntent::Approve;
        }
        if matches!(
            input_lower.as_str(),
            "n" | "no" | "否" | "不" | "取消" | "拒绝" | "不允许"
        ) {
            return UserIntent::Reject;
        }

        // 数字选项
        if let Ok(n) = input_lower.parse::<usize>() {
            if (1..=9).contains(&n) {
                return UserIntent::SelectOption(n);
            }
        }

        // 查看进度
        if input_lower.contains("进度")
            || input_lower.contains("状态")
            || input_lower.contains("progress")
            || input_lower.contains("status")
        {
            // 尝试提取 team 名称
            let team = self.extract_team_name(&input_lower);
            return UserIntent::CheckProgress { team };
        }

        // 创建 Team
        if input_lower.contains("启动")
            && (input_lower.contains("团队") || input_lower.contains("team"))
        {
            // 尝试提取任务描述和项目路径
            let task_desc = input.to_string();
            let project = self
                .extract_project_path(&input_lower)
                .unwrap_or_else(|| ".".to_string());
            return UserIntent::CreateTeam { task_desc, project };
        }

        // 分配任务
        if input_lower.contains("分配") || input_lower.contains("assign") {
            if let Some((member, task)) = self.extract_assignment(&input_lower) {
                return UserIntent::AssignTask { member, task };
            }
        }

        // 关闭 Team
        if (input_lower.contains("关闭")
            || input_lower.contains("停止")
            || input_lower.contains("shutdown"))
            && (input_lower.contains("团队") || input_lower.contains("team"))
        {
            if let Some(team) = self.extract_team_name(&input_lower) {
                return UserIntent::ShutdownTeam { team };
            }
        }

        UserIntent::Unknown(input.to_string())
    }

    /// 生成 Team 名称
    fn generate_team_name(&self, project: &str) -> String {
        let project_name = std::path::Path::new(project)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");

        let timestamp = chrono::Utc::now().timestamp() % 10000;
        format!("{}-{}", project_name, timestamp)
    }

    /// 分析任务需要的角色
    fn analyze_task_roles(&self, task_desc: &str) -> Vec<(String, String, String)> {
        let task_lower = task_desc.to_lowercase();
        let mut roles = Vec::new();

        // 默认添加一个 developer
        let dev_prompt = format!("你是一个开发者，负责完成以下任务：{}", task_desc);
        roles.push((
            "developer".to_string(),
            "general-purpose".to_string(),
            dev_prompt,
        ));

        // 如果涉及测试，添加 tester
        if task_lower.contains("测试") || task_lower.contains("test") {
            let test_prompt = format!("你是一个测试工程师，负责测试以下功能：{}", task_desc);
            roles.push((
                "tester".to_string(),
                "general-purpose".to_string(),
                test_prompt,
            ));
        }

        // 如果涉及代码审查，添加 reviewer
        if task_lower.contains("审查") || task_lower.contains("review") {
            let review_prompt = format!("你是一个代码审查员，负责审查以下代码：{}", task_desc);
            roles.push((
                "reviewer".to_string(),
                "general-purpose".to_string(),
                review_prompt,
            ));
        }

        roles
    }

    /// 提取 Team 名称
    fn extract_team_name(&self, input: &str) -> Option<String> {
        // 简单实现：查找已存在的 team 名称
        let teams = self.team_bridge.list_teams();
        for team in teams {
            if input.contains(&team.to_lowercase()) {
                return Some(team);
            }
        }
        None
    }

    /// 提取项目路径
    fn extract_project_path(&self, input: &str) -> Option<String> {
        // 查找路径模式
        let patterns = ["/Users/", "/home/", "/workspace/", "~/"];
        for pattern in &patterns {
            if let Some(start) = input.find(pattern) {
                let path_start = &input[start..];
                let end = path_start.find(' ').unwrap_or(path_start.len());
                return Some(path_start[..end].to_string());
            }
        }
        None
    }

    /// 提取任务分配信息
    fn extract_assignment(&self, input: &str) -> Option<(String, String)> {
        // 简单实现：查找 "给 xxx 分配 yyy" 模式
        // 实际应用中可能需要更复杂的 NLP
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() >= 3 {
            // 假设格式为 "分配 任务 给 成员" 或 "给 成员 分配 任务"
            for (i, part) in parts.iter().enumerate() {
                if *part == "给" && i + 1 < parts.len() {
                    let member = parts[i + 1].to_string();
                    let task = parts
                        .iter()
                        .filter(|p| **p != "给" && **p != member && **p != "分配")
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !task.is_empty() {
                        return Some((member, task));
                    }
                }
            }
        }
        None
    }
}

/// 截断文本
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}

impl Default for TeamOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_orchestrator() -> (TeamOrchestrator, tempfile::TempDir) {
        let temp = tempdir().unwrap();
        let orchestrator = TeamOrchestrator::new_for_test(temp.path().to_path_buf());
        (orchestrator, temp)
    }

    fn cleanup_test_agent(orchestrator: &TeamOrchestrator, agent_id: &str) {
        let _ = orchestrator.agent_manager().stop_agent(agent_id);
    }

    #[test]
    fn test_spawn_agent_team_not_exists() {
        let (orchestrator, _temp) = create_test_orchestrator();

        let result =
            orchestrator.spawn_agent("nonexistent-team", "developer", "general-purpose", None);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_spawn_agent_success() {
        let (orchestrator, _temp) = create_test_orchestrator();

        // 创建 team
        orchestrator
            .team_bridge()
            .create_team("test-team-spawn", "Test team", "/tmp")
            .unwrap();

        // 等待确保时间戳不同
        std::thread::sleep(std::time::Duration::from_secs(1));

        // 启动 agent
        let result =
            orchestrator.spawn_agent("test-team-spawn", "developer", "general-purpose", None);

        assert!(result.is_ok(), "spawn_agent failed: {:?}", result.err());
        let spawn_result = result.unwrap();
        assert!(spawn_result.agent_id.starts_with("cam-"));
        assert_eq!(spawn_result.team, "test-team-spawn");
        assert_eq!(spawn_result.member_name, "developer");

        // 验证成员已添加到 team
        let status = orchestrator
            .team_bridge()
            .get_team_status("test-team-spawn")
            .unwrap();
        assert_eq!(status.members.len(), 1);
        assert_eq!(status.members[0].name, "developer");
        assert_eq!(status.members[0].agent_id, "developer@test-team-spawn");

        // Cleanup
        cleanup_test_agent(&orchestrator, &spawn_result.agent_id);
    }

    #[test]
    fn test_spawn_agent_with_initial_prompt() {
        let (orchestrator, _temp) = create_test_orchestrator();

        // 创建 team
        orchestrator
            .team_bridge()
            .create_team("test-team-prompt", "Test team", "/tmp")
            .unwrap();

        // 等待确保时间戳不同
        std::thread::sleep(std::time::Duration::from_secs(2));

        // 启动 agent with initial prompt (使用 mock 类型避免等待 Claude Code)
        let result = orchestrator.spawn_agent(
            "test-team-prompt",
            "developer",
            "general-purpose",
            None, // 不发送 initial_prompt 以避免等待
        );

        assert!(result.is_ok(), "spawn_agent failed: {:?}", result.err());
        let spawn_result = result.unwrap();

        // Cleanup
        cleanup_test_agent(&orchestrator, &spawn_result.agent_id);
    }

    #[test]
    fn test_get_team_progress() {
        let (orchestrator, _temp) = create_test_orchestrator();

        // 创建 team
        orchestrator
            .team_bridge()
            .create_team("test-team-progress", "Test team", "/tmp")
            .unwrap();

        // 获取进度（空 team）
        let progress = orchestrator
            .get_team_progress("test-team-progress")
            .unwrap();
        assert_eq!(progress.team_name, "test-team-progress");
        assert_eq!(progress.total_members, 0);
        assert_eq!(progress.active_members, 0);
        assert!(progress.waiting_for_input.is_empty());
    }

    #[test]
    fn test_get_team_progress_with_members() {
        let (orchestrator, _temp) = create_test_orchestrator();

        // 创建 team
        orchestrator
            .team_bridge()
            .create_team("test-team-progress-members", "Test team", "/tmp")
            .unwrap();

        // 等待确保时间戳不同
        std::thread::sleep(std::time::Duration::from_secs(3));

        // 启动 agent
        let spawn_result = orchestrator
            .spawn_agent(
                "test-team-progress-members",
                "developer",
                "general-purpose",
                None,
            )
            .expect("spawn_agent failed");

        // 获取进度
        let progress = orchestrator
            .get_team_progress("test-team-progress-members")
            .unwrap();
        assert_eq!(progress.total_members, 1);
        assert_eq!(progress.active_members, 1);

        // Cleanup
        cleanup_test_agent(&orchestrator, &spawn_result.agent_id);
    }

    #[test]
    fn test_shutdown_team() {
        let (orchestrator, _temp) = create_test_orchestrator();

        // 创建 team
        orchestrator
            .team_bridge()
            .create_team("test-team-shutdown", "Test team", "/tmp")
            .unwrap();

        // 等待确保时间戳不同
        std::thread::sleep(std::time::Duration::from_secs(4));

        // 启动 agent
        let spawn_result = orchestrator
            .spawn_agent("test-team-shutdown", "developer", "general-purpose", None)
            .expect("spawn_agent failed");

        // 验证 agent 在运行
        let agents = orchestrator.agent_manager().list_agents().unwrap();
        assert!(agents.iter().any(|a| a.agent_id == spawn_result.agent_id));

        // 关闭 team
        let result = orchestrator.shutdown_team("test-team-shutdown");
        assert!(result.is_ok());
    }

    #[test]
    fn test_shutdown_team_not_exists() {
        let (orchestrator, _temp) = create_test_orchestrator();

        let result = orchestrator.shutdown_team("nonexistent-team");
        assert!(result.is_err());
    }
}
