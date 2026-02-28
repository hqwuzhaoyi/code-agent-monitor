# Code Agent Monitor

## Skills

Skills location: `~/clawd/skills/code-agent-monitor/SKILL.md`

## 快速参考

### 常用命令

```bash
# Agent 启动
cam start                         # 启动 Claude Code（当前目录）
cam start --agent codex           # 启动 Codex
cam start --cwd /path/to/project  # 指定工作目录
cam start "实现 TODO 应用"         # 带初始 prompt
cam start --resume <session_id>   # 恢复会话

# 初始化配置
cam bootstrap                     # 交互式配置向导
cam bootstrap --auto              # 自动检测，跳过提示

# Agent 管理
cam list                          # 列出所有代理进程
cam sessions                      # 列出历史会话
cam resume <session_id>           # 恢复会话（attach tmux）

# 通知调试
echo '{"cwd": "/tmp"}' | cam notify --event stop --agent-id test --dry-run
tail -f ~/.config/code-agent-monitor/hook.log

# Team 管理
cam team-create <name>            # 创建 Team
cam team-spawn <team> <name>      # 启动 Agent
cam team-progress <team>          # 查看进度
cam team-shutdown <team>          # 关闭 Team

# 状态汇总
cam summary --dry-run             # 预览汇总（不发送）
cam summary --always              # 强制发送（无论是否有异常）
cam summary                       # 有异常时发送（cron 用）

# OpenClaw 定时汇报（工作时间每30分钟）
openclaw cron add --name "cam-periodic-summary" \
  --cron "*/30 9-18 * * 1-5" --tz "Asia/Shanghai" \
  --session isolated \
  --message "调用 cam_summary 工具获取 agent 状态汇总，将结果直接发给我。" \
  --announce --channel telegram

# 快捷回复
cam pending-confirmations         # 查看待处理
cam reply y                       # 批准
cam reply y --all                 # 批准所有待处理
cam reply y --agent "cam-*"       # 批准匹配的 agent
cam reply y --risk low            # 批准所有低风险请求

# 手动触发检测（调试用，不影响 watcher 自动检测）
cam watch-trigger --agent-id <id>           # 触发检测并发送通知
cam watch-trigger --agent-id <id> --force   # 强制发送（绕过 AI 检测，自动跳过去重）
cam watch-trigger --agent-id <id> --no-dedup # 显式跳过去重

# 服务管理
cam install                       # 安装 watcher 为系统服务
cam install --force               # 强制重新安装
cam uninstall                     # 卸载服务
cam service status                # 查看服务状态
cam service restart               # 重启服务（开发后使用）
cam service logs                  # 查看服务日志
cam service logs -f               # 跟踪日志

# 通知问题排查（按顺序检查，不要直接手动触发）
cam service status                # 1. 确认 watcher 服务运行中
cam service logs 2>&1 | tail -50  # 2. 查看最近日志，确认是否检测到等待状态
cat ~/.config/code-agent-monitor/dedup_state.json  # 3. 检查去重状态，是否被 lock
tail -20 ~/.config/code-agent-monitor/hook.log     # 4. 检查 webhook 发送记录
tail -50 ~/.openclaw/logs/gateway.log              # 5. 检查 OpenClaw Gateway 是否收到请求
# 只有确认以上都正常但仍有问题时，才使用 watch-trigger 手动触发调试
```

### 构建和更新

