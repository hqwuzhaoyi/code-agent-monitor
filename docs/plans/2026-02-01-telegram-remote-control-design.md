# CAM Telegram 远程控制设计

## 概述

实现通过 Telegram 远程监控和控制 AI 编码代理（Claude Code、OpenCode、Codex）的能力。

**核心场景：** 在手机 Telegram 上收到 Agent 通知，直接回复让 Agent 继续执行。

## 架构

```
┌──────────────────────────────────────────────────────────────┐
│                        你的手机                              │
│                      Telegram App                            │
└──────────────────────┬───────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────────┐
│                      Clawdbot                                │
│  - 收发 Telegram 消息                                        │
│  - 调用 CAM MCP 接口（start/resume/send/list）               │
│  - 转发 CAM 通知到 Telegram                                  │
└──────────────────────┬───────────────────────────────────────┘
                       │ MCP (stdio/SSE)
                       ▼
┌──────────────────────────────────────────────────────────────┐
│                      CAM MCP Server                          │
│  - agent/start: 在 tmux 中启动 Agent                         │
│  - agent/send: 向指定 Agent 发送输入                         │
│  - agent/list: 列出运行中的 Agent                            │
│  - agent/logs: 获取 Agent 最近输出                           │
│  - agent/stop: 停止指定 Agent                                │
└──────────────────────┬───────────────────────────────────────┘
                       │ tmux send-keys / capture-pane
                       ▼
┌──────────────────────────────────────────────────────────────┐
│                    tmux sessions                             │
│  cam-<id>: claude --resume xxx                               │
│  cam-<id>: opencode ...                                      │
└──────────────────────────────────────────────────────────────┘
```

**设计决策：**
- 所有 Agent 通过 CAM 启动，CAM 拥有完整控制权
- 使用 tmux 托管 Agent 进程，成熟稳定，本地也能直接 attach
- Clawdbot 做 Telegram ⟷ MCP 桥接，CAM MCP 是中控

## MCP 接口

### agent/start

启动新的 Agent 或恢复已有会话。

```typescript
// 请求
{
  project_path: string,      // 项目目录
  agent_type?: "claude" | "opencode" | "codex",  // 默认 claude
  resume_session?: string,   // 可选，恢复指定会话
  initial_prompt?: string,   // 可选，启动后立即发送的消息
}

// 响应
{
  agent_id: string,          // CAM 分配的 ID，如 "cam-1706789012"
  tmux_session: string,      // tmux session 名称
}
```

### agent/send

向指定 Agent 发送输入。

```typescript
// 请求
{
  agent_id: string,          // CAM 分配的 ID
  input: string,             // 要发送的文本
}

// 响应
{
  success: boolean,
}
```

### agent/list

列出所有运行中的 Agent。

```typescript
// 请求
{}

// 响应
{
  agents: [{
    agent_id: string,
    agent_type: string,
    project_path: string,
    tmux_session: string,
    status: "running" | "waiting" | "stopped",
  }]
}
```

### agent/logs

获取 Agent 最近的终端输出。

```typescript
// 请求
{
  agent_id: string,
  lines?: number,            // 默认 50
}

// 响应
{
  output: string,            // tmux capture-pane 的内容
}
```

### agent/stop

停止指定 Agent。

```typescript
// 请求
{
  agent_id: string,
}

// 响应
{
  success: boolean,
}
```

## 交互流程

### 场景 1：启动新 Agent

```
1. Telegram: "在 /workspace/myapp 启动 claude"
2. Clawdbot → CAM MCP agent/start { project_path: "/workspace/myapp" }
3. CAM 创建 tmux session "cam-1706789012"，运行 claude
4. CAM → Clawdbot → Telegram: "🚀 已启动 claude (cam-1706789012)"
```

### 场景 2：Agent 等待输入

```
1. CAM watch 检测到 Agent 等待输入
2. CAM → Clawdbot → Telegram: "⏸️ Agent 等待输入:\n[最近输出预览]"
3. Telegram 回复: "y"
4. Clawdbot → CAM MCP agent/send { agent_id, input: "y" }
5. CAM: tmux send-keys -t cam-1706789012 "y" Enter
6. Agent 继续执行
```

### 场景 3：恢复已退出的会话

```
1. CAM watch 检测到 tmux session 结束
2. CAM → Clawdbot → Telegram: "✅ Agent 退出 (cam-1706789012)"
3. Telegram 回复: "恢复"
4. Clawdbot → CAM MCP agent/start { project_path, resume_session: "<session-id>" }
5. CAM 新建 tmux，运行 claude --resume <session-id>
```

