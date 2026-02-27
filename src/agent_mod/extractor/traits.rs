//! ReAct 消息提取器 trait 定义
//!
//! 定义消息提取的核心接口和数据类型。

use serde::{Deserialize, Serialize};

/// 提取的消息内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMessage {
    /// 格式化的通知消息（直接发送给用户）
    pub content: String,
    /// 语义指纹（用于去重，如 "react-todo-enhance-or-fresh"）
    pub fingerprint: String,
    /// 上下文是否完整
    pub context_complete: bool,
    /// 消息类型
    pub message_type: MessageType,
    /// 是否是决策类问题（方案选择、架构设计等）
    #[serde(default)]
    pub is_decision: bool,
}

/// 消息类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// 选择题（有选项）
    Choice,
    /// 确认题（y/n）
    Confirmation,
    /// 开放式问题
    OpenEnded,
    /// Agent 空闲（无问题）
    Idle {
        status: String,
        last_action: Option<String>,
    },
}

/// 提取结果
#[derive(Debug, Clone)]
pub enum ExtractionResult {
    /// 成功提取到消息
    Success(ExtractedMessage),
    /// 需要更多上下文
    NeedMoreContext,
    /// Agent 正在处理中（不应发送通知）
    Processing,
    /// 提取失败
    Failed(String),
    /// 检测到终端错误
    Error(String),
}

/// 迭代策略配置
#[derive(Debug, Clone)]
pub struct IterationConfig {
    /// 上下文行数序列
    pub context_sizes: Vec<usize>,
    /// 最大迭代次数
    pub max_iterations: usize,
    /// 单次 AI 调用超时（毫秒）
    pub timeout_ms: u64,
}

impl Default for IterationConfig {
    fn default() -> Self {
        Self {
            context_sizes: vec![80, 150, 300, 500, 800],
            max_iterations: 5,
            timeout_ms: 10000,
        }
    }
}

/// 消息提取器 trait
pub trait MessageExtractor: Send + Sync {
    /// 从终端快照提取消息
    ///
    /// # 参数
    /// - `terminal_snapshot`: 完整的终端快照
    /// - `lines`: 要分析的行数（从末尾截取）
    ///
    /// # 返回
    /// - `ExtractionResult`: 提取结果
    fn extract(&self, terminal_snapshot: &str, lines: usize) -> ExtractionResult;

    /// 判断 Agent 是否正在处理中
    fn is_processing(&self, terminal_snapshot: &str) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iteration_config_default() {
        let config = IterationConfig::default();
        assert_eq!(config.context_sizes, vec![80, 150, 300, 500, 800]);
        assert_eq!(config.max_iterations, 5);
        assert_eq!(config.timeout_ms, 10000);
    }

    #[test]
    fn test_message_type_serialization() {
        let choice = MessageType::Choice;
        let json = serde_json::to_string(&choice).unwrap();
        assert_eq!(json, "\"choice\"");

        let idle = MessageType::Idle {
            status: "completed".to_string(),
            last_action: Some("Created file".to_string()),
        };
        let json = serde_json::to_string(&idle).unwrap();
        assert!(json.contains("idle"));
        assert!(json.contains("completed"));
    }

    #[test]
    fn test_extracted_message_clone() {
        let msg = ExtractedMessage {
            content: "Test question".to_string(),
            fingerprint: "test-question".to_string(),
            context_complete: true,
            message_type: MessageType::OpenEnded,
            is_decision: false,
        };
        let cloned = msg.clone();
        assert_eq!(cloned.content, msg.content);
        assert_eq!(cloned.fingerprint, msg.fingerprint);
    }
}
