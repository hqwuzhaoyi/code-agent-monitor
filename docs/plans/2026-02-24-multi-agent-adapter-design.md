# Multi-Agent CLI Adapter 实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 CAM 添加多 Agent CLI 支持的抽象层，支持 Claude Code、Codex CLI、OpenCode 及未来新 CLI。

**Architecture:** 采用 trait-based adapter 模式，配置驱动优先，AI 检测作为通用后备。分三阶段渐进式迁移：抽象层引入 → 新适配器实现 → 配置和文档。

**Tech Stack:** Rust trait、TOML 配置、Haiku AI 状态检测

---

## 设计决策

### 1. 事件模型

统一 `HookEvent` 枚举，映射三个 CLI 的事件：

```rust
pub enum HookEvent {
    SessionStart { session_id: String, cwd: String },
    SessionEnd { session_id: Option<String>, cwd: String },
    WaitingForInput { context: String, is_decision: bool, cwd: String },
    PermissionRequest { tool: String, action: String, cwd: String },
    PermissionReplied { tool: String, approved: bool },  // 新增
    ToolExecuted { tool: String, success: bool },        // 新增
    Error { message: String, cwd: String },
    Custom { event_type: String, payload: Value },
}
```

### 2. 检测策略

```rust
pub enum DetectionStrategy {
    HookOnly,           // Claude Code, OpenCode (with Plugin)
    HookWithPolling,    // Codex CLI
    PollingOnly,        // 外部启动或无 hooks 支持
}
```

### 3. 配置管理

- 合并模式：保留用户现有配置，只添加 CAM 条目
- 时间戳备份：`~/.config/code-agent-monitor/backups/`
- 交互式确认 + `--yes` 静默模式

### 4. 扩展性

分层架构：
1. 内置适配器（Rust）- Claude, OpenCode, Codex
2. 配置驱动适配器（TOML）- 用户自定义 CLI
3. 脚本扩展（Shell）- 高级用户

---

## Phase 1: 抽象层引入

### Task 1: 定义 AgentAdapter trait

**Files:**
- Create: `src/agent_mod/adapter/mod.rs`
- Create: `src/agent_mod/adapter/types.rs`

**Step 1: 创建 types.rs 定义基础类型**

```rust
// src/agent_mod/adapter/types.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 检测策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionStrategy {
    HookOnly,
    HookWithPolling,
    PollingOnly,
}

/// Agent 能力描述
#[derive(Debug, Clone)]
pub struct AgentCapabilities {
    pub native_hooks: bool,
    pub hook_events: Vec<String>,
    pub mcp_support: bool,
    pub json_output: bool,
}

/// Agent 配置路径
#[derive(Debug, Clone)]
pub struct AgentPaths {
    pub config: Option<PathBuf>,
    pub sessions: Option<PathBuf>,
    pub logs: Option<PathBuf>,
}

/// 统一 Hook 事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookEvent {
    SessionStart { session_id: String, cwd: String },
    SessionEnd { session_id: Option<String>, cwd: String },
    WaitingForInput { context: String, is_decision: bool, cwd: String },
    PermissionRequest { tool: String, action: String, cwd: String },
    PermissionReplied { tool: String, approved: bool },
    ToolExecuted { tool: String, success: bool, duration_ms: Option<u64> },
    TurnComplete { thread_id: String, turn_id: String, cwd: String },
    Error { message: String, cwd: String },
    Custom { event_type: String, payload: serde_json::Value },
}
```

**Step 2: 运行测试确认编译通过**

Run: `cargo check`
Expected: 编译成功

**Step 3: 创建 mod.rs 定义 trait**

