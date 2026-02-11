# Code Agent Monitor (CAM)

监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)。

## 功能

- **进程监控** - 扫描系统中所有运行的 AI 编码代理
- **会话管理** - 列出、恢复 Claude Code 历史会话
- **Agent 生命周期** - 启动、停止、发送输入到代理
- **状态检测** - 检测代理是否等待用户输入（支持中英文）
- **智能通知路由** - 根据 urgency 自动选择直接发送或通过 Agent 转发
- **终端快照** - 通知中包含最近终端输出，方便远程了解上下文
- **MCP 服务器** - 提供 MCP 协议接口供其他工具调用
- **OpenClaw 集成** - 通过自然语言管理代理
- **Agent Teams** - 多 Agent 协作，支持远程管理和快捷回复
- **风险评估** - 自动评估权限请求的风险等级（低/中/高）

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
cam notify --event stop --agent-id cam-xxx

# 预览通知（不实际发送）
echo '{"cwd": "/tmp"}' | cam notify --event stop --agent-id cam-xxx --dry-run

# Team 编排命令
cam team-spawn <team> <name> [--type <type>] [--prompt <prompt>]
cam team-progress <team>
cam team-shutdown <team>

# 快捷回复
cam pending-confirmations
cam reply y [--target <agent_id>]
```

## 通知系统

CAM 支持智能通知路由，根据事件紧急程度选择发送方式：

### 通知路由策略

| Urgency | 事件类型 | 发送方式 | 说明 |
|---------|---------|---------|------|
| **HIGH** | permission_request, Error, WaitingForInput, notification(permission_prompt) | 直接发送到 channel | 需要立即响应，阻塞任务进度 |
| **MEDIUM** | stop, session_end, AgentExited, notification(idle_prompt) | 直接发送到 channel | 需要知道，可以分配新任务 |
| **LOW** | session_start, 其他 notification | 发给 OpenClaw Agent | 可选，Agent 可汇总或选择性转发 |

### Channel 自动检测

从 `~/.openclaw/openclaw.json` 按优先级检测：
1. telegram > whatsapp > discord > slack > signal

### 终端快照

HIGH/MEDIUM urgency 通知会自动包含最近 15 行终端输出：

```
✅ [CAM] cam-123 已停止

目录: /workspace/myapp

📸 终端快照:
```
$ cargo test
   Compiling myapp v0.1.0
    Finished release target
```

📡 via direct
```

### 调试通知

```bash
# 使用 --dry-run 预览通知路由
echo '{"cwd": "/tmp"}' | cam notify --event stop --agent-id test --dry-run

# 查看 hook 日志
tail -f ~/.claude-monitor/hook.log

# 验证 channel 检测
cat ~/.openclaw/openclaw.json | jq '.channels'
```

### 支持的输入等待模式

| 模式 | 示例 |
|------|------|
| Claude Code 确认 | `[Y]es / [N]o / [A]lways` |
| 标准确认 | `[Y/n]`, `[y/N]`, `[yes/no]` |
| 中文确认 | `[是/否]`, `确认？`, `是否继续？` |
| 权限请求 | `allow this action`, `是否授权` |
| 冒号提示 | `请输入文件名:`, `Enter your name:` |

### 配置 Claude Code Hooks

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

将 `<CAM_PLUGIN_PATH>` 替换为你的 CAM plugin 安装路径，例如：
- 通过 `openclaw plugins install --link` 安装：使用链接的源目录路径
- 查看安装路径：`openclaw plugins list`

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

## Agent Teams

CAM 支持 Claude Code Agent Teams 多 Agent 协作，可通过 Telegram/WhatsApp 远程管理。

### 快速开始

```bash
# 创建 Team 并启动 Agent
cam team-create my-project --description "我的项目"
cam team-spawn my-project developer --prompt "分析项目结构"

# 查看进度
cam team-progress my-project

