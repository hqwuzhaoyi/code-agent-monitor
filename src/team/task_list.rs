//! Task List 模块 - 读写 Claude Code Agent Teams 的共享任务列表
//!
//! Claude Code Agent Teams 将任务存储在 `~/.claude/tasks/{team-name}/` 目录
//! 每个任务是一个独立的 JSON 文件: `{task-id}.json`

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 任务状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Deleted,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Deleted => write!(f, "deleted"),
        }
    }
}

/// 任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(rename = "blockedBy", default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(rename = "activeForm", default)]
    pub active_form: Option<String>,
}

/// 获取 tasks 目录路径
fn get_tasks_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("tasks"))
}

/// 获取指定 team 的 tasks 目录
fn get_team_tasks_dir(team_name: &str) -> Option<PathBuf> {
    get_tasks_dir().map(|d| d.join(team_name))
}

/// 列出指定 team 的所有任务
pub fn list_tasks(team_name: &str) -> Vec<Task> {
    let tasks_dir = match get_team_tasks_dir(team_name) {
        Some(dir) => dir,
        None => return Vec::new(),
    };

    if !tasks_dir.exists() {
        return Vec::new();
    }

    let mut tasks = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&tasks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "json") {
                // 跳过 .lock 文件
                if path
                    .file_name()
                    .is_some_and(|n| n.to_str().is_some_and(|s| s.starts_with('.')))
                {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(task) = serde_json::from_str::<Task>(&content) {
                        tasks.push(task);
                    }
                }
            }
        }
    }

    // 按 ID 排序
    tasks.sort_by(|a, b| {
        let a_num: i32 = a.id.parse().unwrap_or(i32::MAX);
        let b_num: i32 = b.id.parse().unwrap_or(i32::MAX);
        a_num.cmp(&b_num)
    });

    tasks
}

/// 获取指定任务
pub fn get_task(team_name: &str, task_id: &str) -> Option<Task> {
    let tasks_dir = get_team_tasks_dir(team_name)?;
    let task_path = tasks_dir.join(format!("{}.json", task_id));

    if !task_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&task_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 更新任务状态
pub fn update_task_status(team_name: &str, task_id: &str, status: TaskStatus) -> Result<()> {
    let tasks_dir =
        get_team_tasks_dir(team_name).ok_or_else(|| anyhow::anyhow!("无法获取 tasks 目录"))?;
    let task_path = tasks_dir.join(format!("{}.json", task_id));

    if !task_path.exists() {
        return Err(anyhow::anyhow!("任务 {} 不存在", task_id));
    }

    let content = std::fs::read_to_string(&task_path)?;
    let mut task: Task = serde_json::from_str(&content)?;
    task.status = status;

    let updated_content = serde_json::to_string_pretty(&task)?;
    std::fs::write(&task_path, updated_content)?;

    Ok(())
}

/// 列出所有 team 名称
pub fn list_team_names() -> Vec<String> {
    let tasks_dir = match get_tasks_dir() {
        Some(dir) => dir,
        None => return Vec::new(),
    };

    if !tasks_dir.exists() {
        return Vec::new();
    }

    let mut teams = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&tasks_dir) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_display() {
        assert_eq!(TaskStatus::Pending.to_string(), "pending");
        assert_eq!(TaskStatus::InProgress.to_string(), "in_progress");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
    }

    #[test]
    fn test_task_deserialization() {
        let json = r#"{
            "id": "1",
            "subject": "Test task",
            "description": "A test task description",
            "status": "pending",
            "owner": null,
            "blockedBy": [],
            "blocks": [],
            "activeForm": "Testing task"
        }"#;

        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.id, "1");
        assert_eq!(task.subject, "Test task");
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.owner.is_none());
        assert!(task.blocked_by.is_empty());
    }

    #[test]
    fn test_task_deserialization_with_owner() {
        let json = r#"{
            "id": "2",
            "subject": "Assigned task",
            "description": "Task with owner",
            "status": "in_progress",
            "owner": "developer-1",
            "blockedBy": ["1"],
            "blocks": ["3"]
        }"#;

        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.id, "2");
        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.owner, Some("developer-1".to_string()));
        assert_eq!(task.blocked_by, vec!["1"]);
        assert_eq!(task.blocks, vec!["3"]);
    }

    #[test]
    fn test_list_tasks_nonexistent_team() {
        let tasks = list_tasks("nonexistent-team-12345");
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_get_task_nonexistent() {
        let task = get_task("nonexistent-team-12345", "1");
        assert!(task.is_none());
    }

    #[test]
    fn test_update_task_status_nonexistent() {
        let result = update_task_status("nonexistent-team-12345", "1", TaskStatus::Completed);
        assert!(result.is_err());
    }
}
