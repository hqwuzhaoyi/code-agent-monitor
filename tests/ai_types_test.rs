//! TDD tests for ai_types module
//!
//! These tests define the expected behavior of the ai_types module,
//! which will contain shared types extracted from anthropic.rs to
//! resolve circular dependencies with ai_quality.rs.
//!
//! Types to be extracted:
//! - AgentStatus: Processing, WaitingForInput, Unknown
//! - QuestionType: Options, Confirmation, OpenEnded
//! - NotificationContent: question_type, question, options, summary

use code_agent_monitor::ai_types::{AgentStatus, NotificationContent, QuestionType};

// ============================================================================
// QuestionType Tests
// ============================================================================

mod question_type_tests {
    use super::*;

    #[test]
    fn test_question_type_variants_exist() {
        // Verify all three variants exist
        let _options = QuestionType::Options;
        let _confirmation = QuestionType::Confirmation;
        let _open_ended = QuestionType::OpenEnded;
    }

    #[test]
    fn test_question_type_default_is_open_ended() {
        let default_type = QuestionType::default();
        assert_eq!(default_type, QuestionType::OpenEnded);
    }

    #[test]
    fn test_question_type_equality() {
        assert_eq!(QuestionType::Options, QuestionType::Options);
        assert_eq!(QuestionType::Confirmation, QuestionType::Confirmation);
        assert_eq!(QuestionType::OpenEnded, QuestionType::OpenEnded);

        assert_ne!(QuestionType::Options, QuestionType::Confirmation);
        assert_ne!(QuestionType::Options, QuestionType::OpenEnded);
        assert_ne!(QuestionType::Confirmation, QuestionType::OpenEnded);
    }

    #[test]
    fn test_question_type_clone() {
        let original = QuestionType::Options;
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_question_type_debug() {
        let qt = QuestionType::Options;
        let debug_str = format!("{:?}", qt);
        assert!(debug_str.contains("Options"));
    }

    #[test]
    fn test_question_type_serialization() {
        // Test serialization to snake_case
        let options = QuestionType::Options;
        let json = serde_json::to_string(&options).unwrap();
        assert_eq!(json, "\"options\"");

        let confirmation = QuestionType::Confirmation;
        let json = serde_json::to_string(&confirmation).unwrap();
        assert_eq!(json, "\"confirmation\"");

        let open_ended = QuestionType::OpenEnded;
        let json = serde_json::to_string(&open_ended).unwrap();
        assert_eq!(json, "\"open_ended\"");
    }

    #[test]
    fn test_question_type_deserialization() {
        // Test deserialization from snake_case
        let options: QuestionType = serde_json::from_str("\"options\"").unwrap();
        assert_eq!(options, QuestionType::Options);

        let confirmation: QuestionType = serde_json::from_str("\"confirmation\"").unwrap();
        assert_eq!(confirmation, QuestionType::Confirmation);

        let open_ended: QuestionType = serde_json::from_str("\"open_ended\"").unwrap();
        assert_eq!(open_ended, QuestionType::OpenEnded);
    }

    #[test]
    fn test_question_type_roundtrip_serialization() {
        for qt in [
            QuestionType::Options,
            QuestionType::Confirmation,
            QuestionType::OpenEnded,
        ] {
            let json = serde_json::to_string(&qt).unwrap();
            let deserialized: QuestionType = serde_json::from_str(&json).unwrap();
            assert_eq!(qt, deserialized);
        }
    }
}

// ============================================================================
// AgentStatus Tests
// ============================================================================

mod agent_status_tests {
    use super::*;

    #[test]
    fn test_agent_status_variants_exist() {
        // Verify all three variants exist
        let _processing = AgentStatus::Processing;
        let _waiting = AgentStatus::WaitingForInput;
        let _unknown = AgentStatus::Unknown;
    }