```rust
// src/agent_mod/adapter/mod.rs
mod types;

pub use types::*;

use crate::agent_mod::AgentType;
use anyhow::Result;
use std::path::PathBuf;

/// Agent CLI 适配器 trait
pub trait AgentAdapter: Send + Sync {
    /// 获取 Agent 类型
    fn agent_type(&self) -> AgentType;

    /// 获取启动命令
    fn get_command(&self) -> &str;

    /// 获取恢复会话命令
    fn get_resume_command(&self, session_id: &str) -> String;

    /// 获取检测策略
    fn detection_strategy(&self) -> DetectionStrategy;

    /// 获取能力描述
    fn capabilities(&self) -> AgentCapabilities;

    /// 获取配置路径
    fn paths(&self) -> AgentPaths;

    /// 检测是否已安装
    fn is_installed(&self) -> bool;

    /// 解析 hook 事件
    fn parse_hook_event(&self, payload: &str) -> Option<HookEvent>;

    /// 检测就绪状态
    fn detect_ready(&self, terminal_output: &str) -> bool;
}

/// 获取适配器
pub fn get_adapter(agent_type: &AgentType) -> Box<dyn AgentAdapter> {
    match agent_type {
        AgentType::Claude => Box::new(claude::ClaudeAdapter),
        AgentType::Codex => Box::new(codex::CodexAdapter),
        AgentType::OpenCode => Box::new(opencode::OpenCodeAdapter),
        _ => Box::new(generic::GenericAdapter::new(agent_type.clone())),
    }
}

mod claude;
mod codex;
mod opencode;
mod generic;
```

**Step 4: 运行测试**

Run: `cargo check`
Expected: 编译错误（子模块未创建）

**Step 5: Commit**

```bash
git add src/agent_mod/adapter/
git commit -m "$(cat <<'EOF'
feat(adapter): define AgentAdapter trait and types

- Add HookEvent enum for unified event model
- Add DetectionStrategy for hybrid detection
- Add AgentCapabilities and AgentPaths structs
EOF
)"
```

---

### Task 2: 实现 ClaudeAdapter

**Files:**
- Create: `src/agent_mod/adapter/claude.rs`
- Test: `src/agent_mod/adapter/claude.rs` (inline tests)

**Step 1: 创建 ClaudeAdapter 实现**

```rust
// src/agent_mod/adapter/claude.rs
use super::*;
use crate::agent_mod::AgentType;
use std::path::PathBuf;
use std::process::Command;

pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Claude
    }

    fn get_command(&self) -> &str {
        "claude"
    }

    fn get_resume_command(&self, session_id: &str) -> String {
        format!("claude --resume {}", session_id)
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        DetectionStrategy::HookOnly
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: true,
            hook_events: vec![
                "session_start".into(),
                "stop".into(),
                "notification".into(),
                "PreToolUse".into(),
                "PostToolUse".into(),
            ],
            mcp_support: true,
            json_output: false,
        }
    }

    fn paths(&self) -> AgentPaths {
        let home = dirs::home_dir().unwrap_or_default();
        AgentPaths {
            config: Some(home.join(".claude/settings.json")),
            sessions: Some(home.join(".claude/projects")),
            logs: None,
        }
    }

    fn is_installed(&self) -> bool {
        Command::new("which")
            .arg("claude")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn parse_hook_event(&self, payload: &str) -> Option<HookEvent> {
        let value: serde_json::Value = serde_json::from_str(payload).ok()?;
        let event_type = value.get("event")?.as_str()?;
        let cwd = value.get("cwd").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let session_id = value.get("session_id").and_then(|v| v.as_str()).map(String::from);

        match event_type {
            "session_start" => Some(HookEvent::SessionStart {
                session_id: session_id.unwrap_or_default(),
                cwd,
            }),
            "stop" => Some(HookEvent::SessionEnd { session_id, cwd }),
            "notification" => {
                let notification_type = value.get("notification_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match notification_type {
                    "idle_prompt" => Some(HookEvent::WaitingForInput {
                        context: "idle".into(),
                        is_decision: false,
                        cwd,
                    }),
                    _ => None,
                }
            }
            "PreToolUse" => {
                let tool = value.get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                Some(HookEvent::PermissionRequest {
                    tool,
                    action: "execute".into(),
                    cwd,
                })
            }
            _ => None,
        }
    }

    fn detect_ready(&self, terminal_output: &str) -> bool {
        let prompt_re = regex::Regex::new(r"(?m)^[❯>]\s*$").unwrap();
        prompt_re.is_match(terminal_output)
            || terminal_output.contains("Welcome to")
            || terminal_output.contains("Claude Code")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_command() {
        let adapter = ClaudeAdapter;
        assert_eq!(adapter.get_command(), "claude");
    }

    #[test]
    fn test_get_resume_command() {
        let adapter = ClaudeAdapter;
        assert_eq!(adapter.get_resume_command("abc123"), "claude --resume abc123");
    }

    #[test]
    fn test_detect_ready() {
        let adapter = ClaudeAdapter;
        assert!(adapter.detect_ready("Welcome to Claude Code\n❯"));
        assert!(adapter.detect_ready("Some output\n❯ "));
        assert!(!adapter.detect_ready("Loading..."));
    }

    #[test]
    fn test_parse_session_start() {
        let adapter = ClaudeAdapter;
        let payload = r#"{"event":"session_start","session_id":"abc","cwd":"/tmp"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::SessionStart { session_id, cwd } => {
                assert_eq!(session_id, "abc");
                assert_eq!(cwd, "/tmp");
            }
            _ => panic!("Expected SessionStart"),
        }
    }
}
```

