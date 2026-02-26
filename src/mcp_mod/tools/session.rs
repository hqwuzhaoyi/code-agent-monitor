//! Session tool handlers
//!
//! Handles session-related MCP tools: list_sessions, get_session_info, resume_session

use anyhow::Result;
use serde_json::Value;

use crate::agent::{AgentManager, StartAgentRequest};
use crate::session::{SessionFilter, SessionManager};

/// Handle list_sessions request
pub fn handle_list_sessions(params: Option<Value>) -> Result<Value> {
    let manager = SessionManager::new();

    let filter = params.map(|p| SessionFilter {
        project_path: p["project_path"].as_str().map(|s| s.to_string()),
        days: p["days"].as_i64(),
        limit: Some(p["limit"].as_u64().unwrap_or(20) as usize),
    });

    let sessions = manager.list_sessions_filtered(filter)?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&sessions)?
        }]
    }))
}

/// Handle get_session_info request
pub fn handle_get_session_info(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let session_id = params["session_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing session_id"))?;

    let manager = SessionManager::new();
    let session = manager.get_session(session_id)?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&session)?
        }]
    }))
}

/// Handle resume_session request
pub fn handle_resume_session(agent_manager: &AgentManager, params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let session_id = params["session_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing session_id"))?;

    // Get session info to get project_path
    let session_manager = SessionManager::new();
    let session = session_manager
        .get_session(session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session {} not found", session_id))?;

    let project_path = if session.project_path.is_empty() {
        ".".to_string()
    } else {
        session.project_path
    };

    // Use AgentManager to start, so it's tracked by the monitoring system
    let response = agent_manager.start_agent(StartAgentRequest {
        project_path,
        agent_type: Some("claude".to_string()),
        resume_session: Some(session_id.to_string()),
        initial_prompt: None,
        agent_id: None,
        tmux_session: None,
    })?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Session resumed in tmux\nsession_id: {}\nagent_id: {}\ntmux_session: {}\n\nUse agent_send tool to send input, agent_id: {}",
                session_id, response.agent_id, response.tmux_session, response.agent_id
            )
        }]
    }))
}

/// Handle send_input request (to tmux session)
pub fn handle_send_input(params: Option<Value>) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let tmux_session = params["tmux_session"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing tmux_session"))?;
    let input = params["input"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing input"))?;

    let manager = SessionManager::new();
    manager.send_to_tmux(tmux_session, input)?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": format!("Input sent to {}", tmux_session)
        }]
    }))
}
