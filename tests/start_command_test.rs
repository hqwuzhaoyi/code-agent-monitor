//! cam start 命令单元测试
//!
//! 测试 cam start 命令的参数解析、配置验证和错误处理。

use code_agent_monitor::agent::{AgentManager, AgentType, StartAgentRequest, StartAgentResponse};
use code_agent_monitor::cli::{handle_start, StartArgs, StartOutput};

// ============================================================================
// 参数解析测试
// ============================================================================

mod args_parsing {
    use super::*;

    #[test]
    fn test_start_request_default_agent_type() {
        // Given: 只指定 project_path 的请求
        let request = StartAgentRequest {
            project_path: "/tmp/test-project".to_string(),
            agent_type: None,
            resume_session: None,
            initial_prompt: None,
            agent_id: None,
            tmux_session: None,
        };

        // Then: agent_type 应该为 None（由 AgentManager 默认为 claude）
        assert!(request.agent_type.is_none());
    }

    #[test]
    fn test_start_request_with_agent_type() {
        // Given: 指定 agent_type 的请求
        let request = StartAgentRequest {
            project_path: "/tmp/test-project".to_string(),
            agent_type: Some("codex".to_string()),
            resume_session: None,
            initial_prompt: None,
            agent_id: None,
            tmux_session: None,
        };

        // Then: agent_type 应该正确设置
        assert_eq!(request.agent_type, Some("codex".to_string()));
    }

    #[test]
    fn test_start_request_with_prompt() {
        // Given: 带有初始 prompt 的请求
        let request = StartAgentRequest {
            project_path: "/tmp/test-project".to_string(),
            agent_type: None,
            resume_session: None,
            initial_prompt: Some("Hello, Claude!".to_string()),
            agent_id: None,
            tmux_session: None,
        };

        // Then: initial_prompt 应该正确设置
        assert_eq!(request.initial_prompt, Some("Hello, Claude!".to_string()));
    }

    #[test]
    fn test_start_request_with_custom_agent_id() {
        // Given: 指定自定义 agent_id 的请求
        let request = StartAgentRequest {
            project_path: "/tmp/test-project".to_string(),
            agent_type: None,
            resume_session: None,
            initial_prompt: None,
            agent_id: Some("custom-agent-123".to_string()),
            tmux_session: None,
        };

        // Then: agent_id 应该正确设置
        assert_eq!(request.agent_id, Some("custom-agent-123".to_string()));
    }

    #[test]
    fn test_start_request_with_custom_tmux_session() {
        // Given: 指定自定义 tmux_session 的请求
        let request = StartAgentRequest {
            project_path: "/tmp/test-project".to_string(),
            agent_type: None,
            resume_session: None,
            initial_prompt: None,
            agent_id: None,
            tmux_session: Some("my-session".to_string()),
        };

        // Then: tmux_session 应该正确设置
        assert_eq!(request.tmux_session, Some("my-session".to_string()));
    }
}

// ============================================================================
// AgentType 解析测试
// ============================================================================

mod agent_type_parsing {
    use super::*;

    #[test]
    fn test_parse_claude_variants() {
        // 测试 claude 的各种别名
        assert_eq!("claude".parse::<AgentType>().unwrap(), AgentType::Claude);
        assert_eq!(
            "claude-code".parse::<AgentType>().unwrap(),
            AgentType::Claude
        );
        assert_eq!(
            "claudecode".parse::<AgentType>().unwrap(),
            AgentType::Claude
        );
        assert_eq!("CLAUDE".parse::<AgentType>().unwrap(), AgentType::Claude);
    }

    #[test]
    fn test_parse_codex() {
        assert_eq!("codex".parse::<AgentType>().unwrap(), AgentType::Codex);
        assert_eq!("CODEX".parse::<AgentType>().unwrap(), AgentType::Codex);
    }

    #[test]
    fn test_parse_opencode() {
        assert_eq!(
            "opencode".parse::<AgentType>().unwrap(),
            AgentType::OpenCode
        );
        assert_eq!(
            "OPENCODE".parse::<AgentType>().unwrap(),
            AgentType::OpenCode
        );
    }

