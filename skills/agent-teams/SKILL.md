---
name: agent-teams
description: "Use when user wants to create, manage, or monitor multi-agent teams — team lifecycle, member spawning, task assignment, inbox messaging, progress tracking, and shutdown. Covers orchestrate, spawn, assign, inbox, and team progress workflows."
---

# Agent Teams — 多 Agent 协作编排

管理多个 AI Agent 组成的协作团队，支持并行工作、任务分配、进度追踪和远程管理。

## When to Use

- 创建和管理多 Agent 团队
- 在 Team 中启动（spawn）新 Agent
- 分配任务给 Team 成员
- 通过 Inbox 发送/读取消息
- 查看 Team 整体进度
- 关闭 Team

## When NOT to Use

- 管理单个 Agent（使用 **cam** skill）
- 通知决策、自动审批、风险判断（使用 **cam-notify** skill）

## 工具清单

所有工具通过 OpenClaw Plugin 层暴露，统一使用 `cam_` 前缀。

### Team 发现

| 工具 | 描述 | 参数 |
|------|------|------|
| `cam_team_list` | 列出所有 Teams | 无 |
| `cam_team_members` | 获取 Team 成员列表 | `team_name` |

### Team 生命周期

| 工具 | 描述 | 参数 |
|------|------|------|
| `cam_team_create` | 创建空 Team | `name`, `description`, `project_path` |
| `cam_team_delete` | 删除 Team 及其资源 | `name` |
| `cam_team_status` | 获取 Team 完整状态（成员、任务、消息） | `name` |
| `cam_team_orchestrate` | 根据任务描述自动创建 Team 并启动 agents | `task_desc`, `project` |

### 成员管理

| 工具 | 描述 | 参数 |
|------|------|------|
| `cam_team_spawn_agent` | 在 Team 中启动新 Agent | `team`, `name`, `agent_type`, `initial_prompt?` |
| `cam_team_progress` | 获取 Team 聚合进度 | `team` |
| `cam_team_shutdown` | 优雅关闭 Team（停止所有 agents） | `team` |
| `cam_team_assign_task` | 分配任务给成员 | `team`, `member`, `task` |

### 任务管理

| 工具 | 描述 | 参数 |
|------|------|------|
| `cam_task_list` | 列出 Team 所有任务 | `team_name` |
| `cam_task_get` | 获取任务详情 | `team_name`, `task_id` |
| `cam_task_update` | 更新任务状态 | `team_name`, `task_id`, `status` |

`cam_task_update` 的 `status` 可选值：`pending`, `in_progress`, `completed`, `deleted`。

### Inbox 通信

| 工具 | 描述 | 参数 |
|------|------|------|
| `cam_inbox_read` | 读取成员 inbox 消息 | `team`, `member` |
| `cam_inbox_send` | 发送消息到成员 inbox | `team`, `member`, `message`, `from?` |
| `cam_team_pending_requests` | 获取等待中的权限请求 | `team?`（不指定则返回所有 Team） |

### 回复管理

| 工具 | 描述 | 参数 |
|------|------|------|
| `cam_get_pending_confirmations` | 获取所有待处理确认 | 无 |
| `cam_reply_pending` | 回复待处理确认 | `reply`, `target?` |
| `cam_handle_user_reply` | 处理自然语言回复（自动解析意图） | `reply`, `context?` |

---

## Team 编排流程

### 自动编排（推荐）

使用 `cam_team_orchestrate` 一步完成创建和启动：

```text
cam_team_orchestrate(
  task_desc: "在 myapp 项目实现用户认证功能",
  project: "/Users/admin/workspace/myapp"
)
```

系统自动分析任务 → 创建 Team → 分配角色 → 启动 Agents。

### 手动编排

分步操作，适合精细控制：

