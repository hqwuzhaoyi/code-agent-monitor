---
name: code-agent-monitor
description: 监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)。用于列出运行中的代理、查看会话、恢复任务、发送输入或终止进程。
---

# Code Agent Monitor (CAM)

监控和管理系统中所有 AI 编码代理进程。

## 二进制位置

```
/Users/admin/workspace/code-agent-monitor/target/release/cam
```

## tmux 路径

```
/opt/homebrew/bin/tmux
```

## CLI 命令（推荐）

**重要：直接使用 bash 执行这些命令，不要用 Python！**

### 列出所有代理进程
```bash
/Users/admin/workspace/code-agent-monitor/target/release/cam list
```

### 列出所有会话
```bash
/Users/admin/workspace/code-agent-monitor/target/release/cam sessions
```

### 恢复会话到 tmux
```bash
# 恢复会话，返回 tmux_session 名称
/Users/admin/workspace/code-agent-monitor/target/release/cam resume <SESSION_ID>

# 示例
/Users/admin/workspace/code-agent-monitor/target/release/cam resume 75d6e8ae-56a2-4d9d-8e43-39d79d10faa6
# 输出:
# 已在 tmux 中恢复会话
# tmux_session: cam-75d6e8ae
# 查看输出: /opt/homebrew/bin/tmux attach -t cam-75d6e8ae
```

### 查看会话消息历史
```bash
/Users/admin/workspace/code-agent-monitor/target/release/cam logs <SESSION_ID> --limit 10
```

### 终止进程
```bash
/Users/admin/workspace/code-agent-monitor/target/release/cam kill <PID>
```

### 向 tmux 会话发送输入
```bash
/opt/homebrew/bin/tmux send-keys -t <TMUX_SESSION> "<INPUT>" Enter

# 示例
/opt/homebrew/bin/tmux send-keys -t cam-75d6e8ae "继续之前的任务" Enter
```

### 获取 tmux 会话输出
```bash
/opt/homebrew/bin/tmux capture-pane -t <TMUX_SESSION> -p -S -50
```

### 列出所有 tmux 会话
```bash
/opt/homebrew/bin/tmux ls
```

## MCP 命令（高级）

如需使用 MCP 协议（支持更多过滤参数）：

### 列出会话（带过滤）
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_sessions","arguments":{"days":7,"limit":10}}}' | /Users/admin/workspace/code-agent-monitor/target/release/cam serve 2>/dev/null
```

## 典型工作流

1. **列出会话** -> 获取 session_id
2. **恢复会话** -> 获取 tmux_session 名称（格式：cam-xxxxxxxx）
3. **发送输入** -> 使用 tmux send-keys 发送命令
4. **查看输出** -> tmux capture-pane 获取结果

## MCP Tools 完整参考

### list_sessions
列出 Claude Code 会话，支持过滤。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| project_path | string | 否 | 按项目路径过滤（模糊匹配） |
| days | number | 否 | 只返回最近 N 天的会话 |
| limit | number | 否 | 限制返回的会话数量 |

### resume_session
恢复一个 Claude Code 会话到 tmux。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| session_id | string | 是 | Claude Code 会话 ID |

**返回**: `tmux_session` 名称（格式：cam-xxxxxxxx），用于后续操作。

### send_input
向 tmux 会话发送输入。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| tmux_session | string | 是 | tmux 会话名称（从 resume_session 获取） |
| input | string | 是 | 要发送的输入内容 |

### agent_start
启动一个新的代理实例。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| agent_type | string | 是 | 代理类型 (claude, opencode, codex) |
| project_path | string | 是 | 项目目录路径 |
| prompt | string | 否 | 初始提示词 |

### agent_send
向运行中的代理发送消息。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| agent_id | string | 是 | 代理实例 ID |
| message | string | 是 | 要发送的消息 |

### agent_list
列出所有运行中的代理实例。无参数。

### agent_logs
获取代理的日志输出。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| agent_id | string | 是 | 代理实例 ID |
| lines | number | 否 | 返回的日志行数（默认 50） |

### agent_stop
停止一个代理实例。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| agent_id | string | 是 | 代理实例 ID |

### agent_status
获取代理的结构化状态信息（P1 新功能）。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| agent_id | string | 是 | 代理实例 ID |

**返回字段**:
| 字段 | 类型 | 说明 |
|------|------|------|
| agent_id | string | 代理 ID |
| agent_type | string | 代理类型 (claude/opencode/codex) |
| project_path | string | 项目路径 |
| status | string | 状态 (running/waiting) |
| waiting_for_input | boolean | 是否在等待用户输入 |
| wait_pattern | string | 等待模式 (Confirmation/PermissionRequest 等) |
| wait_context | string | 等待上下文（终端最后几行） |
| recent_tools | array | 最近的工具调用列表 |
| recent_errors | array | 最近的错误列表 |

### list_agents
列出运行中的代理进程。无参数。

### kill_agent
终止一个代理进程。

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| pid | number | 是 | 进程 ID |

## 典型使用场景示例

### 场景 1：列出最近的会话

**用户**: 帮我看看最近有哪些 Claude Code 会话

**助手**: 我来列出最近的会话。

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_sessions","arguments":{"days":7,"limit":10}}}' | /Users/admin/workspace/code-agent-monitor/target/release/cam serve 2>/dev/null
```

