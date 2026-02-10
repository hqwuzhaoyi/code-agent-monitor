//! 终端状态检测模块
//!
//! 使用 AI 判断 agent 状态，兼容多种 AI 编码工具（Claude Code、Codex、OpenCode 等）。
//! 不使用硬编码模式，完全依赖 Haiku API 进行智能判断。

/// 使用 AI 判断 agent 是否正在处理中
///
/// 这个函数调用 Haiku API 分析终端输出，可以识别各种 AI 编码工具的处理状态，
/// 包括 Claude Code、Codex、OpenCode 等。
///
/// # 返回
/// - `true`: agent 正在处理中，不应发送通知
/// - `false`: agent 空闲，等待输入，或 API 调用失败（默认发送通知）
///
/// # 性能
/// - 延迟约 1-2 秒
pub fn is_processing(content: &str) -> bool {
    use crate::anthropic::{is_agent_processing, AgentStatus};

    match is_agent_processing(content) {
        AgentStatus::Processing => true,
        AgentStatus::WaitingForInput => false,
        AgentStatus::Unknown => false, // API 失败时默认发送通知
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // is_processing 测试需要 Anthropic API，标记为 ignore
    // 运行: cargo test is_processing -- --ignored

    #[test]
    #[ignore = "requires Anthropic API"]
    fn test_is_processing_active() {
        let content = "Question?\n✶ Thinking…";
        assert!(is_processing(content), "Should detect active processing");
    }

    #[test]
    #[ignore = "requires Anthropic API"]
    fn test_is_processing_idle() {
        let content = r#"
Which option do you prefer?

1. Option A
2. Option B
3. Option C

❯
"#;
        assert!(!is_processing(content), "Should NOT detect idle state as processing");
    }
}
