//! End-to-end integration tests for CAM MCP Server

use code_agent_monitor::mcp::{McpServer, McpRequest};
use code_agent_monitor::agent::AgentManager;

/// Helper to create a test MCP server
fn create_test_server() -> McpServer {
    McpServer {
        agent_manager: AgentManager::new_for_test(),
    }
}

/// Helper to call MCP method
async fn call_method(server: &McpServer, method: &str, params: serde_json::Value) -> serde_json::Value {
    let request = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: method.to_string(),
        params: Some(params),
    };
    let response = server.handle_request(request).await;
    if let Some(error) = response.error {
        panic!("MCP error: {} (code: {})", error.message, error.code);
    }
    response.result.unwrap()
}

fn cleanup_test_agents(server: &McpServer) {
    if let Ok(agents) = server.agent_manager.list_agents() {
        for agent in agents {
            let _ = server.agent_manager.stop_agent(&agent.agent_id);
        }
    }
}

#[tokio::test]
async fn test_e2e_start_send_stop_flow() {
    // Given: CAM MCP Server 运行中
    let server = create_test_server();
    cleanup_test_agents(&server);

    // When: 完整流程
    // 1. 启动 agent (使用 cat 命令以便测试输入输出)
    let start_result = call_method(&server, "agent/start", serde_json::json!({
        "project_path": "/tmp/e2e-test",
        "agent_type": "mock"
    })).await;
    let agent_id = start_result["agent_id"].as_str().unwrap();
    assert!(agent_id.starts_with("cam-"));

    // 2. 验证 agent 在列表中
    let list_result = call_method(&server, "agent/list", serde_json::json!({})).await;
    let agents = list_result["agents"].as_array().unwrap();
    assert!(agents.iter().any(|a| a["agent_id"] == agent_id));

    // 3. 发送输入
    let send_result = call_method(&server, "agent/send", serde_json::json!({
        "agent_id": agent_id,
        "input": "hello"
    })).await;
    assert_eq!(send_result["success"], true);

    // 4. 获取日志
    let logs_result = call_method(&server, "agent/logs", serde_json::json!({
        "agent_id": agent_id,
        "lines": 10
    })).await;
    // mock agent 运行 sleep，所以日志可能为空或包含 shell 输出
    assert!(logs_result["output"].is_string());

    // 5. 停止 agent
    let stop_result = call_method(&server, "agent/stop", serde_json::json!({
        "agent_id": agent_id
    })).await;
    assert_eq!(stop_result["success"], true);

    // 6. 确认已停止
    let list_result = call_method(&server, "agent/list", serde_json::json!({})).await;
    let agents = list_result["agents"].as_array().unwrap();
    assert!(!agents.iter().any(|a| a["agent_id"] == agent_id));
}

#[tokio::test]
async fn test_e2e_multiple_agents() {
    // Given: CAM MCP Server
    let server = create_test_server();
    cleanup_test_agents(&server);

    // When: 启动多个 agent
    let r1 = call_method(&server, "agent/start", serde_json::json!({
        "project_path": "/tmp/e2e-test-1",
        "agent_type": "mock"
    })).await;
    let id1 = r1["agent_id"].as_str().unwrap().to_string();

    // 等待以确保不同的 agent_id
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let r2 = call_method(&server, "agent/start", serde_json::json!({
        "project_path": "/tmp/e2e-test-2",
        "agent_type": "mock"
    })).await;
    let id2 = r2["agent_id"].as_str().unwrap().to_string();

    // Then: 列表包含两个 agent
    let list_result = call_method(&server, "agent/list", serde_json::json!({})).await;
    let agents = list_result["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 2);

    // 可以分别操作
    let send1 = call_method(&server, "agent/send", serde_json::json!({
        "agent_id": id1,
        "input": "msg1"
    })).await;
    assert_eq!(send1["success"], true);

    let send2 = call_method(&server, "agent/send", serde_json::json!({
        "agent_id": id2,
        "input": "msg2"
    })).await;
    assert_eq!(send2["success"], true);

    // Cleanup
    call_method(&server, "agent/stop", serde_json::json!({ "agent_id": id1 })).await;
    call_method(&server, "agent/stop", serde_json::json!({ "agent_id": id2 })).await;
}

#[tokio::test]
async fn test_e2e_agent_with_initial_prompt() {
    // Given: CAM MCP Server
    let server = create_test_server();
    cleanup_test_agents(&server);

    // When: 启动 agent 并带初始 prompt
    let result = call_method(&server, "agent/start", serde_json::json!({
        "project_path": "/tmp/e2e-test-prompt",
        "agent_type": "mock",
        "initial_prompt": "initial message"
    })).await;
    let agent_id = result["agent_id"].as_str().unwrap();

    // Then: agent 成功启动
    assert!(agent_id.starts_with("cam-"));

    // Cleanup
    call_method(&server, "agent/stop", serde_json::json!({ "agent_id": agent_id })).await;
}

#[tokio::test]
async fn test_e2e_stop_nonexistent_agent() {
    // Given: CAM MCP Server
    let server = create_test_server();
    cleanup_test_agents(&server);

    // When: 尝试停止不存在的 agent
    let request = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: "agent/stop".to_string(),
        params: Some(serde_json::json!({
            "agent_id": "nonexistent-agent-id"
        })),
    };
    let response = server.handle_request(request).await;

    // Then: 返回错误
    assert!(response.error.is_some());
}

#[tokio::test]
async fn test_e2e_send_to_nonexistent_agent() {
    // Given: CAM MCP Server
    let server = create_test_server();
    cleanup_test_agents(&server);

    // When: 尝试向不存在的 agent 发送消息
    let request = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: "agent/send".to_string(),
        params: Some(serde_json::json!({
            "agent_id": "nonexistent-agent-id",
            "input": "hello"
        })),
    };
    let response = server.handle_request(request).await;

    // Then: 返回错误
    assert!(response.error.is_some());
}

#[tokio::test]
async fn test_e2e_logs_from_nonexistent_agent() {
    // Given: CAM MCP Server
    let server = create_test_server();
    cleanup_test_agents(&server);

    // When: 尝试获取不存在的 agent 的日志
    let request = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: "agent/logs".to_string(),
        params: Some(serde_json::json!({
            "agent_id": "nonexistent-agent-id"
        })),
    };
    let response = server.handle_request(request).await;

    // Then: 返回错误
    assert!(response.error.is_some());
}
