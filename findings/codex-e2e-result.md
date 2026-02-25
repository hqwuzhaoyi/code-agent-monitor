# Codex E2E 测试报告

测试日期: 2026-02-25
测试环境: macOS Darwin 25.2.0
Codex 版本: codex-cli 0.101.0

## 1. 测试方法

由于 `cam start` 尚未实现，采用手动方式执行 E2E 测试：

1. 手动创建 tmux session: `tmux new-session -d -s test-codex -c /tmp/test-codex-project`
2. 启动 Codex: `codex --no-alt-screen 'brainstorm 创建笔记 web app'`
3. 手动注册 agent 到 `~/.config/code-agent-monitor/agents.json`
4. 使用 `cam watch-trigger --agent-id test-codex` 触发 ReAct 提取器

## 2. 测试结果

### 2.1 Codex 启动

| 步骤 | 结果 | 说明 |
|------|------|------|
| tmux session 创建 | ✅ PASS | `test-codex` session 创建成功 |
| Codex 启动 | ✅ PASS | 使用 `--no-alt-screen` 参数 |
| 信任确认 | ✅ PASS | 需要手动确认目录信任 |
| Prompt 输入 | ✅ PASS | 初始 prompt 需要单独输入 |

### 2.2 Agent 提问

Codex 成功生成选择题：

```
使用技能：brainstorming（先澄清需求）和 terminal-title（标记当前任务）。

我先确认到当前仓库是空的（还没有代码和提交）。第一问：这个笔记 Web App 的首版
目标更接近哪一种？

1. 个人笔记：快速记录、标签、全文搜索
2. 团队协作：共享笔记、权限、评论
3. PKM 导向：双向链接、知识图谱、每日笔记

回复一个数字即可。
```

### 2.3 ReAct 提取器结果

| 指标 | 结果 | 说明 |
|------|------|------|
| AI 状态检测 | ✅ PASS | 返回 `DECISION` |
| 置信度 | 0.9 | 高置信度 |
| 检测耗时 | ~2s | 两次 API 调用 |
| 通知发送 | ✅ PASS | Webhook 成功发送 |
| Urgency | HIGH | 正确识别为需要用户输入 |

### 2.4 日志输出

```
[INFO] Provider succeeded provider=0 model=claude-sonnet-4-5-20250929
[INFO] AI status detection result ai_response=DECISION
[DEBUG] Status detection quality assessment passed confidence=0.9 status=DecisionRequired
[INFO] Webhook notification sent successfully agent_id=Some("test-codex")
[INFO] System event sent to OpenClaw agent_id=test-codex event_type=WaitingForInput urgency="HIGH"
```

### 2.5 去重状态

```json
{
  "test-codex": {
    "first_notified_at": 1771998762,
    "locked_at": 1771998762,
    "content_fingerprint": 16087390289558788195,
    "reminder_sent": false
  }
}
```

## 3. Codex TUI 特点

### 3.1 与 Claude Code 的差异

| 特性 | Codex | Claude Code |
|------|-------|-------------|
| TUI 框架 | ratatui (Rust) | Ink (React/Node) |
| 状态指示 | `Working... Xs (esc to interrupt)` | 文本动画 (Hatching…, Brewing…) |
| 消息分隔 | 水平分隔线 `────────` | 无明确分隔 |
| 选择题格式 | 数字列表 `1. 2. 3.` | 字母列表 `A) B) C)` |
| 输入提示 | `› Write tests for @filename` | `>` |
| 上下文显示 | `96% context left` | 无 |

### 3.2 终端快照特征

Codex 终端快照包含：
- 版本更新提示框
- 模型信息框
- 工具调用记录 (`• Explored`, `• Running parallel reads`)
- 水平分隔线
- 底部输入提示和上下文百分比

## 4. 发现的问题

### 4.1 初始 Prompt 传递 (P2)

**问题**: 使用 `codex --no-alt-screen 'prompt'` 时，prompt 没有被自动执行，需要手动输入。

**原因**: 可能是 shell 引号处理问题，或 Codex 在 `--no-alt-screen` 模式下的行为差异。

**建议**:
- 使用 `codex -C <dir> --no-alt-screen` 启动后，通过 `tmux send-keys` 发送 prompt
- 或调查 Codex 的 prompt 参数在非交互模式下的行为

### 4.2 信任确认 (P3)

