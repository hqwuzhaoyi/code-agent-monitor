//! AI Client 错误处理测试
//!
//! 测试 Anthropic API 客户端的错误处理场景：
//! - 配置加载错误
//! - API 响应解析错误
//! - Provider fallback 逻辑

// 使用正确的导出路径
use code_agent_monitor::anthropic::{
    AnthropicConfig, AnthropicClient,
    ANTHROPIC_API_URL, ANTHROPIC_VERSION, DEFAULT_MODEL, DEFAULT_TIMEOUT_MS, DEFAULT_MAX_TOKENS,
};

// ============================================================================
// AnthropicConfig 测试
// ============================================================================

mod config_tests {
    use super::*;

    #[test]
    fn test_config_default_values() {
        let config = AnthropicConfig::default();

        assert!(config.api_key.is_empty());
        assert_eq!(config.base_url, ANTHROPIC_API_URL);
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert_eq!(config.max_tokens, DEFAULT_MAX_TOKENS);
        assert!(config.webhook.is_none());
        assert!(config.providers.is_empty());
    }

    #[test]
    fn test_config_custom_values() {
        let config = AnthropicConfig {
            api_key: "test-key".to_string(),
            base_url: "https://custom.api.com/v1/messages".to_string(),
            model: "claude-3-opus".to_string(),
            timeout_ms: 10000,
            max_tokens: 2000,
            webhook: None,
            providers: Vec::new(),
        };

        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.base_url, "https://custom.api.com/v1/messages");
        assert_eq!(config.model, "claude-3-opus");
        assert_eq!(config.timeout_ms, 10000);
        assert_eq!(config.max_tokens, 2000);
    }
}

// ============================================================================
// AnthropicClient 创建测试
// ============================================================================

mod client_creation_tests {
    use super::*;

