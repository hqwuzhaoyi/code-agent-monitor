# Adapter 事件映射分析报告

## 概述

本报告分析 Claude Code、Codex CLI、OpenCode 三个 Agent CLI 的事件系统，评估当前 CAM 的 `NotificationEventType` 是否能够统一映射这些事件。

## 1. 三个 CLI 的事件语义对比

### Claude Code Hooks

| 事件 | 语义 | 触发时机 |
|------|------|----------|
| `session_start` | 会话开始 | Agent 启动时 |
| `stop` | 会话停止 | Agent 退出时 |
| `notification` | 通知 | 各种通知（idle_prompt, permission_prompt 等） |
| `PreToolUse` | 工具调用前 | 执行工具前，可用于权限检查 |
| `PostToolUse` | 工具调用后 | 执行工具后 |

### Codex CLI notify

| 事件 | 语义 | 触发时机 |
|------|------|----------|
| `agent-turn-complete` | 轮次完成 | Agent 完成一轮处理 |

Codex 的事件系统非常简单，只有一个统一的 `notify` 配置项，通过 JSON payload 传递事件信息。

### OpenCode Plugin Events

| 类别 | 事件 | 语义 |
|------|------|------|
| **Session** | `session.created` | 会话创建 |
| | `session.idle` | 会话空闲（等待输入） |
| | `session.status` | 状态变化 |
| | `session.error` | 会话错误 |
| | `session.compacted` | 上下文压缩 |
| | `session.deleted` | 会话删除 |
| | `session.diff` | 会话差异 |
| | `session.updated` | 会话更新 |
| **Message** | `message.updated` | 消息更新 |
| | `message.removed` | 消息删除 |
| | `message.part.updated` | 消息部分更新 |
| | `message.part.removed` | 消息部分删除 |
| **Tool** | `tool.execute.before` | 工具执行前 |
| | `tool.execute.after` | 工具执行后 |
| **Permission** | `permission.asked` | 请求权限 |
| | `permission.replied` | 权限回复 |
| **File** | `file.edited` | 文件编辑 |
| | `file.watcher.updated` | 文件监控更新 |
| **TUI** | `tui.prompt.append` | 提示追加 |
| | `tui.command.execute` | 命令执行 |
| | `tui.toast.show` | Toast 显示 |
| **其他** | `command.executed` | 命令执行完成 |
| | `server.connected` | 服务器连接 |
| | `todo.updated` | Todo 更新 |
| | `lsp.*` | LSP 相关事件 |

## 2. 当前 NotificationEventType 覆盖分析

当前 CAM 定义的事件类型（`src/notification/event.rs`）：

```rust
pub enum NotificationEventType {
    WaitingForInput { pattern_type, is_decision_required },
    PermissionRequest { tool_name, tool_input },
    Notification { notification_type, message },
    AgentExited,
    Error { message },
    Stop,
    SessionStart,
    SessionEnd,
}
```

### 映射覆盖表

| CAM 事件类型 | Claude Code | Codex CLI | OpenCode |
|-------------|-------------|-----------|----------|
| `SessionStart` | ✅ session_start | ❌ 无对应 | ✅ session.created |
| `SessionEnd` | ✅ stop | ❌ 无对应 | ✅ session.deleted |
| `Stop` | ✅ stop | ❌ 无对应 | ❌ 无对应 |
| `AgentExited` | ✅ (tmux 检测) | ✅ (tmux 检测) | ✅ (tmux 检测) |
| `WaitingForInput` | ✅ notification(idle_prompt) | ✅ agent-turn-complete | ✅ session.idle |
| `PermissionRequest` | ✅ PreToolUse | ❌ 无对应 | ✅ permission.asked |
| `Error` | ✅ (JSONL 解析) | ❌ 无对应 | ✅ session.error |
| `Notification` | ✅ notification | ❌ 无对应 | ✅ tui.toast.show |

### 未覆盖的重要事件

| 事件 | 来源 | 重要性 | 建议 |
|------|------|--------|------|
| `tool.execute.before/after` | OpenCode | 中 | 可映射到 `ToolUse` |
| `permission.replied` | OpenCode | 高 | 需要新增 `PermissionReplied` |
| `session.compacted` | OpenCode | 低 | 可忽略或映射到 `Notification` |
| `file.edited` | OpenCode | 中 | 可映射到 `ToolUse(Edit)` |
| `message.*` | OpenCode | 低 | 可忽略 |

## 3. 事件映射语义一致性分析

### 语义一致的映射

| 统一事件 | 语义 | 一致性 |
|----------|------|--------|
| `SessionStart` | Agent 开始工作 | ✅ 高度一致 |
| `AgentExited` | Agent 进程退出 | ✅ 高度一致 |
| `Error` | 发生错误 | ✅ 高度一致 |

### 语义存在差异的映射

| 统一事件 | 差异说明 |
|----------|----------|
| `WaitingForInput` | Claude Code 通过 `idle_prompt` 通知，Codex 通过 `agent-turn-complete`，OpenCode 通过 `session.idle`。语义相近但触发条件可能不同。 |
| `PermissionRequest` | Claude Code 在 `PreToolUse` 时触发，OpenCode 有专门的 `permission.asked`。Codex 无此概念。 |
| `Stop` vs `SessionEnd` | 当前有两个相似事件，语义重叠。建议合并。 |

### 语义不一致的风险