## 关键节点检测

CAM Watch 循环（每 2-3 秒）：
1. 检查 tmux session 是否存活 → 退出事件
2. 读取 JSONL 新增行 → 解析工具调用/错误
3. capture-pane 检测等待输入模式

### 通知事件

| 事件 | 检测方式 | 通知内容示例 |
|------|----------|--------------|
| 开始 | agent/start 调用时 | "🚀 启动 claude @ /workspace/myapp" |
| 工具调用 | JSONL 中 `tool_use` 类型 | "🔧 执行: Edit src/main.rs" |
| 错误 | JSONL 中 `error` 或终端红色输出 | "❌ 错误: Permission denied" |
| 等待输入 | 终端出现提示符且无活动 | "⏸️ 等待输入:\n[最近输出]" |
| 完成 | tmux session 结束 | "✅ 完成 (cam-xxx)" |

### 检测机制详解

#### 1. 退出/完成检测

**方法：** 检查 tmux session 是否存活

```bash
tmux has-session -t cam-1706789012
# 返回 0 = 存活，非 0 = 已退出
```

```rust
fn is_session_alive(session_name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
```

#### 2. 工具调用检测

**数据源：** Claude 会话 JSONL 文件

```
~/.claude/projects/-Users-admin-workspace-myapp/<session-id>.jsonl
```

**JSONL 结构示例：**
```json
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"xxx","name":"Edit","input":{"file_path":"src/main.rs"}}]}}
```

**检测逻辑：**
```rust
struct JsonlWatcher {
    path: PathBuf,
    last_offset: u64,  // 上次读取位置
}

impl JsonlWatcher {
    fn poll_new_events(&mut self) -> Vec<JsonlEvent> {
        let file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(self.last_offset))?;

        let mut events = Vec::new();
        for line in BufReader::new(file).lines() {
            if let Ok(msg) = serde_json::from_str::<JsonlMessage>(&line?) {
                if msg.msg_type == "assistant" {
                    for content in msg.message.content {
                        if content.content_type == "tool_use" {
                            events.push(JsonlEvent::ToolUse {
                                tool_name: content.name,
                                input: content.input,
                            });
                        }
                    }
                }
            }
        }

        self.last_offset = file.stream_position()?;
        events
    }
}
```

**session_id 获取：** 启动时解析 claude 输出，提取 session_id 并保存到 agents.json

#### 3. 等待输入检测

**方法：** 终端模式匹配 + 空闲检测

```rust
struct InputWaitDetector {
    last_output: String,
    last_change_time: Instant,
}

impl InputWaitDetector {
    fn is_waiting_for_input(&mut self, session_name: &str) -> Option<String> {
        // 1. 捕获终端内容
        let output = tmux_capture_pane(session_name, 20)?;

        // 2. 检测是否有变化
        let is_idle = if output == self.last_output {
            self.last_change_time.elapsed() > Duration::from_secs(3)
        } else {
            self.last_output = output.clone();
            self.last_change_time = Instant::now();
            false
        };

        if !is_idle {
            return None;
        }

        // 3. 匹配等待输入的模式
        let waiting_patterns = [
            r"^>\s*$",              // Claude 的 > 提示符
            r"\[Y/n\]",             // 确认提示
            r"\[y/N\]",
            r"Press Enter",         // 按回车继续
            r"Continue\?",
            r"proceed\?",
            r": $",                  // 冒号结尾的提示
        ];

        for pattern in waiting_patterns {
            if Regex::new(pattern).unwrap().is_match(&output) {
                return Some(output);
            }
        }

        None
    }
}

fn tmux_capture_pane(session_name: &str, lines: u32) -> Option<String> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-t", session_name, "-p", "-S", &format!("-{}", lines)])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}
```

#### 4. 错误检测

**来源 1：JSONL 中的错误**
```json
{"type":"assistant","message":{"content":[{"type":"text","text":"Error: ENOENT: no such file"}]}}
```

