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
use std::env;
use std::fs;
use std::time::Duration;
use tracing::{debug, info, warn};

// 清除代理环境变量，避免代理导致请求超时
fn clear_proxy_env() {
    env::remove_var("HTTP_PROXY");
    env::remove_var("HTTPS_PROXY");
    env::remove_var("http_proxy");
    env::remove_var("https_proxy");
    env::remove_var("ALL_PROXY");
    env::remove_var("all_proxy");
    // 设置 NO_PROXY 绕过所有代理
    env::set_var("NO_PROXY", "*");
    env::set_var("no_proxy", "*");
    
    // Debug: 打印当前代理设置
    debug!("Proxy settings after clear: HTTPS_PROXY={:?}, NO_PROXY={:?}", 
        env::var("HTTPS_PROXY").ok(), 
        env::var("NO_PROXY").ok());
}

#[allow(dead_code)]
pub fn init_clear_proxy() {
    clear_proxy_env();
}

/// Anthropic API 基础 URL
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// API 版本
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 默认模型 - MiniMax (免费额度，更稳定)
pub const DEFAULT_MODEL: &str = "MiniMax/M2.5";

/// 默认超时（毫秒）
pub const DEFAULT_TIMEOUT_MS: u64 = 5000;

/// 默认最大 tokens
pub const DEFAULT_MAX_TOKENS: u32 = 1500;

/// 提供商配置
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_type: String,
}

impl ProviderConfig {
    /// 获取完整的 API 端点 URL
    pub fn get_full_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        match self.api_type.as_str() {
            "openai" => format!("{}/chat/completions", base),
            _ => format!("{}/v1/messages", base), // anthropic 或默认
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
    /// Webhook 配置
    pub webhook: Option<WebhookConfig>,
    /// 提供商列表（用于 fallback）
    pub providers: Vec<ProviderConfig>,
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
            providers: Vec::new(),
        }
    }
}

impl AnthropicConfig {
    /// 从环境和配置文件自动加载配置
    pub fn auto_load() -> Result<Self> {
        // 加载 providers 配置
        let providers = Self::load_providers_from_config();
        
        // 如果有 providers 配置，使用第一个作为主配置
        if !providers.is_empty() {
            let primary = &providers[0];
            let webhook = Self::load_webhook_config();
            return Ok(Self {
                api_key: primary.api_key.clone(),
                base_url: primary.base_url.clone(),
                model: primary.model.clone(),
                timeout_ms: DEFAULT_TIMEOUT_MS,
                max_tokens: DEFAULT_MAX_TOKENS,
                webhook,
                providers,
            });
        }
        
        // 降级使用旧的配置方式
        let model = Self::load_model_from_config().unwrap_or_else(|| DEFAULT_MODEL.to_string());
        
        // 优先使用 MiniMax 配置
        if let Some((api_key, base_url)) = Self::load_minimax_config()? {
            let webhook = Self::load_webhook_config();
            return Ok(Self {
                api_key,
                base_url,
                model,
                timeout_ms: DEFAULT_TIMEOUT_MS,
                max_tokens: DEFAULT_MAX_TOKENS,
                webhook,
                providers: Vec::new(),
            });
        }
        
        let (api_key, base_url) = Self::load_api_config()?;
        let webhook = Self::load_webhook_config();
        
        Ok(Self {
            api_key,
            base_url,
            model,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_tokens: DEFAULT_MAX_TOKENS,
            webhook,
            providers: Vec::new(),
        })
    }