    #[test]
    fn test_agent_status_equality() {
        assert_eq!(AgentStatus::Processing, AgentStatus::Processing);
        assert_eq!(AgentStatus::WaitingForInput, AgentStatus::WaitingForInput);
        assert_eq!(AgentStatus::Unknown, AgentStatus::Unknown);

        assert_ne!(AgentStatus::Processing, AgentStatus::WaitingForInput);
        assert_ne!(AgentStatus::Processing, AgentStatus::Unknown);
        assert_ne!(AgentStatus::WaitingForInput, AgentStatus::Unknown);
    }

    #[test]
    fn test_agent_status_clone() {
        let original = AgentStatus::Processing;
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_agent_status_debug() {
        let status = AgentStatus::Processing;
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("Processing"));

        let status = AgentStatus::WaitingForInput;
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("WaitingForInput"));

        let status = AgentStatus::Unknown;
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("Unknown"));
    }

    #[test]
    fn test_agent_status_is_processing() {
        // Test helper method to check if agent is processing
        assert!(AgentStatus::Processing.is_processing());
        assert!(!AgentStatus::WaitingForInput.is_processing());
        assert!(!AgentStatus::Unknown.is_processing());
    }

    #[test]
    fn test_agent_status_is_waiting() {
        // Test helper method to check if agent is waiting for input
        assert!(!AgentStatus::Processing.is_waiting());
        assert!(AgentStatus::WaitingForInput.is_waiting());
        assert!(!AgentStatus::Unknown.is_waiting());
    }
}

// ============================================================================
// NotificationContent Tests
// ============================================================================

mod notification_content_tests {
    use super::*;

    #[test]
    fn test_notification_content_default() {
        let content = NotificationContent::default();

        assert_eq!(content.question_type, QuestionType::OpenEnded);
        assert!(content.question.is_empty());
        assert!(content.options.is_empty());
        assert_eq!(content.summary, "等待输入");
        assert!(content.reply_hint.is_empty());
    }

    #[test]
    fn test_notification_content_confirmation_constructor() {
        let content = NotificationContent::confirmation("Delete this file?");

        assert_eq!(content.question_type, QuestionType::Confirmation);
        assert_eq!(content.question, "Delete this file?");
        assert!(content.options.is_empty());
        assert_eq!(content.summary, "请求确认");
        assert_eq!(content.reply_hint, "y/n");
    }

    #[test]
    fn test_notification_content_options_constructor() {
        let options = vec!["Option A".to_string(), "Option B".to_string()];
        let content = NotificationContent::options("Choose one:", options.clone());

        assert_eq!(content.question_type, QuestionType::Options);
        assert_eq!(content.question, "Choose one:");
        assert_eq!(content.options, options);
        assert_eq!(content.summary, "等待选择");
        assert_eq!(content.reply_hint, "1/2");
    }

    #[test]
    fn test_notification_content_options_many() {
        // Test reply_hint format for many options (> 5)
        let options: Vec<String> = (1..=7).map(|n| format!("Option {}", n)).collect();
        let content = NotificationContent::options("Choose one:", options.clone());

        assert_eq!(content.question_type, QuestionType::Options);
        assert_eq!(content.options.len(), 7);
        assert_eq!(content.reply_hint, "1-7");
    }

    #[test]
    fn test_notification_content_options_five() {
        // Test reply_hint format for exactly 5 options (boundary case)
        let options: Vec<String> = (1..=5).map(|n| format!("Option {}", n)).collect();
        let content = NotificationContent::options("Choose one:", options.clone());

        assert_eq!(content.reply_hint, "1/2/3/4/5");
    }

    #[test]
    fn test_notification_content_open_ended_constructor() {
        let content = NotificationContent::open_ended("What feature do you want?");

        assert_eq!(content.question_type, QuestionType::OpenEnded);
        assert_eq!(content.question, "What feature do you want?");
        assert!(content.options.is_empty());
        assert_eq!(content.summary, "等待回复");
        assert!(content.reply_hint.is_empty());
    }

