# Code Agent Monitor (CAM)

[English](#english) | [中文](#中文)

---

<a name="english"></a>

Monitor and manage AI coding agent processes (Claude Code, OpenCode, Codex).

## Features

- **Process Monitoring** - Scan all running AI coding agents in the system
- **Session Management** - List and resume Claude Code historical sessions
- **Agent Lifecycle** - Start, stop, and send input to agents
- **Smart Notifications** - Route notifications based on urgency (HIGH/MEDIUM/LOW)
- **Terminal Snapshots** - Include recent terminal output in notifications for remote context
- **MCP Server** - Provide MCP protocol interface for other tools
- **OpenClaw Integration** - Manage agents via natural language
- **Agent Teams** - Multi-agent collaboration with remote management and quick replies
- **Risk Assessment** - Automatically evaluate permission request risk levels

## Installation

### Prerequisites

- Rust 1.70+
- tmux
- Claude Code CLI (optional, for agent management)

### Build from Source

```bash
# Clone the repository
git clone https://github.com/hqwuzhaoyi/code-agent-monitor.git
cd code-agent-monitor

# Build release binary
cargo build --release

# Binary location
./target/release/cam

# Optional: Install to PATH
cp target/release/cam /usr/local/bin/
```

### OpenClaw Plugin Installation

```bash
# Install as OpenClaw plugin
openclaw plugins install --link /path/to/code-agent-monitor/plugins/cam
openclaw gateway restart
```

## Usage

### Basic Commands

```bash
# List all agent processes
cam list

# List historical sessions
cam sessions

# Resume a session to tmux
cam resume <session_id>

# View session logs
cam logs <session_id> --limit 10

# Kill a process
cam kill <pid>

# Start MCP server
cam serve

# Start background watcher daemon
cam watch-daemon -i 3
```

### Notification Commands

```bash
# Send notification event
cam notify --event stop --agent-id cam-xxx

# Preview notification (dry-run)
echo '{"cwd": "/tmp"}' | cam notify --event stop --agent-id cam-xxx --dry-run
```

### Team Commands

```bash
# Create a team
cam team-create my-project --description "My project"

# Spawn an agent in team
cam team-spawn my-project developer --prompt "Analyze project structure"

# View team progress
cam team-progress my-project

# Shutdown team
cam team-shutdown my-project
```

### Quick Reply Commands

```bash
# View pending confirmations
cam pending-confirmations

# Reply to pending confirmation
cam reply y [--target <agent_id>]
```

## Configuration

### Haiku API Configuration

CAM uses Claude Haiku 4.5 for terminal state detection and question extraction. API configuration is read in the following priority:

1. `~/.config/cam` (recommended)
2. Environment variables `ANTHROPIC_API_KEY` / `ANTHROPIC_BASE_URL`
3. `~/.anthropic/api_key`
4. `~/.openclaw/openclaw.json`

**Configuration example** (`~/.config/cam`):

```json
{
  "anthropic_api_key": "sk-xxx",
  "anthropic_base_url": "http://localhost:23000/"
}
```

### Claude Code Hooks Configuration

To enable automatic notifications when Claude Code is idle:

**Automatic configuration (recommended)**:

```bash
# Get CAM plugin path
CAM_BIN=$(openclaw plugins list --json | jq -r '.[] | select(.name == "cam") | .path')/bin/cam

# Add hooks to Claude Code config
cat ~/.claude/settings.json | jq --arg cam "$CAM_BIN" '.hooks = {
  "Notification": [{
    "matcher": "idle_prompt",
    "hooks": [{
      "type": "command",
      "command": ($cam + " notify --event idle_prompt --agent-id $SESSION_ID")
    }]
  }]
}' > ~/.claude/settings.json.tmp && mv ~/.claude/settings.json.tmp ~/.claude/settings.json
```

**Manual configuration**:

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "Notification": [
      {
        "matcher": "idle_prompt",
        "hooks": [
          {
            "type": "command",
            "command": "<CAM_PLUGIN_PATH>/bin/cam notify --event idle_prompt --agent-id $SESSION_ID"
          }
        ]
      }
    ]
  }
}
```

## Debugging

### View Logs

```bash
# View hook logs
tail -f ~/.config/code-agent-monitor/hook.log

