# 统一 Agent 状态设计（简化版）

## 背景

当前 CAM 存在三套状态枚举，分布在不同模块中：

| 模块 | 位置 | 状态值 | 用途 |
|------|------|--------|------|
| AI 层 | `src/ai/types.rs` | Processing, WaitingForInput, Unknown | AI 检测终端状态 |
| 持久化层 | `src/agent_mod/manager.rs` | Running, Waiting, Stopped | agents.json 存储 |
| UI 层 | `src/tui/state.rs` | Running, Waiting, Idle, Error | TUI 显示 |

### 问题分析

1. **命名冲突**：两个 `AgentStatus` 枚举，语义完全不同

2. **死代码**：
   - `manager::AgentStatus::Waiting` 和 `Stopped` 从未被设置
   - `tui::AgentState::Error` 从未被设置
   - `tui::AgentState::Idle` 实际是 Stopped 的映射，语义混淆

3. **转换链断裂**：
   - AI 层的检测结果不会更新到持久化层
   - 持久化层的状态是静态的（始终为 Running）
   - TUI 显示的状态与实际运行状态不符

4. **状态边界模糊**：
   - Idle 和 WaitingForInput 难以区分
   - Stopped 和 Error 边界不清
   - Agent 停止时直接删除记录，不保留状态

## 设计目标

1. 统一三套状态枚举为一套
2. 简化状态，只保留 AI 能可靠检测的状态
3. 语义清晰，无歧义
4. 支持 TUI 显示需求

## 兼容性策略

本次状态统一迁移**不兼容**历史 `agents.json` 状态值：

- 旧值：`running` / `waiting` / `stopped`（`lowercase`）
- 新值：`processing` / `waiting_for_input` / `unknown`（`snake_case`）

迁移策略为直接切换到新格式，不增加反序列化兼容分支（如 alias/custom deserialize）。发布或本地执行迁移前，先删除旧 `~/.config/code-agent-monitor/agents.json`，由新版本自动重建。

## 统一状态设计

### 枚举定义

```rust
/// Agent 统一状态
///
/// 简化设计：只保留 AI 能可靠检测的状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// 正在处理中 - agent 正在执行任务
    /// - AI 检测：终端显示处理动画/进度（Thinking…、Brewing…等）
    /// - 通知：不发送通知
    /// - TUI 图标：🟢 (绿色)
    Processing,

    /// 等待输入 - agent 空闲，等待用户响应
    /// - AI 检测：终端显示提示符或问题
    /// - 通知：应发送通知
    /// - TUI 图标：🟡 (黄色)
    WaitingForInput,

    /// 未知 - 无法确定状态
    /// - 场景：AI 检测失败、网络错误
    /// - 通知：保守策略，发送通知
    /// - TUI 图标：❓ (灰色)
    Unknown,
}
```

### 设计决策

**移除的状态及理由**：

| 移除的状态 | 理由 |
|-----------|------|
| `Idle` | 与 WaitingForInput 难以区分，AI 无法可靠检测"空闲但不需要输入"的状态 |
| `Stopped` | Agent 停止时直接从 agents.json 删除记录，不需要状态 |
| `Error` | 作为事件处理（WatchEvent::Error），不作为持久化状态 |

**保留的状态**：

| 状态 | 语义 | 触发条件 | 通知策略 | TUI 图标 |
|------|------|----------|----------|----------|
| Processing | 正在执行任务 | AI 检测到处理动画 | 不发送 | 🟢 |
| WaitingForInput | 等待用户输入 | AI 检测到提示符/问题 | 发送 | 🟡 |
| Unknown | 未知状态 | AI 检测失败 | 发送（保守） | ❓ |

### 状态转换图

```
    ┌────────────┐  wait   ┌──────────────────┐
    │ Processing │ ◄─────► │ WaitingForInput  │
    └────────────┘ resume  └──────────────────┘
          │                        │
          │ detect_fail            │ detect_fail
          ▼                        ▼
    ┌─────────────────────────────────────────┐
    │               Unknown                    │
    └─────────────────────────────────────────┘
                      │
                      │ detect_success
                      ▼
              (实际状态: Processing 或 WaitingForInput)
```

### 辅助方法

