---
name: cam-notify
description: 处理来自 CAM (Code Agent Monitor) 的通知。当收到包含 [CAM] 标记的消息时触发，分析通知内容并决定是否转发到 Telegram 通知用户。用于权限请求、错误、等待输入等 Claude Code agent 状态更新。
---

# CAM 通知处理 Skill

当你收到来自 CAM 的通知消息时（消息包含 `[CAM]` 标记），使用此 skill 处理。

## 通知类型识别

### 单 Agent 通知

| 标记 | 含义 | 紧急程度 |
|------|------|----------|
| 🔐 [CAM] 请求权限 | Agent 需要执行敏感操作（Bash、Write 等） | **高** - 必须转发 |
| ❌ [CAM] 错误 | Agent 遇到错误 | **高** - 必须转发 |
| ⏸️ [CAM] 等待输入 | Agent 完成任务，等待下一步指令 | **中** - 转发 |
| 🚀 [CAM] 已启动 | 新 Agent 启动 | **低** - 可选转发 |
| ✅ [CAM] 已停止 | Agent 退出 | **低** - 可选转发 |

### Team 通知

| 标记 | 含义 | 紧急程度 |
|------|------|----------|
| 🔐 {team}/{member} 请求权限 | Team 成员权限请求 | **高** - 必须转发 + 等待回复 |
| ❌ {team}/{member} 错误 | Team 成员遇到错误 | **高** - 必须转发 + 建议处理 |
| ✅ {team} 任务完成 | Team 完成所有任务 | **中** - 汇总结果转发 |
| ⏸️ {team}/{member} 等待 | Team 成员等待输入 | **中** - 转发 |
| 📊 {team} 进度更新 | Team 阶段性进度 | **低** - 可选转发 |

## 处理流程

### 1. 分析通知

从消息中提取：
- `agent_id`: cam-xxxxxxxx 格式
- `event_type`: permission_request / notification / session_start / stop
- `tool_name`: 如果是权限请求，提取工具名（Bash、Write 等）
- `tool_input`: 具体操作内容

### 2. 决定是否转发

**必须转发到 Telegram：**
- 权限请求（🔐）
- 错误（❌）
- 等待输入且用户可能在等待结果

**可以不转发：**
- 启动/停止通知（除非用户明确在等待）
- 重复通知

### 3. 使用 telegram 工具转发

使用 `sendMessage` action 发送到用户：

```
🔐 Agent {agent_id} 请求权限

工具: {tool_name}
操作: {简短描述}

回复选择:
1 = 允许
2 = 允许并记住
3 = 拒绝
```

## 用户回复处理

当用户回复数字时，使用 `cam_agent_send` 工具：

| 用户回复 | 发送到 Agent |
|----------|-------------|
| 1 | "1" |
| 2 | "2" |
| 3 | "3" |
| y / yes / 允许 | "1" |
| n / no / 拒绝 | "3" |

## 示例

### 收到权限请求

输入：
```
🔐 [CAM] cam-1770282255 请求权限

工具: Bash
目录: /Users/admin/workspace/open
参数:
{
  "command": "mkdir -p /Users/admin/workspace/open/hello-node",
  "description": "Create hello-node project directory"
}

请回复: 1=允许, 2=允许并记住, 3=拒绝
```

处理：
1. 识别为权限请求
2. 使用 telegram sendMessage 转发给用户
3. 等待用户回复

### 用户回复 "1"

处理：
1. 识别用户选择"允许"
2. 调用 `cam_agent_send` 向 `cam-1770282255` 发送 "1"
3. 确认已发送

## 注意事项

- **快速响应** - Agent 在等待，不要让它等太久
- **保持简洁** - 用户在手机上看，消息要短
- **不要吞掉通知** - 权限请求必须让用户知道
- **记住 agent_id** - 用户回复时需要知道发给哪个 agent

## Team 通知处理

### Team 权限请求

当收到 Team 成员的权限请求时：

```
🔐 myapp-fix/developer 请求执行:
git commit -m "fix: login bug"

回复 y 允许，n 拒绝
```

处理：
1. 识别为 Team 权限请求（格式：`{team}/{member}`）
2. 转发到用户
3. 用户回复后，使用 `inbox_send` 发送到对应成员

### Team 任务完成

当 Team 完成所有任务时：

```
✅ myapp-fix 任务完成

- 修复了 session 过期导致的登录失败
- 已提交 commit: fix: login bug
- 建议: 部署到测试环境验证
```

处理：
1. 汇总 Team 完成的工作
2. 提供简洁的结果摘要
3. 给出下一步建议

### Team 错误处理

当 Team 成员遇到错误时：

```
❌ myapp-fix/developer 遇到错误

npm install 失败: EACCES permission denied

建议:
1. 检查目录权限
2. 使用 sudo 重试
3. 停止 Team 手动处理
```

处理：
1. 分析错误类型
2. 提供可能的解决方案
3. 询问用户下一步操作

### Team 回复路由

当用户回复 y/n 时，系统自动路由到等待中的 Team 成员：

| 场景 | 处理 |
|------|------|
| 单个成员等待 | 直接发送到该成员 |
| 多个成员等待 | 列出选项让用户选择 |
| 无成员等待 | 提示"目前没有等待中的请求" |

使用 `team_pending_requests` 获取等待列表，使用 `inbox_send` 发送回复。
