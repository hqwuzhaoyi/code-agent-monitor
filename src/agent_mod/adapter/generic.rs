// src/agent_mod/adapter/generic.rs
//! 通用适配器
//!
//! 用于未知或自定义 CLI 工具，使用 PollingOnly 检测策略

use super::*;
use crate::agent::AgentType;

/// 通用适配器，用于未知或自定义 CLI
pub struct GenericAdapter {
    agent_type: AgentType,
    command: String,
}

impl GenericAdapter {
    /// 创建新的通用适配器
    ///
    /// 根据 AgentType 自动推断命令名称
    pub fn new(agent_type: AgentType) -> Self {
        let command = match &agent_type {
            AgentType::GeminiCli => "gemini".to_string(),
            AgentType::MistralVibe => "vibe".to_string(),
            AgentType::Mock => "sleep 3600".to_string(), // 测试用，保持 session 存活
            _ => "echo".to_string(),
        };
        Self {
            agent_type,
            command,
        }
    }

    /// 使用自定义命令创建适配器
    pub fn with_command(agent_type: AgentType, command: String) -> Self {
        Self {
            agent_type,
            command,
        }
    }
}

impl AgentAdapter for GenericAdapter {
    fn agent_type(&self) -> AgentType {
        self.agent_type.clone()
    }

    fn get_command(&self) -> &str {
        &self.command
    }

    fn get_resume_command(&self, session_id: &str) -> String {
        // Validate session_id even though generic adapter doesn't use it,
        // to maintain consistent security behavior across all adapters
        if !session_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            panic!("Invalid session_id format: only alphanumeric, hyphen, and underscore allowed");
        }
        // 通用适配器不支持恢复会话，直接返回启动命令
        self.command.clone()
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        // 通用适配器只能用轮询检测
        DetectionStrategy::PollingOnly
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: false,
            hook_events: vec![],
            mcp_support: false,
            json_output: false,
        }
    }

    fn paths(&self) -> AgentPaths {
        AgentPaths {
            config: None,
            sessions: None,
            logs: None,
        }
    }

    fn is_installed(&self) -> bool {
        which::which(&self.command).is_ok()
    }

    fn parse_hook_event(&self, _payload: &str) -> Option<HookEvent> {
        // 通用适配器不解析 hook 事件
        None
    }

    fn detect_ready(&self, terminal_output: &str) -> bool {
        // 使用简单的非空检测作为后备
        // 实际使用时会配合 AI 检测
        !terminal_output.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_with_gemini_cli() {
        let adapter = GenericAdapter::new(AgentType::GeminiCli);
        assert_eq!(adapter.get_command(), "gemini");
        assert_eq!(adapter.agent_type(), AgentType::GeminiCli);
    }

    #[test]
    fn test_new_with_mistral_vibe() {
        let adapter = GenericAdapter::new(AgentType::MistralVibe);
        assert_eq!(adapter.get_command(), "vibe");
        assert_eq!(adapter.agent_type(), AgentType::MistralVibe);
    }

    #[test]
    fn test_new_with_unknown() {
        let adapter = GenericAdapter::new(AgentType::Unknown);
        assert_eq!(adapter.get_command(), "echo");
        assert_eq!(adapter.agent_type(), AgentType::Unknown);
    }

    #[test]
    fn test_with_command() {
        let adapter = GenericAdapter::with_command(AgentType::Unknown, "my-custom-cli".to_string());
        assert_eq!(adapter.get_command(), "my-custom-cli");
        assert_eq!(adapter.agent_type(), AgentType::Unknown);
    }

    #[test]
    fn test_detection_strategy() {
        let adapter = GenericAdapter::new(AgentType::Unknown);
        assert_eq!(adapter.detection_strategy(), DetectionStrategy::PollingOnly);
    }

    #[test]
    fn test_capabilities_no_hooks() {
        let adapter = GenericAdapter::new(AgentType::Unknown);
        let caps = adapter.capabilities();
        assert!(!caps.native_hooks);
        assert!(caps.hook_events.is_empty());
        assert!(!caps.mcp_support);
        assert!(!caps.json_output);
    }

    #[test]
    fn test_paths_all_none() {
        let adapter = GenericAdapter::new(AgentType::Unknown);
        let paths = adapter.paths();
        assert!(paths.config.is_none());
        assert!(paths.sessions.is_none());
        assert!(paths.logs.is_none());
    }

    #[test]
    fn test_parse_hook_event_returns_none() {
        let adapter = GenericAdapter::new(AgentType::Unknown);
        assert!(adapter.parse_hook_event("{}").is_none());
        assert!(adapter.parse_hook_event(r#"{"event":"test"}"#).is_none());
    }

    #[test]
    fn test_detect_ready() {
        let adapter = GenericAdapter::new(AgentType::Unknown);
        assert!(adapter.detect_ready("Some output"));
        assert!(adapter.detect_ready("> "));
        assert!(!adapter.detect_ready(""));
    }

    #[test]
    fn test_get_resume_command_returns_base_command() {
        let adapter = GenericAdapter::new(AgentType::GeminiCli);
        // 通用适配器不支持恢复，返回原始命令
        assert_eq!(adapter.get_resume_command("session-123"), "gemini");
    }

    #[test]
    fn test_get_resume_command_with_hyphen_underscore() {
        let adapter = GenericAdapter::new(AgentType::GeminiCli);
        assert_eq!(adapter.get_resume_command("session-123_abc"), "gemini");
    }

    #[test]
    #[should_panic(expected = "Invalid session_id format")]
    fn test_get_resume_command_rejects_shell_injection() {
        let adapter = GenericAdapter::new(AgentType::GeminiCli);
        adapter.get_resume_command("abc; rm -rf /");
    }

    #[test]
    #[should_panic(expected = "Invalid session_id format")]
    fn test_get_resume_command_rejects_spaces() {
        let adapter = GenericAdapter::new(AgentType::GeminiCli);
        adapter.get_resume_command("abc def");
    }
}
