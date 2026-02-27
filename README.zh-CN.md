# Code Agent Monitor (CAM)

[English](README.md) | [中文](README.zh-CN.md)

远程监控和管理 AI 编码 Agent，通过 OpenClaw 对话交互。

AI 编码 Agent（Claude Code、Codex、OpenCode）在执行过程中经常需要人工输入——权限确认、决策选择、错误处理。CAM 让你可以通过 OpenClaw 对话在手机上处理这些请求，而不用一直守在电脑前。

```
Agent 需要输入 → CAM 检测 → OpenClaw 通知你 → 你回复 → Agent 继续执行
```

## 特性

- **远程审批** — 通过 OpenClaw 对话在手机上监控和审批 Agent 请求
- **TUI 仪表盘** — 四面板布局：Agent 列表、终端预览、通知历史、详情面板
- **多 Agent 支持** — Claude Code、Codex、OpenCode，统一适配层自动检测
- **AI 智能提取** — 使用 AI 从终端快照中提取通知内容，无需硬编码模式
- **风险评估** — 自动评估 Bash 命令风险等级（低/中/高），辅助审批决策
- **Agent Teams** — 多 Agent 编排，支持团队创建、任务分配、进度追踪
- **通知去重** — 120 秒窗口内 80% 相似度自动去重，避免重复打扰
- **服务模式** — 安装为 launchd 系统服务，开机自启，持续监控

## 入门教程

从零开始，10 分钟内完成安装和首次使用。

### 前置要求

| 依赖 | 说明 |
|------|------|
| Rust 工具链 | 需要 `cargo`，安装：`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| tmux | CAM 在 tmux 会话中运行 Agent，macOS：`brew install tmux` |
| OpenClaw | 已安装并配置了消息渠道（用于接收通知和回复） |

### 第一步：安装 CAM

```bash
# 克隆仓库
git clone https://github.com/anthropics/code-agent-monitor.git
cd code-agent-monitor

# 编译 release 版本
cargo build --release

# 安装到 PATH（二选一）
cp target/release/cam /usr/local/bin/
# 或者添加到你的 shell 配置
# export PATH="$PATH:/path/to/code-agent-monitor/target/release"
```

验证安装：

```bash
cam --help
```

如果你使用 OpenClaw，还需要安装插件和 Skills：

```bash
# 安装 CAM 插件
openclaw plugins install --link /path/to/code-agent-monitor/plugins/cam

# 安装 Skills
mkdir -p ~/.openclaw/skills
for skill in cam agent-teams cam-notify; do
  cp -r skills/$skill ~/.openclaw/skills/
done

openclaw gateway restart
```

### 第二步：配置 CAM（推荐使用 Bootstrap）

运行交互式配置向导，自动检测 OpenClaw 配置和已安装的 Agent 工具：

```bash
cam bootstrap
```

这一步会自动完成 webhook、AI 监控、agent hooks 的配置。如果你安装了 OpenClaw，它会自动检测 gateway URL、hook token 和 API provider。

全自动模式（不提示，使用所有检测到的默认值）：

```bash
cam bootstrap --auto
```

如果你更喜欢手动配置，请按照下面的步骤 2a 和 2b 操作，否则直接跳到第三步。

### 步骤 2a：配置 Webhook（手动）

CAM 通过 Webhook 将通知发送到 OpenClaw Gateway，并使用 AI 分析终端内容。创建配置文件：

```bash
mkdir -p ~/.config/code-agent-monitor
```

编辑 `~/.config/code-agent-monitor/config.json`：

```json
{
  "webhook": {
    "gateway_url": "http://localhost:18789",
    "hook_token": "your-token",
    "timeout_secs": 30
  },
  "anthropic_api_key": "sk-ant-xxx",
  "anthropic_base_url": "https://api.anthropic.com"
}
```

- `gateway_url` — OpenClaw Gateway 地址，默认本地 `18789` 端口
- `hook_token` — OpenClaw 的 Hooks 认证 token，来自 `~/.openclaw/openclaw.json` 中的 `hooks.token` 字段。可以用以下命令查看：
  ```bash
  cat ~/.openclaw/openclaw.json | python3 -c "import sys,json; print(json.load(sys.stdin)['hooks']['token'])"
  ```
- `timeout_secs` — 请求超时时间
- `anthropic_api_key`（推荐）— Anthropic API Key，用于 AI 智能分析终端内容、提取 Agent 问题。强烈推荐配置，否则通知将缺少 AI 分析能力
- `anthropic_base_url` — Anthropic API 地址，默认 `https://api.anthropic.com`，如使用代理可修改