输出会显示会话列表，包含 session_id、项目路径、最后活动时间等信息。

### 场景 2：恢复会话并发送消息

**用户**: 恢复会话 75d6e8ae-56a2-4d9d-8e43-39d79d10faa6 并让它继续之前的任务

**助手**:

步骤 1 - 恢复会话：
```bash
/Users/admin/workspace/code-agent-monitor/target/release/cam resume 75d6e8ae-56a2-4d9d-8e43-39d79d10faa6
```

输出：
```
已在 tmux 中恢复会话
tmux_session: cam-75d6e8ae
查看输出: /opt/homebrew/bin/tmux attach -t cam-75d6e8ae
```

步骤 2 - 发送消息：
```bash
/opt/homebrew/bin/tmux send-keys -t cam-75d6e8ae "继续之前的任务" Enter
```

### 场景 3：查看会话输出

**用户**: 看看 cam-abc12345 现在的输出

**助手**:
```bash
/opt/homebrew/bin/tmux capture-pane -t cam-abc12345 -p -S -100
```

这会显示最近 100 行的终端输出。

### 场景 4：按项目过滤会话

**用户**: 列出 code-agent-monitor 项目的会话

**助手**:
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_sessions","arguments":{"project_path":"code-agent-monitor","limit":5}}}' | /Users/admin/workspace/code-agent-monitor/target/release/cam serve 2>/dev/null
```

### 场景 5：启动新代理并监控

**用户**: 在 /Users/admin/my-project 启动一个新的 Claude Code

**助手**:
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"agent_start","arguments":{"agent_type":"claude","project_path":"/Users/admin/my-project"}}}' | /Users/admin/workspace/code-agent-monitor/target/release/cam serve 2>/dev/null
```

然后可以用 agent_list 查看运行状态，用 agent_logs 查看输出。

### 场景 6：获取代理结构化状态（推荐）

**用户**: 查看代理 cam-abc123 的状态

**助手**:
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"agent_status","arguments":{"agent_id":"cam-abc123"}}}' | /Users/admin/workspace/code-agent-monitor/target/release/cam serve 2>/dev/null
```

返回结构化信息：
```json
{
  "agent_id": "cam-abc123",
  "status": "waiting",
  "waiting_for_input": true,
  "wait_pattern": "Confirmation",
  "wait_context": "Continue? [Y/n]",
  "recent_tools": ["Edit main.rs", "Read config.toml"],
  "recent_errors": []
}
```

**优势**：比 `agent_logs` 更精确，直接告诉你代理是否在等待输入、最近执行了什么工具、有无错误。

## 注意事项

1. **始终使用 bash 直接执行命令**，不要用 Python 或其他语言包装
2. tmux 的完整路径是 `/opt/homebrew/bin/tmux`
3. resume_session 返回的 tmux_session 名称格式为 `cam-xxxxxxxx`
4. 发送多行输入时，每行需要单独发送，或使用 `\n` 分隔
5. 查看长输出时，增加 `-S` 参数的值（如 `-S -200` 查看 200 行）

---

## 自然语言理解指南

用户不会说"调用 agent_start"，而是用日常语言。根据以下映射理解意图：

### 意图识别

| 用户可能说的 | 意图 | 对应操作 |
|-------------|------|---------|
| "看看在干嘛" / "有什么任务" / "现在跑着什么" | 查看状态 | agent_list |
| "继续" / "接着干" / "go" | 继续执行 | send_input 到当前 agent |
| "y" / "n" / "是" / "否" / "好" / "可以" | 确认/拒绝 | send_input 原样发送 |
| "帮我看看 xxx 项目" / "xxx 项目的会话" | 查找会话 | list_sessions + project_path 过滤 |
| "恢复上次的" / "继续之前的" / "接着上次" | 恢复会话 | 找最近会话 + resume_session |
| "在 xxx 启动" / "开个新的" | 启动新 agent | agent_start |
| "停" / "别干了" / "取消" | 停止 | agent_stop |
| "看看输出" / "干了什么" / "进度怎样" | 查看日志 | agent_logs 或 agent_status |
| "怎么了" / "卡住了吗" / "什么情况" | 诊断状态 | agent_status |

### 模糊输入处理

当用户输入不明确时：

1. **缺少项目路径**: "启动一个 Claude" → 询问 "要在哪个项目启动？"
2. **缺少 agent 指向**: "继续" 但有多个 agent → 列出选项让用户选
3. **缺少会话 ID**: "恢复之前的" → 列出最近 3 个会话供选择

---

## 上下文管理

### 记住当前状态

在对话过程中，维护以下上下文：

| 上下文 | 用途 | 更新时机 |
|--------|------|----------|
| `current_agent_id` | 用户说"继续"时的默认目标 | 启动/恢复 agent 后更新 |
| `current_project` | 用户说"启动"时的默认项目 | 用户提及项目时更新 |
| `last_notification` | 避免重复响应同一通知 | 收到通知后更新 |

### 上下文推断规则

1. **单 agent 场景**: 只有一个运行中的 agent 时，所有操作默认指向它
2. **多 agent 场景**: 优先使用最近交互的 agent，不确定时列出选项
3. **无 agent 场景**: 用户说"继续"时，自动查找最近的会话并询问是否恢复

### 示例对话

**场景：用户说"继续"**

```
# 情况 1: 有一个运行中的 agent
用户: 继续
助手: [直接 send_input "继续" 到 current_agent]

