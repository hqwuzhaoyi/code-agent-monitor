//! AI 提示词模板
//!
//! 用于 ReAct 消息提取的 AI 提示词。

/// 状态检测系统提示词
pub const STATUS_DETECTION_SYSTEM: &str = r#"你是终端状态分析专家。分析 AI 编码助手的终端输出，判断其当前状态。

输出要求：只返回一个词：PROCESSING、WAITING 或 DECISION。不要输出任何其他内容。"#;

/// 状态检测用户提示词模板
pub fn status_detection_prompt(terminal_content: &str) -> String {
    format!(
        r#"分析终端输出的最后部分，判断 AI 编码助手的状态。

<terminal>
{terminal_content}
</terminal>

## 状态定义

### PROCESSING（正在处理）
AI 正在执行任务，用户无需操作。

必须满足以下任一条件：
- 终端最后显示加载动画字符：✢ ✻ ✶ ✽ ◐ ◑ ◒ ◓ ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
- 终端最后显示带省略号的状态词：Thinking…、Brewing…、Running…、Hatching…、Working…
- 终端最后显示进度指示：[====>    ] 或百分比进度

### WAITING（等待输入）
AI 已完成当前步骤，等待用户提供信息或确认。

必须满足以下全部条件：
- 没有 PROCESSING 的任何指示器
- 终端显示问题或提示，如：
  - 确认请求：「是否继续？」「确认执行？」「y/n」
  - 信息请求：「请输入...」「请提供...」
  - 选择题：带编号的选项列表

### DECISION（需要决策）
AI 需要用户做出影响后续方向的重要决策。

必须满足以下全部条件：
- 没有 PROCESSING 的任何指示器
- 问题涉及：技术方案选择、架构设计、实现策略、功能取舍

## 判断流程

1. 首先检查终端最后是否有 PROCESSING 指示器 → 如果有，返回 PROCESSING
2. 然后检查是否有问题等待回答 → 如果没有问题，返回 PROCESSING
3. 最后判断问题类型 → 重要决策返回 DECISION，其他返回 WAITING

回答："#
    )
}

/// 消息提取系统提示词
pub const MESSAGE_EXTRACTION_SYSTEM: &str = r#"你是终端输出分析专家。
从 AI Agent 终端快照中提取最新的问题，格式化为简洁的通知消息。"#;

/// 消息提取用户提示词模板
pub fn message_extraction_prompt(terminal_content: &str) -> String {
    format!(
        r#"分析以下 AI Agent 终端输出，提取最新的问题。

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

只返回 JSON。"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_detection_prompt() {
        let prompt = status_detection_prompt("test content");
        assert!(prompt.contains("test content"));
        assert!(prompt.contains("PROCESSING"));
        assert!(prompt.contains("WAITING"));
        assert!(prompt.contains("DECISION"));
    }

    #[test]
    fn test_message_extraction_prompt() {
        let prompt = message_extraction_prompt("terminal output");
        assert!(prompt.contains("terminal output"));
        assert!(prompt.contains("has_question"));
        assert!(prompt.contains("fingerprint"));
        assert!(prompt.contains("context_complete"));
    }
}
