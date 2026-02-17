---
name: cam-notify
description: 处理 CAM System Event 并做出通知决策。当收到 CAM 发送的 system event 时，决定是否通知用户、发送到哪个 thread、是否自动审批。
---

# CAM System Event 处理指南

当你收到来自 CAM 的 system event 时，使用此 skill 处理。

## System Event 数据结构

CAM 发送的 system event 格式：

```json
{
  "source": "cam",
  "version": "1.0",
  "agent_id": "cam-xxx",
  "event_type": "permission_request",
  "urgency": "HIGH",
  "project_path": "/path/to/project",
  "timestamp": "2026-02-18T10:00:00Z",
  "event_data": {
    "tool_name": "Bash",
    "tool_input": {"command": "npm install express"}
  },
  "context": {
    "terminal_snapshot": "...",
    "risk_level": "MEDIUM"
  }
}
```

### Event Types

| event_type | 描述 | 典型 urgency |
|------------|------|-------------|
| `permission_request` | 权限请求（工具执行确认） | HIGH |
| `waiting_for_input` | 等待用户输入 | HIGH |
| `notification` | 一般通知 | MEDIUM/LOW |
| `agent_exited` | Agent 退出 | MEDIUM |
| `error` | 错误发生 | HIGH |
| `session_start` | 会话启动 | LOW |
| `session_end` | 会话结束 | LOW |

### Risk Levels

| risk_level | 描述 | 示例 |
|------------|------|------|
| LOW | 安全操作 | `ls`, `cat`, `/tmp/` 路径 |
| MEDIUM | 需确认 | `npm install`, `git push`, 项目文件 |
| HIGH | 高风险 | `rm -rf`, `sudo`, 系统文件 |

## AI 决策指南

### 1. 是否需要通知用户

**必须通知：**
- `urgency: HIGH` 的所有事件
- `permission_request` 类型
- `error` 类型

**可以自动处理：**
- `risk_level: LOW` 且 `urgency: MEDIUM` 的权限请求
- `session_start` / `session_end` 事件

**静默处理：**
- `urgency: LOW` 的事件

### 2. Thread 选择策略

根据项目或 agent 选择 thread：
- 同一项目的事件发送到同一 thread
- 或按紧急程度分 thread（高优先级单独 thread）

### 3. 自动审批规则

以下情况可以自动批准（回复 "y"）：
- `risk_level: LOW` 的 Bash 命令（如 `ls`, `cat`, `echo`）
- `/tmp/` 或 `node_modules/` 路径的文件操作
- 只读操作（Read 工具）

以下情况必须用户确认：
- `risk_level: HIGH` 的任何操作
- 涉及 `.env`, `.ssh/`, `/etc/` 的操作
- `rm`, `sudo`, `chmod` 命令

## 消息格式化建议

### 权限请求

```
⚠️ {project_name} 请求权限

执行: {tool_name}
{tool_input 简短描述}

风险: {risk_level_emoji}

回复 y 允许 / n 拒绝
```

### 错误通知

```
❌ {project_name} 遇到错误

{error_message}

回复查看详情或处理建议
```

### 等待输入

```
⏸️ {project_name} 等待输入

{question}

{options if any}

回复选择或输入内容
```

## 用户回复处理

当用户回复时，调用 CAM MCP 接口：

| 用户回复 | 操作 |
|----------|------|
| y / yes / 允许 | `cam_agent_send(agent_id, "y")` |
| n / no / 拒绝 | `cam_agent_send(agent_id, "n")` |
| 1 / 2 / 3 | `cam_agent_send(agent_id, "1")` 等 |
| 其他文本 | `cam_agent_send(agent_id, 用户输入)` |

### Team 回复路由

如果 agent_id 包含 team 信息（如 `team-xxx/member`）：
- 使用 `inbox_send(team, member, reply)` 发送回复
