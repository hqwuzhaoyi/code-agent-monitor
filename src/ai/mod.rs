//! AI subsystem - Anthropic API client and content extraction

pub mod client;
mod extractor;

pub use client::{AnthropicClient, AnthropicConfig};
pub use extractor::{
    detect_waiting_question, extract_notification_content, extract_notification_content_or_default,
    extract_question_with_haiku, is_agent_processing, ExtractionResult, ExtractedQuestion,
    TaskSummary,
};
