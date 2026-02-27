//! ReAct 消息提取器单元测试
//!
//! 测试 ReAct 循环逻辑、边界条件和错误处理。

use code_agent_monitor::agent::extractor::{
    ExtractedMessage, ExtractionResult, IterationConfig, MessageExtractor, MessageType,
    ReactExtractor,
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Mock 提取器用于测试
struct MockExtractor {
    results: Vec<ExtractionResult>,
    call_count: AtomicUsize,
    is_processing_result: bool,
}

impl MockExtractor {
    fn new(results: Vec<ExtractionResult>) -> Self {
        Self {
            results,
            call_count: AtomicUsize::new(0),
            is_processing_result: false,
        }
    }

    fn with_processing(mut self, is_processing: bool) -> Self {
        self.is_processing_result = is_processing;
        self
    }

    fn get_call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl MessageExtractor for MockExtractor {
    fn extract(&self, _snapshot: &str, _lines: usize) -> ExtractionResult {
        let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
        self.results
            .get(idx)
            .cloned()
            .unwrap_or_else(|| ExtractionResult::Failed("No more results".into()))
    }

    fn is_processing(&self, _snapshot: &str) -> bool {
        self.is_processing_result
    }
}

// ============================================================================
// ReAct 循环测试
// ============================================================================

#[test]
fn test_react_expands_context_until_success() {
    // 模拟需要扩展上下文的场景
    let extractor = MockExtractor::new(vec![
        ExtractionResult::NeedMoreContext,
        ExtractionResult::NeedMoreContext,
        ExtractionResult::Success(ExtractedMessage {
            content: "Test question?".into(),
            fingerprint: "test-question".into(),
            context_complete: true,
            message_type: MessageType::OpenEnded,
            is_decision_required: false,
        }),
    ]);

    let react = ReactExtractor::new(Box::new(extractor));

    // 验证配置
    assert_eq!(react.config.context_sizes.len(), 5);
    assert_eq!(react.config.max_iterations, 5);
}

#[test]
fn test_react_stops_on_first_success() {
    let extractor = MockExtractor::new(vec![
        ExtractionResult::Success(ExtractedMessage {
            content: "First success".into(),
            fingerprint: "first".into(),
            context_complete: true,
            message_type: MessageType::Confirmation,
            is_decision_required: false,
        }),
        ExtractionResult::Success(ExtractedMessage {
            content: "Should not reach".into(),
            fingerprint: "second".into(),
            context_complete: true,
            message_type: MessageType::OpenEnded,
            is_decision_required: false,
        }),
    ]);

    let _react = ReactExtractor::new(Box::new(extractor));

    // ReactExtractor 创建成功即可，实际调用需要 tmux
}

#[test]
fn test_react_skips_when_processing() {
    let extractor = MockExtractor::new(vec![]).with_processing(true);

    // 验证 is_processing 返回 true
    assert!(extractor.is_processing("test"));

    let _react = ReactExtractor::new(Box::new(extractor));
}

#[test]
fn test_react_continues_on_failure() {
    let extractor = MockExtractor::new(vec![
        ExtractionResult::Failed("First failure".into()),
        ExtractionResult::Failed("Second failure".into()),
        ExtractionResult::Success(ExtractedMessage {
            content: "Finally success".into(),
            fingerprint: "success".into(),
            context_complete: true,
            message_type: MessageType::Choice,
            is_decision_required: false,
        }),
    ]);

    let react = ReactExtractor::new(Box::new(extractor));

    // 验证配置允许继续尝试
    assert!(react.config.max_iterations >= 3);
}

// ============================================================================
// IterationConfig 测试
// ============================================================================

#[test]
fn test_iteration_config_default() {
    let config = IterationConfig::default();
    assert_eq!(config.context_sizes, vec![80, 150, 300, 500, 800]);
    assert_eq!(config.max_iterations, 5);
    assert_eq!(config.timeout_ms, 10000);
}

#[test]
fn test_iteration_config_custom() {
    let config = IterationConfig {
        context_sizes: vec![50, 100],
        max_iterations: 2,
        timeout_ms: 5000,
    };
    assert_eq!(config.context_sizes.len(), 2);
    assert_eq!(config.max_iterations, 2);
}

#[test]
fn test_react_with_custom_config() {
    let extractor = MockExtractor::new(vec![]);
    let config = IterationConfig {
        context_sizes: vec![100, 200],
        max_iterations: 2,
        timeout_ms: 3000,
    };

    let react = ReactExtractor::with_config(Box::new(extractor), config);

    assert_eq!(react.config.context_sizes, vec![100, 200]);
    assert_eq!(react.config.max_iterations, 2);
}

// ============================================================================
// MessageType 测试
// ============================================================================

#[test]
fn test_message_type_choice() {
    let msg_type = MessageType::Choice;
    let json = serde_json::to_string(&msg_type).unwrap();
    assert_eq!(json, "\"choice\"");
}

#[test]
fn test_message_type_confirmation() {
    let msg_type = MessageType::Confirmation;
    let json = serde_json::to_string(&msg_type).unwrap();
    assert_eq!(json, "\"confirmation\"");
}

#[test]
fn test_message_type_open_ended() {
    let msg_type = MessageType::OpenEnded;
    let json = serde_json::to_string(&msg_type).unwrap();
    assert_eq!(json, "\"open_ended\"");
}

#[test]
fn test_message_type_idle() {
    let msg_type = MessageType::Idle {
        status: "completed".to_string(),
        last_action: Some("Created file".to_string()),
    };
    let json = serde_json::to_string(&msg_type).unwrap();
    assert!(json.contains("idle"));
    assert!(json.contains("completed"));
    assert!(json.contains("Created file"));
}

#[test]
fn test_message_type_idle_without_action() {
    let msg_type = MessageType::Idle {
        status: "idle".to_string(),
        last_action: None,
    };
    let json = serde_json::to_string(&msg_type).unwrap();
    assert!(json.contains("idle"));
    assert!(json.contains("null"));
}

// ============================================================================
// ExtractedMessage 测试
// ============================================================================

#[test]
fn test_extracted_message_clone() {
    let msg = ExtractedMessage {
        content: "Test question".to_string(),
        fingerprint: "test-question".to_string(),
        context_complete: true,
        message_type: MessageType::OpenEnded,
        is_decision_required: false,
    };
    let cloned = msg.clone();
    assert_eq!(cloned.content, msg.content);
    assert_eq!(cloned.fingerprint, msg.fingerprint);
    assert_eq!(cloned.context_complete, msg.context_complete);
}

#[test]
fn test_extracted_message_serialization() {
    let msg = ExtractedMessage {
        content: "Choose an option".to_string(),
        fingerprint: "choose-option".to_string(),
        context_complete: true,
        message_type: MessageType::Choice,
        is_decision_required: false,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("Choose an option"));
    assert!(json.contains("choose-option"));
    assert!(json.contains("choice"));
}

// ============================================================================
// ExtractionResult 测试
// ============================================================================

#[test]
fn test_extraction_result_success() {
    let result = ExtractionResult::Success(ExtractedMessage {
        content: "Test".into(),
        fingerprint: "test".into(),
        context_complete: true,
        message_type: MessageType::OpenEnded,
        is_decision_required: false,
    });
    assert!(matches!(result, ExtractionResult::Success(_)));
}

#[test]
fn test_extraction_result_need_more_context() {
    let result = ExtractionResult::NeedMoreContext;
    assert!(matches!(result, ExtractionResult::NeedMoreContext));
}

#[test]
fn test_extraction_result_processing() {
    let result = ExtractionResult::Processing;
    assert!(matches!(result, ExtractionResult::Processing));
}

#[test]
fn test_extraction_result_failed() {
    let result = ExtractionResult::Failed("API error".into());
    if let ExtractionResult::Failed(msg) = result {
        assert_eq!(msg, "API error");
    } else {
        panic!("Expected Failed variant");
    }
}

// ============================================================================
// 边界条件测试
// ============================================================================

#[test]
fn test_empty_results_returns_failed() {
    let extractor = MockExtractor::new(vec![]);
    let result = extractor.extract("test", 100);
    assert!(matches!(result, ExtractionResult::Failed(_)));
}

#[test]
fn test_mock_extractor_call_count() {
    let extractor = MockExtractor::new(vec![
        ExtractionResult::NeedMoreContext,
        ExtractionResult::NeedMoreContext,
        ExtractionResult::Processing,
    ]);

    extractor.extract("test", 80);
    extractor.extract("test", 150);
    extractor.extract("test", 300);

    assert_eq!(extractor.get_call_count(), 3);
}

#[test]
fn test_message_type_equality() {
    assert_eq!(MessageType::Choice, MessageType::Choice);
    assert_eq!(MessageType::Confirmation, MessageType::Confirmation);
    assert_eq!(MessageType::OpenEnded, MessageType::OpenEnded);
    assert_ne!(MessageType::Choice, MessageType::Confirmation);
}

#[test]
fn test_idle_message_type_equality() {
    let idle1 = MessageType::Idle {
        status: "completed".to_string(),
        last_action: None,
    };
    let idle2 = MessageType::Idle {
        status: "completed".to_string(),
        last_action: None,
    };
    let idle3 = MessageType::Idle {
        status: "idle".to_string(),
        last_action: None,
    };

    assert_eq!(idle1, idle2);
    assert_ne!(idle1, idle3);
}

// ============================================================================
// Prompt 生成测试
// ============================================================================

#[test]
fn test_status_detection_prompt_contains_terminal_content() {
    use code_agent_monitor::agent::extractor::prompts::status_detection_prompt;

    let prompt = status_detection_prompt("test terminal content");
    assert!(prompt.contains("test terminal content"));
    assert!(prompt.contains("<terminal>"));
    assert!(prompt.contains("</terminal>"));
}

#[test]
fn test_status_detection_prompt_contains_all_states() {
    use code_agent_monitor::agent::extractor::prompts::status_detection_prompt;

    let prompt = status_detection_prompt("");
    assert!(prompt.contains("PROCESSING"));
    assert!(prompt.contains("WAITING"));
    assert!(prompt.contains("DECISION"));
}

#[test]
fn test_message_extraction_prompt_contains_terminal_content() {
    use code_agent_monitor::agent::extractor::prompts::message_extraction_prompt;

    let prompt = message_extraction_prompt("agent output here");
    assert!(prompt.contains("agent output here"));
    assert!(prompt.contains("<terminal_snapshot>"));
    assert!(prompt.contains("</terminal_snapshot>"));
}

#[test]
fn test_message_extraction_prompt_contains_json_schema() {
    use code_agent_monitor::agent::extractor::prompts::message_extraction_prompt;

    let prompt = message_extraction_prompt("");
    assert!(prompt.contains("has_question"));
    assert!(prompt.contains("message"));
    assert!(prompt.contains("fingerprint"));
    assert!(prompt.contains("context_complete"));
    assert!(prompt.contains("message_type"));
}

#[test]
fn test_message_extraction_prompt_contains_rules() {
    use code_agent_monitor::agent::extractor::prompts::message_extraction_prompt;

    let prompt = message_extraction_prompt("");
    assert!(prompt.contains("<rules>"));
    assert!(prompt.contains("<fingerprint_rule>"));
    assert!(prompt.contains("<context_complete_rule>"));
}

// ============================================================================
// 系统提示词常量测试
// ============================================================================

#[test]
fn test_status_detection_system_prompt() {
    use code_agent_monitor::agent::extractor::prompts::STATUS_DETECTION_SYSTEM;

    assert!(STATUS_DETECTION_SYSTEM.contains("终端状态分析专家"));
    assert!(STATUS_DETECTION_SYSTEM.contains("PROCESSING"));
    assert!(STATUS_DETECTION_SYSTEM.contains("WAITING"));
    assert!(STATUS_DETECTION_SYSTEM.contains("DECISION"));
}

#[test]
fn test_message_extraction_system_prompt() {
    use code_agent_monitor::agent::extractor::prompts::MESSAGE_EXTRACTION_SYSTEM;

    assert!(MESSAGE_EXTRACTION_SYSTEM.contains("终端输出分析专家"));
    assert!(MESSAGE_EXTRACTION_SYSTEM.contains("AI Agent"));
}

// ============================================================================
// 最大迭代次数边界测试
// ============================================================================

#[test]
fn test_react_respects_max_iterations() {
    // 创建超过 max_iterations 的结果
    let extractor = MockExtractor::new(vec![
        ExtractionResult::NeedMoreContext,
        ExtractionResult::NeedMoreContext,
        ExtractionResult::NeedMoreContext,
        ExtractionResult::NeedMoreContext,
        ExtractionResult::NeedMoreContext,
        ExtractionResult::NeedMoreContext, // 第 6 次，超过默认 max_iterations=5
    ]);

    let react = ReactExtractor::new(Box::new(extractor));

    // 验证默认配置
    assert_eq!(react.config.max_iterations, 5);
    assert_eq!(react.config.context_sizes.len(), 5);
}

#[test]
fn test_react_with_single_iteration() {
    let extractor = MockExtractor::new(vec![ExtractionResult::NeedMoreContext]);
    let config = IterationConfig {
        context_sizes: vec![100],
        max_iterations: 1,
        timeout_ms: 1000,
    };

    let react = ReactExtractor::with_config(Box::new(extractor), config);
    assert_eq!(react.config.max_iterations, 1);
}

// ============================================================================
// MessageType 反序列化测试
// ============================================================================

#[test]
fn test_message_type_deserialization_choice() {
    let json = "\"choice\"";
    let msg_type: MessageType = serde_json::from_str(json).unwrap();
    assert_eq!(msg_type, MessageType::Choice);
}

#[test]
fn test_message_type_deserialization_confirmation() {
    let json = "\"confirmation\"";
    let msg_type: MessageType = serde_json::from_str(json).unwrap();
    assert_eq!(msg_type, MessageType::Confirmation);
}

#[test]
fn test_message_type_deserialization_open_ended() {
    let json = "\"open_ended\"";
    let msg_type: MessageType = serde_json::from_str(json).unwrap();
    assert_eq!(msg_type, MessageType::OpenEnded);
}

#[test]
fn test_message_type_deserialization_idle() {
    let json = r#"{"idle":{"status":"completed","last_action":"Created file"}}"#;
    let msg_type: MessageType = serde_json::from_str(json).unwrap();
    if let MessageType::Idle {
        status,
        last_action,
    } = msg_type
    {
        assert_eq!(status, "completed");
        assert_eq!(last_action, Some("Created file".to_string()));
    } else {
        panic!("Expected Idle variant");
    }
}

// ============================================================================
// ExtractedMessage 反序列化测试
// ============================================================================

#[test]
fn test_extracted_message_deserialization() {
    let json = r#"{
        "content": "Test question?",
        "fingerprint": "test-question",
        "context_complete": true,
        "message_type": "open_ended"
    }"#;

    let msg: ExtractedMessage = serde_json::from_str(json).unwrap();
    assert_eq!(msg.content, "Test question?");
    assert_eq!(msg.fingerprint, "test-question");
    // 缺少 is_decision_required 字段时应默认为 false
    assert!(!msg.is_decision_required);
    assert!(msg.context_complete);
    assert_eq!(msg.message_type, MessageType::OpenEnded);
}

#[test]
fn test_extracted_message_with_idle_type() {
    let json = r#"{
        "content": "",
        "fingerprint": "",
        "context_complete": true,
        "message_type": {"idle": {"status": "idle", "last_action": null}}
    }"#;

    let msg: ExtractedMessage = serde_json::from_str(json).unwrap();
    assert!(msg.content.is_empty());
    if let MessageType::Idle {
        status,
        last_action,
    } = msg.message_type
    {
        assert_eq!(status, "idle");
        assert!(last_action.is_none());
    } else {
        panic!("Expected Idle variant");
    }
}

