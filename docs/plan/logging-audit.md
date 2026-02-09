# CAM 日志覆盖率审查报告

审查日期: 2026-02-09
审查范围: `/Users/admin/workspace/code-agent-monitor/src/` 下所有模块

## 审查标准

- **错误处理路径**: `?` 操作符、`unwrap()`、`expect()`、`match Err(e)` 分支
- **关键操作**: Agent 启动/停止、通知发送、文件 I/O、进程管理
- **外部调用**: HTTP 请求、命令执行 (tmux/openclaw)、文件系统操作
- **状态变化**: Agent 状态转换、Watcher 启动/停止、检测结果

## 优先级定义

- **HIGH**: 影响核心功能、难以调试的问题
- **MEDIUM**: 影响用户体验、可能导致静默失败
- **LOW**: 辅助功能、有其他方式可以排查

---

## 1. agent.rs - Agent 管理模块

### 1.1 Agent 启动
文件: `src/agent.rs`
位置: `start_agent()` 函数 (181-251 行)
当前状态: 无日志
建议: 添加 agent 启动日志，记录 agent_id、project_path、agent_type
优先级: **HIGH**

### 1.2 tmux session 创建
文件: `src/agent.rs`
位置: `start_agent()` 第 193 行 `self.tmux.create_session(...)?`
当前状态: 错误通过 `?` 传播，无日志
建议: 添加 tmux session 创建成功/失败日志
优先级: **HIGH**

### 1.3 初始 prompt 发送
文件: `src/agent.rs`
位置: `start_agent()` 第 215-237 行
当前状态: 只有 `eprintln!` 警告
建议: 添加结构化日志记录 prompt 发送状态
优先级: **MEDIUM**

### 1.4 Agent 停止
文件: `src/agent.rs`
位置: `stop_agent()` 函数 (286-303 行)
当前状态: 无日志
建议: 添加 agent 停止日志，记录 agent_id
优先级: **HIGH**

### 1.5 agents.json 读写
文件: `src/agent.rs`
位置: `read_agents_file()` / `write_agents_file()` (137-153 行)
当前状态: 错误通过 `?` 传播，无日志
建议: 添加文件操作失败日志
优先级: **MEDIUM**

### 1.6 外部会话注册
文件: `src/agent.rs`
位置: `register_external_session()` 函数 (410-440 行)
当前状态: 无日志
建议: 添加外部会话注册日志
优先级: **LOW**

---

## 2. agent_watcher.rs - Agent 监控模块

### 2.1 轮询检测
文件: `src/agent_watcher.rs`
位置: `poll_once()` 函数 (104-195 行)
当前状态: 使用 `eprintln!` 输出调试信息
建议: 替换为结构化日志，支持日志级别控制
优先级: **MEDIUM**

### 2.2 JSONL 解析
文件: `src/agent_watcher.rs`
位置: `poll_once()` 第 136 行 `parser.read_new_events()`
当前状态: 错误被静默忽略 (`if let Ok(...)`)
建议: 添加 JSONL 解析失败日志
优先级: **MEDIUM**

### 2.3 输入等待检测
文件: `src/agent_watcher.rs`
位置: `poll_once()` 第 162-191 行
当前状态: 使用 `eprintln!` 输出检测结果
建议: 替换为结构化日志
优先级: **LOW**

### 2.4 Agent 清理
文件: `src/agent_watcher.rs`
位置: `cleanup_agent()` 函数 (260-264 行)
当前状态: 无日志
建议: 添加 agent 清理日志
优先级: **LOW**

---

## 3. openclaw_notifier.rs - 通知模块

### 3.1 通知发送
文件: `src/openclaw_notifier.rs`
位置: `send_event()` 函数
当前状态: 部分使用 `eprintln!`
建议: 添加完整的通知发送日志（成功/失败/跳过）
优先级: **HIGH**

### 3.2 Channel 检测
文件: `src/openclaw_notifier.rs`
位置: `detect_channel()` 函数
当前状态: 无日志
建议: 添加 channel 检测结果日志
优先级: **MEDIUM**

