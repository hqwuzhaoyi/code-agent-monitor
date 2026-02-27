# ReAct 消息提取器架构设计

## 概述

ReAct (Reasoning + Acting) 消息提取器是 CAM 的核心组件，负责从 tmux 终端捕获 AI 编码工具的输出，通过迭代扩展上下文直到提取完整的消息内容。

## 设计目标

1. **上下文完整性** - 确保提取的消息包含足够的上下文让用户理解和回答
2. **避免重复 AI 调用** - 使用 fingerprint 去重，避免对同一问题重复发送通知
3. **多工具兼容** - 支持 Claude Code、Codex、OpenCode 等不同 AI 编码工具
4. **低延迟** - 使用 Haiku 模型，单次调用 < 2 秒

## 模块结构

```
src/agent_mod/extractor/
├── mod.rs          # ReAct 循环逻辑 + 公开 API
├── traits.rs       # MessageExtractor trait 定义
├── prompts.rs      # AI 提示词模板
└── context.rs      # 上下文管理（可选，如果逻辑复杂）
```

## 核心 Trait 设计

### MessageExtractor Trait

```rust
// src/agent_mod/extractor/traits.rs

use anyhow::Result;

/// 提取的消息内容
#[derive(Debug, Clone)]
pub struct ExtractedMessage {
    /// 格式化的通知消息（直接发送给用户）
    pub content: String,
    /// 语义指纹（用于去重，如 "react-todo-enhance-or-fresh"）
    pub fingerprint: String,
    /// 上下文是否完整
    pub context_complete: bool,
    /// 消息类型
    pub message_type: MessageType,
    /// 是否是决策类问题（方案选择、架构设计等）
    #[serde(default, alias = "is_decision")]
    pub is_decision_required: bool,
}

/// 消息类型
#[derive(Debug, Clone, PartialEq)]
pub enum MessageType {
    /// 选择题（有选项）
    Choice,
    /// 确认题（y/n）
    Confirmation,
    /// 开放式问题
    OpenEnded,
    /// Agent 空闲（无问题）
    Idle { status: String, last_action: Option<String> },
}

/// 提取结果
#[derive(Debug, Clone)]
pub enum ExtractionResult {
    /// 成功提取到消息
    Success(ExtractedMessage),
    /// 需要更多上下文
    NeedMoreContext,
    /// Agent 正在处理中（不应发送通知）
    Processing,
    /// 提取失败
    Failed(String),
}

/// 消息提取器 trait
pub trait MessageExtractor: Send + Sync {
    /// 从终端快照提取消息
    ///
    /// # 参数
    /// - `terminal_snapshot`: 完整的终端快照
    /// - `lines`: 要分析的行数
    ///
    /// # 返回
    /// - `ExtractionResult`: 提取结果
    fn extract(&self, terminal_snapshot: &str, lines: usize) -> ExtractionResult;

    /// 判断 Agent 是否正在处理中
    fn is_processing(&self, terminal_snapshot: &str) -> bool;
}
```

### 迭代策略配置

```rust
/// 迭代策略配置
#[derive(Debug, Clone)]
pub struct IterationConfig {
    /// 上下文行数序列
    pub context_sizes: Vec<usize>,
    /// 最大迭代次数
    pub max_iterations: usize,
    /// 单次 AI 调用超时（毫秒）
    pub timeout_ms: u64,
}

impl Default for IterationConfig {
    fn default() -> Self {
        Self {
            context_sizes: vec![80, 150, 300, 500, 800],
            max_iterations: 5,
            timeout_ms: 10000,
        }
    }
}
```

## ReAct 循环实现

### 核心逻辑

