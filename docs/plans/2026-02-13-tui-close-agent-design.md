# TUI 关闭 Agent 功能设计

## 概述

在 TUI Dashboard 中添加关闭选中 agent 的功能，按 `x` 或 `d` 键直接关闭，无需确认。

## 方案选择

选择方案 A：在 App 中直接调用 AgentManager

- 保持 App 作为状态管理中心的职责
- 复用现有的 `AgentManager::stop_agent()` 方法
- 改动最小

## 组件变更

### 1. src/tui/app.rs

添加 `close_selected_agent(&mut self) -> AppResult<Option<String>>` 方法：
- 获取当前选中 agent 的 ID
- 调用 `AgentManager::stop_agent()`
- 返回被关闭的 agent ID
- 关闭后自动刷新 agent 列表

### 2. src/tui/event.rs

在 `handle_dashboard_key` 中添加按键处理：
- `KeyCode::Char('x')` 和 `KeyCode::Char('d')` 触发关闭
- 由于需要在主循环中处理（类似 Enter 键），设置标志位

### 3. src/tui/ui.rs

更新底部帮助栏：
- 添加 `[x] close` 提示

## 数据流

```
用户按 x/d → handle_dashboard_key 设置标志 → 主循环检测标志 → 调用 close_selected_agent() → AgentManager::stop_agent() → 刷新列表
```

## 错误处理

- 没有选中 agent：静默忽略
- 关闭失败（agent 已不存在）：静默忽略并刷新列表

## 测试

- 单元测试：`close_selected_agent` 方法正确调用 AgentManager
- 集成测试：按 x 键后 agent 从列表消失