# View watcher logs
tail -f ~/.config/code-agent-monitor/watcher.log

# Check watcher status
cat ~/.config/code-agent-monitor/watcher.pid
```

### Dry-Run Testing

```bash
# Preview HIGH urgency notification
echo '{"cwd": "/workspace"}' | cam notify --event permission_request --agent-id cam-test --dry-run

# Preview MEDIUM urgency notification
echo '{"cwd": "/workspace"}' | cam notify --event stop --agent-id cam-test --dry-run
```

### Verify Channel Detection

```bash
# Check OpenClaw channel configuration
cat ~/.openclaw/openclaw.json | jq '.channels'

# Test channel detection
echo '{}' | cam notify --event stop --agent-id test --dry-run 2>&1 | grep "channel="
```

### Common Issues

| Issue | Solution |
|-------|----------|
| Notifications not sending | Check `~/.config/code-agent-monitor/hook.log` for records |
| Send failures | Check stderr output, may be network or API rate limiting |
| Wrong routing | Use `--dry-run` to verify urgency classification |
| Channel detection failed | Check `~/.openclaw/openclaw.json` configuration |
| New format not applied | Restart watcher daemon |

### Restart Watcher

After code changes, restart the watcher:

```bash
kill $(cat ~/.config/code-agent-monitor/watcher.pid) 2>/dev/null
# Watcher will auto-start on next agent launch
```

## Architecture

### Module Structure

```
src/
├── main.rs              # CLI entry point
├── lib.rs               # Library exports
├── process.rs           # Process scanning
├── session.rs           # Session management
├── agent.rs             # Agent lifecycle
├── mcp.rs               # MCP server
├── agent_watcher.rs     # Agent state monitoring
├── anthropic.rs         # Haiku API integration
├── notification/        # Notification module
│   ├── channel.rs       # NotificationChannel trait
│   ├── dispatcher.rs    # Multi-channel dispatcher
│   ├── builder.rs       # Auto-configuration builder
│   ├── urgency.rs       # Urgency classification
│   ├── payload.rs       # Payload construction
│   ├── formatter.rs     # Message formatting
│   └── channels/        # Channel implementations
└── team/                # Agent Teams module
    ├── discovery.rs     # Team discovery
    ├── bridge.rs        # Team bridge
    ├── orchestrator.rs  # Team orchestration
    └── inbox_watcher.rs # Inbox monitoring
```

### Notification Routing

| Urgency | Events | Behavior |
|---------|--------|----------|
| HIGH | permission_request, Error, WaitingForInput | Send immediately |
| MEDIUM | AgentExited, idle_prompt | Send notification |
| LOW | session_start, stop | Silent |

### Data Storage

| Path | Description |
|------|-------------|
| `~/.config/code-agent-monitor/agents.json` | Running agent records |
| `~/.config/code-agent-monitor/watcher.pid` | Watcher process PID |
| `~/.config/code-agent-monitor/hook.log` | Hook logs |
| `~/.config/code-agent-monitor/conversation_state.json` | Conversation state |
| `~/.config/cam` | Haiku API configuration |
| `~/.claude/teams/` | Agent Teams |
| `~/.claude/tasks/` | Task lists |

## Development

### Build

```bash
# Debug build
cargo build

# Release build
cargo build --release
```

### Run Tests

```bash
# Run all tests
cargo test

# Run tests sequentially (avoid tmux conflicts)
cargo test -- --test-threads=1

