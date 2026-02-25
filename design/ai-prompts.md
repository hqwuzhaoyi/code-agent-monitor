# ReAct 提取器 AI Prompt 设计

## 概述

本文档定义 ReAct 消息提取器使用的 AI prompts，用于：
1. 判断终端上下文是否完整
2. 提取格式化的通知消息
3. 生成语义指纹用于去重

设计原则：
- **简洁高效** - 减少 token 消耗，单次调用 < 2 秒
- **多工具兼容** - 支持 Claude Code、Codex、OpenCode 等
- **准确可靠** - 明确的判断规则，减少歧义

---

## 1. 统一提取 Prompt（推荐）

将状态检测和消息提取合并为单次 AI 调用，减少延迟和成本。

### System Prompt

```
你是终端输出分析专家。分析 AI Agent 终端快照，判断状态并提取问题。只返回 JSON。
```

### User Prompt Template

```
分析终端快照，判断 Agent 状态并提取问题。

<terminal>
{terminal_content}
</terminal>

<判断规则>
状态优先级：PROCESSING > HAS_QUESTION > IDLE

PROCESSING（正在处理，跳过通知）:
- 带省略号的状态词：Thinking…, Brewing…, Running…, Working…
- 旋转动画字符：✢✻✶✽◐◑◒◓⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏
- 括号内运行提示：(running), (executing), (loading)
- 注意：底部状态栏的上下文使用量（如 ██░░ 22%）不算

HAS_QUESTION（有问题等待回答）:
- 终端显示问题且问题后无新的 ⏺ 回复
- "[用户正在输入...]" 表示用户未提交，忽略

IDLE（空闲无问题）:
- 只有提示符 ❯ 或 >，无问题
- Agent 显示完成信息后等待
</判断规则>

<输出格式>
{
  "status": "processing" | "has_question" | "idle",
  "context_complete": true | false,
  "message": "格式化的问题内容（status=has_question 时）",
  "fingerprint": "语义指纹（status=has_question 时）",
  "message_type": "choice" | "confirmation" | "open_ended",
  "options": ["选项1", "选项2"],
  "agent_status": "completed" | "idle" | "waiting",
  "last_action": "最后操作摘要或 null"
}
</输出格式>

<context_complete 规则>
true: 能看到完整问题和所有选项
false: 问题引用了不可见内容（如"这个方案"但方案不可见）
</context_complete>

<fingerprint 规则>
- 英文短横线连接关键词：react-todo-enhance-or-fresh
- 只含核心语义，忽略措辞差异
- 相同问题不同表述 → 相同 fingerprint
</fingerprint>

<message 格式>
- 选择题：问题 + 选项列表 + "回复字母/数字选择"
- 确认题：问题 + "回复 y/n"
- 开放题：问题 + "回复内容"
- 不超过 500 字符
</message>

只返回 JSON。
```

### 输出 Schema

```json
{
  "status": "processing" | "has_question" | "idle",
  "context_complete": boolean,
  "message": string,
  "fingerprint": string,
  "message_type": "choice" | "confirmation" | "open_ended",
  "options": string[],
  "agent_status": "completed" | "idle" | "waiting",
  "last_action": string | null
}
```

### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| status | enum | 主状态：processing/has_question/idle |
| context_complete | bool | 上下文是否完整，false 时需扩展 |
| message | string | 格式化的通知消息 |
| fingerprint | string | 语义指纹，用于去重 |
| message_type | enum | 问题类型 |
| options | array | 选项列表（仅 choice 类型） |
| agent_status | enum | Agent 细分状态 |
| last_action | string? | 最后操作摘要 |

---

## 2. 分离式 Prompts（备选）

如果需要更细粒度控制，可分离为两个 prompt。

### 2.1 状态检测 Prompt

用于快速判断是否需要发送通知。

#### System Prompt

```
你是终端状态分析专家。严格返回：PROCESSING / WAITING / DECISION。禁止其他输出。
```

#### User Prompt Template

```
判断 AI 编码助手状态：

<terminal>
{terminal_content}
</terminal>

规则：
- PROCESSING: 状态词（Thinking…, Brewing…）、动画字符、运行提示
- DECISION: 等待关键决策（方向/方案/技术选择）
- WAITING: 其他等待输入

直接回答 PROCESSING、WAITING 或 DECISION。
```

### 2.2 消息提取 Prompt

仅在状态为 WAITING/DECISION 时调用。

#### System Prompt

```
你是终端输出分析专家。提取 Agent 问题，格式化为通知消息。只返回 JSON。
```

#### User Prompt Template

```
提取终端中的问题：

<terminal>
{terminal_content}
</terminal>

返回 JSON：
{
  "has_question": boolean,
  "message": "格式化问题",
  "fingerprint": "语义指纹",
  "context_complete": boolean,
  "message_type": "choice" | "confirmation" | "open_ended",
  "options": []
}

规则：
- 找最后的问题，检查后面有无新 ⏺ 回复
- fingerprint: 英文短横线连接关键词
- context_complete: 能看到完整问题和选项

只返回 JSON。
```

