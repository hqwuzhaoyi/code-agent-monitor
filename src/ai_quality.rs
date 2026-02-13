//! AI 响应质量评估模块
//!
//! 提供对 AI 响应的质量评估功能，包括：
//! - JSON 格式验证
//! - 字段完整性检查
//! - 内容合理性评估
//! - 置信度计算

use crate::ai_types::{AgentStatus, NotificationContent, QuestionType};
use tracing::warn;

/// 质量评估结果
#[derive(Debug, Clone)]
pub struct QualityAssessment {
    /// 响应是否有效
    pub is_valid: bool,
    /// 置信度 (0.0 - 1.0)
    pub confidence: f32,
    /// 发现的问题列表
    pub issues: Vec<String>,
}

impl QualityAssessment {
    /// 创建有效的评估结果
    pub fn valid(confidence: f32) -> Self {
        Self {
            is_valid: true,
            confidence: confidence.clamp(0.0, 1.0),
            issues: Vec::new(),
        }
    }

    /// 创建无效的评估结果
    pub fn invalid(issues: Vec<String>) -> Self {
        Self {
            is_valid: false,
            confidence: 0.0,
            issues,
        }
    }

    /// 添加问题并降低置信度
    pub fn with_issue(mut self, issue: &str, confidence_penalty: f32) -> Self {
        self.issues.push(issue.to_string());
        self.confidence = (self.confidence - confidence_penalty).max(0.0);
        if self.confidence < 0.3 {
            self.is_valid = false;
        }
        self
    }
}

/// 评估 JSON 响应的质量
///
/// # 参数
/// - `response`: AI 返回的原始响应
/// - `expected_fields`: 期望存在的字段列表
///
/// # 返回
/// 质量评估结果
pub fn assess_json_response(response: &str, expected_fields: &[&str]) -> QualityAssessment {
    // 尝试解析 JSON
    let json: serde_json::Value = match serde_json::from_str(response) {
        Ok(v) => v,
        Err(e) => {
            return QualityAssessment::invalid(vec![format!("JSON 解析失败: {}", e)]);
        }
    };

    let mut assessment = QualityAssessment::valid(1.0);

    // 检查必需字段
    for field in expected_fields {
        if json.get(*field).is_none() {
            assessment = assessment.with_issue(&format!("缺少字段: {}", field), 0.2);
        }
    }

    // 检查字段值是否为空
    if let Some(obj) = json.as_object() {
        for (key, value) in obj {
            if let Some(s) = value.as_str() {
                if s.trim().is_empty() && expected_fields.contains(&key.as_str()) {
                    assessment = assessment.with_issue(&format!("字段 {} 为空", key), 0.15);
                }
            }
        }
    }

    assessment
}

/// 评估问题提取结果的质量
///
/// # 参数
/// - `content`: 提取的通知内容
///
/// # 返回
/// 质量评估结果
pub fn assess_question_extraction(content: &NotificationContent) -> QualityAssessment {
    let mut assessment = QualityAssessment::valid(1.0);

    // 检查问题是否为空
    if content.question.trim().is_empty() {
        assessment = assessment.with_issue("问题内容为空", 0.4);
    }

    // 检查问题长度是否合理
    if content.question.len() > 500 {
        assessment = assessment.with_issue("问题内容过长", 0.1);
    }

    // 检查摘要是否为空
    if content.summary.trim().is_empty() {
        assessment = assessment.with_issue("摘要为空", 0.2);
    }

    // 检查选项类型的一致性
    match content.question_type {
        QuestionType::Options => {
            if content.options.is_empty() {
                assessment = assessment.with_issue("选项类型但没有选项列表", 0.3);
            }
        }
        QuestionType::Confirmation => {
            if !content.options.is_empty() {
                assessment = assessment.with_issue("确认类型不应有选项列表", 0.1);
            }
        }
        QuestionType::OpenEnded => {
            // 开放式问题可以有或没有选项
        }
    }

    // 检查问题内容是否包含常见的问题标志
    let question_indicators = ["?", "？", "请", "是否", "选择", "确认", "输入"];
    let has_indicator = question_indicators
        .iter()
        .any(|i| content.question.contains(i));
    if !has_indicator {
        assessment = assessment.with_issue("问题内容缺少问题标志词", 0.15);
    }

    if !assessment.is_valid {
        warn!(
            confidence = assessment.confidence,
            issues = ?assessment.issues,
            "Question extraction quality is low"
        );
    }

    assessment
}