# 快捷回复权限请求
cam pending-confirmations
cam reply y
```

### 风险评估

权限请求自动评估风险等级：

| 风险 | 示例 | 显示 |
|------|------|------|
| ✅ 低 | `ls`, `cat`, `/tmp` 文件 | 安全操作 |
| ⚠️ 中 | `npm install`, 项目文件 | 请确认 |
| 🔴 高 | `rm -rf`, `sudo`, 系统文件 | 高风险警告 |

### 远程管理

通过 Telegram 收到通知后，可直接回复：
- `y` / `是` - 批准操作
- `n` / `否` - 拒绝操作
- `1` / `2` / `3` - 选择选项

详见 [skills/agent-teams/SKILL.md](skills/agent-teams/SKILL.md)

## Haiku API 配置

CAM 使用 Claude Haiku 4.5 进行终端状态判断和问题提取。API 配置按以下优先级读取：

1. **`~/.config/cam`**（推荐）
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

**模型**: `claude-haiku-4-5-20251001`

## MCP 工具

### Agent 管理工具

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

### Team 管理工具

| 工具 | 描述 |
|------|------|
| `team_create` | 创建新 Team |
| `team_delete` | 删除 Team |
| `team_status` | 获取 Team 状态 |
| `inbox_read` | 读取成员收件箱 |
| `inbox_send` | 发送消息到成员收件箱 |
| `team_pending_requests` | 获取待处理的权限请求 |

### Team 编排工具

| 工具 | 描述 |
|------|------|
| `team_spawn_agent` | 在 Team 中启动 Agent |
| `team_progress` | 获取 Team 聚合进度 |
| `team_shutdown` | 优雅关闭 Team |
| `team_orchestrate` | 根据任务描述创建 Team |
| `team_assign_task` | 分配任务给成员 |

### 快捷回复工具

| 工具 | 描述 |
|------|------|
| `get_pending_confirmations` | 获取待处理确认 |
| `reply_pending` | 回复待处理确认（支持 y/n/1/2/3） |
| `handle_user_reply` | 处理自然语言回复 |

## 数据存储

| 路径 | 说明 |
|------|------|
| `~/.claude-monitor/agents.json` | 运行中的代理记录 |
| `~/.claude-monitor/watcher.pid` | Watcher 进程 PID |
| `~/.claude-monitor/watcher.log` | Watcher 日志 |
| `~/.claude-monitor/conversation_state.json` | 对话状态（pending confirmations） |
| `~/.claude/projects/` | Claude Code 会话数据 |
| `~/.claude/settings.json` | Claude Code 配置（含 hooks） |
| `~/.claude/teams/` | Agent Teams 配置和 inbox |
| `~/.claude/tasks/` | Agent Teams 任务列表 |
| `~/.config/cam` | Haiku API 配置（JSON） |

## 架构

### 通知模块架构

CAM 使用模块化的通知系统，支持多渠道发送：

```
src/notification/
├── mod.rs              # 模块导出
├── channel.rs          # NotificationChannel trait 定义
├── dispatcher.rs       # 多渠道分发器
├── builder.rs          # 自动配置构建器
├── urgency.rs          # Urgency 分类
├── payload.rs          # Payload 构建
├── terminal_cleaner.rs # 终端输出清理
├── formatter.rs        # 消息格式化
└── channels/
    ├── mod.rs
    ├── openclaw_message.rs  # 通用 OpenClaw 渠道
    └── dashboard.rs         # Dashboard 渠道
```

**核心组件**：

| 组件 | 职责 |
|------|------|
| `NotificationChannel` | 渠道 trait，定义 `send()` / `send_async()` |
| `NotificationDispatcher` | 管理多渠道，路由消息 |
| `NotificationBuilder` | 从配置自动检测并注册渠道 |
| `MessageFormatter` | 格式化不同事件类型的消息 |
| `PayloadBuilder` | 构建结构化 JSON payload |

**扩展新渠道**：

```rust
use crate::notification::{NotificationChannel, NotificationMessage, SendResult};

pub struct MyChannel { /* ... */ }

impl NotificationChannel for MyChannel {
    fn name(&self) -> &str { "my-channel" }
    fn should_send(&self, msg: &NotificationMessage) -> bool { true }
    fn send(&self, msg: &NotificationMessage) -> Result<SendResult> { /* ... */ }
    fn send_async(&self, msg: &NotificationMessage) -> Result<()> { /* ... */ }
}
```

### 目录结构

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
│   ├── openclaw_notifier.rs # 通知系统门面
│   ├── notification/      # 通知模块（见上方架构图）
│   ├── team_discovery.rs  # Agent Teams 发现
│   ├── team_bridge.rs     # Agent Teams 桥接
│   ├── inbox_watcher.rs   # Inbox 监控
│   ├── team_orchestrator.rs # Team 编排
│   ├── conversation_state.rs # 对话状态管理
│   └── notification_summarizer.rs # 智能通知汇总
├── plugins/
│   └── cam/               # OpenClaw plugin
├── skills/
│   ├── cam/               # 主 Skill（OpenClaw 集成）
│   ├── agent-teams/       # Agent Teams 专家 Skill
│   ├── cam-notify/        # 通知处理 Skill
│   └── cam-e2e-test/      # 端到端测试 Skill（开发用）
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
