//! 终端快照处理工具模块

/// AI 分析用的上下文行数
pub const AI_CONTEXT_LINES: usize = 80;

/// 状态判断用的行数
pub const STATUS_CHECK_LINES: usize = 30;

/// 显示用的行数
pub const DISPLAY_LINES: usize = 15;

/// 截取终端快照的最后 N 行（用于 AI 分析）
pub fn truncate_for_ai(snapshot: &str) -> String {
    truncate_last_lines(snapshot, AI_CONTEXT_LINES)
}

/// 截取终端快照的最后 N 行（用于状态判断）
pub fn truncate_for_status(snapshot: &str) -> String {
    truncate_last_lines(snapshot, STATUS_CHECK_LINES)
}

/// 截取终端快照的最后 N 行
pub fn truncate_last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= n {
        return text.to_string();
    }
    lines[lines.len() - n..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_last_lines() {
        let text = (1..=100).map(|i| format!("line{}", i)).collect::<Vec<_>>().join("\n");
        let result = truncate_last_lines(&text, 10);
        assert!(result.starts_with("line91"));
        assert!(result.ends_with("line100"));
    }

    #[test]
    fn test_truncate_short_text() {
        let text = "line1\nline2\nline3";
        let result = truncate_last_lines(text, 10);
        assert_eq!(result, text);
    }
}
