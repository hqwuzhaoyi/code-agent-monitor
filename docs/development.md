# CAM 开发指南

## 项目结构

```
src/
├── main.rs              # CLI 入口，处理 notify/watch/team 等命令
├── openclaw_notifier.rs # 通知系统门面（委托给 notification 子模块）
├── notification/        # 通知模块（模块化架构）
│   ├── mod.rs           # 模块导出
│   ├── channel.rs       # NotificationChannel trait 定义
│   ├── dispatcher.rs    # 多渠道分发器
│   ├── builder.rs       # 自动配置构建器（从 openclaw.json 检测渠道）
│   ├── urgency.rs       # Urgency 分类（HIGH/MEDIUM/LOW）
│   ├── payload.rs       # PayloadBuilder（结构化 JSON payload）
│   ├── terminal_cleaner.rs # 终端输出清理（30+ 噪音模式过滤）
│   ├── formatter.rs     # MessageFormatter（消息格式化）
│   └── channels/        # 渠道实现
│       ├── mod.rs
│       ├── openclaw_message.rs  # 通用 OpenClaw 渠道
│       └── dashboard.rs         # Dashboard 渠道
├── agent.rs             # Agent 管理（启动、停止、列表）
├── tmux.rs              # Tmux 会话操作
├── input_detector.rs    # 终端输入模式检测（20+ 种模式）
├── session.rs           # Claude Code 会话管理
├── team_discovery.rs    # Agent Teams 发现
├── task_list.rs         # Task List 集成
├── team_bridge.rs       # Agent Teams 桥接
├── inbox_watcher.rs     # Inbox 监控
├── team_orchestrator.rs # Team 编排
├── conversation_state.rs # 对话状态管理
├── notification_summarizer.rs # 智能通知汇总
└── mcp.rs               # MCP Server 实现
```

## 构建

```bash
# Debug 构建
cargo build

# Release 构建
cargo build --release

# 构建后二进制位置
./target/release/cam
```

## 运行测试

```bash
# 运行所有测试
cargo test

# 运行测试（顺序执行，避免 tmux 冲突）
cargo test -- --test-threads=1

# 运行特定模块测试
cargo test --lib openclaw_notifier
cargo test --lib team_bridge
cargo test --lib inbox_watcher
cargo test --lib team_orchestrator
cargo test --lib conversation_state
cargo test --lib notification_summarizer
```

## 更新插件二进制

修改代码后，需要更新插件目录的二进制文件：

```bash
cargo build --release
cp target/release/cam plugins/cam/bin/cam
openclaw gateway restart
```

**重要**：修改代码并重新构建后，必须重启 watcher daemon，否则运行中的进程仍使用旧代码。

```bash
# 重启 watcher
kill $(cat ~/.claude-monitor/watcher.pid) 2>/dev/null
# watcher 会在下次 agent 启动时自动启动
```

## 添加新事件类型

1. 在 `notification/urgency.rs` 的 `get_urgency()` 中添加 urgency 分类
2. 在 `notification/formatter.rs` 的 `format_event()` 中添加消息格式化
3. 在 `notification/payload.rs` 的 `build_event_object()` 中添加结构化 payload
4. 在 `notification/payload.rs` 的 `generate_summary()` 中添加摘要生成
5. 在 `main.rs` 的 `needs_snapshot` 中决定是否需要终端快照
6. 添加对应的单元测试

## 添加新通知渠道

1. 在 `notification/channels/` 下创建新文件（如 `my_channel.rs`）
2. 实现 `NotificationChannel` trait
3. 在 `notification/channels/mod.rs` 中导出
4. 在 `notification/builder.rs` 的 `build()` 中添加自动检测逻辑
5. 添加对应的单元测试

示例：

```rust
use crate::notification::{NotificationChannel, NotificationMessage, SendResult};

pub struct MyChannel { /* config */ }

impl NotificationChannel for MyChannel {
    fn name(&self) -> &str { "my-channel" }
    fn should_send(&self, msg: &NotificationMessage) -> bool { true }
    fn send(&self, msg: &NotificationMessage) -> Result<SendResult> { /* ... */ }
    fn send_async(&self, msg: &NotificationMessage) -> Result<()> { /* ... */ }
}

// 注册到 dispatcher
dispatcher.register_channel(Arc::new(MyChannel::new(config)));
```

## 通知系统架构

