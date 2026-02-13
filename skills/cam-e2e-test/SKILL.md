# CAM 端到端测试 Skill

端到端测试 CAM 通知系统的完整链路。

## 触发条件

- 用户说 "测试 CAM"、"测试通知系统"、"e2e 测试"
- 用户说 "验证通知链路"、"检查通知是否正常"

## 测试流程

### 1. 环境检查

```bash
# 检查 CAM 二进制
./target/release/cam --version || cargo build --release

# 检查 OpenClaw gateway
openclaw gateway status

# 检查 channel 配置
cat ~/.openclaw/openclaw.json | jq '.channels'
```

### 2. 启动测试 Agent

```bash
# 通过 OpenClaw 启动 Claude Code agent
openclaw agent --agent main --message "在 /tmp/cam-test 使用 claude code 创建一个简单项目"

# 或直接使用 CAM
./target/release/cam agent-start --cwd /tmp/cam-test --prompt "echo hello"
```

### 3. 验证 Agent 注册

```bash
# 检查 agents.json
cat ~/.config/code-agent-monitor/agents.json | jq '.agents[] | {agent_id, cwd, status}'

# 检查 watcher 是否运行
cat ~/.config/code-agent-monitor/watcher.pid && ps aux | grep "cam watch-daemon" | grep -v grep
```

### 4. 监控通知链路

```bash
# 实时监控 hook 日志
tail -f ~/.config/code-agent-monitor/hook.log

# 检查 gateway 日志
tail -f ~/.openclaw/logs/gateway.log | grep -E "system event|notification"

# 检查 gateway 错误
tail -f ~/.openclaw/logs/gateway.err.log
```

### 5. 测试通知内容（Dry-run）

```bash
# 测试 idle_prompt 通知
echo '{"notification_type": "idle_prompt", "cwd": "/tmp/cam-test"}' | \
  ./target/release/cam notify --event notification --agent-id <agent_id> --dry-run

# 测试 permission_request 通知
echo '{"tool_name": "Bash", "tool_input": {"command": "npm install"}, "cwd": "/tmp"}' | \
  ./target/release/cam notify --event permission_request --agent-id <agent_id> --dry-run
```

### 6. 测试消息发送

```bash
# 通过 OpenClaw 发送消息
openclaw agent --agent main --message "使用 cam_agent_send 向 <agent_id> 发送：y"

# 直接发送
./target/release/cam agent-send <agent_id> "y"
```

### 7. 清理

```bash
# 停止 agent
openclaw agent --agent main --message "使用 cam_agent_stop 停止 <agent_id>"

# 或直接停止
./target/release/cam agent-stop <agent_id>
```

## 检查清单

| 环节 | 检查命令 | 预期结果 |
|------|---------|---------|
| Agent 注册 | `cat ~/.config/code-agent-monitor/agents.json \| jq '.agents[].agent_id'` | 显示 cam-xxx |
| Watcher 运行 | `ps aux \| grep "cam watch-daemon"` | 进程存在 |
| Hook 触发 | `tail ~/.config/code-agent-monitor/hook.log` | 显示事件记录 |
| Urgency 分类 | dry-run 输出 | HIGH/MEDIUM/LOW 正确 |
| Dashboard payload | dry-run 输出 | JSON 格式正确 |
| Telegram 消息 | dry-run 输出 | 包含问题和选项 |
| 网络连接 | `tail ~/.openclaw/logs/gateway.err.log` | 无 fetch failed |

## 常见问题

### Agent 注册为 ext-xxx

**原因**：Agent 没有通过 CAM 启动，或 session_id 不匹配

**解决**：
```bash
# 检查 gateway 日志
tail -5000 ~/.openclaw/logs/gateway.log | grep "Agent ID"

# 重新通过 CAM 启动
openclaw agent --agent main --message "使用 cam_agent_start 在 /tmp 启动 Claude Code"
```

### 通知内容不完整

**原因**：终端快照行数不足，选项被截断

**临时解决**：增加 `main.rs` 中 `get_logs()` 的行数

### 网络连接失败

**原因**：VPN/网络问题

**解决**：
```bash
# 检查网络
curl -I https://api.telegram.org

# 重启 gateway
openclaw gateway restart
```

## 输出示例

成功的 dry-run 输出：
```
[DRY-RUN] Would send via system event (async)
[DRY-RUN] Payload: {
  "agent_id": "cam-1770687919",
  "event_type": "notification",
  "urgency": "MEDIUM",
  "summary": "等待用户输入",
  "terminal_snapshot": "..."
}
[DRY-RUN] Would send to channel=telegram target=1440537501
[DRY-RUN] Message: ⏸️ workspace 等待选择

1. 选项一
2. 选项二
3. 选项三

回复数字选择
[DRY-RUN] Agent ID tag: cam-1770687919
```