```rust
// src/agent_mod/extractor/mod.rs

pub mod traits;
pub mod prompts;

use traits::{MessageExtractor, ExtractionResult, ExtractedMessage, IterationConfig};
use crate::infra::tmux::TmuxManager;
use anyhow::Result;
use tracing::{debug, info, warn};

/// ReAct 消息提取器
pub struct ReactExtractor {
    /// AI 提取器实现
    extractor: Box<dyn MessageExtractor>,
    /// 迭代配置
    config: IterationConfig,
}

impl ReactExtractor {
    pub fn new(extractor: Box<dyn MessageExtractor>) -> Self {
        Self {
            extractor,
            config: IterationConfig::default(),
        }
    }

    pub fn with_config(extractor: Box<dyn MessageExtractor>, config: IterationConfig) -> Self {
        Self { extractor, config }
    }

    /// 执行 ReAct 循环提取消息
    ///
    /// # 参数
    /// - `session_id`: tmux session ID
    /// - `tmux`: tmux 管理器
    ///
    /// # 返回
    /// - `Ok(Some(ExtractedMessage))`: 成功提取到消息
    /// - `Ok(None)`: Agent 正在处理中或空闲
    /// - `Err`: 提取失败
    pub fn extract_message(
        &self,
        session_id: &str,
        tmux: &TmuxManager,
    ) -> Result<Option<ExtractedMessage>> {
        // 获取最大行数的终端快照（一次性获取，避免多次 tmux 调用）
        let max_lines = *self.config.context_sizes.last().unwrap_or(&800);
        let full_snapshot = tmux.capture_pane(session_id, max_lines as u32)?;

        // 先检查是否在处理中
        if self.extractor.is_processing(&full_snapshot) {
            debug!(session_id = %session_id, "Agent is processing, skipping extraction");
            return Ok(None);
        }

        // ReAct 循环：逐步扩展上下文
        for (iteration, &lines) in self.config.context_sizes.iter().enumerate() {
            if iteration >= self.config.max_iterations {
                warn!(session_id = %session_id, "Max iterations reached");
                break;
            }

            debug!(
                session_id = %session_id,
                iteration = iteration,
                lines = lines,
                "ReAct iteration"
            );

            match self.extractor.extract(&full_snapshot, lines) {
                ExtractionResult::Success(message) => {
                    info!(
                        session_id = %session_id,
                        fingerprint = %message.fingerprint,
                        iterations = iteration + 1,
                        "Message extracted successfully"
                    );
                    return Ok(Some(message));
                }
                ExtractionResult::NeedMoreContext => {
                    debug!(
                        session_id = %session_id,
                        lines = lines,
                        "Need more context, expanding"
                    );
                    continue;
                }
                ExtractionResult::Processing => {
                    debug!(session_id = %session_id, "Agent is processing");
                    return Ok(None);
                }
                ExtractionResult::Failed(reason) => {
                    warn!(
                        session_id = %session_id,
                        reason = %reason,
                        "Extraction failed"
                    );
                    // 继续尝试更多上下文
                    continue;
                }
            }
        }

        warn!(session_id = %session_id, "Failed to extract message after all iterations");
        Ok(None)
    }
}
```

## AI 提示词设计

### 状态检测提示词

```rust
// src/agent_mod/extractor/prompts.rs

/// 状态检测系统提示词
pub const STATUS_DETECTION_SYSTEM: &str = r#"你是终端状态分析专家。
严格返回且只能返回以下之一：PROCESSING / WAITING / DECISION。
禁止输出任何解释、分析、示例、编号或其它文字。"#;

/// 状态检测用户提示词模板
pub fn status_detection_prompt(terminal_content: &str) -> String {
    format!(r#"分析以下终端输出，判断 AI 编码助手的状态：

<terminal>
{terminal_content}
</terminal>

判断规则：
- 只判断终端最后的状态，忽略历史输出
- PROCESSING: 如果看到以下任一指示器：
  * 带省略号的状态词（如 Thinking…、Brewing…、Running… 等）
  * 旋转动画字符（✢✻✶✽◐◑◒◓⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏）
  * 进度条（注意：底部状态栏不算）
- DECISION: 等待用户做关键决策（方向、方案、技术选择）
- WAITING: 其他等待用户输入的情况

直接回答 PROCESSING、WAITING 或 DECISION。"#)
}
```

### 消息提取提示词

```rust
/// 消息提取系统提示词
pub const MESSAGE_EXTRACTION_SYSTEM: &str = r#"你是终端输出分析专家。
从 AI Agent 终端快照中提取最新的问题，格式化为简洁的通知消息。"#;

/// 消息提取用户提示词模板
pub fn message_extraction_prompt(terminal_content: &str) -> String {
    format!(r#"分析以下 AI Agent 终端输出，提取最新的问题。

<terminal_snapshot>
{terminal_content}
</terminal_snapshot>

<task>
判断 Agent 是否有问题等待用户回答，并提取问题内容。
</task>

<rules>
1. 找到 Agent 最后提出的问题（选择题/确认题/开放式问题）
2. 检查问题之后是否有新的 ⏺ 开头的 Agent 回复
3. 如果没有新的 ⏺ 回复 → has_question = true
4. "[用户正在输入...]" 表示用户还没提交回答，忽略它
</rules>

<output_format>
返回 JSON：
{{
  "has_question": boolean,
  "message": string,           // 问题内容，格式化后
  "fingerprint": string,       // 问题的语义指纹，用于去重
  "context_complete": boolean, // 只要能看到完整的问题和选项就是 true
  "message_type": "choice" | "confirmation" | "open_ended" | "idle",
  "is_decision": boolean,      // 是否需要关键决策（方案选择、架构设计等）
  "agent_status": "completed" | "idle" | "waiting",
  "last_action": string | null
}}
</output_format>

<fingerprint_rule>
fingerprint 是问题的唯一标识符，用于判断两次通知是否是同一个问题。
规则：
- 用英文短横线连接的关键词，如 "react-todo-enhance-or-fresh"
- 只包含问题的核心语义，忽略措辞差异
- 相同问题的不同表述应该生成相同的 fingerprint
</fingerprint_rule>

<context_complete_rule>
context_complete = true 的条件：能看到完整的问题文本和所有选项
context_complete = false 的条件：问题或选项被截断，无法完整显示
</context_complete_rule>

只返回 JSON。"#)
}
```