    #[test]
    fn test_parse_gemini_variants() {
        assert_eq!("gemini".parse::<AgentType>().unwrap(), AgentType::GeminiCli);
        assert_eq!(
            "gemini-cli".parse::<AgentType>().unwrap(),
            AgentType::GeminiCli
        );
        assert_eq!(
            "geminicli".parse::<AgentType>().unwrap(),
            AgentType::GeminiCli
        );
    }

    #[test]
    fn test_parse_mistral_variants() {
        assert_eq!(
            "mistral".parse::<AgentType>().unwrap(),
            AgentType::MistralVibe
        );
        assert_eq!(
            "mistral-vibe".parse::<AgentType>().unwrap(),
            AgentType::MistralVibe
        );
        assert_eq!(
            "mistralvibe".parse::<AgentType>().unwrap(),
            AgentType::MistralVibe
        );
    }

    #[test]
    fn test_parse_mock() {
        assert_eq!("mock".parse::<AgentType>().unwrap(), AgentType::Mock);
    }

    #[test]
    fn test_parse_invalid_agent_type() {
        // 无效的 agent 类型应该返回错误
        let result = "invalid-agent".parse::<AgentType>();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown agent type"));
    }
}

// ============================================================================
// AgentType Display 测试
// ============================================================================

mod agent_type_display {
    use super::*;

    #[test]
    fn test_display_claude() {
        assert_eq!(AgentType::Claude.to_string(), "claude");
    }

    #[test]
    fn test_display_codex() {
        assert_eq!(AgentType::Codex.to_string(), "codex");
    }

    #[test]
    fn test_display_opencode() {
        assert_eq!(AgentType::OpenCode.to_string(), "opencode");
    }

    #[test]
    fn test_display_gemini() {
        assert_eq!(AgentType::GeminiCli.to_string(), "gemini-cli");
    }

    #[test]
    fn test_display_mistral() {
        assert_eq!(AgentType::MistralVibe.to_string(), "mistral-vibe");
    }

    #[test]
    fn test_display_mock() {
        assert_eq!(AgentType::Mock.to_string(), "mock");
    }

    #[test]
    fn test_display_unknown() {
        assert_eq!(AgentType::Unknown.to_string(), "unknown");
    }
}

// ============================================================================
// StartAgentRequest 序列化测试
// ============================================================================

mod serialization {
    use super::*;

    #[test]
    fn test_start_request_serialization_minimal() {
        // Given: 最小化请求
        let request = StartAgentRequest {
            project_path: "/tmp/test".to_string(),
            agent_type: None,
            resume_session: None,
            initial_prompt: None,
            agent_id: None,
            tmux_session: None,
        };

        // When: 序列化
        let json = serde_json::to_string(&request).unwrap();

        // Then: 只包含 project_path
        assert!(json.contains("project_path"));
        assert!(!json.contains("agent_type"));
        assert!(!json.contains("resume_session"));
    }

    #[test]
    fn test_start_request_serialization_full() {
        // Given: 完整请求
        let request = StartAgentRequest {
            project_path: "/tmp/test".to_string(),
            agent_type: Some("claude".to_string()),
            resume_session: Some("session-123".to_string()),
            initial_prompt: Some("Hello".to_string()),
            agent_id: Some("agent-456".to_string()),
            tmux_session: Some("tmux-789".to_string()),
        };

        // When: 序列化
        let json = serde_json::to_string(&request).unwrap();

        // Then: 包含所有字段
        assert!(json.contains("project_path"));
        assert!(json.contains("agent_type"));
        assert!(json.contains("resume_session"));
        assert!(json.contains("initial_prompt"));
        assert!(json.contains("agent_id"));
        assert!(json.contains("tmux_session"));
    }

    #[test]
    fn test_start_request_deserialization() {
        // Given: JSON 字符串
        let json = r#"{
            "project_path": "/tmp/test",
            "agent_type": "codex",
            "initial_prompt": "Hello"
        }"#;

