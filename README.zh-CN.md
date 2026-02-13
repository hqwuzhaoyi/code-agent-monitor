# Code Agent Monitor (CAM)

[English](README.md) | [中文](README.zh-CN.md)

监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)。

## 功能

- **TUI 仪表盘** - 终端 UI 监控代理，支持实时状态、过滤和 tmux 集成
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

# 启动 TUI 仪表盘
cam tui
```

### TUI 仪表盘

TUI 提供实时仪表盘，用于监控所有运行中的 agent，支持 lazygit 风格的即时过滤。

```bash
# 启动 TUI
cam tui
```

功能特性：
- 实时 agent 列表，显示状态指示器（Running/Idle/Error）
- 选中 agent 的 tmux 会话实时终端预览
- Lazygit 风格即时过滤（输入即过滤）
- 日志查看器，支持级别过滤（Error/Warn/Info/Debug）
- 最近通知面板

快捷键：
| 按键 | 操作 |
|-----|--------|
| `j/k` 或 `↑/↓` | 导航 agents |
| `/` | 进入过滤模式（输入过滤 ID 或项目名） |
| `Enter` | 连接到选中 agent 的 tmux 会话 |
| `l` | 切换到日志视图 |
| `f` | 切换日志级别过滤（日志视图中） |
| `Esc` | 清除过滤 / 返回仪表盘 |
| `q` | 退出 |

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

1. `~/.config/code-agent-monitor/config.json`（推荐）
2. 环境变量 `ANTHROPIC_API_KEY` / `ANTHROPIC_BASE_URL`
3. `~/.anthropic/api_key`
4. `~/.openclaw/openclaw.json`

**配置示例** (`~/.config/code-agent-monitor/config.json`):

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

CAM 采用分层架构，职责清晰：

```
┌─────────────────────────────────────────────────────────────┐
│                        CLI / MCP                            │
│                   (用户交互入口层)                            │
├─────────────────────────────────────────────────────────────┤
│     agent_mod    │   session_mod   │    team    │    ai     │
│   (Agent 管理)   │   (会话管理)     │  (团队编排) │  (AI 集成) │
├─────────────────────────────────────────────────────────────┤
│                      notification                           │
│                    (多渠道通知系统)                           │
├─────────────────────────────────────────────────────────────┤
│                        infra                                │
│              (基础设施: tmux, 进程扫描, jsonl)               │
└─────────────────────────────────────────────────────────────┘
```

### OpenClaw Plugin 集成

```
OpenClaw Gateway → CAM Plugin (TypeScript) → cam serve (MCP) → Rust 后端
                        ↓
                  spawn + stdin/stdout
                        ↓
                  JSON-RPC 2.0 协议
```

Plugin 通过 spawn 子进程调用 `cam serve`，使用 JSON-RPC 协议通过 stdin/stdout 通信。

### 模块结构

```
src/
├── main.rs              # CLI 入口
├── lib.rs               # 库导出
├── infra/               # 基础设施层
│   ├── tmux.rs          # tmux 会话管理
│   ├── process.rs       # 进程扫描
│   ├── jsonl.rs         # JSONL 日志解析
│   └── input.rs         # 输入等待检测
├── agent_mod/           # Agent 生命周期管理
│   ├── manager.rs       # 启动/停止/列表
│   ├── watcher.rs       # 状态监控
│   └── daemon.rs        # 后台 watcher 守护进程
├── session_mod/         # 会话管理
│   ├── manager.rs       # Claude Code 会话列表
│   └── state.rs         # 对话状态、快捷回复
├── mcp_mod/             # MCP Server
│   ├── server.rs        # JSON-RPC 请求处理
│   ├── types.rs         # 协议类型
│   └── tools/           # MCP 工具实现
├── notification/        # 通知系统
│   ├── channel.rs       # NotificationChannel trait
│   ├── dispatcher.rs    # 多渠道分发器
│   ├── urgency.rs       # 紧急程度分类
│   ├── formatter.rs     # AI 驱动的消息格式化
│   ├── deduplicator.rs  # 120 秒去重窗口
│   └── channels/        # Telegram, WhatsApp, Dashboard 等
├── team/                # Agent Teams
│   ├── discovery.rs     # Team 配置发现
│   ├── bridge.rs        # 文件系统桥接
│   ├── orchestrator.rs  # Agent 编排
│   ├── task_list.rs     # 任务管理
│   └── inbox_watcher.rs # Inbox 监控
├── tui/                 # TUI 仪表盘
│   ├── app.rs           # 应用状态和主循环
│   ├── event.rs         # 事件处理（键盘、定时）
│   ├── ui.rs            # UI 渲染（仪表盘、日志）
│   ├── logs.rs          # 日志查看器（级别过滤）
│   ├── search.rs        # Lazygit 风格搜索输入
│   ├── state.rs         # Agent/通知状态类型
│   └── terminal_stream.rs # 实时 tmux 捕获
├── ai/                  # AI 集成
│   ├── client.rs        # Anthropic API 客户端
│   └── extractor.rs     # 终端内容提取
└── anthropic.rs         # Haiku API 便捷封装
```

### 架构文档

详细架构文档请参阅：

- [核心模块](docs/architecture/core-modules.md) - 模块职责和依赖关系
- [Plugin 集成](docs/architecture/plugin-integration.md) - OpenClaw Plugin 架构
- [通知系统](docs/architecture/notification-system.md) - 多渠道通知路由
- [Agent Teams](docs/architecture/agent-teams.md) - 多 Agent 协作系统

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
| `~/.config/code-agent-monitor/config.json` | Haiku API 配置 |
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
