//! Anthropic API 客户端 - 直接调用 Claude API
//!
//! 用于快速 AI 推理任务，绕过 OpenClaw 的 Opus 默认配置。
//! 主要用于终端问题提取等简单任务，使用 Haiku 模型以获得最低延迟。
//!
//! API Key 读取优先级：
//! 1. 环境变量 `ANTHROPIC_API_KEY`
//! 2. 文件 `~/.anthropic/api_key`
//! 3. OpenClaw 配置 `~/.openclaw/openclaw.json` 的 `models.providers.anthropic.apiKey` 或 `providers.anthropic.apiKey`

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::terminal_utils::{truncate_for_ai, truncate_for_status};

/// Anthropic API 基础 URL
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// API 版本
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 默认模型 - Haiku 4.5（最快最便宜）
pub const DEFAULT_MODEL: &str = "claude-3-5-haiku-20241022";

/// 默认超时（毫秒）
const DEFAULT_TIMEOUT_MS: u64 = 5000;

/// 默认最大 tokens
const DEFAULT_MAX_TOKENS: u32 = 500;

/// 内容提取超时（毫秒）- 10 秒（本地代理可能需要更长时间）
const EXTRACT_TIMEOUT_MS: u64 = 10000;

// ============================================================================
// 通知内容提取
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
}

impl Default for NotificationContent {
    fn default() -> Self {
        Self {
            question_type: QuestionType::OpenEnded,
            question: String::new(),
            options: Vec::new(),
            summary: "等待输入".to_string(),
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
        }
    }

    /// 创建默认的选项类型内容
    pub fn options(question: &str, options: Vec<String>) -> Self {
        Self {
            question_type: QuestionType::Options,
            question: question.to_string(),
            options,
            summary: "等待选择".to_string(),
        }
    }

    /// 创建默认的开放式问题内容
    pub fn open_ended(question: &str) -> Self {
        Self {
            question_type: QuestionType::OpenEnded,
            question: question.to_string(),
            options: Vec::new(),
            summary: "等待回复".to_string(),
        }
    }
}

/// Anthropic 客户端配置
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// API 密钥
    pub api_key: String,
    /// API 基础 URL（支持代理）
    pub base_url: String,
    /// 模型名称
    pub model: String,
    /// 请求超时（毫秒）
    pub timeout_ms: u64,
    /// 最大输出 tokens
    pub max_tokens: u32,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: ANTHROPIC_API_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }
}

impl AnthropicConfig {
    /// 从环境和配置文件自动加载 API key 和 base_url
    pub fn auto_load() -> Result<Self> {
        let (api_key, base_url) = Self::load_api_config()?;
        Ok(Self {
            api_key,
            base_url,
            ..Default::default()
        })
    }

    /// 加载 API 配置（key 和 base_url），按优先级尝试多个来源
    fn load_api_config() -> Result<(String, String)> {
        let default_url = ANTHROPIC_API_URL.to_string();

        // 1. 环境变量
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                debug!("Using ANTHROPIC_API_KEY from environment");
                let base_url = std::env::var("ANTHROPIC_BASE_URL")
                    .ok()
                    .filter(|u| !u.is_empty())
                    .unwrap_or_else(|| default_url.clone());
                return Ok((key, base_url));
            }
        }

        // 2. ~/.anthropic/api_key 文件
        if let Some(home) = dirs::home_dir() {
            let key_file = home.join(".anthropic/api_key");
            if key_file.exists() {
                if let Ok(key) = fs::read_to_string(&key_file) {
                    let key = key.trim().to_string();
                    if !key.is_empty() {
                        debug!("Using API key from ~/.anthropic/api_key");
                        return Ok((key, default_url.clone()));
                    }
                }
            }
        }

        // 3. OpenClaw 配置
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".openclaw/openclaw.json");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                        // 尝试从 .models.providers.anthropic 获取
                        let anthropic_config = config
                            .get("models")
                            .and_then(|m| m.get("providers"))
                            .and_then(|p| p.get("anthropic"))
                            .or_else(|| {
                                config
                                    .get("providers")
                                    .and_then(|p| p.get("anthropic"))
                            });

                        if let Some(ac) = anthropic_config {
                            let key = ac.get("apiKey").and_then(|k| k.as_str());
                            let base_url = ac.get("baseUrl").and_then(|u| u.as_str());

                            if let Some(key) = key {
                                if !key.is_empty() {
                                    // 使用配置的 baseUrl，如果没有则使用默认 URL
                                    let url = base_url
                                        .filter(|u| !u.is_empty())
                                        .map(|u| {
                                            // 确保 URL 以 /v1/messages 结尾
                                            let u = u.trim_end_matches('/');
                                            if u.ends_with("/v1/messages") {
                                                u.to_string()
                                            } else if u.ends_with("/v1") {
                                                format!("{}/messages", u)
                                            } else {
                                                format!("{}/v1/messages", u)
                                            }
                                        })
                                        .unwrap_or_else(|| default_url.clone());
                                    debug!("Using API key from OpenClaw config, base_url: {}", url);
                                    return Ok((key.to_string(), url));
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow!(
            "No Anthropic API key found. Set ANTHROPIC_API_KEY env var, \
             create ~/.anthropic/api_key, or configure in ~/.openclaw/openclaw.json"
        ))
    }
}