# Run specific module tests
cargo test --lib notification
cargo test --lib team
```

### Update Plugin Binary

```bash
cargo build --release
cp target/release/cam plugins/cam/bin/cam
openclaw gateway restart
```

## License

MIT

---

<a name="中文"></a>

# Code Agent Monitor (CAM)

监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)。

## 功能

- **进程监控** - 扫描系统中所有运行的 AI 编码代理
- **会话管理** - 列出、恢复 Claude Code 历史会话
- **Agent 生命周期** - 启动、停止、发送输入到代理
- **智能通知路由** - 根据 urgency 自动选择直接发送或通过 Agent 转发
- **终端快照** - 通知中包含最近终端输出，方便远程了解上下文
- **MCP 服务器** - 提供 MCP 协议接口供其他工具调用
- **OpenClaw 集成** - 通过自然语言管理代理
- **Agent Teams** - 多 Agent 协作，支持远程管理和快捷回复
- **风险评估** - 自动评估权限请求的风险等级（低/中/高）

## 安装

### 前置要求

- Rust 1.70+
- tmux
- Claude Code CLI（可选，用于 agent 管理）

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/hqwuzhaoyi/code-agent-monitor.git
cd code-agent-monitor

# 编译 release 版本
cargo build --release

# 二进制位置
./target/release/cam

# 可选：安装到 PATH
cp target/release/cam /usr/local/bin/
```

### OpenClaw 插件安装

```bash
# 安装为 OpenClaw 插件
openclaw plugins install --link /path/to/code-agent-monitor/plugins/cam
openclaw gateway restart
```

## 使用方法

### 基础命令

```bash
# 列出所有代理进程
cam list

# 列出历史会话
cam sessions

# 恢复会话到 tmux
cam resume <session_id>

# 查看会话日志
cam logs <session_id> --limit 10

# 终止进程
cam kill <pid>

# 启动 MCP 服务器
cam serve

# 启动后台监控 daemon
cam watch-daemon -i 3
```

### 通知命令

```bash
# 发送通知事件
cam notify --event stop --agent-id cam-xxx

# 预览通知（不实际发送）
echo '{"cwd": "/tmp"}' | cam notify --event stop --agent-id cam-xxx --dry-run
```

### Team 命令

```bash
# 创建 Team
cam team-create my-project --description "我的项目"

# 在 Team 中启动 Agent
cam team-spawn my-project developer --prompt "分析项目结构"

# 查看 Team 进度
cam team-progress my-project

# 关闭 Team
cam team-shutdown my-project
```

### 快捷回复命令

```bash
# 查看待处理确认
cam pending-confirmations

# 回复待处理确认
cam reply y [--target <agent_id>]
```

## 配置

### Haiku API 配置

CAM 使用 Claude Haiku 4.5 进行终端状态判断和问题提取。API 配置按以下优先级读取：

1. `~/.config/cam`（推荐）
2. 环境变量 `ANTHROPIC_API_KEY` / `ANTHROPIC_BASE_URL`
3. `~/.anthropic/api_key`
4. `~/.openclaw/openclaw.json`

**配置示例** (`~/.config/cam`):

```json
{
  "anthropic_api_key": "sk-xxx",
  "anthropic_base_url": "http://localhost:23000/"
}
```

### Claude Code Hooks 配置

为了让 Claude Code 在空闲时自动通知 CAM，需要配置 hooks。

**自动配置（推荐）**：

```bash
# 获取 CAM plugin 安装路径
CAM_BIN=$(openclaw plugins list --json | jq -r '.[] | select(.name == "cam") | .path')/bin/cam

# 添加 hooks 到 Claude Code 配置
cat ~/.claude/settings.json | jq --arg cam "$CAM_BIN" '.hooks = {
  "Notification": [{
    "matcher": "idle_prompt",
    "hooks": [{
      "type": "command",
      "command": ($cam + " notify --event idle_prompt --agent-id $SESSION_ID")
    }]
  }]
}' > ~/.claude/settings.json.tmp && mv ~/.claude/settings.json.tmp ~/.claude/settings.json
```

**手动配置**：

在 `~/.claude/settings.json` 中添加：

```json
{
  "hooks": {
    "Notification": [
      {
        "matcher": "idle_prompt",
        "hooks": [
          {
            "type": "command",
            "command": "<CAM_PLUGIN_PATH>/bin/cam notify --event idle_prompt --agent-id $SESSION_ID"
          }
        ]
      }
    ]
  }
}
```