```text
1. team_create(name: "auth-team", description: "认证功能开发", project_path: "/path/to/myapp")
2. team_spawn_agent(team: "auth-team", name: "developer", agent_type: "general-purpose", initial_prompt: "实现 JWT 认证")
3. team_spawn_agent(team: "auth-team", name: "tester", agent_type: "general-purpose", initial_prompt: "为认证模块编写测试")
4. team_assign_task(team: "auth-team", member: "developer", task: "实现登录注册 API")
```

### 角色自动分配规则

`cam_team_orchestrate` 根据任务描述自动分配角色：

| 任务关键词 | 分配角色 |
|-----------|---------|
| 默认 | developer |
| 测试、test | developer + tester |
| 审查、review | developer + reviewer |
| 重构、refactor | developer + reviewer |
| 文档、docs | developer + tech-writer |

---

## 自然语言意图映射

### 创建团队

| 用户说的 | 意图 | 操作 |
|----------|------|------|
| "启动一个团队做 xxx" | 创建团队 | `cam_team_orchestrate` |
| "帮我在 xxx 项目做 yyy" | 创建团队 | `cam_team_orchestrate` |
| "组个团队修复 bug" | 创建团队 | `cam_team_orchestrate` |
| "在 xxx 加个 developer" | 添加成员 | `cam_team_spawn_agent` |

### 查看状态

| 用户说的 | 意图 | 操作 |
|----------|------|------|
| "团队进度怎样" | 查看进度 | `cam_team_progress` |
| "xxx team 在干嘛" | 查看状态 | `cam_team_status` |
| "有什么等着我" | 待处理确认 | `cam_get_pending_confirmations` |
| "看看消息" | 查看 inbox | `cam_inbox_read` |
| "有哪些团队" | 列出团队 | `cam_team_list` |
| "xxx 团队有谁" | 成员列表 | `cam_team_members` |

### 快捷回复

| 用户说的 | 意图 | 操作 |
|----------|------|------|
| "y" / "yes" / "是" / "好" / "可以" / "批准" | 批准 | `cam_reply_pending(reply: "y")` |
| "n" / "no" / "否" / "不" / "取消" / "拒绝" | 拒绝 | `cam_reply_pending(reply: "n")` |
| "1" / "2" / "3" | 选择选项 | `cam_reply_pending(reply: "1")` |

### 任务管理

| 用户说的 | 意图 | 操作 |
|----------|------|------|
| "给 developer 分配 xxx" | 分配任务 | `cam_team_assign_task` |
| "告诉 xxx 去做 yyy" | 分配任务 | `cam_team_assign_task` |
| "看看任务列表" | 查看任务 | `cam_task_list` |
| "xxx 任务完成了" | 更新状态 | `cam_task_update(status: "completed")` |
| "停掉团队" / "关闭 xxx team" | 关闭团队 | `cam_team_shutdown` |

---

## 风险评估

权限请求自动评估风险等级：

| 风险 | 示例 | 显示 |
|------|------|------|
| LOW | `ls`, `cat`, `/tmp` 文件、测试命令 | 安全操作 |
| MEDIUM | `npm install`, `git push`, 项目文件写入 | 请确认 |
| HIGH | `rm -rf`, `sudo`, 系统文件、命令链 | 高风险警告 |

详细的自动审批规则（白名单/黑名单/LLM 判断三层决策模型）见 **cam-notify** skill。

---

## System Event 架构

CAM 检测到 Agent 事件后发送 system event 给 OpenClaw，OpenClaw Agent 决定：
- 是否需要通知用户
- 发送到哪个 thread
- 是否可以自动审批

用户回复后，OpenClaw 调用 CAM MCP 工具执行操作。

### 与 cam-notify 的分工

| Skill | 职责 |
|-------|------|
| agent-teams | Team 编排、成员管理、任务分配、Inbox 通信 |
| cam-notify | System Event 处理、通知格式化、自动审批决策、回复路由 |

---

## 完整示例

### 示例 1：创建团队、权限审批、待处理管理

