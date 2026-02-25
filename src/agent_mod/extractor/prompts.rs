//! AI 提示词模板
//!
//! 用于 ReAct 消息提取的 AI 提示词。

/// 状态检测系统提示词
pub const STATUS_DETECTION_SYSTEM: &str = r#"你是终端状态分析专家。
严格返回且只能返回以下之一：PROCESSING / WAITING / DECISION。
禁止输出任何解释、分析、示例、编号或其它文字。"#;

/// 状态检测用户提示词模板
pub fn status_detection_prompt(terminal_content: &str) -> String {
    format!(
        r#"分析以下终端输出，判断 AI 编码助手的状态：

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

直接回答 PROCESSING、WAITING 或 DECISION。"#
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
