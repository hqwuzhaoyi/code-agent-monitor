# CAM 调试指南

本文档记录通知系统的调试方法和常见问题排查。

## 使用 --dry-run 预览通知

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
[DRY-RUN] Message: ⏸️ myproject 等待选择

1. 选项一
2. 选项二

回复数字选择
[DRY-RUN] Agent ID tag: cam-test
```

## 查看 Hook 日志

```bash
# 查看最近的 hook 触发记录
tail -50 ~/.claude-monitor/hook.log

# 实时监控 hook 日志
tail -f ~/.claude-monitor/hook.log

# 查看特定 agent 的日志
grep "cam-xxxxxxx" ~/.claude-monitor/hook.log
```

## 验证 Channel 检测

```bash
# 检查 OpenClaw channel 配置
cat ~/.openclaw/openclaw.json | jq '.channels'

# 测试 channel 检测是否正常（应显示 telegram/whatsapp 等）
echo '{}' | ./target/release/cam notify --event stop --agent-id test --dry-run 2>&1 | grep "channel="
```

## 常见问题排查

| 问题 | 排查方法 |
|------|---------|
| 通知没有发送 | 检查 `~/.claude-monitor/hook.log` 是否有记录 |
| 发送失败 | 查看 stderr 输出，可能是网络问题或 API 限流 |
| 路由错误 | 使用 `--dry-run` 确认 urgency 分类是否正确 |
| Channel 检测失败 | 检查 `~/.openclaw/openclaw.json` 配置 |
| 新格式未生效 | 重启 watcher daemon |
| 外部会话收到通知 | 检查 agent_id 是否为 ext- 前缀 |
| CAM agent 被识别为外部会话 | 见下方「Agent 注册问题排查」 |

## Agent 注册问题排查

**症状**：通过 OpenClaw 启动的 agent 没有收到通知，hook.log 显示被注册为 `ext-xxx`。

**排查步骤**：

```bash
# 1. 检查 agents.json 中是否有该 agent
cat ~/.claude-monitor/agents.json | jq '.agents[].agent_id'

# 2. 检查 tmux session 是否存在
command tmux list-sessions | grep cam

# 3. 检查 gateway 日志中是否有创建记录
tail -5000 ~/.openclaw/logs/gateway.log | grep "Agent ID"