        // When: 反序列化
        let request: StartAgentRequest = serde_json::from_str(json).unwrap();

        // Then: 正确解析
        assert_eq!(request.project_path, "/tmp/test");
        assert_eq!(request.agent_type, Some("codex".to_string()));
        assert_eq!(request.initial_prompt, Some("Hello".to_string()));
        assert!(request.resume_session.is_none());
    }

    #[test]
    fn test_start_response_serialization() {
        // Given: 响应
        let response = StartAgentResponse {
            agent_id: "cam-12345678".to_string(),
            tmux_session: "cam-12345678".to_string(),
        };

        // When: 序列化
        let json = serde_json::to_string(&response).unwrap();

        // Then: 包含所有字段
        assert!(json.contains("agent_id"));
        assert!(json.contains("tmux_session"));
        assert!(json.contains("cam-12345678"));
    }
}

// ============================================================================
// AgentManager 测试（使用测试专用实例）
// ============================================================================

mod agent_manager {
    use super::*;

    #[test]
    fn test_agent_manager_new_for_test() {
        // Given/When: 创建测试用 AgentManager
        let manager = AgentManager::new_for_test();

        // Then: 应该成功创建
        drop(manager);
    }

    #[test]
    fn test_agent_manager_list_agents_empty() {
        // Given: 新的测试 AgentManager
        let manager = AgentManager::new_for_test();

        // When: 列出 agents
        let agents = manager.list_agents().unwrap();

        // Then: 应该为空
        assert!(agents.is_empty());
    }
}

// ============================================================================
// 错误处理测试
// ============================================================================

mod error_handling {
    use super::*;

    #[test]
    fn test_invalid_agent_type_error_message() {
        // Given: 无效的 agent 类型
        let result = "not-a-real-agent".parse::<AgentType>();

        // Then: 错误消息应该包含类型名称
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not-a-real-agent"));
    }

