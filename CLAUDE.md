# Code Agent Monitor

## Skills

Skills location: `~/clawd/skills/code-agent-monitor/SKILL.md`

## 调试通知系统

### 使用 --dry-run 预览通知

```bash
# 预览 HIGH urgency 通知（直接发送到 channel）
echo '{"cwd": "/workspace"}' | ./target/release/cam notify --event permission_request --agent-id cam-test --dry-run

# 预览 MEDIUM urgency 通知（直接发送到 channel）
echo '{"cwd": "/workspace"}' | ./target/release/cam notify --event stop --agent-id cam-test --dry-run

# 预览 LOW urgency 通知（发给 Agent）
echo '{"cwd": "/workspace"}' | ./target/release/cam notify --event session_start --agent-id cam-test --dry-run
```

输出示例：
```
[DRY-RUN] Would send to channel=telegram target=1440537501
[DRY-RUN] Message: ✅ [CAM] cam-test 已停止
```

### 查看 Hook 日志

```bash
# 查看最近的 hook 触发记录
tail -50 ~/.claude-monitor/hook.log

# 实时监控 hook 日志
tail -f ~/.claude-monitor/hook.log

# 查看特定 agent 的日志
grep "cam-xxxxxxx" ~/.claude-monitor/hook.log
```

### 验证 Channel 检测

```bash
# 检查 OpenClaw channel 配置
cat ~/.openclaw/openclaw.json | jq '.channels'

# 测试 channel 检测是否正常（应显示 telegram/whatsapp 等）
echo '{}' | ./target/release/cam notify --event stop --agent-id test --dry-run 2>&1 | grep "channel="
```

### 常见问题排查

| 问题 | 排查方法 |
|------|---------|
| 通知没有发送 | 检查 `~/.claude-monitor/hook.log` 是否有记录 |
| 发送失败 | 查看 stderr 输出，可能是网络问题或 API 限流 |
| 路由错误 | 使用 `--dry-run` 确认 urgency 分类是否正确 |
| Channel 检测失败 | 检查 `~/.openclaw/openclaw.json` 配置 |

### 手动测试通知发送

```bash
# 测试直接发送到 Telegram（绕过 CAM）
openclaw message send --channel telegram --target <chat_id> --message "test"

# 测试发送给 Agent
openclaw agent --session-id main --message "test"
```

## Testing

### 使用 openclaw agent 直接测试

```bash
# 发送简单消息
openclaw agent --agent main --message "你好"

# 指定 session-id 创建独立会话
openclaw agent --agent main --session-id test-session --message "你好"

# 使用 tui 查看会话历史和状态
openclaw tui --session main --history-limit 10

# 重启 gateway（如果遇到连接问题）
openclaw gateway restart
```

### 使用 CAM plugin 测试 Claude Code 会话

CAM plugin 位置: `plugins/cam/`

```bash
# 安装 plugin（首次）
openclaw plugins install --link /Users/admin/workspace/code-agent-monitor/plugins/cam
openclaw gateway restart

# 通过 openclaw agent 调用 CAM 工具
openclaw agent --agent main --message "使用 cam_agent_start 在 /Users/admin/workspace 启动 Claude Code"
openclaw agent --agent main --message "使用 cam_agent_logs 查看 cam-xxx 的输出"
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：你好"
openclaw agent --agent main --message "使用 cam_agent_list 列出所有运行中的 agent"
```

### CAM Plugin 提供的工具

| 工具 | 描述 |
|------|------|
| `cam_agent_start` | 启动新的 Claude Code agent |
| `cam_agent_stop` | 停止运行中的 agent |
| `cam_agent_list` | 列出 CAM 管理的 agent |
| `cam_agent_send` | 向 agent 发送消息 |
| `cam_agent_status` | 获取 agent 状态 |
| `cam_agent_logs` | 获取 agent 终端输出（注意：显示的百分比如 23% 是 context window 占用率，不是任务进度） |
| `cam_list_sessions` | 列出历史会话 |
| `cam_resume_session` | 恢复历史会话 |

