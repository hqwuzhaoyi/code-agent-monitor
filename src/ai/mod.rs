//! AI 集成 - Anthropic API 客户端和内容提取

pub mod client;
pub mod extractor;
pub mod quality;
pub mod types;

pub use client::{AnthropicClient, AnthropicConfig};
pub use extractor::{
    detect_waiting_question, extract_formatted_message, extract_notification_content,
    extract_notification_content_or_default, extract_question_with_haiku, is_agent_processing,
    ExtractedQuestion, ExtractionResult, SimpleExtractionResult, TaskSummary,
};
pub use quality::{assess_question_extraction, assess_status_detection, thresholds};
pub use types::{NotificationContent, QuestionType};
