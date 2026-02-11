//! Conversation State Manager 模块 - 对话状态管理
//!
//! 追踪对话上下文，支持快捷回复（y/n/1/2/3）。
//!
//! 存储位置：`~/.claude-monitor/conversation_state.json`

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::agent::AgentManager;
use crate::team::{TeamBridge, InboxMessage};

/// 确认类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConfirmationType {
    /// 权限请求
    #[serde(rename = "permission_request")]
    PermissionRequest {
        tool: String,
        input: serde_json::Value,
    },
    /// 任务审批
    #[serde(rename = "task_approval")]
    TaskApproval { task_id: String },
    /// 关闭请求
    #[serde(rename = "shutdown_request")]
    ShutdownRequest { request_id: String },
    /// 选项选择
    #[serde(rename = "option_selection")]
    OptionSelection { options: Vec<String> },
}

/// 待处理的确认
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConfirmation {
    /// 确认 ID
    pub id: String,
    /// Agent ID (cam-xxx 或 name@team)
    pub agent_id: String,
    /// Team 名称（如果是 team 成员）
    pub team: Option<String>,
    /// 确认类型
    pub confirmation_type: ConfirmationType,
    /// 上下文描述
    pub context: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// tmux session 名称（用于发送回复）
    pub tmux_session: Option<String>,
}

/// Agent 上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    /// Agent ID
    pub agent_id: String,
    /// Team 名称
    pub team: Option<String>,
    /// tmux session 名称
    pub tmux_session: Option<String>,
    /// 项目路径
    pub project_path: Option<String>,
}

/// 对话状态
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationState {
    /// 当前活跃的 Team
    pub current_team: Option<String>,
    /// 当前活跃的 Agent
    pub current_agent: Option<AgentContext>,
    /// 待处理的确认列表
    pub pending_confirmations: Vec<PendingConfirmation>,
    /// 最后更新时间
    pub last_updated: Option<DateTime<Utc>>,
}

/// 回复结果
#[derive(Debug, Clone)]
pub enum ReplyResult {
    /// 回复已发送
    Sent {
        agent_id: String,
        reply: String,
    },
    /// 需要选择目标
    NeedSelection {
        options: Vec<PendingConfirmation>,
    },
    /// 没有待处理的确认
    NoPending,
    /// 无效的选择
    InvalidSelection(String),
}

/// 对话状态管理器
pub struct ConversationStateManager {
    state_file: PathBuf,
    agent_manager: AgentManager,
    team_bridge: TeamBridge,
}

impl ConversationStateManager {
    /// 创建新的状态管理器
    pub fn new() -> Self {
        let state_file = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude-monitor")
            .join("conversation_state.json");

        Self {
            state_file,
            agent_manager: AgentManager::new(),
            team_bridge: TeamBridge::new(),
        }
    }

    /// 创建用于测试的状态管理器
    pub fn new_for_test(state_file: PathBuf) -> Self {
        Self {
            state_file,
            agent_manager: AgentManager::new_for_test(),
            team_bridge: TeamBridge::new(),
        }
    }

    /// 加载状态
    pub fn load_state(&self) -> Result<ConversationState> {
        if !self.state_file.exists() {
            return Ok(ConversationState::default());
        }

        let content = fs::read_to_string(&self.state_file)?;
        let state: ConversationState = serde_json::from_str(&content)?;
        Ok(state)
    }

    /// 保存状态
    pub fn save_state(&self, state: &ConversationState) -> Result<()> {
        // 确保目录存在
        if let Some(parent) = self.state_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(state)?;
        fs::write(&self.state_file, content)?;
        Ok(())
    }

    /// 注册待处理的确认
    pub fn register_pending(
        &self,
        agent_id: &str,
        team: Option<&str>,
        confirmation_type: ConfirmationType,
        context: &str,
        tmux_session: Option<&str>,
    ) -> Result<String> {
        let mut state = self.load_state()?;

        // 生成确认 ID
        let id = format!("conf-{}", chrono::Utc::now().timestamp_millis());

        let confirmation = PendingConfirmation {
            id: id.clone(),
            agent_id: agent_id.to_string(),
            team: team.map(|s| s.to_string()),
            confirmation_type,
            context: context.to_string(),
            created_at: Utc::now(),
            tmux_session: tmux_session.map(|s| s.to_string()),
        };

        state.pending_confirmations.push(confirmation);
        state.last_updated = Some(Utc::now());

        // 清理过期的确认（超过 1 小时）
        let one_hour_ago = Utc::now() - chrono::Duration::hours(1);
        state
            .pending_confirmations
            .retain(|c| c.created_at > one_hour_ago);

        self.save_state(&state)?;
        Ok(id)
    }