```
Claude Code Hook / Watcher Daemon
       │
       ▼
  cam notify
       │
       ▼
┌──────────────────────────────────────────────────────────┐
│                    OpenclawNotifier                       │
│  ┌─────────────────┐  ┌─────────────────┐                │
│  │ MessageFormatter │  │  PayloadBuilder │                │
│  └─────────────────┘  └─────────────────┘                │
│           │                    │                          │
│           ▼                    ▼                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │              NotificationDispatcher                  │ │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │ │
│  │  │  Telegram   │  │  Dashboard  │  │  WhatsApp   │  │ │
│  │  │  Channel    │  │   Channel   │  │  Channel    │  │ │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  │ │
│  └─────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
       │
       ├─── HIGH/MEDIUM ──▶ Dashboard (system event) + Channel (message send)
       │
       └─── LOW ──────────▶ 静默（不发送）
```

**核心组件**：

| 组件 | 文件 | 职责 |
|------|------|------|
| `NotificationChannel` | `notification/channel.rs` | 渠道 trait，定义 `send()` / `send_async()` |
| `NotificationDispatcher` | `notification/dispatcher.rs` | 管理多渠道，路由消息 |
| `NotificationBuilder` | `notification/builder.rs` | 从 `~/.openclaw/openclaw.json` 自动检测渠道 |
| `MessageFormatter` | `notification/formatter.rs` | 格式化不同事件类型的消息 |
| `PayloadBuilder` | `notification/payload.rs` | 构建结构化 JSON payload |
| `Urgency` | `notification/urgency.rs` | 事件紧急程度分类 |
| `terminal_cleaner` | `notification/terminal_cleaner.rs` | 终端输出噪音过滤（30+ 模式） |

## Hook API 数据源

| 数据源 | 可用性 | 说明 |
|--------|--------|------|
| **Hook stdin** | 最佳 | PermissionRequest 包含完整 tool_name + tool_input |
| **终端快照** | 必需 | idle_prompt 必须用终端快照获取当前问题 |
| **CLI 命令** | 不可用 | 无状态查询命令 |
| **JSONL** | 不适用 | 历史记录，非实时状态 |
| **MCP** | 不可用 | 独立实例，无法访问运行中会话 |
| **环境变量** | 有限 | 只有基础信息，详情在 stdin |

### Hook stdin 数据结构

**PermissionRequest** - 包含完整工具信息：
```json
{
  "tool_name": "Bash",
  "tool_input": {"command": "npm install"},
  "cwd": "/workspace"
}
```

**Notification (idle_prompt)** - 只有通用消息：
```json
{
  "notification_type": "idle_prompt",
  "message": "Claude is ready for input",
  "cwd": "/workspace"
}
```

### 终端快照策略

| 事件类型 | 需要终端快照 | 原因 |
|----------|-------------|------|
| permission_request | 否 | stdin 已有完整 tool_name + tool_input |
| notification (idle_prompt) | 是 | stdin 无问题内容，必须从终端获取 |
| notification (permission_prompt) | 否 | stdin 已有完整信息 |
| stop/session_end | 是 | 需要最终状态上下文 |

### 终端快照行数配置

| 模块 | 函数 | 行数 | 说明 |
|------|------|------|------|
| agent_watcher.rs | capture_pane (poll) | 50 | 轮询检测时获取终端输出 |
| agent_watcher.rs | capture_pane (snapshot) | 50 | 获取 agent 状态快照 |
| input_detector.rs | get_last_lines | 30 | 模式匹配时提取上下文 |
| main.rs | get_logs (notify) | 30 | Hook 触发时获取终端快照 |

## Payload 格式

HIGH/MEDIUM urgency 事件发送结构化 JSON payload：

```json
{
  "type": "cam_notification",
  "version": "1.0",
  "urgency": "HIGH",
  "event_type": "permission_request",
  "agent_id": "cam-xxx",
  "project": "/path/to/project",
  "summary": "请求执行 Bash 工具",
  "event": { "tool_name": "Bash", "tool_input": {...} },
  "timestamp": "2026-02-08T00:00:00Z"
}
```

## 会话类型

| 类型 | agent_id 格式 | 来源 | 通知 | 远程回复 |
|------|--------------|------|------|---------|
| CAM 管理 | `cam-xxxxxxxx` | 通过 CAM 启动 | 发送 | 支持 |
| 外部会话 | `ext-xxxxxxxx` | 直接运行 `claude` | 过滤 | 不支持 |

**外部会话说明**：
- 用户直接在终端运行 `claude` 产生的会话
- CAM 自动注册为 `ext-{session_id前8位}`
- 不发送通知（因为无法远程回复，通知只会造成打扰）
- 用户需要在终端直接操作