**Step 2: 运行测试**

Run: `cargo test adapter::claude --lib`
Expected: 所有测试通过

**Step 3: Commit**

```bash
git add src/agent_mod/adapter/claude.rs
git commit -m "feat(adapter): implement ClaudeAdapter"
```

---

### Task 3: 实现 CodexAdapter

**Files:**
- Create: `src/agent_mod/adapter/codex.rs`

**Step 1: 创建 CodexAdapter 实现**

```rust
// src/agent_mod/adapter/codex.rs
use super::*;
use crate::agent_mod::AgentType;
use std::path::PathBuf;
use std::process::Command;

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Codex
    }

    fn get_command(&self) -> &str {
        "codex"
    }

    fn get_resume_command(&self, session_id: &str) -> String {
        format!("codex --resume {}", session_id)
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        // Codex 只有 turn-complete，需要轮询补充
        DetectionStrategy::HookWithPolling
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: true,
            hook_events: vec!["agent-turn-complete".into()],
            mcp_support: true,
            json_output: true,
        }
    }

    fn paths(&self) -> AgentPaths {
        let home = dirs::home_dir().unwrap_or_default();
        AgentPaths {
            config: Some(home.join(".codex/config.toml")),
            sessions: Some(home.join(".codex/sessions")),
            logs: None,
        }
    }

    fn is_installed(&self) -> bool {
        Command::new("which")
            .arg("codex")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn parse_hook_event(&self, payload: &str) -> Option<HookEvent> {
        // Codex notify payload 作为命令行参数传递
        let value: serde_json::Value = serde_json::from_str(payload).ok()?;
        let event_type = value.get("type")?.as_str()?;

        match event_type {
            "agent-turn-complete" => {
                let thread_id = value.get("thread-id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let turn_id = value.get("turn-id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let cwd = value.get("cwd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                Some(HookEvent::TurnComplete { thread_id, turn_id, cwd })
            }
            _ => None,
        }
    }

    fn detect_ready(&self, terminal_output: &str) -> bool {
        // Codex TUI 就绪检测
        terminal_output.contains("codex")
            || terminal_output.contains("Ready")
            || terminal_output.contains(">")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detection_strategy() {
        let adapter = CodexAdapter;
        assert_eq!(adapter.detection_strategy(), DetectionStrategy::HookWithPolling);
    }

    #[test]
    fn test_parse_turn_complete() {
        let adapter = CodexAdapter;
        let payload = r#"{
            "type": "agent-turn-complete",
            "thread-id": "019c8eda-8d98-7ca3-bdd6-8bdbb1a80f1f",
            "turn-id": "019c8eda-955d-7853-84a0-4ed91b90014d",
            "cwd": "/tmp/project"
        }"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::TurnComplete { thread_id, turn_id, cwd } => {
                assert!(thread_id.starts_with("019c8eda"));
                assert!(turn_id.starts_with("019c8eda"));
                assert_eq!(cwd, "/tmp/project");
            }
            _ => panic!("Expected TurnComplete"),
        }
    }
}
```