/// Messages API 请求体
#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<Message>,
}

/// 消息
#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

/// Messages API 响应体
#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

/// 内容块
#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

/// API 错误响应
#[derive(Deserialize)]
struct ErrorResponse {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

/// Anthropic API 客户端
pub struct AnthropicClient {
    client: reqwest::blocking::Client,
    config: AnthropicConfig,
}

impl AnthropicClient {
    /// 创建新客户端
    pub fn new(config: AnthropicConfig) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| anyhow!("Cannot create HTTP client: {}", e))?;

        Ok(Self { client, config })
    }

    /// 从自动加载的配置创建客户端
    pub fn from_config() -> Result<Self> {
        let config = AnthropicConfig::auto_load()?;
        Self::new(config)
    }

    /// 发送消息并获取响应
    pub fn complete(&self, prompt: &str, system: Option<&str>) -> Result<String> {
        let request = MessagesRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            system: system.map(|s| s.to_string()),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        debug!(
            model = %self.config.model,
            prompt_len = prompt.len(),
            base_url = %self.config.base_url,
            timeout_ms = self.config.timeout_ms,
            "Sending request to Anthropic API"
        );

        let start = std::time::Instant::now();
        let response = self
            .client
            .post(&self.config.base_url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| {
                let elapsed = start.elapsed();
                anyhow!("API request failed after {}ms: {}", elapsed.as_millis(), e)
            })?;

        debug!(elapsed_ms = start.elapsed().as_millis(), "API request completed");

        let status = response.status();
        let body = response
            .text()
            .map_err(|e| anyhow!("Failed to read response: {}", e))?;

        if !status.is_success() {
            // 尝试解析错误响应
            if let Ok(error_resp) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(anyhow!("API error ({}): {}", status, error_resp.error.message));
            }
            return Err(anyhow!("API error ({}): {}", status, body));
        }

        let response: MessagesResponse = serde_json::from_str(&body)
            .map_err(|e| anyhow!("Failed to parse response: {} - body: {}", e, body))?;

        // 提取文本内容
        let text = response
            .content
            .iter()
            .filter(|c| c.content_type == "text")
            .filter_map(|c| c.text.as_ref())
            .cloned()
            .collect::<Vec<String>>()
            .join("");

        if text.is_empty() {
            warn!("Empty response from Anthropic API");
        }

        Ok(text)
    }
}

// ============================================================================
// 通知内容提取函数
// ============================================================================

