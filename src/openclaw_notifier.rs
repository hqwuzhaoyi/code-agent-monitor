//! OpenClaw 通知模块 - 通过 openclaw CLI 发送事件到 clawdbot

use anyhow::Result;
use std::process::Command;

/// OpenClaw 通知器
pub struct OpenclawNotifier {
    /// openclaw 命令路径
    openclaw_cmd: String,
    /// 目标 session id
    session_id: String,
}

impl OpenclawNotifier {
    /// 创建新的通知器
    pub fn new() -> Self {
        Self {
            openclaw_cmd: Self::find_openclaw_path(),
            session_id: "main".to_string(),
        }
    }

    /// 创建指定 session 的通知器
    pub fn with_session(session_id: &str) -> Self {
        Self {
            openclaw_cmd: Self::find_openclaw_path(),
            session_id: session_id.to_string(),
        }
    }

    /// 查找 openclaw 可执行文件路径
    fn find_openclaw_path() -> String {
        let possible_paths = [
            "/Users/admin/.volta/bin/openclaw",
            "/opt/homebrew/bin/openclaw",
            "/usr/local/bin/openclaw",
            "openclaw",
        ];

        for path in possible_paths {
            if std::path::Path::new(path).exists() || path == "openclaw" {
                return path.to_string();
            }
        }

        "openclaw".to_string()
    }

    /// 格式化事件消息
    pub fn format_event(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> String {
        // 尝试解析 JSON context 获取更多信息
        let json: Option<serde_json::Value> = serde_json::from_str(context).ok();

        match event_type {
            "permission_request" => {
                // 提取工具名和输入
                let tool_name = json.as_ref()
                    .and_then(|j| j.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let tool_input = json.as_ref()
                    .and_then(|j| j.get("tool_input"))
                    .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                    .unwrap_or_default();
                let cwd = json.as_ref()
                    .and_then(|j| j.get("cwd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                format!(
                    "🔐 [CAM] {} 请求权限\n\n工具: {}\n目录: {}\n参数:\n```\n{}\n```\n\n请回复: 1=允许, 2=允许并记住, 3=拒绝",
                    agent_id, tool_name, cwd, tool_input
                )
            }
            "notification" => {
                let message = json.as_ref()
                    .and_then(|j| j.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let notification_type = json.as_ref()
                    .and_then(|j| j.get("notification_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if notification_type == "idle_prompt" {
                    format!("⏸️ [CAM] {} 等待输入\n\n{}", agent_id, message)
                } else if notification_type == "permission_prompt" {
                    format!("🔐 [CAM] {} 需要权限确认\n\n{}\n\n请回复: 1=允许, 2=允许并记住, 3=拒绝", agent_id, message)
                } else {
                    format!("📢 [CAM] {} 通知\n\n{}", agent_id, message)
                }
            }
            "session_start" => {
                let cwd = json.as_ref()
                    .and_then(|j| j.get("cwd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("🚀 [CAM] {} 已启动\n\n目录: {}", agent_id, cwd)
            }
            "session_end" | "stop" => {
                let cwd = json.as_ref()
                    .and_then(|j| j.get("cwd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("✅ [CAM] {} 已停止\n\n目录: {}", agent_id, cwd)
            }
            "WaitingForInput" => {
                format!(
                    "⏸️ [CAM] {} 等待输入\n\n类型: {}\n上下文:\n---\n{}\n---\n\n请问如何响应？",
                    agent_id, pattern_or_path, context
                )
            }
            "Error" => {
                format!(
                    "❌ [CAM] {} 发生错误\n\n错误信息:\n---\n{}\n---\n\n请问如何处理？",
                    agent_id, context
                )
            }
            "AgentExited" => {
                let last_output = if context.is_empty() {
                    String::new()
                } else {
                    format!("\n\n最后输出:\n---\n{}\n---", context)
                };
                format!(
                    "✅ [CAM] {} 已退出\n\n项目: {}{}",
                    agent_id, pattern_or_path, last_output
                )
            }
            _ => format!("[CAM] {} - {}: {}", agent_id, event_type, context),
        }
    }

    /// 发送事件到 clawdbot
    pub fn send_event(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> Result<()> {
        let message = self.format_event(agent_id, event_type, pattern_or_path, context);
        self.send_message(&message)
    }

    /// 发送消息到 clawdbot
    pub fn send_message(&self, message: &str) -> Result<()> {
        let result = Command::new(&self.openclaw_cmd)
            .args([
                "agent",
                "--session-id",
                &self.session_id,
                "--message",
                message,
            ])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("OpenClaw 通知失败: {}", stderr);
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("无法执行 OpenClaw: {}", e);
                Err(e.into())
            }
        }
    }
}

impl Default for OpenclawNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_waiting_event() {
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
            "cam-1234567890",
            "WaitingForInput",
            "Confirmation",
            "Do you want to continue? [Y/n]",
        );

        assert!(message.contains("cam-1234567890"));
        assert!(message.contains("等待输入"));
        assert!(message.contains("[Y/n]"));
    }

    #[test]
    fn test_format_error_event() {
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
            "cam-1234567890",
            "Error",
            "",
            "API rate limit exceeded",
        );

        assert!(message.contains("错误"));
        assert!(message.contains("API rate limit"));
    }

    #[test]
    fn test_format_exited_event() {
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
            "cam-1234567890",
            "AgentExited",
            "/workspace/myapp",
            "",
        );

        assert!(message.contains("已退出"));
        assert!(message.contains("/workspace/myapp"));
    }
}