**Step 2: 运行测试**

Run: `cargo test adapter::codex --lib`
Expected: 所有测试通过

**Step 3: Commit**

```bash
git add src/agent_mod/adapter/codex.rs
git commit -m "feat(adapter): implement CodexAdapter with HookWithPolling strategy"
```

---

### Task 4: 实现 OpenCodeAdapter

**Files:**
- Create: `src/agent_mod/adapter/opencode.rs`

**Step 1: 创建 OpenCodeAdapter 实现**

```rust
// src/agent_mod/adapter/opencode.rs
use super::*;
use crate::agent_mod::AgentType;
use std::path::PathBuf;
use std::process::Command;

pub struct OpenCodeAdapter;

impl AgentAdapter for OpenCodeAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::OpenCode
    }

    fn get_command(&self) -> &str {
        "opencode"
    }

    fn get_resume_command(&self, session_id: &str) -> String {
        format!("opencode --session {}", session_id)
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        // OpenCode Plugin 系统完整，可以纯 Hook
        DetectionStrategy::HookOnly
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: true,
            hook_events: vec![
                "session.created".into(),
                "session.idle".into(),
                "session.error".into(),
                "permission.asked".into(),
                "permission.replied".into(),
                "tool.execute.before".into(),
                "tool.execute.after".into(),
            ],
            mcp_support: true,
            json_output: false,
        }
    }

    fn paths(&self) -> AgentPaths {
        let home = dirs::home_dir().unwrap_or_default();
        AgentPaths {
            config: Some(home.join(".config/opencode/opencode.json")),
            sessions: Some(home.join(".config/opencode/sessions")),
            logs: None,
        }
    }

    fn is_installed(&self) -> bool {
        Command::new("which")
            .arg("opencode")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn parse_hook_event(&self, payload: &str) -> Option<HookEvent> {
        let value: serde_json::Value = serde_json::from_str(payload).ok()?;
        let event_type = value.get("type")?.as_str()?;
        let cwd = value.get("cwd").and_then(|v| v.as_str()).unwrap_or("").to_string();

        match event_type {
            "session.created" => Some(HookEvent::SessionStart {
                session_id: value.get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                cwd,
            }),
            "session.idle" => Some(HookEvent::WaitingForInput {
                context: "idle".into(),
                is_decision: false,
                cwd,
            }),
            "session.error" => Some(HookEvent::Error {
                message: value.get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error")
                    .to_string(),
                cwd,
            }),
            "permission.asked" => Some(HookEvent::PermissionRequest {
                tool: value.get("tool").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                action: value.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                cwd,
            }),
            "permission.replied" => Some(HookEvent::PermissionReplied {
                tool: value.get("tool").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                approved: value.get("approved").and_then(|v| v.as_bool()).unwrap_or(false),
            }),
            _ => None,
        }
    }

    fn detect_ready(&self, terminal_output: &str) -> bool {
        terminal_output.contains("opencode")
            || terminal_output.contains("Ready")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities() {
        let adapter = OpenCodeAdapter;
        let caps = adapter.capabilities();
        assert!(caps.native_hooks);
        assert!(caps.hook_events.contains(&"session.idle".to_string()));
        assert!(caps.hook_events.contains(&"permission.asked".to_string()));
    }

    #[test]
    fn test_parse_permission_asked() {
        let adapter = OpenCodeAdapter;
        let payload = r#"{"type":"permission.asked","tool":"Bash","action":"rm -rf","cwd":"/tmp"}"#;
        let event = adapter.parse_hook_event(payload).unwrap();
        match event {
            HookEvent::PermissionRequest { tool, action, cwd } => {
                assert_eq!(tool, "Bash");
                assert_eq!(action, "rm -rf");
                assert_eq!(cwd, "/tmp");
            }
            _ => panic!("Expected PermissionRequest"),
        }
    }
}
```