### 3.3 openclaw 命令执行
文件: `src/openclaw_notifier.rs`
位置: 执行 `openclaw system event` 命令处
当前状态: 错误通过 `?` 传播
建议: 添加命令执行日志，包含 stdout/stderr
优先级: **HIGH**

### 3.4 AI 问题提取
文件: `src/openclaw_notifier.rs`
位置: 调用 `extract_question_with_embedding()` 处
当前状态: 无日志
建议: 添加 AI 提取结果日志（成功/失败/超时）
优先级: **MEDIUM**

---

## 4. tmux.rs - Tmux 操作模块

### 4.1 Session 创建
文件: `src/tmux.rs`
位置: `create_session()` 函数 (15-31 行)
当前状态: 无日志
建议: 添加 session 创建日志
优先级: **HIGH**

### 4.2 Session 终止
文件: `src/tmux.rs`
位置: `kill_session()` 函数 (99-109 行)
当前状态: 无日志
建议: 添加 session 终止日志
优先级: **MEDIUM**

### 4.3 按键发送
文件: `src/tmux.rs`
位置: `send_keys()` / `send_keys_raw()` 函数 (44-78 行)
当前状态: 无日志
建议: 添加按键发送日志（调试级别）
优先级: **LOW**

### 4.4 Pane 捕获
文件: `src/tmux.rs`
位置: `capture_pane()` 函数 (81-96 行)
当前状态: 无日志
建议: 添加捕获失败日志
优先级: **LOW**

---

## 5. watcher_daemon.rs - Watcher Daemon 模块

### 5.1 Daemon 启动
文件: `src/watcher_daemon.rs`
位置: `ensure_started()` 函数 (92-125 行)
当前状态: 无日志
建议: 添加 daemon 启动日志，记录 PID
优先级: **HIGH**

### 5.2 Daemon 停止
文件: `src/watcher_daemon.rs`
位置: `stop()` 函数 (128-140 行)
当前状态: 无日志
建议: 添加 daemon 停止日志
优先级: **MEDIUM**

### 5.3 PID 文件操作
文件: `src/watcher_daemon.rs`
位置: `write_pid()` / `read_pid()` / `remove_pid()` (66-89 行)
当前状态: 错误通过 `?` 传播
建议: 添加 PID 文件操作失败日志
优先级: **LOW**

---

## 6. session.rs - 会话管理模块

### 6.1 会话列表
文件: `src/session.rs`
位置: `list_sessions_filtered()` 函数 (109-173 行)
当前状态: 无日志，错误静默处理
建议: 添加会话扫描日志
优先级: **LOW**

### 6.2 会话恢复
文件: `src/session.rs`
位置: `resume_in_tmux()` 函数 (206-233 行)
当前状态: 无日志
建议: 添加会话恢复日志
优先级: **MEDIUM**

### 6.3 JSONL 解析
文件: `src/session.rs`
位置: `parse_session_logs()` 函数 (316-366 行)
当前状态: 解析错误静默跳过
建议: 添加解析失败日志
优先级: **LOW**

---

## 7. input_detector.rs - 输入检测模块

### 7.1 模式匹配
文件: `src/input_detector.rs`
位置: `detect()` / `detect_immediate()` 函数 (105-180 行)
当前状态: 无日志
建议: 添加调试级别的模式匹配日志
优先级: **LOW**

---

## 8. team_bridge.rs - Team Bridge 模块

### 8.1 Team 创建
文件: `src/team_bridge.rs`
位置: `create_team()` 函数 (153-193 行)
当前状态: 无日志
建议: 添加 team 创建日志
优先级: **MEDIUM**

### 8.2 Team 删除
文件: `src/team_bridge.rs`
位置: `delete_team()` 函数 (196-213 行)
当前状态: 无日志
建议: 添加 team 删除日志
优先级: **MEDIUM**

### 8.3 Inbox 读写
文件: `src/team_bridge.rs`
位置: `send_to_inbox()` / `read_inbox()` 函数 (275-319 行)
当前状态: 无日志
建议: 添加 inbox 操作日志
优先级: **LOW**

