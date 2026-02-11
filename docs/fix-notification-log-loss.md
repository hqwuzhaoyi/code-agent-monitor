# 修复：通知系统日志丢失问题

## 问题描述

用户收到 `⏸️ ClaudePrompt 等待输入` 通知，但中间的决策内容（Agent 在问什么问题）丢失了。

例如：
```
# 用户期望看到
⏸️ myproject 等待选择

这部分结构看起来合适吗？

1. 是的，继续
2. 不，需要修改

回复数字选择 `cam-xxx`

# 实际看到
⏸️ myproject 等待输入 cam-xxx
```

## 根本原因

### 1. 终端快照行数不足

数据流中有多次行数限制：

| 位置 | 原值 | 问题 |
|------|------|------|
| agent_watcher.rs:164 | 30 行 | capture_pane 获取不够 |
| input_detector.rs:163 | 15 行 | get_last_lines 再截断 |
| main.rs:594 | 15 行 | get_logs 再截断 |

结果：最终只有 15 行，可能丢失问题内容。

### 2. clean_terminal_context 过度清洗

`openclaw_notifier.rs:320-322` 的代码会丢弃用户输入前的所有内容：

```rust
// 问题代码
let start_idx = last_user_input_idx.map(|i| i + 1).unwrap_or(0);
let content_to_process = raw_lines[start_idx..].join("\n");
```

如果 Agent 问"这部分结构看起来合适吗？"，而用户最后输入是"y"，问题会被丢弃。

## 修复方案

### 修复 1: 增加终端快照行数

| 文件 | 修改 |
|------|------|
| src/agent_watcher.rs:164 | `capture_pane(..., 30)` → `capture_pane(..., 50)` |
| src/agent_watcher.rs:234 | `capture_pane(..., 20)` → `capture_pane(..., 50)` |
| src/input_detector.rs:140 | `get_last_lines(output, 15)` → `get_last_lines(output, 30)` |
| src/input_detector.rs:163 | `get_last_lines(output, 15)` → `get_last_lines(output, 30)` |
| src/main.rs:594,598,602 | `get_logs(..., 15)` → `get_logs(..., 30)` |

### 修复 2: 改进 clean_terminal_context 逻辑

在丢弃用户输入前的内容之前，先向前查找最近的问题行：

```rust
// 修复后的代码
let start_idx = if let Some(last_input_idx) = last_user_input_idx {
    // 向前查找最近的问题行（最多 10 行）
    let search_start = last_input_idx.saturating_sub(10);
    let mut question_idx = None;
    for i in (search_start..last_input_idx).rev() {
        let trimmed = raw_lines[i].trim();
        // 检查是否是问题行
        if trimmed.contains('?') || trimmed.contains('？')
            || trimmed.ends_with(':') || trimmed.ends_with('：')
            || trimmed.contains("[Y]es") || trimmed.contains("[Y/n]")
            || trimmed.contains("[y/N]") || trimmed.contains("[是/否]") {
            question_idx = Some(i);
            break;
        }
    }
    // 如果找到问题行，从问题行开始；否则从用户输入后开始
    question_idx.unwrap_or(last_input_idx + 1)
} else {
    0
};
```

## 测试验证

### 新增测试用例

```rust
#[test]
fn test_clean_terminal_context_preserves_question_before_user_input() {
    // 场景：Agent 问"这部分结构看起来合适吗？"，用户回复"y"
    let context = r#"
这是一个设计方案：
1. 组件 A
2. 组件 B
这部分结构看起来合适吗？
❯ y
好的，我继续执行
❯ "#;

    let cleaned = OpenclawNotifier::clean_terminal_context(context);

    // 应该保留问题
    assert!(cleaned.contains("这部分结构看起来合适吗"));
}
```

### 运行测试

```bash
# 运行所有相关测试
cargo test --lib openclaw_notifier
cargo test --lib input_detector

# 运行新增的回归测试
cargo test --lib test_clean_terminal_context_preserves
```

## 部署

```bash
# 构建
cargo build --release

# 更新插件二进制
cp target/release/cam plugins/cam/bin/cam

# 重启 gateway（如果使用 OpenClaw）
openclaw gateway restart

# 重启 watcher daemon（如果正在运行）
kill $(cat ~/.claude-monitor/watcher.pid) 2>/dev/null
# watcher 会在下次 agent 启动时自动启动
```

## 相关文件

- `src/agent_watcher.rs` - Agent 状态监控
- `src/input_detector.rs` - 输入等待检测
- `src/main.rs` - CLI 入口和 Hook 处理
- `src/openclaw_notifier.rs` - 通知格式化和发送
