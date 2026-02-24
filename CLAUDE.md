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
tail -f ~/.config/code-agent-monitor/hook.log

# Team 管理
cam team-create <name>            # 创建 Team
cam team-spawn <team> <name>      # 启动 Agent
cam team-progress <team>          # 查看进度
cam team-shutdown <team>          # 关闭 Team

# 快捷回复
cam pending-confirmations         # 查看待处理
cam reply y                       # 批准

# 手动触发检测
cam watch-trigger --agent-id <id>           # 触发检测并发送通知
cam watch-trigger --agent-id <id> --force   # 强制发送（绕过 AI 检测）
cam watch-trigger --agent-id <id> --no-dedup # 跳过去重
```

### 构建和更新

```bash
cargo build --release
cp target/release/cam plugins/cam/bin/cam
openclaw gateway restart

# 重启 watcher（更新后必须）
kill $(cat ~/.config/code-agent-monitor/watcher.pid) 2>/dev/null
```

### 数据存储

| 路径 | 说明 |
|------|------|
| `~/.config/code-agent-monitor/agents.json` | 运行中的代理 |
| `~/.config/code-agent-monitor/watcher.pid` | Watcher PID |
| `~/.config/code-agent-monitor/hook.log` | Hook 日志 |
| `~/.config/code-agent-monitor/conversation_state.json` | 对话状态 |
| `~/.config/code-agent-monitor/dedup_state.json` | 通知去重状态 |
| `~/.config/code-agent-monitor/config.json` | Webhook 和 Haiku API 配置 |
| `~/.claude/teams/` | Agent Teams |
| `~/.claude/tasks/` | 任务列表 |

### 通知路由

所有通知统一通过 Webhook 发送到 OpenClaw Gateway（`POST /hooks/agent`），触发 OpenClaw 对话。用户可以直接在对话中回复，CAM skill 会通过 `cam reply` 处理。

| Urgency | 事件 | 行为 |
|---------|------|------|
| HIGH | permission_request, Error, WaitingForInput | 立即发送，需要用户回复 |
| MEDIUM | AgentExited, idle_prompt | 发送通知，可能需要用户操作 |
| LOW | session_start, stop, ToolUse | 静默（不发送通知） |

#### 自动审批（OpenClaw Skill 实现）

OpenClaw 使用三层决策模型自动处理低风险操作：

1. **白名单** - 安全命令自动批准：`ls`, `cat`, `git status`, `cargo test`, `npm test`
2. **黑名单** - 必须人工确认：`rm`, `sudo`, 包含 `&&`, `|`, `>` 的命令
3. **LLM 判断** - AI 分析不在名单中的命令风险

**参数安全检查**：即使白名单命令，如果参数包含敏感路径（`/etc/`, `~/.ssh/`, `.env`），仍需人工确认。

详见 [自动审批设计](docs/plans/2026-02-24-auto-approve-design.md)。

**回复链路**：
```
CAM → POST /hooks/agent → Gateway → OpenClaw 对话
                                        ↓
                              用户回复 "y"
                                        ↓
                              CAM skill → cam reply → tmux send-keys
```

**配置要求**：需要在 `~/.config/code-agent-monitor/config.json` 中配置 webhook：
```json
{
  "webhook": {
    "gateway_url": "http://localhost:18789",
    "hook_token": "your-token",
    "timeout_secs": 30
  }
}
```

### 会话类型

| 类型 | 格式 | 通知 |
|------|------|------|
| CAM 管理 | `cam-xxxxxxxx` | 发送 |
| 外部会话 | `ext-xxxxxxxx` | 过滤 |

## 详细文档

- [开发指南](docs/development.md) - 项目结构、构建、扩展
- [调试指南](docs/debugging.md) - 问题排查、链路调试
- [测试指南](docs/testing.md) - 测试场景、端到端测试
- [自动审批设计](docs/plans/2026-02-24-auto-approve-design.md) - 三层决策模型、白名单/黑名单规则
- [Agent Teams Skill](skills/agent-teams/SKILL.md) - Team 编排详细用法
- [通知处理 Skill](skills/cam-notify/SKILL.md) - 通知类型、自动审批规则、回复路由
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
3. 回退策略：AI 失败时显示"无法解析通知内容，请查看终端"
4. 上下文完整性：AI 判断上下文是否完整，不完整时自动扩展

### 上下文完整性检测

通知内容提取时，AI 会判断终端快照是否包含完整的上下文。如果问题引用了未显示的内容（如"这个项目结构看起来合适吗？"但结构未显示），AI 会返回 `context_complete: false`，系统会自动扩展上下文重试。

**扩展策略**：80 行 → 150 行 → 300 行

**AI 提示词包含**：
```json
{
  "context_complete": true/false,  // 上下文是否完整
  "contains_ui_noise": true/false  // 是否包含 UI 噪音（加载动画等）
}
```

**相关代码**：`src/anthropic.rs` - `extract_question_with_haiku()`

### Haiku API 配置

CAM 使用 Claude Haiku 4.5 进行终端状态判断和问题提取。API 配置按以下优先级读取：

1. **`~/.config/code-agent-monitor/config.json`**（推荐）- JSON 格式
2. 环境变量 `ANTHROPIC_API_KEY` / `ANTHROPIC_BASE_URL`
3. `~/.anthropic/api_key`
4. `~/.openclaw/openclaw.json`

**配置示例** (`~/.config/code-agent-monitor/config.json`):
```json
{
  "anthropic_api_key": "sk-xxx",
  "anthropic_base_url": "http://localhost:23000/"
}
```

**模型**: `claude-haiku-4-5-20251001`

### tmux send-keys 必须使用 -l 标志

向 tmux 发送输入时，**必须使用 `-l` 标志**确保文本被字面解释，否则某些字符可能被解释为特殊按键。

**错误示例**：
```rust
// ❌ 没有 -l 标志，"1" 可能被解释为特殊按键
Command::new("tmux")
    .args(["send-keys", "-t", session, input])
    .status()?;
```

**正确做法**：
```rust
// ✅ 使用 -l 标志发送字面文本
Command::new("tmux")
    .args(["send-keys", "-t", session, "-l", input])
    .status()?;

// ✅ Enter 单独发送（不使用 -l，因为需要解释为按键）
Command::new("tmux")
    .args(["send-keys", "-t", session, "Enter"])
    .status()?;
```

**相关文件**：
- `src/tmux.rs` - `send_keys()` 和 `send_keys_raw()`
- `src/session.rs` - `send_to_tmux()`
- `src/conversation_state.rs` - `send_to_tmux()`

