//! Output formatting for CLI commands

use serde::Serialize;

/// Format output as JSON or table based on --json flag
pub fn format_output<T: Serialize>(data: &T, json: bool) -> String {
    if json {
        serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string())
    } else {
        // Default to JSON for now
        serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string())
    }
}