**问题**: Codex 启动时需要确认目录信任，这会阻塞自动化流程。

**建议**:
- 使用 `--full-auto` 或配置 `trust_level = "trusted"` 跳过确认
- 或在 `cam start` 实现中自动发送 Enter 确认

## 5. 与 Claude Code 对比

| 测试项 | Claude Code | Codex |
|--------|-------------|-------|
| 状态检测 | ✅ DECISION | ✅ DECISION |
| 置信度 | 0.9 | 0.9 |
| 消息类型 | Choice | Choice |
| 通知发送 | ✅ | ✅ |
| 去重 | ✅ | ✅ |

两者的 ReAct 提取器表现一致，AI 能够正确识别选择题场景。

## 6. 终端快照样本

```
╭───────────────────────────────────────────────────╮
│ >_ OpenAI Codex (v0.101.0)                        │
│                                                   │
│ model:     gpt-5.3-codex xhigh   /model to change │
│ directory: /private/tmp/test-codex-project        │
╰───────────────────────────────────────────────────╯

› brainstorm 创建笔记 web app

• 使用技能：brainstorming（先澄清需求）和 terminal-title（标记当前任务）。

  我先确认到当前仓库是空的（还没有代码和提交）。第一问：这个笔记 Web App 的首版
  目标更接近哪一种？

  1. 个人笔记：快速记录、标签、全文搜索
  2. 团队协作：共享笔记、权限、评论
  3. PKM 导向：双向链接、知识图谱、每日笔记

  回复一个数字即可。

› Write tests for @filename

  ? for shortcuts                                             96% context left
```

## 7. cam start 集成测试

### 7.1 测试命令

```bash
cam start --agent codex --cwd /tmp/test-codex-project2 "brainstorm 创建笔记 web app"
```

### 7.2 测试结果

| 步骤 | 结果 | 说明 |
|------|------|------|
| Session 创建 | ✅ PASS | `cam-1771998961708-0` |
| Agent 注册 | ✅ PASS | 自动注册到 agents.json |
| Codex 启动 | ✅ PASS | 使用 `--no-alt-screen` |
| 更新提示 | ⚠️ 需处理 | 需要手动跳过 |
| Prompt 发送 | ⚠️ 需处理 | 需要手动输入 |
| 权限请求检测 | ✅ PASS | AI 返回 `WAITING` |
| 通知发送 | ✅ PASS | Webhook 成功 |

### 7.3 权限请求场景

Codex 请求执行命令时的终端快照：

```
Would you like to run the following command?

Reason: Do you want me to run the terminal-title script outside the sandbox
so it can write its temp file and fully apply the title?

$ bash /Users/admin/.codex/skills/terminal-title/scripts/set_title.sh
"Brainstorm: Notes Web App"

› 1. Yes, proceed (y)
  2. Yes, and don't ask again for commands that start with `bash /Users/
     admin/.codex/skills/terminal-title/scripts/set_title.sh` (p)
  3. No, and tell Codex what to do differently (esc)

Press enter to confirm or esc to cancel
```

### 7.4 ReAct 提取器输出

```
[INFO] AI status detection result ai_response=WAITING
[INFO] Webhook notification sent successfully agent_id=Some("cam-1771998961708-0")
[INFO] System event sent to OpenClaw agent_id=cam-1771998961708-0 event_type=WaitingForInput urgency="HIGH"
```

AI 正确识别为 `WAITING` 状态（权限请求场景）。

## 8. 结论

### 通过项
- ✅ Codex 在 tmux 中正常运行（使用 `--no-alt-screen`）
- ✅ ReAct 提取器正确检测 Codex 等待输入状态
- ✅ AI 状态检测返回 `DECISION`/`WAITING`，置信度 0.9
- ✅ Webhook 通知成功发送
- ✅ 去重机制正常工作
- ✅ `cam start --agent codex` 基本功能正常

### 待改进
- ⚠️ 初始 prompt 传递需要调整（Codex 更新提示会阻断）
- ⚠️ 信任确认需要自动化处理
- ⚠️ Codex 更新提示需要自动跳过

### 建议
1. `cam start --agent codex` 启动后，检测并自动跳过更新提示
2. 使用 `--full-auto` 或预配置信任级别跳过信任确认
3. Prompt 通过 `tmux send-keys` 在 Codex 完全启动后发送