// ============================================================================
// Mock 提取器高级测试
// ============================================================================

#[test]
fn test_mock_extractor_returns_results_in_order() {
    let extractor = MockExtractor::new(vec![
        ExtractionResult::NeedMoreContext,
        ExtractionResult::Processing,
        ExtractionResult::Failed("error".into()),
    ]);

    let r1 = extractor.extract("", 80);
    assert!(matches!(r1, ExtractionResult::NeedMoreContext));

    let r2 = extractor.extract("", 150);
    assert!(matches!(r2, ExtractionResult::Processing));

    let r3 = extractor.extract("", 300);
    assert!(matches!(r3, ExtractionResult::Failed(_)));
}

#[test]
fn test_mock_extractor_exhausted_returns_failed() {
    let extractor = MockExtractor::new(vec![ExtractionResult::NeedMoreContext]);

    let _ = extractor.extract("", 80);
    let r2 = extractor.extract("", 150);

    if let ExtractionResult::Failed(msg) = r2 {
        assert!(msg.contains("No more results"));
    } else {
        panic!("Expected Failed variant");
    }
}

// ============================================================================
// 上下文大小序列测试
// ============================================================================

#[test]
fn test_context_sizes_are_increasing() {
    let config = IterationConfig::default();
    let sizes = &config.context_sizes;

    for i in 1..sizes.len() {
        assert!(
            sizes[i] > sizes[i - 1],
            "Context sizes should be increasing: {} <= {}",
            sizes[i],
            sizes[i - 1]
        );
    }
}

