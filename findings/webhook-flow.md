# CAM → OpenClaw Webhook 数据流分析

## 1. CAM 端数据结构

### 1.1 NotificationEvent (内部事件)

位置: `src/notification/event.rs`

```rust
pub struct NotificationEvent {
    pub agent_id: String,
    pub event_type: NotificationEventType,
    pub project_path: Option<String>,
    pub terminal_snapshot: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub dedup_key: Option<String>,
    pub skip_dedup: bool,
}

pub enum NotificationEventType {
    WaitingForInput { pattern_type: String, is_decision_required: bool },
    PermissionRequest { tool_name: String, tool_input: Value },
    Notification { notification_type: String, message: String },
    AgentExited,
    Error { message: String },
    Stop,
    SessionStart,
    SessionEnd,
}
```

### 1.2 SystemEventPayload (发送给 OpenClaw)

位置: `src/notification/system_event.rs`

```rust
pub struct SystemEventPayload {
    pub source: String,           // "cam"
    pub version: String,          // "1.0"
    pub agent_id: String,
    pub event_type: String,       // "permission_request", "waiting_for_input", etc.
    pub urgency: String,          // "HIGH", "MEDIUM", "LOW"
    pub project_path: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub event_data: EventData,
    pub context: EventContext,
}

pub struct EventContext {
    pub terminal_snapshot: Option<String>,
    pub extracted_message: Option<String>,  // AI 提取的格式化消息
    pub question_fingerprint: Option<String>,
    pub risk_level: String,
}
```

### 1.3 WebhookPayload (HTTP 请求体)

位置: `src/notification/webhook.rs`

```rust
pub struct WebhookPayload {
    pub message: String,          // 格式化的消息文本
    pub name: Option<String>,     // "CAM"
    pub agent_id: Option<String>,
    pub wake_mode: Option<String>, // "now"
    pub deliver: Option<bool>,    // true
    pub channel: Option<String>,
    pub to: Option<String>,
}
```

## 2. 数据流转换链路

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              CAM 端                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  NotificationEvent                                                           │
│  ├── agent_id: "cam-abc123"                                                 │
│  ├── event_type: PermissionRequest { tool_name, tool_input }                │
│  ├── project_path: "/workspace/myapp"                                       │
│  ├── terminal_snapshot: "完整终端输出..."                                    │
│  └── timestamp                                                               │
│           │                                                                  │
│           ▼                                                                  │
│  SystemEventPayload::from_event()                                           │
│  ├── source: "cam"                                                          │
│  ├── version: "1.0"                                                         │
│  ├── agent_id: "cam-abc123"                                                 │
│  ├── event_type: "permission_request"                                       │
│  ├── urgency: "HIGH"                                                        │
│  ├── event_data: { tool_name, tool_input }                                  │
│  └── context: { terminal_snapshot, extracted_message, risk_level }          │
│           │                                                                  │
│           ▼                                                                  │
│  to_telegram_message() → 格式化文本                                          │
│           │                                                                  │
│           ▼                                                                  │
│  WebhookPayload                                                              │
│  ├── message: "⚠️ *CAM* cam-abc123\n\n执行: Bash rm -rf...\n\n风险: 🔴 HIGH" │
│  ├── name: "CAM"                                                            │
│  ├── agent_id: "cam-abc123"                                                 │
│  ├── wake_mode: "now"                                                       │
│  └── deliver: true                                                          │
│           │                                                                  │
│           │  + raw_event_json (对于 permission_request/waiting_for_input)   │
│           │                                                                  │
└───────────┼─────────────────────────────────────────────────────────────────┘
            │
            │ POST /hooks/agent
            │ Authorization: Bearer {hook_token}
            │
            ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           OpenClaw Gateway                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  接收 WebhookPayload:                                                        │
