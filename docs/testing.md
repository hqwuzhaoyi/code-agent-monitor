# CAM 测试指南

## 使用 openclaw agent 直接测试

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

## 使用 CAM plugin 测试 Claude Code 会话

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

## CAM Plugin 提供的工具

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

## 手动操作 tmux 会话

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

## 测试自动通知场景

以下场景用于测试 watcher daemon 是否能正确检测事件并推送通知。

### 场景 1: 测试等待输入检测（确认提示）

```bash
# 1. 启动一个会产生确认提示的 agent
openclaw agent --agent main --message "使用 cam_agent_start 在 /tmp 启动 Claude Code，初始 prompt 为：请帮我删除 /tmp/test-file.txt 文件"

# 2. 等待 agent 运行，Claude Code 会询问是否确认删除
# 预期：watcher 检测到 [Y/n] 提示后，clawdbot 应收到通知

# 3. 查看 watcher 是否在运行
cat ~/.claude-monitor/watcher.pid && echo "Watcher PID: $(cat ~/.claude-monitor/watcher.pid)"
```

### 场景 2: 测试中文等待输入检测

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
./target/release/cam watch-daemon -i 2 &

# 3. 在另一个终端运行脚本，观察是否检测到中文提示
```

### 场景 3: 测试 Agent 退出通知

```bash
# 1. 启动一个简单任务的 agent
openclaw agent --agent main --message "使用 cam_agent_start 在 /tmp 启动 Claude Code，初始 prompt 为：echo hello 然后退出"

# 2. 等待 agent 完成任务并退出
# 预期：agent 退出后，clawdbot 应收到 ✅ Agent 已退出 通知

# 3. 验证 watcher 在所有 agent 退出后自动停止
sleep 10
cat ~/.claude-monitor/watcher.pid 2>/dev/null || echo "Watcher 已停止（PID 文件不存在）"
```

### 场景 4: 测试错误通知

```bash
# 1. 模拟一个会产生错误的操作
openclaw agent --agent main --message "使用 cam_agent_start 在 /nonexistent/path 启动 Claude Code"

# 预期：由于路径不存在，应该收到错误通知
```

### 场景 5: 手动触发 notify 命令测试

```bash
# 直接测试 notify 子命令是否能发送通知到 clawdbot
echo "测试错误信息" | ./target/release/cam notify --event Error --agent-id cam-test-123

# 测试等待输入事件
echo "Continue? [Y/n]" | ./target/release/cam notify --event WaitingForInput --agent-id cam-test-456
```

### 场景 6: 完整流程测试

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

## 真实 Claude Code 确认场景

以下是 Claude Code 实际会产生的确认提示场景，可用于端到端测试：

### 场景 A: 文件写入确认

```bash
# Claude Code 在写入新文件时会询问确认
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：创建文件 /tmp/new-component.tsx，内容为一个简单的 React 组件"

# 预期 Claude Code 输出类似：
# ╭──────────────────────────────────────────╮
# │ Write to /tmp/new-component.tsx?         │
# │ [Y]es / [N]o / [A]lways / [D]on't ask    │
# ╰──────────────────────────────────────────╯
```

### 场景 B: Bash 命令执行确认

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

### 场景 C: 文件编辑确认

```bash
# Claude Code 编辑现有文件时会显示 diff 并询问确认
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：在 package.json 中添加一个新的 script"

# 预期 Claude Code 输出类似：
# ╭──────────────────────────────────────────╮
# │ Apply changes to package.json?           │
# │ [Y]es / [N]o / [A]lways / [D]on't ask    │
# ╰──────────────────────────────────────────╯
```

### 场景 D: 文件删除确认

```bash
# Claude Code 删除文件时会询问确认
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：删除 /tmp/old-file.txt"

# 预期 Claude Code 输出类似：
# ╭──────────────────────────────────────────╮
# │ Delete /tmp/old-file.txt?                │
# │ [Y]es / [N]o                             │
# ╰──────────────────────────────────────────╯
```

### 场景 E: Git 操作确认

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

### 场景 F: MCP 工具调用确认

```bash
# Claude Code 调用 MCP 工具时可能询问确认
openclaw agent --agent main --message "使用 cam_agent_send 向 cam-xxx 发送：使用浏览器打开 https://example.com"

# 预期 Claude Code 输出类似：
# ╭──────────────────────────────────────────╮
# │ Allow mcp__browser__navigate?            │
# │ [Y]es / [N]o / [A]lways / [D]on't ask    │
# ╰──────────────────────────────────────────╯
```

## 检测到的模式汇总

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

## 端到端测试通知流程（通过 Hook 触发）

完整测试从 Claude Code Hook 到 Telegram 的通知流程：

```bash
# 1. 确保有运行中的 CAM agent
cat ~/.claude-monitor/agents.json | jq '.agents[].agent_id'

# 2. 查看 agent 当前终端状态
command tmux capture-pane -t <agent_id> -p -S -30

# 3. 手动触发 idle_prompt hook（模拟 Claude Code 空闲）
echo '{"notification_type": "idle_prompt", "cwd": "/Users/admin/workspace"}' | \
  ./target/release/cam notify --event notification --agent-id <agent_id>

# 4. 查看完整日志（包含终端快照）
tail -100 ~/.claude-monitor/hook.log

