# Agent Adapter 集成分析报告

## 1. 当前架构概览

### 核心模块依赖关系

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLI Commands                             │
│  (cam start, cam list, cam watch, cam notify, cam reply)        │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      agent_mod (核心层)                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ AgentManager │  │ AgentWatcher │  │ WatcherDaemon        │  │
│  │ - start/stop │  │ - poll_once  │  │ - ensure_started     │  │
│  │ - list       │  │ - trigger    │  │ - stop               │  │
│  │ - send_input │  │ - snapshots  │  │                      │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│         │                 │                                      │
│         ▼                 ▼                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ AgentMonitor │  │ EventProc.   │  │ StabilityDetector    │  │
│  │ - is_alive   │  │ - read_new   │  │ - is_stable          │  │
│  │ - capture    │  │              │  │                      │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        infra (基础设施层)                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ TmuxManager  │  │ JsonlParser  │  │ InputWaitDetector    │  │
│  │ - create     │  │ - read_new   │  │ - detect             │  │
│  │ - capture    │  │ - get_recent │  │ - detect_immediate   │  │
│  │ - send_keys  │  │              │  │                      │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│         │                 │                    │                 │
│         ▼                 ▼                    ▼                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ ProcessScan  │  │ terminal.rs  │  │ anthropic.rs (AI)    │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    notification (通知层)                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ Notification │  │ Deduplicator │  │ Webhook              │  │
│  │ Store        │  │              │  │                      │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### 关键数据流

1. **Agent 启动流程**:
   ```
   CLI → AgentManager.start_agent() → TmuxManager.create_session()
                                    → agents.json 写入
                                    → WatcherDaemon.ensure_started()
   ```

2. **状态监控流程**:
   ```
   WatcherDaemon → AgentWatcher.poll_once()
                 → TmuxManager.capture_pane()
                 → InputWaitDetector.detect_immediate()
                 → anthropic.is_agent_processing() [AI 判断]
                 → NotificationDeduplicator.should_send()
                 → Webhook 发送
   ```

3. **事件处理流程**:
   ```
   JSONL 文件 → JsonlParser.read_new_events()
             → EventProcessor
             → WatchEvent 生成
   ```

## 2. 改动范围评估

### 需要修改的文件

| 文件 | 改动类型 | 改动程度 | 说明 |
|------|----------|----------|------|
| `src/agent_mod/mod.rs` | 新增导出 | 小 | 导出 adapter trait 和实现 |
| `src/agent_mod/adapter.rs` | 新建 | 大 | 定义 AgentAdapter trait |
| `src/agent_mod/adapters/mod.rs` | 新建 | 中 | 适配器模块组织 |
| `src/agent_mod/adapters/claude.rs` | 新建 | 大 | Claude Code 适配器 |
| `src/agent_mod/adapters/opencode.rs` | 新建 | 大 | OpenCode 适配器 |
| `src/agent_mod/adapters/codex.rs` | 新建 | 大 | Codex CLI 适配器 |
| `src/agent_mod/manager.rs` | 修改 | 中 | 使用 adapter 启动 agent |
| `src/agent_mod/watcher.rs` | 修改 | 中 | 使用 adapter 检测状态 |
| `src/infra/input.rs` | 可能修改 | 小 | 可能需要适配器特定逻辑 |

### 不需要修改的文件

| 文件 | 原因 |
|------|------|
| `src/notification/store.rs` | 通知存储与 agent 类型无关 |
| `src/notification/dedup.rs` | 去重逻辑与 agent 类型无关 |
| `src/infra/tmux.rs` | tmux 操作是通用的 |
| `src/infra/terminal.rs` | 终端处理是通用的 |
| `src/agent_mod/daemon.rs` | daemon 管理与 agent 类型无关 |

## 3. 向后兼容性分析

### 现有 Claude Code 功能保护

**关键原则**: 所有现有功能必须在引入抽象层后继续正常工作。

#### 兼容性保证措施

1. **默认适配器**: 当 `agent_type` 未指定或为 `claude` 时，使用 `ClaudeAdapter`
   ```rust
   impl AgentManager {
       fn get_adapter(&self, agent_type: &AgentType) -> Box<dyn AgentAdapter> {
           match agent_type {
               AgentType::Claude => Box::new(ClaudeAdapter::new()),
               AgentType::OpenCode => Box::new(OpenCodeAdapter::new()),
               AgentType::Codex => Box::new(CodexAdapter::new()),
               _ => Box::new(ClaudeAdapter::new()), // 默认
           }
       }
   }
   ```