```rust
impl AgentStatus {
    /// 是否应该发送通知
    pub fn should_notify(&self) -> bool {
        matches!(self, Self::WaitingForInput | Self::Unknown)
    }

    /// 获取 TUI 显示图标
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Processing => "🟢",
            Self::WaitingForInput => "🟡",
            Self::Unknown => "❓",
        }
    }

    /// 获取显示颜色
    pub fn color(&self) -> Color {
        match self {
            Self::Processing => Color::Green,
            Self::WaitingForInput => Color::Yellow,
            Self::Unknown => Color::DarkGray,
        }
    }

    /// 是否正在处理
    pub fn is_processing(&self) -> bool {
        matches!(self, Self::Processing)
    }

    /// 是否在等待输入
    pub fn is_waiting(&self) -> bool {
        matches!(self, Self::WaitingForInput)
    }
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Unknown
    }
}
```

## 迁移方案

### 阶段 0：清理旧 agents.json（不做向后兼容）

```bash
rm -f ~/.config/code-agent-monitor/agents.json
```

说明：清理后由新版本按统一状态定义写入新文件。

### 阶段 1：替换 manager.rs 中的 AgentStatus

**修改 `src/agent_mod/manager.rs`**：
```rust
// 当前：Running, Waiting, Stopped
// 改为：Processing, WaitingForInput, Unknown
```

- 替换枚举变体
- 添加辅助方法
- 启动时设置为 `Processing`（而非 Running）

### 阶段 2：删除 ai::types::AgentStatus

- 删除 `src/ai/types.rs` 中的 `AgentStatus` 定义（第 34-61 行）
- 更新 `src/ai/mod.rs` 导出
- 更新所有 import 路径指向 `crate::agent_mod::manager::AgentStatus`

### 阶段 3：删除 tui::state::AgentState

- 删除 `src/tui/state.rs` 中的 `AgentState` 定义（第 6-24 行）
- `AgentItem.state` 改为使用 `AgentStatus`
- 移除 `src/tui/app.rs` 中的状态转换逻辑

### 阶段 4：添加状态同步机制

在 `AgentWatcher.poll_once()` 中，当检测到状态变化时更新 agents.json：

```rust
// 检测到状态变化时
if new_status != agent.status {
    agent_manager.update_agent_status(&agent.agent_id, new_status)?;
}
```

需要在 `AgentManager` 中添加 `update_agent_status()` 方法。

## 需要修改的文件

| 文件 | 修改内容 |
|------|----------|
| `src/agent_mod/manager.rs` | 替换 AgentStatus 枚举，添加 `update_agent_status()` |
| `src/ai/types.rs` | 删除 AgentStatus 定义 |
| `src/ai/mod.rs` | 移除 AgentStatus 导出 |
| `src/ai/extractor.rs` | import 路径改为 `crate::agent_mod::manager::AgentStatus` |
| `src/ai/quality.rs` | import 路径改为 `crate::agent_mod::manager::AgentStatus` |
| `src/infra/input.rs` | import 路径改为 `crate::agent_mod::manager::AgentStatus` |
| `src/tui/state.rs` | 删除 AgentState 定义 |
| `src/tui/app.rs` | 直接使用 AgentStatus，移除转换逻辑 |
| `src/agent_mod/watcher.rs` | 检测到状态变化时调用 `update_agent_status()` |

## 代码改动量估算

| 文件 | 改动类型 | 行数 |
|------|----------|------|
| `src/agent_mod/manager.rs` | 替换+新增方法 | +50 |
| `src/ai/types.rs` | 删除 | -30 |
| `src/tui/state.rs` | 删除 | -20 |
| `src/ai/extractor.rs` | import | ~3 |
| `src/ai/quality.rs` | import | ~3 |
| `src/infra/input.rs` | import | ~1 |
| `src/tui/app.rs` | 简化 | -10 |
| `src/agent_mod/watcher.rs` | 状态同步 | +10 |
| **总计** | | ~100 行 |

## 删除的类型

统一后删除：
1. `src/ai/types.rs` 中的 `AgentStatus` - 合并到统一类型
2. `src/tui/state.rs` 中的 `AgentState` - 直接使用统一类型

## 文件位置

统一的 `AgentStatus` 保留在 `src/agent_mod/manager.rs`，作为全局共享类型。