# 5. 使用 dry-run 预览通知内容（不实际发送）
echo '{"notification_type": "idle_prompt", "cwd": "/Users/admin/workspace"}' | \
  ./target/release/cam notify --event notification --agent-id <agent_id> --dry-run
```

日志格式说明：
```
[时间] Hook triggered: event=notification, agent_id=cam-xxx, session_id=None
[时间] Context: {"notification_type": "idle_prompt", "cwd": "..."}
[时间] Terminal snapshot (1234 chars):
  ... 终端内容 ...
[时间] ⏱️ pattern_match open_question took 0ms
[时间] ⏱️ format_event notification took 4ms
[时间] ⏱️ send_direct telegram took 8645ms
[时间] ✅ Notification sent: notification cam-xxx
```

## 手动测试通知发送

```bash
# 测试直接发送到 Telegram（绕过 CAM）
openclaw message send --channel telegram --target <chat_id> --message "test"

# 测试发送给 Agent
openclaw agent --session-id main --message "test"
```

## 端到端测试检查清单

完整测试 CAM 通知链路时，按以下清单逐项验证：

| 环节 | 检查命令 | 预期结果 |
|------|---------|---------|
| Agent 注册 | `cat ~/.claude-monitor/agents.json \| jq '.agents[].agent_id'` | 显示 `cam-xxx` |
| Watcher 运行 | `ps aux \| grep "cam watch-daemon" \| grep -v grep` | 进程存在 |
| Hook 触发 | `tail ~/.claude-monitor/hook.log` | 显示事件记录 |
| Urgency 分类 | dry-run 输出 | HIGH/MEDIUM/LOW 正确 |
| Dashboard payload | dry-run 输出 | JSON 格式正确，包含 terminal_snapshot |
| Telegram 消息 | dry-run 输出 | 包含问题和选项 |
| 网络连接 | `tail ~/.openclaw/logs/gateway.err.log` | 无 fetch failed |
| 消息发送 | `cam_agent_send` | Agent 收到并响应 |

### 快速验证脚本

```bash
#!/bin/bash
# 保存为 test-cam-e2e.sh

echo "=== CAM E2E Test ==="

# 1. 检查 agents
echo -n "1. Agents: "
cat ~/.claude-monitor/agents.json 2>/dev/null | jq -r '.agents[].agent_id' | head -1 || echo "None"

# 2. 检查 watcher
echo -n "2. Watcher: "
if ps aux | grep -q "[c]am watch-daemon"; then echo "Running"; else echo "Not running"; fi

# 3. 检查 gateway
echo -n "3. Gateway: "
openclaw gateway status 2>/dev/null | grep -q "running" && echo "Running" || echo "Not running"

# 4. 检查网络错误
echo -n "4. Network errors: "
grep -c "fetch failed" ~/.openclaw/logs/gateway.err.log 2>/dev/null || echo "0"

# 5. 最近 hook 事件
echo "5. Recent hooks:"
tail -5 ~/.claude-monitor/hook.log 2>/dev/null | grep -E "Hook triggered|Notification" || echo "None"
```

## 已知问题

### 1. 通知内容不完整（待修复）

**问题**：当 Claude Code 显示多行选项时，通知只包含最后几行，选项被截断。

**症状**：
```
# 期望
⏸️ workspace 等待选择
1. 选项一
2. 选项二
3. 选项三
回复数字选择

# 实际
⏸️ workspace 等待输入
这部分看起来对吗？
回复内容
```

**原因**：终端快照行数不足，`terminal_cleaner` 提取的内容被截断。

**临时解决**：增加 `main.rs` 中 `get_logs()` 的行数到 50+。

**跟踪**：`docs/fix-notification-content-loss.md`

### 2. 网络连接失败

**问题**：Telegram API 请求失败，gateway.err.log 显示 `fetch failed`。

**原因**：VPN/网络环境问题。

**解决**：
```bash
# 检查网络
curl -I https://api.telegram.org

# 重启 gateway
openclaw gateway restart
```

### 3. Agent 注册为 ext-xxx

**问题**：通过 OpenClaw 启动的 agent 被注册为外部会话。

**原因**：CAM 的 `cam_agent_start` 返回错误，OpenClaw Agent 用 `exec` 绕过 CAM。

**解决**：
```bash
# 重新构建并更新插件
cargo build --release
cp target/release/cam plugins/cam/bin/cam
openclaw gateway restart
```

**跟踪**：见 CLAUDE.md 中的「CAM 插件错误排查」

## Brainstorming 测试场景

测试多轮选项交互：

```bash
# 1. 启动 brainstorming agent
openclaw agent --agent main --message "在 /tmp/test-project 使用 claude code 用 brainstorm 创建一个 react 项目"

# 2. 等待出现选项问题

# 3. 检查通知内容
echo '{"notification_type": "idle_prompt", "cwd": "/tmp/test-project"}' | \
  ./target/release/cam notify --event notification --agent-id <agent_id> --dry-run

# 4. 验证选项是否完整显示

# 5. 发送回复
openclaw agent --agent main --message "使用 cam_agent_send 向 <agent_id> 发送：1"

# 6. 重复 3-5 直到 brainstorming 完成

# 7. 清理
openclaw agent --agent main --message "使用 cam_agent_stop 停止 <agent_id>"
```
