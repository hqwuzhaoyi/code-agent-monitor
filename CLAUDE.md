# Code Agent Monitor

## Skills

Skills location: `~/clawd/skills/code-agent-monitor/SKILL.md`

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

### 自动状态通知

CAM 支持自动推送 Agent 状态变化到 clawdbot：

**自动启动**：当第一个 agent 启动时，watcher daemon 自动启动。

**关键事件通知**：
- Agent 退出/完成
- 错误发生
- 等待用户输入（支持中英文模式检测）

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
