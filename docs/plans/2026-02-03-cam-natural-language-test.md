# CAM 自然语言交互测试计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 验证 Clawdbot 能正确理解自然语言并调用 CAM 完成所有交互场景

**Architecture:** 使用 `clawdbot agent --agent main --message "xxx"` 测试每个场景，验证 Clawdbot 是否正确调用 CAM MCP 工具

**Tech Stack:** Clawdbot CLI, CAM MCP Server, tmux

---

## 代码修复记录

### 修复 1: CLI resume 命令未注册到 AgentManager

**文件:** `src/main.rs`

**问题:** `cam resume` 命令直接调用 `SessionManager::resume_in_tmux`，恢复的会话不会被 AgentManager 追踪

**修复:** 改为使用 `AgentManager::start_agent` with `resume_session` 参数

### 修复 2: MCP resume_session 工具未注册到 AgentManager

**文件:** `src/mcp.rs`

**问题:** `tools/call` 的 `resume_session` 工具调用 `SessionManager::resume_in_tmux`，恢复的会话不会被监控

**修复:** 改为使用 `AgentManager::start_agent` with `resume_session` 参数，返回 `agent_id` 供后续操作

---

## 前置准备

### Task 0: 环境准备

**Step 1: 确认 CAM 已编译**

```bash
ls -la /Users/admin/workspace/code-agent-monitor/target/release/cam
```

Expected: 文件存在

**Step 2: 确认 clawdbot 可用**

```bash
which clawdbot
```

Expected: 返回 clawdbot 路径

**Step 3: 清理现有 agent（干净环境测试）**

```bash
/opt/homebrew/bin/tmux kill-server 2>/dev/null || true
rm -f ~/.claude-monitor/agents.json
```

Expected: 无输出或 "no server running"

---

## 场景 1: 查看状态（无 agent）

### Task 1.1: 空状态查询

**测试命令:**
```bash
clawdbot agent --agent main --message "现在跑着什么"
```

**预期行为:**
- Clawdbot 调用 `agent_list`
- 返回 "目前没有运行中的任务" 或类似表述

**验证点:**
- [ ] 不报错
- [ ] 正确识别为"查看状态"意图
- [ ] 返回空列表的友好提示

---

## 场景 2: 启动 Agent

### Task 2.1: 明确项目路径启动

**测试命令:**
```bash
clawdbot agent --agent main --message "在 /tmp 启动 Claude"
```

**预期行为:**
- Clawdbot 调用 `agent_start` with `project_path="/tmp"`
- 返回 agent_id 和确认消息

**验证点:**
- [ ] 创建了 tmux session（`tmux ls` 可见 cam-xxx）
- [ ] agents.json 有记录
- [ ] 返回类似 "已启动 Claude @ /tmp (cam-xxx)"

**Step 2: 验证 agent 创建**

```bash
/opt/homebrew/bin/tmux ls
cat ~/.claude-monitor/agents.json
```

### Task 2.2: 模糊启动（无项目路径）

**测试命令:**
```bash
clawdbot agent --agent main --message "开个新的"
```

**预期行为:**
- Clawdbot 识别为启动意图
- 因缺少 project_path，询问 "要在哪个项目启动？"

**验证点:**
- [ ] 不直接执行
- [ ] 主动询问缺失参数

---

## 场景 3: 查看状态（有 agent）

### Task 3.1: 列出运行中的 agent

**前置:** 确保 Task 2.1 创建的 agent 还在运行

**测试命令:**
```bash
clawdbot agent --agent main --message "看看在干嘛"
```

**预期行为:**
- 调用 `agent_list`
- 返回 agent 列表，包含 agent_id、项目路径、状态

**验证点:**
- [ ] 列出了 Task 2.1 创建的 agent
- [ ] 显示格式清晰（agent_id, project, status）

---

## 场景 4: 发送输入

### Task 4.1: 发送确认 (y)

**前置:** 有运行中的 agent

**测试命令:**
```bash
clawdbot agent --agent main --message "y"
```

**预期行为:**
- 识别为确认意图
- 调用 `agent_send` 发送 "y" 到当前 agent

**验证点:**
- [ ] 返回 "已发送" 确认
- [ ] tmux capture-pane 能看到 "y" 被发送

**Step 2: 验证输入已发送**

```bash
/opt/homebrew/bin/tmux capture-pane -t $(cat ~/.claude-monitor/agents.json | jq -r '.agents[0].tmux_session') -p -S -10
```

### Task 4.2: 发送拒绝 (n)

**测试命令:**
```bash
clawdbot agent --agent main --message "n"
```

**预期行为:** 同上，发送 "n"

### Task 4.3: 发送自然语言确认

**测试命令:**
```bash
clawdbot agent --agent main --message "好的，继续"
```

**预期行为:**
- 识别为继续意图
- 发送输入到当前 agent

---

## 场景 5: 查看日志

### Task 5.1: 查看输出

**测试命令:**
```bash
clawdbot agent --agent main --message "看看输出"
```

**预期行为:**
- 调用 `agent_logs` 或 `agent_status`
- 返回最近的终端输出

**验证点:**
- [ ] 返回了终端内容
- [ ] 内容可读

### Task 5.2: 诊断状态

**测试命令:**
```bash
clawdbot agent --agent main --message "什么情况"
```