/// 从终端快照中智能提取通知内容（使用 Haiku 4.5）
///
/// 使用 AI 分析终端输出，提取：
/// - 问题类型（选项/确认/开放式）
/// - 完整问题文本
/// - 选项列表
/// - 简洁摘要
///
/// # 参数
/// - `terminal_snapshot`: 终端快照内容
///
/// # 返回
/// - `Ok(NotificationContent)`: 提取的内容
/// - `Err`: API 调用失败时返回错误
///
/// # 超时
/// 使用 3 秒超时，超时后返回默认值
pub fn extract_notification_content(terminal_snapshot: &str) -> Result<NotificationContent> {
    let start = std::time::Instant::now();

    // 创建带 3 秒超时的客户端
    let config = AnthropicConfig {
        timeout_ms: EXTRACT_TIMEOUT_MS,
        ..AnthropicConfig::auto_load()?
    };
    let client = AnthropicClient::new(config)?;

    // 截取最后 N 行，避免 token 过多
    let truncated = truncate_for_ai(terminal_snapshot);

    let system = "你是一个终端输出分析专家。从 AI Agent 终端快照中提取正在询问用户的问题信息。只返回 JSON，不要其他内容。";

    let prompt = format!(
        r#"从以下终端输出中提取问题信息：

<terminal>
{truncated}
</terminal>

返回 JSON 格式：
{{
  "question_type": "options" | "confirmation" | "open_ended",
  "question": "完整的问题文本",
  "options": ["选项1", "选项2", ...],
  "summary": "简洁的摘要（10字以内）"
}}

规则：
- 重要：只分析终端输出中最后出现的问题或提示，忽略之前的历史会话内容。
- question_type:
  - "options": 有多个选项供选择（如 1. xxx 2. xxx）
  - "confirmation": 是/否确认（如 [Y/n]、确认？）
  - "open_ended": 开放式问题（需要输入内容）
- question: 提取完整的问题文本，包括上下文
- options: 仅 options 类型需要填写，其他类型为空数组
- summary: 简洁描述，如"等待选择"、"请求确认"、"等待回复"

只返回 JSON，不要其他内容。"#
    );

    let response = client.complete(&prompt, Some(system))?;
    let elapsed = start.elapsed();
    info!(elapsed_ms = elapsed.as_millis(), "Haiku extract_notification_content completed");

    // 解析 JSON 响应
    let json_str = extract_json_from_output(&response)
        .ok_or_else(|| anyhow!("No JSON found in response: {}", response))?;

    let parsed: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| anyhow!("Failed to parse JSON: {} - content: {}", e, json_str))?;

    // 提取字段
    let question_type_str = parsed
        .get("question_type")
        .and_then(|v| v.as_str())
        .unwrap_or("open_ended");

    let question_type = match question_type_str {
        "options" => QuestionType::Options,
        "confirmation" => QuestionType::Confirmation,
        _ => QuestionType::OpenEnded,
    };

    let question = parsed
        .get("question")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let options: Vec<String> = parsed
        .get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let summary = parsed
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("等待输入")
        .to_string();

    Ok(NotificationContent {
        question_type,
        question,
        options,
        summary,
    })
}

/// 从终端快照中提取通知内容，失败时返回默认值
///
/// 这是 `extract_notification_content` 的便捷包装，
/// 在 API 调用失败或超时时返回默认的 `NotificationContent`。
pub fn extract_notification_content_or_default(terminal_snapshot: &str) -> NotificationContent {
    match extract_notification_content(terminal_snapshot) {
        Ok(content) => content,
        Err(e) => {
            warn!(error = %e, "Failed to extract notification content, using default");
            NotificationContent::default()
        }
    }
}

/// 从终端快照中提取问题（使用 Haiku）
///
/// 这是一个便捷函数，用于替代 `extract_question_with_ai`。
/// 使用 Haiku 模型，延迟约 1-2 秒，比 Opus 快 10 倍。
pub fn extract_question_with_haiku(terminal_snapshot: &str) -> Option<(String, String, String)> {
    let client = match AnthropicClient::from_config() {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to create Anthropic client");
            return None;
        }
    };

    // 截取最后 N 行
    let truncated = truncate_for_ai(terminal_snapshot);

    let system = "你是一个终端输出分析专家。从给定的终端快照中提取用户正在被询问的问题。";

    let prompt = format!(
        r#"分析以下 AI Agent 终端输出，提取正在询问用户的问题。

终端输出:
{}

请用 JSON 格式回复，包含以下字段：
- question_type: "open"（开放问题）、"choice"（选择题）、"confirm"（确认）、"none"（无问题）
- question: 核心问题内容（简洁，不超过 100 字）
- reply_hint: 回复提示（如"回复 y/n"、"回复数字选择"、"回复内容"）

重要：只分析终端输出中最后出现的问题或提示，忽略之前的历史会话内容。

只返回 JSON，不要其他内容。如果没有问题，question_type 设为 "none"。"#,
        truncated
    );

    let start = std::time::Instant::now();
    let response = match client.complete(&prompt, Some(system)) {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Haiku API call failed");
            return None;
        }
    };
    debug!(elapsed_ms = start.elapsed().as_millis(), "Haiku API call completed");

    // 解析 JSON 响应
    let json_str = extract_json_from_output(&response)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str).ok()?;

    let question_type = parsed.get("question_type")?.as_str()?;
    if question_type == "none" {
        return None;
    }

    let question = parsed.get("question")?.as_str()?.to_string();
    let reply_hint = parsed.get("reply_hint")?.as_str()?.to_string();

    Some((question_type.to_string(), question, reply_hint))
}