1. **WaitingForInput 的判断标准不同**
   - Claude Code: 依赖 `idle_prompt` 通知或 AI 检测终端状态
   - Codex: `agent-turn-complete` 表示轮次完成，不一定等待输入
   - OpenCode: `session.idle` 明确表示空闲状态

2. **PermissionRequest 的粒度不同**
   - Claude Code: 每个工具调用前都可能触发
   - OpenCode: 只有需要权限时才触发 `permission.asked`
   - Codex: 无权限请求概念，使用 `approval_policy` 配置

## 4. 建议新增的事件类型

### 高优先级

```rust
/// 权限回复（用户已响应权限请求）
PermissionReplied {
    tool_name: String,
    approved: bool,
}

/// 工具执行（统一的工具调用事件）
ToolExecuted {
    tool_name: String,
    tool_input: Value,
    success: bool,
    duration_ms: Option<u64>,
}
```

### 中优先级

```rust
/// Agent 恢复（从等待状态恢复）
AgentResumed

/// 上下文压缩（长对话压缩）
ContextCompacted {
    before_tokens: u64,
    after_tokens: u64,
}
```

### 低优先级

```rust
/// 文件变更（Agent 修改了文件）
FileChanged {
    file_path: String,
    change_type: String, // "created", "modified", "deleted"
}
```

## 5. 事件优先级和重要性排序

### 需要立即通知用户（HIGH）

| 事件 | 原因 |
|------|------|
| `PermissionRequest` | 需要用户授权 |
| `WaitingForInput` | Agent 阻塞等待 |
| `Error` | 可能需要干预 |

### 需要通知但不紧急（MEDIUM）

| 事件 | 原因 |
|------|------|
| `AgentExited` | 任务可能完成或异常退出 |
| `PermissionReplied` | 状态变更确认 |
| `ToolExecuted` (失败时) | 可能需要关注 |

### 仅记录不通知（LOW）

| 事件 | 原因 |
|------|------|
| `SessionStart` | 信息性 |
| `SessionEnd` | 信息性 |
| `ToolExecuted` (成功时) | 正常流程 |
| `ContextCompacted` | 内部优化 |

## 6. 统一 Adapter 设计建议

### 事件转换层

```rust
pub trait AgentEventAdapter {
    /// 将 Agent 特定事件转换为统一事件
    fn to_notification_event(&self, raw_event: &RawEvent) -> Option<NotificationEvent>;

    /// 获取 Agent 类型
    fn agent_type(&self) -> AgentType;
}

pub enum AgentType {
    ClaudeCode,
    CodexCli,
    OpenCode,
}
```

### Claude Code Adapter

```rust
impl AgentEventAdapter for ClaudeCodeAdapter {
    fn to_notification_event(&self, raw: &RawEvent) -> Option<NotificationEvent> {
        match raw.event_name.as_str() {
            "session_start" => Some(NotificationEvent::session_start(raw.agent_id)),
            "stop" => Some(NotificationEvent::session_end(raw.agent_id)),
            "notification" => self.map_notification(raw),
            "PreToolUse" => self.map_pre_tool_use(raw),
            _ => None,
        }
    }
}
```

### Codex CLI Adapter

```rust
impl AgentEventAdapter for CodexCliAdapter {
    fn to_notification_event(&self, raw: &RawEvent) -> Option<NotificationEvent> {
        // Codex 只有 notify 事件，需要解析 payload
        let payload: CodexPayload = serde_json::from_value(raw.data.clone()).ok()?;

        match payload.event_type.as_str() {
            "agent-turn-complete" => {
                // 判断是否真的在等待输入
                if payload.waiting_for_input {
                    Some(NotificationEvent::waiting_for_input(raw.agent_id, "TurnComplete"))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
```

### OpenCode Adapter

```rust
impl AgentEventAdapter for OpenCodeAdapter {
    fn to_notification_event(&self, raw: &RawEvent) -> Option<NotificationEvent> {
        match raw.event_name.as_str() {
            "session.created" => Some(NotificationEvent::session_start(raw.agent_id)),
            "session.idle" => Some(NotificationEvent::waiting_for_input(raw.agent_id, "SessionIdle")),
            "session.error" => Some(NotificationEvent::error(raw.agent_id, raw.message())),
            "permission.asked" => self.map_permission_asked(raw),
            "permission.replied" => self.map_permission_replied(raw),
            "tool.execute.after" => self.map_tool_executed(raw),
            _ => None,
        }
    }
}
```

## 7. 结论

### 当前 NotificationEventType 评估

- **覆盖率**: 约 70%，能处理核心场景
- **语义一致性**: 中等，部分事件需要适配层转换
- **扩展性**: 良好，enum 设计便于添加新类型

### 建议的改进

1. **合并 `Stop` 和 `SessionEnd`** - 语义重叠，保留 `SessionEnd`
2. **新增 `PermissionReplied`** - 支持 OpenCode 的权限回复事件
3. **新增 `ToolExecuted`** - 统一工具执行事件（当前 `WatchEvent::ToolUse` 未映射到 `NotificationEventType`）
4. **新增 `AgentResumed`** - 当前只在 `WatchEvent` 中有，未映射到通知事件

### 实现优先级

1. 实现 Adapter trait 和三个具体实现
2. 新增 `PermissionReplied` 和 `ToolExecuted` 事件类型
3. 统一 `WatchEvent` 和 `NotificationEventType` 的映射
4. 为每个 Adapter 编写单元测试
