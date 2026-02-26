//! 基础设施层 - tmux、进程、终端、解析器

pub mod input;
pub mod jsonl;
pub mod process;
pub mod terminal;
pub mod tmux;

pub use input::{InputWaitDetector, InputWaitPattern, InputWaitResult};
pub use jsonl::{extract_tool_target_from_input, format_tool_use, JsonlEvent, JsonlParser};
pub use process::ProcessScanner;
pub use tmux::TmuxManager;

/// 安全截断 UTF-8 字符串，避免在多字节字符中间截断
///
/// # Arguments
/// * `s` - 要截断的字符串
/// * `max_chars` - 最大字符数（不是字节数）
///
/// # Returns
/// 如果字符串超过 max_chars，返回截断后的字符串加 "..."，否则返回原字符串
pub fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}