**Step 2: 运行测试**

Run: `cargo test adapter::opencode --lib`
Expected: 所有测试通过

**Step 3: Commit**

```bash
git add src/agent_mod/adapter/opencode.rs
git commit -m "feat(adapter): implement OpenCodeAdapter with full Plugin events"
```

---

### Task 5: 实现 GenericAdapter

**Files:**
- Create: `src/agent_mod/adapter/generic.rs`

**Step 1: 创建 GenericAdapter 实现**

```rust
// src/agent_mod/adapter/generic.rs
use super::*;
use crate::agent_mod::AgentType;
use std::process::Command;

/// 通用适配器，用于未知或自定义 CLI
pub struct GenericAdapter {
    agent_type: AgentType,
    command: String,
}

impl GenericAdapter {
    pub fn new(agent_type: AgentType) -> Self {
        let command = match &agent_type {
            AgentType::GeminiCli => "gemini".to_string(),
            AgentType::MistralVibe => "vibe".to_string(),
            _ => "echo".to_string(),
        };
        Self { agent_type, command }
    }

    pub fn with_command(agent_type: AgentType, command: String) -> Self {
        Self { agent_type, command }
    }
}

impl AgentAdapter for GenericAdapter {
    fn agent_type(&self) -> AgentType {
        self.agent_type.clone()
    }

    fn get_command(&self) -> &str {
        &self.command
    }

    fn get_resume_command(&self, _session_id: &str) -> String {
        // 通用适配器不支持恢复
        self.command.clone()
    }

    fn detection_strategy(&self) -> DetectionStrategy {
        // 通用适配器只能用轮询
        DetectionStrategy::PollingOnly
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities {
            native_hooks: false,
            hook_events: vec![],
            mcp_support: false,
            json_output: false,
        }
    }

    fn paths(&self) -> AgentPaths {
        AgentPaths {
            config: None,
            sessions: None,
            logs: None,
        }
    }

    fn is_installed(&self) -> bool {
        Command::new("which")
            .arg(&self.command)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn parse_hook_event(&self, _payload: &str) -> Option<HookEvent> {
        // 通用适配器不解析 hook 事件
        None
    }

    fn detect_ready(&self, terminal_output: &str) -> bool {
        // 使用 AI 检测作为后备
        !terminal_output.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generic_detection_strategy() {
        let adapter = GenericAdapter::new(AgentType::Unknown);
        assert_eq!(adapter.detection_strategy(), DetectionStrategy::PollingOnly);
    }

    #[test]
    fn test_generic_no_hooks() {
        let adapter = GenericAdapter::new(AgentType::Unknown);
        assert!(!adapter.capabilities().native_hooks);
        assert!(adapter.parse_hook_event("{}").is_none());
    }
}
```

**Step 2: 运行测试**

Run: `cargo test adapter::generic --lib`
Expected: 所有测试通过

**Step 3: Commit**

```bash
git add src/agent_mod/adapter/generic.rs
git commit -m "feat(adapter): implement GenericAdapter for unknown CLIs"
```

---

### Task 6: 更新模块导出

**Files:**
- Modify: `src/agent_mod/mod.rs`

**Step 1: 添加 adapter 模块导出**

在 `src/agent_mod/mod.rs` 中添加：

```rust
pub mod adapter;

pub use adapter::{
    AgentAdapter, AgentCapabilities, AgentPaths, DetectionStrategy, HookEvent, get_adapter,
};
```

**Step 2: 运行完整测试**

Run: `cargo test --lib`
Expected: 所有测试通过

**Step 3: Commit**

```bash
git add src/agent_mod/mod.rs
git commit -m "feat(adapter): export adapter module from agent_mod"
```

---

## Phase 2: 集成到现有系统

### Task 7: 修改 AgentManager 使用 Adapter

**Files:**
- Modify: `src/agent_mod/manager.rs`

**Step 1: 添加 adapter 导入和使用**

