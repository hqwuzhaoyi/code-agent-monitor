# TUI 排序和动画设计

## 概述

为 CAM TUI 添加两个功能：
1. Agent 列表按启动时间排序（最新在前）
2. 状态图标动画效果

## 排序

在 `refresh_agents()` 中，加载完 agents 后按 `started_at` 降序排序：

```rust
items.sort_by(|a, b| b.started_at.cmp(&a.started_at));
```

## 动画

### 帧序列

| 状态 | 帧序列 | 含义 |
|------|--------|------|
| Running | `◐ ◓ ◑ ◒` | 旋转圆，表示处理中 |
| Waiting | `◉ ◎ ◉ ◎` | 脉冲闪烁，引起注意 |
| Idle | `○ ◌ ○ ◌` | 缓慢呼吸，表示空闲 |
| Error | `✗ ⚠ ✗ ⚠` | 警告闪烁 |

### 实现方案

采用全局 tick 方案：

1. `App` 新增 `animation_tick: usize` 字段
2. `AgentState::icon()` 改为 `icon(&self, tick: usize) -> &'static str`
3. 每次渲染后递增 tick
4. 动画速度：每 200ms 切换一帧（每 2 次渲染）

### 代码变更

- `src/tui/app.rs`: 添加 `animation_tick` 字段，主循环中递增
- `src/tui/state.rs`: 修改 `AgentState::icon()` 签名和实现
- `src/tui/ui.rs`: 传递 tick 参数给 `icon()`
- `src/tui/tests.rs`: 更新测试用例
