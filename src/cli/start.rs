// src/cli/start.rs
//! Start 命令 - 启动 AI 编码代理
//!
//! 启动 Claude Code 或 Codex agent，并自动注册到 CAM 进行监控。

use crate::agent::{AgentManager, AgentType, StartAgentRequest};
use crate::agent::adapter::get_adapter;
use crate::infra::tmux::TmuxManager;
use anyhow::{anyhow, Result};
use clap::Args;
use serde::Serialize;
use std::path::Path;

/// Start 命令参数
#[derive(Args)]
pub struct StartArgs {
    /// Agent 类型: claude-code, codex
    #[arg(long, short, default_value = "claude-code")]
    pub agent: String,

    /// 工作目录
    #[arg(long, short = 'c')]
    pub cwd: Option<String>,

    /// tmux session 名称
    #[arg(long, short)]
    pub name: Option<String>,

    /// 恢复指定 session
    #[arg(long, short, conflicts_with = "prompt")]
    pub resume: Option<String>,

    /// 输出 JSON 格式
    #[arg(long)]
    pub json: bool,

    /// 初始 prompt
    pub prompt: Option<String>,
}

/// Start 命令输出
#[derive(Debug, Serialize)]
pub struct StartOutput {
    pub agent_id: String,
    pub tmux_session: String,
    pub agent_type: String,
    pub project_path: String,
}

/// 处理 start 命令
pub fn handle_start(args: StartArgs) -> Result<()> {
    // 1. 参数验证
    let agent_type: AgentType = args.agent.parse()
        .map_err(|_| anyhow!("不支持的 agent 类型: {}，可选: claude-code, codex", args.agent))?;

    // 获取工作目录
    let cwd = args.cwd
        .map(|p| {
            // 展开 ~ 为 home 目录
            if p.starts_with("~/") {
                dirs::home_dir()
                    .map(|h| h.join(&p[2..]).to_string_lossy().into_owned())
                    .unwrap_or(p)
            } else {
                p
            }
        })
        .unwrap_or_else(|| std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".to_string()));

    // 验证工作目录存在
    if !Path::new(&cwd).exists() {
        return Err(anyhow!("工作目录不存在: {}", cwd));
    }

    // 2. 检查依赖
    let tmux = TmuxManager::new();
    if !tmux.is_available() {
        return Err(anyhow!("tmux 未安装或不可用\n请先安装 tmux: brew install tmux"));
    }

    let adapter = get_adapter(&agent_type);
    if !adapter.is_installed() {
        let install_hint = match agent_type {
            AgentType::Claude => "npm install -g @anthropic-ai/claude-code",
            AgentType::Codex => "npm install -g @openai/codex",
            _ => "请参考官方文档安装",
        };
        return Err(anyhow!("{} 命令未找到\n请先安装: {}", args.agent, install_hint));
    }

    // 3. 构建启动请求
    let request = StartAgentRequest {
        project_path: cwd.clone(),
        agent_type: Some(agent_type.to_string()),
        resume_session: args.resume,
        initial_prompt: args.prompt,
        agent_id: None,
        tmux_session: args.name,
    };

    // 4. 启动 agent
    let agent_manager = AgentManager::new();
    let response = agent_manager.start_agent(request)?;

    // 5. 输出结果
    let output = StartOutput {
        agent_id: response.agent_id.clone(),
        tmux_session: response.tmux_session.clone(),
        agent_type: agent_type.to_string(),
        project_path: cwd,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let agent_name = match agent_type {
            AgentType::Claude => "Claude Code",
            AgentType::Codex => "Codex",
            _ => &args.agent,
        };
        println!("已启动 {} agent", agent_name);
        println!("  agent_id: {}", output.agent_id);
        println!("  tmux_session: {}", output.tmux_session);
        println!("  工作目录: {}", output.project_path);
        println!();
        println!("查看输出: tmux attach -t {}", output.tmux_session);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_args_defaults() {
        // 验证默认值
        let args = StartArgs {
            agent: "claude-code".to_string(),
            cwd: None,
            name: None,
            resume: None,
            json: false,
            prompt: None,
        };
        assert_eq!(args.agent, "claude-code");
        assert!(!args.json);
    }

    #[test]
    fn test_agent_type_parsing() {
        // claude-code 应该解析为 Claude
        let agent_type: Result<AgentType, _> = "claude-code".parse();
        assert!(agent_type.is_ok());
        assert_eq!(agent_type.unwrap(), AgentType::Claude);

        // codex 应该解析为 Codex
        let agent_type: Result<AgentType, _> = "codex".parse();
        assert!(agent_type.is_ok());
        assert_eq!(agent_type.unwrap(), AgentType::Codex);
    }

    #[test]
    fn test_invalid_agent_type() {
        let agent_type: Result<AgentType, _> = "invalid-agent".parse();
        assert!(agent_type.is_err());
    }

    #[test]
    fn test_cwd_expansion() {
        // 测试 ~ 展开
        let home = dirs::home_dir().unwrap();
        let expanded = if "~/test".starts_with("~/") {
            home.join("test").to_string_lossy().into_owned()
        } else {
            "~/test".to_string()
        };
        assert!(expanded.contains("test"));
        assert!(!expanded.starts_with("~"));
    }

    #[test]
    fn test_start_output_serialization() {
        let output = StartOutput {
            agent_id: "cam-123".to_string(),
            tmux_session: "cam-123".to_string(),
            agent_type: "claude".to_string(),
            project_path: "/tmp".to_string(),
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("cam-123"));
        assert!(json.contains("claude"));
    }
}
