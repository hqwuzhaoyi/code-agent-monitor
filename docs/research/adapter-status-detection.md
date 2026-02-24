# Adapter 状态检测策略分析

## 概述

本文档分析 CAM 多 Agent 支持中 native hooks 和终端轮询的协作策略，为 adapter 模块设计提供依据。

## 各 CLI 的 Native Hooks 能力对比

### Claude Code

| 事件 | 支持 | 用途 |
|------|------|------|
| `session_start` | ✅ | 会话开始，获取 session_id |
| `stop` | ✅ | Agent turn 结束 |
| `notification` | ✅ | 系统通知 |
| `PreToolUse` | ✅ | 工具调用前（可拦截） |
| `PostToolUse` | ✅ | 工具调用后 |
| `idle_prompt` | ✅ | 等待用户输入 |

**能力评估**: 完整的生命周期覆盖，可以检测所有关键状态。

### Codex CLI

| 事件 | 支持 | 用途 |
|------|------|------|
| `agent-turn-complete` | ✅ | Turn 完成时触发 |
| `session_start` | ❌ | 无 |
| `permission_request` | ❌ | 无 |
| `waiting_for_input` | ❌ | 无 |

**能力评估**: 只有一个事件类型，无法检测等待输入状态。

**Payload 格式**:
```json
{
  "type": "agent-turn-complete",
  "thread-id": "019c8eda-8d98-7ca3-bdd6-8bdbb1a80f1f",
  "turn-id": "019c8eda-955d-7853-84a0-4ed91b90014d",
  "cwd": "/path/to/project",
  "input-messages": ["user message"],
  "last-assistant-message": "assistant response"
}
```

### OpenCode

| 事件 | 支持 | 用途 |
|------|------|------|
| `session.idle` | ✅ | Agent 空闲 |
| `session.status` | ✅ | 状态变化 |
| `permission.asked` | ✅ | 权限请求 |
| `permission.replied` | ✅ | 权限回复 |
| `tool.execute.before` | ✅ | 工具执行前 |
| `tool.execute.after` | ✅ | 工具执行后 |
| `session.error` | ✅ | 错误发生 |
| `message.updated` | ✅ | 消息更新 |

**能力评估**: 最丰富的事件系统（30+ 事件类型），通过 Plugin 机制实现。

## 何时需要回退到终端轮询

### 必须使用终端轮询的场景

| 场景 | 原因 | 示例 |
|------|------|------|
| Codex 等待输入检测 | notify 只在 turn 完成时触发 | 用户需要回答问题时 |
| Hook 事件丢失 | 网络问题或进程崩溃 | hook 脚本执行失败 |
| 外部启动的 Agent | 未配置 CAM hooks | 用户直接运行 `claude` |
| 状态验证 | 确认 hook 报告的状态 | 防止误报 |

### 可以纯依赖 Native Hooks 的场景

| 场景 | 条件 |
|------|------|
| Claude Code 完整监控 | hooks 正确配置 |
| OpenCode Plugin 集成 | CAM Plugin 已安装 |
| 工具调用追踪 | 所有 CLI 都支持 |

## 混合检测的触发条件和切换逻辑

### 检测策略决策树

```
┌─────────────────────────────────────────────────────────────────┐
│                     状态检测决策流程                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────┐                                                │
│  │ Hook 事件   │                                                │
│  │ 到达?       │                                                │
│  └──────┬──────┘                                                │
│         │                                                       │
│    ┌────┴────┐                                                  │
│    │ Yes     │ No                                               │
│    ▼         ▼                                                  │
│  ┌─────────────┐  ┌─────────────┐                               │
│  │ 处理 Hook   │  │ 检查轮询    │                               │
│  │ 事件        │  │ 条件        │                               │
│  └──────┬──────┘  └──────┬──────┘                               │
│         │                │                                      │
│         │           ┌────┴────┐                                 │
│         │           │ 终端稳定 │                                │
│         │           │ ≥6秒?    │                                │
│         │           └────┬────┘                                 │
│         │           ┌────┴────┐                                 │
│         │      Yes  │         │ No                              │
│         │           ▼         ▼                                 │
│         │     ┌─────────┐  ┌─────────┐                          │
│         │     │ AI 状态 │  │ 跳过    │                          │
│         │     │ 检测    │  │ 本轮    │                          │
│         │     └────┬────┘  └─────────┘                          │
│         │          │                                            │
│         └──────────┴──────────────────────────────────────────► │
│                    │                                            │
│              ┌─────▼─────┐                                      │
│              │ 发送通知  │                                      │
│              │ (去重后)  │                                      │
│              └───────────┘                                      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 切换逻辑实现

```rust
/// 检测策略选择
pub enum DetectionStrategy {
    /// 纯 Hook 驱动（Claude Code, OpenCode with Plugin）
    HookOnly,
    /// Hook + 轮询混合（Codex）
    HookWithPolling,
    /// 纯轮询（无 Hook 支持或外部启动）
    PollingOnly,
}

