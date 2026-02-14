//! AI 集成 - Anthropic API 客户端和内容提取

pub mod client;
pub mod extractor;
pub mod types;
pub mod quality;

pub use client::{AnthropicClient, AnthropicConfig};
pub use extractor::{
    extract_question_with_haiku, extract_notification_content, is_agent_processing,
    detect_waiting_question, ExtractedQuestion, ExtractionResult, TaskSummary,
    extract_notification_content_or_default, extract_formatted_message, SimpleExtractionResult,
};
pub use types::{QuestionType, NotificationContent};
pub use quality::{assess_question_extraction, assess_status_detection, thresholds};
