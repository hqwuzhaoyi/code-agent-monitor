# Code Agent Monitor (CAM)

监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)。

## 功能

- **进程监控** - 扫描系统中所有运行的 AI 编码代理
- **会话管理** - 列出、恢复 Claude Code 历史会话
- **Agent 生命周期** - 启动、停止、发送输入到代理
- **状态检测** - 检测代理是否等待用户输入（支持中英文）
- **自动状态通知** - 检测到关键事件时自动推送到 clawdbot
- **MCP 服务器** - 提供 MCP 协议接口供其他工具调用
- **OpenClaw 集成** - 通过自然语言管理代理

## 安装

```bash
# 编译
cargo build --release

# 二进制位置
./target/release/cam
```

## CLI 使用

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

# 发送通知事件
cam notify --event WaitingForInput --agent-id cam-xxx
```

## 自动状态通知

CAM 支持自动推送 Agent 状态变化到 clawdbot：

### 工作原理

1. **自动启动**: 当第一个 agent 启动时，watcher daemon 自动启动
2. **事件检测**: Watcher 每 3 秒轮询检测关键事件
3. **通知推送**: 通过 `openclaw agent --session-id main` 发送到 clawdbot
4. **用户响应**: clawdbot 询问用户后调用 `cam_agent_send` 执行

### 检测的事件类型

| 事件 | 触发条件 | 通知格式 |
|------|---------|---------|
| `AgentExited` | tmux session 退出 | `✅ Agent 已退出: cam-xxx` |
| `Error` | JSONL 解析到错误 | `❌ cam-xxx 错误: ...` |
| `WaitingForInput` | 检测到等待输入模式 | `⏸️ cam-xxx 等待输入 (Confirmation)` |

### 支持的输入等待模式

| 模式 | 示例 |
|------|------|
| Claude Code 确认 | `[Y]es / [N]o / [A]lways` |
| 标准确认 | `[Y/n]`, `[y/N]`, `[yes/no]` |
| 中文确认 | `[是/否]`, `确认？`, `是否继续？` |
| 权限请求 | `allow this action`, `是否授权` |
| 冒号提示 | `请输入文件名:`, `Enter your name:` |

### 手动控制 Watcher

```bash
# 查看 watcher 状态
cat ~/.claude-monitor/watcher.pid

# 查看 watcher 日志
tail -f ~/.claude-monitor/watcher.log

# 手动停止 watcher
kill $(cat ~/.claude-monitor/watcher.pid)
```

## OpenClaw 集成

通过 OpenClaw 使用自然语言管理代理：

```bash
# 安装 plugin
openclaw plugins install --link /Users/admin/workspace/code-agent-monitor/plugins/cam

# 使用自然语言
openclaw agent --agent main --message "现在跑着什么"
openclaw agent --agent main --message "在 /tmp 启动一个 Claude"
openclaw agent --agent main --message "什么情况"
openclaw agent --agent main --message "停掉"
```

详见 [plugins/cam/README.md](plugins/cam/README.md)

## MCP 工具

| 工具 | 描述 |
|------|------|
| `list_agents` | 列出系统中所有代理进程 |
| `list_sessions` | 列出 Claude Code 会话 |
| `resume_session` | 恢复会话到 tmux |
| `send_input` | 向 tmux 会话发送输入 |
| `kill_agent` | 终止进程 |
| `agent_start` | 启动新代理（自动启动 watcher） |
| `agent_stop` | 停止代理 |
| `agent_list` | 列出 CAM 管理的代理 |
| `agent_send` | 向代理发送消息 |
| `agent_logs` | 获取代理日志 |
| `agent_status` | 获取代理状态 |

## 数据存储

| 路径 | 说明 |
|------|------|
| `~/.claude-monitor/agents.json` | 运行中的代理记录 |
| `~/.claude-monitor/watcher.pid` | Watcher 进程 PID |
| `~/.claude-monitor/watcher.log` | Watcher 日志 |
| `~/.claude/projects/` | Claude Code 会话数据 |
| `~/.claude/settings.json` | Claude Code 配置（含 hooks） |

## 目录结构

```
code-agent-monitor/
├── src/
│   ├── main.rs            # CLI 入口
│   ├── lib.rs             # 库入口
│   ├── process.rs         # 进程扫描
│   ├── session.rs         # 会话管理
│   ├── agent.rs           # Agent 生命周期
│   ├── mcp.rs             # MCP 服务器
│   ├── agent_watcher.rs   # Agent 状态监控
│   ├── input_detector.rs  # 输入等待检测
│   ├── jsonl_parser.rs    # JSONL 解析
│   ├── watcher_daemon.rs  # Watcher 后台进程管理
│   └── openclaw_notifier.rs # OpenClaw 通知模块
├── plugins/
│   └── cam/               # OpenClaw plugin
├── tests/
│   ├── e2e.rs             # 端到端测试
│   ├── input_detector_test.rs # 输入检测测试
│   └── integration_test.rs # 集成测试
└── docs/
    └── plans/             # 设计文档
```

## 开发

```bash
# 运行测试
cargo test

# 运行测试（顺序执行，避免 tmux 冲突）
cargo test -- --test-threads=1

# 编译 release
cargo build --release
```

## License

MIT