### 8.4 成员添加
文件: `src/team_bridge.rs`
位置: `spawn_member()` 函数 (216-272 行)
当前状态: 无日志
建议: 添加成员添加日志
优先级: **MEDIUM**

---

## 9. inbox_watcher.rs - Inbox 监控模块

### 9.1 Team 监控
文件: `src/inbox_watcher.rs`
位置: `watch_team()` / `watch_all_teams()` 函数 (89-111 行)
当前状态: 使用 `eprintln!`
建议: 替换为结构化日志
优先级: **MEDIUM**

### 9.2 消息处理
文件: `src/inbox_watcher.rs`
位置: `process_new_messages()` 函数 (169-225 行)
当前状态: 使用 `eprintln!`
建议: 替换为结构化日志
优先级: **MEDIUM**

### 9.3 通知发送失败
文件: `src/inbox_watcher.rs`
位置: `process_new_messages()` 第 215 行
当前状态: 使用 `eprintln!` 记录错误
建议: 替换为结构化错误日志
优先级: **HIGH**

---

## 10. embedding.rs - Embedding 模块

### 10.1 API 请求
文件: `src/embedding.rs`
位置: `embed()` 函数 (185-229 行)
当前状态: 无日志
建议: 添加 API 请求日志（请求/响应/错误）
优先级: **MEDIUM**

### 10.2 配置加载
文件: `src/embedding.rs`
位置: `from_openclaw_config()` 函数 (116-158 行)
当前状态: 错误通过 `?` 传播
建议: 添加配置加载失败日志
优先级: **LOW**

### 10.3 全局提取器初始化
文件: `src/embedding.rs`
位置: `QuestionExtractor::global()` 函数 (337-349 行)
当前状态: 使用 `eprintln!` 记录初始化失败
建议: 替换为结构化日志
优先级: **LOW**

---

## 11. jsonl_parser.rs - JSONL 解析模块

### 11.1 文件读取
文件: `src/jsonl_parser.rs`
位置: 文件读取操作
当前状态: 使用 `.ok()?` 静默忽略错误
建议: 添加文件读取失败日志
优先级: **MEDIUM**

### 11.2 JSON 解析
文件: `src/jsonl_parser.rs`
位置: `serde_json::from_str(line).ok()?`
当前状态: 解析错误静默忽略
建议: 添加解析失败日志（调试级别）
优先级: **LOW**

---

## 12. process.rs - 进程扫描模块

### 12.1 进程扫描
文件: `src/process.rs`
位置: `scan_agents()` 函数
当前状态: 无日志
建议: 添加进程扫描日志
优先级: **LOW**

### 12.2 进程终止
文件: `src/process.rs`
位置: `kill_agent()` 函数，`process.kill()` 调用
当前状态: 无日志
建议: 添加进程终止日志
优先级: **MEDIUM**

---

## 13. team_orchestrator.rs - Team 编排模块

### 13.1 Agent 启动
文件: `src/team_orchestrator.rs`
位置: `spawn_agent()` 函数
当前状态: 使用 `eprintln!` 记录错误
建议: 替换为结构化日志
优先级: **HIGH**

### 13.2 Team 关闭
文件: `src/team_orchestrator.rs`
位置: `shutdown_team()` 函数
当前状态: 无日志
建议: 添加 team 关闭日志
优先级: **MEDIUM**

---

## 14. conversation_state.rs - 对话状态模块

### 14.1 状态加载/保存
文件: `src/conversation_state.rs`
位置: `load_state()` / `save_state()` 函数 (132-152 行)
当前状态: 错误通过 `?` 传播
建议: 添加状态文件操作日志
优先级: **LOW**

### 14.2 回复发送
文件: `src/conversation_state.rs`
位置: `send_reply_to_agent()` 函数 (286-328 行)
当前状态: 无日志
建议: 添加回复发送日志
优先级: **MEDIUM**

### 14.3 tmux 发送
文件: `src/conversation_state.rs`
位置: `send_to_tmux()` 函数 (331-353 行)
当前状态: 无日志
建议: 添加 tmux 发送日志
优先级: **LOW**

---

## 15. main.rs - CLI 入口

