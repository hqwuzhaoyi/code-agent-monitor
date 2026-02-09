# AI Processing Layer Design

## Overview

本文档设计 CAM 通知系统的 AI 处理层，让 OpenClaw Agent 智能处理 CAM 通知后再呈现给用户，而不是简单转发原始机器格式信息。

## 核心问题

当前架构：
```
CAM Hook → cam notify → openclaw message send → channel (直接)
```

问题：
1. 用户收到的是机器格式的原始信息（JSON、技术术语）
2. 没有上下文解释（为什么需要这个权限？这个错误意味着什么？）
3. 用户需要理解技术细节才能做出决策

## 目标架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                         CAM AI Processing Layer                      │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Claude Code Hook                                                    │
│        │                                                             │
│        ▼                                                             │
│   cam notify                                                         │
│        │                                                             │
│        ▼                                                             │
│  ┌──────────────────┐                                               │
│  │ OpenclawNotifier │                                               │
│  │ (路由决策)        │                                               │
│  └────────┬─────────┘                                               │
│           │                                                          │
│           ├─── HIGH urgency ──▶ gateway wake ──▶ Agent 处理 ──▶ channel │
│           │                         │                                │
│           │                         ▼                                │
│           │                   AI 解释 + 建议                         │
│           │                                                          │
│           ├─── MEDIUM urgency ─▶ gateway wake ──▶ Agent 汇总 ──▶ channel │
│           │                                                          │
│           └─── LOW urgency ────▶ 静默（或 Agent 内部记录）            │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Gateway Wake 机制

### 工作原理

`openclaw gateway call wake` 是一个一次性触发机制：
- 不进入 agent 对话上下文（避免上下文累积）
- 触发 agent 执行一次性任务
- 适合事件驱动的通知场景

### 调用方式

```bash
openclaw gateway call wake --params '{"text": "<notification_payload>", "mode": "now"}'
```

参数说明：
- `text`: 传递给 agent 的通知内容（JSON 格式）
- `mode`: `"now"` 立即触发

### Agent 响应流程

1. Gateway 收到 wake 调用
2. 解析 `text` 参数中的通知 payload
3. 触发 main agent 处理
4. Agent 根据 payload 中的元数据决定如何处理
5. Agent 通过 `--deliver` 或 `openclaw message send` 发送处理后的消息

## 通知 Payload 设计

### 结构化 Payload

```json
{
  "type": "cam_notification",
  "version": "1.0",
  "metadata": {
    "urgency": "HIGH",
    "event_type": "permission_request",
    "agent_id": "cam-abc123",
    "timestamp": "2024-02-08T10:30:00Z"
  },
  "event": {
    "tool_name": "Bash",
    "tool_input": {
      "command": "rm -rf /tmp/test"
    },
    "cwd": "/Users/admin/workspace/myproject"
  },
  "context": {
    "terminal_snapshot": "$ cargo build\n   Compiling...\n   Finished",
    "project_name": "myproject",
    "recent_actions": ["Read file.rs", "Edit main.rs"]
  }
}
```

### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `type` | string | 固定为 `"cam_notification"` |
| `version` | string | Payload 版本号 |
| `metadata.urgency` | string | `HIGH` / `MEDIUM` / `LOW` |
| `metadata.event_type` | string | 事件类型 |
| `metadata.agent_id` | string | CAM agent ID |
| `metadata.timestamp` | string | ISO 8601 时间戳 |
| `event` | object | 事件具体内容（因事件类型而异） |
| `context` | object | 上下文信息 |

## AI 处理场景

### 1. 权限请求 (permission_request)

**输入**:
```json
{
  "event_type": "permission_request",
  "event": {
    "tool_name": "Bash",
    "tool_input": {"command": "rm -rf /tmp/test-cache"}
  }
}
```

**AI 处理后输出**:
```
🔐 cam-abc123 请求执行命令

Agent 想要删除 /tmp/test-cache 目录。

📋 风险评估：低
- 这是临时目录中的缓存文件
- 不会影响项目源代码
- 可能是清理构建缓存

💡 建议：可以允许

回复选项：
• 1 = 允许这次
• 2 = 允许并记住
• 3 = 拒绝
```

### 2. 错误 (Error)

**输入**:
```json
{
  "event_type": "Error",
  "event": {
    "message": "API rate limit exceeded"
  },
  "context": {
    "terminal_snapshot": "Error: 429 Too Many Requests..."
  }
}
```

**AI 处理后输出**:
```
❌ cam-abc123 遇到错误

问题：API 请求频率超限

🔍 分析：
Agent 在短时间内发送了太多 API 请求，触发了速率限制。

💡 建议：
1. 等待几分钟后重试
2. 检查是否有循环调用
3. 考虑添加请求间隔

需要我帮你处理吗？
```

### 3. 等待输入 (WaitingForInput)

**输入**:
```json
{
  "event_type": "WaitingForInput",
  "event": {
    "pattern_type": "Confirmation",
    "prompt": "Delete /Users/admin/important.txt? [Y/n]"
  }
}
```

**AI 处理后输出**:
```
⏸️ cam-abc123 等待确认

Agent 询问是否删除文件：
/Users/admin/important.txt

⚠️ 注意：这个文件在用户主目录下，不是临时文件。

请确认：
• Y = 确认删除
• N = 取消操作
```

### 4. Agent 退出 (AgentExited)

**输入**:
```json
{
  "event_type": "AgentExited",
  "event": {
    "project_path": "/Users/admin/workspace/myproject"
  },
  "context": {
    "terminal_snapshot": "✓ All tests passed\n✓ Build successful"
  }
}
```