在 `AgentManager::start_agent` 中使用 adapter：

```rust
// 在 start_agent 方法中
let adapter = get_adapter(&agent_type);
let command = if let Some(ref session_id) = request.resume_session {
    adapter.get_resume_command(session_id)
} else {
    adapter.get_command().to_string()
};
```

**Step 2: 替换 get_agent_command 方法**

将现有的 `get_agent_command` 方法标记为 deprecated，内部调用 adapter：

```rust
#[deprecated(note = "Use get_adapter().get_command() instead")]
fn get_agent_command(&self, agent_type: &AgentType, resume_session: Option<&str>) -> String {
    let adapter = get_adapter(agent_type);
    if let Some(session_id) = resume_session {
        adapter.get_resume_command(session_id)
    } else {
        adapter.get_command().to_string()
    }
}
```

**Step 3: 运行测试验证向后兼容**

Run: `cargo test agent_mod::manager --lib`
Expected: 所有现有测试通过

**Step 4: Commit**

```bash
git add src/agent_mod/manager.rs
git commit -m "refactor(manager): use AgentAdapter for command generation"
```

---

### Task 8: 修改 Watcher 使用检测策略

**Files:**
- Modify: `src/agent_mod/watcher.rs`

**Step 1: 根据 DetectionStrategy 调整轮询行为**

```rust
use crate::agent_mod::adapter::{get_adapter, DetectionStrategy};

impl AgentWatcher {
    fn should_poll(&self, agent: &AgentRecord) -> bool {
        let adapter = get_adapter(&agent.agent_type);
        match adapter.detection_strategy() {
            DetectionStrategy::HookOnly => {
                // 只在 hook 失效时轮询
                self.hook_seems_inactive(agent)
            }
            DetectionStrategy::HookWithPolling => {
                // 总是轮询
                true
            }
            DetectionStrategy::PollingOnly => {
                // 总是轮询
                true
            }
        }
    }

    fn hook_seems_inactive(&self, agent: &AgentRecord) -> bool {
        // 检查最近是否收到过 hook 事件
        let last_hook = self.last_hook_time.get(&agent.agent_id);
        match last_hook {
            Some(time) => time.elapsed().as_secs() > 300, // 5 分钟无 hook
            None => true,
        }
    }
}
```

**Step 2: 运行测试**

Run: `cargo test agent_mod::watcher --lib`
Expected: 所有测试通过

**Step 3: Commit**

```bash
git add src/agent_mod/watcher.rs
git commit -m "refactor(watcher): use DetectionStrategy for polling decisions"
```

---

### Task 9: 添加 codex-notify CLI 命令

**Files:**
- Modify: `src/cli/mod.rs`
- Create: `src/cli/codex_notify.rs`

**Step 1: 创建 codex-notify 命令处理**

```rust
// src/cli/codex_notify.rs
use crate::agent_mod::adapter::{get_adapter, HookEvent};
use crate::agent_mod::AgentType;
use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct CodexNotifyArgs {
    /// JSON payload from Codex notify
    pub payload: String,
}

pub async fn handle_codex_notify(args: CodexNotifyArgs) -> Result<()> {
    let adapter = get_adapter(&AgentType::Codex);

    // 解析事件
    let event = adapter.parse_hook_event(&args.payload)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse Codex payload"))?;

    // 记录事件
    tracing::info!(?event, "Received Codex notify event");

    // 根据事件类型处理
    match event {
        HookEvent::TurnComplete { thread_id, cwd, .. } => {
            // 查找对应的 agent
            let manager = crate::agent_mod::AgentManager::new()?;
            if let Some(agent) = manager.find_agent_by_cwd(&cwd) {
                // 触发状态检测
                let watcher = crate::agent_mod::AgentWatcher::new()?;
                watcher.trigger_check(&agent.agent_id).await?;
            }
        }
        _ => {}
    }

    Ok(())
}
```

**Step 2: 注册命令到 CLI**

在 `src/cli/mod.rs` 中添加：

