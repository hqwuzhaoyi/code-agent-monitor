//! Unified dedup key generator for notification deduplication
//!
//! Generates deterministic hash-based keys from terminal snapshots by:
//! 1. Stripping ANSI escape codes (tool-agnostic)
//! 2. Stripping timestamps (tool-agnostic)
//! 3. Hashing the normalized content
//!
//! NOTE: This module intentionally does NOT filter tool-specific noise patterns
//! (like "Brewing", "Thinking", spinners, etc.) because CAM must be compatible
//! with multiple AI coding tools (Claude Code, Codex, OpenCode, etc.).
//! Noise filtering is handled by the AI extraction layer in src/anthropic.rs.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Generate a deterministic dedup key from terminal snapshot
///
/// The key is a hash of the normalized content, ensuring:
/// - Same meaningful content → same key
/// - Different content → different keys
/// - Ignores ANSI codes and timestamps (tool-agnostic)
pub fn generate_dedup_key(terminal_snapshot: &str) -> String {
    let normalized = normalize_terminal_content(terminal_snapshot);
    let hash = hash_content(&normalized);
    format!("{:016x}", hash)
}

/// Normalize terminal content by removing tool-agnostic noise
///
/// Steps:
/// 1. Strip ANSI escape codes
/// 2. Strip timestamps
/// 3. Filter empty lines
/// 4. Trim and join remaining lines
///
/// NOTE: Does NOT filter tool-specific patterns (spinners, loading text, etc.)
/// to maintain compatibility with multiple AI coding tools.
pub fn normalize_terminal_content(content: &str) -> String {
    let stripped = strip_ansi_codes(content);
    let stripped = strip_timestamps(&stripped);

    stripped
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Strip ANSI escape codes from a string
///
/// Handles:
/// - CSI sequences: \x1b[...m (colors, styles)
/// - OSC sequences: \x1b]...(\x07|\x1b\\) (terminal titles, etc.)
/// - Simple escapes: \x1b followed by single char
pub fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // ESC sequence
            match chars.peek() {
                Some('[') => {
                    // CSI sequence: \x1b[...m
                    chars.next(); // consume '['
                    while let Some(&c) = chars.peek() {
                        chars.next();
                        if c.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC sequence: \x1b]...(\x07|\x1b\\)
                    chars.next(); // consume ']'
                    while let Some(&c) = chars.peek() {
                        if c == '\x07' {
                            chars.next();
                            break;
                        }
                        if c == '\x1b' {
                            chars.next();
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                        chars.next();
                    }
                }
                Some(_) => {
                    // Simple escape: skip next char
                    chars.next();
                }
                None => {}
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Strip timestamps from a string
///
/// Handles common timestamp formats:
/// - ISO 8601: 2024-01-15T10:30:00
/// - Time only: 10:30:00, 10:30
/// - Date only: 2024-01-15, 2024/01/15
/// - Unix timestamps in brackets: [1705312200]
pub fn strip_timestamps(s: &str) -> String {
    use regex::Regex;

    // Lazy static would be better for performance, but keeping it simple
    let patterns = [
        // ISO 8601 datetime
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?",
        // Date with time
        r"\d{4}[-/]\d{2}[-/]\d{2}\s+\d{2}:\d{2}(?::\d{2})?",
        // Time with optional seconds (but only if it looks like a timestamp context)
        r"\[\d{2}:\d{2}(?::\d{2})?\]",
        // Unix timestamp in brackets
        r"\[\d{10,13}\]",
        // Date only at line start or after whitespace
        r"(?:^|\s)\d{4}[-/]\d{2}[-/]\d{2}(?:\s|$)",
    ];

    let mut result = s.to_string();
    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            result = re.replace_all(&result, "").to_string();
        }
    }

    result
}

/// Hash content using DefaultHasher
pub fn hash_content(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== generate_dedup_key tests ====================

    #[test]
    fn test_same_content_same_key() {
        let content = "What would you like to do?\n1. Option A\n2. Option B";
        let key1 = generate_dedup_key(content);
        let key2 = generate_dedup_key(content);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_different_content_different_keys() {
        let content1 = "Question A?";
        let content2 = "Question B?";
        let key1 = generate_dedup_key(content1);
        let key2 = generate_dedup_key(content2);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_key_format() {
        let key = generate_dedup_key("test content");
        assert_eq!(key.len(), 16); // 16 hex chars
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ==================== ANSI code stripping tests ====================

    #[test]
    fn test_ignores_ansi_codes() {
        let plain = "Hello World";
        let with_ansi = "\x1b[31mHello\x1b[0m \x1b[32mWorld\x1b[0m";
        let key1 = generate_dedup_key(plain);
        let key2 = generate_dedup_key(with_ansi);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_strip_ansi_csi_sequences() {
        let input = "\x1b[31;1mBold Red\x1b[0m Normal";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "Bold Red Normal");
    }

    #[test]
    fn test_strip_ansi_osc_sequences() {
        let input = "\x1b]0;Window Title\x07Content";
        let result = strip_ansi_codes(input);
        assert_eq!(result, "Content");
    }

    #[test]
    fn test_strip_ansi_preserves_normal_text() {
        let input = "Normal text without escapes";
        let result = strip_ansi_codes(input);
        assert_eq!(result, input);
    }

    // ==================== Timestamp stripping tests ====================

    #[test]
    fn test_ignores_timestamps() {
        let content1 = "[10:30:00] Question?";
        let content2 = "[11:45:30] Question?";
        let key1 = generate_dedup_key(content1);
        let key2 = generate_dedup_key(content2);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_strip_iso_timestamp() {
        let input = "2024-01-15T10:30:00Z Event happened";
        let result = strip_timestamps(input);
        assert_eq!(result.trim(), "Event happened");
    }

    #[test]
    fn test_strip_bracketed_time() {
        let input = "[10:30:00] Log message";
        let result = strip_timestamps(input);
        assert_eq!(result.trim(), "Log message");
    }

    #[test]
    fn test_strip_unix_timestamp() {
        let input = "[1705312200] Event";
        let result = strip_timestamps(input);
        assert_eq!(result.trim(), "Event");
    }

    // ==================== normalize_terminal_content tests ====================

    #[test]
    fn test_normalize_removes_empty_lines() {
        let content = "Line 1\n\n\nLine 2";
        let normalized = normalize_terminal_content(content);
        assert_eq!(normalized, "Line 1\nLine 2");
    }

    #[test]
    fn test_normalize_trims_result() {
        let content = "  \n  Content  \n  ";
        let normalized = normalize_terminal_content(content);
        assert_eq!(normalized, "Content");
    }

    #[test]
    fn test_normalize_preserves_tool_specific_content() {
        // Tool-specific patterns should NOT be filtered (per CLAUDE.md guidelines)
        // Noise filtering is done by AI extraction layer, not dedup key
        let content = "Question?\nBrewing...\nAnswer here";
        let normalized = normalize_terminal_content(content);
        assert!(normalized.contains("Brewing"));
    }

    // ==================== hash_content tests ====================

    #[test]
    fn test_hash_content_deterministic() {
        let content = "test content";
        let hash1 = hash_content(content);
        let hash2 = hash_content(content);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_content_different_for_different_input() {
        let hash1 = hash_content("content a");
        let hash2 = hash_content("content b");
        assert_ne!(hash1, hash2);
    }

    // ==================== Integration tests ====================

    #[test]
    fn test_full_terminal_snapshot_dedup_with_ansi_and_timestamps() {
        // Same content with different ANSI codes and timestamps should match
        let snapshot1 = "\x1b[32m[10:30:00]\x1b[0m Question?\n1. Option A\n2. Option B";
        let snapshot2 = "\x1b[31m[11:45:30]\x1b[0m Question?\n1. Option A\n2. Option B";

        let key1 = generate_dedup_key(snapshot1);
        let key2 = generate_dedup_key(snapshot2);

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_different_questions_different_keys() {
        let snapshot1 = "What file should I create?";
        let snapshot2 = "What file should I delete?";

        let key1 = generate_dedup_key(snapshot1);
        let key2 = generate_dedup_key(snapshot2);

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_tool_agnostic_normalization() {
        // Verify that tool-specific patterns are preserved (not filtered)
        // This ensures compatibility with multiple AI coding tools
        let content = "Thinking...\nQuestion?\nBrewing...";
        let normalized = normalize_terminal_content(content);

        // All lines should be preserved (only ANSI/timestamps stripped)
        assert!(normalized.contains("Thinking"));
        assert!(normalized.contains("Question"));
        assert!(normalized.contains("Brewing"));
    }
}