    /// 获取所有待处理的确认
    pub fn get_pending_confirmations(&self) -> Result<Vec<PendingConfirmation>> {
        let state = self.load_state()?;
        Ok(state.pending_confirmations)
    }

    /// 获取最近的待处理确认
    pub fn get_latest_pending(&self) -> Result<Option<PendingConfirmation>> {
        let state = self.load_state()?;
        Ok(state.pending_confirmations.last().cloned())
    }

    /// 移除待处理的确认
    pub fn remove_pending(&self, confirmation_id: &str) -> Result<Option<PendingConfirmation>> {
        let mut state = self.load_state()?;

        let pos = state
            .pending_confirmations
            .iter()
            .position(|c| c.id == confirmation_id);

        let removed = pos.map(|i| state.pending_confirmations.remove(i));
        state.last_updated = Some(Utc::now());

        self.save_state(&state)?;
        Ok(removed)
    }

    /// 处理快捷回复
    ///
    /// 支持的回复格式：
    /// - "y" / "yes" / "是" / "好" / "可以" -> 发送 "y"
    /// - "n" / "no" / "否" / "不" / "取消" -> 发送 "n"
    /// - "1" / "2" / "3" -> 发送对应选项
    /// - 其他 -> 原样发送
    pub fn handle_reply(&self, reply: &str, target: Option<&str>) -> Result<ReplyResult> {
        let pending = self.get_pending_confirmations()?;

        if pending.is_empty() {
            return Ok(ReplyResult::NoPending);
        }

        // 解析回复
        let normalized_reply = self.normalize_reply(reply);

        // 确定目标
        let target_confirmation = if let Some(target_id) = target {
            // 指定了目标
            pending
                .iter()
                .find(|c| c.agent_id == target_id || c.id == target_id)
                .cloned()
        } else if pending.len() == 1 {
            // 只有一个待处理，直接使用
            pending.first().cloned()
        } else {
            // 多个待处理，需要选择
            return Ok(ReplyResult::NeedSelection { options: pending });
        };

        let confirmation = match target_confirmation {
            Some(c) => c,
            None => {
                return Ok(ReplyResult::InvalidSelection(format!(
                    "未找到目标: {}",
                    target.unwrap_or("unknown")
                )));
            }
        };

        // 发送回复
        self.send_reply_to_agent(&confirmation, &normalized_reply)?;

        // 移除已处理的确认
        self.remove_pending(&confirmation.id)?;

        Ok(ReplyResult::Sent {
            agent_id: confirmation.agent_id,
            reply: normalized_reply,
        })
    }

    /// 标准化回复
    fn normalize_reply(&self, reply: &str) -> String {
        let reply_lower = reply.to_lowercase().trim().to_string();

        match reply_lower.as_str() {
            "y" | "yes" | "是" | "好" | "可以" | "确认" | "同意" | "允许" => "y".to_string(),
            "n" | "no" | "否" | "不" | "取消" | "拒绝" | "不允许" => "n".to_string(),
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => reply_lower,
            _ => reply.to_string(),
        }
    }

