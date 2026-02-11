//! Team Bridge 模块 - 桥接 OpenClaw 命令与 Agent Teams 文件系统
//!
//! 负责 Team 创建/删除和 Inbox 读写操作。
//! 数据存储在 `~/.claude/teams/{team-name}/` 目录。

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::discovery::{TeamConfig, TeamMember};

/// Inbox 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessage {
    pub from: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub read: bool,
}

/// 特殊消息类型（通过 text 字段的 JSON 内容区分）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SpecialMessage {
    #[serde(rename = "task_assignment")]
    TaskAssignment { task_id: String, subject: String },

    #[serde(rename = "idle_notification")]
    IdleNotification { idle_reason: String },

    #[serde(rename = "shutdown_approved")]
    ShutdownApproved { request_id: String },

    #[serde(rename = "permission_request")]
    PermissionRequest {
        tool: String,
        input: serde_json::Value,
    },
}

/// Agent ID（使用 Agent Teams 的 {name}@{team} 格式）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentId {
    pub name: String,
    pub team: String,
}

impl AgentId {
    pub fn new(name: &str, team: &str) -> Self {
        Self {
            name: name.to_string(),
            team: team.to_string(),
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        let parts: Vec<&str> = id.split('@').collect();
        if parts.len() == 2 {
            Some(Self {
                name: parts[0].to_string(),
                team: parts[1].to_string(),
            })
        } else {
            None
        }
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name, self.team)
    }
}

/// Team 状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamStatus {
    pub team_name: String,
    pub description: Option<String>,
    pub project_path: Option<String>,
    pub members: Vec<TeamMemberStatus>,
    pub pending_tasks: usize,
    pub completed_tasks: usize,
    pub unread_messages: usize,
}

/// 成员状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMemberStatus {
    pub name: String,
    pub agent_id: String,
    pub is_active: bool,
    pub unread_count: usize,
}

/// Team Bridge - 桥接 OpenClaw 与 Agent Teams
pub struct TeamBridge {
    teams_dir: PathBuf,
    tasks_dir: PathBuf,
}