/// Agent 状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    /// Agent 正在处理中（不应发送通知）
    Processing,
    /// Agent 空闲，等待用户输入（应发送通知）
    WaitingForInput,
    /// 无法确定状态
    Unknown,
}

/// 使用 Haiku 判断 Agent 是否正在处理中
///
/// 这个函数使用 AI 分析终端输出，判断 agent 当前状态：
/// - Processing: agent 正在执行任务（如 Thinking、Brewing、Running 等）
/// - WaitingForInput: agent 空闲，等待用户输入
///
/// 这种方式比硬编码模式更灵活，可以兼容不同的 AI 编码工具：
/// - Claude Code: Hatching…, Brewing…, Thinking…
/// - Codex: Running…, Executing…
/// - OpenCode: Processing…, Working…
///
/// # 参数
/// - `terminal_snapshot`: 终端快照内容（最后 10-15 行即可）
///
/// # 返回
/// - `AgentStatus::Processing`: agent 正在处理，不应发送通知
/// - `AgentStatus::WaitingForInput`: agent 空闲，应发送通知
/// - `AgentStatus::Unknown`: 无法确定（API 失败时）
///
/// # 超时
/// 使用 3 秒超时，超时后返回 Unknown
pub fn is_agent_processing(terminal_snapshot: &str) -> AgentStatus {
    let start = std::time::Instant::now();

    // 创建带 3 秒超时的客户端
    let config = match AnthropicConfig::auto_load() {
        Ok(c) => AnthropicConfig {
            timeout_ms: 3000,
            max_tokens: 50, // 只需要简短回答
            ..c
        },
        Err(e) => {
            warn!(error = %e, "Failed to load Anthropic config for is_agent_processing");
            return AgentStatus::Unknown;
        }
    };

    let client = match AnthropicClient::new(config) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to create Anthropic client for is_agent_processing");
            return AgentStatus::Unknown;
        }
    };

    // 只取最后 N 行，减少 token 消耗
    let last_lines = truncate_for_status(terminal_snapshot);

    let system = "你是一个终端状态分析专家。判断 AI 编码助手（如 Claude Code、Codex、OpenCode）的当前状态。只回答 PROCESSING 或 WAITING，不要其他内容。";

    let prompt = format!(
        r#"分析以下终端输出，判断 AI 编码助手的状态：

<terminal>
{last_lines}
</terminal>

判断规则：
- 只判断终端最后的状态，忽略历史输出
- PROCESSING: 如果看到处理中的指示器（如 Thinking…、Brewing…、Hatching…、Running…、Executing…、Loading…、Working… 等带省略号的状态，或旋转动画字符 ✢✻✶✽◐）
- WAITING: 如果看到空闲提示符（如 >、❯、$）或问题/选项等待用户输入

只回答一个词：PROCESSING 或 WAITING"#
    );

    let response = match client.complete(&prompt, Some(system)) {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Haiku API call failed for is_agent_processing");
            return AgentStatus::Unknown;
        }
    };

    let elapsed = start.elapsed();
    debug!(elapsed_ms = elapsed.as_millis(), response = %response.trim(), "is_agent_processing completed");

    let response_upper = response.trim().to_uppercase();
    if response_upper.contains("PROCESSING") {
        AgentStatus::Processing
    } else if response_upper.contains("WAITING") {
        AgentStatus::WaitingForInput
    } else {
        warn!(response = %response, "Unexpected response from is_agent_processing");
        AgentStatus::Unknown
    }
}

/// 从输出中提取 JSON 字符串
fn extract_json_from_output(output: &str) -> Option<String> {
    let start = output.find('{')?;
    let end = output.rfind('}')?;
    if end > start {
        Some(output[start..=end].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_output() {
        let output = r#"Here is the JSON:
{"question_type": "confirm", "question": "Continue?", "reply_hint": "y/n"}
That's all."#;

        let json = extract_json_from_output(output).unwrap();
        assert!(json.contains("question_type"));
        assert!(json.contains("confirm"));
    }

    #[test]
    fn test_extract_json_no_json() {
        let output = "No JSON here";
        assert!(extract_json_from_output(output).is_none());
    }

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
        };

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"question_type\":\"options\""));
        assert!(json.contains("\"question\":\"Choose:\""));
        assert!(json.contains("\"options\":[\"A\",\"B\"]"));
        assert!(json.contains("\"summary\":\"选择\""));
    }
}