## 与现有代码集成

### 集成点

1. **替换 `extract_formatted_message`**
   - 现有函数在 `src/ai/extractor.rs`
   - 新的 `ReactExtractor` 提供相同功能但更清晰的架构

2. **复用 `AnthropicClient`**
   - 使用 `src/ai/client.rs` 中的客户端
   - 支持 provider fallback 机制

3. **集成到 `AgentWatcher`**
   - 在 `src/agent_mod/watcher.rs` 中使用 `ReactExtractor`
   - 替换现有的 `input_detector.detect_immediate()` 调用

### 迁移路径

```rust
// 在 AgentWatcher 中使用
impl AgentWatcher {
    pub fn new() -> Self {
        let extractor = HaikuExtractor::new(); // 实现 MessageExtractor trait
        let react_extractor = ReactExtractor::new(Box::new(extractor));

        Self {
            // ... 其他字段
            react_extractor,
        }
    }

    fn check_waiting_state(&mut self, agent: &AgentRecord) -> Option<WatchEvent> {
        match self.react_extractor.extract_message(&agent.tmux_session, &self.tmux) {
            Ok(Some(message)) => {
                // 使用 fingerprint 进行去重
                let action = self.deduplicator.should_send(&agent.agent_id, &message.fingerprint);
                match action {
                    NotifyAction::Send => Some(WatchEvent::WaitingForInput {
                        agent_id: agent.agent_id.clone(),
                        pattern_type: format!("{:?}", message.message_type),
                        context: message.content,
                        dedup_key: message.fingerprint,
                        is_decision_required: message.is_decision_required,
                    }),
                    _ => None,
                }
            }
            Ok(None) => None,
            Err(e) => {
                warn!(error = %e, "Failed to extract message");
                None
            }
        }
    }
}
```

## 错误处理

### 错误类型

```rust
/// 提取器错误
#[derive(Debug, thiserror::Error)]
pub enum ExtractorError {
    #[error("Tmux capture failed: {0}")]
    TmuxError(#[from] anyhow::Error),

    #[error("AI API call failed: {0}")]
    ApiError(String),

    #[error("JSON parse failed: {0}")]
    ParseError(String),

    #[error("Timeout after {0}ms")]
    Timeout(u64),

    #[error("Max iterations reached")]
    MaxIterations,
}
```

### 错误处理策略

| 错误类型 | 处理策略 |
|---------|---------|
| Tmux 捕获失败 | 返回 `Err`，由调用方决定是否重试 |
| AI API 超时 | 尝试下一个 provider，最终返回 `Failed` |
| JSON 解析失败 | 记录警告，尝试更多上下文 |
| 上下文不完整 | 自动扩展上下文重试 |
| 最大迭代次数 | 返回 `None`，记录警告 |

## 性能考虑

### 优化策略

1. **一次性获取终端快照**
   - 获取最大行数的快照，避免多次 tmux 调用
   - 在内存中截取不同行数进行分析

2. **稳定性检测前置**
   - 在调用 AI 之前检查终端是否稳定
   - 避免对正在变化的终端进行 AI 分析

3. **Fingerprint 缓存**
   - 缓存最近的 fingerprint，避免重复发送相同问题
   - 使用 LRU 缓存，限制内存使用

### 预期性能

| 指标 | 目标值 |
|-----|-------|
| 单次 AI 调用 | < 2 秒 |
| 完整提取流程 | < 10 秒（最多 5 次迭代）|
| 内存使用 | < 10 MB（终端快照 + 缓存）|

