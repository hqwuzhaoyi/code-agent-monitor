//! JSONL 事件解析模块 - 解析 Claude Code 的 JSONL 日志

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;

/// JSONL 事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event_type")]
pub enum JsonlEvent {
    /// 工具调用
    ToolUse {
        tool_name: String,
        tool_id: String,
        input: serde_json::Value,
        timestamp: Option<String>,
    },
    /// 工具结果
    ToolResult {
        tool_id: String,
        success: bool,
        output: Option<String>,
        timestamp: Option<String>,
    },
    /// 错误
    Error {
        message: String,
        timestamp: Option<String>,
    },
    /// 用户消息
    UserMessage {
        content: String,
        timestamp: Option<String>,
    },
    /// 助手文本响应
    AssistantText {
        content: String,
        timestamp: Option<String>,
    },
    /// 进度事件
    Progress {
        progress_type: String,
        message: Option<String>,
        timestamp: Option<String>,
    },
}

/// JSONL 消息的原始格式
#[derive(Debug, Clone, Deserialize)]
struct RawJsonlMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    message: Option<RawMessageContent>,
    #[serde(rename = "userMessage")]
    user_message: Option<RawUserMessage>,
    timestamp: Option<String>,
    data: Option<RawProgressData>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawMessageContent {
    role: Option<String>,
    content: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawUserMessage {
    content: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawProgressData {
    #[serde(rename = "type")]
    progress_type: Option<String>,
    message: Option<serde_json::Value>,
}

/// JSONL 解析器
pub struct JsonlParser {
    path: PathBuf,
    position: u64,
}

impl JsonlParser {
    /// 创建新的解析器
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            position: 0,
        }
    }

    /// 设置读取位置
    pub fn set_position(&mut self, position: u64) {
        self.position = position;
    }

    /// 获取当前位置
    pub fn position(&self) -> u64 {
        self.position
    }

    /// 解析单行 JSONL
    pub fn parse_line(line: &str) -> Option<JsonlEvent> {
        let raw: RawJsonlMessage = serde_json::from_str(line).ok()?;
        Self::convert_raw_message(&raw)
    }

    /// 读取新增的事件
    pub fn read_new_events(&mut self) -> Result<Vec<JsonlEvent>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.path)?;
        let file_len = file.metadata()?.len();

        // 如果文件没有新内容，直接返回
        if file_len <= self.position {
            return Ok(Vec::new());
        }

        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(self.position))?;

        let mut events = Vec::new();
        let mut current_pos = self.position;

        for line in reader.lines() {
            let line = line?;
            let line_len = line.len() as u64 + 1; // +1 for newline
            current_pos += line_len;

            if line.trim().is_empty() {
                continue;
            }

            if let Some(event) = Self::parse_line(&line) {
                events.push(event);
            }
        }

        self.position = current_pos;
        Ok(events)
    }

    /// 转换原始消息为事件
    fn convert_raw_message(raw: &RawJsonlMessage) -> Option<JsonlEvent> {
        let msg_type = raw.msg_type.as_deref()?;

        match msg_type {
            "user" => {
                let content = raw.user_message.as_ref()?.content.as_ref()?;
                Some(JsonlEvent::UserMessage {
                    content: content.clone(),
                    timestamp: raw.timestamp.clone(),
                })
            }
            "assistant" => {
                let message = raw.message.as_ref()?;
                let content = message.content.as_ref()?;
                Self::parse_assistant_content(content, raw.timestamp.as_deref())
            }
            "progress" => {
                let data = raw.data.as_ref()?;
                let progress_type = data.progress_type.clone().unwrap_or_default();
                let message = data.message.as_ref().and_then(|m| {
                    if let Some(s) = m.as_str() {
                        Some(s.to_string())
                    } else {
                        serde_json::to_string(m).ok()
                    }
                });
                Some(JsonlEvent::Progress {
                    progress_type,
                    message,
                    timestamp: raw.timestamp.clone(),
                })
            }
            _ => None,
        }
    }

    /// 解析 assistant 消息内容
    fn parse_assistant_content(content: &serde_json::Value, timestamp: Option<&str>) -> Option<JsonlEvent> {
        match content {
            serde_json::Value::Array(arr) => {
                // 遍历内容数组，找到第一个有意义的事件
                for item in arr {
                    if let Some(obj) = item.as_object() {
                        let content_type = obj.get("type").and_then(|t| t.as_str())?;

                        match content_type {
                            "tool_use" => {
                                let tool_name = obj.get("name").and_then(|n| n.as_str())?.to_string();
                                let tool_id = obj.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                                let input = obj.get("input").cloned().unwrap_or(serde_json::Value::Null);

                                return Some(JsonlEvent::ToolUse {
                                    tool_name,
                                    tool_id,
                                    input,
                                    timestamp: timestamp.map(|s| s.to_string()),
                                });
                            }
                            "tool_result" => {
                                let tool_id = obj.get("tool_use_id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                                let is_error = obj.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false);
                                let output = obj.get("content").and_then(|c| {
                                    if let Some(s) = c.as_str() {
                                        Some(s.to_string())
                                    } else {
                                        serde_json::to_string(c).ok()
                                    }
                                });

                                return Some(JsonlEvent::ToolResult {
                                    tool_id,
                                    success: !is_error,
                                    output,
                                    timestamp: timestamp.map(|s| s.to_string()),
                                });
                            }
                            "text" => {
                                let text = obj.get("text").and_then(|t| t.as_str())?;

                                // 检查是否包含错误信息
                                if Self::is_error_text(text) {
                                    return Some(JsonlEvent::Error {
                                        message: text.to_string(),
                                        timestamp: timestamp.map(|s| s.to_string()),
                                    });
                                }

                                return Some(JsonlEvent::AssistantText {
                                    content: text.to_string(),
                                    timestamp: timestamp.map(|s| s.to_string()),
                                });
                            }
                            "thinking" => {
                                // 跳过 thinking 内容
                                continue;
                            }
                            _ => continue,
                        }
                    }
                }
                None
            }
            serde_json::Value::String(s) => {
                if Self::is_error_text(s) {
                    Some(JsonlEvent::Error {
                        message: s.clone(),
                        timestamp: timestamp.map(|s| s.to_string()),
                    })
                } else {
                    Some(JsonlEvent::AssistantText {
                        content: s.clone(),
                        timestamp: timestamp.map(|s| s.to_string()),
                    })
                }
            }
            _ => None,
        }
    }

    /// 检查文本是否为错误信息
    fn is_error_text(text: &str) -> bool {
        let error_patterns = [
            "error:",
            "Error:",
            "ERROR:",
            "failed:",
            "Failed:",
            "FAILED:",
            "permission denied",
            "Permission denied",
            "ENOENT:",
            "EACCES:",
            "panic!",
            "command not found",
        ];

        error_patterns.iter().any(|pattern| text.contains(pattern))
    }

    /// 获取最近的工具调用事件
    pub fn get_recent_tool_calls(&mut self, limit: usize) -> Result<Vec<JsonlEvent>> {
        let events = self.read_all_events()?;
        let tool_calls: Vec<JsonlEvent> = events
            .into_iter()
            .filter(|e| matches!(e, JsonlEvent::ToolUse { .. }))
            .collect();

        let start = if tool_calls.len() > limit {
            tool_calls.len() - limit
        } else {
            0
        };

        Ok(tool_calls[start..].to_vec())
    }

    /// 读取所有事件（从头开始）
    pub fn read_all_events(&mut self) -> Result<Vec<JsonlEvent>> {
        let old_position = self.position;
        self.position = 0;
        let events = self.read_new_events()?;
        self.position = old_position;
        Ok(events)
    }

    /// 获取最近的错误事件
    pub fn get_recent_errors(&mut self, limit: usize) -> Result<Vec<JsonlEvent>> {
        let events = self.read_all_events()?;
        let errors: Vec<JsonlEvent> = events
            .into_iter()
            .filter(|e| matches!(e, JsonlEvent::Error { .. }))
            .collect();

        let start = if errors.len() > limit {
            errors.len() - limit
        } else {
            0
        };

        Ok(errors[start..].to_vec())
    }
}

