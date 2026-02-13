//! 会话管理模块 - 管理 Claude Code 等代理的会话

use serde::{Deserialize, Serialize};
use anyhow::Result;
use std::path::PathBuf;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use chrono::{DateTime, Utc, Duration};

use crate::tmux::TmuxManager;

/// 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub project_path: String,
    pub summary: Option<String>,
    pub git_branch: Option<String>,
    pub message_count: u32,
    pub created: String,
    pub modified: String,
    pub status: String,
}

/// 会话消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
}

/// Claude 会话索引条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeSessionEntry {
    session_id: String,
    full_path: Option<String>,
    first_prompt: Option<String>,
    summary: Option<String>,
    message_count: Option<u32>,
    created: Option<String>,
    modified: Option<String>,
    git_branch: Option<String>,
    project_path: Option<String>,
}

/// Claude 会话索引
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClaudeSessionIndex {
    version: u32,
    entries: Vec<ClaudeSessionEntry>,
}

/// JSONL 消息格式
#[derive(Debug, Clone, Deserialize)]
struct JsonlMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    message: Option<JsonlMessageContent>,
    #[serde(rename = "userMessage")]
    user_message: Option<JsonlUserMessage>,
    timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonlMessageContent {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonlUserMessage {
    content: Option<String>,
}

/// 会话过滤选项
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionFilter {
    /// 按项目路径过滤（支持部分匹配）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// 只返回最近 N 天的会话
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<i64>,
    /// 限制返回数量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// 会话管理器
pub struct SessionManager {
    claude_projects_dir: PathBuf,
    tmux_manager: TmuxManager,
}

impl SessionManager {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let claude_projects_dir = home.join(".claude").join("projects");

        Self {
            claude_projects_dir,
            tmux_manager: TmuxManager::new(),
        }
    }

    /// 列出所有 Claude Code 会话
    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        self.list_sessions_filtered(None)
    }

    /// 列出 Claude Code 会话（带过滤）
    pub fn list_sessions_filtered(&self, filter: Option<SessionFilter>) -> Result<Vec<SessionInfo>> {
        let mut sessions = Vec::new();

        if !self.claude_projects_dir.exists() {
            return Ok(sessions);
        }

        // 遍历所有项目目录
        for entry in fs::read_dir(&self.claude_projects_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let index_path = path.join("sessions-index.json");
                if index_path.exists() {
                    if let Ok(content) = fs::read_to_string(&index_path) {
                        if let Ok(index) = serde_json::from_str::<ClaudeSessionIndex>(&content) {
                            for entry in index.entries {
                                sessions.push(SessionInfo {
                                    id: entry.session_id,
                                    project_path: entry.project_path.unwrap_or_default(),
                                    summary: entry.summary,
                                    git_branch: entry.git_branch,
                                    message_count: entry.message_count.unwrap_or(0),
                                    created: entry.created.unwrap_or_default(),
                                    modified: entry.modified.unwrap_or_default(),
                                    status: "inactive".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // 应用过滤
        if let Some(filter) = filter {
            // 按项目路径过滤
            if let Some(ref project_path) = filter.project_path {
                sessions.retain(|s| s.project_path.contains(project_path));
            }

            // 按时间过滤
            if let Some(days) = filter.days {
                let cutoff = Utc::now() - Duration::days(days);
                sessions.retain(|s| {
                    if let Ok(modified) = DateTime::parse_from_rfc3339(&s.modified) {
                        modified.with_timezone(&Utc) > cutoff
                    } else {
                        false
                    }
                });
            }

            // 按修改时间排序（最新的在前）
            sessions.sort_by(|a, b| b.modified.cmp(&a.modified));

            // 限制数量
            if let Some(limit) = filter.limit {
                sessions.truncate(limit);
            }
        }

        Ok(sessions)
    }

    /// 获取指定会话的详细信息
    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionInfo>> {
        let sessions = self.list_sessions()?;
        Ok(sessions.into_iter().find(|s| s.id == session_id))
    }

    /// 恢复指定会话
    pub fn resume_session(&self, session_id: &str) -> Result<()> {
        // 查找会话对应的项目路径
        if let Some(session) = self.get_session(session_id)? {
            let project_path = if session.project_path.is_empty() {
                ".".to_string()
            } else {
                session.project_path
            };

            // 使用 claude --resume 恢复会话
            // 注意：这里只是启动命令，实际的交互需要在终端中进行
            println!("恢复会话: {} (项目: {})", session_id, project_path);
            println!("运行命令: cd {} && claude --resume {}", project_path, session_id);
            
            Ok(())
        } else {
            anyhow::bail!("会话 {} 不存在", session_id)
        }
    }

    /// 在 tmux 中恢复会话，返回 tmux session 名称
    ///
    /// 注意：此方法仅创建 tmux 会话，不会注册到 AgentManager。
    /// 如需被监控系统追踪，请使用 AgentManager::start_agent 并设置 resume_session。
    pub fn resume_in_tmux(&self, session_id: &str, tmux_session_name: Option<&str>) -> Result<String> {
        if let Some(session) = self.get_session(session_id)? {
            let project_path = if session.project_path.is_empty() {
                ".".to_string()
            } else {
                session.project_path.clone()
            };

            // 生成 tmux session 名称：cam-<session_id前8位>
            let tmux_name = tmux_session_name
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("cam-{}", &session_id[..8]));

            // 创建 tmux 会话并运行 claude --resume
            let cmd = format!("claude --resume {}", session_id);
            self.tmux_manager.create_session(&tmux_name, &project_path, &cmd)?;

            Ok(tmux_name)
        } else {
            anyhow::bail!("会话 {} 不存在", session_id)
        }
    }

    /// 向 tmux 会话发送输入
    pub fn send_to_tmux(&self, tmux_session: &str, input: &str) -> Result<()> {
        self.tmux_manager.send_keys(tmux_session, input)
    }

    /// 获取 tmux 会话列表
    pub fn list_tmux_sessions(&self) -> Result<Vec<String>> {
        self.tmux_manager.list_sessions()
    }

    /// 获取会话的最近消息
    pub fn get_session_logs(&self, session_id: &str, limit: usize) -> Result<Vec<SessionMessage>> {
        // 查找会话文件
        let jsonl_path = self.find_session_file(session_id)?;
        
        if let Some(path) = jsonl_path {
            self.parse_session_logs(&path, limit)
        } else {
            Ok(Vec::new())
        }
    }

    /// 获取指定项目路径的最新会话
    pub fn get_latest_session_by_project(&self, project_path: &str) -> Result<Option<SessionInfo>> {
        let mut sessions: Vec<SessionInfo> = self
            .list_sessions()?
            .into_iter()
            .filter(|s| s.project_path == project_path)
            .collect();

        if sessions.is_empty() {
            return Ok(None);
        }

        sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(sessions.into_iter().next())
    }

    /// 查找会话 JSONL 文件
    fn find_session_file(&self, session_id: &str) -> Result<Option<PathBuf>> {
        if !self.claude_projects_dir.exists() {
            return Ok(None);
        }

        for entry in fs::read_dir(&self.claude_projects_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                let jsonl_path = path.join(format!("{}.jsonl", session_id));
                if jsonl_path.exists() {
                    return Ok(Some(jsonl_path));
                }
            }
        }

        Ok(None)
    }

    /// 解析会话日志文件
    fn parse_session_logs(&self, path: &PathBuf, limit: usize) -> Result<Vec<SessionMessage>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut messages = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(msg) = serde_json::from_str::<JsonlMessage>(&line) {
                // 处理 assistant 消息
                if let Some(ref msg_type) = msg.msg_type {
                    if msg_type == "assistant" {
                        if let Some(ref message) = msg.message {
                            if let Some(ref content) = message.content {
                                let text = self.extract_text_content(content);
                                if !text.is_empty() {
                                    messages.push(SessionMessage {
                                        role: "assistant".to_string(),
                                        content: text,
                                        timestamp: msg.timestamp.clone(),
                                    });
                                }
                            }
                        }
                    } else if msg_type == "user" {
                        if let Some(ref user_msg) = msg.user_message {
                            if let Some(ref content) = user_msg.content {
                                messages.push(SessionMessage {
                                    role: "user".to_string(),
                                    content: content.clone(),
                                    timestamp: msg.timestamp.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // 返回最后 N 条消息
        let start = if messages.len() > limit {
            messages.len() - limit
        } else {
            0
        };

        Ok(messages[start..].to_vec())
    }

    /// 从 content 中提取文本
    fn extract_text_content(&self, content: &serde_json::Value) -> String {
        match content {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => {
                let mut texts = Vec::new();
                for item in arr {
                    if let Some(obj) = item.as_object() {
                        if obj.get("type").and_then(|t| t.as_str()) == Some("text") {
                            if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                                texts.push(text.to_string());
                            }
                        }
                    }
                }
                texts.join("\n")
            }
            _ => String::new(),
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_sessions() {
        let manager = SessionManager::new();
        let sessions = manager.list_sessions().unwrap();
        println!("Found {} sessions", sessions.len());
    }
}