API Key 也可以通过以下方式提供（按优先级）：
1. 配置文件（推荐，如上）
2. 环境变量 `ANTHROPIC_API_KEY`
3. `~/.anthropic/api_key`
4. `~/.openclaw/openclaw.json`

### 步骤 2b：配置 Agent Hooks（手动）

让 Claude Code 在需要输入时自动通知 CAM：

```bash
cam setup claude
```

这条命令会自动将 hooks 写入 `~/.claude/settings.json`，让 CAM 能接收 Claude Code 的事件（权限请求、空闲提示等）。

如果你使用其他 Agent：

```bash
cam setup codex      # 配置 Codex CLI
cam setup opencode   # 配置 OpenCode
```

想先预览变更而不实际写入？加 `--dry-run`：

```bash
cam setup --dry-run claude
```

### 第三步：安装 Watcher 服务

Watcher 是后台守护进程，持续监控所有 Agent 的终端状态。安装为系统服务后会开机自启：

```bash
# 安装为 launchd 服务（macOS）
cam install

# 确认服务运行中
cam service status
```

你应该看到服务状态为 running。如果需要重新安装：

```bash
cam install --force
```

### 第四步：通过 OpenClaw 启动你的第一个 Agent

一切就绪！现在打开 OpenClaw 对话，用自然语言启动一个 Agent：

```
你: 帮我在 ~/workspace/myapp 启动个 Claude
OpenClaw: 好，启动中...
          ✅ 已启动 Claude @ ~/workspace/myapp (cam-1706789012)
```

你也可以直接给 Agent 一个任务：

```
你: 在 ~/workspace/myapp 启动 Claude，实现一个 TODO 应用
```

其他自然语言示例：

| 你说的话 | OpenClaw 执行的操作 |
|---------|-------------------|
| "帮我在 xxx 启动个 Claude" | 启动 Agent |
| "开个 Codex 跑 xxx 项目" | 启动 Codex Agent |
| "看看在干嘛" / "现在跑着什么" | 列出运行中的 Agent |
| "继续" / "y" / "好" | 发送确认给 Agent |
| "看看输出" / "进度怎样" | 查看 Agent 日志 |
| "停掉" / "别干了" | 停止 Agent |
| "恢复之前的" / "继续上次的" | 恢复历史会话 |

> 你也可以用 CLI 直接启动：`cam start "实现一个 TODO 应用"`

### 第五步：通过 TUI 监控

打开 TUI 仪表盘，实时查看所有 Agent 状态：

```bash
cam tui
```

TUI 是四面板布局，你可以看到 Agent 列表、终端实时预览、通知历史。用 `Tab` 切换面板，`j/k` 导航，`Enter` 连接到 Agent 的 tmux 会话。

### 第六步：在 OpenClaw 中接收通知并回复

当 Agent 需要你的输入时（比如请求执行 `rm -rf` 的权限），CAM 会通过 Webhook 发送通知到 OpenClaw Gateway，你会在 OpenClaw 对话中收到消息。

直接在对话中回复即可：
- 回复 `y` — 批准请求
- 回复 `n` — 拒绝请求
- 回复其他文本 — 作为输入发送给 Agent

你也可以在终端中快速回复：