/// 格式化工具调用为人类可读的字符串
pub fn format_tool_use(event: &JsonlEvent) -> Option<String> {
    if let JsonlEvent::ToolUse { tool_name, input, .. } = event {
        let target = extract_tool_target(tool_name, input);
        if let Some(target) = target {
            Some(format!("{} {}", tool_name, target))
        } else {
            Some(tool_name.clone())
        }
    } else {
        None
    }
}

/// 从工具输入中提取目标（文件路径、命令等）- 公开版本
pub fn extract_tool_target_from_input(tool_name: &str, input: &serde_json::Value) -> Option<String> {
    extract_tool_target(tool_name, input)
}

/// 从工具输入中提取目标（文件路径、命令等）
fn extract_tool_target(tool_name: &str, input: &serde_json::Value) -> Option<String> {
    match tool_name {
        "Read" | "Edit" | "Write" => {
            input.get("file_path").and_then(|p| p.as_str()).map(|s| {
                // 简化路径显示
                s.split('/').last().unwrap_or(s).to_string()
            })
        }
        "Bash" => {
            input.get("command").and_then(|c| c.as_str()).map(|s| {
                // 截断长命令
                if s.len() > 50 {
                    format!("{}...", &s[..47])
                } else {
                    s.to_string()
                }
            })
        }
        "Glob" => {
            input.get("pattern").and_then(|p| p.as_str()).map(|s| s.to_string())
        }
        "Grep" => {
            input.get("pattern").and_then(|p| p.as_str()).map(|s| {
                if s.len() > 30 {
                    format!("{}...", &s[..27])
                } else {
                    s.to_string()
                }
            })
        }
        "Task" => {
            input.get("description").and_then(|d| d.as_str()).map(|s| {
                if s.len() > 40 {
                    format!("{}...", &s[..37])
                } else {
                    s.to_string()
                }
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_use_event() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"test-id","name":"Edit","input":{"file_path":"src/main.rs"}}]},"timestamp":"2026-02-01T10:00:00Z"}"#;

        let event = JsonlParser::parse_line(line).unwrap();

        match event {
            JsonlEvent::ToolUse { tool_name, tool_id, input, .. } => {
                assert_eq!(tool_name, "Edit");
                assert_eq!(tool_id, "test-id");
                assert_eq!(input["file_path"], "src/main.rs");
            }
            _ => panic!("Expected ToolUse event"),
        }
    }

    #[test]
    fn test_parse_user_message() {
        let line = r#"{"type":"user","userMessage":{"content":"Hello world"},"timestamp":"2026-02-01T10:00:00Z"}"#;

        let event = JsonlParser::parse_line(line).unwrap();

        match event {
            JsonlEvent::UserMessage { content, .. } => {
                assert_eq!(content, "Hello world");
            }
            _ => panic!("Expected UserMessage event"),
        }
    }

    #[test]
    fn test_parse_assistant_text() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"This is a response"}]},"timestamp":"2026-02-01T10:00:00Z"}"#;

        let event = JsonlParser::parse_line(line).unwrap();

        match event {
            JsonlEvent::AssistantText { content, .. } => {
                assert_eq!(content, "This is a response");
            }
            _ => panic!("Expected AssistantText event"),
        }
    }

    #[test]
    fn test_parse_error_text() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Error: Permission denied"}]},"timestamp":"2026-02-01T10:00:00Z"}"#;

        let event = JsonlParser::parse_line(line).unwrap();

        match event {
            JsonlEvent::Error { message, .. } => {
                assert!(message.contains("Permission denied"));
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_parse_progress_event() {
        let line = r#"{"type":"progress","data":{"type":"hook_progress","message":"Running hook"},"timestamp":"2026-02-01T10:00:00Z"}"#;

        let event = JsonlParser::parse_line(line).unwrap();

        match event {
            JsonlEvent::Progress { progress_type, .. } => {
                assert_eq!(progress_type, "hook_progress");
            }
            _ => panic!("Expected Progress event"),
        }
    }

    #[test]
    fn test_format_tool_use() {
        let event = JsonlEvent::ToolUse {
            tool_name: "Edit".to_string(),
            tool_id: "test".to_string(),
            input: serde_json::json!({"file_path": "/path/to/main.rs"}),
            timestamp: None,
        };

        let formatted = format_tool_use(&event).unwrap();
        assert_eq!(formatted, "Edit main.rs");
    }

    #[test]
    fn test_is_error_text() {
        assert!(JsonlParser::is_error_text("Error: something went wrong"));
        assert!(JsonlParser::is_error_text("ENOENT: file not found"));
        assert!(JsonlParser::is_error_text("permission denied"));
        assert!(!JsonlParser::is_error_text("This is normal text"));
    }
}