impl TeamBridge {
    /// 创建新的 TeamBridge（使用默认路径）
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            teams_dir: home.join(".claude").join("teams"),
            tasks_dir: home.join(".claude").join("tasks"),
        }
    }

    /// 创建 TeamBridge（使用自定义基础目录，用于测试）
    pub fn new_with_base_dir(base_dir: PathBuf) -> Self {
        Self {
            teams_dir: base_dir.join("teams"),
            tasks_dir: base_dir.join("tasks"),
        }
    }

    /// 获取 team 目录路径
    fn get_team_dir(&self, team: &str) -> PathBuf {
        self.teams_dir.join(team)
    }

    /// 获取 team config 文件路径
    fn get_config_path(&self, team: &str) -> PathBuf {
        self.get_team_dir(team).join("config.json")
    }

    /// 获取 team inboxes 目录路径
    fn get_inboxes_dir(&self, team: &str) -> PathBuf {
        self.get_team_dir(team).join("inboxes")
    }

    /// 获取成员 inbox 文件路径
    fn get_inbox_path(&self, team: &str, member: &str) -> PathBuf {
        self.get_inboxes_dir(team).join(format!("{}.json", member))
    }

    /// 获取 tasks 目录路径
    fn get_tasks_dir(&self, team: &str) -> PathBuf {
        self.tasks_dir.join(team)
    }

    /// 创建新 Team
    pub fn create_team(
        &self,
        name: &str,
        description: &str,
        project_path: &str,
    ) -> Result<TeamConfig> {
        let team_dir = self.get_team_dir(name);

        // 检查是否已存在
        if team_dir.exists() {
            return Err(anyhow!("Team '{}' already exists", name));
        }

        // 创建目录结构
        fs::create_dir_all(&team_dir)?;
        fs::create_dir_all(self.get_inboxes_dir(name))?;
        fs::create_dir_all(self.get_tasks_dir(name))?;

        // 创建 config.json
        let config = TeamConfig {
            team_name: name.to_string(),
            description: Some(description.to_string()),
            lead_agent_id: None,
            created_at: Some(Utc::now().timestamp_millis() as u64),
            members: Vec::new(),
        };

        // 写入配置文件（包含 project_path）
        let config_with_path = serde_json::json!({
            "name": name,
            "description": description,
            "projectPath": project_path,
            "createdAt": config.created_at,
            "members": []
        });

        let config_path = self.get_config_path(name);
        fs::write(&config_path, serde_json::to_string_pretty(&config_with_path)?)?;

        Ok(config)
    }

    /// 删除 Team 及其资源
    pub fn delete_team(&self, name: &str) -> Result<()> {
        let team_dir = self.get_team_dir(name);

        if !team_dir.exists() {
            return Err(anyhow!("Team '{}' does not exist", name));
        }

        // 删除 team 目录
        fs::remove_dir_all(&team_dir)?;

        // 删除 tasks 目录（如果存在）
        let tasks_dir = self.get_tasks_dir(name);
        if tasks_dir.exists() {
            fs::remove_dir_all(&tasks_dir)?;
        }

        Ok(())
    }

    /// 添加成员到 Team
    pub fn spawn_member(&self, team: &str, member: TeamMember) -> Result<()> {
        let config_path = self.get_config_path(team);

        if !config_path.exists() {
            return Err(anyhow!("Team '{}' does not exist", team));
        }

        // 读取现有配置
        let content = fs::read_to_string(&config_path)?;
        let mut config: serde_json::Value = serde_json::from_str(&content)?;

        // 获取或创建 members 数组
        let members = config
            .get_mut("members")
            .and_then(|m| m.as_array_mut())
            .ok_or_else(|| anyhow!("Invalid config: missing members array"))?;

        // 检查是否已存在同名成员
        let exists = members.iter().any(|m| {
            m.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n == member.name)
                .unwrap_or(false)
        });

        if exists {
            return Err(anyhow!(
                "Member '{}' already exists in team '{}'",
                member.name,
                team
            ));
        }

        // 添加新成员
        let member_json = serde_json::json!({
            "name": member.name,
            "agentId": member.agent_id,
            "agentType": member.agent_type,
            "model": member.model,
            "color": member.color,
            "isActive": member.is_active.unwrap_or(true),
            "tmuxPaneId": member.tmux_pane_id,
            "cwd": member.cwd
        });
        members.push(member_json);

        // 写回配置
        fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

        // 创建成员的 inbox 文件（空数组）
        let inbox_path = self.get_inbox_path(team, &member.name);
        if !inbox_path.exists() {
            fs::write(&inbox_path, "[]")?;
        }

        Ok(())
    }

    /// 发送消息到成员 inbox
    pub fn send_to_inbox(&self, team: &str, member: &str, message: InboxMessage) -> Result<()> {
        let inbox_path = self.get_inbox_path(team, member);

        // 确保 inboxes 目录存在
        let inboxes_dir = self.get_inboxes_dir(team);
        if !inboxes_dir.exists() {
            return Err(anyhow!("Team '{}' does not exist", team));
        }

        // 读取现有消息
        let mut messages: Vec<InboxMessage> = if inbox_path.exists() {
            let content = fs::read_to_string(&inbox_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        // 添加新消息
        messages.push(message);

        // 写回文件
        fs::write(&inbox_path, serde_json::to_string_pretty(&messages)?)?;

        Ok(())
    }

    /// 读取成员 inbox
    pub fn read_inbox(&self, team: &str, member: &str) -> Result<Vec<InboxMessage>> {
        let inbox_path = self.get_inbox_path(team, member);

        if !inbox_path.exists() {
            // 检查 team 是否存在
            let team_dir = self.get_team_dir(team);
            if !team_dir.exists() {
                return Err(anyhow!("Team '{}' does not exist", team));
            }
            // Team 存在但 inbox 为空
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&inbox_path)?;
        let messages: Vec<InboxMessage> = serde_json::from_str(&content)?;

        Ok(messages)
    }

    /// 标记消息为已读
    pub fn mark_as_read(&self, team: &str, member: &str) -> Result<usize> {
        let inbox_path = self.get_inbox_path(team, member);

        if !inbox_path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(&inbox_path)?;
        let mut messages: Vec<InboxMessage> = serde_json::from_str(&content)?;

        let mut marked_count = 0;
        for msg in &mut messages {
            if !msg.read {
                msg.read = true;
                marked_count += 1;
            }
        }

        if marked_count > 0 {
            fs::write(&inbox_path, serde_json::to_string_pretty(&messages)?)?;
        }

        Ok(marked_count)
    }

    /// 获取 Team 完整状态
    pub fn get_team_status(&self, team: &str) -> Result<TeamStatus> {
        let config_path = self.get_config_path(team);

        if !config_path.exists() {
            return Err(anyhow!("Team '{}' does not exist", team));
        }

        // 读取配置
        let content = fs::read_to_string(&config_path)?;
        let config: serde_json::Value = serde_json::from_str(&content)?;

        let description = config
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let project_path = config
            .get("projectPath")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 获取成员状态
        let members_json = config
            .get("members")
            .and_then(|m| m.as_array())
            .map(|arr| arr.to_vec())
            .unwrap_or_default();

        let mut members = Vec::new();
        let mut total_unread = 0;

        for m in members_json {
            let name = m
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let agent_id = m
                .get("agentId")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let is_active = m
                .get("isActive")
                .and_then(|n| n.as_bool())
                .unwrap_or(false);

            // 统计未读消息
            let unread_count = self
                .read_inbox(team, &name)
                .map(|msgs| msgs.iter().filter(|m| !m.read).count())
                .unwrap_or(0);

            total_unread += unread_count;

            members.push(TeamMemberStatus {
                name,
                agent_id,
                is_active,
                unread_count,
            });
        }

        // 统计任务
        let tasks_dir = self.get_tasks_dir(team);
        let (pending_tasks, completed_tasks) = if tasks_dir.exists() {
            let mut pending = 0;
            let mut completed = 0;

            if let Ok(entries) = fs::read_dir(&tasks_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().is_some_and(|e| e == "json") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(task) = serde_json::from_str::<serde_json::Value>(&content) {
                                let status = task
                                    .get("status")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("");
                                match status {
                                    "pending" | "in_progress" => pending += 1,
                                    "completed" => completed += 1,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }

            (pending, completed)
        } else {
            (0, 0)
        };

        Ok(TeamStatus {
            team_name: team.to_string(),
            description,
            project_path,
            members,
            pending_tasks,
            completed_tasks,
            unread_messages: total_unread,
        })
    }

    /// 检查 Team 是否存在
    pub fn team_exists(&self, team: &str) -> bool {
        self.get_team_dir(team).exists()
    }

    /// 列出所有 Teams
    pub fn list_teams(&self) -> Vec<String> {
        if !self.teams_dir.exists() {
            return Vec::new();
        }

        let mut teams = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.teams_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        teams.push(name.to_string());
                    }
                }
            }
        }

        teams.sort();
        teams
    }
}