```bash
# 查看所有待处理的确认请求
cam pending-confirmations

# 批准
cam reply y

# 批准所有待处理请求
cam reply y --all

# 只批准低风险请求
cam reply y --risk low
```

到这里，你已经完成了 CAM 的完整配置。Agent 在后台工作，需要你时手机会收到通知，回复即可。

## TUI 仪表盘

四面板布局，一屏掌握所有 Agent 状态：

```
┌─────────────────────┬──────────────────────┐
│   Agent 列表         │  终端预览             │
│   状态指示器          │  实时 tmux 截取       │
├─────────────────────┼──────────────────────┤
│   通知历史           │  通知详情             │
│   按紧急程度着色      │  项目、风险、快照      │
├─────────────────────┴──────────────────────┤
│   帮助栏（根据当前焦点显示快捷键）            │
└─────────────────────────────────────────────┘
```

功能亮点：
- 实时 Agent 列表，状态指示器（Running / Idle / Error）
- 选中 Agent 的终端实时预览（可滚动）
- 通知面板按紧急程度着色（红色=HIGH，黄色=MEDIUM，灰色=LOW）
- 通知详情：项目名、风险等级、事件详情、终端快照
- 智能时间显示：今天 `HH:MM`，更早 `MM-DD HH:MM`
- Lazygit 风格即时过滤（输入即过滤）
- 焦点感知的鼠标滚动

快捷键：

| 按键 | 操作 |
|------|------|
| `Tab` | 切换面板焦点（Agent 列表 → 终端预览 → 通知详情 → 通知历史） |
| `j/k` 或 `↑/↓` | 在当前面板中导航 |
| `Enter` | 连接到选中 Agent 的 tmux 会话 |
| `x` / `d` | 关闭选中的 Agent |
| `/` | 进入过滤模式（按 ID 或项目名过滤） |
| `l` | 切换到日志视图 |
| `f` | 切换日志级别过滤 |
| `Esc` | 清除过滤 / 返回 Agent 列表焦点 |
| `?` | 显示帮助 |
| `q` | 退出 |

## CLI 命令参考

### Agent 管理

| 命令 | 说明 |
|------|------|
| `cam start [prompt]` | 启动 Agent（支持 `--agent`、`--cwd`、`--resume`） |
| `cam list` | 列出所有运行中的 Agent |
| `cam kill <pid>` | 终止 Agent 进程 |
| `cam resume <session_id>` | 恢复历史会话（attach tmux） |
| `cam sessions` | 列出所有历史会话 |

### 监控

| 命令 | 说明 |
|------|------|
| `cam tui` | 启动 TUI 仪表盘 |
| `cam watch-daemon -i <秒>` | 启动后台 Watcher |
| `cam logs <session_id>` | 查看会话日志 |

### 通知与回复

| 命令 | 说明 |
|------|------|
| `cam notify --event <event>` | 发送通知事件 |
| `cam watch-trigger --agent-id <id>` | 手动触发检测（调试用） |
| `cam pending-confirmations` | 查看待处理确认 |
| `cam reply <response>` | 回复确认（支持 `--all`、`--agent`、`--risk`） |

### 服务管理

| 命令 | 说明 |
|------|------|
| `cam install` | 安装 Watcher 为系统服务 |
| `cam uninstall` | 卸载服务 |
| `cam service status` | 查看服务状态 |
| `cam service restart` | 重启服务 |
| `cam service logs [-f]` | 查看/跟踪服务日志 |

### Agent Teams

| 命令 | 说明 |
|------|------|
| `cam team-create <name>` | 创建 Team |
| `cam team-spawn <team> <name>` | 在 Team 中启动 Agent |
| `cam team-progress <team>` | 查看 Team 进度 |
| `cam team-shutdown <team>` | 关闭 Team |

### Hooks 配置

| 命令 | 说明 |
|------|------|
| `cam setup claude` | 配置 Claude Code hooks |
| `cam setup codex` | 配置 Codex CLI |
| `cam setup opencode` | 配置 OpenCode |
| `cam setup --dry-run <agent>` | 预览变更 |