```text
用户: 帮我在 /Users/admin/workspace/myapp 实现用户认证功能

助手: 好，创建专家团队处理这个任务...

team_orchestrate(task_desc: "实现用户认证功能", project: "/Users/admin/workspace/myapp")

已创建 Team: myapp-8234
  项目: /Users/admin/workspace/myapp
  成员:
  - developer (负责实现)

[几分钟后，收到权限请求通知]

助手: myapp-8234/developer 请求权限
  操作: npm install jsonwebtoken bcrypt
  风险: MEDIUM

  回复 y 允许 / n 拒绝

用户: y

reply_pending(reply: "y")

已发送，developer 继续执行中...

[又收到多个权限请求]

用户: 有什么等着我

get_pending_confirmations()

  2 个待处理确认:

  1. [myapp-8234/developer]
     执行: git push origin main
     风险: MEDIUM

  2. [review-team/tester]
     创建: test/api.test.ts
     风险: LOW

  回复 "y 1" 批准第一个，或 "y" 批准全部

用户: y 1

reply_pending(reply: "y", target: "myapp-8234/developer")

已批准 myapp-8234/developer 的 git push 请求。
```

### 示例 2：多成员协作与进度追踪

```text
用户: 组个团队做代码审查，需要 reviewer 和 tester

助手: 好，创建多成员团队...

team_create(name: "review-team", description: "代码审查", project_path: ".")
team_spawn_agent(team: "review-team", name: "reviewer", agent_type: "general-purpose", initial_prompt: "审查代码质量")
team_spawn_agent(team: "review-team", name: "tester", agent_type: "general-purpose", initial_prompt: "准备测试")

[几分钟后]

用户: 团队进度怎样

team_progress(team: "review-team")

  review-team
  ├── reviewer: 执行中 (context 35%)
  │   最近: 审查 src/api/
  ├── tester: 等待输入
  │   等待: 确认测试范围
  └── 任务: 1/3 完成

用户: 告诉 tester 只测 API 模块

team_assign_task(team: "review-team", member: "tester", task: "只测 API 模块")

已发送任务给 tester，tester 开始执行测试...
```

### 示例 3：查看和管理任务

```text
用户: 看看 review-team 的任务

task_list(team_name: "review-team")

  ID     | 状态        | 负责人   | 描述
  task-1 | completed   | reviewer | 审查 src/api/
  task-2 | in_progress | tester   | 测试 API 模块
  task-3 | pending     | -        | 审查 src/utils/

用户: task-3 已经完成了

task_update(team_name: "review-team", task_id: "task-3", status: "completed")

已更新 task-3 状态为 completed。
```

---

## CLI 命令

完整 CLI 命令参见项目 `CLAUDE.md` 中的 "常用命令 > Team 管理" 部分。

---

## Common Mistakes

| 错误 | 正确做法 |
|------|----------|
| 用 `cam_team_assign_task` 发送自由文本消息 | 自由文本用 `cam_inbox_send`；`cam_team_assign_task` 仅用于正式任务分配 |
| 任务完成后不关闭 Team | 完成后必须 `cam_team_shutdown` 释放 tmux 会话和资源 |
| 直接用 cam 的 `cam_agent_stop` 停 Team 成员 | Team 成员通过 `cam_team_shutdown` 统一关闭 |
| spawn 过多 Agent 不控制并发 | 单机建议不超过 3-5 个并发 Agent |

---

## 最佳实践

1. **任务描述要清晰** — `"帮我在 /path/to/myapp 修复登录页面的表单验证 bug"` 优于 `"帮我改改代码"`
2. **及时响应权限请求** — HIGH 风险操作会阻塞 Agent，及时回复提高效率
3. **使用快捷回复** — `y` 批准 / `n` 拒绝 / `1/2/3` 选择选项
4. **定期检查进度** — `cam_team_progress` 查看团队状态
5. **任务完成后关闭团队** — `cam_team_shutdown` 释放资源