## 测试策略

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Mock 提取器用于测试
    struct MockExtractor {
        results: Vec<ExtractionResult>,
        call_count: std::sync::atomic::AtomicUsize,
    }

    impl MessageExtractor for MockExtractor {
        fn extract(&self, _snapshot: &str, _lines: usize) -> ExtractionResult {
            let idx = self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.results.get(idx).cloned().unwrap_or(ExtractionResult::Failed("No more results".into()))
        }

        fn is_processing(&self, _snapshot: &str) -> bool {
            false
        }
    }

    #[test]
    fn test_react_loop_expands_context() {
        let extractor = MockExtractor {
            results: vec![
                ExtractionResult::NeedMoreContext,
                ExtractionResult::NeedMoreContext,
                ExtractionResult::Success(ExtractedMessage {
                    content: "Test question".into(),
                    fingerprint: "test-question".into(),
                    context_complete: true,
                    message_type: MessageType::OpenEnded,
                    is_decision_required: false,
                }),
            ],
            call_count: Default::default(),
        };

        let react = ReactExtractor::new(Box::new(extractor));
        // ... 测试逻辑
    }
}
```

### 集成测试

1. **真实 tmux 会话测试**
   - 创建测试 tmux 会话
   - 模拟 Claude Code 输出
   - 验证提取结果

2. **端到端测试**
   - 使用 `cam watch-trigger` 触发检测
   - 验证通知发送到 webhook

## Decision Detection Pipeline

`is_decision_required` 字段贯穿整个通知管道，从 AI 提取到最终 Webhook 发送。以下是完整的数据流：

### 管道流程图

```
Terminal Snapshot
    ↓
HaikuExtractor.extract() → ExtractedMessage.is_decision_required
    ↓
ReactExtractor.extract_message() → Option<ExtractedMessage>
    ↓
AgentWatcher (poll_once / trigger_wait_check / check_waiting_with_react)
    ↓
WatchEvent::WaitingForInput { is_decision_required }
    ↓
NotificationEventType::WaitingForInput { is_decision_required }
    ↓
SystemEventPayload::from_event() → EventData + risk_level
    ↓
Webhook JSON: { "eventData": { "isDecisionRequired": true }, "riskLevel": "HIGH" }
```

### String-to-Bool 强制转换

AI 模型（Haiku）返回的 JSON 中，`is_decision` 字段可能是布尔值 `true` 或字符串 `"true"`。解析器同时处理两种情况：

```rust
// src/agent_mod/extractor/mod.rs
let is_decision_required = parsed.get("is_decision")
    .and_then(|v| v.as_bool()
        .or_else(|| v.as_str().map(|s| s.eq_ignore_ascii_case("true"))))
    .unwrap_or(false);
```

这确保了无论 AI 返回 `"is_decision": true` 还是 `"is_decision": "true"`，都能正确解析为 `bool`。

### Serde 别名兼容性

`ExtractedMessage` 结构体中的 `is_decision_required` 字段使用 serde 别名，兼容旧版 AI 返回的 `is_decision` 字段名：

```rust
// src/agent_mod/extractor/traits.rs
#[serde(default, alias = "is_decision")]
pub is_decision_required: bool,
```

- `default` — 字段缺失时默认为 `false`
- `alias = "is_decision"` — 同时接受 `is_decision` 和 `is_decision_required` 两种字段名

### 风险等级映射

| is_decision_required | risk_level | 行为 |
|---------------------|------------|------|
| true | HIGH | 立即通知，需要人工决策 |
| false | MEDIUM | 常规通知 |

风险等级在 `SystemEventPayload::from_event()` 中计算（`src/notification/system_event.rs`）：

```rust
NotificationEventType::WaitingForInput { is_decision_required, .. } => {
    if *is_decision_required {
        "HIGH".to_string()
    } else {
        "MEDIUM".to_string()
    }
}
```

### 相关文件

| 文件 | 职责 |
|------|------|
| `src/agent_mod/extractor/traits.rs` | `ExtractedMessage.is_decision_required` 定义 |
| `src/agent_mod/extractor/mod.rs` | AI JSON 解析 + string-to-bool 转换 |
| `src/agent_mod/watcher.rs` | `poll_once` / `trigger_wait_check` / `check_waiting_with_react` |
| `src/notification/event.rs` | `NotificationEventType::WaitingForInput` 枚举 |
| `src/notification/system_event.rs` | `EventData::WaitingForInput` + 风险等级映射 |
| `src/notification/openclaw.rs` | AI 提取结果升级 payload |

## 后续优化方向

1. **流式处理** - 支持流式 AI 响应，减少延迟
2. **本地模型** - 支持本地 LLM 作为 fallback
3. **学习机制** - 记录用户反馈，优化提示词
4. **多语言支持** - 支持中英文混合的终端输出