**预期行为:**
- 调用 `agent_status`
- 返回结构化状态（是否等待输入、最近工具调用等）

---

## 场景 6: 停止 Agent

### Task 6.1: 停止当前 agent

**测试命令:**
```bash
clawdbot agent --agent main --message "停掉"
```

**预期行为:**
- 调用 `agent_stop`
- 终止 tmux session
- 从 agents.json 移除

**验证点:**
- [ ] 返回 "已停止 cam-xxx"
- [ ] `tmux ls` 不再显示该 session
- [ ] agents.json 为空

**Step 2: 验证 agent 已停止**

```bash
/opt/homebrew/bin/tmux ls 2>&1 || echo "no sessions"
cat ~/.claude-monitor/agents.json
```

---

## 场景 7: 恢复会话

### Task 7.1: 恢复最近会话

**前置:** 有历史 Claude 会话（~/.claude/projects/ 下有 sessions-index.json）

**测试命令:**
```bash
clawdbot agent --agent main --message "继续之前的"
```

**预期行为:**
- 调用 `list_sessions` 获取最近会话
- 列出 2-3 个供选择
- 询问 "恢复哪个？"

**验证点:**
- [ ] 列出了历史会话
- [ ] 显示项目名和时间

### Task 7.2: 选择并恢复

**测试命令:**
```bash
clawdbot agent --agent main --message "1"
```

**预期行为:**
- 调用 `resume_session` 恢复第一个
- 创建新的 tmux session
- 返回确认

**验证点:**
- [ ] tmux session 创建
- [ ] agents.json 有新记录

---

## 场景 8: 多 Agent 管理

### Task 8.1: 启动第二个 agent

**测试命令:**
```bash
clawdbot agent --agent main --message "再启动一个在 /var/tmp"
```

**预期行为:**
- 创建第二个 agent

### Task 8.2: 列出多个 agent

**测试命令:**
```bash
clawdbot agent --agent main --message "现在有几个在跑"
```

**预期行为:**
- 列出 2 个 agent
- 编号显示

### Task 8.3: 指定 agent 操作

**测试命令:**
```bash
clawdbot agent --agent main --message "看看 1 的输出"
```

**预期行为:**
- 识别 "1" 指向第一个 agent
- 返回该 agent 的日志

### Task 8.4: 停止指定 agent

**测试命令:**
```bash
clawdbot agent --agent main --message "把 2 停了"
```

**预期行为:**
- 停止第二个 agent
- 第一个继续运行

---

## 场景 9: 异常处理

### Task 9.1: 操作不存在的 agent

**前置:** 清理所有 agent

**测试命令:**
```bash
clawdbot agent --agent main --message "看看输出"
```

**预期行为:**
- 检测到没有运行中的 agent
- 友好提示 "目前没有运行中的任务"

### Task 9.2: 无效项目路径

**测试命令:**
```bash
clawdbot agent --agent main --message "在 /nonexistent/path 启动"
```

**预期行为:**
- 尝试启动
- 返回错误或询问确认

---

## 测试结果汇总

| 场景 | 测试项 | 通过 | 备注 |
|------|--------|------|------|
| 1 | 空状态查询 | [x] | 正确识别意图，返回进程列表 |
| 2.1 | 明确路径启动 | [~] | tmux session 创建成功，但未使用 CAM MCP 工具 |
| 2.2 | 模糊启动 | [ ] | 未询问缺失参数，直接使用 /tmp |
| 3.1 | 列出 agent | [x] | 正确显示 agent 状态和等待输入提示 |
| 4.1 | 发送 y | [x] | 正确识别确认意图并发送输入 |
| 4.2 | 发送 n | [-] | 未测试 |
| 4.3 | 自然语言确认 | [~] | 识别为查看状态而非发送确认 |
| 5.1 | 查看输出 | [~] | 主动发送确认而非仅显示输出 |
| 5.2 | 诊断状态 | [x] | 正确诊断状态 |
| 6.1 | 停止 agent | [x] | 正确停止 tmux session |
| 7.1 | 恢复会话列表 | [~] | 显示运行中 session 而非历史会话 |
| 7.2 | 选择恢复 | [x] | 正确显示选中 session 状态 |
| 8.1 | 启动第二个 | [x] | 成功创建第二个 agent |
| 8.2 | 列出多个 | [x] | 正确列出多个 agent |
| 8.3 | 指定 agent 查看 | [x] | 正确识别编号并显示输出 |
| 8.4 | 指定 agent 停止 | [x] | 正确停止指定 agent |
| 9.1 | 无 agent 时操作 | [~] | 显示系统进程而非提示无 agent |
| 9.2 | 无效路径 | [~] | 警告但仍尝试，未验证结果 |

**图例:** [x] 通过 | [~] 部分通过 | [ ] 失败 | [-] 未测试

**关键发现:**
1. Clawdbot 未使用 CAM MCP 工具，而是直接操作 tmux
2. agents.json 始终为空，说明 CAM 的 AgentManager 未被调用
3. 自然语言理解基本正确，但部分场景过于主动（如自动发送确认）

---

## 清理

测试完成后清理环境：

```bash
/opt/homebrew/bin/tmux kill-server 2>/dev/null || true
rm -f ~/.claude-monitor/agents.json
```