**来源 2：终端输出匹配**
```rust
fn detect_error_in_output(output: &str) -> Option<String> {
    let error_patterns = [
        (r"(?i)error:\s*(.+)", 1),
        (r"(?i)failed:\s*(.+)", 1),
        (r"(?i)permission denied", 0),
        (r"(?i)command not found:\s*(.+)", 1),
        (r"ENOENT:\s*(.+)", 1),
        (r"EACCES:\s*(.+)", 1),
        (r"panic!?\s*(.+)", 1),
    ];

    for (pattern, group) in error_patterns {
        if let Some(caps) = Regex::new(pattern).unwrap().captures(output) {
            return Some(caps.get(group).map(|m| m.as_str()).unwrap_or("Unknown error").to_string());
        }
    }

    None
}
```

### 限流策略

- 工具调用合并：连续多个工具调用合并为一条通知（3 秒窗口内）
- 相同错误去重：同一错误 5 分钟内只通知一次
- 等待输入防抖：检测到等待后，10 秒内不重复通知

## 数据存储

```
~/.claude-monitor/
├── agents.json          # 运行中的 Agent 列表
├── config.json          # 配置（通知目标、轮询间隔等）
└── logs/
    └── cam-<id>.log     # 每个 Agent 的输出日志（可选）
```

### agents.json 结构

```json
{
  "agents": [
    {
      "agent_id": "cam-1706789012",
      "agent_type": "claude",
      "project_path": "/workspace/myapp",
      "tmux_session": "cam-1706789012",
      "session_id": "abc123...",
      "jsonl_path": "~/.claude/projects/-workspace-myapp/abc123.jsonl",
      "jsonl_offset": 12345,
      "last_output_hash": "a1b2c3...",
      "started_at": "2026-02-01T10:00:00Z",
      "status": "running"
    }
  ]
}
```

### CAM 启动恢复

1. 读取 agents.json
2. 检查每个 tmux session 是否存活
3. 清理已失效的记录，恢复监控存活的

## 实现计划 (TDD)

### 实现状态

| 阶段 | 任务 | 状态 | 备注 |
|------|------|------|------|
| P0.1 | tmux 管理模块 | ✅ 完成 | `src/tmux.rs` |
| P0.2 | Agent 管理模块 | ✅ 完成 | `src/agent.rs` |
| P0.3 | MCP Server 接口 | ✅ 完成 | `src/mcp.rs` |
| P0.4 | 端到端集成测试 | ⏳ 待做 | 需要 `tests/e2e.rs` |
| P1.1 | tmux Session 状态监控 | ✅ 完成 | `src/agent_watcher.rs` |
| P1.2 | JSONL 事件解析 | ✅ 完成 | `src/jsonl_parser.rs` |
| P1.3 | 通知限流 | ✅ 完成 | `src/throttle.rs` |
| P1.4 | 输入等待检测 | ✅ 完成 | `src/input_detector.rs` |
| P1.5 | MCP agent/status 端点 | ✅ 完成 | 结构化状态返回 |
| P2.1 | 修复 project_path 匹配 | ⏳ 待做 | `SessionManager::normalize_path()` |

**最后更新:** 2026-02-03
**提交:** a0f4d4e feat: implement P1 real-time monitoring features

---

### P0 - 交互闭环

#### 任务 0.1: tmux 管理模块

**实现：** `src/tmux.rs` - 封装 tmux 操作