    #[test]
    fn test_client_creation_with_valid_config() {
        let config = AnthropicConfig {
            api_key: "test-key".to_string(),
            base_url: ANTHROPIC_API_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_tokens: DEFAULT_MAX_TOKENS,
            webhook: None,
            providers: Vec::new(),
        };

        let result = AnthropicClient::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_creation_with_empty_api_key() {
        // 空 API key 应该能创建客户端（验证在请求时进行）
        let config = AnthropicConfig {
            api_key: "".to_string(),
            ..Default::default()
        };

        let result = AnthropicClient::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_creation_with_custom_timeout() {
        let config = AnthropicConfig {
            api_key: "test-key".to_string(),
            timeout_ms: 30000, // 30 秒
            ..Default::default()
        };

        let result = AnthropicClient::new(config);
        assert!(result.is_ok());
    }
}

// ============================================================================
// 常量测试
// ============================================================================

mod constants_tests {
    use super::*;

    #[test]
    fn test_anthropic_api_url() {
        assert_eq!(ANTHROPIC_API_URL, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_anthropic_version() {
        assert_eq!(ANTHROPIC_VERSION, "2023-06-01");
    }

    #[test]
    fn test_default_model() {
        // 默认模型应该是 MiniMax
        assert!(DEFAULT_MODEL.contains("MiniMax"));
    }

    #[test]
    fn test_default_timeout() {
        // 默认超时应该是 5 秒
        assert_eq!(DEFAULT_TIMEOUT_MS, 5000);
    }

    #[test]
    fn test_default_max_tokens() {
        // 默认最大 tokens
        assert_eq!(DEFAULT_MAX_TOKENS, 1500);
    }
}

// ============================================================================
// 配置加载错误测试（需要隔离环境）
// ============================================================================

mod config_loading_tests {
    use super::*;

    #[test]
    fn test_auto_load_function_exists() {
        // 验证 auto_load 函数存在且不会 panic
        // 实际结果取决于环境配置
        let result = AnthropicConfig::auto_load();
        // 不检查结果，只验证函数可以调用
        let _ = result;
    }
}

// ============================================================================
// Provider Fallback 逻辑测试
// ============================================================================

mod fallback_tests {
    use super::*;
    use code_agent_monitor::ai::client::ProviderConfig;

    #[test]
    fn test_config_with_multiple_providers() {
        let config = AnthropicConfig {
            api_key: "primary-key".to_string(),
            providers: vec![
                ProviderConfig {
                    api_key: "fallback1-key".to_string(),
                    base_url: "https://fallback1.api.com".to_string(),
                    model: "fallback1-model".to_string(),
                    api_type: "anthropic".to_string(),
                },
                ProviderConfig {
                    api_key: "fallback2-key".to_string(),
                    base_url: "https://fallback2.api.com".to_string(),
                    model: "fallback2-model".to_string(),
                    api_type: "openai".to_string(),
                },
            ],
            ..Default::default()
        };

        // 验证 providers 配置正确
        assert_eq!(config.providers.len(), 2);
        assert_eq!(config.providers[0].model, "fallback1-model");
        assert_eq!(config.providers[1].api_type, "openai");
    }

    #[test]
    fn test_empty_providers_uses_primary_config() {
        let config = AnthropicConfig {
            api_key: "primary-key".to_string(),
            base_url: "https://primary.api.com/v1/messages".to_string(),
            model: "primary-model".to_string(),
            providers: Vec::new(),
            ..Default::default()
        };

        // 没有 providers 时应该使用主配置
        assert!(config.providers.is_empty());
        assert_eq!(config.api_key, "primary-key");
        assert_eq!(config.model, "primary-model");
    }

    #[test]
    fn test_provider_get_full_url_anthropic() {
        let provider = ProviderConfig {
            api_key: "key".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            model: "claude".to_string(),
            api_type: "anthropic".to_string(),
        };

        assert_eq!(provider.get_full_url(), "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_provider_get_full_url_openai() {
        let provider = ProviderConfig {
            api_key: "key".to_string(),
            base_url: "https://api.openai.com".to_string(),
            model: "gpt-4".to_string(),
            api_type: "openai".to_string(),
        };

        assert_eq!(provider.get_full_url(), "https://api.openai.com/chat/completions");
    }

    #[test]
    fn test_provider_get_full_url_default_type() {
        let provider = ProviderConfig {
            api_key: "key".to_string(),
            base_url: "https://custom.api.com".to_string(),
            model: "custom".to_string(),
            api_type: "".to_string(), // 空类型默认为 anthropic
        };

        assert_eq!(provider.get_full_url(), "https://custom.api.com/v1/messages");
    }

    #[test]
    fn test_provider_get_full_url_trailing_slash() {
        let provider = ProviderConfig {
            api_key: "key".to_string(),
            base_url: "https://api.example.com/".to_string(), // 带尾部斜杠
            model: "model".to_string(),
            api_type: "anthropic".to_string(),
        };

        // 应该正确处理尾部斜杠
        assert_eq!(provider.get_full_url(), "https://api.example.com/v1/messages");
    }
}

// ============================================================================
// WebhookConfig 测试
// ============================================================================

mod webhook_config_tests {
    use super::*;
    use code_agent_monitor::ai::client::WebhookConfig;

    #[test]
    fn test_webhook_config_creation() {
        let webhook = WebhookConfig {
            gateway_url: "http://localhost:18789".to_string(),
            hook_token: "test-token".to_string(),
            timeout_secs: 30,
        };

        assert_eq!(webhook.gateway_url, "http://localhost:18789");
        assert_eq!(webhook.hook_token, "test-token");
        assert_eq!(webhook.timeout_secs, 30);
    }

    #[test]
    fn test_config_with_webhook() {
        let webhook = WebhookConfig {
            gateway_url: "http://localhost:18789".to_string(),
            hook_token: "test-token".to_string(),
            timeout_secs: 60,
        };

        let config = AnthropicConfig {
            webhook: Some(webhook),
            ..Default::default()
        };

        assert!(config.webhook.is_some());
        let wh = config.webhook.unwrap();
        assert_eq!(wh.timeout_secs, 60);
    }
}