/// 评估状态检测结果的质量
///
/// # 参数
/// - `status`: 检测到的状态
/// - `snapshot`: 终端快照
///
/// # 返回
/// 质量评估结果
pub fn assess_status_detection(status: &AgentStatus, snapshot: &str) -> QualityAssessment {
    let mut assessment = QualityAssessment::valid(0.9);

    // 检查快照是否为空
    if snapshot.trim().is_empty() {
        return QualityAssessment::invalid(vec!["终端快照为空".to_string()]);
    }

    // 检查快照长度
    if snapshot.len() < 10 {
        assessment = assessment.with_issue("终端快照过短", 0.2);
    }

    // 根据状态检查快照内容的一致性
    match status {
        AgentStatus::Processing => {
            // 处理中状态应该有处理指示器
            let processing_hints = ["…", "...", "Thinking", "Brewing", "Running", "Loading", "Streaming", "Executing"];
            let has_processing_hint = processing_hints.iter().any(|h| snapshot.contains(h));

            // 检查是否有等待输入的指示器（与 Processing 状态矛盾）
            let waiting_hints = [">", "❯", "$"];
            let has_waiting_hint = waiting_hints.iter().any(|h| snapshot.contains(h));

            if !has_processing_hint {
                assessment = assessment.with_issue("处理中状态但快照无处理指示器", 0.3);
            }

            // 如果有等待提示符但没有处理指示器，大幅降低置信度
            if has_waiting_hint && !has_processing_hint {
                assessment = assessment.with_issue("快照有等待提示符但 AI 判断为处理中", 0.4);
            }
        }
        AgentStatus::WaitingForInput => {
            // 等待输入状态应该有提示符或问题
            let waiting_hints = [">", "❯", "?", "？", "[Y/n]", "[y/N]"];
            let has_hint = waiting_hints.iter().any(|h| snapshot.contains(h));
            if !has_hint {
                assessment = assessment.with_issue("等待输入状态但快照无等待指示器", 0.2);
            }
        }
        AgentStatus::Unknown => {
            // Unknown 状态本身就表示不确定
            assessment.confidence = 0.5;
        }
    }

    assessment
}

/// 置信度阈值常量
pub mod thresholds {
    /// 高置信度阈值 - 结果非常可靠
    pub const HIGH: f32 = 0.8;
    /// 中等置信度阈值 - 结果可以使用但需注意
    pub const MEDIUM: f32 = 0.6;
    /// 低置信度阈值 - 结果不可靠，应使用默认值
    pub const LOW: f32 = 0.4;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assess_valid_json() {
        let json = r#"{"question_type": "open_ended", "question": "What?", "summary": "test"}"#;
        let result = assess_json_response(json, &["question_type", "question"]);
        assert!(result.is_valid);
        assert!(result.confidence > 0.8);
    }

    #[test]
    fn test_assess_invalid_json() {
        let json = "not json";
        let result = assess_json_response(json, &["question_type"]);
        assert!(!result.is_valid);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_assess_missing_fields() {
        let json = r#"{"question_type": "open_ended"}"#;
        let result = assess_json_response(json, &["question_type", "question", "summary"]);
        assert!(result.confidence < 1.0);
        assert!(!result.issues.is_empty());
    }

    #[test]
    fn test_assess_question_extraction_valid() {
        let content = NotificationContent {
            question_type: QuestionType::OpenEnded,
            question: "你想要实现什么功能？".to_string(),
            options: vec![],
            summary: "等待回复".to_string(),
        };
        let result = assess_question_extraction(&content);
        assert!(result.is_valid);
        assert!(result.confidence > 0.7);
    }

    #[test]
    fn test_assess_question_extraction_empty() {
        let content = NotificationContent {
            question_type: QuestionType::OpenEnded,
            question: "".to_string(),
            options: vec![],
            summary: "".to_string(),
        };
        let result = assess_question_extraction(&content);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_assess_options_without_list() {
        let content = NotificationContent {
            question_type: QuestionType::Options,
            question: "请选择一个选项".to_string(),
            options: vec![], // 选项类型但没有选项
            summary: "等待选择".to_string(),
        };
        let result = assess_question_extraction(&content);
        assert!(result.confidence < 0.8);
    }

    #[test]
    fn test_confidence_thresholds() {
        assert!(thresholds::HIGH > thresholds::MEDIUM);
        assert!(thresholds::MEDIUM > thresholds::LOW);
    }
}
