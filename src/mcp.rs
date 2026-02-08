//! MCP Server 模块 - 提供 MCP 协议接口

use serde::{Deserialize, Serialize};
use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use crate::{ProcessScanner, SessionManager, AgentManager, StartAgentRequest};
use crate::jsonl_parser::{JsonlParser, JsonlEvent, format_tool_use};
use crate::input_detector::InputWaitDetector;
use crate::team_discovery;
use crate::task_list;

/// MCP 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// MCP 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

/// MCP 错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
}

/// MCP 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpTool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: serde_json::Value,
}

/// MCP Server
pub struct McpServer {
    pub agent_manager: AgentManager,
}

impl McpServer {
    pub fn new(_port: u16) -> Self {
        Self {
            agent_manager: AgentManager::new(),
        }
    }

    /// 创建用于测试的 MCP Server
    pub fn new_for_test() -> Self {
        Self {
            agent_manager: AgentManager::new_for_test(),
        }
    }

    /// 运行 MCP Server (stdio 模式)
    pub async fn run(&self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        eprintln!("Code Agent Monitor MCP Server 已启动 (stdio 模式)");

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                break; // EOF
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<McpRequest>(line) {
                Ok(request) => {
                    let response = self.handle_request(request).await;
                    let response_json = serde_json::to_string(&response)?;
                    stdout.write_all(response_json.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                }
                Err(e) => {
                    eprintln!("解析请求失败: {}", e);
                }
            }
        }

        Ok(())
    }

    /// 处理 MCP 请求
    pub async fn handle_request(&self, request: McpRequest) -> McpResponse {
        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(request.params),
            // 新增的 agent/* 方法
            "agent/start" => self.handle_agent_start(request.params),
            "agent/send" => self.handle_agent_send(request.params),
            "agent/list" => self.handle_agent_list(),
            "agent/logs" => self.handle_agent_logs(request.params),
            "agent/stop" => self.handle_agent_stop(request.params),
            "agent/status" => self.handle_agent_status(request.params),
            // Team discovery methods
            "team/list" => self.handle_team_list(),
            "team/members" => self.handle_team_members(request.params),
            _ => Err(anyhow::anyhow!("Method not found: {}", request.method)),
        };

