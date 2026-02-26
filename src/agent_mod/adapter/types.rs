// src/agent_mod/adapter/types.rs
//! 基础类型定义

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 检测策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionStrategy {
    /// 仅依赖 Hook 事件
    HookOnly,
    /// Hook 事件 + 轮询补充
    HookWithPolling,
    /// 仅轮询（无 Hook 支持）
    PollingOnly,
}

/// Agent 能力描述
#[derive(Debug, Clone)]
pub struct AgentCapabilities {
    /// 是否支持原生 hooks
    pub native_hooks: bool,
    /// 支持的 hook 事件列表
    pub hook_events: Vec<String>,
    /// 是否支持 MCP
    pub mcp_support: bool,
    /// 是否支持 JSON 输出
    pub json_output: bool,
}

/// Agent 配置路径
#[derive(Debug, Clone)]
pub struct AgentPaths {
    /// 配置文件路径
    pub config: Option<PathBuf>,
    /// 会话存储路径
    pub sessions: Option<PathBuf>,
    /// 日志路径
    pub logs: Option<PathBuf>,
}

/// 统一 Hook 事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookEvent {
    /// 会话开始
    SessionStart { session_id: String, cwd: String },
    /// 会话结束
    SessionEnd {
        session_id: Option<String>,
        cwd: String,
    },
    /// 等待用户输入
    WaitingForInput {
        context: String,
        is_decision: bool,
        cwd: String,
    },
    /// 权限请求
    PermissionRequest {
        tool: String,
        action: String,
        cwd: String,
    },
    /// 权限回复
    PermissionReplied { tool: String, approved: bool },
    /// 工具执行完成
    ToolExecuted {
        tool: String,
        success: bool,
        duration_ms: Option<u64>,
    },
    /// Turn 完成（Codex 特有）
    TurnComplete {
        thread_id: String,
        turn_id: String,
        cwd: String,
    },
    /// 错误
    Error { message: String, cwd: String },
    /// 自定义事件
    Custom {
        event_type: String,
        payload: serde_json::Value,
    },
}