## 通知系统

### 紧急程度

| 级别 | 事件 | 行为 |
|------|------|------|
| HIGH | 权限请求、错误、等待输入 | 立即发送，需要用户回复 |
| MEDIUM | Agent 退出、空闲提示 | 发送通知，可能需要操作 |
| LOW | 会话开始、结束、工具使用 | 静默，不发送通知 |

### 工作原理

1. **AI 智能提取** — AI 分析终端快照，提取 Agent 的问题内容，而非硬编码正则匹配
2. **风险评估** — 对 Bash 命令进行三层评估：白名单自动通过、黑名单必须人工确认、其余由 AI 判断
3. **通知去重** — 120 秒窗口内相似度超过 80% 的通知自动合并
4. **上下文扩展** — 如果终端快照不完整，自动扩展行数重试（80 → 150 → 300 → 500 → 800 行）

### 自动审批

OpenClaw 使用三层决策模型处理低风险操作：

- **白名单** — `ls`、`cat`、`git status`、`cargo test`、`npm test` 等安全命令自动批准
- **黑名单** — `rm`、`sudo`、含 `&&`、`|`、`>` 的命令必须人工确认
- **LLM 判断** — 不在名单中的命令由 AI 分析风险

即使白名单命令，如果参数包含敏感路径（`/etc/`、`~/.ssh/`、`.env`），仍需人工确认。

## 配置

所有配置文件位于 `~/.config/code-agent-monitor/`：

| 文件 | 说明 |
|------|------|
| `config.json` | Webhook 和 AI 监控配置 |
| `agents.json` | 运行中的 Agent 记录 |
| `notifications.jsonl` | 本地通知记录（TUI 使用） |
| `conversation_state.json` | 对话状态 |
| `dedup_state.json` | 通知去重状态 |
| `hook.log` | Hook 日志 |
| `watcher.pid` | Watcher 进程 PID |

完整配置示例（`config.json`）：

```json
{
  "webhook": {
    "gateway_url": "http://localhost:18789",
    "hook_token": "your-token",
    "timeout_secs": 30
  },
  "anthropic_api_key": "sk-ant-xxx",
  "anthropic_base_url": "https://api.anthropic.com"
}
```

## 架构概览

```
Claude Code Hooks ──→ CAM ──→ Webhook ──→ OpenClaw Gateway ──→ 你的手机
Terminal Watcher  ──↗        ↙
                    CAM TUI
```

CAM 通过两种方式感知 Agent 状态：
- **Hooks** — Agent 主动推送事件（Claude Code、Codex、OpenCode 各有适配器）
- **Watcher** — 后台轮询终端状态，用 AI 判断 Agent 是否在等待输入

所有通知统一通过 Webhook 发送到 OpenClaw Gateway（`POST /hooks/agent`），触发 OpenClaw 对话。用户在对话中回复后，CAM skill 通过 `cam reply` 将输入发送回 Agent 的 tmux 会话。

```
CAM → POST /hooks/agent → Gateway → OpenClaw 对话
                                        ↓
                              用户回复 "y"
                                        ↓
                              CAM skill → cam reply → tmux send-keys
```

详细架构文档：
- [核心模块](docs/architecture/core-modules.md)
- [Plugin 集成](docs/architecture/plugin-integration.md)
- [通知系统](docs/architecture/notification-system.md)
- [Agent Teams](docs/architecture/agent-teams.md)

## 开发

```bash
# 编译
cargo build --release

# 运行测试
cargo test

# 顺序执行测试（避免 tmux 冲突）
cargo test -- --test-threads=1

# 更新插件二进制并重启服务
cargo build --release
cp target/release/cam plugins/cam/bin/cam
cam service restart
```

项目结构和开发指南详见 [docs/development.md](docs/development.md)。

## License

MIT
