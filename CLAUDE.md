# Code Agent Monitor

## Skills

Skills location: `~/clawd/skills/code-agent-monitor/SKILL.md`

## 快速参考

### 常用命令

```bash
# Agent 管理
cam list                          # 列出所有代理进程
cam sessions                      # 列出历史会话
cam resume <session_id>           # 恢复会话

# 通知调试
echo '{"cwd": "/tmp"}' | cam notify --event stop --agent-id test --dry-run
tail -f ~/.claude-monitor/hook.log

# Team 管理
cam team-create <name>            # 创建 Team
cam team-spawn <team> <name>      # 启动 Agent
cam team-progress <team>          # 查看进度
cam team-shutdown <team>          # 关闭 Team

# 快捷回复
cam pending-confirmations         # 查看待处理
cam reply y                       # 批准
```

### 构建和更新

```bash
cargo build --release
cp target/release/cam plugins/cam/bin/cam
openclaw gateway restart

# 重启 watcher（更新后必须）
kill $(cat ~/.claude-monitor/watcher.pid) 2>/dev/null
```

### 数据存储

| 路径 | 说明 |
|------|------|
| `~/.claude-monitor/agents.json` | 运行中的代理 |
| `~/.claude-monitor/watcher.pid` | Watcher PID |
| `~/.claude-monitor/hook.log` | Hook 日志 |
| `~/.claude-monitor/conversation_state.json` | 对话状态 |
| `~/.claude/teams/` | Agent Teams |
| `~/.claude/tasks/` | 任务列表 |

### 通知路由

| Urgency | 事件 | 行为 |
|---------|------|------|
| HIGH | permission_request, Error, WaitingForInput | 立即发送 |
| MEDIUM | AgentExited, idle_prompt | 发送 |
| LOW | session_start, stop | 静默 |

### 会话类型

| 类型 | 格式 | 通知 |
|------|------|------|
| CAM 管理 | `cam-xxxxxxxx` | 发送 |
| 外部会话 | `ext-xxxxxxxx` | 过滤 |

## 详细文档

- [开发指南](docs/development.md) - 项目结构、构建、扩展
- [调试指南](docs/debugging.md) - 问题排查、链路调试
- [测试指南](docs/testing.md) - 测试场景、端到端测试
- [Agent Teams Skill](skills/agent-teams/SKILL.md) - Team 编排详细用法
- [通知处理 Skill](skills/cam-notify/SKILL.md) - 通知类型和处理流程
- [E2E 测试 Skill](skills/cam-e2e-test/SKILL.md) - 端到端测试流程

## 开发原则

### 避免硬编码 AI 工具特定模式

CAM 需要兼容多种 AI 编码工具（Claude Code、Codex、OpenCode 等），**不要硬编码特定工具的模式**。

**错误示例**：
```rust
// ❌ 硬编码 Claude Code 特定状态
static PROCESSING_PATTERNS: &[&str] = &[
    "Hatching…",
    "Brewing…",
    "Thinking…",
];

// ❌ 硬编码终端清理模式
static NOISE_PATTERNS: &[&str] = &[
    r"(?m)^.*Brewing.*$",
    r"(?m)^.*Thinking.*$",
];
```

**正确做法**：使用 Haiku API 进行智能判断
```rust
// ✅ 使用 AI 判断 agent 状态
pub fn is_processing(content: &str) -> bool {
    use crate::anthropic::{is_agent_processing, AgentStatus};
    match is_agent_processing(content) {
        AgentStatus::Processing => true,
        AgentStatus::WaitingForInput => false,
        AgentStatus::Unknown => false,
    }
}

// ✅ 使用 AI 提取问题内容
pub fn extract_question_with_haiku(terminal_snapshot: &str) -> Option<(String, String, String)>
```

**关键模块**：
- `src/anthropic.rs` - Haiku API 客户端，包含：
  - `is_agent_processing()` - 判断 agent 是否在处理中
  - `extract_question_with_haiku()` - 提取问题和选项
  - `extract_notification_content()` - 提取通知内容
- `src/notification/terminal_cleaner.rs` - 只有 `is_processing()` 函数，调用 AI 判断
- `src/notification/formatter.rs` - 消息格式化，使用 AI 提取问题

**原则**：
1. 状态判断用 AI，不用正则
2. 内容提取用 AI，不用硬编码模式
3. 回退策略：AI 失败时显示原始内容的最后 N 行