    /// 从配置文件加载 providers
    fn load_providers_from_config() -> Vec<ProviderConfig> {
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".config/code-agent-monitor/config.json");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(providers) = config.get("providers").and_then(|p| p.as_array()) {
                            let mut result = Vec::new();
                            for p in providers {
                                if let (Some(api_key), Some(base_url), Some(model)) = (
                                    p.get("api_key").and_then(|k| k.as_str()),
                                    p.get("base_url").and_then(|u| u.as_str()),
                                    p.get("model").and_then(|m| m.as_str()),
                                ) {
                                    let api_type = p.get("api_type")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("anthropic")
                                        .to_string();
                                    result.push(ProviderConfig {
                                        api_key: api_key.to_string(),
                                        base_url: base_url.to_string(),
                                        model: model.to_string(),
                                        api_type,
                                    });
                                }
                            }
                            if !result.is_empty() {
                                debug!("Loaded {} providers from config", result.len());
                                return result;
                            }
                        }
                    }
                }
            }
        }
        Vec::new()
    }

    /// 从配置文件加载模型
    fn load_model_from_config() -> Option<String> {
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".config/code-agent-monitor/config.json");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(model) = config.get("model").and_then(|m| m.as_str()) {
                            if !model.is_empty() {
                                debug!("Loaded model from config: {}", model);
                                return Some(model.to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// 加载 MiniMax 配置
    fn load_minimax_config() -> Result<Option<(String, String)>> {
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".config/code-agent-monitor/config.json");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                        let key = config.get("minimax_api_key").and_then(|k| k.as_str());
                        let base_url = config.get("minimax_base_url").and_then(|u| u.as_str());

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
                                    .unwrap_or_else(|| "https://api.minimaxi.com/anthropic/v1/messages".to_string());
                                debug!("Using MiniMax API: key=***, base_url={}", url);
                                return Ok(Some((key.to_string(), url)));
                            }
                        }
                    }
                }
            }
        }
        Ok(None)
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
    #[allow(dead_code)]
    pub thinking: Option<String>,
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
        // 清除代理环境变量 - 确保在 Client 创建之前清除
        clear_proxy_env();
        
        // 创建一个不使用系统代理配置的 client
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .danger_accept_invalid_certs(false)
            .build()
            .map_err(|e| anyhow!("Cannot create HTTP client: {}", e))?;

        Ok(Self { client, config })
    }

    /// 从自动加载的配置创建客户端
    pub fn from_config() -> Result<Self> {
        let config = AnthropicConfig::auto_load()?;
        Self::new(config)
    }

    /// 发送消息并获取响应（支持 fallback）
    pub fn complete(&self, prompt: &str, system: Option<&str>) -> Result<String> {
        // 如果有多个 providers，尝试 fallback
        if !self.config.providers.is_empty() {
            for (i, provider) in self.config.providers.iter().enumerate() {
                debug!(provider = i, model = %provider.model, "Trying provider");
                
                // 获取完整的 API URL
                let full_url = provider.get_full_url();
                
                // 创建临时配置
                let temp_config = AnthropicConfig {
                    api_key: provider.api_key.clone(),
                    base_url: full_url,
                    model: provider.model.clone(),
                    timeout_ms: self.config.timeout_ms,
                    max_tokens: self.config.max_tokens,
                    webhook: None,
                    providers: Vec::new(),
                };
                
                // 尝试这个 provider
                let temp_client = match AnthropicClient::new(temp_config) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!(provider = i, error = %e, "Failed to create client for provider");
                        continue;
                    }
                };
                
                match temp_client.send_anthropic_request(prompt, system) {
                    Ok(result) => {
                        info!(provider = i, model = %provider.model, "Provider succeeded");
                        return Ok(result);
                    }
                    Err(e) => {
                        warn!(provider = i, error = %e, "Provider failed, trying next");
                    }
                }
            }
            
            // 所有 providers 都失败
            return Err(anyhow!("All {} providers failed", self.config.providers.len()));
        }
        
        // 降级到旧的处理方式
        // 首先尝试 Anthropic 格式
        if let Ok(result) = self.send_anthropic_request(prompt, system) {
            return Ok(result);
        }
        
        // 如果失败，尝试 OpenAI 格式 (用于 MiniMax)
        debug!("Anthropic format failed, trying OpenAI format");
        self.send_openai_request(prompt)
    }
    
    /// 发送 Anthropic 格式请求
    fn send_anthropic_request(&self, prompt: &str, system: Option<&str>) -> Result<String> {
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
            "Sending Anthropic format request"
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

        // 提取文本内容 - 优先获取 text，如果没有则获取 thinking
        let text = response
            .content
            .iter()
            .filter(|c| c.content_type == "text")
            .filter_map(|c| c.text.as_ref())
            .cloned()
            .collect::<Vec<String>>()
            .join("");

        // 如果没有 text，尝试获取 thinking 内容
        let text = if text.is_empty() {
            response
                .content
                .iter()
                .filter(|c| c.content_type == "thinking")
                .filter_map(|c| c.thinking.as_ref())
                .cloned()
                .collect::<Vec<String>>()
                .join("")
        } else {
            text
        };

        if text.is_empty() {
            // 打印调试信息
            let content_types: Vec<&str> = response.content.iter().map(|c| c.content_type.as_str()).collect();
            warn!(content_types = ?content_types, "Empty response from Anthropic API - no text or thinking content found");
        }

        Ok(text)
    }
    
    /// 发送 OpenAI 格式请求 (用于 MiniMax)
    fn send_openai_request(&self, prompt: &str) -> Result<String> {
        // 构建 OpenAI 格式请求
        let request = serde_json::json!({
            "model": self.config.model,
            "max_tokens": self.config.max_tokens,
            "messages": [
                {"role": "user", "content": prompt}
            ]
        });
        
        // 尝试多个可能的端点
        let endpoints = vec![
            format!("{}/v1/chat/completions", self.config.base_url.trim_end_matches("/v1/messages")),
            format!("{}/chat/completions", self.config.base_url.trim_end_matches("/v1")),
            "https://api.minimaxi.com/v1/chat/completions".to_string(),
        ];
        
        let mut last_error = None;
        
        for url in endpoints {
            debug!(url = %url, "Trying OpenAI endpoint");
            
            let start = std::time::Instant::now();
            let result = self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("content-type", "application/json")
                .json(&request)
                .send();
            
            match result {
                Ok(response) => {
                    let status = response.status();
                    if !status.is_success() {
                        let body = response.text().unwrap_or_default();
                        last_error = Some(anyhow!("API error ({}): {}", status, body));
                        continue;
                    }
                    
                    let body = response.text().map_err(|e| anyhow!("Failed to read response: {}", e))?;
                    
                    // 解析 OpenAI 格式响应
                    if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&body) {
                        if let Some(content) = resp.get("choices")
                            .and_then(|c| c.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|c| c.get("message"))
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            debug!(elapsed_ms = start.elapsed().as_millis(), "OpenAI request succeeded");
                            return Ok(content.to_string());
                        }
                    }
                    
                    last_error = Some(anyhow!("Failed to parse OpenAI response: {}", body));
                }
                Err(e) => {
                    last_error = Some(anyhow!("Request failed: {}", e));
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| anyhow!("All endpoints failed")))
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