**测试用例：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        // Given: 一个不存在的 session 名
        let manager = TmuxManager::new();
        let session_name = "cam-test-001";

        // When: 创建 session 运行 echo 命令
        let result = manager.create_session(session_name, "/tmp", "echo hello");

        // Then: 返回成功，session 存在
        assert!(result.is_ok());
        assert!(manager.session_exists(session_name));

        // Cleanup
        manager.kill_session(session_name).unwrap();
    }

    #[test]
    fn test_send_keys() {
        // Given: 一个运行中的 session
        let manager = TmuxManager::new();
        let session_name = "cam-test-002";
        manager.create_session(session_name, "/tmp", "cat").unwrap();

        // When: 发送输入
        let result = manager.send_keys(session_name, "hello");

        // Then: 返回成功
        assert!(result.is_ok());

        // Cleanup
        manager.kill_session(session_name).unwrap();
    }

    #[test]
    fn test_capture_pane() {
        // Given: 一个有输出的 session
        let manager = TmuxManager::new();
        let session_name = "cam-test-003";
        manager.create_session(session_name, "/tmp", "echo 'test output'").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));

        // When: 捕获输出
        let output = manager.capture_pane(session_name, 50).unwrap();

        // Then: 包含预期内容
        assert!(output.contains("test output"));

        // Cleanup
        manager.kill_session(session_name).unwrap();
    }

    #[test]
    fn test_session_exists_false_for_nonexistent() {
        // Given: 一个不存在的 session 名
        let manager = TmuxManager::new();

        // When/Then: 返回 false
        assert!(!manager.session_exists("nonexistent-session-xyz"));
    }

    #[test]
    fn test_list_sessions() {
        // Given: 创建两个 session
        let manager = TmuxManager::new();
        manager.create_session("cam-test-list-1", "/tmp", "sleep 60").unwrap();
        manager.create_session("cam-test-list-2", "/tmp", "sleep 60").unwrap();

        // When: 列出 cam- 前缀的 session
        let sessions = manager.list_cam_sessions().unwrap();

        // Then: 包含这两个
        assert!(sessions.contains(&"cam-test-list-1".to_string()));
        assert!(sessions.contains(&"cam-test-list-2".to_string()));

        // Cleanup
        manager.kill_session("cam-test-list-1").unwrap();
        manager.kill_session("cam-test-list-2").unwrap();
    }
}
```

**验收标准：**
- [ ] `cargo test tmux` 全部通过
- [ ] 手动验证：`cam tmux-test` 能创建/销毁 session

---

#### 任务 0.2: Agent 管理模块

**实现：** `src/agent.rs` - Agent 生命周期管理

**测试用例：**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_start_agent_creates_tmux_session() {
        // Given: AgentManager 和一个临时目录
        let manager = AgentManager::new();
        let temp_dir = tempdir().unwrap();

        // When: 启动一个 mock agent (用 sleep 代替真实 claude)
        let result = manager.start_agent(StartAgentRequest {
            project_path: temp_dir.path().to_string_lossy().to_string(),
            agent_type: Some("mock".to_string()),  // 测试用
            resume_session: None,
            initial_prompt: None,
        });

        // Then: 返回 agent_id，tmux session 存在
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.agent_id.starts_with("cam-"));
        assert!(manager.tmux.session_exists(&response.tmux_session));

        // Cleanup
        manager.stop_agent(&response.agent_id).unwrap();
    }

    #[test]
    fn test_start_agent_persists_to_agents_json() {
        // Given: AgentManager
        let manager = AgentManager::new();
        let temp_dir = tempdir().unwrap();

        // When: 启动 agent
        let response = manager.start_agent(StartAgentRequest {
            project_path: temp_dir.path().to_string_lossy().to_string(),
            agent_type: Some("mock".to_string()),
            resume_session: None,
            initial_prompt: None,
        }).unwrap();

        // Then: agents.json 包含该记录
        let agents = manager.list_agents().unwrap();
        assert!(agents.iter().any(|a| a.agent_id == response.agent_id));

        // Cleanup
        manager.stop_agent(&response.agent_id).unwrap();
    }

    #[test]
    fn test_stop_agent_kills_tmux_and_removes_record() {
        // Given: 一个运行中的 agent
        let manager = AgentManager::new();
        let temp_dir = tempdir().unwrap();
        let response = manager.start_agent(StartAgentRequest {
            project_path: temp_dir.path().to_string_lossy().to_string(),
            agent_type: Some("mock".to_string()),
            resume_session: None,
            initial_prompt: None,
        }).unwrap();

        // When: 停止 agent
        let result = manager.stop_agent(&response.agent_id);

        // Then: 成功，tmux session 不存在，记录已删除
        assert!(result.is_ok());
        assert!(!manager.tmux.session_exists(&response.tmux_session));
        let agents = manager.list_agents().unwrap();
        assert!(!agents.iter().any(|a| a.agent_id == response.agent_id));
    }

    #[test]
    fn test_send_input_to_agent() {
        // Given: 一个运行 cat 的 agent
        let manager = AgentManager::new();
        let temp_dir = tempdir().unwrap();
        let response = manager.start_agent_with_command(
            temp_dir.path().to_string_lossy().to_string(),
            "cat",  // cat 会 echo 输入
        ).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));

        // When: 发送输入
        let result = manager.send_input(&response.agent_id, "hello world");

        // Then: 成功
        assert!(result.is_ok());

        // Verify: 输出包含发送的内容
        std::thread::sleep(std::time::Duration::from_millis(300));
        let logs = manager.get_logs(&response.agent_id, 50).unwrap();
        assert!(logs.contains("hello world"));

        // Cleanup
        manager.stop_agent(&response.agent_id).unwrap();
    }

    #[test]
    fn test_list_agents_filters_dead_sessions() {
        // Given: 一个已手动 kill 的 tmux session
        let manager = AgentManager::new();
        let temp_dir = tempdir().unwrap();
        let response = manager.start_agent(StartAgentRequest {
            project_path: temp_dir.path().to_string_lossy().to_string(),
            agent_type: Some("mock".to_string()),
            resume_session: None,
            initial_prompt: None,
        }).unwrap();

        // 手动 kill tmux (模拟意外退出)
        manager.tmux.kill_session(&response.tmux_session).unwrap();

        // When: 列出 agents
        let agents = manager.list_agents().unwrap();

        // Then: 不包含已死亡的 agent
        assert!(!agents.iter().any(|a| a.agent_id == response.agent_id));
    }
}
```