**AI 处理后输出**:
```
✅ cam-abc123 已完成

项目：myproject

📊 执行摘要：
- 所有测试通过
- 构建成功

需要启动新任务吗？
```

## Agent Prompt 设计

### System Prompt 扩展

在 OpenClaw main agent 的 system prompt 中添加：

```markdown
## CAM 通知处理

当收到 `type: "cam_notification"` 的 wake 消息时，你需要：

1. **解析通知**：提取 metadata 和 event 信息
2. **评估风险**：根据 event_type 和具体内容评估
3. **生成解释**：用自然语言解释发生了什么
4. **提供建议**：给出操作建议
5. **发送到 channel**：使用 `openclaw message send` 发送处理后的消息

### 风险评估指南

| 操作类型 | 低风险 | 中风险 | 高风险 |
|---------|--------|--------|--------|
| 文件删除 | /tmp, cache | 项目内文件 | 系统文件、用户目录 |
| 命令执行 | ls, cat, echo | npm, cargo | rm -rf, sudo |
| 网络请求 | GET 请求 | POST 请求 | 敏感 API |

### 输出格式

使用 emoji 标识紧急程度：
- 🔐 权限请求
- ❌ 错误
- ⏸️ 等待输入
- ✅ 完成
- 📢 一般通知

保持消息简洁，重点突出：
- 第一行：状态 + agent ID
- 中间：解释和分析
- 最后：操作选项或建议
```

## 实现步骤

### Phase 1: Payload 标准化

1. 修改 `OpenclawNotifier::send_event()` 生成结构化 payload
2. 添加 `build_notification_payload()` 方法
3. 更新 `send_via_gateway_wake()` 使用新 payload 格式

```rust
// src/openclaw_notifier.rs

fn build_notification_payload(
    &self,
    agent_id: &str,
    event_type: &str,
    pattern_or_path: &str,
    context: &str,
) -> serde_json::Value {
    let urgency = Self::get_urgency(event_type, context);

    serde_json::json!({
        "type": "cam_notification",
        "version": "1.0",
        "metadata": {
            "urgency": urgency,
            "event_type": event_type,
            "agent_id": agent_id,
            "timestamp": chrono::Utc::now().to_rfc3339()
        },
        "event": self.parse_event_data(event_type, pattern_or_path, context),
        "context": self.extract_context(context)
    })
}
```

### Phase 2: Gateway Wake 集成

1. 修改 `send_via_gateway_wake()` 发送结构化 payload
2. 添加错误处理和重试逻辑
3. 添加 dry-run 支持

```rust
fn send_via_gateway_wake(&self, payload: &serde_json::Value) -> Result<()> {
    let params = serde_json::json!({
        "text": payload.to_string(),
        "mode": "now"
    });

    // ... 执行 gateway call wake
}
```

### Phase 3: Agent Prompt 配置

1. 创建 CAM 通知处理的 prompt 模板
2. 配置到 OpenClaw agent 的 system prompt
3. 测试各种事件类型的处理效果

### Phase 4: 回退机制

1. 如果 gateway wake 失败，回退到直接发送
2. 添加超时处理
3. 记录处理日志

## 配置选项

### 环境变量

```bash
# 启用 AI 处理层
CAM_AI_PROCESSING=true

# AI 处理超时（秒）
CAM_AI_TIMEOUT=30

# 回退到直接发送
CAM_FALLBACK_DIRECT=true
```

### 配置文件

`~/.claude-monitor/config.json`:
```json
{
  "ai_processing": {
    "enabled": true,
    "timeout_seconds": 30,
    "fallback_to_direct": true,
    "risk_assessment": true
  }
}
```

## 测试计划

### 单元测试

1. `test_build_notification_payload()` - payload 结构正确
2. `test_parse_event_data()` - 各事件类型解析正确
3. `test_extract_context()` - 上下文提取正确

### 集成测试

```bash
# 测试权限请求
echo '{"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/test"}}' | \
  ./target/release/cam notify --event permission_request --agent-id test --dry-run

# 测试错误
echo '{"message": "API rate limit"}' | \
  ./target/release/cam notify --event Error --agent-id test --dry-run

# 测试 gateway wake
openclaw gateway call wake --params '{"text": "{\"type\":\"cam_notification\",...}", "mode": "now"}' --json
```

### 端到端测试

1. 启动 CAM agent
2. 触发权限请求
3. 验证 AI 处理后的消息格式
4. 验证用户可以正常回复

## 迁移策略

### 向后兼容

1. 保留 `send_direct()` 作为回退
2. 通过配置开关控制是否启用 AI 处理
3. 渐进式迁移：先 HIGH urgency，再 MEDIUM

### 版本控制

- v1.0: 基础 AI 处理（权限请求、错误）
- v1.1: 扩展到所有事件类型
- v2.0: 添加对话式交互（用户可以追问）

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| AI 处理延迟 | 通知不及时 | 设置超时，回退直接发送 |
| AI 误解事件 | 错误建议 | 保留原始信息，用户可查看 |
| Gateway 不可用 | 通知丢失 | 回退到直接发送 |
| Payload 过大 | 传输失败 | 截断终端快照，压缩 context |

## 总结

AI 处理层通过 gateway wake 机制，让 OpenClaw Agent 智能处理 CAM 通知：

1. **结构化 Payload**: 标准化通知格式，便于 AI 解析
2. **风险评估**: AI 分析操作风险，提供建议
3. **自然语言**: 将技术信息转换为用户友好的描述
4. **回退机制**: 确保通知可靠送达

这个设计保持了现有系统的稳定性，同时提供了更好的用户体验。
