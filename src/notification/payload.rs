//! Payload 构建模块 - 创建结构化通知 payload
//!
//! 负责将事件信息转换为结构化的 JSON payload，用于发送到 Dashboard 和其他系统。
//!
//! Payload 格式：
//! ```json
//! {
//!   "type": "cam_notification",
//!   "version": "1.0",
//!   "urgency": "HIGH",
//!   "event_type": "permission_request",
//!   "agent_id": "cam-xxx",
//!   "project": "/path/to/project",
//!   "event": { ... },
//!   "summary": "简短摘要",
//!   "timestamp": "2026-02-08T00:00:00Z"
//! }
//! ```

use chrono::Utc;
use serde_json;

use super::summarizer::NotificationSummarizer;
use super::urgency::Urgency;

/// Notification message constants (Chinese)
mod msg {
    pub const WAITING_INPUT: &str = "等待输入";
    pub const NEED_PERMISSION_CONFIRM: &str = "需要权限确认";
    pub const WAITING_USER_INPUT: &str = "等待用户输入";
    pub const ERROR_OCCURRED: &str = "发生错误";
    pub const AGENT_EXITED: &str = "Agent 已退出";
    pub const SESSION_ENDED: &str = "会话已结束";
    pub const SESSION_STARTED: &str = "会话已启动";
    pub const NOTIFICATION: &str = "通知";
    pub const REQUEST_EXECUTE_TOOL: &str = "请求执行";
    pub const EXECUTE_TOOL: &str = "执行工具";
}

/// Payload 构建器
pub struct PayloadBuilder {
    /// 是否禁用 AI 提取（用于测试）
    no_ai: bool,
}

impl PayloadBuilder {
    /// 创建新的 PayloadBuilder
    pub fn new() -> Self {
        Self { no_ai: false }
    }

    /// 设置是否禁用 AI 提取
    pub fn with_no_ai(mut self, no_ai: bool) -> Self {
        self.no_ai = no_ai;
        self
    }