### 手动操作 tmux 会话

当需要直接操作 CAM 管理的 tmux 会话时：

```bash
# 列出所有 tmux 会话
command tmux list-sessions

# 查看会话终端输出（最近 50 行）
command tmux capture-pane -t cam-xxxxxxx -p -S -50

# 发送消息到会话（重要：文本和 Enter 必须分开发送）
command tmux send-keys -t cam-xxxxxxx "你的消息"
command tmux send-keys -t cam-xxxxxxx Enter

# 发送 Ctrl+C 中断当前操作
command tmux send-keys -t cam-xxxxxxx C-c
```

**注意**：`tmux send-keys` 发送文本和回车键时，必须分成两条命令。如果写成 `send-keys "message" Enter` 在一条命令中，Enter 可能被解释为换行符而不是回车键。

### 配置 Claude Code Hooks

为了让 Claude Code 在空闲时自动通知 CAM，需要配置 hooks。

**自动配置**：

```bash
# 获取 CAM plugin 安装路径
CAM_BIN=$(openclaw plugins list --json | jq -r '.[] | select(.name == "cam") | .path')/bin/cam

# 添加 hooks 到 Claude Code 配置（保留现有配置）
jq --arg cam "$CAM_BIN" '.hooks.Notification = [{
  "matcher": "idle_prompt",
  "hooks": [{"type": "command", "command": ($cam + " notify --event idle_prompt --agent-id $SESSION_ID")}]
}]' ~/.claude/settings.json > ~/.claude/settings.json.tmp && mv ~/.claude/settings.json.tmp ~/.claude/settings.json
```

**手动配置**：在 `~/.claude/settings.json` 的 `hooks` 字段添加：

```json
"hooks": {
  "Notification": [{
    "matcher": "idle_prompt",
    "hooks": [{
      "type": "command",
      "command": "<CAM_PLUGIN_PATH>/bin/cam notify --event idle_prompt --agent-id $SESSION_ID"
    }]
  }]
}
```

### 自动状态通知

CAM 支持自动推送 Agent 状态变化到 clawdbot：

**自动启动**：当第一个 agent 启动时，watcher daemon 自动启动。

**关键事件通知**：
- Agent 退出/完成
- 错误发生
- 等待用户输入（支持中英文模式检测）

**通知路由策略**：

| Urgency | 事件类型 | 发送方式 |
|---------|---------|---------|
| **HIGH** | permission_request, Error, WaitingForInput, notification(permission_prompt) | 直接发送到 channel |
| **MEDIUM** | stop, session_end, AgentExited, notification(idle_prompt) | 直接发送到 channel |
| **LOW** | session_start, 其他 notification | 发给 OpenClaw Agent |

Channel 自动从 `~/.openclaw/openclaw.json` 检测，按优先级：telegram > whatsapp > discord > slack > signal

**通知流程**：
1. Watcher 检测到事件
2. 通过 `openclaw agent --session-id main` 发送到 clawdbot
3. clawdbot 询问用户如何处理
4. 用户确认后，clawdbot 调用 `cam_agent_send` 执行

**手动控制 watcher**：
```bash
# 查看 watcher 状态
cat ~/.claude-monitor/watcher.pid

# 手动停止 watcher
kill $(cat ~/.claude-monitor/watcher.pid)
```

### 测试自动通知场景

以下场景用于测试 watcher daemon 是否能正确检测事件并推送通知。

#### 场景 1: 测试等待输入检测（确认提示）

```bash
# 1. 启动一个会产生确认提示的 agent
openclaw agent --agent main --message "使用 cam_agent_start 在 /tmp 启动 Claude Code，初始 prompt 为：请帮我删除 /tmp/test-file.txt 文件"

# 2. 等待 agent 运行，Claude Code 会询问是否确认删除
# 预期：watcher 检测到 [Y/n] 提示后，clawdbot 应收到通知

# 3. 查看 watcher 是否在运行
cat ~/.claude-monitor/watcher.pid && echo "Watcher PID: $(cat ~/.claude-monitor/watcher.pid)"
```