```rust
#[derive(Subcommand)]
pub enum Commands {
    // ... existing commands

    /// Handle Codex CLI notify events
    CodexNotify(codex_notify::CodexNotifyArgs),
}

// 在 match 中添加
Commands::CodexNotify(args) => codex_notify::handle_codex_notify(args).await,
```

**Step 3: 运行测试**

Run: `cargo build`
Expected: 编译成功

**Step 4: Commit**

```bash
git add src/cli/
git commit -m "feat(cli): add codex-notify command for Codex integration"
```

---

## Phase 3: 配置管理

### Task 10: 实现配置备份管理器

**Files:**
- Create: `src/agent_mod/adapter/config_manager.rs`

**Step 1: 创建 BackupManager**

```rust
// src/agent_mod/adapter/config_manager.rs
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use chrono::Local;

pub struct BackupManager {
    backup_dir: PathBuf,
    max_backups: usize,
}

impl BackupManager {
    pub fn new() -> Self {
        let backup_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("code-agent-monitor/backups");
        Self {
            backup_dir,
            max_backups: 5,
        }
    }

    /// 创建备份
    pub fn backup(&self, tool: &str, original_path: &Path) -> Result<PathBuf> {
        if !original_path.exists() {
            return Ok(original_path.to_path_buf());
        }

        let timestamp = Local::now().format("%Y-%m-%dT%H-%M-%S");
        let filename = original_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config");
        let backup_path = self.backup_dir
            .join(tool)
            .join(format!("{}.{}.bak", filename, timestamp));

        fs::create_dir_all(backup_path.parent().unwrap())?;
        fs::copy(original_path, &backup_path)?;

        self.cleanup_old_backups(tool)?;
        Ok(backup_path)
    }

    /// 回滚到最近备份
    pub fn rollback(&self, tool: &str, target_path: &Path) -> Result<()> {
        let latest = self.get_latest_backup(tool, target_path)?;
        fs::copy(&latest, target_path)?;
        Ok(())
    }

    fn get_latest_backup(&self, tool: &str, target_path: &Path) -> Result<PathBuf> {
        let filename = target_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config");
        let tool_dir = self.backup_dir.join(tool);

        let mut backups: Vec<_> = fs::read_dir(&tool_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(filename))
            .collect();

        backups.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        backups.last()
            .map(|e| e.path())
            .ok_or_else(|| anyhow::anyhow!("No backup found"))
    }

    fn cleanup_old_backups(&self, tool: &str) -> Result<()> {
        let tool_dir = self.backup_dir.join(tool);
        if !tool_dir.exists() {
            return Ok(());
        }

        let mut backups: Vec<_> = fs::read_dir(&tool_dir)?
            .filter_map(|e| e.ok())
            .collect();

        backups.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());

        while backups.len() > self.max_backups {
            if let Some(oldest) = backups.first() {
                fs::remove_file(oldest.path())?;
                backups.remove(0);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_backup_and_rollback() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "original content").unwrap();

        let mut manager = BackupManager::new();
        manager.backup_dir = temp.path().join("backups");

        // 创建备份
        let backup_path = manager.backup("test", &config_path).unwrap();
        assert!(backup_path.exists());

        // 修改原文件
        fs::write(&config_path, "modified content").unwrap();

        // 回滚
        manager.rollback("test", &config_path).unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "original content");
    }
}
```

**Step 2: 运行测试**

Run: `cargo test adapter::config_manager --lib`
Expected: 所有测试通过

**Step 3: Commit**

```bash
git add src/agent_mod/adapter/config_manager.rs
git commit -m "feat(adapter): add BackupManager for config file safety"
```

---

### Task 11: 添加 setup 命令

**Files:**
- Create: `src/cli/setup.rs`
- Modify: `src/cli/mod.rs`

**Step 1: 创建 setup 命令**