# 4. 检查 hook 日志中的注册情况
grep "Registered\|Auto-registered" ~/.claude-monitor/hook.log | tail -20
```

**常见原因**：

| 原因 | 症状 | 解决方案 |
|------|------|---------|
| tmux session 手动创建 | gateway 日志无创建记录 | 必须通过 `cam_agent_start` 创建 |
| CAM 二进制未更新 | 旧代码逻辑 | `cargo build --release && cp target/release/cam plugins/cam/bin/cam && openclaw gateway restart` |
| agents.json 被清空 | agent 列表为空 | 重新通过 CAM 启动 agent |
| session_id 不匹配 | Hook 找不到对应 agent | 检查 Claude Code 的 session_id 是否与 agents.json 中的匹配 |
| cam_agent_start 返回错误 | OpenClaw Agent 绕过 CAM 用 exec 创建 | 见下方「CAM 插件错误排查」 |

## CAM 插件错误排查

**症状**：OpenClaw Agent 说"已启动 Claude Code session cam-xxx"，但实际上没有通过 CAM 创建。

**排查步骤**：

```bash
# 1. 检查 OpenClaw 对话历史，看是否用了 exec 而非 cam_agent_start
grep -i "exec.*tmux\|cam_agent_start" ~/.openclaw/agents/main/sessions/*.jsonl | tail -20

# 2. 检查 gateway.err.log 中的 CAM 错误
grep "cam_agent_start failed" ~/.openclaw/logs/gateway.err.log | tail -10

# 3. 检查 CAM 插件返回的错误
grep "Invalid JSON response\|CAM exited" ~/.openclaw/agents/main/sessions/*.jsonl | tail -10
```

**根本原因**：CAM 的 stderr 日志（INFO/WARN 等）混入了 stdout，导致 JSON 解析失败。OpenClaw Agent 看到错误后，会尝试用 `exec` 命令直接创建 tmux session 作为 fallback。

**解决方案**：

1. **修复 CAM stderr 输出**：确保 CAM 的日志输出到 stderr，JSON 响应输出到 stdout
2. **重新构建并更新插件**：
   ```bash
   cargo build --release
   cp target/release/cam plugins/cam/bin/cam
   openclaw gateway restart
   ```
3. **验证修复**：
   ```bash
   echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"agent_list","arguments":{}}}' | ./target/release/cam serve 2>/dev/null | jq '.'
   ```

## 通知链路架构

```
Watcher Daemon → input_detector → notifier.send_event → openclaw system event → Gateway → Telegram
```

完整数据流：
1. **Watcher Daemon** 轮询 tmux 会话，获取终端输出
2. **input_detector** 分析终端内容，检测等待输入模式
3. **notifier.send_event** 根据 urgency 决定是否发送
4. **openclaw system event** 将结构化 payload 发送到 Gateway
5. **Gateway** 路由到 OpenClaw Agent 进行 AI 处理
6. **Telegram** 最终用户收到通知

## 逐层检查方法

### Step 1: Watcher 检测层

```bash
# 检查 watcher 是否运行
ps aux | grep "cam watch-daemon" | grep -v grep

# 检查 watcher PID 文件
cat ~/.claude-monitor/watcher.pid

# 检查 agent 是否在列表
cat ~/.claude-monitor/agents.json | jq '.agents[].agent_id'

# 检查特定 agent 状态
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"agent_status","arguments":{"agent_id":"<AGENT_ID>"}}}' | ./target/release/cam serve 2>/dev/null | jq -r '.result.content[0].text'

# 手动运行 watcher 查看详细输出
./target/release/cam watch-daemon -i 2 2>&1
```

### Step 2: Input Detector 层

```bash
# 直接查看 agent 终端输出（最近 15 行）
command tmux capture-pane -t <AGENT_ID> -p -S -15

# 检查是否包含等待输入模式
command tmux capture-pane -t <AGENT_ID> -p -S -15 | grep -E '\[Y/n\]|\[Y\]es|❯|>'
```

### Step 3: OpenClaw 发送层

```bash
# 使用 dry-run 测试通知（不实际发送）
echo '{"cwd": "/workspace"}' | ./target/release/cam notify --event WaitingForInput --agent-id <AGENT_ID> --dry-run

# 检查 gateway 状态
openclaw gateway status

# 查看 gateway 日志
tail -50 ~/.openclaw/logs/gateway.log

# 手动测试 system event
openclaw system event --text '{"type":"test"}' --mode now
```

### Step 4: Telegram 接收层

```bash
# 检查 channel 配置
cat ~/.openclaw/openclaw.json | jq '.channels'

# 手动测试直接发送到 Telegram
openclaw message send --channel telegram --target <CHAT_ID> --message "test"
```

## 端到端调试流程

```bash
# 1. 确认 watcher 运行
ps aux | grep "cam watch-daemon"

# 2. 确认 agent 在列表中
cat ~/.claude-monitor/agents.json | jq '.agents[].agent_id'

# 3. 查看 agent 终端内容
command tmux capture-pane -t cam-xxxxxxx -p -S -15

# 4. 测试 dry-run 通知
echo '{"cwd": "/workspace"}' | ./target/release/cam notify --event WaitingForInput --agent-id cam-xxxxxxx --dry-run

# 5. 如果 dry-run 成功，检查 gateway
openclaw gateway status

# 6. 如果 gateway 正常，检查网络
openclaw message send --channel telegram --target <CHAT_ID> --message "debug test"
```

## 已知问题和解决方案

### 1. ❯ 提示符检测

- **问题**：Claude Code 使用 Unicode ❯ (U+276F) 作为主提示符
- **位置**：`src/input_detector.rs`
- **解决**：在 `CLAUDE_PROMPT_PATTERNS` 中添加 `❯\s*$` 模式

### 2. 检测行数不足

- **问题**：`get_last_lines(5)` 获取的行数被状态栏占用，实际内容被截断
- **位置**：`src/agent_watcher.rs` 的 `check_agent_status()`
- **解决**：增加到 `get_last_lines(15)` 确保捕获足够内容

### 3. 空闲检测冲突

- **问题**：`detect()` 方法内置 3 秒等待，与 watcher 轮询间隔冲突
- **位置**：`src/input_detector.rs`
- **解决**：新增 `detect_immediate()` 方法，跳过等待直接检测

### 4. 网络问题

- **问题**：Telegram API 请求在某些网络环境下失败
- **解决**：检查 VPN 连接，或使用 `--dry-run` 先验证逻辑正确性
