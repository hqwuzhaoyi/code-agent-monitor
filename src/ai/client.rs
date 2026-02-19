//! Anthropic API 客户端
//!
//! 用于快速 AI 推理任务，绕过 OpenClaw 的 Opus 默认配置。
//! 主要用于终端问题提取等简单任务，使用 Haiku 模型以获得最低延迟。
//!
//! API Key 读取优先级：
//! 1. CAM 配置文件 `~/.config/code-agent-monitor/config.json`（JSON 格式，字段 `anthropic_api_key` 和可选 `anthropic_base_url`）
//! 2. 环境变量 `ANTHROPIC_API_KEY`
//! 3. 文件 `~/.anthropic/api_key`
//! 4. OpenClaw 配置 `~/.openclaw/openclaw.json` 的 `models.providers.anthropic.apiKey` 或 `providers.anthropic.apiKey`

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;
use tracing::{debug, warn};

/// Anthropic API 基础 URL
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// API 版本
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 默认模型 - Haiku 4.5（最快最便宜）
pub const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";

/// 默认超时（毫秒）
pub const DEFAULT_TIMEOUT_MS: u64 = 5000;

/// 默认最大 tokens
pub const DEFAULT_MAX_TOKENS: u32 = 1500;

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
    /// Webhook 配置
    pub webhook: Option<WebhookConfig>,
}

/// Webhook 配置
#[derive(Debug, Clone)]
pub struct WebhookConfig {
    /// Gateway URL
    pub gateway_url: String,
    /// Hook token
    pub hook_token: String,
    /// 超时秒数
    pub timeout_secs: u64,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: ANTHROPIC_API_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_tokens: DEFAULT_MAX_TOKENS,
            webhook: None,
        }
    }
}

impl AnthropicConfig {
    /// 从环境和配置文件自动加载配置
    pub fn auto_load() -> Result<Self> {
        let (api_key, base_url) = Self::load_api_config()?;
        let webhook = Self::load_webhook_config();
        
        Ok(Self {
            api_key,
            base_url,
            model: DEFAULT_MODEL.to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_tokens: DEFAULT_MAX_TOKENS,
            webhook,
        })
    }

    /// 加载 API 配置（key 和 base_url），按优先级尝试多个来源
    fn load_api_config() -> Result<(String, String)> {
        let default_url = ANTHROPIC_API_URL.to_string();

        // 1. CAM 配置文件 ~/.config/code-agent-monitor/config.json
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".config/code-agent-monitor/config.json");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                        let key = config.get("anthropic_api_key").and_then(|k| k.as_str());
                        let base_url = config.get("anthropic_base_url").and_then(|u| u.as_str());

                        if let Some(key) = key {
                            if !key.is_empty() {
                                let url = base_url
                                    .filter(|u| !u.is_empty())
                                    .map(|u| {
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
                                debug!("Using API key from ~/.config/code-agent-monitor/config.json, base_url: {}", url);
                                return Ok((key.to_string(), url));
                            }
                        }
                    }
                }
            }
        }

        // 2. 环境变量
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

        // 3. ~/.anthropic/api_key 文件
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

        // 4. OpenClaw 配置
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
            "No Anthropic API key found. Create ~/.config/code-agent-monitor/config.json with anthropic_api_key, \
             set ANTHROPIC_API_KEY env var, create ~/.anthropic/api_key, \
             or configure in ~/.openclaw/openclaw.json"
        ))
    }

    /// 加载 Webhook 配置
    fn load_webhook_config() -> Option<WebhookConfig> {
        // 从 ~/.config/code-agent-monitor/config.json 加载
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".config/code-agent-monitor/config.json");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(webhook) = config.get("webhook") {
                            let gateway_url = webhook.get("gateway_url")
                                .and_then(|u| u.as_str())
                                .unwrap_or("http://localhost:18789")
                                .to_string();
                            let hook_token = webhook.get("hook_token")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();
                            let timeout_secs = webhook.get("timeout_secs")
                                .and_then(|t| t.as_u64())
                                .unwrap_or(30);
                            
                            if !hook_token.is_empty() {
                                debug!("Loaded webhook config from ~/.config/code-agent-monitor/config.json");
                                return Some(WebhookConfig {
                                    gateway_url,
                                    hook_token,
                                    timeout_secs,
                                });
                            }
                        }
                    }
                }
            }
        }
        
        // 从环境变量加载
        if let Ok(url) = std::env::var("CAM_WEBHOOK_URL") {
            if let Ok(token) = std::env::var("CAM_WEBHOOK_TOKEN") {
                if !token.is_empty() {
                    return Some(WebhookConfig {
                        gateway_url: url,
                        hook_token: token,
                        timeout_secs: 30,
                    });
                }
            }
        }
        
        None
    }
}

/// Messages API 请求体
#[derive(Serialize)]
pub(crate) struct MessagesRequest {
    pub model: String,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<Message>,
}

/// 消息
#[derive(Serialize)]
pub(crate) struct Message {
    pub role: String,
    pub content: String,
}

/// Messages API 响应体
#[derive(Deserialize)]
pub(crate) struct MessagesResponse {
    pub content: Vec<ContentBlock>,
}

/// 内容块
#[derive(Deserialize)]
pub(crate) struct ContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
}

/// API 错误响应
#[derive(Deserialize)]
pub(crate) struct ErrorResponse {
    pub error: ApiError,
}

#[derive(Deserialize)]
pub(crate) struct ApiError {
    pub message: String,
}

/// Anthropic API 客户端
pub struct AnthropicClient {
    client: reqwest::blocking::Client,
    pub(crate) config: AnthropicConfig,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = AnthropicConfig::default();
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert_eq!(config.max_tokens, DEFAULT_MAX_TOKENS);
    }
}