#### 场景 2: 测试中文等待输入检测

```bash
# 1. 创建一个模拟中文确认提示的测试脚本
echo '#!/bin/bash
echo "正在准备..."
sleep 2
read -p "是否继续？[是/否] " choice
echo "你选择了: $choice"
' > /tmp/test-chinese-prompt.sh
chmod +x /tmp/test-chinese-prompt.sh

# 2. 使用 CAM 启动这个脚本（使用 mock agent 类型或直接 tmux）
# 通过 cam CLI 直接测试
./target/release/cam watch-daemon -i 2 &

# 3. 在另一个终端运行脚本，观察是否检测到中文提示
```

#### 场景 3: 测试 Agent 退出通知

```bash
# 1. 启动一个简单任务的 agent
openclaw agent --agent main --message "使用 cam_agent_start 在 /tmp 启动 Claude Code，初始 prompt 为：echo hello 然后退出"

# 2. 等待 agent 完成任务并退出
# 预期：agent 退出后，clawdbot 应收到 ✅ Agent 已退出 通知

# 3. 验证 watcher 在所有 agent 退出后自动停止
sleep 10
cat ~/.claude-monitor/watcher.pid 2>/dev/null || echo "Watcher 已停止（PID 文件不存在）"
```

#### 场景 4: 测试错误通知

```bash
# 1. 模拟一个会产生错误的操作
openclaw agent --agent main --message "使用 cam_agent_start 在 /nonexistent/path 启动 Claude Code"

# 预期：由于路径不存在，应该收到错误通知
```

#### 场景 5: 手动触发 notify 命令测试

```bash
# 直接测试 notify 子命令是否能发送通知到 clawdbot
echo "测试错误信息" | ./target/release/cam notify --event Error --agent-id cam-test-123

# 测试等待输入事件
echo "Continue? [Y/n]" | ./target/release/cam notify --event WaitingForInput --agent-id cam-test-456
```

#### 场景 6: 完整流程测试

```bash
# 1. 确保没有残留的 watcher
kill $(cat ~/.claude-monitor/watcher.pid) 2>/dev/null

# 2. 启动 agent（应自动启动 watcher）
openclaw agent --agent main --message "使用 cam_agent_start 在 /Users/admin/workspace 启动 Claude Code"

# 3. 验证 watcher 已启动
sleep 2
ps aux | grep "cam watch-daemon" | grep -v grep

# 4. 查看 agent 列表
openclaw agent --agent main --message "使用 cam_agent_list 列出所有 agent"

# 5. 给 agent 发送一个会触发确认的任务
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：请创建一个新文件 /tmp/test.txt"

# 6. 等待并观察 clawdbot 是否收到等待输入通知
# 7. 停止 agent
openclaw agent --agent main --message "使用 cam_agent_stop 停止 cam-xxx"

# 8. 验证 watcher 自动停止（所有 agent 退出后）
sleep 5
cat ~/.claude-monitor/watcher.pid 2>/dev/null || echo "Watcher 已自动停止"
```

### 真实 Claude Code 确认场景

以下是 Claude Code 实际会产生的确认提示场景，可用于端到端测试：

#### 场景 A: 文件写入确认

```bash
# Claude Code 在写入新文件时会询问确认
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：创建文件 /tmp/new-component.tsx，内容为一个简单的 React 组件"

# 预期 Claude Code 输出类似：
# ╭──────────────────────────────────────────╮
# │ Write to /tmp/new-component.tsx?         │
# │ [Y]es / [N]o / [A]lways / [D]on't ask    │
# ╰──────────────────────────────────────────╯
```

#### 场景 B: Bash 命令执行确认

```bash
# Claude Code 执行 bash 命令时会询问确认
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：运行 npm install 安装依赖"

# 预期 Claude Code 输出类似：
# ╭──────────────────────────────────────────╮
# │ Run bash command?                        │
# │ npm install                              │
# │ [Y]es / [N]o / [A]lways / [D]on't ask    │
# ╰──────────────────────────────────────────╯
```

#### 场景 C: 文件编辑确认

