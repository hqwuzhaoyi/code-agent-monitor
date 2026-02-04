---
name: code-agent-monitor
description: 监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)。用于列出运行中的代理、查看会话、恢复任务、发送输入或终止进程。
---

# Code Agent Monitor (CAM)

监控和管理系统中所有 AI 编码代理进程。

## 使用方式

CAM 通过 OpenClaw plugin 提供 MCP 工具，直接调用即可：

| 工具 | 描述 |
|------|------|
| `cam_agent_list` | 列出所有运行中的代理 |
| `cam_agent_start` | 启动新代理 |
| `cam_agent_stop` | 停止代理 |
| `cam_agent_send` | 向代理发送消息 |
| `cam_agent_logs` | 获取代理终端输出（注意：显示的百分比是 context 占用率，不是任务进度） |
| `cam_agent_status` | 获取代理结构化状态 |
| `cam_list_sessions` | 列出历史会话 |
| `cam_resume_session` | 恢复历史会话 |

## 手动 tmux 操作（调试用）

当需要直接操作 CAM 管理的 tmux 会话时：

```bash
# 列出所有 tmux 会话
tmux list-sessions

# 查看会话终端输出（最近 50 行）
tmux capture-pane -t cam-xxxxxxx -p -S -50

# 发送消息到会话（重要：文本和 Enter 必须分开发送）
tmux send-keys -t cam-xxxxxxx "你的消息"
tmux send-keys -t cam-xxxxxxx Enter

# 发送 Ctrl+C 中断当前操作
tmux send-keys -t cam-xxxxxxx C-c
```

**注意**：`tmux send-keys` 发送文本和回车键时，必须分成两条命令。

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

### 上下文推断规则

1. **单 agent 场景**: 只有一个运行中的 agent 时，所有操作默认指向它
2. **多 agent 场景**: 优先使用最近交互的 agent，不确定时列出选项
3. **无 agent 场景**: 用户说"继续"时，自动查找最近的会话并询问是否恢复

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

## 完整示例对话

### 示例 1：从零开始一个任务

```
用户: 帮我在 myapp 项目跑个 Claude
助手: 好，启动中...
      ✅ 已启动 Claude @ ~/workspace/myapp (cam-1706789012)

[几分钟后]
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
