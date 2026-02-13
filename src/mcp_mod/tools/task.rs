//! Task tool handlers
//!
//! Handles task-related MCP tools: task_list, task_get, task_update

use anyhow::Result;
use serde_json::Value;

use crate::session::state::{ConversationStateManager, ReplyResult};
use crate::task_list::{self, TaskStatus};

/// Handle task_list request
pub fn handle_task_list(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let team_name = params
        .get("team_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing team_name parameter"))?;

    let tasks = task_list::list_tasks(team_name);
    let result: Vec<Value> = tasks
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "subject": t.subject,
                "status": t.status.to_string(),
                "owner": t.owner,
                "blocked_by": t.blocked_by
            })
        })
        .collect();

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&result)?
        }]
    }))
}

/// Handle task_get request
pub fn handle_task_get(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let team_name = params
        .get("team_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing team_name parameter"))?;
    let task_id = params
        .get("task_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing task_id parameter"))?;

    let task = task_list::get_task(team_name, task_id)
        .ok_or_else(|| anyhow::anyhow!("Task {} not found", task_id))?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&task)?
        }]
    }))
}

/// Handle task_update request
pub fn handle_task_update(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let team_name = params
        .get("team_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing team_name parameter"))?;
    let task_id = params
        .get("task_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing task_id parameter"))?;
    let status_str = params
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing status parameter"))?;

    let status = match status_str {
        "pending" => TaskStatus::Pending,
        "in_progress" => TaskStatus::InProgress,
        "completed" => TaskStatus::Completed,
        "deleted" => TaskStatus::Deleted,
        _ => return Err(anyhow::anyhow!("Invalid status: {}", status_str)),
    };

    task_list::update_task_status(team_name, task_id, status)?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": format!("Task {} status updated to {}", task_id, status_str)
        }]
    }))
}

/// Handle get_pending_confirmations request
pub fn handle_get_pending_confirmations() -> Result<Value> {
    let state_manager = ConversationStateManager::new();
    let pending = state_manager.get_pending_confirmations()?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&pending)?
        }]
    }))
}

/// Handle reply_pending request
pub fn handle_reply_pending(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let reply = params
        .get("reply")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing reply parameter"))?;
    let target = params.get("target").and_then(|v| v.as_str());

    let state_manager = ConversationStateManager::new();
    let result = state_manager.handle_reply(reply, target)?;

    let response = match result {
        ReplyResult::Sent { agent_id, reply } => {
            serde_json::json!({
                "status": "sent",
                "agent_id": agent_id,
                "reply": reply
            })
        }
        ReplyResult::NeedSelection { options } => {
            serde_json::json!({
                "status": "need_selection",
                "options": options.iter().map(|o| serde_json::json!({
                    "id": o.id,
                    "agent_id": o.agent_id,
                    "context": o.context
                })).collect::<Vec<_>>()
            })
        }
        ReplyResult::NoPending => {
            serde_json::json!({
                "status": "no_pending",
                "message": "No pending confirmation requests"
            })
        }
        ReplyResult::InvalidSelection(msg) => {
            serde_json::json!({
                "status": "invalid_selection",
                "message": msg
            })
        }
    };

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&response)?
        }]
    }))
}