```rust
// src/cli/setup.rs
use crate::agent_mod::adapter::{config_manager::BackupManager, get_adapter};
use crate::agent_mod::AgentType;
use anyhow::Result;
use clap::Args;
use std::fs;
use std::io::{self, Write};

#[derive(Args)]
pub struct SetupArgs {
    /// Target tool: claude, codex, opencode
    pub tool: String,

    /// Skip confirmation prompt
    #[arg(short, long)]
    pub yes: bool,

    /// Show changes without applying
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn handle_setup(args: SetupArgs) -> Result<()> {
    let agent_type = AgentType::from_str(&args.tool)?;
    let adapter = get_adapter(&agent_type);

    let config_path = adapter.paths().config
        .ok_or_else(|| anyhow::anyhow!("No config path for {}", args.tool))?;

    println!("Setting up CAM hooks for {}", args.tool);
    println!("Config file: {}", config_path.display());

    // 生成新配置
    let new_config = generate_hook_config(&args.tool)?;

    if args.dry_run {
        println!("\n--- Changes to apply ---");
        println!("{}", new_config);
        return Ok(());
    }

    // 确认
    if !args.yes {
        print!("\nApply changes? [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    // 备份
    let backup_manager = BackupManager::new();
    if config_path.exists() {
        let backup_path = backup_manager.backup(&args.tool, &config_path)?;
        println!("✓ Backed up to {}", backup_path.display());
    }

    // 应用配置
    apply_hook_config(&args.tool, &config_path, &new_config)?;
    println!("✓ Updated {}", config_path.display());

    Ok(())
}

fn generate_hook_config(tool: &str) -> Result<String> {
    match tool {
        "codex" => Ok(r#"notify = ["cam", "codex-notify"]"#.to_string()),
        "claude" => Ok(r#"{
  "hooks": {
    "Stop": [{"matcher": ".*", "hooks": ["cam notify --event stop"]}],
    "notification": [{"matcher": ".*", "hooks": ["cam notify --event notification"]}]
  }
}"#.to_string()),
        _ => Err(anyhow::anyhow!("Unsupported tool: {}", tool)),
    }
}

fn apply_hook_config(tool: &str, config_path: &Path, new_config: &str) -> Result<()> {
    match tool {
        "codex" => {
            // 追加到 TOML
            let mut content = if config_path.exists() {
                fs::read_to_string(config_path)?
            } else {
                String::new()
            };
            if !content.contains("notify") {
                content.push_str("\n");
                content.push_str(new_config);
            }
            fs::write(config_path, content)?;
        }
        "claude" => {
            // 合并 JSON
            // TODO: 实现 JSON 合并逻辑
            fs::write(config_path, new_config)?;
        }
        _ => {}
    }
    Ok(())
}
```

**Step 2: 注册命令**

**Step 3: 运行测试**

Run: `cargo build`
Expected: 编译成功

**Step 4: Commit**

```bash
git add src/cli/setup.rs src/cli/mod.rs
git commit -m "feat(cli): add setup command for hook configuration"
```

---

## 验收标准

### Phase 1 完成标准
- [ ] `AgentAdapter` trait 定义完成
- [ ] `ClaudeAdapter` 实现并通过测试
- [ ] `CodexAdapter` 实现并通过测试
- [ ] `OpenCodeAdapter` 实现并通过测试
- [ ] `GenericAdapter` 实现并通过测试
- [ ] 所有现有测试继续通过

### Phase 2 完成标准
- [ ] `AgentManager` 使用 adapter 获取命令
- [ ] `AgentWatcher` 根据 `DetectionStrategy` 调整行为
- [ ] `cam codex-notify` 命令可用
- [ ] Codex turn-complete 事件能触发状态检测

### Phase 3 完成标准
- [ ] `cam setup codex` 能自动配置 notify
- [ ] `cam setup claude` 能自动配置 hooks
- [ ] 配置修改前自动备份
- [ ] `cam remove <tool>` 能清理 CAM 配置

---

Plan complete and saved to `docs/plans/2026-02-24-multi-agent-adapter-design.md`. Two execution options:

**1. Subagent-Driven (this session)** - I dispatch fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** - Open new session with executing-plans, batch execution with checkpoints

Which approach?
