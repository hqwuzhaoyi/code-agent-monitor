# Notification 模块

CAM 的通知抽象层，负责将 Agent 事件转换为用户友好的通知并分发到多个渠道。

## 架构概览

```
┌─────────────────────────────────────────────────────────────────┐
│                        NotificationBuilder                       │
│                    (自动检测渠道配置)                              │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                     NotificationDispatcher                       │
│                    (路由消息到多个渠道)                            │
└─────────────────────────────────────────────────────────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│   Telegram    │     │   Dashboard   │     │   WhatsApp    │
│   Channel     │     │   Channel     │     │   Channel     │
└───────────────┘     └───────────────┘     └───────────────┘
```

## 核心组件

### 1. 事件系统 (`event.rs`)

统一的事件结构，Hook 和 Watcher 共用：

```rust
use crate::notification::{NotificationEvent, NotificationEventType};

// 创建等待输入事件
let event = NotificationEvent::waiting_for_input("cam-123", "ClaudePrompt")
    .with_project_path("/workspace/myproject")
    .with_terminal_snapshot("What would you like me to do?");

// 创建权限请求事件
let event = NotificationEvent::permission_request(
    "cam-456",
    "Bash",
    serde_json::json!({"command": "npm install"}),
);
```

### 2. 紧急程度 (`urgency.rs`)

三级紧急程度，决定通知路由：

| 级别 | 事件类型 | 行为 |
|------|----------|------|
| HIGH | permission_request, Error, WaitingForInput | 立即发送 |
| MEDIUM | AgentExited, idle_prompt | 发送 |
| LOW | session_start, stop, ToolUse | 静默 |

```rust
use crate::notification::{Urgency, get_urgency};

let urgency = get_urgency("permission_request", "{}");
assert_eq!(urgency, Urgency::High);
```

### 3. 消息格式化 (`formatter.rs`)

将事件转换为用户友好的通知消息：

```rust
use crate::notification::MessageFormatter;

let formatter = MessageFormatter::new();
let message = formatter.format_notification_event(&event);
// 输出: "⏸️ myproject 等待输入\n\n你想要实现什么功能？\n\n回复数字 (1-3)"
```

**设计原则：**
- 简洁 - 核心内容不超过 5 行
- 可操作 - 明确告诉用户怎么做
- 专业 - 现代机器人风格
- 友好 ID - 用项目名替代 `cam-xxxxxxxxxx`
- 无硬编码 - 使用 AI 判断，兼容多种 AI 编码工具

### 4. 去重器 (`deduplicator.rs`)

防止短时间内发送重复通知：

```rust
use crate::notification::NotificationDeduplicator;

let mut dedup = NotificationDeduplicator::new();

// 第一次发送
assert!(dedup.should_send("cam-123", "你想要实现什么功能？"));

// 相似内容在 120 秒内被去重
assert!(!dedup.should_send("cam-123", "你想要实现什么功能"));
```

**去重策略：**
- 提取核心问题内容（忽略 reply_hint 变化）
- 120 秒时间窗口
- 相似度 > 80% 视为重复
- 状态持久化到 `~/.config/code-agent-monitor/dedup_state.json`

### 5. 渠道系统 (`channel.rs`, `channels/`)

所有渠道实现 `NotificationChannel` trait：

```rust
pub trait NotificationChannel: Send + Sync {
    fn name(&self) -> &str;
    fn should_send(&self, message: &NotificationMessage) -> bool;
    fn send(&self, message: &NotificationMessage) -> Result<SendResult>;
    fn send_async(&self, message: &NotificationMessage) -> Result<()>;
}
```

**内置渠道：**
- `OpenclawMessageChannel` - 通用渠道（Telegram/WhatsApp/Discord/Slack/Signal）
- `DashboardChannel` - 结构化 payload 发送到 Dashboard

### 6. 构建器 (`builder.rs`)

自动检测 OpenClaw 配置并构建 Dispatcher：

