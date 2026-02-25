# E2E 通知链路测试报告

测试日期: 2026-02-25
测试环境: macOS Darwin 25.2.0

## 1. 测试场景结果

### 1.1 权限请求 (permission_request)

| 测试用例 | 输入 | 预期 | 实际 | 状态 |
|----------|------|------|------|------|
| LOW 风险命令 | `ls -la` | risk_level: LOW, urgency: HIGH | ✅ 匹配 | PASS |
| HIGH 风险命令 | `rm -rf /tmp/test && sudo apt install` | risk_level: HIGH, urgency: HIGH | ✅ 匹配 | PASS |
| 带 tool_input | `{"tool_name": "Bash", "tool_input": {"command": "cargo test"}}` | tool_name/tool_input 正确解析 | ✅ 匹配 | PASS |
| 空 payload | `{}` | tool_name: "unknown", risk_level: LOW | ✅ 匹配 | PASS |

### 1.2 等待输入 (WaitingForInput)

| 测试用例 | 输入 | 预期 | 实际 | 状态 |
|----------|------|------|------|------|
| 基本测试 | 简单 terminal_snapshot | urgency: HIGH | ✅ 匹配 | PASS |
| AI 提取 | 包含选择题的 snapshot | extracted_message 和 fingerprint 存在 | ✅ 匹配 | PASS |
| 项目路径 | JSON 包含 cwd | project_path 正确设置 | ✅ 匹配 | PASS |

**AI 提取示例输出**:
```json
{
  "extracted_message": "我分析了你的代码结构，发现有两种方案...\n\n💬 回复 A 或 B",
  "question_fingerprint": "react-component-library-or-custom"
}
```

### 1.3 错误通知 (Error)

| 测试用例 | 输入 | 预期 | 实际 | 状态 |
|----------|------|------|------|------|
| 小写 event | `--event error` | urgency: LOW (被跳过) | ⚠️ 被跳过 | ISSUE |
| 正确大小写 | `--event Error` | urgency: HIGH | ✅ 匹配 | PASS |

### 1.4 Agent 退出 (AgentExited)

| 测试用例 | 输入 | 预期 | 实际 | 状态 |
|----------|------|------|------|------|
| 小写 event | `--event agent_exited` | urgency: MEDIUM | ⚠️ urgency: LOW | ISSUE |
| 正确大小写 | `--event AgentExited` | urgency: MEDIUM | ✅ 匹配 | PASS |

## 2. 发现的问题

### 2.1 事件名称大小写不一致 (P1)

**问题**: CLI `--event` 参数和 `urgency.rs` 中的事件名称大小写不一致。

| CLI 输入 | urgency.rs 期望 | 结果 |
|----------|-----------------|------|
| `waiting_for_input` | `WaitingForInput` | ❌ 被判为 LOW |
| `agent_exited` | `AgentExited` | ❌ 被判为 LOW |
| `error` | `Error` | ❌ 被判为 LOW |

**影响**: 使用小写事件名称时，通知会被错误地跳过。

**建议修复**: 在 `urgency.rs` 的 `get_urgency()` 函数中添加大小写不敏感匹配：
```rust
match event_type.to_lowercase().as_str() {
    "permission_request" => Urgency::High,
    "waitingforinput" | "waiting_for_input" => Urgency::High,
    "agentexited" | "agent_exited" => Urgency::Medium,
    "error" => Urgency::High,
    // ...
}
```

### 2.2 JSON 换行符处理 (P2)

**问题**: 当 `terminal_snapshot` 包含字面 `\n` 字符串（而非转义的换行符）时，JSON 解析失败。

**示例**:
```bash
# 失败 - \n 是字面字符串
echo '{"cwd": "/tmp", "terminal_snapshot": "line1\nline2"}'

# 成功 - 使用 heredoc
cat << 'EOF'
{"cwd": "/tmp", "terminal_snapshot": "line1\nline2"}
EOF
```

**影响**: 测试命令需要使用 heredoc 或确保 JSON 正确转义。

**建议**: 这是预期行为，文档中应说明正确的测试方法。

### 2.3 extracted_message 字段未在 Skill 文档中完整说明 (P3)

**问题**: `skills/cam-notify/SKILL.md` 已更新包含 `extracted_message` 和 `question_fingerprint`，但示例场景中的 JSON 结构与实际输出略有差异。

**实际输出**:
```json
{
  "context": {
    "terminal_snapshot": "...",
    "extracted_message": "...",
    "question_fingerprint": "...",
    "risk_level": "MEDIUM"
  }
}
```