---

## 3. 边界情况处理

### 3.1 UI 噪音过滤

终端快照可能包含 UI 元素，需要过滤：

| 噪音类型 | 示例 | 处理 |
|---------|------|------|
| 状态动画 | ✶ Brewing… | 识别为 PROCESSING |
| 进度条 | ████░░░░ 41% | 底部状态栏忽略 |
| ASCII Logo | ▐▛███▜▌ | 忽略 |
| 工具标记 | ⏺ Read 1 file | 识别为 Agent 回复边界 |

### 3.2 不完整消息

当 `context_complete = false` 时：

```
原因示例：
- "这个方案可以吗？" - 方案内容不可见
- "以下选项选哪个？" - 选项被截断
- "上面的代码有问题" - 代码不可见

处理：
1. 返回 context_complete: false
2. 调用方扩展上下文重试
3. 扩展序列：80 → 150 → 300 → 500 → 800 行
```

### 3.3 多 Agent 类型兼容

| Agent | 状态指示 | 问题格式 | 提示符 |
|-------|---------|---------|--------|
| Claude Code | ✶ Brewing… | A) B) C) 或 [Y/n] | ❯ |
| Codex | Working... | y/n/a/p | $ |
| OpenCode | Processing… | 1. 2. 3. | > |

Prompt 设计不硬编码特定模式，而是描述通用规则让 AI 判断。

### 3.4 用户输入处理

```
场景：用户正在输入但未提交
终端显示：❯ 我想要一个简单的...

处理：
1. 预处理替换为 "❯ [用户正在输入...]"
2. AI 忽略此行，不影响问题判断
```

---

## 4. Prompt 优化技巧

### 4.1 Token 优化

| 优化点 | 方法 | 效果 |
|-------|------|------|
| 终端截取 | 只取最后 N 行 | 减少 50-80% token |
| System 简短 | 一句话定义角色 | 减少 ~50 token |
| 规则压缩 | 用符号代替文字 | 减少 ~100 token |
| 输出限制 | 只返回 JSON | 避免解释性输出 |

### 4.2 准确率优化

| 问题 | 解决方案 |
|------|---------|
| 状态误判 | 明确列出所有状态词 |
| 选项重复 | 让 AI 直接输出格式化文本 |
| 上下文不足 | 迭代扩展机制 |
| 指纹不稳定 | 明确指纹生成规则 |

### 4.3 延迟优化

| 优化点 | 方法 |
|-------|------|
| 模型选择 | 使用 Haiku 4.5（最快） |
| 并行调用 | 状态检测和提取可并行 |
| 缓存 | 缓存最近的 fingerprint |
| 超时 | 设置 10 秒超时 |

---

## 5. 实现参考

### Rust 代码结构

```rust
// src/agent_mod/extractor/prompts.rs

/// 统一提取 System Prompt
pub const UNIFIED_SYSTEM: &str =
    "你是终端输出分析专家。分析 AI Agent 终端快照，判断状态并提取问题。只返回 JSON。";

/// 生成统一提取 User Prompt
pub fn unified_prompt(terminal_content: &str) -> String {
    format!(r#"分析终端快照，判断 Agent 状态并提取问题。

<terminal>
{terminal_content}
</terminal>

<判断规则>
状态优先级：PROCESSING > HAS_QUESTION > IDLE
...
</判断规则>

只返回 JSON。"#)
}

/// 解析统一提取结果
#[derive(Debug, Deserialize)]
pub struct UnifiedResult {
    pub status: String,
    pub context_complete: bool,
    pub message: String,
    pub fingerprint: String,
    pub message_type: String,
    pub options: Vec<String>,
    pub agent_status: String,
    pub last_action: Option<String>,
}
```

### 调用流程

```
1. 获取终端快照（最大 800 行）
2. 从 80 行开始调用 AI
3. 如果 context_complete = false，扩展到 150 行重试
4. 继续扩展直到成功或达到最大行数
5. 返回提取结果或失败
```

---

## 6. 测试用例

### 6.1 状态检测测试

```
输入：✶ Brewing…
期望：status = "processing"

输入：❯ (空行)
期望：status = "idle"

输入：你想选择哪个方案？A) 方案一 B) 方案二
期望：status = "has_question"
```

### 6.2 消息提取测试

```
输入：
⏺ 第一个问题：项目用途？
A) 学习项目
B) 作品集
C) 实际工具
❯

期望：
{
  "status": "has_question",
  "context_complete": true,
  "message": "项目用途？\nA) 学习项目\nB) 作品集\nC) 实际工具\n\n回复字母选择",
  "fingerprint": "project-purpose-learning-portfolio-tool",
  "message_type": "choice"
}
```

### 6.3 上下文不完整测试

```
输入：
这个方案可以吗？[Y/n]
❯

期望：
{
  "status": "has_question",
  "context_complete": false,
  ...
}
```

---

## 7. 版本历史

| 版本 | 日期 | 变更 |
|------|------|------|
| 1.0 | 2026-02-25 | 初始设计 |