## 调试

### 查看日志

```bash
# 查看 hook 日志
tail -f ~/.config/code-agent-monitor/hook.log

# 查看 watcher 日志
tail -f ~/.config/code-agent-monitor/watcher.log

# 检查 watcher 状态
cat ~/.config/code-agent-monitor/watcher.pid
```

### Dry-Run 测试

```bash
# 预览 HIGH urgency 通知
echo '{"cwd": "/workspace"}' | cam notify --event permission_request --agent-id cam-test --dry-run

# 预览 MEDIUM urgency 通知
echo '{"cwd": "/workspace"}' | cam notify --event stop --agent-id cam-test --dry-run
```

### 验证 Channel 检测

```bash
# 检查 OpenClaw channel 配置
cat ~/.openclaw/openclaw.json | jq '.channels'

# 测试 channel 检测
echo '{}' | cam notify --event stop --agent-id test --dry-run 2>&1 | grep "channel="
```

### 常见问题

| 问题 | 解决方案 |
|------|---------|
| 通知没有发送 | 检查 `~/.config/code-agent-monitor/hook.log` 是否有记录 |
| 发送失败 | 查看 stderr 输出，可能是网络问题或 API 限流 |
| 路由错误 | 使用 `--dry-run` 确认 urgency 分类是否正确 |
| Channel 检测失败 | 检查 `~/.openclaw/openclaw.json` 配置 |
| 新格式未生效 | 重启 watcher daemon |

### 重启 Watcher

修改代码后，需要重启 watcher：

```bash
kill $(cat ~/.config/code-agent-monitor/watcher.pid) 2>/dev/null
# watcher 会在下次 agent 启动时自动启动
```

## 架构

### 模块结构

```
src/
├── main.rs              # CLI 入口
├── lib.rs               # 库导出
├── process.rs           # 进程扫描
├── session.rs           # 会话管理
├── agent.rs             # Agent 生命周期
├── mcp.rs               # MCP 服务器
├── agent_watcher.rs     # Agent 状态监控
├── anthropic.rs         # Haiku API 集成
├── notification/        # 通知模块
│   ├── channel.rs       # NotificationChannel trait
│   ├── dispatcher.rs    # 多渠道分发器
│   ├── builder.rs       # 自动配置构建器
│   ├── urgency.rs       # Urgency 分类
│   ├── payload.rs       # Payload 构建
│   ├── formatter.rs     # 消息格式化
│   └── channels/        # 渠道实现
└── team/                # Agent Teams 模块
    ├── discovery.rs     # Team 发现
    ├── bridge.rs        # Team 桥接
    ├── orchestrator.rs  # Team 编排
    └── inbox_watcher.rs # Inbox 监控
```

### 通知路由

| Urgency | 事件类型 | 行为 |
|---------|---------|------|
| HIGH | permission_request, Error, WaitingForInput | 立即发送 |
| MEDIUM | AgentExited, idle_prompt | 发送通知 |
| LOW | session_start, stop | 静默 |

### 数据存储

| 路径 | 说明 |
|------|------|
| `~/.config/code-agent-monitor/agents.json` | 运行中的代理记录 |
| `~/.config/code-agent-monitor/watcher.pid` | Watcher 进程 PID |
| `~/.config/code-agent-monitor/hook.log` | Hook 日志 |
| `~/.config/code-agent-monitor/conversation_state.json` | 对话状态 |
| `~/.config/cam` | Haiku API 配置 |
| `~/.claude/teams/` | Agent Teams |
| `~/.claude/tasks/` | 任务列表 |

## 开发

### 构建

```bash
# Debug 构建
cargo build

# Release 构建
cargo build --release
```

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行测试（顺序执行，避免 tmux 冲突）
cargo test -- --test-threads=1

# 运行特定模块测试
cargo test --lib notification
cargo test --lib team
```

### 更新插件二进制

```bash
cargo build --release
cp target/release/cam plugins/cam/bin/cam
openclaw gateway restart
```

## License

MIT
