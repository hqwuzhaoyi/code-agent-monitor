//! Agent tool handlers
//!
//! Handles agent_* MCP tools: agent_start, agent_send, agent_list, agent_logs, agent_stop, agent_status

use anyhow::Result;
use serde_json::Value;

use crate::agent::{AgentManager, StartAgentRequest};
use crate::infra::input::InputWaitDetector;
use crate::infra::jsonl::{format_tool_use, JsonlEvent, JsonlParser};

/// Handle agent/start request
pub fn handle_agent_start(
    agent_manager: &AgentManager,
    params: Option<Value>,
) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let request = StartAgentRequest {
        project_path: params["project_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing project_path"))?
            .to_string(),
        agent_type: params["agent_type"].as_str().map(|s| s.to_string()),
        resume_session: params["resume_session"].as_str().map(|s| s.to_string()),
        initial_prompt: params["initial_prompt"].as_str().map(|s| s.to_string()),
        agent_id: params["agent_id"].as_str().map(|s| s.to_string()),
        tmux_session: params["tmux_session"].as_str().map(|s| s.to_string()),
    };

    let response = agent_manager.start_agent(request)?;

    Ok(serde_json::json!({
        "agent_id": response.agent_id,
        "tmux_session": response.tmux_session
    }))
}

/// Handle agent/send request
pub fn handle_agent_send(
    agent_manager: &AgentManager,
    params: Option<Value>,
) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let agent_id = params["agent_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing agent_id"))?;
    // Support both input and message parameters (for plugin compatibility)
    let input = params["input"]
        .as_str()
        .or_else(|| params["message"].as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing input or message"))?;

    agent_manager.send_input(agent_id, input)?;

    Ok(serde_json::json!({
        "success": true
    }))
}

/// Handle agent/list request
pub fn handle_agent_list(agent_manager: &AgentManager) -> Result<Value> {
    let agents = agent_manager.list_agents()?;

    let agents_json: Vec<Value> = agents
        .iter()
        .map(|a| {
            serde_json::json!({
                "agent_id": a.agent_id,
                "agent_type": a.agent_type.to_string(),
                "project_path": a.project_path,
                "tmux_session": a.tmux_session,
                "status": format!("{:?}", a.status).to_lowercase()
            })
        })
        .collect();

    Ok(serde_json::json!({
        "agents": agents_json
    }))
}

/// Handle agent/logs request
pub fn handle_agent_logs(
    agent_manager: &AgentManager,
    params: Option<Value>,
) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let agent_id = params["agent_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing agent_id"))?;
    let lines = params["lines"].as_u64().unwrap_or(50) as u32;

    let output = agent_manager.get_logs(agent_id, lines)?;

    Ok(serde_json::json!({
        "output": output
    }))
}

/// Handle agent/stop request
pub fn handle_agent_stop(
    agent_manager: &AgentManager,
    params: Option<Value>,
) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let agent_id = params["agent_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing agent_id"))?;

    agent_manager.stop_agent(agent_id)?;

    Ok(serde_json::json!({
        "success": true
    }))
}

/// Handle agent/status request - returns structured agent status
pub fn handle_agent_status(
    agent_manager: &AgentManager,
    params: Option<Value>,
) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let agent_id = params["agent_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing agent_id"))?;

    // Get agent record
    let agent = agent_manager
        .get_agent(agent_id)?
        .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", agent_id))?;

    // Get terminal output
    let terminal_output = agent_manager.get_logs(agent_id, 20).unwrap_or_default();

    // Detect if waiting for input
    let input_detector = InputWaitDetector::new();
    let wait_result = input_detector.detect_immediate(&terminal_output);

    // Parse JSONL to get recent tool calls and errors
    let (recent_tools, recent_errors) = if let Some(ref jsonl_path) = agent.jsonl_path {
        let mut parser = JsonlParser::new(jsonl_path);
        let tools = parser.get_recent_tool_calls(5).unwrap_or_default();
        let errors = parser.get_recent_errors(3).unwrap_or_default();
        (tools, errors)
    } else {
        (Vec::new(), Vec::new())
    };

    // Format tool calls
    let tools_formatted: Vec<String> = recent_tools.iter().filter_map(format_tool_use).collect();

    // Format errors
    let errors_formatted: Vec<String> = recent_errors
        .iter()
        .filter_map(|e| {
            if let JsonlEvent::Error { message, .. } = e {
                Some(message.clone())
            } else {
                None
            }
        })
        .collect();

    // Determine status
    let status = if wait_result.is_waiting {
        "waiting"
    } else {
        "running"
    };

    Ok(serde_json::json!({
        "agent_id": agent.agent_id,
        "agent_type": agent.agent_type.to_string(),
        "project_path": agent.project_path,
        "tmux_session": agent.tmux_session,
        "status": status,
        "waiting_for_input": wait_result.is_waiting,
        "wait_pattern": wait_result.pattern_type.map(|p| format!("{:?}", p)),
        "wait_context": if wait_result.is_waiting { Some(wait_result.context) } else { None },
        "recent_tools": tools_formatted,
        "recent_errors": errors_formatted,
        "started_at": agent.started_at
    }))
}

/// Handle agent_by_session_id request
pub fn handle_agent_by_session_id(
    agent_manager: &AgentManager,
    params: Option<Value>,
) -> Result<Value> {
    let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

    let session_id = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing session_id parameter"))?;

    if let Some(agent) = agent_manager.find_agent_by_session_id(session_id)? {
        Ok(serde_json::json!({
            "found": true,
            "agent_id": agent.agent_id,
            "tmux_session": agent.tmux_session,
            "project_path": agent.project_path,
            "status": agent.status
        }))
    } else {
        Ok(serde_json::json!({
            "found": false,
            "session_id": session_id
        }))
    }
}