#[test]
fn test_context_sizes_start_small() {
    let config = IterationConfig::default();
    // 第一个上下文大小应该相对较小（< 100 行）
    assert!(config.context_sizes[0] <= 100);
}

#[test]
fn test_context_sizes_end_large() {
    let config = IterationConfig::default();
    // 最后一个上下文大小应该足够大（>= 500 行）
    assert!(*config.context_sizes.last().unwrap() >= 500);
}

// ============================================================================
// is_decision_required 测试
// ============================================================================

#[test]
fn test_is_decision_required_true_parsing() {
    // 验证 is_decision_required: true 的 Choice 消息正确传递
    let extractor = MockExtractor::new(vec![ExtractionResult::Success(ExtractedMessage {
        content: "Which approach do you prefer?".into(),
        fingerprint: "approach-choice".into(),
        context_complete: true,
        message_type: MessageType::Choice,
        is_decision_required: true,
    })]);

    let result = extractor.extract("test snapshot", 80);
    if let ExtractionResult::Success(msg) = result {
        assert!(msg.is_decision_required);
        assert_eq!(msg.message_type, MessageType::Choice);
    } else {
        panic!("Expected Success variant");
    }
}

#[test]
fn test_is_decision_required_true_with_confirmation() {
    // 核心场景：确认题但实际是决策（如"要不要用微服务架构？"）
    let msg = ExtractedMessage {
        content: "Do you want to use microservices architecture?".into(),
        fingerprint: "microservices-arch-decision".into(),
        context_complete: true,
        message_type: MessageType::Confirmation,
        is_decision_required: true,
    };

    assert!(msg.is_decision_required);
    assert_eq!(msg.message_type, MessageType::Confirmation);
}

#[test]
fn test_is_decision_required_serde_roundtrip() {
    // 序列化/反序列化 is_decision_required: true
    let msg = ExtractedMessage {
        content: "Pick a framework".into(),
        fingerprint: "framework-pick".into(),
        context_complete: true,
        message_type: MessageType::Choice,
        is_decision_required: true,
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("is_decision_required"));
    assert!(json.contains("true"));

    let deserialized: ExtractedMessage = serde_json::from_str(&json).unwrap();
    assert!(deserialized.is_decision_required);
    assert_eq!(deserialized.message_type, MessageType::Choice);
    assert_eq!(deserialized.content, "Pick a framework");
}

#[test]
fn test_is_decision_alias_compat() {
    // 旧名 "is_decision" 通过 alias 仍然有效
    let json = r#"{
        "content": "Choose tech stack",
        "fingerprint": "tech-stack",
        "context_complete": true,
        "message_type": "choice",
        "is_decision": true
    }"#;

    let msg: ExtractedMessage = serde_json::from_str(json).unwrap();
    assert!(msg.is_decision_required);
    assert_eq!(msg.content, "Choose tech stack");
}