```bash
cargo build --release
cp target/release/cam plugins/cam/bin/cam
openclaw gateway restart

# 重启 watcher（更新后必须）
kill $(cat ~/.config/code-agent-monitor/watcher.pid) 2>/dev/null

# 开发后更新服务
cargo build --release
cp target/release/cam plugins/cam/bin/cam
cam service restart               # 重启服务加载新二进制
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
| `~/.config/code-agent-monitor/notifications.jsonl` | TUI 本地通知记录 |
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
- [E2E 测试报告](findings/e2e-test-report.md) - 通知链路测试结果

## 已知问题

| 问题 | 优先级 | 状态 |
|------|--------|------|
| 事件名称大小写不一致 | P1 | 待修复 |
| ~~Skill 文档字段与实际输出不匹配~~ | ~~P3~~ | 已修复 |

详见 [E2E 测试报告](findings/e2e-test-report.md)。

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
- `src/agent_mod/extractor/` - ReAct 消息提取器（推荐使用）
  - `mod.rs` - ReAct 循环逻辑，`ReactExtractor` 和 `HaikuExtractor`
  - `traits.rs` - `MessageExtractor` trait、`ExtractedMessage`、`ExtractionResult`
  - `prompts.rs` - AI 提示词模板
- `src/ai/client.rs` - Anthropic API 客户端
- `src/ai/extractor.rs` - 旧版提取器（兼容保留）

**原则**：
1. 状态判断用 AI，不用正则
2. 内容提取用 AI，不用硬编码模式
3. 回退策略：AI 失败时显示"无法解析通知内容，请查看终端"
4. 上下文完整性：AI 判断上下文是否完整，不完整时自动扩展

### 上下文完整性检测

通知内容提取时，AI 会判断终端快照是否包含完整的上下文。如果问题引用了未显示的内容（如"这个项目结构看起来合适吗？"但结构未显示），AI 会返回 `context_complete: false`，系统会自动扩展上下文重试。

**扩展策略**：80 行 → 150 行 → 300 行 → 500 行 → 800 行

**AI 提示词包含**：
```json
{
  "context_complete": true/false,  // 上下文是否完整
  "contains_ui_noise": true/false  // 是否包含 UI 噪音（加载动画等）
}
```

**相关代码**：`src/agent_mod/extractor/` - ReAct 消息提取器

### ReAct 消息提取器

ReAct (Reasoning + Acting) 提取器是 CAM 的核心组件，负责从终端快照中提取 Agent 问题。

**工作原理**：
1. 一次性获取最大行数（800 行）的终端快照
2. 从 80 行开始调用 AI 分析
3. 如果 AI 返回 `context_complete: false`，扩展上下文重试
4. 迭代直到成功或达到最大行数

**核心类型**：
```rust
// 提取结果
enum ExtractionResult {
    Success(ExtractedMessage),  // 成功提取
    NeedMoreContext,            // 需要更多上下文
    Processing,                 // Agent 正在处理
    Failed(String),             // 提取失败
}

// 消息类型
enum MessageType {
    Choice,        // 选择题
    Confirmation,  // 确认题 (y/n)
    OpenEnded,     // 开放式问题
    Idle { .. },   // Agent 空闲
}
```

**使用方式**：
```rust
let extractor = HaikuExtractor::new()?;
let react = ReactExtractor::new(Box::new(extractor));
let message = react.extract_message(session_id, &tmux)?;
```

**设计文档**：
- [架构设计](design/react-extractor.md)
- [AI Prompt 设计](design/ai-prompts.md)

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

### API 变更时同步 Skills 和 Plugin

修改 MCP Server 工具（`src/mcp_mod/server.rs`）或 CLI 命令时，**必须同步更新**：

1. **OpenClaw Plugin** (`plugins/cam/src/index.ts`) — 工具包装层，所有工具使用 `cam_` 前缀
2. **Skills 文档** — 根据变更影响范围更新对应 skill：
   - `skills/cam/SKILL.md` — Agent 管理、会话管理、进程管理工具
   - `skills/agent-teams/SKILL.md` — Team 编排、任务管理、Inbox 工具
   - `skills/cam-notify/SKILL.md` — 通知处理、自动审批、回复路由

**检查清单**：
- [ ] 新增/删除 MCP 工具 → Plugin 添加/移除对应 `cam_` 包装
- [ ] 工具参数变更 → Plugin 参数 + Skill 文档工具表同步更新
- [ ] 新增 CLI 命令 → 评估是否需要 MCP 工具 + Skill 覆盖
- [ ] 通知事件类型变更 → cam-notify SKILL.md 更新

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