    /// 发送回复到 agent
    fn send_reply_to_agent(&self, confirmation: &PendingConfirmation, reply: &str) -> Result<()> {
        // 优先使用 tmux_session
        if let Some(ref tmux_session) = confirmation.tmux_session {
            return self.send_to_tmux(tmux_session, reply);
        }

        // 尝试通过 agent_id 查找 tmux session
        if let Ok(agents) = self.agent_manager.list_agents() {
            for agent in agents {
                if agent.agent_id == confirmation.agent_id {
                    return self.send_to_tmux(&agent.tmux_session, reply);
                }
            }
        }

        // 如果是 team 成员，尝试通过 inbox 发送
        if let Some(ref team) = confirmation.team {
            // 从 agent_id 提取成员名称 (name@team 格式)
            let member_name = if confirmation.agent_id.contains('@') {
                confirmation.agent_id.split('@').next().unwrap_or("")
            } else {
                &confirmation.agent_id
            };

            if !member_name.is_empty() {
                let msg = InboxMessage {
                    from: "user".to_string(),
                    text: reply.to_string(),
                    summary: Some("用户回复".to_string()),
                    timestamp: Utc::now(),
                    color: None,
                    read: false,
                };
                self.team_bridge.send_to_inbox(team, member_name, msg)?;
                return Ok(());
            }
        }

        Err(anyhow!(
            "无法找到 agent {} 的通信方式",
            confirmation.agent_id
        ))
    }

    /// 发送到 tmux session
    fn send_to_tmux(&self, session: &str, message: &str) -> Result<()> {
        use std::process::Command;

        // 使用 -l 标志发送字面文本，避免特殊字符被解释
        let status = Command::new("tmux")
            .args(["send-keys", "-t", session, "-l", message])
            .status()?;

        if !status.success() {
            return Err(anyhow!("发送文本到 tmux 失败"));
        }

        // 发送回车
        let status = Command::new("tmux")
            .args(["send-keys", "-t", session, "Enter"])
            .status()?;

        if !status.success() {
            return Err(anyhow!("发送回车到 tmux 失败"));
        }

        Ok(())
    }

    /// 设置当前活跃的 Team
    pub fn set_current_team(&self, team: Option<&str>) -> Result<()> {
        let mut state = self.load_state()?;
        state.current_team = team.map(|s| s.to_string());
        state.last_updated = Some(Utc::now());
        self.save_state(&state)
    }

    /// 设置当前活跃的 Agent
    pub fn set_current_agent(&self, agent: Option<AgentContext>) -> Result<()> {
        let mut state = self.load_state()?;
        state.current_agent = agent;
        state.last_updated = Some(Utc::now());
        self.save_state(&state)
    }

    /// 获取当前活跃的 Team
    pub fn get_current_team(&self) -> Result<Option<String>> {
        let state = self.load_state()?;
        Ok(state.current_team)
    }

    /// 获取当前活跃的 Agent
    pub fn get_current_agent(&self) -> Result<Option<AgentContext>> {
        let state = self.load_state()?;
        Ok(state.current_agent)
    }
}

