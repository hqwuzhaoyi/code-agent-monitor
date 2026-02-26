//! AI 类型定义模块
//!
//! 包含 AI 相关的共享类型，从 anthropic.rs 提取以解决循环依赖问题。
//! 这些类型被 anthropic.rs 和 ai_quality.rs 共同使用。

use serde::{Deserialize, Serialize};

// ============================================================================
// QuestionType - 问题类型
// ============================================================================

/// 问题类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    /// 多选项问题
    Options,
    /// 是/否确认
    Confirmation,
    /// 开放式问题
    OpenEnded,
}

impl Default for QuestionType {
    fn default() -> Self {
        Self::OpenEnded
    }
}

// ============================================================================
// NotificationContent - 通知内容
// ============================================================================

/// 从终端快照提取的通知内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationContent {
    /// 问题类型
    pub question_type: QuestionType,
    /// 完整问题文本
    pub question: String,
    /// 选项列表（仅 Options 类型有值）
    pub options: Vec<String>,
    /// 简洁摘要（10 字以内）
    pub summary: String,
    /// 回复提示（如 "y/n"、"1/2/3"）
    pub reply_hint: String,
}

impl Default for NotificationContent {
    fn default() -> Self {
        Self {
            question_type: QuestionType::OpenEnded,
            question: String::new(),
            options: Vec::new(),
            summary: "等待输入".to_string(),
            reply_hint: String::new(),
        }
    }
}

impl NotificationContent {
    /// 创建默认的确认类型内容
    pub fn confirmation(question: &str) -> Self {
        Self {
            question_type: QuestionType::Confirmation,
            question: question.to_string(),
            options: Vec::new(),
            summary: "请求确认".to_string(),
            reply_hint: "y/n".to_string(),
        }
    }

    /// 创建默认的选项类型内容
    pub fn options(question: &str, options: Vec<String>) -> Self {
        let reply_hint = if options.len() <= 5 {
            // For small number of options, show all: "1/2/3"
            (1..=options.len())
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("/")
        } else {
            // For many options, show range: "1-N"
            format!("1-{}", options.len())
        };
        Self {
            question_type: QuestionType::Options,
            question: question.to_string(),
            options,
            summary: "等待选择".to_string(),
            reply_hint,
        }
    }

    /// 创建默认的开放式问题内容
    pub fn open_ended(question: &str) -> Self {
        Self {
            question_type: QuestionType::OpenEnded,
            question: question.to_string(),
            options: Vec::new(),
            summary: "等待回复".to_string(),
            reply_hint: String::new(),
        }
    }
}