# 情况 2: 有多个 agent
用户: 继续
助手: 现在有 2 个 agent 在跑:
      1. cam-abc123 (myapp 项目)
      2. cam-def456 (backend 项目)
      继续哪个？

# 情况 3: 没有运行中的 agent
用户: 继续
助手: 目前没有运行中的任务。要恢复最近的会话吗？
      - code-agent-monitor (2小时前)
      - myapp (昨天)
```

---

## 主动行为

### 通知响应

当 CAM 推送通知时，主动告知用户并提供快捷操作：

| 通知类型 | 响应方式 |
|----------|----------|
| 等待输入 | "Agent 在等你确认：[上下文]。回复 y/n 或其他指令" |
| 工具调用 | 静默或简要汇报（根据用户偏好） |
| 错误 | "出错了：[错误信息]。要查看详情吗？" |
| 任务完成 | "任务完成了。要看看结果吗？" |
| Agent 退出 | "Agent 已退出。要恢复继续吗？" |

### 执行反馈

每次操作后简要汇报：

```
# 启动
"已启动 Claude @ myapp 项目 (cam-abc123)"

# 发送输入
"已发送，Agent 继续执行中..."

# 恢复
"已恢复会话，Agent 在跑了"

# 停止
"已停止 cam-abc123"
```

### 异常处理

```
# tmux 会话不存在
"这个 Agent 已经不在了，可能已退出。要查看历史会话吗？"

# 项目路径不存在
"找不到这个项目路径，确认一下？"

# 多个匹配
"找到多个匹配的会话，你要哪个？"
```

---

## 完整示例对话

### 示例 1：从零开始一个任务

```
用户: 帮我在 myapp 项目跑个 Claude
助手: 好，启动中...
      ✅ 已启动 Claude @ /Users/admin/workspace/myapp (cam-1706789012)

[几分钟后，CAM 推送通知]
助手: ⏸️ Agent 在等你确认:
      "要创建 src/utils.rs 吗？[Y/n]"

用户: y
助手: 已发送，继续执行中...

[任务完成]
助手: ✅ Agent 完成了。要看看做了什么吗？

用户: 看看
助手: 最近操作:
      - Edit src/main.rs
      - Write src/utils.rs
      - Edit Cargo.toml
      终端输出: "完成，共修改 3 个文件"
```

### 示例 2：恢复中断的任务

```
用户: 继续之前的
助手: 最近的会话:
      1. myapp - "实现用户认证" (3小时前)
      2. backend - "修复 API bug" (昨天)
      恢复哪个？

用户: 1
助手: 恢复中...
      ✅ 已恢复 myapp 会话 (cam-1706789099)

用户: 接着把登录功能做完
助手: 已发送，Agent 开始干活了
```

### 示例 3：多 Agent 管理

```
用户: 现在跑着什么
助手: 2 个 Agent 在运行:
      1. cam-abc123 | myapp | 运行中
      2. cam-def456 | backend | 等待输入

用户: 2 怎么了
助手: backend 项目的 Agent 在等确认:
      "删除 old_api.rs？[y/N]"

用户: n
助手: 已发送，Agent 继续了

用户: 把 1 停掉
助手: 已停止 cam-abc123 (myapp)
```

---

## 测试方法

使用 Clawdbot CLI 测试自然语言交互：

```bash
# 基本测试命令
clawdbot agent --agent main --message "你的自然语言输入"

# 测试用例
clawdbot agent --agent main --message "看看在干嘛"
clawdbot agent --agent main --message "帮我在 myapp 启动个 Claude"
clawdbot agent --agent main --message "继续之前的"
clawdbot agent --agent main --message "y"
clawdbot agent --agent main --message "停掉"
```

### 测试场景清单

| 场景 | 测试命令 | 预期行为 |
|------|----------|----------|
| 查看状态 | `"现在跑着什么"` | 列出运行中的 agent |
| 启动 agent | `"在 /tmp 启动 Claude"` | 创建新 agent |
| 模糊启动 | `"开个新的"` | 询问项目路径 |
| 发送确认 | `"y"` | 发送到当前 agent |
| 恢复会话 | `"继续之前的"` | 列出最近会话供选择 |
| 查看日志 | `"看看输出"` | 返回最近终端输出 |
| 停止 agent | `"停掉"` | 停止当前 agent |