impl Default for ConversationStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_manager() -> (ConversationStateManager, tempfile::TempDir) {
        let temp = tempdir().unwrap();
        let state_file = temp.path().join("conversation_state.json");
        let manager = ConversationStateManager::new_for_test(state_file);
        (manager, temp)
    }

    #[test]
    fn test_load_empty_state() {
        let (manager, _temp) = create_test_manager();
        let state = manager.load_state().unwrap();

        assert!(state.current_team.is_none());
        assert!(state.current_agent.is_none());
        assert!(state.pending_confirmations.is_empty());
    }

    #[test]
    fn test_register_pending() {
        let (manager, _temp) = create_test_manager();

        let id = manager
            .register_pending(
                "cam-123",
                None,
                ConfirmationType::PermissionRequest {
                    tool: "Bash".to_string(),
                    input: serde_json::json!({"command": "ls"}),
                },
                "执行 ls 命令",
                Some("cam-123"),
            )
            .unwrap();

        assert!(id.starts_with("conf-"));

        let pending = manager.get_pending_confirmations().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].agent_id, "cam-123");
    }

    #[test]
    fn test_remove_pending() {
        let (manager, _temp) = create_test_manager();

        let id = manager
            .register_pending(
                "cam-123",
                None,
                ConfirmationType::PermissionRequest {
                    tool: "Bash".to_string(),
                    input: serde_json::json!({}),
                },
                "test",
                None,
            )
            .unwrap();

        let removed = manager.remove_pending(&id).unwrap();
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, id);

        let pending = manager.get_pending_confirmations().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_normalize_reply() {
        let (manager, _temp) = create_test_manager();

        assert_eq!(manager.normalize_reply("y"), "y");
        assert_eq!(manager.normalize_reply("Y"), "y");
        assert_eq!(manager.normalize_reply("yes"), "y");
        assert_eq!(manager.normalize_reply("YES"), "y");
        assert_eq!(manager.normalize_reply("是"), "y");
        assert_eq!(manager.normalize_reply("好"), "y");
        assert_eq!(manager.normalize_reply("可以"), "y");

        assert_eq!(manager.normalize_reply("n"), "n");
        assert_eq!(manager.normalize_reply("N"), "n");
        assert_eq!(manager.normalize_reply("no"), "n");
        assert_eq!(manager.normalize_reply("否"), "n");
        assert_eq!(manager.normalize_reply("不"), "n");
        assert_eq!(manager.normalize_reply("取消"), "n");

        assert_eq!(manager.normalize_reply("1"), "1");
        assert_eq!(manager.normalize_reply("2"), "2");

        assert_eq!(manager.normalize_reply("custom reply"), "custom reply");
    }

    #[test]
    fn test_handle_reply_no_pending() {
        let (manager, _temp) = create_test_manager();

        let result = manager.handle_reply("y", None).unwrap();
        assert!(matches!(result, ReplyResult::NoPending));
    }

    #[test]
    fn test_handle_reply_need_selection() {
        let (manager, _temp) = create_test_manager();

        // 注册两个待处理确认
        manager
            .register_pending(
                "cam-123",
                None,
                ConfirmationType::PermissionRequest {
                    tool: "Bash".to_string(),
                    input: serde_json::json!({}),
                },
                "test1",
                None,
            )
            .unwrap();

        manager
            .register_pending(
                "cam-456",
                None,
                ConfirmationType::PermissionRequest {
                    tool: "Write".to_string(),
                    input: serde_json::json!({}),
                },
                "test2",
                None,
            )
            .unwrap();

        let result = manager.handle_reply("y", None).unwrap();
        assert!(matches!(result, ReplyResult::NeedSelection { .. }));
    }

    #[test]
    fn test_set_current_team() {
        let (manager, _temp) = create_test_manager();

        manager.set_current_team(Some("my-team")).unwrap();
        assert_eq!(manager.get_current_team().unwrap(), Some("my-team".to_string()));

        manager.set_current_team(None).unwrap();
        assert_eq!(manager.get_current_team().unwrap(), None);
    }

    #[test]
    fn test_set_current_agent() {
        let (manager, _temp) = create_test_manager();

        let agent = AgentContext {
            agent_id: "cam-123".to_string(),
            team: Some("my-team".to_string()),
            tmux_session: Some("cam-123".to_string()),
            project_path: Some("/workspace".to_string()),
        };

        manager.set_current_agent(Some(agent.clone())).unwrap();
        let loaded = manager.get_current_agent().unwrap().unwrap();
        assert_eq!(loaded.agent_id, "cam-123");
        assert_eq!(loaded.team, Some("my-team".to_string()));
    }

    #[test]
    fn test_get_latest_pending() {
        let (manager, _temp) = create_test_manager();

        manager
            .register_pending(
                "cam-123",
                None,
                ConfirmationType::PermissionRequest {
                    tool: "Bash".to_string(),
                    input: serde_json::json!({}),
                },
                "first",
                None,
            )
            .unwrap();

        manager
            .register_pending(
                "cam-456",
                None,
                ConfirmationType::PermissionRequest {
                    tool: "Write".to_string(),
                    input: serde_json::json!({}),
                },
                "second",
                None,
            )
            .unwrap();

        let latest = manager.get_latest_pending().unwrap().unwrap();
        assert_eq!(latest.agent_id, "cam-456");
        assert_eq!(latest.context, "second");
    }

    #[test]
    fn test_confirmation_type_serialization() {
        let perm = ConfirmationType::PermissionRequest {
            tool: "Bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
        };
        let json = serde_json::to_string(&perm).unwrap();
        assert!(json.contains("permission_request"));
        assert!(json.contains("Bash"));

        let task = ConfirmationType::TaskApproval {
            task_id: "123".to_string(),
        };
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("task_approval"));

        let shutdown = ConfirmationType::ShutdownRequest {
            request_id: "req-1".to_string(),
        };
        let json = serde_json::to_string(&shutdown).unwrap();
        assert!(json.contains("shutdown_request"));
    }
}