**Skill 文档示例**:
```json
{
  "context": {
    "terminal_snapshot": "...",
    "extracted_message": "...",
    "question_fingerprint": "...",
    "message_type": "choice",  // 实际输出中没有
    "options": [],              // 实际输出中没有
    "risk_level": "MEDIUM"
  }
}
```

**建议**: 更新 Skill 文档，移除 `message_type` 和 `options` 字段，或在代码中添加这些字段。

## 3. Payload 格式验证

### 3.1 必需字段

| 字段 | 类型 | 存在 | 说明 |
|------|------|------|------|
| source | string | ✅ | 固定为 "cam" |
| version | string | ✅ | 固定为 "1.0" |
| agent_id | string | ✅ | 从 --agent-id 或 JSON 解析 |
| event_type | string | ✅ | 小写下划线格式 |
| urgency | string | ✅ | HIGH/MEDIUM/LOW |
| timestamp | string | ✅ | ISO 8601 格式 |
| event_data | object | ✅ | 根据 event_type 变化 |
| context | object | ✅ | 包含 risk_level |

### 3.2 可选字段

| 字段 | 类型 | 条件 | 说明 |
|------|------|------|------|
| project_path | string? | JSON 包含 cwd | 项目路径 |
| context.terminal_snapshot | string? | 特定事件类型 | 终端快照 |
| context.extracted_message | string? | AI 提取成功 | 格式化消息 |
| context.question_fingerprint | string? | AI 提取成功 | 语义指纹 |

## 4. 错误场景测试

### 4.1 无效 JSON

```bash
echo 'invalid json' | cam notify --event permission_request --agent-id test --dry-run
```

**结果**: 正常处理，使用默认值
- tool_name: "unknown"
- tool_input: {}
- risk_level: "LOW"

### 4.2 空 JSON

```bash
echo '{}' | cam notify --event permission_request --agent-id test --dry-run
```

**结果**: 正常处理，使用默认值

### 4.3 AI 提取失败

当 `terminal_snapshot` 内容无法被 AI 解析时：
- `extracted_message` 为 null
- `question_fingerprint` 为 null
- 回退到使用 `terminal_snapshot`

## 5. 性能观察

### 5.1 AI 提取延迟

| 操作 | 耗时 |
|------|------|
| AI 状态检测 | ~2s |
| AI 消息提取 | ~3s |
| 总延迟 | ~5s |

**注意**: AI 提取只在 `WaitingForInput` 和 `PermissionRequest` 事件且有 `terminal_snapshot` 时触发。

### 5.2 去重机制

去重通过 `question_fingerprint` 实现，相同指纹的通知在短时间内不会重复发送。

## 6. 测试命令参考

```bash
# 权限请求 - LOW 风险
cat << 'EOF' | cam notify --event permission_request --agent-id test --dry-run
{"cwd": "/tmp/project", "tool_name": "Bash", "tool_input": {"command": "ls -la"}}
EOF

# 权限请求 - HIGH 风险
cat << 'EOF' | cam notify --event permission_request --agent-id test --dry-run
{"cwd": "/tmp/project", "tool_name": "Bash", "tool_input": {"command": "rm -rf /"}}
EOF

# 等待输入 - 带 AI 提取
cat << 'EOF' | cam notify --event WaitingForInput --agent-id test --dry-run
{"cwd": "/tmp/project", "terminal_snapshot": "选择方案:\nA) 方案一\nB) 方案二"}
EOF

# 错误通知
cat << 'EOF' | cam notify --event Error --agent-id test --dry-run
{"cwd": "/tmp/project", "message": "编译失败"}
EOF

# Agent 退出
cat << 'EOF' | cam notify --event AgentExited --agent-id test --dry-run
{"cwd": "/tmp/project", "exit_code": 1}
EOF
```

## 7. 结论

### 通过项
- ✅ Payload 格式正确
- ✅ 新字段 (extracted_message, fingerprint) 正常工作
- ✅ 风险等级判断正确
- ✅ AI 提取功能正常
- ✅ 错误场景处理稳健

### 需要修复
- ⚠️ 事件名称大小写不一致 (P1)
- ⚠️ Skill 文档与实际输出字段不完全匹配 (P3)

### 建议
1. 统一事件名称大小写处理
2. 更新 Skill 文档以反映实际输出格式
3. 添加更多集成测试覆盖边界情况