    #[test]
    fn test_get_nonexistent_agent() {
        // Given: 测试 AgentManager
        let manager = AgentManager::new_for_test();

        // When: 获取不存在的 agent
        let result = manager.get_agent("nonexistent-agent");

        // Then: 应该返回 None
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_stop_nonexistent_agent() {
        // Given: 测试 AgentManager
        let manager = AgentManager::new_for_test();

        // When: 停止不存在的 agent
        let result = manager.stop_agent("nonexistent-agent");

        // Then: 应该返回错误
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_send_input_to_nonexistent_agent() {
        // Given: 测试 AgentManager
        let manager = AgentManager::new_for_test();

        // When: 向不存在的 agent 发送输入
        let result = manager.send_input("nonexistent-agent", "hello");

        // Then: 应该返回错误
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_logs_from_nonexistent_agent() {
        // Given: 测试 AgentManager
        let manager = AgentManager::new_for_test();

        // When: 获取不存在的 agent 的日志
        let result = manager.get_logs("nonexistent-agent", 50);

        // Then: 应该返回错误
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}

// ============================================================================
// CLI StartArgs 测试
// ============================================================================

mod cli_start_args {
    use super::*;

    #[test]
    fn test_start_args_default_values() {
        // Given: 使用默认值创建 StartArgs
        let args = StartArgs {
            agent: "claude-code".to_string(),
            cwd: None,
            name: None,
            resume: None,
            json: false,
            prompt: None,
        };

        // Then: 验证默认值
        assert_eq!(args.agent, "claude-code");
        assert!(args.cwd.is_none());
        assert!(args.name.is_none());
        assert!(args.resume.is_none());
        assert!(!args.json);
        assert!(args.prompt.is_none());
    }

    #[test]
    fn test_start_args_with_all_options() {
        // Given: 设置所有选项
        let args = StartArgs {
            agent: "codex".to_string(),
            cwd: Some("/tmp/project".to_string()),
            name: Some("my-session".to_string()),
            resume: None,
            json: true,
            prompt: Some("Hello".to_string()),
        };

        // Then: 验证所有值
        assert_eq!(args.agent, "codex");
        assert_eq!(args.cwd, Some("/tmp/project".to_string()));
        assert_eq!(args.name, Some("my-session".to_string()));
        assert!(args.json);
        assert_eq!(args.prompt, Some("Hello".to_string()));
    }

    #[test]
    fn test_start_args_with_resume() {
        // Given: 使用 resume 选项
        let args = StartArgs {
            agent: "claude-code".to_string(),
            cwd: None,
            name: None,
            resume: Some("session-abc123".to_string()),
            json: false,
            prompt: None, // resume 和 prompt 互斥
        };

        // Then: 验证 resume 设置
        assert_eq!(args.resume, Some("session-abc123".to_string()));
        assert!(args.prompt.is_none());
    }
}

// ============================================================================
// CLI StartOutput 测试
// ============================================================================

mod cli_start_output {
    use super::*;

    #[test]
    fn test_start_output_serialization() {
        // Given: StartOutput
        let output = StartOutput {
            agent_id: "cam-12345678".to_string(),
            tmux_session: "cam-12345678".to_string(),
            agent_type: "claude".to_string(),
            project_path: "/tmp/project".to_string(),
        };

        // When: 序列化为 JSON
        let json = serde_json::to_string(&output).unwrap();

        // Then: 包含所有字段
        assert!(json.contains("agent_id"));
        assert!(json.contains("tmux_session"));
        assert!(json.contains("agent_type"));
        assert!(json.contains("project_path"));
        assert!(json.contains("cam-12345678"));
        assert!(json.contains("claude"));
    }

    #[test]
    fn test_start_output_pretty_json() {
        // Given: StartOutput
        let output = StartOutput {
            agent_id: "cam-abc".to_string(),
            tmux_session: "cam-abc".to_string(),
            agent_type: "codex".to_string(),
            project_path: "/home/user/project".to_string(),
        };

        // When: 序列化为 pretty JSON
        let json = serde_json::to_string_pretty(&output).unwrap();

        // Then: 格式化输出包含换行
        assert!(json.contains('\n'));
        assert!(json.contains("codex"));
    }
}

// ============================================================================
// handle_start 错误处理测试
// ============================================================================

mod handle_start_errors {
    use super::*;

    #[test]
    fn test_handle_start_invalid_agent_type() {
        // Given: 无效的 agent 类型
        let args = StartArgs {
            agent: "invalid-agent-xyz".to_string(),
            cwd: Some("/tmp".to_string()),
            name: None,
            resume: None,
            json: false,
            prompt: None,
        };

        // When: 调用 handle_start
        let result = handle_start(args);

        // Then: 应该返回错误
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("不支持的 agent 类型") || err.contains("invalid-agent-xyz"));
    }

    #[test]
    fn test_handle_start_nonexistent_directory() {
        // Given: 不存在的工作目录
        let args = StartArgs {
            agent: "claude-code".to_string(),
            cwd: Some("/nonexistent/path/that/does/not/exist".to_string()),
            name: None,
            resume: None,
            json: false,
            prompt: None,
        };

        // When: 调用 handle_start
        let result = handle_start(args);

        // Then: 应该返回错误
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("工作目录不存在") || err.contains("不存在"));
    }
}

// ============================================================================
// TODO: cam start CLI 命令测试（待 src/cli/start.rs 实现后启用）
// ============================================================================

// mod cli_start_command {
//     use super::*;
//
//     #[test]
//     fn test_start_command_default_args() {
//         // 测试默认参数
//     }
//
//     #[test]
//     fn test_start_command_with_agent_type() {
//         // 测试指定 agent 类型
//     }
//
//     #[test]
//     fn test_start_command_with_working_dir() {
//         // 测试指定工作目录
//     }
//
//     #[test]
//     fn test_start_command_with_prompt() {
//         // 测试指定 prompt
//     }
//
//     #[test]
//     fn test_start_command_invalid_agent_type() {
//         // 测试无效 agent 类型
//     }
//
//     #[test]
//     fn test_start_command_nonexistent_dir() {
//         // 测试不存在的工作目录
//     }
// }