    #[test]
    fn test_notification_content_clone() {
        let original = NotificationContent {
            question_type: QuestionType::Options,
            question: "Test question".to_string(),
            options: vec!["A".to_string(), "B".to_string()],
            summary: "Test summary".to_string(),
            reply_hint: "1/2".to_string(),
        };

        let cloned = original.clone();
        assert_eq!(original.question_type, cloned.question_type);
        assert_eq!(original.question, cloned.question);
        assert_eq!(original.options, cloned.options);
        assert_eq!(original.summary, cloned.summary);
        assert_eq!(original.reply_hint, cloned.reply_hint);
    }

    #[test]
    fn test_notification_content_debug() {
        let content = NotificationContent::default();
        let debug_str = format!("{:?}", content);

        assert!(debug_str.contains("NotificationContent"));
        assert!(debug_str.contains("question_type"));
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
    fn test_notification_content_deserialization() {
        let json = r#"{
            "question_type": "confirmation",
            "question": "Proceed?",
            "options": [],
            "summary": "确认",
            "reply_hint": "y/n"
        }"#;

        let content: NotificationContent = serde_json::from_str(json).unwrap();

        assert_eq!(content.question_type, QuestionType::Confirmation);
        assert_eq!(content.question, "Proceed?");
        assert!(content.options.is_empty());
        assert_eq!(content.summary, "确认");
        assert_eq!(content.reply_hint, "y/n");
    }

    #[test]
    fn test_notification_content_roundtrip_serialization() {
        let original = NotificationContent {
            question_type: QuestionType::Options,
            question: "Select an option:".to_string(),
            options: vec!["1. First".to_string(), "2. Second".to_string()],
            summary: "等待选择".to_string(),
            reply_hint: "1/2".to_string(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: NotificationContent = serde_json::from_str(&json).unwrap();

        assert_eq!(original.question_type, deserialized.question_type);
        assert_eq!(original.question, deserialized.question);
        assert_eq!(original.options, deserialized.options);
        assert_eq!(original.summary, deserialized.summary);
        assert_eq!(original.reply_hint, deserialized.reply_hint);
    }

    #[test]
    fn test_notification_content_with_empty_options() {
        let content = NotificationContent {
            question_type: QuestionType::OpenEnded,
            question: "What do you want?".to_string(),
            options: vec![],
            summary: "等待回复".to_string(),
            reply_hint: String::new(),
        };

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"options\":[]"));

        let deserialized: NotificationContent = serde_json::from_str(&json).unwrap();
        assert!(deserialized.options.is_empty());
    }

    #[test]
    fn test_notification_content_with_unicode() {
        let content = NotificationContent {
            question_type: QuestionType::OpenEnded,
            question: "你想要实现什么功能？".to_string(),
            options: vec!["选项一".to_string(), "选项二".to_string()],
            summary: "等待回复".to_string(),
            reply_hint: String::new(),
        };

        let json = serde_json::to_string(&content).unwrap();
        let deserialized: NotificationContent = serde_json::from_str(&json).unwrap();

        assert_eq!(content.question, deserialized.question);
        assert_eq!(content.options, deserialized.options);
        assert_eq!(content.summary, deserialized.summary);
        assert_eq!(content.reply_hint, deserialized.reply_hint);
    }
}

// ============================================================================
// Integration Tests - Type Interactions
// ============================================================================

mod integration_tests {
    use super::*;

    #[test]
    fn test_notification_content_with_all_question_types() {
        // Test that NotificationContent works with all QuestionType variants
        let types = [
            QuestionType::Options,
            QuestionType::Confirmation,
            QuestionType::OpenEnded,
        ];

        for qt in types {
            let content = NotificationContent {
                question_type: qt.clone(),
                question: "Test".to_string(),
                options: vec![],
                summary: "Test".to_string(),
                reply_hint: String::new(),
            };

            assert_eq!(content.question_type, qt);
        }
    }

    #[test]
    fn test_types_are_send_and_sync() {
        // Verify types can be safely shared across threads
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<QuestionType>();
        assert_sync::<QuestionType>();

        assert_send::<AgentStatus>();
        assert_sync::<AgentStatus>();

        assert_send::<NotificationContent>();
        assert_sync::<NotificationContent>();
    }
}
