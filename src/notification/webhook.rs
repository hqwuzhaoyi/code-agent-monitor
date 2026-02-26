//! OpenClaw Webhook 客户端模块
//!
//! 通过 HTTP Webhook 调用 OpenClaw Gateway API

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Webhook 客户端配置
#[derive(Debug, Clone)]
pub struct WebhookConfig {
    /// Gateway URL (如 http://localhost:9080)
    pub gateway_url: String,
    /// Hooks token (认证用)
    pub hook_token: String,
    /// 超时时间 (秒)
    pub timeout_secs: u64,
    /// Optional delivery defaults for `/hooks/agent`
    pub default_channel: Option<String>,
    pub default_to: Option<String>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            gateway_url: "http://localhost:9080".to_string(),
            hook_token: String::new(),
            timeout_secs: 30,
            default_channel: None,
            default_to: None,
        }
    }
}

/// 从配置文件加载 webhook 配置
/// 配置文件路径: ~/.config/code-agent-monitor/config.json
pub fn load_webhook_config_from_file() -> Option<WebhookConfig> {
    use std::fs;

    let config_path = dirs::home_dir()?
        .join(".config")
        .join("code-agent-monitor")
        .join("config.json");

    if !config_path.exists() {
        return None;
    }

    let content = fs::read_to_string(&config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    let webhook = json.get("webhook")?;

    Some(WebhookConfig {
        gateway_url: webhook
            .get("gateway_url")
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:18789")
            .to_string(),
        hook_token: webhook
            .get("hook_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        timeout_secs: webhook
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30),
        default_channel: webhook
            .get("default_channel")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        default_to: webhook
            .get("default_to")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

/// Webhook 请求载荷
///
/// NOTE: OpenClaw Gateway 的 `/hooks/agent` 使用 camelCase 字段名（如 `agentId`, `wakeMode`）。
/// 这里用 serde rename 保持 Rust 侧字段命名风格不变，同时确保网关能正确解析。
#[derive(Debug, Serialize)]
pub struct WebhookPayload {
    /// 消息内容
    pub message: String,
    /// 来源名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 目标 Agent ID
    #[serde(skip_serializing_if = "Option::is_none", rename = "agentId")]
    pub agent_id: Option<String>,
    /// 唤醒模式: "now" | "next-heartbeat"
    #[serde(skip_serializing_if = "Option::is_none", rename = "wakeMode")]
    pub wake_mode: Option<String>,
    /// 是否发送回复
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deliver: Option<bool>,
    /// Channel: "telegram" | "whatsapp" | 等
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// 接收者 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

/// Webhook 响应
#[derive(Debug, Deserialize)]
pub struct WebhookResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
}

/// OpenClaw Webhook 客户端
#[derive(Debug)]
pub struct WebhookClient {
    client: Client,
    config: WebhookConfig,
}

impl WebhookClient {
    /// 创建新的 Webhook 客户端
    pub fn new(config: WebhookConfig) -> Result<Self, String> {
        if config.hook_token.is_empty() {
            return Err("hook_token is required".to_string());
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self { client, config })
    }

    /// 发送通知到 OpenClaw Gateway (同步阻塞版本)
    pub fn send_notification_blocking(
        &self,
        message: String,
        agent_id: Option<String>,
        channel: Option<String>,
        to: Option<String>,
    ) -> Result<WebhookResponse, String> {
        use std::time::Duration;

        let url = format!("{}/hooks/agent", self.config.gateway_url);

        let payload = WebhookPayload {
            message,
            name: Some("CAM".to_string()),
            agent_id,
            wake_mode: Some("now".to_string()),
            deliver: Some(true),
            channel,
            to,
        };

        // 使用 blocking client
        let blocking_client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .build()
            .map_err(|e| format!("Failed to create blocking client: {}", e))?;

        let response = blocking_client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.hook_token),
            )
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let webhook_response: WebhookResponse = response
            .json()
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if webhook_response.ok {
            Ok(webhook_response)
        } else {
            Err(webhook_response
                .error
                .unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    /// 发送通知到 OpenClaw Gateway
    ///
    /// # Arguments
    /// * `message` - 要发送的消息内容
    /// * `agent_id` - 可选的 agent ID
    /// * `channel` - 可选的 channel (如 "telegram")
    /// * `to` - 可选的接收者 ID
    pub async fn send_notification(
        &self,
        message: String,
        agent_id: Option<String>,
        channel: Option<String>,
        to: Option<String>,
    ) -> Result<WebhookResponse, String> {
        let url = format!("{}/hooks/agent", self.config.gateway_url);

        let payload = WebhookPayload {
            message,
            name: Some("CAM".to_string()),
            agent_id,
            wake_mode: Some("now".to_string()),
            deliver: Some(true),
            channel,
            to,
        };

        let response = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.hook_token),
            )
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let webhook_response: WebhookResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if webhook_response.ok {
            Ok(webhook_response)
        } else {
            Err(webhook_response
                .error
                .unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    /// 发送 CAM 事件通知
    pub async fn send_cam_event(
        &self,
        event_data: &serde_json::Value,
    ) -> Result<WebhookResponse, String> {
        let message = format!(
            "CAM Event: {}",
            serde_json::to_string(event_data).unwrap_or_default()
        );

        self.send_notification(
            message,
            Some("cam-handler".to_string()),
            Some("telegram".to_string()),
            None,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_config_default() {
        let config = WebhookConfig::default();
        assert_eq!(config.gateway_url, "http://localhost:9080");
        assert_eq!(config.timeout_secs, 30);
        assert!(config.default_channel.is_none());
        assert!(config.default_to.is_none());
    }

    #[test]
    fn test_webhook_client_requires_token() {
        let config = WebhookConfig {
            hook_token: String::new(),
            ..Default::default()
        };

        let result = WebhookClient::new(config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("hook_token"));
    }

    #[test]
    fn test_webhook_payload_uses_camel_case_for_gateway() {
        let payload = WebhookPayload {
            message: "hi".to_string(),
            name: Some("CAM".to_string()),
            agent_id: Some("hooks".to_string()),
            wake_mode: Some("now".to_string()),
            deliver: Some(true),
            channel: Some("telegram".to_string()),
            to: Some("1440537501".to_string()),
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert!(json.get("agentId").is_some());
        assert!(json.get("wakeMode").is_some());
        assert!(json.get("agent_id").is_none());
        assert!(json.get("wake_mode").is_none());
    }
}