    /// 创建结构化 payload
    ///
    /// # Arguments
    /// * `agent_id` - Agent 标识符
    /// * `event_type` - 事件类型
    /// * `pattern_or_path` - 模式类型或路径
    /// * `context` - 上下文信息（可能包含终端快照）
    /// * `urgency` - 紧急程度
    ///
    /// # Returns
    /// 结构化的 JSON payload
    pub fn create_payload(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
        urgency: Urgency,
    ) -> serde_json::Value {
        // 分离终端快照和原始 context
        let (raw_context, terminal_snapshot) = Self::split_terminal_snapshot(context);

        // 尝试解析 JSON context
        let json: Option<serde_json::Value> = serde_json::from_str(raw_context).ok();

        // 提取项目路径
        let project = json
            .as_ref()
            .and_then(|j| j.get("cwd"))
            .and_then(|v| v.as_str())
            .unwrap_or(pattern_or_path);

        // 构建 event 对象
        let event = self.build_event_object(event_type, pattern_or_path, &json, raw_context);

        // 生成简短摘要
        let summary = self.generate_summary(event_type, &json, pattern_or_path);

        // 对于权限请求，添加风险评估
        let risk_level = if event_type == "permission_request" {
            let tool_name = json
                .as_ref()
                .and_then(|j| j.get("tool_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let tool_input = json
                .as_ref()
                .and_then(|j| j.get("tool_input"))
                .cloned()
                .unwrap_or(serde_json::json!({}));

            let summarizer = NotificationSummarizer::new();
            let perm_summary = summarizer.summarize_permission(tool_name, &tool_input);
            Some(format!("{:?}", perm_summary.risk_level).to_uppercase())
        } else {
            None
        };

        let mut payload = serde_json::json!({
            "type": "cam_notification",
            "version": "1.0",
            "urgency": urgency.as_str(),
            "event_type": event_type,
            "agent_id": agent_id,
            "project": project,
            "timestamp": Utc::now().to_rfc3339(),
            "event": event,
            "summary": summary
        });

        // 添加风险等级（如果有）
        if let Some(risk) = risk_level {
            payload["risk_level"] = serde_json::Value::String(risk);
        }

        // 添加终端快照（如果有）
        if let Some(snapshot) = terminal_snapshot {
            // 截取最后 15 行
            let lines: Vec<&str> = snapshot.lines().collect();
            let truncated = if lines.len() > 15 {
                lines[lines.len() - 15..].join("\n")
            } else {
                snapshot.to_string()
            };
            payload["terminal_snapshot"] = serde_json::Value::String(truncated);
        }

        payload
    }

    /// 分离终端快照和原始 context
    fn split_terminal_snapshot(context: &str) -> (&str, Option<&str>) {
        if let Some(idx) = context.find("\n\n--- 终端快照 ---\n") {
            let (before, after) = context.split_at(idx);
            let snapshot = after.trim_start_matches("\n\n--- 终端快照 ---\n");
            (before, Some(snapshot))
        } else {
            (context, None)
        }
    }

    /// 构建 event 对象
    fn build_event_object(
        &self,
        event_type: &str,
        pattern_or_path: &str,
        json: &Option<serde_json::Value>,
        raw_context: &str,
    ) -> serde_json::Value {
        match event_type {
            "permission_request" => {
                let tool_name = json
                    .as_ref()
                    .and_then(|j| j.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let tool_input = json
                    .as_ref()
                    .and_then(|j| j.get("tool_input"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                serde_json::json!({
                    "tool_name": tool_name,
                    "tool_input": tool_input
                })
            }
            "notification" => {
                let message = json
                    .as_ref()
                    .and_then(|j| j.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let notification_type = json
                    .as_ref()
                    .and_then(|j| j.get("notification_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                serde_json::json!({
                    "notification_type": notification_type,
                    "message": message
                })
            }
            "WaitingForInput" => {
                serde_json::json!({
                    "pattern_type": pattern_or_path,
                    "prompt": raw_context
                })
            }
            "Error" => {
                serde_json::json!({
                    "message": raw_context
                })
            }
            "AgentExited" => {
                serde_json::json!({
                    "project_path": pattern_or_path
                })
            }
            "ToolUse" => {
                serde_json::json!({
                    "tool_name": pattern_or_path,
                    "tool_target": raw_context
                })
            }
            _ => {
                serde_json::json!({
                    "raw_context": raw_context
                })
            }
        }
    }

    /// 生成简短摘要
    fn generate_summary(
        &self,
        event_type: &str,
        json: &Option<serde_json::Value>,
        pattern_or_path: &str,
    ) -> String {
        match event_type {
            "permission_request" => {
                let tool_name = json
                    .as_ref()
                    .and_then(|j| j.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format!("{} {} 工具", msg::REQUEST_EXECUTE_TOOL, tool_name)
            }
            "notification" => {
                let notification_type = json
                    .as_ref()
                    .and_then(|j| j.get("notification_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match notification_type {
                    "idle_prompt" => msg::WAITING_USER_INPUT.to_string(),
                    "permission_prompt" => msg::NEED_PERMISSION_CONFIRM.to_string(),
                    _ => msg::NOTIFICATION.to_string(),
                }
            }
            "WaitingForInput" => format!("{}: {}", msg::WAITING_INPUT, pattern_or_path),
            "Error" => msg::ERROR_OCCURRED.to_string(),
            "AgentExited" => msg::AGENT_EXITED.to_string(),
            "ToolUse" => format!("{}: {}", msg::EXECUTE_TOOL, pattern_or_path),
            "stop" | "session_end" => msg::SESSION_ENDED.to_string(),
            "session_start" => msg::SESSION_STARTED.to_string(),
            _ => event_type.to_string(),
        }
    }
}

impl Default for PayloadBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_payload_permission_request() {
        let builder = PayloadBuilder::new();

        let context = r#"{"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/test"}, "cwd": "/workspace"}"#;
        let payload =
            builder.create_payload("cam-123", "permission_request", "", context, Urgency::High);

        assert_eq!(payload["type"], "cam_notification");
        assert_eq!(payload["version"], "1.0");
        assert_eq!(payload["urgency"], "HIGH");
        assert_eq!(payload["event_type"], "permission_request");
        assert_eq!(payload["agent_id"], "cam-123");
        assert_eq!(payload["project"], "/workspace");
        assert_eq!(payload["event"]["tool_name"], "Bash");
        assert!(payload["event"]["tool_input"]["command"]
            .as_str()
            .unwrap()
            .contains("rm -rf"));
        assert!(payload["summary"].as_str().unwrap().contains("Bash"));
        assert!(payload["timestamp"].as_str().is_some());
    }

    #[test]
    fn test_create_payload_error() {
        let builder = PayloadBuilder::new();

        let payload = builder.create_payload(
            "cam-456",
            "Error",
            "",
            "API rate limit exceeded",
            Urgency::High,
        );

        assert_eq!(payload["type"], "cam_notification");
        assert_eq!(payload["urgency"], "HIGH");
        assert_eq!(payload["event_type"], "Error");
        assert_eq!(payload["event"]["message"], "API rate limit exceeded");
        assert_eq!(payload["summary"], "发生错误");
    }

    #[test]
    fn test_create_payload_waiting_for_input() {
        let builder = PayloadBuilder::new();

        let payload = builder.create_payload(
            "cam-789",
            "WaitingForInput",
            "Confirmation",
            "Continue? [Y/n]",
            Urgency::High,
        );

        assert_eq!(payload["urgency"], "HIGH");
        assert_eq!(payload["event_type"], "WaitingForInput");
        assert_eq!(payload["event"]["pattern_type"], "Confirmation");
        assert_eq!(payload["event"]["prompt"], "Continue? [Y/n]");
        assert!(payload["summary"]
            .as_str()
            .unwrap()
            .contains("Confirmation"));
    }

    #[test]
    fn test_create_payload_agent_exited() {
        let builder = PayloadBuilder::new();

        let payload =
            builder.create_payload("cam-abc", "AgentExited", "/myproject", "", Urgency::Medium);

        assert_eq!(payload["urgency"], "MEDIUM");
        assert_eq!(payload["event_type"], "AgentExited");
        assert_eq!(payload["event"]["project_path"], "/myproject");
        assert_eq!(payload["summary"], "Agent 已退出");
    }

    #[test]
    fn test_create_payload_notification_idle_prompt() {
        let builder = PayloadBuilder::new();

        let context = r#"{"notification_type": "idle_prompt", "message": "Task completed"}"#;
        let payload =
            builder.create_payload("cam-def", "notification", "", context, Urgency::Medium);

        assert_eq!(payload["urgency"], "MEDIUM");
        assert_eq!(payload["event"]["notification_type"], "idle_prompt");
        assert_eq!(payload["event"]["message"], "Task completed");
        assert_eq!(payload["summary"], "等待用户输入");
    }

    #[test]
    fn test_create_payload_with_terminal_snapshot() {
        let builder = PayloadBuilder::new();

        let context = r#"{"cwd": "/workspace"}

--- 终端快照 ---
$ cargo build
   Compiling myapp v0.1.0
    Finished release target"#;

        let payload =
            builder.create_payload("cam-123", "AgentExited", "", context, Urgency::Medium);

        assert_eq!(payload["urgency"], "MEDIUM");
        assert!(payload["terminal_snapshot"].as_str().is_some());
        assert!(payload["terminal_snapshot"]
            .as_str()
            .unwrap()
            .contains("cargo build"));
    }

    #[test]
    fn test_create_payload_snapshot_truncation() {
        let builder = PayloadBuilder::new();

        // 创建超过 15 行的终端输出
        let mut long_output = String::from(
            r#"{"cwd": "/tmp"}

--- 终端快照 ---
"#,
        );
        for i in 1..=20 {
            long_output.push_str(&format!("line {}\n", i));
        }

        let payload =
            builder.create_payload("cam-123", "AgentExited", "", &long_output, Urgency::Medium);

        let snapshot = payload["terminal_snapshot"].as_str().unwrap();
        // 应该只包含最后 15 行
        assert!(snapshot.contains("line 20"));
        assert!(snapshot.contains("line 6"));
        assert!(!snapshot.contains("line 5\n"));
    }

    #[test]
    fn test_generate_summary() {
        let builder = PayloadBuilder::new();

        // permission_request
        let json: Option<serde_json::Value> =
            serde_json::from_str(r#"{"tool_name": "Write"}"#).ok();
        assert!(builder
            .generate_summary("permission_request", &json, "")
            .contains("Write"));

        // Error
        assert_eq!(builder.generate_summary("Error", &None, ""), "发生错误");

        // AgentExited
        assert_eq!(
            builder.generate_summary("AgentExited", &None, ""),
            "Agent 已退出"
        );

        // WaitingForInput
        assert!(builder
            .generate_summary("WaitingForInput", &None, "Confirmation")
            .contains("Confirmation"));
    }

    #[test]
    fn test_split_terminal_snapshot() {
        // With snapshot
        let context = r#"{"cwd": "/workspace"}

--- 终端快照 ---
terminal content"#;
        let (raw, snapshot) = PayloadBuilder::split_terminal_snapshot(context);
        assert_eq!(raw, r#"{"cwd": "/workspace"}"#);
        assert_eq!(snapshot, Some("terminal content"));

        // Without snapshot
        let context2 = r#"{"cwd": "/workspace"}"#;
        let (raw2, snapshot2) = PayloadBuilder::split_terminal_snapshot(context2);
        assert_eq!(raw2, r#"{"cwd": "/workspace"}"#);
        assert_eq!(snapshot2, None);
    }

    #[test]
    fn test_build_event_object_permission_request() {
        let builder = PayloadBuilder::new();
        let json: Option<serde_json::Value> =
            serde_json::from_str(r#"{"tool_name": "Bash", "tool_input": {"command": "ls"}}"#).ok();

        let event = builder.build_event_object("permission_request", "", &json, "");

        assert_eq!(event["tool_name"], "Bash");
        assert_eq!(event["tool_input"]["command"], "ls");
    }

    #[test]
    fn test_build_event_object_notification() {
        let builder = PayloadBuilder::new();
        let json: Option<serde_json::Value> =
            serde_json::from_str(r#"{"notification_type": "idle_prompt", "message": "waiting"}"#)
                .ok();

        let event = builder.build_event_object("notification", "", &json, "");

        assert_eq!(event["notification_type"], "idle_prompt");
        assert_eq!(event["message"], "waiting");
    }

    #[test]
    fn test_build_event_object_tool_use() {
        let builder = PayloadBuilder::new();

        let event = builder.build_event_object("ToolUse", "Edit", &None, "src/main.rs");

        assert_eq!(event["tool_name"], "Edit");
        assert_eq!(event["tool_target"], "src/main.rs");
    }
}
