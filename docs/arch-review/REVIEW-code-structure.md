# CAM 代码结构评审报告

## 1. 各模块职责分析

### 1.1 src/agent_watcher.rs (942 行)
**职责**: Agent 状态监控、JSONL 事件解析、输入等待检测

**当前职责**:
- Agent 生命周期监控 (tmux session 存活检测)
- JSONL 事件流解析 (工具调用、错误)
- 输入等待状态检测 (AI 调用)
- 通知锁定/去重逻辑
- 终端稳定性检测
- Hook 事件跨进程协调

**问题**: 职责过多，违反单一职责原则。一个模块同时处理监控、解析、检测、去重、协调等多个关注点。

### 1.2 src/anthropic.rs (1375 行)
**职责**: Anthropic API 客户端、终端状态判断、问题提取

**当前职责**:
- API 配置加载 (多来源优先级)
- HTTP 客户端封装
- 终端状态判断 (`is_agent_processing`)
- 问题内容提取 (`extract_question_with_haiku`)
- 通知内容提取 (`extract_notification_content`)
- 上下文完整性检测

**问题**: 文件过长，混合了基础设施代码 (API 客户端) 和业务逻辑 (问题提取)。

### 1.3 src/session.rs (409 行)
**职责**: Claude Code 会话管理

**当前职责**:
- 会话列表读取 (sessions-index.json)
- 会话过滤和排序
- 会话恢复 (tmux)
- JSONL 日志解析
- tmux 输入发送

**评价**: 职责相对清晰，但 JSONL 解析与 `jsonl_parser.rs` 有重复。

### 1.4 src/conversation_state.rs (615 行)
**职责**: 对话状态管理、快捷回复处理

**当前职责**:
- 待处理确认管理 (注册、查询、移除)
- 快捷回复标准化 (y/n/数字)
- 回复路由 (tmux/inbox)
- 当前活跃 Team/Agent 追踪

**评价**: 职责清晰，但 `send_to_tmux` 与 `session.rs` 重复。

### 1.5 src/notification/ 模块
**职责**: 通知系统抽象层

| 文件 | 行数 | 职责 |
|------|------|------|
| mod.rs | 41 | 模块导出 |
| channel.rs | 135 | 渠道 trait 定义 |
| dispatcher.rs | 180 | 多渠道分发 |
| formatter.rs | 1251 | 消息格式化 |
| event.rs | 443 | 统一事件结构 |
| builder.rs | ~200 | 构建器模式 |
| deduplicator.rs | ~150 | 通知去重 |
| urgency.rs | ~100 | 紧急程度定义 |

**评价**:
- 抽象层次合理，trait 设计清晰
- `formatter.rs` 过长 (1251 行)，包含大量重复的格式化逻辑
- `event.rs` 设计良好，提供统一的事件结构

### 1.6 src/agent.rs (795 行)
**职责**: Agent 生命周期管理

**当前职责**:
- Agent 启动/停止
- agents.json 持久化 (带文件锁)
- tmux session 管理
- 外部会话注册
- 初始 prompt 发送

**评价**: 职责相对清晰，文件锁实现规范。

---

## 2. 代码质量问题清单

### 2.1 过长函数 (>50 行)

| 文件 | 函数 | 行数 | 问题 |
|------|------|------|------|
| agent_watcher.rs | `poll_once` | ~200 | 包含多个独立逻辑块 |
| agent_watcher.rs | `should_send_notification` | ~85 | 复杂的条件分支 |
| anthropic.rs | `extract_question_with_context` | ~140 | AI 调用 + JSON 解析 + 验证 |
| anthropic.rs | `is_agent_processing` | ~90 | 配置 + 调用 + 解析 |
| anthropic.rs | `load_api_config` | ~115 | 多来源配置加载 |
| formatter.rs | `format_event` | ~70 | 大量 match 分支 |
| formatter.rs | `format_notification` | ~90 | 重复的 AI 提取逻辑 |
| agent.rs | `start_agent` | ~100 | 启动流程 + 等待就绪 |

### 2.2 重复代码

1. **tmux send_keys 重复实现**:
   - `session.rs:237-249` - `send_to_tmux`
   - `conversation_state.rs:331-353` - `send_to_tmux`
   - `tmux.rs` - `send_keys`

   三处实现相同的 tmux 发送逻辑。

2. **AI 问题提取重复调用**:
   - `formatter.rs` 中 `format_notification`、`format_waiting_for_input`、`format_notification_type_event` 都有相似的 AI 提取逻辑

3. **项目名提取重复**:
   - `formatter.rs:78-86` - `extract_project_name`
   - `event.rs:69-74` - `project_name`

   两处实现相同的路径解析逻辑。

4. **JSON 提取重复**:
   - `anthropic.rs:959-967` - `extract_json_from_output`
   - 类似逻辑在多处出现

### 2.3 过度耦合

1. **agent_watcher.rs 依赖过多**:
   ```rust
   use crate::agent::{AgentManager, AgentRecord};
   use crate::input_detector::{InputWaitDetector, InputWaitResult};
   use crate::jsonl_parser::{JsonlEvent, JsonlParser};
   use crate::tmux::TmuxManager;
   ```
   直接依赖 4 个模块，且内部维护多个 HashMap 状态。

2. **formatter.rs 直接调用 AI**:
   格式化器不应该直接调用 AI API，这违反了关注点分离。

3. **conversation_state.rs 依赖 agent 和 team**:
   ```rust
   use crate::agent::AgentManager;
   use crate::team::{TeamBridge, InboxMessage};
   ```
   状态管理器不应该直接依赖具体的通信实现。