impl DetectionStrategy {
    pub fn for_agent_type(agent_type: &AgentType, has_hooks: bool) -> Self {
        match (agent_type, has_hooks) {
            (AgentType::Claude, true) => Self::HookOnly,
            (AgentType::OpenCode, true) => Self::HookOnly,
            (AgentType::Codex, true) => Self::HookWithPolling,
            (_, false) => Self::PollingOnly,
        }
    }
}
```

### Hook 与轮询协调机制

当前实现中已有的协调机制（`src/agent_mod/watcher.rs`）：

1. **Hook 静默期**: 收到 hook 事件后 10 秒内不进行 AI 检测
2. **终端稳定性检测**: 内容稳定 6 秒后才触发 AI 检测
3. **跨进程协调**: 通过 `last_hook_events.json` 文件共享 hook 时间戳

```rust
/// Hook quiet period - skip AI check if hook event within this window (seconds)
const HOOK_QUIET_PERIOD_SECS: u64 = 10;

/// Terminal stability threshold (seconds)
const STABILITY_THRESHOLD_SECS: u64 = 6;
```

## 性能影响评估

### 轮询频率与 CPU 占用

| 轮询间隔 | CPU 占用 | AI 调用频率 | 适用场景 |
|----------|----------|-------------|----------|
| 1 秒 | 高 | 高 | 不推荐 |
| 3 秒 | 中 | 中 | 开发调试 |
| 5 秒 | 低 | 低 | 生产环境（推荐） |
| 10 秒 | 极低 | 极低 | 低优先级监控 |

### AI 调用优化

当前实现的优化策略：

1. **稳定性检测**: 只有终端内容稳定后才调用 AI
2. **内容指纹**: 使用 hash 避免重复检测相同内容
3. **Hook 协调**: hook 事件后跳过 AI 检测
4. **截断处理**: 只分析最后 30 行（`truncate_for_status`）

```rust
/// 计算内容指纹（用于稳定性检测）
fn content_fingerprint(content: &str) -> u64 {
    // 规范化内容：移除动画字符和时间相关内容
    let normalized = Self::normalize_content(content);
    // ... hash 计算
}
```

### 资源消耗对比

| 检测方式 | CPU | 网络 | 延迟 |
|----------|-----|------|------|
| Native Hook | 极低 | 无 | <100ms |
| 终端轮询 + AI | 中 | 每次检测 | 1-3s |
| 终端轮询 + 规则 | 低 | 无 | <10ms |

## 边界情况处理

### 1. 网络延迟

**问题**: AI API 调用超时导致状态检测失败

**处理策略**:
```rust
// 当前实现：15 秒超时
const EXTRACT_TIMEOUT_MS: u64 = 10000;