2. **现有 API 不变**: `AgentManager` 的公开方法签名保持不变
   - `start_agent(request: StartAgentRequest) -> Result<StartAgentResponse>`
   - `stop_agent(agent_id: &str) -> Result<()>`
   - `send_input(agent_id: &str, input: &str) -> Result<()>`
   - `list_agents() -> Result<Vec<AgentRecord>>`

3. **数据格式兼容**: `agents.json` 格式保持不变，`AgentType` 枚举已存在

4. **AI 状态检测**: 现有的 `is_agent_processing()` 已经是 agent-agnostic 的

#### 风险点

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Claude 特定逻辑被移除 | 高 | 在 ClaudeAdapter 中保留所有特定逻辑 |
| 启动命令变化 | 中 | 单元测试覆盖启动流程 |
| 状态检测准确性下降 | 中 | 保留 AI 判断作为后备 |

## 4. 迁移路径设计

### 推荐方案: 渐进式迁移

```
Phase 1: 抽象层引入 (不改变行为)
    │
    ├── 定义 AgentAdapter trait
    ├── 实现 ClaudeAdapter (包装现有逻辑)
    ├── AgentManager 内部使用 adapter
    └── 所有测试通过
    │
Phase 2: 新适配器实现
    │
    ├── 实现 OpenCodeAdapter
    ├── 实现 CodexAdapter
    └── 集成测试
    │
Phase 3: 配置和文档
    │
    ├── 配置文件支持
    ├── CLI 参数支持
    └── 文档更新
```

### Phase 1 详细步骤

1. **创建 adapter trait** (`src/agent_mod/adapter.rs`)
   ```rust
   pub trait AgentAdapter: Send + Sync {
       fn agent_type(&self) -> AgentType;
       fn get_start_command(&self, resume_session: Option<&str>) -> String;
       fn detect_ready(&self, terminal_output: &str) -> bool;
       fn get_config_paths(&self) -> Vec<PathBuf>;
       fn parse_events(&self, log_path: &str) -> Vec<AgentEvent>;
   }
   ```

2. **实现 ClaudeAdapter** (提取现有逻辑)
   ```rust
   impl AgentAdapter for ClaudeAdapter {
       fn get_start_command(&self, resume_session: Option<&str>) -> String {
           // 从 AgentManager::get_agent_command 提取
           if let Some(session_id) = resume_session {
               format!("claude --resume {}", session_id)
           } else {
               "claude".to_string()
           }
       }

       fn detect_ready(&self, terminal_output: &str) -> bool {
           // 从 AgentManager::start_agent 提取
           let claude_prompt_re = regex::Regex::new(r"(?m)^[❯>]\s*$").unwrap();
           claude_prompt_re.is_match(terminal_output)
               || terminal_output.contains("Welcome to")
               || terminal_output.contains("Claude Code")
       }
   }
   ```

3. **修改 AgentManager** (使用 adapter)
   ```rust
   impl AgentManager {
       pub fn start_agent(&self, request: StartAgentRequest) -> Result<StartAgentResponse> {
           let adapter = self.get_adapter(&agent_type);
           let command = adapter.get_start_command(request.resume_session.as_deref());
           // ... 其余逻辑不变
       }
   }
   ```

### Phase 2 详细步骤

1. **OpenCodeAdapter** 实现要点:
   - 启动命令: `opencode`
   - 就绪检测: 检测 TUI 初始化完成
   - 事件解析: 通过 Plugin 系统或终端监控

2. **CodexAdapter** 实现要点:
   - 启动命令: `codex` 或 `codex exec --json`
   - 就绪检测: 检测 TUI 或 JSON 输出开始
   - 事件解析: JSON Lines 流解析