### 15.1 WatchDaemon 命令
文件: `src/main.rs`
位置: `Commands::WatchDaemon` 分支 (345-445 行)
当前状态: 使用 `eprintln!` 输出状态
建议: 替换为结构化日志
优先级: **MEDIUM**

### 15.2 Notify 命令
文件: `src/main.rs`
位置: `Commands::Notify` 分支 (446-642 行)
当前状态: 使用文件日志 (`hook.log`)
建议: 统一使用结构化日志框架
优先级: **LOW**

### 15.3 TeamWatch 命令
文件: `src/main.rs`
位置: `Commands::TeamWatch` 分支 (876-929 行)
当前状态: 使用 `println!` 输出
建议: 替换为结构化日志
优先级: **LOW**

---

## 16. task_list.rs - 任务列表模块

### 16.1 任务列表读取
文件: `src/task_list.rs`
位置: `list_tasks()` 函数 (59-96 行)
当前状态: 错误静默忽略
建议: 添加任务读取失败日志
优先级: **LOW**

### 16.2 任务状态更新
文件: `src/task_list.rs`
位置: `update_task_status()` 函数 (112-129 行)
当前状态: 无日志
建议: 添加任务状态更新日志
优先级: **LOW**

---

## 17. team_discovery.rs - Team 发现模块

### 17.1 Team 发现
文件: `src/team_discovery.rs`
位置: `discover_teams()` 函数 (47-83 行)
当前状态: 无日志，错误静默忽略
建议: 添加 team 发现日志
优先级: **LOW**

### 17.2 配置加载
文件: `src/team_discovery.rs`
位置: `load_team_config()` 函数 (107-116 行)
当前状态: 使用 `.ok()?` 静默忽略错误
建议: 添加配置加载失败日志
优先级: **LOW**

---

## 18. notification_summarizer.rs - 通知摘要模块

### 18.1 风险评估
文件: `src/notification_summarizer.rs`
位置: 风险评估逻辑
当前状态: 无日志（纯计算模块）
建议: 无需添加日志
优先级: **N/A**

---

## 19. mcp.rs - MCP Server 模块

### 19.1 工具调用
文件: `src/mcp.rs`
位置: 各工具处理函数
当前状态: 无日志
建议: 添加工具调用日志
优先级: **MEDIUM**

### 19.2 错误处理
文件: `src/mcp.rs`
位置: 各工具的错误分支
当前状态: 错误返回给调用方，无本地日志
建议: 添加错误日志
优先级: **MEDIUM**

---

## 统计汇总

| 优先级 | 数量 | 说明 |
|--------|------|------|
| HIGH | 10 | 核心功能，必须添加日志 |
| MEDIUM | 22 | 重要功能，建议添加日志 |
| LOW | 18 | 辅助功能，可选添加日志 |
| N/A | 1 | 无需日志 |

## 建议实施顺序

### 第一阶段 (HIGH 优先级)
1. `agent.rs` - Agent 启动/停止日志
2. `openclaw_notifier.rs` - 通知发送日志
3. `tmux.rs` - Session 创建日志
4. `watcher_daemon.rs` - Daemon 启动日志
5. `team_orchestrator.rs` - Agent 启动日志
6. `inbox_watcher.rs` - 通知发送失败日志

### 第二阶段 (MEDIUM 优先级)
1. 替换所有 `eprintln!` 为结构化日志
2. 添加外部命令执行日志
3. 添加文件操作失败日志
4. 添加 API 请求日志

### 第三阶段 (LOW 优先级)
1. 添加调试级别日志
2. 添加辅助功能日志

## 推荐日志框架

建议使用 `tracing` crate：
- 支持结构化日志
- 支持日志级别过滤
- 支持异步日志
- 与 tokio 生态集成良好

```toml
# Cargo.toml
[dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

## 日志格式建议

```rust
// 成功操作
tracing::info!(agent_id = %agent_id, project_path = %project_path, "Agent started");

// 错误处理
tracing::error!(agent_id = %agent_id, error = %e, "Failed to start agent");

// 调试信息
tracing::debug!(session = %session_name, output_len = output.len(), "Captured pane output");
```