```rust
use crate::notification::NotificationBuilder;

let dispatcher = NotificationBuilder::new()
    .min_urgency(Urgency::Medium)
    .dry_run(false)
    .build()?;

dispatcher.send_async(&message)?;
```

### 7. Payload 构建 (`payload.rs`)

创建结构化 JSON payload，用于 Dashboard：

```json
{
  "type": "cam_notification",
  "version": "1.0",
  "urgency": "HIGH",
  "event_type": "permission_request",
  "agent_id": "cam-xxx",
  "project": "/path/to/project",
  "event": { "tool_name": "Bash", "tool_input": {...} },
  "summary": "请求执行 Bash 工具",
  "risk_level": "MEDIUM",
  "timestamp": "2026-02-08T00:00:00Z"
}
```

### 8. 终端状态检测 (`terminal_cleaner.rs`)

使用 AI 判断 agent 是否正在处理中：

```rust
use crate::notification::is_processing;

if is_processing(terminal_content) {
    // agent 正在处理，不发送通知
}
```

## 使用示例

### 完整流程

```rust
use crate::notification::{
    NotificationBuilder, NotificationMessage, NotificationEvent,
    MessageFormatter, PayloadBuilder, Urgency, get_urgency,
};

// 1. 创建事件
let event = NotificationEvent::permission_request(
    "cam-123",
    "Bash",
    serde_json::json!({"command": "rm -rf /tmp/test"}),
).with_project_path("/workspace/myproject");

// 2. 格式化消息
let formatter = MessageFormatter::new();
let content = formatter.format_notification_event(&event);

// 3. 构建 payload
let payload_builder = PayloadBuilder::new();
let payload = payload_builder.create_payload(
    "cam-123",
    "permission_request",
    "",
    r#"{"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/test"}}"#,
    Urgency::High,
);

// 4. 创建消息
let message = NotificationMessage::new(content, Urgency::High)
    .with_agent_id("cam-123")
    .with_payload(payload);

// 5. 发送
let dispatcher = NotificationBuilder::new().build()?;
dispatcher.send_async(&message)?;
```

### 便捷函数

```rust
use crate::notification::builder::send_notification;

send_notification(
    "⏸️ myproject 等待输入",
    Urgency::High,
    Some("cam-123"),
    None,
)?;
```

## 风险评估

`NotificationSummarizer` 提供智能风险评估：

| 风险等级 | 示例 | Emoji |
|----------|------|-------|
| Low | `ls`, `/tmp/` 路径, 读操作 | ✅ |
| Medium | `npm install`, 项目文件写入 | ⚠️ |
| High | `rm -rf`, `sudo`, `/etc/` 路径 | 🔴 |

## 文件结构

```
src/notification/
├── mod.rs              # 模块导出
├── event.rs            # 统一事件结构
├── urgency.rs          # 紧急程度分类
├── channel.rs          # 渠道 trait 定义
├── dispatcher.rs       # 消息分发器
├── builder.rs          # 自动配置构建器
├── formatter.rs        # 消息格式化
├── payload.rs          # Payload 构建
├── deduplicator.rs     # 通知去重
├── terminal_cleaner.rs # 终端状态检测
└── channels/
    ├── mod.rs
    ├── openclaw_message.rs  # 通用 OpenClaw 渠道
    ├── dashboard.rs         # Dashboard 渠道
    ├── telegram.rs          # (遗留) Telegram 渠道
    └── whatsapp.rs          # (遗留) WhatsApp 渠道
```

## 设计原则

1. **无硬编码** - 使用 Haiku API 进行智能判断，兼容多种 AI 编码工具
2. **渠道解耦** - 每个渠道独立实现，互不影响
3. **异步发送** - 所有渠道支持异步发送，不阻塞调用方
4. **持久化去重** - 跨进程调用也能正确去重
5. **优雅降级** - AI 提取失败时显示简洁提示，不泄露终端 UI 元素
