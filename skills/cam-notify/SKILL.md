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
| `agent_exited` | Agent 退出 | MEDIUM（正常）/ HIGH（异常）|
| `error` | 错误发生 | HIGH |
| `session_start` | 会话启动 | LOW |
| `session_end` | 会话结束 | LOW |

### Risk Levels

| risk_level | 描述 | 示例 |
|------------|------|------|
| LOW | 安全操作 | `ls`, `cat`, `/tmp/` 路径 |
| MEDIUM | 需确认 | `npm install`, `git push`, 项目文件 |
| HIGH | 高风险 | `rm -rf`, `sudo`, 系统文件 |

## 三层决策模型

```
命令/确认请求
    ↓
┌─────────────────────────────────────┐
│ 第一层：白名单 → 直接批准            │
└─────────────────────────────────────┘
    ↓ 不在白名单
┌─────────────────────────────────────┐
│ 第二层：黑名单 → 必须人工确认        │
└─────────────────────────────────────┘
    ↓ 不在黑名单
┌─────────────────────────────────────┐
│ 第三层：LLM 判断 → 智能决策          │
└─────────────────────────────────────┘
```

### 第一层：白名单（直接批准）

```
# 只读命令
git status, git diff, git log
ls, pwd, which, cat, head, tail

# 测试命令
cargo test, cargo check, cargo clippy
npm test, npm run lint, npm run build
yarn test, pytest, go test, tsc --noEmit
```

**⚠️ 参数安全检查**：即使命令在白名单，如果参数包含以下敏感路径，仍需人工确认：

```
/etc/, ~/.ssh/, ~/.aws/, ~/.config/
.env, credentials, secret, token, password, id_rsa
```

**示例**：
- `cat README.md` → ✅ 自动批准
- `cat /etc/passwd` → ⚠️ 人工确认（敏感路径）
- `ls ~/.ssh/` → ⚠️ 人工确认（敏感路径）

### 第二层：黑名单（必须人工确认）

```
# 删除类命令
rm, rmdir, delete, drop, truncate

# 决策类提示
包含 "brainstorm", "选择方案", "which approach", "你想要" 的提示

# 生产/部署相关
deploy, push --force, production, release

# 命令链和重定向
包含 &&, ||, ;, |, >, >>, <, $(), `` 的命令

# 环境变量展开
包含 $VAR 形式的变量引用
```

### 第三层：LLM 判断

不在白名单也不在黑名单的命令，分析风险后决策：

- LOW: 只读操作、不影响系统状态、可逆操作、/tmp/ 路径 → 自动批准
- MEDIUM: 写入操作但影响范围有限、项目内文件 → 自动批准并通知
- HIGH: 删除、覆盖、不可逆、影响生产、敏感路径 → 人工确认

## AgentExited 处理

区分正常退出和异常退出：

| 退出类型 | 判断条件 | Urgency | 行为 |
|----------|----------|---------|------|
| 正常完成 | exit code 0 | LOW | 静默或简短通知 |
| 异常退出 | exit code != 0 | HIGH | 立即通知用户 |
| 超时退出 | 超过配置时间 | MEDIUM | 通知用户 |

## 通知聚合（Swarm 场景）

分层聚合，根据 urgency 级别使用不同窗口：

| Urgency | 聚合窗口 | 行为 |
|---------|----------|------|
| HIGH | 不聚合 | 立即发送 |
| MEDIUM | 30 秒 | 合并同类通知 |
| LOW | 5 分钟 | 合并或静默 |

**聚合格式**：
```
✅ [refactor-team] 已自动批准 5 个操作:
  - git status (dev1, dev2, dev3)
  - cargo check (dev1, dev2)
```

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

### 自动批准通知

```
✅ [{agent_id}] 已自动批准: {command}
```

## 用户回复处理

当用户回复时，调用 CAM MCP 接口：

| 用户回复 | 操作 |
|----------|------|
| y / yes / 允许 | `cam_agent_send(agent_id, "y")` |
| n / no / 拒绝 | `cam_agent_send(agent_id, "n")` |
| 1 / 2 / 3 | `cam_agent_send(agent_id, "1")` 等 |
| 其他文本 | `cam_agent_send(agent_id, 用户输入)` |

### 批量回复

支持批量操作：

| 命令 | 说明 |
|------|------|
| `cam reply y --all` | 批准所有待处理请求 |
| `cam reply y --agent cam-*` | 批准指定 agent 的请求 |
| `cam reply y --risk low` | 批准所有 LOW 风险请求 |

### Team 回复路由

如果 agent_id 包含 team 信息（如 `team-xxx/member`）：
- 使用 `inbox_send(team, member, reply)` 发送回复

## 重复确认机制

首次人工批准的命令，5 分钟内相同命令自动批准：

- 命令必须**完全相等**（包括所有参数）
- 检测到命令链符号时不自动批准
- 状态存储在 OpenClaw 会话中（会话结束清空）
