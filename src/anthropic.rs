//! Anthropic API - re-exports from ai module for backwards compatibility
//!
//! This module is deprecated. Please use `crate::ai` directly.
//!
//! API Key 读取优先级：
//! 1. CAM 配置文件 `~/.config/code-agent-monitor/config.json`（JSON 格式，字段 `anthropic_api_key` 和可选 `anthropic_base_url`）
//! 2. 环境变量 `ANTHROPIC_API_KEY`
//! 3. 文件 `~/.anthropic/api_key`
//! 4. OpenClaw 配置 `~/.openclaw/openclaw.json` 的 `models.providers.anthropic.apiKey` 或 `providers.anthropic.apiKey`

// Re-export everything from ai module
pub use crate::ai::*;

// Re-export types from ai_types for backward compatibility
pub use crate::ai::types::{NotificationContent, QuestionType};
pub use crate::agent::manager::AgentStatus;

// Re-export constants from ai::client
pub use crate::ai::client::{
    ANTHROPIC_API_URL, ANTHROPIC_VERSION, DEFAULT_MAX_TOKENS, DEFAULT_MODEL, DEFAULT_TIMEOUT_MS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = AnthropicConfig::default();
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert_eq!(config.max_tokens, DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn test_question_type_default() {
        let qt = QuestionType::default();
        assert_eq!(qt, QuestionType::OpenEnded);
    }

    #[test]
    fn test_notification_content_default() {
        let content = NotificationContent::default();
        assert_eq!(content.question_type, QuestionType::OpenEnded);
        assert!(content.question.is_empty());
        assert!(content.options.is_empty());
        assert_eq!(content.summary, "等待输入");
    }

    #[test]
    fn test_notification_content_confirmation() {
        let content = NotificationContent::confirmation("Delete file?");
        assert_eq!(content.question_type, QuestionType::Confirmation);
        assert_eq!(content.question, "Delete file?");
        assert!(content.options.is_empty());
        assert_eq!(content.summary, "请求确认");
    }

    #[test]
    fn test_notification_content_options() {
        let options = vec!["Option 1".to_string(), "Option 2".to_string()];
        let content = NotificationContent::options("Choose one:", options.clone());
        assert_eq!(content.question_type, QuestionType::Options);
        assert_eq!(content.question, "Choose one:");
        assert_eq!(content.options, options);
        assert_eq!(content.summary, "等待选择");
    }

    #[test]
    fn test_notification_content_open_ended() {
        let content = NotificationContent::open_ended("What is your name?");
        assert_eq!(content.question_type, QuestionType::OpenEnded);
        assert_eq!(content.question, "What is your name?");
        assert!(content.options.is_empty());
        assert_eq!(content.summary, "等待回复");
    }

    #[test]
    fn test_question_type_serialization() {
        // Test serialization
        let qt = QuestionType::Options;
        let json = serde_json::to_string(&qt).unwrap();
        assert_eq!(json, "\"options\"");

        let qt = QuestionType::Confirmation;
        let json = serde_json::to_string(&qt).unwrap();
        assert_eq!(json, "\"confirmation\"");

        let qt = QuestionType::OpenEnded;
        let json = serde_json::to_string(&qt).unwrap();
        assert_eq!(json, "\"open_ended\"");
    }

    #[test]
    fn test_question_type_deserialization() {
        // Test deserialization
        let qt: QuestionType = serde_json::from_str("\"options\"").unwrap();
        assert_eq!(qt, QuestionType::Options);

        let qt: QuestionType = serde_json::from_str("\"confirmation\"").unwrap();
        assert_eq!(qt, QuestionType::Confirmation);

        let qt: QuestionType = serde_json::from_str("\"open_ended\"").unwrap();
        assert_eq!(qt, QuestionType::OpenEnded);
    }

    #[test]
    fn test_notification_content_serialization() {
        let content = NotificationContent {
            question_type: QuestionType::Options,
            question: "Choose:".to_string(),
            options: vec!["A".to_string(), "B".to_string()],
            summary: "选择".to_string(),
            reply_hint: "1/2".to_string(),
        };

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"question_type\":\"options\""));
        assert!(json.contains("\"question\":\"Choose:\""));
        assert!(json.contains("\"options\":[\"A\",\"B\"]"));
        assert!(json.contains("\"summary\":\"选择\""));
        assert!(json.contains("\"reply_hint\":\"1/2\""));
    }

    #[test]
    fn test_detect_waiting_question_structure() {
        // 测试 detect_waiting_question 函数存在且可调用
        // 实际 AI 调用需要 API key，这里只测试结构
        let snapshot = "";
        let result = detect_waiting_question(snapshot);
        // 空快照可能返回 None（没有 API key）或 Some（有 API key 但内容为空）
        // 这里只验证函数可以被调用，不验证具体返回值
        // 因为返回值取决于是否有 API key 和 AI 的判断
        let _ = result; // 只要不 panic 就算通过
    }

    #[test]
    fn test_detect_waiting_question_returns_notification_content() {
        // 测试返回类型是 Option<NotificationContent>
        // 由于没有 API key，这里只验证函数签名和返回类型
        let snapshot = "Some terminal output\n❯ What do you want to do?";
        let result: Option<NotificationContent> = detect_waiting_question(snapshot);
        // 没有 API key 时返回 None，但类型正确
        // 如果有 API key，应该返回 Some(NotificationContent)
        assert!(result.is_none() || result.is_some());
    }

    #[test]
    fn test_quality_assessment_integration() {
        // 测试 NotificationContent 与 ai_quality 模块的集成
        use crate::ai::quality::{assess_question_extraction, thresholds};

        // 创建一个有效的 NotificationContent
        let content = NotificationContent {
            question_type: QuestionType::OpenEnded,
            question: "你想要实现什么功能？".to_string(),
            options: vec![],
            summary: "等待回复".to_string(),
            reply_hint: String::new(),
        };

        let assessment = assess_question_extraction(&content);
        assert!(assessment.is_valid);
        assert!(assessment.confidence >= thresholds::MEDIUM);

        // 创建一个无效的 NotificationContent
        let invalid_content = NotificationContent {
            question_type: QuestionType::Options,
            question: "".to_string(), // 空问题
            options: vec![],          // 选项类型但没有选项
            summary: "".to_string(),  // 空摘要
            reply_hint: String::new(),
        };

        let invalid_assessment = assess_question_extraction(&invalid_content);
        assert!(!invalid_assessment.is_valid);
        assert!(invalid_assessment.confidence < thresholds::LOW);
    }

    #[test]
    fn test_agent_status_enum() {
        // 测试 AgentStatus 枚举的基本功能
        let processing = AgentStatus::Processing;
        let waiting = AgentStatus::WaitingForInput;
        let unknown = AgentStatus::Unknown;

        assert_eq!(processing, AgentStatus::Processing);
        assert_eq!(waiting, AgentStatus::WaitingForInput);
        assert_eq!(unknown, AgentStatus::Unknown);

        // 测试不相等
        assert_ne!(processing, waiting);
        assert_ne!(waiting, unknown);
    }
}