**验收标准：**
- [ ] `cargo test agent` 全部通过
- [ ] 手动验证：`cam start /tmp` 能启动 mock agent

---

#### 任务 0.3: MCP Server 接口

**实现：** `src/mcp.rs` - MCP JSON-RPC 处理

**测试用例：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_agent_start() {
        // Given: MCP Server
        let server = McpServer::new_for_test();

        // When: 调用 agent/start
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "agent/start",
            "params": {
                "project_path": "/tmp",
                "agent_type": "mock"
            }
        });
        let response = server.handle_request(&request.to_string()).await;

        // Then: 返回 agent_id
        let result: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert!(result["result"]["agent_id"].is_string());

        // Cleanup
        let agent_id = result["result"]["agent_id"].as_str().unwrap();
        server.agent_manager.stop_agent(agent_id).unwrap();
    }

    #[tokio::test]
    async fn test_mcp_agent_send() {
        // Given: 一个运行中的 agent
        let server = McpServer::new_for_test();
        let start_response = server.handle_request(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "agent/start",
            "params": { "project_path": "/tmp", "agent_type": "mock" }
        }).to_string()).await;
        let start_result: serde_json::Value = serde_json::from_str(&start_response).unwrap();
        let agent_id = start_result["result"]["agent_id"].as_str().unwrap();

        // When: 调用 agent/send
        let response = server.handle_request(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "agent/send",
            "params": { "agent_id": agent_id, "input": "test input" }
        }).to_string()).await;

        // Then: 返回 success: true
        let result: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(result["result"]["success"], true);

        // Cleanup
        server.agent_manager.stop_agent(agent_id).unwrap();
    }

    #[tokio::test]
    async fn test_mcp_agent_list() {
        // Given: 两个运行中的 agent
        let server = McpServer::new_for_test();
        let r1 = server.handle_request(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "agent/start",
            "params": { "project_path": "/tmp/a", "agent_type": "mock" }
        }).to_string()).await;
        let r2 = server.handle_request(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "agent/start",
            "params": { "project_path": "/tmp/b", "agent_type": "mock" }
        }).to_string()).await;

        // When: 调用 agent/list
        let response = server.handle_request(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "agent/list", "params": {}
        }).to_string()).await;

        // Then: 返回两个 agent
        let result: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(result["result"]["agents"].as_array().unwrap().len(), 2);

        // Cleanup
        let id1 = serde_json::from_str::<serde_json::Value>(&r1).unwrap()["result"]["agent_id"].as_str().unwrap().to_string();
        let id2 = serde_json::from_str::<serde_json::Value>(&r2).unwrap()["result"]["agent_id"].as_str().unwrap().to_string();
        server.agent_manager.stop_agent(&id1).unwrap();
        server.agent_manager.stop_agent(&id2).unwrap();
    }

    #[tokio::test]
    async fn test_mcp_invalid_method_returns_error() {
        // Given: MCP Server
        let server = McpServer::new_for_test();

        // When: 调用不存在的方法
        let response = server.handle_request(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "invalid/method",
            "params": {}
        }).to_string()).await;

        // Then: 返回错误
        let result: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert!(result["error"].is_object());
        assert_eq!(result["error"]["code"], -32601); // Method not found
    }
}
```

**验收标准：**
- [ ] `cargo test mcp` 全部通过
- [ ] 手动验证：`echo '{"jsonrpc":"2.0","id":1,"method":"agent/list","params":{}}' | cam serve --stdio` 返回正确 JSON

---

#### 任务 0.4: 端到端集成测试

**实现：** `tests/e2e.rs`

**测试用例：**

```rust
#[tokio::test]
async fn test_e2e_start_send_stop_flow() {
    // Given: CAM MCP Server 运行中
    let server = spawn_cam_server().await;

    // When: 完整流程
    // 1. 启动 agent
    let start_result = server.call("agent/start", json!({
        "project_path": "/tmp/e2e-test",
        "agent_type": "mock"
    })).await;
    let agent_id = start_result["agent_id"].as_str().unwrap();

    // 2. 发送输入
    let send_result = server.call("agent/send", json!({
        "agent_id": agent_id,
        "input": "hello"
    })).await;
    assert_eq!(send_result["success"], true);

    // 3. 获取日志
    let logs_result = server.call("agent/logs", json!({
        "agent_id": agent_id,
        "lines": 10
    })).await;
    assert!(logs_result["output"].as_str().unwrap().contains("hello"));

    // 4. 停止 agent
    let stop_result = server.call("agent/stop", json!({
        "agent_id": agent_id
    })).await;
    assert_eq!(stop_result["success"], true);

    // 5. 确认已停止
    let list_result = server.call("agent/list", json!({})).await;
    assert!(!list_result["agents"].as_array().unwrap()
        .iter().any(|a| a["agent_id"] == agent_id));
}