```bash
# Claude Code 编辑现有文件时会显示 diff 并询问确认
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：在 package.json 中添加一个新的 script"

# 预期 Claude Code 输出类似：
# ╭──────────────────────────────────────────╮
# │ Apply changes to package.json?           │
# │ [Y]es / [N]o / [A]lways / [D]on't ask    │
# ╰──────────────────────────────────────────╯
```

#### 场景 D: 文件删除确认

```bash
# Claude Code 删除文件时会询问确认
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：删除 /tmp/old-file.txt"

# 预期 Claude Code 输出类似：
# ╭──────────────────────────────────────────╮
# │ Delete /tmp/old-file.txt?                │
# │ [Y]es / [N]o                             │
# ╰──────────────────────────────────────────╯
```

#### 场景 E: Git 操作确认

```bash
# Claude Code 执行 git 操作时会询问确认
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：提交当前的修改，commit message 为 fix: update config"

# 预期 Claude Code 输出类似：
# ╭──────────────────────────────────────────╮
# │ Run bash command?                        │
# │ git commit -m "fix: update config"       │
# │ [Y]es / [N]o / [A]lways / [D]on't ask    │
# ╰──────────────────────────────────────────╯
```

#### 场景 F: MCP 工具调用确认

```bash
# Claude Code 调用 MCP 工具时可能询问确认
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：使用浏览器打开 https://example.com"

# 预期 Claude Code 输出类似：
# ╭──────────────────────────────────────────╮
# │ Allow mcp__browser__navigate?            │
# │ [Y]es / [N]o / [A]lways / [D]on't ask    │
# ╰──────────────────────────────────────────╯
```

#### 检测到的模式汇总

| 模式 | 示例 | 类型 |
|------|------|------|
| `[Y]es / [N]o` | Write to file? | Confirmation |
| `[Y/n]` | Continue? [Y/n] | Confirmation |
| `[y/N]` | Delete file? [y/N] | Confirmation |
| `[是/否]` | 是否继续？[是/否] | Confirmation |
| `确认？` | 确认删除？ | Confirmation |
| `>\s*$` | Claude Code 主提示符 | ClaudePrompt |
| `:\s*$` | 请输入文件名: | ColonPrompt |
| `allow this action` | Do you want to allow this action? | PermissionRequest |
| `是否授权` | 是否授权此操作？ | PermissionRequest |

## 开发指南

### 项目结构

```
src/
├── main.rs              # CLI 入口，处理 notify/watch 等命令
├── openclaw_notifier.rs # 通知系统核心（urgency 分类、channel 检测、消息格式化）
├── agent.rs             # Agent 管理（启动、停止、列表）
├── tmux.rs              # Tmux 会话操作
├── input_detector.rs    # 终端输入模式检测
├── session.rs           # Claude Code 会话管理
└── mcp.rs               # MCP Server 实现
```

### 运行测试

```bash
# 运行所有通知系统测试
cargo test --lib openclaw_notifier

# 运行特定测试
cargo test --lib test_get_urgency

# 运行所有测试（包括需要 tmux 的集成测试）
cargo test --lib
```

### 构建

```bash
# Debug 构建
cargo build

# Release 构建
cargo build --release

# 构建后二进制位置
./target/release/cam
```

### 添加新事件类型

1. 在 `get_urgency()` 中添加 urgency 分类
2. 在 `format_event()` 中添加消息格式化
3. 在 `main.rs` 的 `needs_snapshot` 中决定是否需要终端快照
4. 添加对应的单元测试

### 通知系统架构

```
Claude Code Hook
       │
       ▼
  cam notify
       │
       ▼
┌──────────────────┐
│ OpenclawNotifier │
├──────────────────┤
│ 1. 解析 context  │
│ 2. 判断 urgency  │
│ 3. 格式化消息    │
│ 4. 路由发送      │
└──────────────────┘
       │
       ├─── HIGH/MEDIUM ──▶ openclaw message send (直接到 channel)
       │
       └─── LOW ──────────▶ openclaw agent (发给 Agent)
```
