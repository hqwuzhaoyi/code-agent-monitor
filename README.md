# Code Agent Monitor (CAM)

监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)。

## 功能

- **进程监控** - 扫描系统中所有运行的 AI 编码代理
- **会话管理** - 列出、恢复 Claude Code 历史会话
- **Agent 生命周期** - 启动、停止、发送输入到代理
- **状态检测** - 检测代理是否等待用户输入
- **MCP 服务器** - 提供 MCP 协议接口供其他工具调用
- **Clawdbot 集成** - 通过自然语言管理代理

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
```

## Clawdbot 集成

通过 Clawdbot 使用自然语言管理代理：

```bash
# 安装 plugin
clawdbot plugins install -l ~/clawd/plugins/cam

# 使用自然语言
clawdbot agent --agent main --message "现在跑着什么"
clawdbot agent --agent main --message "在 /tmp 启动一个 Claude"
clawdbot agent --agent main --message "什么情况"
clawdbot agent --agent main --message "停掉"
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
| `agent_start` | 启动新代理 |
| `agent_stop` | 停止代理 |
| `agent_list` | 列出 CAM 管理的代理 |
| `agent_send` | 向代理发送消息 |
| `agent_logs` | 获取代理日志 |
| `agent_status` | 获取代理状态 |

## 数据存储

| 路径 | 说明 |
|------|------|
| `~/.claude-monitor/agents.json` | 运行中的代理记录 |
| `~/.claude/projects/` | Claude Code 会话数据 |

## 目录结构

```
code-agent-monitor/
├── src/
│   ├── main.rs          # CLI 入口
│   ├── lib.rs           # 库入口
│   ├── process.rs       # 进程扫描
│   ├── session.rs       # 会话管理
│   ├── agent.rs         # Agent 生命周期
│   ├── mcp.rs           # MCP 服务器
│   ├── watcher.rs       # 状态监控
│   ├── input_detector.rs # 输入等待检测
│   └── jsonl_parser.rs  # JSONL 解析
├── plugins/
│   └── cam/             # Clawdbot plugin
├── skills/
│   └── SKILL.md         # Claude Code skill
└── docs/
    └── plans/           # 设计文档
```

## 开发

```bash
# 运行测试
cargo test

# 编译 release
cargo build --release
```

## License

MIT
