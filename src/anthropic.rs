//! Anthropic API 客户端 - 直接调用 Claude API
//!
//! 用于快速 AI 推理任务，绕过 OpenClaw 的 Opus 默认配置。
//! 主要用于终端问题提取等简单任务，使用 Haiku 模型以获得最低延迟。
//!
//! API Key 读取优先级：
//! 1. 环境变量 `ANTHROPIC_API_KEY`
//! 2. 文件 `~/.anthropic/api_key`
//! 3. OpenClaw 配置 `~/.openclaw/openclaw.json` 的 `providers.anthropic.apiKey`

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;
use tracing::{debug, warn};

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

/// Anthropic 客户端配置
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// API 密钥
    pub api_key: String,
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
            model: DEFAULT_MODEL.to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }
}

impl AnthropicConfig {
    /// 从环境和配置文件自动加载 API key
    pub fn auto_load() -> Result<Self> {
        let api_key = Self::load_api_key()?;
        Ok(Self {
            api_key,
            ..Default::default()
        })
    }

    /// 加载 API key，按优先级尝试多个来源
    fn load_api_key() -> Result<String> {
        // 1. 环境变量
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                debug!("Using ANTHROPIC_API_KEY from environment");
                return Ok(key);
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
                        return Ok(key);
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
                        if let Some(key) = config
                            .get("providers")
                            .and_then(|p| p.get("anthropic"))
                            .and_then(|a| a.get("apiKey"))
                            .and_then(|k| k.as_str())
                        {
                            if !key.is_empty() {
                                debug!("Using API key from OpenClaw config");
                                return Ok(key.to_string());
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
            "Sending request to Anthropic API"
        );

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| anyhow!("API request failed: {}", e))?;

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

    // 截取最后 30 行
    let lines: Vec<&str> = terminal_snapshot.lines().collect();
    let truncated = if lines.len() > 30 {
        lines[lines.len() - 30..].join("\n")
    } else {
        terminal_snapshot.to_string()
    };

    let system = "你是一个终端输出分析专家。从给定的终端快照中提取用户正在被询问的问题。";

    let prompt = format!(
        r#"分析以下 AI Agent 终端输出，提取正在询问用户的问题。

终端输出:
{}

请用 JSON 格式回复，包含以下字段：
- question_type: "open"（开放问题）、"choice"（选择题）、"confirm"（确认）、"none"（无问题）
- question: 核心问题内容（简洁，不超过 100 字）
- reply_hint: 回复提示（如"回复 y/n"、"回复数字选择"、"回复内容"）

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
}
