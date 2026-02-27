---
name: code-agent-monitor
description: "Use when user wants to monitor, start, stop, resume, or manage AI coding agents (Claude Code, OpenCode, Codex). Handles agent lifecycle, session history, tmux session management, and natural language intent routing. For team orchestration see agent-teams skill; for notification decisions see cam-notify skill."
---

# Code Agent Monitor (CAM)

监控和管理系统中所有 AI 编码代理进程。

## 相关 Skills

- **agent-teams** skill - Team 编排、多 Agent 协作、任务分配
- **cam-notify** skill - System Event 处理、AI 决策、回复路由

> 注：新架构下，CAM 只发送 system event，通知决策由 OpenClaw Agent 处理。

## When to Use

- 启动、停止、恢复单个 Agent
- 查看 Agent 状态、日志、输出
- 管理历史会话（列出、恢复）
- 处理单 Agent 的确认回复
- 自然语言意图路由到具体操作

## When NOT to Use

- 多 Agent 团队编排（使用 **agent-teams** skill）
- 通知决策、自动审批、风险判断（使用 **cam-notify** skill）

---

## 工具总览

所有工具通过 OpenClaw Plugin 层暴露，统一使用 `cam_` 前缀。

### Agent 管理工具

核心的 Agent 生命周期管理。

| 工具 | 描述 | 必需参数 | 可选参数 |
|------|------|----------|----------|
| `cam_agent_list` | 列出所有运行中的 Agent | - | - |
| `cam_agent_start` | 启动新 Agent 或恢复会话 | `project_path` | `agent_type` (claude/opencode/codex), `resume_session`, `initial_prompt` |
| `cam_agent_stop` | 停止指定 Agent | `agent_id` | - |
| `cam_agent_send` | 向 Agent 发送输入 | `agent_id`, `input` | - |
| `cam_agent_status` | 获取结构化状态（是否等待输入、工具调用、错误） | `agent_id` | - |
| `cam_agent_logs` | 获取终端输出（注意：百分比是 context 占用率，非任务进度） | `agent_id` | `lines` (默认 50) |
| `cam_agent_by_session_id` | 通过 Claude Code session_id 查找 CAM Agent | `session_id` | - |

### 会话管理工具

历史会话的查询与恢复。

| 工具 | 描述 | 必需参数 | 可选参数 |
|------|------|----------|----------|
| `cam_list_sessions` | 列出历史会话（支持过滤） | - | `project_path`, `days`, `limit` (默认 20) |
| `cam_resume_session` | 在 tmux 中恢复会话 | `session_id` | `tmux_name` |
| `cam_get_session_info` | 获取会话详细信息 | `session_id` | - |

### 进程管理工具（低级）

直接操作进程和 tmux 会话，通常优先使用上面的 Agent 管理工具。

| 工具 | 描述 | 必需参数 | 可选参数 |
|------|------|----------|----------|
| `cam_list_agents` | 列出运行中的代理进程 | - | - |
| `cam_get_agent_info` | 获取进程详情 | `pid` | - |
| `cam_kill_agent` | 终止指定进程 | `pid` | - |
| `cam_send_input` | 向 tmux 会话发送输入 | `tmux_session`, `input` | - |

### 回复管理工具

处理 Agent 的确认请求和用户回复。详见 **cam-notify** skill 了解决策规则。

| 工具 | 描述 | 必需参数 | 可选参数 |
|------|------|----------|----------|
| `cam_get_pending_confirmations` | 获取所有待处理的确认请求 | - | - |
| `cam_reply_pending` | 回复待处理确认（支持 y/n/1/2/3） | `reply` | `target` |
| `cam_handle_user_reply` | 处理自然语言回复（自动解析意图） | `reply` | `context` |

### Team 管理工具（简要）

完整 Team 工具详见 **agent-teams** skill。

| 工具 | 描述 | 关键参数 |
|------|------|----------|
| `cam_team_orchestrate` | 根据任务描述创建团队并启动 Agents | `task_desc`, `project` |
| `cam_team_progress` | 获取团队聚合进度 | `team` |
| `cam_team_shutdown` | 优雅关闭团队 | `team` |

---

## 自然语言意图路由

用户不会说"调用 agent_start"，而是用日常语言。根据以下映射理解意图。

### Agent 管理意图