        match result {
            Ok(value) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(value),
                error: None,
            },
            Err(e) => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError {
                    code: if e.to_string().contains("not found") { -32601 } else { -32603 },
                    message: e.to_string(),
                }),
            },
        }
    }

    /// 处理 agent/start
    fn handle_agent_start(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let request = StartAgentRequest {
            project_path: params["project_path"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing project_path"))?
                .to_string(),
            agent_type: params["agent_type"].as_str().map(|s| s.to_string()),
            resume_session: params["resume_session"].as_str().map(|s| s.to_string()),
            initial_prompt: params["initial_prompt"].as_str().map(|s| s.to_string()),
        };

        let response = self.agent_manager.start_agent(request)?;

        Ok(serde_json::json!({
            "agent_id": response.agent_id,
            "tmux_session": response.tmux_session
        }))
    }

    /// 处理 agent/send
    fn handle_agent_send(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let agent_id = params["agent_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing agent_id"))?;
        // 同时支持 input 和 message 参数（兼容 plugin）
        let input = params["input"]
            .as_str()
            .or_else(|| params["message"].as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing input or message"))?;

        self.agent_manager.send_input(agent_id, input)?;

        Ok(serde_json::json!({
            "success": true
        }))
    }

    /// 处理 agent/list
    fn handle_agent_list(&self) -> Result<serde_json::Value> {
        let agents = self.agent_manager.list_agents()?;

        let agents_json: Vec<serde_json::Value> = agents.iter().map(|a| {
            serde_json::json!({
                "agent_id": a.agent_id,
                "agent_type": a.agent_type.to_string(),
                "project_path": a.project_path,
                "tmux_session": a.tmux_session,
                "status": format!("{:?}", a.status).to_lowercase()
            })
        }).collect();

        Ok(serde_json::json!({
            "agents": agents_json
        }))
    }

    /// 处理 agent/logs
    fn handle_agent_logs(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let agent_id = params["agent_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing agent_id"))?;
        let lines = params["lines"].as_u64().unwrap_or(50) as u32;

        let output = self.agent_manager.get_logs(agent_id, lines)?;

        Ok(serde_json::json!({
            "output": output
        }))
    }

    /// 处理 agent/stop
    fn handle_agent_stop(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let agent_id = params["agent_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing agent_id"))?;

        self.agent_manager.stop_agent(agent_id)?;

        Ok(serde_json::json!({
            "success": true
        }))
    }

    /// 处理 agent/status - 返回结构化的 agent 状态
    fn handle_agent_status(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let agent_id = params["agent_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing agent_id"))?;

        // 获取 agent 记录
        let agent = self.agent_manager.get_agent(agent_id)?
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {}", agent_id))?;

        // 获取终端输出
        let terminal_output = self.agent_manager.get_logs(agent_id, 20).unwrap_or_default();

        // 检测是否在等待输入
        let input_detector = InputWaitDetector::new();
        let wait_result = input_detector.detect_immediate(&terminal_output);

        // 解析 JSONL 获取最近的工具调用和错误
        let (recent_tools, recent_errors) = if let Some(ref jsonl_path) = agent.jsonl_path {
            let mut parser = JsonlParser::new(jsonl_path);
            let tools = parser.get_recent_tool_calls(5).unwrap_or_default();
            let errors = parser.get_recent_errors(3).unwrap_or_default();
            (tools, errors)
        } else {
            (Vec::new(), Vec::new())
        };

        // 格式化工具调用
        let tools_formatted: Vec<String> = recent_tools.iter()
            .filter_map(|e| format_tool_use(e))
            .collect();

        // 格式化错误
        let errors_formatted: Vec<String> = recent_errors.iter()
            .filter_map(|e| {
                if let JsonlEvent::Error { message, .. } = e {
                    Some(message.clone())
                } else {
                    None
                }
            })
            .collect();

        // 确定状态
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

    /// 处理 team/list - 列出所有 teams
    fn handle_team_list(&self) -> Result<serde_json::Value> {
        let teams = team_discovery::discover_teams();

        let teams_json: Vec<serde_json::Value> = teams.iter().map(|t| {
            serde_json::json!({
                "team_name": t.team_name,
                "member_count": t.members.len()
            })
        }).collect();

        Ok(serde_json::json!({
            "teams": teams_json
        }))
    }

    /// 处理 team/members - 获取指定 team 的成员
    fn handle_team_members(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let team_name = params["team_name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing team_name"))?;

        match team_discovery::get_team_members(team_name) {
            Some(members) => {
                let members_json: Vec<serde_json::Value> = members.iter().map(|m| {
                    serde_json::json!({
                        "name": m.name,
                        "agent_id": m.agent_id,
                        "agent_type": m.agent_type
                    })
                }).collect();

                Ok(serde_json::json!({
                    "team_name": team_name,
                    "members": members_json
                }))
            }
            None => Err(anyhow::anyhow!("Team not found: {}", team_name)),
        }
    }

    /// 处理 initialize
    fn handle_initialize(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "code-agent-monitor",
                "version": "0.1.0"
            }
        }))
    }

    /// 处理 tools/list
    fn handle_tools_list(&self) -> Result<serde_json::Value> {
        let tools = vec![
            McpTool {
                name: "list_agents".to_string(),
                description: "列出所有正在运行的 AI 编码代理进程 (Claude Code, OpenCode, Codex 等)".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            McpTool {
                name: "get_agent_info".to_string(),
                description: "获取指定进程的详细信息".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pid": {
                            "type": "integer",
                            "description": "进程 PID"
                        }
                    },
                    "required": ["pid"]
                }),
            },
            McpTool {
                name: "list_sessions".to_string(),
                description: "列出 Claude Code 会话，支持按项目路径、时间过滤".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "project_path": {
                            "type": "string",
                            "description": "按项目路径过滤（支持部分匹配）"
                        },
                        "days": {
                            "type": "integer",
                            "description": "只返回最近 N 天的会话"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "限制返回数量，默认 20"
                        }
                    },
                    "required": []
                }),
            },
            McpTool {
                name: "get_session_info".to_string(),
                description: "获取指定会话的详细信息".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "会话 ID"
                        }
                    },
                    "required": ["session_id"]
                }),
            },
            McpTool {
                name: "resume_session".to_string(),
                description: "在 tmux 中恢复指定会话".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "会话 ID"
                        },
                        "tmux_name": {
                            "type": "string",
                            "description": "tmux 会话名称 (可选)"
                        }
                    },
                    "required": ["session_id"]
                }),
            },
            McpTool {
                name: "kill_agent".to_string(),
                description: "终止指定的代理进程".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pid": {
                            "type": "integer",
                            "description": "进程 PID"
                        }
                    },
                    "required": ["pid"]
                }),
            },
            McpTool {
                name: "send_input".to_string(),
                description: "向 tmux 会话发送输入".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tmux_session": {
                            "type": "string",
                            "description": "tmux 会话名称"
                        },
                        "input": {
                            "type": "string",
                            "description": "要发送的输入内容"
                        }
                    },
                    "required": ["tmux_session", "input"]
                }),
            },
            // 新增的 agent 管理工具
            McpTool {
                name: "agent_start".to_string(),
                description: "启动新的 Agent 或恢复已有会话".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "project_path": {
                            "type": "string",
                            "description": "项目目录"
                        },
                        "agent_type": {
                            "type": "string",
                            "enum": ["claude", "opencode", "codex"],
                            "description": "Agent 类型，默认 claude"
                        },
                        "resume_session": {
                            "type": "string",
                            "description": "可选，恢复指定会话"
                        },
                        "initial_prompt": {
                            "type": "string",
                            "description": "可选，启动后立即发送的消息"
                        }
                    },
                    "required": ["project_path"]
                }),
            },
            McpTool {
                name: "agent_send".to_string(),
                description: "向指定 Agent 发送输入".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "CAM 分配的 Agent ID"
                        },
                        "input": {
                            "type": "string",
                            "description": "要发送的文本"
                        }
                    },
                    "required": ["agent_id", "input"]
                }),
            },
            McpTool {
                name: "agent_list".to_string(),
                description: "列出所有运行中的 Agent".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            McpTool {
                name: "agent_logs".to_string(),
                description: "获取 Agent 最近的终端输出".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "CAM 分配的 Agent ID"
                        },
                        "lines": {
                            "type": "integer",
                            "description": "返回的行数，默认 50"
                        }
                    },
                    "required": ["agent_id"]
                }),
            },
            McpTool {
                name: "agent_stop".to_string(),
                description: "停止指定 Agent".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "CAM 分配的 Agent ID"
                        }
                    },
                    "required": ["agent_id"]
                }),
            },
            McpTool {
                name: "agent_status".to_string(),
                description: "获取 Agent 的结构化状态信息，包括是否等待输入、最近工具调用、错误等".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "description": "CAM 分配的 Agent ID"
                        }
                    },
                    "required": ["agent_id"]
                }),
            },
            McpTool {
                name: "agent_by_session_id".to_string(),
                description: "通过 Claude Code session_id 查找对应的 CAM Agent".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Claude Code 的 session_id"
                        }
                    },
                    "required": ["session_id"]
                }),
            },
            // Team discovery tools
            McpTool {
                name: "team_list".to_string(),
                description: "列出所有 Claude Code Agent Teams".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            McpTool {
                name: "team_members".to_string(),
                description: "获取指定 Team 的成员列表".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "team_name": {
                            "type": "string",
                            "description": "Team 名称"
                        }
                    },
                    "required": ["team_name"]
                }),
            },
            // Task list tools
            McpTool {
                name: "task_list".to_string(),
                description: "列出指定 Team 的所有任务".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "team_name": {
                            "type": "string",
                            "description": "Team 名称"
                        }
                    },
                    "required": ["team_name"]
                }),
            },
            McpTool {
                name: "task_get".to_string(),
                description: "获取指定任务的详细信息".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "team_name": {
                            "type": "string",
                            "description": "Team 名称"
                        },
                        "task_id": {
                            "type": "string",
                            "description": "任务 ID"
                        }
                    },
                    "required": ["team_name", "task_id"]
                }),
            },
            McpTool {
                name: "task_update".to_string(),
                description: "更新任务状态".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "team_name": {
                            "type": "string",
                            "description": "Team 名称"
                        },
                        "task_id": {
                            "type": "string",
                            "description": "任务 ID"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["pending", "in_progress", "completed", "deleted"],
                            "description": "新状态"
                        }
                    },
                    "required": ["team_name", "task_id", "status"]
                }),
            },
        ];

        Ok(serde_json::json!({ "tools": tools }))
    }

    /// 处理 tools/call
    fn handle_tools_call(&self, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("缺少参数"))?;
        let name = params["name"].as_str().ok_or_else(|| anyhow::anyhow!("缺少工具名称"))?;
        let arguments = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));

        match name {
            "list_agents" => {
                let scanner = ProcessScanner::new();
                let agents = scanner.scan_agents()?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&agents)?
                    }]
                }))
            }
            "get_agent_info" => {
                let pid = arguments["pid"].as_u64().ok_or_else(|| anyhow::anyhow!("缺少 pid"))? as u32;
                let scanner = ProcessScanner::new();
                let agent = scanner.get_agent_info(pid)?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&agent)?
                    }]
                }))
            }
            "list_sessions" => {
                let manager = SessionManager::new();
                let filter = crate::session::SessionFilter {
                    project_path: arguments["project_path"].as_str().map(|s| s.to_string()),
                    days: arguments["days"].as_i64(),
                    limit: Some(arguments["limit"].as_u64().unwrap_or(20) as usize),
                };
                let sessions = manager.list_sessions_filtered(Some(filter))?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&sessions)?
                    }]
                }))
            }
            "get_session_info" => {
                let session_id = arguments["session_id"].as_str().ok_or_else(|| anyhow::anyhow!("缺少 session_id"))?;
                let manager = SessionManager::new();
                let session = manager.get_session(session_id)?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&session)?
                    }]
                }))
            }
            "resume_session" => {
                let session_id = arguments["session_id"].as_str().ok_or_else(|| anyhow::anyhow!("缺少 session_id"))?;

                // 获取会话信息以获取 project_path
                let session_manager = SessionManager::new();
                let session = session_manager.get_session(session_id)?
                    .ok_or_else(|| anyhow::anyhow!("会话 {} 不存在", session_id))?;

                let project_path = if session.project_path.is_empty() {
                    ".".to_string()
                } else {
                    session.project_path
                };

                // 使用 AgentManager 启动，这样会被监控系统追踪
                let response = self.agent_manager.start_agent(StartAgentRequest {
                    project_path,
                    agent_type: Some("claude".to_string()),
                    resume_session: Some(session_id.to_string()),
                    initial_prompt: None,
                })?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("已在 tmux 中恢复会话\nsession_id: {}\nagent_id: {}\ntmux_session: {}\n\n使用 agent_send 工具向此会话发送输入，agent_id 参数填 {}", session_id, response.agent_id, response.tmux_session, response.agent_id)
                    }]
                }))
            }
            "kill_agent" => {
                let pid = arguments["pid"].as_u64().ok_or_else(|| anyhow::anyhow!("缺少 pid"))? as u32;
                let scanner = ProcessScanner::new();
                scanner.kill_agent(pid)?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("已终止进程: {}", pid)
                    }]
                }))
            }
            "send_input" => {
                let tmux_session = arguments["tmux_session"].as_str().ok_or_else(|| anyhow::anyhow!("缺少 tmux_session"))?;
                let input = arguments["input"].as_str().ok_or_else(|| anyhow::anyhow!("缺少 input"))?;
                let manager = SessionManager::new();
                manager.send_to_tmux(tmux_session, input)?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("已向 {} 发送输入", tmux_session)
                    }]
                }))
            }
            // 新增的 agent 工具
            "agent_start" => {
                let result = self.handle_agent_start(Some(arguments))?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }
            "agent_send" => {
                let result = self.handle_agent_send(Some(arguments))?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }
            "agent_list" => {
                let result = self.handle_agent_list()?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }
            "agent_logs" => {
                let result = self.handle_agent_logs(Some(arguments))?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }
            "agent_stop" => {
                let result = self.handle_agent_stop(Some(arguments))?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }
            "agent_status" => {
                let result = self.handle_agent_status(Some(arguments))?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }
            "agent_by_session_id" => {
                let session_id = arguments.get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("缺少 session_id 参数"))?;

                let result = if let Some(agent) = self.agent_manager.find_agent_by_session_id(session_id)? {
                    serde_json::json!({
                        "found": true,
                        "agent_id": agent.agent_id,
                        "tmux_session": agent.tmux_session,
                        "project_path": agent.project_path,
                        "status": agent.status
                    })
                } else {
                    serde_json::json!({
                        "found": false,
                        "session_id": session_id
                    })
                };

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }
            // Team discovery tools
            "team_list" => {
                let result = self.handle_team_list()?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }
            "team_members" => {
                let result = self.handle_team_members(Some(arguments))?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }
            // Task list tools
            "task_list" => {
                let team_name = arguments.get("team_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("缺少 team_name 参数"))?;

                let tasks = task_list::list_tasks(team_name);
                let result: Vec<serde_json::Value> = tasks.iter().map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "subject": t.subject,
                        "status": t.status.to_string(),
                        "owner": t.owner,
                        "blocked_by": t.blocked_by
                    })
                }).collect();

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }
            "task_get" => {
                let team_name = arguments.get("team_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("缺少 team_name 参数"))?;
                let task_id = arguments.get("task_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("缺少 task_id 参数"))?;

                let task = task_list::get_task(team_name, task_id)
                    .ok_or_else(|| anyhow::anyhow!("任务 {} 不存在", task_id))?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&task)?
                    }]
                }))
            }
            "task_update" => {
                let team_name = arguments.get("team_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("缺少 team_name 参数"))?;
                let task_id = arguments.get("task_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("缺少 task_id 参数"))?;
                let status_str = arguments.get("status")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("缺少 status 参数"))?;

                let status = match status_str {
                    "pending" => task_list::TaskStatus::Pending,
                    "in_progress" => task_list::TaskStatus::InProgress,
                    "completed" => task_list::TaskStatus::Completed,
                    "deleted" => task_list::TaskStatus::Deleted,
                    _ => return Err(anyhow::anyhow!("无效的状态: {}", status_str)),
                };

                task_list::update_task_status(team_name, task_id, status)?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("任务 {} 状态已更新为 {}", task_id, status_str)
                    }]
                }))
            }
            _ => Err(anyhow::anyhow!("未知工具: {}", name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup_test_agents(server: &McpServer) {
        if let Ok(agents) = server.agent_manager.list_agents() {
            for agent in agents {
                let _ = server.agent_manager.stop_agent(&agent.agent_id);
            }
        }
    }

    #[tokio::test]
    async fn test_mcp_agent_start() {
        // Given: MCP Server
        let server = McpServer::new_for_test();
        cleanup_test_agents(&server);

        // When: 调用 agent/start
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "agent/start".to_string(),
            params: Some(serde_json::json!({
                "project_path": "/tmp",
                "agent_type": "mock"
            })),
        };
        let response = server.handle_request(request).await;

        // Then: 返回 agent_id
        assert!(response.error.is_none());
        let result = response.result.unwrap();
        assert!(result["agent_id"].is_string());

        // Cleanup
        let agent_id = result["agent_id"].as_str().unwrap();
        server.agent_manager.stop_agent(agent_id).unwrap();
    }

    #[tokio::test]
    async fn test_mcp_agent_send() {
        // Given: 一个运行中的 agent
        let server = McpServer::new_for_test();
        cleanup_test_agents(&server);

        let start_request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "agent/start".to_string(),
            params: Some(serde_json::json!({
                "project_path": "/tmp",
                "agent_type": "mock"
            })),
        };
        let start_response = server.handle_request(start_request).await;
        let agent_id = start_response.result.unwrap()["agent_id"].as_str().unwrap().to_string();

        // When: 调用 agent/send
        let send_request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(2)),
            method: "agent/send".to_string(),
            params: Some(serde_json::json!({
                "agent_id": agent_id,
                "input": "test input"
            })),
        };
        let response = server.handle_request(send_request).await;

        // Then: 返回 success: true
        assert!(response.error.is_none());
        let result = response.result.unwrap();
        assert_eq!(result["success"], true);

        // Cleanup
        server.agent_manager.stop_agent(&agent_id).unwrap();
    }

    #[tokio::test]
    async fn test_mcp_agent_list() {
        // Given: 两个运行中的 agent
        let server = McpServer::new_for_test();
        cleanup_test_agents(&server);

        let r1 = server.handle_request(McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "agent/start".to_string(),
            params: Some(serde_json::json!({
                "project_path": "/tmp/a",
                "agent_type": "mock"
            })),
        }).await;

        // 等待一秒以确保不同的 agent_id
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let r2 = server.handle_request(McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(2)),
            method: "agent/start".to_string(),
            params: Some(serde_json::json!({
                "project_path": "/tmp/b",
                "agent_type": "mock"
            })),
        }).await;

        // When: 调用 agent/list
        let response = server.handle_request(McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(3)),
            method: "agent/list".to_string(),
            params: Some(serde_json::json!({})),
        }).await;

        // Then: 返回两个 agent
        assert!(response.error.is_none());
        let result = response.result.unwrap();
        assert_eq!(result["agents"].as_array().unwrap().len(), 2);

        // Cleanup
        let id1 = r1.result.unwrap()["agent_id"].as_str().unwrap().to_string();
        let id2 = r2.result.unwrap()["agent_id"].as_str().unwrap().to_string();
        server.agent_manager.stop_agent(&id1).unwrap();
        server.agent_manager.stop_agent(&id2).unwrap();
    }

    #[tokio::test]
    async fn test_mcp_invalid_method_returns_error() {
        // Given: MCP Server
        let server = McpServer::new_for_test();

        // When: 调用不存在的方法
        let response = server.handle_request(McpRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "invalid/method".to_string(),
            params: Some(serde_json::json!({})),
        }).await;

        // Then: 返回错误
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601); // Method not found
    }
}
