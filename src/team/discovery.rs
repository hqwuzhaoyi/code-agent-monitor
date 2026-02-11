//! Team Discovery 模块 - 发现和管理 Claude Code Agent Teams
//!
//! Claude Code Agent Teams 将配置存储在 `~/.claude/teams/{team-name}/config.json`

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Team 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub team_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    pub members: Vec<TeamMember>,
}

/// Team 成员
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub name: String,
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "agentType")]
    pub agent_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, rename = "isActive", skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
    #[serde(default, rename = "tmuxPaneId", skip_serializing_if = "Option::is_none")]
    pub tmux_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// 获取 teams 目录路径
fn get_teams_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("teams"))
}

/// 发现所有 teams
pub fn discover_teams() -> Vec<TeamConfig> {
    let teams_dir = match get_teams_dir() {
        Some(dir) => dir,
        None => return Vec::new(),
    };

    if !teams_dir.exists() {
        return Vec::new();
    }

    let mut teams = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&teams_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let team_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());

                if let Some(name) = team_name {
                    if let Some(config) = load_team_config(&path) {
                        teams.push(TeamConfig {
                            team_name: name,
                            description: config.description,
                            lead_agent_id: config.lead_agent_id,
                            created_at: config.created_at,
                            members: config.members,
                        });
                    }
                }
            }
        }
    }

    teams
}

/// 获取指定 team 的成员
pub fn get_team_members(team_name: &str) -> Option<Vec<TeamMember>> {
    let teams_dir = get_teams_dir()?;
    let team_path = teams_dir.join(team_name);

    if !team_path.exists() {
        return None;
    }

    load_team_config(&team_path).map(|c| c.members)
}

/// 获取指定 team 的活跃成员
pub fn get_active_team_members(team_name: &str) -> Option<Vec<TeamMember>> {
    get_team_members(team_name).map(|members| {
        members.into_iter()
            .filter(|m| m.is_active.unwrap_or(false))
            .collect()
    })
}

/// 从目录加载 team 配置
fn load_team_config(team_dir: &PathBuf) -> Option<TeamConfigFile> {
    let config_path = team_dir.join("config.json");

    if !config_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&config_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Team 配置文件结构（内部使用）
#[derive(Debug, Deserialize)]
struct TeamConfigFile {
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "leadAgentId")]
    lead_agent_id: Option<String>,
    #[serde(default, rename = "createdAt")]
    created_at: Option<u64>,
    #[serde(default)]
    members: Vec<TeamMember>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_teams_returns_vec() {
        // discover_teams 应该返回一个 Vec，即使为空
        let teams = discover_teams();
        assert!(teams.len() >= 0);
    }

    #[test]
    fn test_get_team_members_not_found() {
        let result = get_team_members("nonexistent-team-12345");
        assert!(result.is_none());
    }

    #[test]
    fn test_team_config_deserialization() {
        let json = r#"{
            "members": [
                {
                    "name": "team-lead",
                    "agentId": "abc-123",
                    "agentType": "general-purpose",
                    "isActive": true
                },
                {
                    "name": "developer",
                    "agentId": "def-456",
                    "agentType": "Bash",
                    "isActive": false,
                    "color": "blue"
                }
            ]
        }"#;

        let config: TeamConfigFile = serde_json::from_str(json).unwrap();
        assert_eq!(config.members.len(), 2);
        assert_eq!(config.members[0].name, "team-lead");
        assert_eq!(config.members[0].agent_id, "abc-123");
        assert_eq!(config.members[0].is_active, Some(true));
        assert_eq!(config.members[1].agent_type, "Bash");
        assert_eq!(config.members[1].is_active, Some(false));
        assert_eq!(config.members[1].color, Some("blue".to_string()));
    }

    #[test]
    fn test_full_team_config_deserialization() {
        let json = r#"{
            "name": "test-team",
            "description": "Test team description",
            "createdAt": 1770481532431,
            "leadAgentId": "team-lead@test-team",
            "members": [
                {
                    "name": "team-lead",
                    "agentId": "team-lead@test-team",
                    "agentType": "team-lead",
                    "model": "claude-opus-4-6",
                    "tmuxPaneId": "%1",
                    "cwd": "/workspace",
                    "isActive": true
                }
            ]
        }"#;

        let config: TeamConfigFile = serde_json::from_str(json).unwrap();
        assert_eq!(config.description, Some("Test team description".to_string()));
        assert_eq!(config.lead_agent_id, Some("team-lead@test-team".to_string()));
        assert_eq!(config.created_at, Some(1770481532431));
        assert_eq!(config.members[0].model, Some("claude-opus-4-6".to_string()));
        assert_eq!(config.members[0].tmux_pane_id, Some("%1".to_string()));
    }
}
