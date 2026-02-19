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
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            gateway_url: "http://localhost:9080".to_string(),
            hook_token: String::new(),
            timeout_secs: 30,
        }
    }
}

/// Webhook 请求载荷
#[derive(Debug, Serialize)]
pub struct WebhookPayload {
    /// 消息内容
    pub message: String,
    /// 来源名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 目标 Agent ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// 唤醒模式: "now" | "next-heartbeat"
    #[serde(skip_serializing_if = "Option::is_none")]
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
            .header("Authorization", format!("Bearer {}", self.config.hook_token))
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
            Err(webhook_response.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    /// 发送 CAM 事件通知
    pub async fn send_cam_event(
        &self,
        event_data: &serde_json::Value,
    ) -> Result<WebhookResponse, String> {
        let message = format!("CAM Event: {}", serde_json::to_string(event_data).unwrap_or_default());
        
        self.send_notification(
            message,
            Some("cam-handler".to_string()),
            Some("telegram".to_string()),
            None,
        ).await
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
}