│  {                                                                           │
│    "message": "⚠️ *CAM* cam-abc123\n\n...\n\n---\nraw_event_json:\n```json  │
│               {完整 SystemEventPayload JSON}```",                            │
│    "name": "CAM",                                                           │
│    "agent_id": "cam-abc123",                                                │
│    "wake_mode": "now",                                                      │
│    "deliver": true                                                          │
│  }                                                                           │
│           │                                                                  │
│           ▼                                                                  │
│  触发 OpenClaw Agent 对话                                                    │
│  - 唤醒 Agent (wake_mode: "now")                                            │
│  - 将 message 作为系统消息注入                                               │
│  - Agent 加载 cam-notify Skill 处理                                         │
│                                                                              │
└───────────┼─────────────────────────────────────────────────────────────────┘
            │
            ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           OpenClaw Agent                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  cam-notify Skill 处理:                                                      │
│  1. 解析 raw_event_json 获取结构化数据                                       │
│  2. 根据 event_type 和 risk_level 决策:                                      │
│     - 白名单命令 → 自动批准 (cam_agent_send "y")                             │
│     - 黑名单命令 → 发送通知给用户                                            │
│     - 其他 → LLM 判断风险                                                    │
│  3. 格式化消息发送到 Telegram                                                │
│                                                                              │
└───────────┼─────────────────────────────────────────────────────────────────┘
            │
            ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              用户端                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Telegram 消息:                                                              │
│  ┌────────────────────────────────────────┐                                 │
│  │ ⚠️ *CAM* cam-abc123                    │                                 │
│  │                                         │                                 │
│  │ 执行: Bash rm -rf /tmp/test            │                                 │
│  │                                         │                                 │
│  │ 风险: 🔴 HIGH                           │                                 │
│  │                                         │                                 │
│  │ 回复 y 允许 / n 拒绝                    │                                 │
│  └────────────────────────────────────────┘                                 │
│                                                                              │
│  用户回复: "y"                                                               │
│           │                                                                  │
│           ▼                                                                  │
│  OpenClaw Agent 调用 cam_agent_send("cam-abc123", "y")                      │
│           │                                                                  │
│           ▼                                                                  │
│  CAM MCP → tmux send-keys -t cam-abc123 -l "y" && tmux send-keys Enter      │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 3. 数据丢失点分析

### 3.1 terminal_snapshot 截断

**位置**: `src/notification/system_event.rs:218-228`

```rust
// Fallback: 截取终端最后 30 行
let snapshot_tail = self.context.terminal_snapshot.as_ref().map(|snapshot| {
    let lines: Vec<&str> = snapshot.lines().collect();
    let start = lines.len().saturating_sub(30);
    lines[start..].join("\n")
});
```

**问题**: 终端快照被截断到最后 30 行，可能丢失重要上下文。

**影响**: 用户看到的消息可能缺少问题的完整背景。

### 3.2 raw_event_json 截断

**位置**: `src/notification/openclaw.rs:448-457`

```rust
let max_chars = 3500usize;
let raw_trunc: String = raw.chars().take(max_chars).collect();
msg.push_str(&raw_trunc);
if raw.len() > max_chars {
    msg.push_str("\n... (truncated)");
}
```

**问题**: raw_event_json 被截断到 3500 字符。

**影响**: 大型 tool_input（如长命令或大文件路径）可能被截断。

### 3.3 AI 提取失败回退

**位置**: `src/notification/openclaw.rs:312-334`

```rust
match extract_formatted_message(snapshot) {
    SimpleExtractionResult::Message { message, fingerprint } => {
        payload.set_extracted_message(message, fingerprint);
    }
    SimpleExtractionResult::Idle { .. } => {
        // 检测到 idle 状态，不设置 extracted_message
    }
    SimpleExtractionResult::Failed => {
        warn!(agent_id = %agent_id, "AI extraction failed, using fallback");
        // 不设置 extracted_message，使用 terminal_snapshot 回退
    }
}
```

**问题**: AI 提取失败时，用户看到的是原始终端输出而非格式化消息。

**影响**: 用户体验下降，需要自己解析终端输出。

### 3.4 project_path 可能为空

**位置**: `src/notification/event.rs:78-83`

```rust
pub fn project_name(&self) -> &str {
    self.project_path
        .as_ref()
        .and_then(|p| p.rsplit('/').next())
        .unwrap_or(&self.agent_id)
}
```

**问题**: 如果 project_path 未设置，回退到 agent_id。

**影响**: 用户可能无法识别是哪个项目的通知。

## 4. 数据格式对比

### 4.1 CAM 发送的完整数据

```json
{
  "source": "cam",
  "version": "1.0",
  "agent_id": "cam-abc123",
  "event_type": "permission_request",
  "urgency": "HIGH",
  "project_path": "/workspace/myapp",
  "timestamp": "2026-02-25T10:00:00Z",
  "event_data": {
    "tool_name": "Bash",
    "tool_input": {
      "command": "rm -rf /tmp/test"
    }
  },
  "context": {
    "terminal_snapshot": "$ rm -rf /tmp/test\n[等待确认]",
    "extracted_message": "执行: rm -rf /tmp/test\n\n确认删除 /tmp/test 目录？",
    "question_fingerprint": "abc123",
    "risk_level": "HIGH"
  }
}
```

### 4.2 OpenClaw Skill 期望的数据

根据 `skills/cam-notify/SKILL.md`:

```json
{
  "source": "cam",
  "version": "1.0",
  "agent_id": "cam-xxx",
  "event_type": "permission_request",
  "urgency": "HIGH",
  "project_path": "/path/to/project",
  "timestamp": "2026-02-18T10:00:00Z",
  "event_data": {
    "tool_name": "Bash",
    "tool_input": {"command": "npm install express"}
  },
  "context": {
    "terminal_snapshot": "...",
    "risk_level": "MEDIUM"
  }
}
```

### 4.3 差异分析

| 字段 | CAM 发送 | Skill 期望 | 状态 |
|------|----------|------------|------|
| source | ✅ "cam" | ✅ "cam" | 匹配 |
| version | ✅ "1.0" | ✅ "1.0" | 匹配 |
| agent_id | ✅ | ✅ | 匹配 |
| event_type | ✅ | ✅ | 匹配 |
| urgency | ✅ | ✅ | 匹配 |
| project_path | ✅ | ✅ | 匹配 |
| timestamp | ✅ | ✅ | 匹配 |
| event_data | ✅ | ✅ | 匹配 |
| context.terminal_snapshot | ✅ | ✅ | 匹配 |
| context.risk_level | ✅ | ✅ | 匹配 |
| context.extracted_message | ✅ | ❌ 未文档化 | **新增字段** |
| context.question_fingerprint | ✅ | ❌ 未文档化 | **新增字段** |

## 5. 改进建议

### 5.1 更新 Skill 文档

`skills/cam-notify/SKILL.md` 需要更新以包含新字段:

```json
{
  "context": {
    "terminal_snapshot": "...",
    "extracted_message": "AI 提取的格式化消息",
    "question_fingerprint": "用于去重的指纹",
    "risk_level": "MEDIUM"
  }
}
```

### 5.2 增加 terminal_snapshot 行数

当前截断到 30 行可能不够，建议:
- 对于 permission_request: 保留 50 行
- 对于 waiting_for_input: 保留 80 行（可能包含长问题）

### 5.3 优化 raw_event_json 截断

3500 字符可能不够，建议:
- 增加到 6000 字符
- 或者只截断 terminal_snapshot，保留其他字段完整

### 5.4 添加 AI 提取失败通知

当 AI 提取失败时，在消息中明确告知用户:

```
⚠️ 无法解析通知内容，请查看终端

[原始终端输出]
```

## 6. 完整数据流图

```
┌──────────────────────────────────────────────────────────────────────────────────────┐
│                                    数据流概览                                         │
└──────────────────────────────────────────────────────────────────────────────────────┘

  Watcher 检测                    CAM 处理                      Gateway                 用户
      │                              │                            │                      │
      │  终端状态变化                │                            │                      │
      ├─────────────────────────────►│                            │                      │
      │                              │                            │                      │
      │                              │  NotificationEvent         │                      │
      │                              │  ├─ agent_id               │                      │
      │                              │  ├─ event_type             │                      │
      │                              │  ├─ terminal_snapshot      │                      │
      │                              │  └─ project_path           │                      │
      │                              │         │                  │                      │
      │                              │         ▼                  │                      │
      │                              │  SystemEventPayload        │                      │
      │                              │  ├─ + urgency              │                      │
      │                              │  ├─ + risk_level           │                      │
      │                              │  └─ + extracted_message    │                      │
      │                              │         │                  │                      │
      │                              │         ▼                  │                      │
      │                              │  to_telegram_message()     │                      │
      │                              │         │                  │                      │
      │                              │         ▼                  │                      │
      │                              │  WebhookPayload            │                      │
      │                              │  ├─ message (格式化)       │                      │
      │                              │  └─ + raw_event_json       │                      │
      │                              │         │                  │                      │
      │                              │         │ POST /hooks/agent│                      │
      │                              │         ├─────────────────►│                      │
      │                              │         │                  │                      │
      │                              │         │                  │  唤醒 Agent          │
      │                              │         │                  │  加载 cam-notify     │
      │                              │         │                  │  Skill               │
      │                              │         │                  │         │            │
      │                              │         │                  │         ▼            │
      │                              │         │                  │  三层决策            │
      │                              │         │                  │  ├─ 白名单 → 自动批准│
      │                              │         │                  │  ├─ 黑名单 → 通知    │
      │                              │         │                  │  └─ LLM → 智能决策   │
      │                              │         │                  │         │            │
      │                              │         │                  │         │ Telegram   │
      │                              │         │                  │         ├───────────►│
      │                              │         │                  │         │            │
      │                              │         │                  │         │            │ 用户回复
      │                              │         │                  │         │◄───────────┤
      │                              │         │                  │         │            │
      │                              │         │                  │  cam_agent_send      │
      │                              │         │◄─────────────────┤         │            │
      │                              │         │                  │         │            │
      │  tmux send-keys              │◄────────┤                  │         │            │
      │◄─────────────────────────────┤         │                  │         │            │
      │                              │         │                  │         │            │
```

## 7. 关键代码位置

| 功能 | 文件 | 函数/结构 |
|------|------|----------|
| 内部事件定义 | `src/notification/event.rs` | `NotificationEvent`, `NotificationEventType` |
| System Event 构建 | `src/notification/system_event.rs` | `SystemEventPayload::from_event()` |
| Telegram 消息格式化 | `src/notification/system_event.rs` | `to_telegram_message()` |
| Webhook 发送 | `src/notification/webhook.rs` | `WebhookClient::send_notification_blocking()` |
| 通知调度 | `src/notification/openclaw.rs` | `OpenclawNotifier::send_system_event_only()` |
| AI 消息提取 | `src/ai/extractor.rs` | `extract_formatted_message()` |
| Urgency 计算 | `src/notification/urgency.rs` | `get_urgency()` |
| 风险评估 | `src/notification/system_event.rs` | `assess_risk_level()` |