impl Default for TeamBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_bridge() -> (TeamBridge, tempfile::TempDir) {
        let temp = tempdir().unwrap();
        let bridge = TeamBridge::new_with_base_dir(temp.path().to_path_buf());
        (bridge, temp)
    }

    #[test]
    fn test_create_team() {
        let (bridge, _temp) = create_test_bridge();

        let result = bridge.create_team("test-team", "Test description", "/path/to/project");
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.team_name, "test-team");
        assert_eq!(config.description, Some("Test description".to_string()));

        // 验证目录结构
        assert!(bridge.get_team_dir("test-team").exists());
        assert!(bridge.get_inboxes_dir("test-team").exists());
        assert!(bridge.get_config_path("test-team").exists());
    }

    #[test]
    fn test_create_team_already_exists() {
        let (bridge, _temp) = create_test_bridge();

        bridge
            .create_team("test-team", "First", "/path1")
            .unwrap();
        let result = bridge.create_team("test-team", "Second", "/path2");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_delete_team() {
        let (bridge, _temp) = create_test_bridge();

        bridge
            .create_team("test-team", "Test", "/path")
            .unwrap();
        assert!(bridge.team_exists("test-team"));

        let result = bridge.delete_team("test-team");
        assert!(result.is_ok());
        assert!(!bridge.team_exists("test-team"));
    }

    #[test]
    fn test_delete_nonexistent_team() {
        let (bridge, _temp) = create_test_bridge();

        let result = bridge.delete_team("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_spawn_member() {
        let (bridge, _temp) = create_test_bridge();

        bridge
            .create_team("test-team", "Test", "/path")
            .unwrap();

        let member = TeamMember {
            name: "developer".to_string(),
            agent_id: "developer@test-team".to_string(),
            agent_type: "general-purpose".to_string(),
            model: Some("claude-opus-4-6".to_string()),
            color: Some("blue".to_string()),
            is_active: Some(true),
            tmux_pane_id: None,
            cwd: Some("/path".to_string()),
        };

        let result = bridge.spawn_member("test-team", member);
        assert!(result.is_ok());

        // 验证成员已添加
        let status = bridge.get_team_status("test-team").unwrap();
        assert_eq!(status.members.len(), 1);
        assert_eq!(status.members[0].name, "developer");
    }

    #[test]
    fn test_spawn_member_duplicate() {
        let (bridge, _temp) = create_test_bridge();

        bridge
            .create_team("test-team", "Test", "/path")
            .unwrap();

        let member = TeamMember {
            name: "developer".to_string(),
            agent_id: "developer@test-team".to_string(),
            agent_type: "general-purpose".to_string(),
            model: None,
            color: None,
            is_active: Some(true),
            tmux_pane_id: None,
            cwd: None,
        };

        bridge.spawn_member("test-team", member.clone()).unwrap();
        let result = bridge.spawn_member("test-team", member);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_send_to_inbox() {
        let (bridge, _temp) = create_test_bridge();

        bridge
            .create_team("test-team", "Test", "/path")
            .unwrap();

        let message = InboxMessage {
            from: "team-lead".to_string(),
            text: "Hello, developer!".to_string(),
            summary: Some("Greeting".to_string()),
            timestamp: Utc::now(),
            color: Some("red".to_string()),
            read: false,
        };

        let result = bridge.send_to_inbox("test-team", "developer", message);
        assert!(result.is_ok());

        // 验证消息已写入
        let messages = bridge.read_inbox("test-team", "developer").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "team-lead");
        assert_eq!(messages[0].text, "Hello, developer!");
    }

    #[test]
    fn test_send_to_inbox_nonexistent_team() {
        let (bridge, _temp) = create_test_bridge();

        let message = InboxMessage {
            from: "sender".to_string(),
            text: "test".to_string(),
            summary: None,
            timestamp: Utc::now(),
            color: None,
            read: false,
        };

        let result = bridge.send_to_inbox("nonexistent", "member", message);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_inbox_empty() {
        let (bridge, _temp) = create_test_bridge();

        bridge
            .create_team("test-team", "Test", "/path")
            .unwrap();

        let messages = bridge.read_inbox("test-team", "developer").unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn test_read_inbox_nonexistent_team() {
        let (bridge, _temp) = create_test_bridge();

        let result = bridge.read_inbox("nonexistent", "developer");
        assert!(result.is_err());
    }

    #[test]
    fn test_mark_as_read() {
        let (bridge, _temp) = create_test_bridge();

        bridge
            .create_team("test-team", "Test", "/path")
            .unwrap();

        // 发送两条消息
        for i in 0..2 {
            let message = InboxMessage {
                from: "sender".to_string(),
                text: format!("Message {}", i),
                summary: None,
                timestamp: Utc::now(),
                color: None,
                read: false,
            };
            bridge
                .send_to_inbox("test-team", "developer", message)
                .unwrap();
        }

        // 标记为已读
        let marked = bridge.mark_as_read("test-team", "developer").unwrap();
        assert_eq!(marked, 2);

        // 验证已标记
        let messages = bridge.read_inbox("test-team", "developer").unwrap();
        assert!(messages.iter().all(|m| m.read));

        // 再次标记应返回 0
        let marked_again = bridge.mark_as_read("test-team", "developer").unwrap();
        assert_eq!(marked_again, 0);
    }

    #[test]
    fn test_get_team_status() {
        let (bridge, _temp) = create_test_bridge();

        bridge
            .create_team("test-team", "Test description", "/path/to/project")
            .unwrap();

        let member = TeamMember {
            name: "developer".to_string(),
            agent_id: "developer@test-team".to_string(),
            agent_type: "general-purpose".to_string(),
            model: None,
            color: None,
            is_active: Some(true),
            tmux_pane_id: None,
            cwd: None,
        };
        bridge.spawn_member("test-team", member).unwrap();

        // 发送一条未读消息
        let message = InboxMessage {
            from: "sender".to_string(),
            text: "test".to_string(),
            summary: None,
            timestamp: Utc::now(),
            color: None,
            read: false,
        };
        bridge
            .send_to_inbox("test-team", "developer", message)
            .unwrap();

        let status = bridge.get_team_status("test-team").unwrap();

        assert_eq!(status.team_name, "test-team");
        assert_eq!(status.description, Some("Test description".to_string()));
        assert_eq!(status.members.len(), 1);
        assert_eq!(status.unread_messages, 1);
    }

    #[test]
    fn test_list_teams() {
        let (bridge, _temp) = create_test_bridge();

        bridge.create_team("team-a", "A", "/a").unwrap();
        bridge.create_team("team-b", "B", "/b").unwrap();
        bridge.create_team("team-c", "C", "/c").unwrap();

        let teams = bridge.list_teams();
        assert_eq!(teams, vec!["team-a", "team-b", "team-c"]);
    }

    #[test]
    fn test_agent_id_parse() {
        let id = AgentId::parse("developer@my-team");
        assert!(id.is_some());
        let id = id.unwrap();
        assert_eq!(id.name, "developer");
        assert_eq!(id.team, "my-team");
        assert_eq!(id.to_string(), "developer@my-team");
    }

    #[test]
    fn test_agent_id_parse_invalid() {
        assert!(AgentId::parse("invalid").is_none());
        assert!(AgentId::parse("").is_none());
    }

    #[test]
    fn test_special_message_serialization() {
        let msg = SpecialMessage::PermissionRequest {
            tool: "Bash".to_string(),
            input: serde_json::json!({"command": "ls -la"}),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("permission_request"));
        assert!(json.contains("Bash"));

        let parsed: SpecialMessage = serde_json::from_str(&json).unwrap();
        if let SpecialMessage::PermissionRequest { tool, .. } = parsed {
            assert_eq!(tool, "Bash");
        } else {
            panic!("Wrong variant");
        }
    }
}