// 失败时返回 Unknown 状态
AgentStatus::Unknown => InputWaitResult {
    is_waiting: false,
    is_decision_required: false,
    pattern_type: Some(InputWaitPattern::Unknown),
    context,
}
```

**建议改进**:
- 添加重试机制（最多 2 次）
- 超时后回退到规则检测
- 发送 webhook 通知 AI 检测失败

### 2. 事件丢失

**问题**: Hook 脚本执行失败或被 kill

**处理策略**:
- 轮询作为兜底机制
- 定期验证 hook 配置状态
- 检测 hook 进程是否存活

**建议改进**:
```rust
/// 检测 hook 是否正常工作
fn verify_hook_health(agent_id: &str) -> bool {
    // 检查最近是否收到过 hook 事件
    let last_hook_time = load_last_hook_time(agent_id);
    let now = current_timestamp();

    // 如果 5 分钟没有 hook 事件，可能 hook 配置有问题
    now - last_hook_time < 300
}
```

### 3. 状态不一致

**问题**: Hook 报告的状态与终端实际状态不一致

**处理策略**:
- 使用 AI 验证 hook 报告的状态
- 状态冲突时以 AI 判断为准
- 记录不一致事件用于调试

### 4. 多 Agent 同目录

**问题**: 多个 Agent 在同一目录运行，cwd 匹配歧义

**处理策略**:
- 使用 thread-id（Codex）或 session-id（Claude）区分
- 记录警告日志
- 优先匹配最近启动的 Agent

### 5. TUI 渲染干扰

**问题**: 全屏 TUI（Codex、OpenCode）的 ANSI 转义序列干扰解析

**处理策略**:
- 使用 AI 而非正则解析
- 预处理移除 ANSI 转义序列
- 提取纯文本内容

## 各 Agent 类型的推荐策略

### Claude Code

```
检测策略: HookOnly
主要依赖: idle_prompt, stop, PreToolUse hooks
轮询角色: 仅用于验证和兜底
AI 调用: 仅在 hook 失效时
```

### Codex CLI

```
检测策略: HookWithPolling
主要依赖: agent-turn-complete notify
轮询角色: 检测等待输入状态
AI 调用: 每次 turn 完成后检测终端状态
```

### OpenCode

```
检测策略: HookOnly (通过 Plugin)
主要依赖: session.idle, permission.asked 事件
轮询角色: 仅用于 Plugin 未安装时
AI 调用: 仅在 Plugin 不可用时
```

### 未知/外部 Agent

```
检测策略: PollingOnly
主要依赖: 终端轮询 + AI 状态检测
轮询角色: 唯一检测手段
AI 调用: 每次稳定后检测
```

## 实现建议

### 1. Adapter 接口设计

```rust
/// Agent 适配器 trait
pub trait AgentAdapter {
    /// 获取检测策略
    fn detection_strategy(&self) -> DetectionStrategy;

    /// 处理 native hook 事件
    fn handle_hook_event(&mut self, event: HookEvent) -> Option<AgentStatus>;

    /// 执行终端轮询检测
    fn poll_terminal(&mut self, content: &str) -> Option<AgentStatus>;

    /// 是否需要 AI 检测
    fn needs_ai_detection(&self) -> bool;
}
```

### 2. 统一事件模型

```rust
/// 统一的 Agent 状态事件
pub enum AgentEvent {
    /// 会话开始
    SessionStart { session_id: String },
    /// 等待输入
    WaitingForInput { context: String, is_decision: bool },
    /// 工具调用
    ToolUse { tool: String, target: Option<String> },
    /// 错误
    Error { message: String },
    /// 会话结束
    SessionEnd,
}
```

### 3. 配置驱动

```json
{
  "adapters": {
    "claude": {
      "detection_strategy": "hook_only",
      "poll_interval_secs": 5,
      "ai_detection_enabled": true
    },
    "codex": {
      "detection_strategy": "hook_with_polling",
      "poll_interval_secs": 3,
      "ai_detection_enabled": true
    },
    "opencode": {
      "detection_strategy": "hook_only",
      "plugin_required": true
    }
  }
}
```

## 结论

1. **Claude Code**: 完整的 hooks 支持，可以纯依赖 native hooks
2. **Codex CLI**: 需要混合策略，notify 触发后用 AI 检测终端状态
3. **OpenCode**: Plugin 系统最强大，推荐开发 CAM Plugin
4. **通用策略**: 终端轮询 + AI 检测作为通用兜底方案

关键设计原则：
- **优先使用 native hooks**: 延迟低、资源消耗小
- **AI 检测作为补充**: 处理 hooks 无法覆盖的场景
- **稳定性检测优化**: 避免频繁 AI 调用
- **跨进程协调**: 防止 hook 和轮询重复通知