#[tokio::test]
async fn test_e2e_resume_session() {
    // Given: 一个已存在的 Claude session ID (需要 fixture)
    let server = spawn_cam_server().await;
    let session_id = "test-session-fixture"; // 预置的测试 session

    // When: 恢复会话
    let result = server.call("agent/start", json!({
        "project_path": "/tmp",
        "resume_session": session_id
    })).await;

    // Then: 成功启动
    assert!(result["agent_id"].is_string());

    // Cleanup
    server.call("agent/stop", json!({
        "agent_id": result["agent_id"]
    })).await;
}
```

**验收标准：**
- [ ] `cargo test e2e` 全部通过
- [ ] 手动验证：完整流程 Telegram → Clawdbot → CAM → tmux 可走通

---

### P1 - 进度监控

#### 任务 1.1: tmux Session 状态监控

**实现：** `src/watcher.rs` - 改造现有 Watcher

**测试用例：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_session_exit() {
        // Given: Watcher 监控一个 agent
        let mut watcher = Watcher::new_for_test();
        let agent_id = watcher.agent_manager.start_mock_agent("/tmp").unwrap();
        watcher.add_agent(&agent_id);

        // When: 手动 kill session
        watcher.agent_manager.tmux.kill_session(&agent_id).unwrap();
        let events = watcher.poll_once().unwrap();

        // Then: 检测到退出事件
        assert!(events.iter().any(|e| matches!(e, WatchEvent::AgentExited { .. })));
    }

    #[test]
    fn test_detect_waiting_for_input() {
        // Given: 一个等待输入的 agent (运行 read 命令)
        let mut watcher = Watcher::new_for_test();
        let agent_id = watcher.agent_manager.start_agent_with_command("/tmp", "read -p 'input: '").unwrap();
        watcher.add_agent(&agent_id);
        std::thread::sleep(std::time::Duration::from_millis(500));

        // When: 轮询
        let events = watcher.poll_once().unwrap();

        // Then: 检测到等待输入
        assert!(events.iter().any(|e| matches!(e, WatchEvent::WaitingForInput { .. })));

        // Cleanup
        watcher.agent_manager.stop_agent(&agent_id).unwrap();
    }
}
```

**验收标准：**
- [ ] `cargo test watcher` 全部通过
- [ ] 手动验证：`cam watch` 能检测 agent 退出和等待输入

---

#### 任务 1.2: JSONL 事件解析

**实现：** `src/jsonl_parser.rs` - 解析 Claude JSONL 日志