| 用户可能说的 | 意图 | 对应操作 |
|-------------|------|---------|
| "看看在干嘛" / "有什么任务" / "现在跑着什么" | 查看状态 | `cam_agent_list` |
| "继续" / "接着干" / "go" | 继续执行 | `cam_agent_send` 到当前 agent |
| "y" / "n" / "是" / "否" / "好" / "可以" | 确认/拒绝 | `cam_agent_send` 原样发送 |
| "帮我看看 xxx 项目" / "xxx 项目的会话" | 查找会话 | `cam_list_sessions` + project_path 过滤 |
| "恢复上次的" / "继续之前的" / "接着上次" | 恢复会话 | 找最近会话 + `cam_resume_session` |
| "在 xxx 启动" / "开个新的" | 启动新 agent | `cam_agent_start` |
| "停" / "别干了" / "取消" | 停止 | `cam_agent_stop` |
| "看看输出" / "干了什么" / "进度怎样" | 查看日志 | `cam_agent_logs` 或 `cam_agent_status` |
| "怎么了" / "卡住了吗" / "什么情况" | 诊断状态 | `cam_agent_status` |

### Team 管理意图（简要）

完整的 Team 意图映射详见 **agent-teams** skill。

| 用户可能说的 | 意图 | 对应操作 |
|-------------|------|---------|
| "帮我修复 xxx bug" / "在 xxx 项目做 yyy" | 创建 Team 执行任务 | `cam_team_orchestrate` |
| "team 进度怎样" / "看看 team 在干嘛" | 查看 Team 状态 | `cam_team_progress` |
| "停掉 xxx team" | 关闭团队 | `cam_team_shutdown` |
| "有什么等着我" / "待处理" | 查看待处理请求 | `cam_get_pending_confirmations` |

### 模糊输入处理

当用户输入不明确时：

1. **缺少项目路径**: "启动一个 Claude" -> 询问 "要在哪个项目启动？"
2. **缺少 agent 指向**: "继续" 但有多个 agent -> 列出选项让用户选
3. **缺少会话 ID**: "恢复之前的" -> 列出最近 3 个会话供选择

---

## 上下文管理

### 记住当前状态

在对话过程中，维护以下上下文：

| 上下文 | 用途 | 更新时机 |
|--------|------|----------|
| `current_agent_id` | 用户说"继续"时的默认目标 | 启动/恢复 agent 后更新 |
| `current_project` | 用户说"启动"时的默认项目 | 用户提及项目时更新 |

### 上下文推断规则

1. **单 agent 场景**: 只有一个运行中的 agent 时，所有操作默认指向它
2. **多 agent 场景**: 优先使用最近交互的 agent，不确定时列出选项
3. **无 agent 场景**: 用户说"继续"时，自动查找最近的会话并询问是否恢复

---

## 手动 tmux 操作（调试用）

当需要直接操作 CAM 管理的 tmux 会话时：

```bash
# 列出所有 tmux 会话
tmux list-sessions

# 查看会话终端输出（最近 50 行）
tmux capture-pane -t cam-xxxxxxx -p -S -50

# 发送消息到会话（重要：文本和 Enter 必须分开发送）
tmux send-keys -t cam-xxxxxxx -l "你的消息"
tmux send-keys -t cam-xxxxxxx Enter

# 发送 Ctrl+C 中断当前操作
tmux send-keys -t cam-xxxxxxx C-c
```

**注意**：`tmux send-keys` 发送文本时必须使用 `-l` 标志确保字面解释。文本和回车键必须分成两条命令。

---

## 执行反馈

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

## Common Mistakes

| 错误 | 正确做法 |
|------|----------|
| 混用 `cam_agent_stop` 和 `cam_kill_agent` | `cam_agent_stop` 优雅停止 Agent；`cam_kill_agent` 是低级进程操作，仅调试用 |
| 把 `cam_agent_logs` 中的百分比当成任务进度 | 百分比是 context window 占用率，不是任务完成度 |
| 多 Agent 时不指定目标就发送命令 | 有多个运行中 Agent 时，必须先确认操作目标 |
| 用 cam 工具管理 Team 成员 | Team 成员通过 **agent-teams** skill 的工具管理 |

---

## 完整示例对话

### 示例 1：从零开始一个任务

```
用户: 帮我在 myapp 项目跑个 Claude
助手: 好，启动中...
      已启动 Claude @ ~/workspace/myapp (cam-1706789012)

[几分钟后]
助手: Agent 在等你确认:
      "要创建 src/utils.rs 吗？[Y/n]"

用户: y
助手: 已发送，继续执行中...

[任务完成]
助手: Agent 完成了。要看看做了什么吗？

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
      已恢复 myapp 会话 (cam-1706789099)

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

使用 OpenClaw CLI 测试自然语言交互：

```bash
# 基本测试命令
openclaw agent --agent main --message "你的自然语言输入"

# 测试用例
openclaw agent --agent main --message "看看在干嘛"
openclaw agent --agent main --message "帮我在 myapp 启动个 Claude"
openclaw agent --agent main --message "继续之前的"
openclaw agent --agent main --message "y"
openclaw agent --agent main --message "停掉"
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
