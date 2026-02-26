// src/agent_mod/adapter/mod.rs
//! Agent CLI 适配器模块
//!
//! 提供统一的抽象层，支持多种 AI 编码工具（Claude Code、Codex CLI、OpenCode 等）

mod types;

pub use types::*;

use crate::agent::AgentType;
use anyhow::Result;

/// Agent CLI 适配器 trait
pub trait AgentAdapter: Send + Sync {
    /// 获取 Agent 类型
    fn agent_type(&self) -> AgentType;

    /// 获取启动命令
    fn get_command(&self) -> &str;

    /// 获取恢复会话命令
    fn get_resume_command(&self, session_id: &str) -> String;

    /// 获取检测策略
    fn detection_strategy(&self) -> DetectionStrategy;

    /// 获取能力描述
    fn capabilities(&self) -> AgentCapabilities;

    /// 获取配置路径
    fn paths(&self) -> AgentPaths;

    /// 检测是否已安装
    fn is_installed(&self) -> bool;

    /// 解析 hook 事件
    fn parse_hook_event(&self, payload: &str) -> Option<HookEvent>;

    /// 检测就绪状态
    fn detect_ready(&self, terminal_output: &str) -> bool;
}

/// 获取适配器
pub fn get_adapter(agent_type: &AgentType) -> Box<dyn AgentAdapter> {
    match agent_type {
        AgentType::Claude => Box::new(claude::ClaudeAdapter),
        AgentType::Codex => Box::new(codex::CodexAdapter),
        AgentType::OpenCode => Box::new(opencode::OpenCodeAdapter),
        _ => Box::new(generic::GenericAdapter::new(agent_type.clone())),
    }
}

pub mod claude;
pub mod codex;
pub mod config_manager;
pub mod generic;
pub mod opencode;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentType;

    #[test]
    fn test_get_adapter_claude() {
        let adapter = get_adapter(&AgentType::Claude);
        assert_eq!(adapter.agent_type(), AgentType::Claude);
    }

    #[test]
    fn test_get_adapter_codex() {
        let adapter = get_adapter(&AgentType::Codex);
        assert_eq!(adapter.agent_type(), AgentType::Codex);
    }

    #[test]
    fn test_get_adapter_opencode() {
        let adapter = get_adapter(&AgentType::OpenCode);
        assert_eq!(adapter.agent_type(), AgentType::OpenCode);
    }

    #[test]
    fn test_get_adapter_gemini_cli() {
        let adapter = get_adapter(&AgentType::GeminiCli);
        assert_eq!(adapter.agent_type(), AgentType::GeminiCli);
    }

    #[test]
    fn test_get_adapter_mistral_vibe() {
        let adapter = get_adapter(&AgentType::MistralVibe);
        assert_eq!(adapter.agent_type(), AgentType::MistralVibe);
    }

    #[test]
    fn test_get_adapter_mock() {
        let adapter = get_adapter(&AgentType::Mock);
        assert_eq!(adapter.agent_type(), AgentType::Mock);
    }

    #[test]
    fn test_get_adapter_unknown() {
        let adapter = get_adapter(&AgentType::Unknown);
        assert_eq!(adapter.agent_type(), AgentType::Unknown);
    }
}