**测试用例：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_use_event() {
        // Given: 包含 tool_use 的 JSONL 行
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"src/main.rs"}}]}}"#;

        // When: 解析
        let event = JsonlParser::parse_line(line).unwrap();

        // Then: 识别为工具调用
        assert!(matches!(event, JsonlEvent::ToolUse { tool_name, .. } if tool_name == "Edit"));
    }

    #[test]
    fn test_parse_error_event() {
        // Given: 包含 error 的 JSONL 行
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Error: Permission denied"}]}}"#;

        // When: 解析
        let event = JsonlParser::parse_line(line).unwrap();

        // Then: 识别为错误
        assert!(matches!(event, JsonlEvent::Error { .. }));
    }

    #[test]
    fn test_parse_incremental_file() {
        // Given: 一个 JSONL 文件和已读取的位置
        let parser = JsonlParser::new("/path/to/session.jsonl");
        parser.set_position(100); // 从第 100 字节开始

        // When: 读取新增内容
        let events = parser.read_new_events().unwrap();

        // Then: 只返回新增的事件
        // (具体断言取决于测试 fixture)
    }
}
```

**验收标准：**
- [ ] `cargo test jsonl` 全部通过
- [ ] 手动验证：能正确解析真实 Claude session JSONL

---

#### 任务 1.3: 通知限流

**实现：** `src/throttle.rs` - 通知去重和合并

**测试用例：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_consecutive_tool_calls() {
        // Given: 限流器
        let mut throttle = NotifyThrottle::new();

        // When: 连续 3 个工具调用
        throttle.push(NotifyEvent::ToolUse { tool: "Edit".into(), target: "a.rs".into() });
        throttle.push(NotifyEvent::ToolUse { tool: "Edit".into(), target: "b.rs".into() });
        throttle.push(NotifyEvent::ToolUse { tool: "Read".into(), target: "c.rs".into() });
        let events = throttle.flush();

        // Then: 合并为一条
        assert_eq!(events.len(), 1);
        assert!(events[0].message.contains("Edit a.rs, Edit b.rs, Read c.rs"));
    }

    #[test]
    fn test_dedupe_same_error() {
        // Given: 限流器
        let mut throttle = NotifyThrottle::new();

        // When: 同一错误出现两次
        throttle.push(NotifyEvent::Error { message: "Permission denied".into() });
        throttle.push(NotifyEvent::Error { message: "Permission denied".into() });
        let events = throttle.flush();

        // Then: 只有一条
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_error_dedupe_expires_after_5_minutes() {
        // Given: 限流器，5 分钟前的错误
        let mut throttle = NotifyThrottle::new();
        throttle.push_with_time(
            NotifyEvent::Error { message: "Permission denied".into() },
            Instant::now() - Duration::from_secs(301),
        );

        // When: 同一错误再次出现
        throttle.push(NotifyEvent::Error { message: "Permission denied".into() });
        let events = throttle.flush();

        // Then: 两条都发送
        assert_eq!(events.len(), 2);
    }
}
```

**验收标准：**
- [ ] `cargo test throttle` 全部通过

---

### P2 - 消息准确性

#### 任务 2.1: 修复 project_path 匹配

**实现：** 修改 `src/session.rs`

**测试用例：**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_project_path_with_different_formats() {
        // Given: SessionManager
        let manager = SessionManager::new();

        // When: 用不同格式查询同一项目
        let result1 = manager.get_latest_session_by_project("/Users/admin/workspace/myapp");
        let result2 = manager.get_latest_session_by_project("~/-Users-admin-workspace-myapp");
        let result3 = manager.get_latest_session_by_project("/Users/admin/workspace/myapp/");

        // Then: 都能找到同一个 session
        assert_eq!(result1.unwrap().map(|s| s.id), result2.unwrap().map(|s| s.id));
        assert_eq!(result1.unwrap().map(|s| s.id), result3.unwrap().map(|s| s.id));
    }

    #[test]
    fn test_normalize_project_path() {
        // Given: 各种格式的路径
        let paths = vec![
            "/Users/admin/workspace/myapp",
            "/Users/admin/workspace/myapp/",
            "~/-Users-admin-workspace-myapp",
        ];

        // When: 标准化
        let normalized: Vec<_> = paths.iter()
            .map(|p| SessionManager::normalize_path(p))
            .collect();

        // Then: 结果相同
        assert!(normalized.windows(2).all(|w| w[0] == w[1]));
    }
}
```

**验收标准：**
- [ ] `cargo test session` 全部通过
- [ ] 手动验证：退出通知能稳定带上最后消息

---

## 测试运行顺序

```bash
# 1. 单元测试 (快速反馈)
cargo test tmux
cargo test agent
cargo test mcp
cargo test jsonl
cargo test throttle
cargo test session

# 2. 集成测试
cargo test e2e

# 3. 手动验收测试
cam start /tmp --agent-type mock
cam list
cam send <agent_id> "hello"
cam logs <agent_id>
cam stop <agent_id>

# 4. 端到端验收 (需要 Clawdbot)
# 在 Telegram 发送命令，验证完整流程
```
