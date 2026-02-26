//! Team tool handlers
//!
//! Handles team-related MCP tools: team_list, team_members, team_create, team_delete, etc.

use anyhow::Result;
use serde_json::Value;

use crate::notification::load_webhook_config_from_file;
use crate::notification::openclaw::OpenclawNotifier;
use crate::team::{discovery, InboxMessage, InboxWatcher, TeamBridge, TeamOrchestrator};

/// Handle team/list request
pub fn handle_team_list() -> Result<Value> {
    let teams = discovery::discover_teams();

    let teams_json: Vec<Value> = teams
        .iter()
        .map(|t| {
            serde_json::json!({
                "team_name": t.team_name,
                "member_count": t.members.len()
            })
        })
        .collect();

    Ok(serde_json::json!({
        "teams": teams_json
    }))
}

/// Handle team/members request
pub fn handle_team_members(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let team_name = params["team_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing team_name"))?;

    match discovery::get_team_members(team_name) {
        Some(members) => {
            let members_json: Vec<Value> = members
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "name": m.name,
                        "agent_id": m.agent_id,
                        "agent_type": m.agent_type
                    })
                })
                .collect();

            Ok(serde_json::json!({
                "team_name": team_name,
                "members": members_json
            }))
        }
        None => Err(anyhow::anyhow!("Team not found: {}", team_name)),
    }
}

/// Handle team/create request
pub fn handle_team_create(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let name = params["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing name"))?;
    let description = params["description"].as_str().unwrap_or("Created by CAM");
    let project_path = params["project_path"].as_str().unwrap_or(".");

    let bridge = TeamBridge::new();
    bridge.create_team(name, description, project_path)?;

    Ok(serde_json::json!({
        "success": true,
        "team_name": name
    }))
}

/// Handle team/delete request
pub fn handle_team_delete(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let name = params["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing name"))?;

    let bridge = TeamBridge::new();
    bridge.delete_team(name)?;

    Ok(serde_json::json!({
        "success": true,
        "team_name": name
    }))
}

/// Handle team/status request
pub fn handle_team_status(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let name = params["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing name"))?;

    let bridge = TeamBridge::new();
    let status = bridge.get_team_status(name)?;

    Ok(serde_json::to_value(status)?)
}

/// Handle inbox/read request
pub fn handle_inbox_read(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let team = params["team"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing team"))?;
    let member = params["member"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing member"))?;
    let unread_only = params["unread_only"].as_bool().unwrap_or(false);

    let bridge = TeamBridge::new();
    let messages = bridge.read_inbox(team, member)?;

    let filtered: Vec<_> = if unread_only {
        messages.into_iter().filter(|m| !m.read).collect()
    } else {
        messages
    };

    Ok(serde_json::json!({
        "team": team,
        "member": member,
        "messages": filtered
    }))
}

/// Handle inbox/send request
pub fn handle_inbox_send(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let team = params["team"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing team"))?;
    let member = params["member"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing member"))?;
    let from = params["from"].as_str().unwrap_or("cam");
    let text = params["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing text"))?;
    let summary = params["summary"].as_str().map(String::from);

    let bridge = TeamBridge::new();
    let message = InboxMessage {
        from: from.to_string(),
        text: text.to_string(),
        summary,
        timestamp: chrono::Utc::now(),
        color: None,
        read: false,
    };
    bridge.send_to_inbox(team, member, message)?;

    Ok(serde_json::json!({
        "success": true,
        "team": team,
        "member": member
    }))
}

/// Handle team/pending_requests request
pub fn handle_team_pending_requests(params: Option<Value>) -> Result<Value> {
    let bridge = TeamBridge::new();

    // If team is specified, only get requests for that team
    if let Some(params) = params {
        if let Some(team) = params["team"].as_str() {
            let status = bridge.get_team_status(team)?;
            return Ok(serde_json::json!({
                "team": team,
                "pending_tasks": status.pending_tasks,
                "unread_messages": status.unread_messages
            }));
        }
    }

    // Otherwise get requests for all teams
    let teams = bridge.list_teams();
    let mut all_pending = Vec::new();

    for team in teams {
        if let Ok(status) = bridge.get_team_status(&team) {
            if status.pending_tasks > 0 || status.unread_messages > 0 {
                all_pending.push(serde_json::json!({
                    "team": team,
                    "pending_tasks": status.pending_tasks,
                    "unread_messages": status.unread_messages
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "pending_requests": all_pending
    }))
}

/// Handle team_spawn_agent request
pub fn handle_team_spawn_agent(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let team = params
        .get("team")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing team parameter"))?;
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing name parameter"))?;
    let agent_type = params
        .get("agent_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing agent_type parameter"))?;
    let initial_prompt = params.get("initial_prompt").and_then(|v| v.as_str());

    let orchestrator = TeamOrchestrator::new();
    let result = orchestrator.spawn_agent(team, name, agent_type, initial_prompt)?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&result)?
        }]
    }))
}