### 2.4 接口设计问题

1. **format_event 参数过多**:
   ```rust
   pub fn format_event(
       &self,
       agent_id: &str,
       event_type: &str,
       pattern_or_path: &str,  // 语义不清
       context: &str,          // 混合 JSON 和终端快照
   ) -> String
   ```
   参数语义不清晰，`pattern_or_path` 根据 event_type 有不同含义。

2. **NotificationContent vs ExtractedQuestion**:
   两个结构体表示相似概念，但字段不同：
   - `NotificationContent`: question_type, question, options, summary
   - `ExtractedQuestion`: question_type, question, options, reply_hint

   应该统一。

### 2.5 错误处理不一致

1. **静默忽略错误**:
   ```rust
   // agent.rs:124
   let _ = fs::create_dir_all(&data_dir);

   // agent_watcher.rs:458
   if let Ok(new_events) = parser.read_new_events() { ... }
   ```

2. **错误信息不够具体**:
   ```rust
   // anthropic.rs:328
   .map_err(|e| anyhow!("Cannot create HTTP client: {}", e))?;
   ```
   缺少上下文信息（如配置来源、URL 等）。

---

## 3. 重构建议

### 3.1 拆分 agent_watcher.rs

**建议**: 拆分为 3 个模块

```
src/watcher/
├── mod.rs              # 导出
├── agent_monitor.rs    # Agent 生命周期监控
├── event_processor.rs  # JSONL 事件处理
└── stability.rs        # 终端稳定性检测
```

**agent_monitor.rs** 职责:
- tmux session 存活检测
- Agent 状态快照
- 清理逻辑

**event_processor.rs** 职责:
- JSONL 解析
- 事件转换
- 工具调用/错误提取

**stability.rs** 职责:
- 内容指纹计算
- 稳定性状态管理
- Hook 事件协调

### 3.2 拆分 anthropic.rs

**建议**: 拆分为 2 个模块

```
src/ai/
├── mod.rs              # 导出
├── client.rs           # API 客户端 (配置、HTTP)
└── extractor.rs        # 内容提取 (问题、状态)
```

**client.rs** (~300 行):
- `AnthropicConfig`
- `AnthropicClient`
- `load_api_config`

**extractor.rs** (~500 行):
- `extract_notification_content`
- `extract_question_with_haiku`
- `is_agent_processing`
- `detect_waiting_question`

### 3.3 统一 tmux 操作

**建议**: 所有 tmux 操作通过 `TmuxManager` 进行

```rust
// 删除 session.rs 和 conversation_state.rs 中的 send_to_tmux
// 统一使用 tmux.rs 的实现

impl TmuxManager {
    pub fn send_keys(&self, session: &str, text: &str) -> Result<()>;
    pub fn send_keys_raw(&self, session: &str, key: &str) -> Result<()>;
}
```

### 3.4 重构 formatter.rs

**建议**:
1. 将 AI 提取逻辑移到调用方
2. 格式化器只负责纯格式化

```rust
// 当前: formatter 内部调用 AI
pub fn format_notification_event(&self, event: &NotificationEvent) -> String {
    // ... 内部调用 extract_question_with_haiku
}

// 建议: 调用方先提取，再格式化
pub fn format_notification_event(&self, event: &NotificationEvent, extracted: Option<&ExtractedQuestion>) -> String {
    // 纯格式化，不调用 AI
}
```

### 3.5 统一问题结构

**建议**: 合并 `NotificationContent` 和 `ExtractedQuestion`

```rust
#[derive(Debug, Clone)]
pub struct QuestionContent {
    pub question_type: QuestionType,
    pub question: String,
    pub options: Vec<String>,
    pub summary: String,      // 简洁摘要
    pub reply_hint: String,   // 回复提示
}
```

### 3.6 引入依赖注入

**建议**: 为 `ConversationStateManager` 引入 trait

```rust
pub trait ReplyRouter: Send + Sync {
    fn send_reply(&self, target: &ReplyTarget, message: &str) -> Result<()>;
}

pub struct ConversationStateManager<R: ReplyRouter> {
    state_file: PathBuf,
    router: R,
}
```

这样可以在测试中使用 mock 实现，避免直接依赖 `AgentManager` 和 `TeamBridge`。

---

## 4. 优先级建议

| 优先级 | 重构项 | 影响 | 工作量 |
|--------|--------|------|--------|
| P0 | 统一 tmux 操作 | 消除重复，减少 bug | 小 |
| P0 | 统一问题结构 | 简化接口 | 小 |
| P1 | 拆分 agent_watcher.rs | 提高可维护性 | 中 |
| P1 | 格式化器去 AI 依赖 | 关注点分离 | 中 |
| P2 | 拆分 anthropic.rs | 代码组织 | 中 |
| P2 | 引入依赖注入 | 可测试性 | 大 |

---

## 5. 总结

CAM 项目整体架构合理，通知系统的 trait 抽象设计良好。主要问题集中在：

1. **职责过重**: `agent_watcher.rs` 和 `anthropic.rs` 承担了过多职责
2. **代码重复**: tmux 操作、项目名提取、AI 调用逻辑存在重复
3. **耦合过紧**: 格式化器直接调用 AI，状态管理器直接依赖具体实现

建议优先处理 P0 级别的重构（统一 tmux 操作、统一问题结构），这些改动风险低、收益高。P1 级别的拆分可以在后续迭代中逐步进行。
