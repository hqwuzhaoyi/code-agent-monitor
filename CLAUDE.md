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