/// Handle team_progress request
pub fn handle_team_progress(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let team = params
        .get("team")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing team parameter"))?;

    let orchestrator = TeamOrchestrator::new();
    let progress = orchestrator.get_team_progress(team)?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&progress)?
        }]
    }))
}

/// Handle team_shutdown request
pub fn handle_team_shutdown(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let team = params
        .get("team")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing team parameter"))?;

    let orchestrator = TeamOrchestrator::new();
    orchestrator.shutdown_team(team)?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": format!("Team '{}' has been shut down", team)
        }]
    }))
}

/// Handle team_orchestrate request
pub fn handle_team_orchestrate(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let task_desc = params
        .get("task_desc")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing task_desc parameter"))?;
    let project = params
        .get("project")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing project parameter"))?;

    let orchestrator = TeamOrchestrator::new();
    let result = orchestrator.create_team_for_task(task_desc, project)?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&result)?
        }]
    }))
}

/// Handle team_assign_task request
pub fn handle_team_assign_task(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let team = params
        .get("team")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing team parameter"))?;
    let member = params
        .get("member")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing member parameter"))?;
    let task = params
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing task parameter"))?;

    let orchestrator = TeamOrchestrator::new();
    let result = orchestrator.assign_task(team, member, task)?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&result)?
        }]
    }))
}

/// Handle handle_user_reply request
pub fn handle_user_reply(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let reply = params
        .get("reply")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing reply parameter"))?;
    let context = params.get("context").and_then(|v| v.as_str());

    let orchestrator = TeamOrchestrator::new();
    let result = orchestrator.handle_user_reply(reply, context)?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": result
        }]
    }))
}

/// Handle get_pending_permission_requests (via InboxWatcher)
pub fn handle_get_pending_permission_requests(params: Option<Value>) -> Result<Value> {
    let team = params
        .as_ref()
        .and_then(|p| p.get("team").and_then(|v| v.as_str()));

    let notifier = match load_webhook_config_from_file() {
        Some(config) => {
            OpenclawNotifier::with_webhook(config).unwrap_or_else(|_| OpenclawNotifier::new())
        }
        None => OpenclawNotifier::new(),
    };
    let watcher = InboxWatcher::new(notifier);

    let requests = if let Some(team_name) = team {
        watcher.get_pending_permission_requests(team_name)?
    } else {
        // Get requests for all teams
        let bridge = TeamBridge::new();
        let mut all_requests = Vec::new();
        for team_name in bridge.list_teams() {
            if let Ok(reqs) = watcher.get_pending_permission_requests(&team_name) {
                all_requests.extend(reqs);
            }
        }
        all_requests
    };

    let result: Vec<Value> = requests
        .iter()
        .map(|r| {
            serde_json::json!({
                "team": r.team,
                "member": r.member,
                "tool": r.tool,
                "input": r.input,
                "timestamp": r.timestamp.to_rfc3339()
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