## 5. 测试策略

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // 1. Adapter trait 测试
    #[test]
    fn test_claude_adapter_start_command() {
        let adapter = ClaudeAdapter::new();
        assert_eq!(adapter.get_start_command(None), "claude");
        assert_eq!(
            adapter.get_start_command(Some("abc123")),
            "claude --resume abc123"
        );
    }

    #[test]
    fn test_claude_adapter_detect_ready() {
        let adapter = ClaudeAdapter::new();
        assert!(adapter.detect_ready("Welcome to Claude Code\n❯"));
        assert!(!adapter.detect_ready("Loading..."));
    }

    // 2. 向后兼容性测试
    #[test]
    fn test_default_adapter_is_claude() {
        let manager = AgentManager::new_for_test();
        let adapter = manager.get_adapter(&AgentType::Unknown);
        assert_eq!(adapter.agent_type(), AgentType::Claude);
    }

    // 3. AgentManager 行为不变测试
    #[test]
    fn test_start_agent_behavior_unchanged() {
        // 使用 mock adapter 验证调用顺序
    }
}
```

### 集成测试

```rust
#[test]
#[ignore = "requires tmux"]
fn test_claude_agent_lifecycle() {
    let manager = AgentManager::new_for_test();

    // 启动
    let response = manager.start_agent(StartAgentRequest {
        project_path: "/tmp".to_string(),
        agent_type: Some("claude".to_string()),
        ..Default::default()
    }).unwrap();

    // 验证 tmux session 存在
    assert!(manager.tmux.session_exists(&response.tmux_session));

    // 停止
    manager.stop_agent(&response.agent_id).unwrap();
    assert!(!manager.tmux.session_exists(&response.tmux_session));
}
```

### E2E 测试场景

| 场景 | 验证点 |
|------|--------|
| Claude Code 启动 | tmux session 创建，agents.json 更新 |
| Claude Code 状态检测 | AI 判断正确，通知发送 |
| Claude Code 输入发送 | tmux send-keys 正确执行 |
| 多 agent 类型共存 | 不同类型 agent 互不干扰 |

## 6. 错误处理和降级策略

### 错误处理层次

```
┌─────────────────────────────────────────────────────────────────┐
│ Level 1: Adapter 层错误                                          │
│ - 启动命令失败 → 返回错误，不创建 agent 记录                      │
│ - 就绪检测超时 → 记录警告，继续（可能 initial_prompt 不发送）     │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ Level 2: 状态检测错误                                            │
│ - AI API 失败 → 返回 Unknown 状态，不发送通知                    │
│ - 终端捕获失败 → 跳过本次检测，下次重试                          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ Level 3: 通知发送错误                                            │
│ - Webhook 失败 → 重试 3 次，记录到本地                           │
│ - 去重状态损坏 → 重置去重状态，可能重复通知                      │
└─────────────────────────────────────────────────────────────────┘
```

### 降级策略

1. **适配器不可用时**:
   ```rust
   fn get_adapter(&self, agent_type: &AgentType) -> Box<dyn AgentAdapter> {
       match agent_type {
           AgentType::OpenCode => {
               if OpenCodeAdapter::is_available() {
                   Box::new(OpenCodeAdapter::new())
               } else {
                   warn!("OpenCode not available, falling back to generic");
                   Box::new(GenericAdapter::new())
               }
           }
           // ...
       }
   }
   ```

2. **状态检测降级**:
   ```rust
   fn detect_status(&self, agent: &AgentRecord) -> AgentStatus {
       // 优先使用适配器特定检测
       if let Some(status) = self.adapter.detect_status(terminal_output) {
           return status;
       }

       // 降级到 AI 通用检测
       is_agent_processing(terminal_output)
   }
   ```

3. **事件解析降级**:
   ```rust
   fn parse_events(&self, agent: &AgentRecord) -> Vec<WatchEvent> {
       // 优先使用 JSONL/Plugin
       if let Some(events) = self.adapter.parse_events(agent) {
           return events;
       }

       // 降级到终端监控
       self.detect_from_terminal(agent)
   }
   ```

## 7. 实施建议

### 优先级排序

1. **P0 (必须)**: ClaudeAdapter 实现，确保现有功能不受影响
2. **P1 (重要)**: OpenCodeAdapter 实现（Plugin 方式）
3. **P2 (可选)**: CodexAdapter 实现（JSON Lines 方式）
4. **P3 (未来)**: 其他 agent 支持

### 时间估算

| 阶段 | 工作量 | 依赖 |
|------|--------|------|
| Phase 1 | 2-3 天 | 无 |
| Phase 2 (OpenCode) | 3-4 天 | Phase 1 |
| Phase 2 (Codex) | 2-3 天 | Phase 1 |
| Phase 3 | 1-2 天 | Phase 2 |

### 风险缓解

1. **充分测试**: 每个 phase 完成后运行完整测试套件
2. **渐进发布**: 使用 feature flag 控制新适配器启用
3. **监控**: 添加 metrics 跟踪各适配器的成功率
4. **回滚计划**: 保留直接使用 Claude 逻辑的代码路径

## 8. 结论

抽象层设计可以在不破坏现有功能的前提下实现。关键是:

1. **ClaudeAdapter 必须完整包装现有逻辑**，不能遗漏任何特定处理
2. **渐进式迁移**比一次性重构风险更低
3. **AI 状态检测**已经是 agent-agnostic 的，这是一个优势
4. **测试覆盖**是确保兼容性的关键

建议从 Phase 1 开始，在 ClaudeAdapter 完全验证后再进行 Phase 2。
