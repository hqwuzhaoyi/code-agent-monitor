# CAM Notify Skill 分析报告

## 1. 当前 Skill 结构

### 触发方式
Skill 通过 OpenClaw Gateway 的 webhook 触发：
- CAM 发送 POST 请求到 `/hooks/agent`
- Gateway 将 payload 传递给 OpenClaw 对话
- OpenClaw 加载 `cam-notify` skill 处理事件

### Payload 格式 (SystemEventPayload)
```rust
pub struct SystemEventPayload {
    source: String,           // "cam"
    version: String,          // "1.0"
    agent_id: String,         // "cam-xxx"
    event_type: String,       // "permission_request" | "waiting_for_input" | ...
    urgency: String,          // "HIGH" | "MEDIUM" | "LOW"
    project_path: Option<String>,
    timestamp: DateTime<Utc>,
    event_data: EventData,    // 根据 event_type 不同
    context: EventContext,    // 包含 terminal_snapshot 和 extracted_message
}

pub struct EventContext {
    terminal_snapshot: Option<String>,    // 终端快照
    extracted_message: Option<String>,    // AI 提取的格式化消息
    question_fingerprint: Option<String>, // 问题指纹（去重用）
    risk_level: String,                   // "LOW" | "MEDIUM" | "HIGH"
}
```

### 处理逻辑
1. 根据 `event_type` 和 `urgency` 决定是否发送通知
2. 使用三层决策模型（白名单/黑名单/LLM）判断是否自动审批
3. 格式化消息并发送给用户
4. 等待用户回复，通过 `cam reply` 路由回 Agent

## 2. 消息格式化

### 当前格式化逻辑 (system_event.rs:198-290)
```rust
fn to_telegram_message(&self) -> String {
    // 优先使用 AI 提取的消息
    if let Some(extracted) = &self.context.extracted_message {
        extracted.clone()
    } else {
        // Fallback: 截取终端最后 30 行
        let snapshot_tail = self.context.terminal_snapshot.as_ref().map(|snapshot| {
            let lines: Vec<&str> = snapshot.lines().collect();
            let start = lines.len().saturating_sub(30);
            lines[start..].join("\n")
        });
        // ...
    }
}
```

### 包含的信息
- emoji 标识（⚠️/💬/ℹ️）
- agent_id
- 事件描述（从 extracted_message 或 fallback）
- 风险等级（🔴/🟡/🟢）
- 操作提示（"回复 y 允许 / n 拒绝"）

### 缺失的信息
1. **Claude Code 原始问题的完整上下文** - 用户看不到 Agent 问的具体问题
2. **选项列表** - 如果是选择题，选项可能被截断或丢失
3. **问题背景** - Agent 为什么问这个问题，之前做了什么

## 3. 问题识别

### 为什么用户看不到原始上下文

#### 问题 1: AI 提取可能失败或返回不完整
`extract_formatted_message()` 函数（extractor.rs:682-702）：
- 使用 Haiku 从终端快照提取问题
- 如果 AI 判断 `has_question=false`，返回 `Idle` 状态
- 如果 AI 提取失败，返回 `Failed`
- **关键问题**: AI 可能误判或提取不完整

#### 问题 2: Fallback 机制不够智能
当 `extracted_message` 为空时，fallback 只是截取终端最后 30 行：
```rust
let lines: Vec<&str> = snapshot.lines().collect();
let start = lines.len().saturating_sub(30);
lines[start..].join("\n")
```
这种方式：
- 可能包含大量无关的终端 UI 噪音
- 可能截断重要的上下文
- 没有智能过滤

#### 问题 3: Skill 只看到 OpenClaw 的补充建议
当前流程：
```
CAM → Webhook → OpenClaw → cam-notify skill → 用户
                    ↓
              OpenClaw 可能添加自己的分析/建议
```
用户看到的是 OpenClaw 处理后的消息，而不是原始的 Claude Code 问题。

### 数据丢失点
1. **终端快照截断** - 80/150/300 行的上下文可能不够
2. **AI 提取失败** - Haiku 可能无法正确理解复杂问题
3. **消息格式化** - `to_telegram_message()` 可能丢失结构化信息
4. **OpenClaw 处理** - Skill 可能添加额外内容，稀释原始问题

### Skill 的局限性
1. **依赖 AI 提取质量** - 如果 Haiku 提取失败，用户看不到问题
2. **没有原始终端快照展示** - 用户无法查看原始终端输出
3. **消息格式固定** - 无法根据问题类型动态调整格式
4. **缺少上下文扩展机制** - 如果上下文不够，没有让用户主动获取更多信息的方式

## 4. 改进建议

### 4.1 让 Skill 发送完整上下文

#### 方案 A: 增强 extracted_message 字段
修改 AI 提取 prompt，要求包含：
- 完整的问题文本
- 所有选项（如果是选择题）
- 问题的背景上下文（Agent 之前做了什么）

```rust
// extractor.rs 中的 prompt 改进
let prompt = format!(r#"
提取问题时，必须包含：
1. 完整的问题文本（不要截断）
2. 所有选项（如果是选择题，列出所有选项）
3. 问题背景（Agent 之前完成了什么任务，为什么问这个问题）
4. 回复提示（用户应该如何回复）
"#);
```

#### 方案 B: 添加 raw_context 字段
在 payload 中添加原始终端快照的清理版本：
```rust
pub struct EventContext {
    // ... 现有字段
    raw_context: Option<String>,  // 清理后的终端快照（去除 UI 噪音）
}
```

### 4.2 消息格式改进

#### 当前格式
```
⚠️ CAM cam-xxx

执行: Bash npm install

风险: 🟡 MEDIUM

回复 y 允许 / n 拒绝
```

#### 建议格式
```
⚠️ [cam-xxx] 请求确认

📋 问题:
你想要增强现有的 React Todo List 还是从头开始？

🔢 选项:
A) 增强现有项目 - 添加新功能到当前代码
B) 从头开始 - 创建全新的项目结构

💡 背景:
Agent 已完成项目分析，发现现有 Todo List 项目

📝 回复: A 或 B
```

### 4.3 回复引导改进

#### 添加快捷回复按钮（如果平台支持）
```
[A] 增强现有  [B] 从头开始  [查看详情]
```

#### 添加"查看更多上下文"选项
```
回复 "more" 查看完整终端输出
```

### 4.4 Skill 处理逻辑改进

```markdown
## 消息格式化规则（建议添加到 SKILL.md）

### 选择题格式
当 event_data.pattern_type 包含选项时：
1. 提取完整问题
2. 列出所有选项（保持原始编号）
3. 添加回复提示

### 确认题格式
当 event_type 为 permission_request 时：
1. 显示要执行的命令
2. 显示风险等级
3. 显示命令的影响范围

### 开放式问题格式
当问题需要用户输入时：
1. 显示完整问题
2. 显示问题背景
3. 提示用户可以直接回复内容
```

## 5. 总结

### 核心问题
用户看不到 Claude Code 原始问题的根本原因是：
1. AI 提取可能失败或不完整
2. Fallback 机制只是简单截取，没有智能处理
3. 消息格式化丢失了结构化信息
4. Skill 没有展示原始上下文的机制

### 优先级建议
1. **高优先级**: 改进 AI 提取 prompt，确保包含完整问题和选项
2. **中优先级**: 添加 raw_context 字段作为 fallback
3. **低优先级**: 改进消息格式，添加"查看更多"功能
