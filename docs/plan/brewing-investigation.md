# Brewing 通知问题调查报告

## 问题描述

用户收到的通知内容是：
```
⏸️ ClaudePrompt 等待输入

✶ Brewing…

回复内容 cam-1770565728
```

期望显示的是实际问题内容（选项列表），而不是 "✶ Brewing…"。

## 终端快照

Agent `cam-1770565728` 的实际终端内容：

```
 ▐▛███▜▌   Claude Code v2.1.33
▝▜█████▛▘  Opus 4.6 · API Usage Billing
  ▘▘ ▝▝    ~/workspace

  Welcome to Opus 4.6

❯ 使用 brainstorm 模式，帮我创建一个 React todo list
  项目。先头脑风暴一下功能设计和技术选型，然后再开始实现。

⏺ Skill(brainstorming)
  ⎿  Successfully loaded skill

⏺ 让我先了解一下当前项目的上下文。

⏺ Read 1 file (ctrl+o to expand)

⏺ 这是一个新项目。让我开始头脑风暴。

  第一个问题：项目的主要用途是什么？

  A) 个人学习项目 - 用于学习 React 基础，功能简单即可
  B) 作品集展示 - 需要精美的 UI 和完整功能，展示技术能力
  C) 实际使用的工具 - 真正用来管理日常任务，需要实用性
  D) 其他 - 请说明

────────────────────────────────────────────────────────────────────────────────
❯
────────────────────────────────────────────────────────────────────────────────
  [Opus 4.6] ██░░░░░░░░ 22% | ⏱️  <1m
  workspace git:(main*)
  2 MCPs | 5 hooks
  ✓ Skill ×1 | ✓ Bash ×1
```

**关键发现**：终端快照中没有 "✶ Brewing…" 内容。这说明通知是在 Claude Code 正在思考/生成时触发的，当时终端显示的是 "✶ Brewing…" 状态指示器。

## 根因分析

### 1. "Brewing" 是什么？

"✶ Brewing…" 是 Claude Code 的**思考中状态指示器**，表示 AI 正在生成响应。这是一个临时状态，会在生成完成后被实际内容替换。

### 2. 为什么通知捕获到了 Brewing？

通知触发时机问题：
- Hook 在 Claude Code 显示 "✶ Brewing…" 时触发了 `idle_prompt` 通知
- 此时终端快照捕获的是中间状态，而不是最终的问题内容

### 3. 噪音过滤器为什么没有过滤 Brewing？

查看 `clean_terminal_context` 函数的噪音过滤模式：

```rust
// 工具调用状态
r"(?m)^.*[✓◐⏺✻].*$",
```

过滤器只包含了 `✻` (U+273B)，但 "Brewing" 使用的是 `✶` (U+2736)：
- `✻` = U+273B (TEARDROP-SPOKED ASTERISK)
- `✶` = U+2736 (SIX POINTED BLACK STAR)

这两个是不同的 Unicode 字符，所以 "✶ Brewing…" 没有被过滤。

### 4. 为什么 ClaudePrompt 检测到了等待输入？

查看 `input_detector.rs` 中的 ClaudePrompt 检测模式：

```rust
// Claude Code 的提示符（支持 > 和 ❯）
(Regex::new(r"(?m)^[>❯]\s*$").unwrap(), InputWaitPattern::ClaudePrompt),
// Claude Code 的 ❯ 提示符（Unicode U+276F）
(Regex::new(r"❯\s*$").unwrap(), InputWaitPattern::ClaudePrompt),
```

问题：当 Claude Code 显示 "✶ Brewing…" 时，终端可能仍然有 `❯` 提示符在某一行，导致误检测为 ClaudePrompt 等待输入。

## 问题链路

```
1. Claude Code 开始思考，显示 "✶ Brewing…"
2. 终端某处仍有 ❯ 提示符
3. input_detector 检测到 ClaudePrompt 模式
4. 触发 WaitingForInput 通知
5. clean_terminal_context 没有过滤 ✶ 字符
6. 通知显示 "✶ Brewing…" 而不是实际问题
```

## 修复建议

### 方案 1：添加 ✶ 到噪音过滤器（推荐）

在 `clean_terminal_context` 的噪音过滤模式中添加 `✶`：

```rust
// 工具调用状态
r"(?m)^.*[✓◐⏺✻✶].*$",
```

**优点**：简单直接，过滤所有 Claude Code 状态指示器
**缺点**：只是治标，没有解决根本的时机问题

### 方案 2：添加 Brewing 专用过滤（推荐）

在噪音过滤模式中添加 Brewing 相关模式：

```rust
// Claude Code 思考状态
r"(?m)^.*Brewing.*$",
r"(?m)^.*Thinking.*$",
```

**优点**：明确过滤思考状态
**缺点**：需要维护更多模式

### 方案 3：检测 Brewing 状态并跳过通知

在 `input_detector.rs` 中添加 Brewing 状态检测：

```rust
// 如果检测到 Brewing 状态，不认为是等待输入
if context.contains("Brewing") || context.contains("✶") {
    return InputWaitResult {
        is_waiting: false,
        pattern_type: None,
        context: String::new(),
    };
}
```

**优点**：从根本上解决问题，避免在思考状态时触发通知
**缺点**：需要修改检测逻辑

### 方案 4：增加空闲检测延迟

增加 `idle_threshold` 时间，确保 Claude Code 完成思考后再触发通知。

**优点**：简单
**缺点**：会延迟所有通知，影响用户体验

## 推荐修复

建议同时实施方案 1 和方案 3：

1. **短期修复**：在 `clean_terminal_context` 中添加 `✶` 到噪音过滤器
2. **长期修复**：在 `input_detector` 中检测 Brewing 状态并跳过

### 具体代码修改

#### 文件：`src/openclaw_notifier.rs`

```rust
// 修改 clean_terminal_context 函数中的噪音过滤模式
// 工具调用状态 - 添加 ✶
r"(?m)^.*[✓◐⏺✻✶].*$",

// 添加 Brewing/Thinking 状态过滤
r"(?m)^.*Brewing.*$",
r"(?m)^.*Thinking.*$",
```

#### 文件：`src/input_detector.rs`

```rust
// 在 detect_immediate 函数开头添加 Brewing 状态检测
pub fn detect_immediate(&self, output: &str) -> InputWaitResult {
    let context = Self::get_last_lines(output, 15);

    // 如果检测到 Brewing/Thinking 状态，不认为是等待输入
    if context.contains("Brewing") || context.contains("Thinking") || context.contains("✶") {
        return InputWaitResult {
            is_waiting: false,
            pattern_type: None,
            context,
        };
    }

    // ... 原有逻辑
}
```

## 测试验证

修复后，可以使用以下测试验证：

```bash
# 测试噪音过滤
echo "✶ Brewing…" | ./target/release/cam notify --event WaitingForInput --agent-id test --dry-run

# 预期：不应该显示 "✶ Brewing…"
```

## 总结

| 项目 | 内容 |
|------|------|
| 问题 | 通知显示 "✶ Brewing…" 而不是实际问题 |
| 根因 | 1. 通知在 Claude Code 思考时触发；2. ✶ 字符未被噪音过滤 |
| 修复 | 添加 ✶ 到噪音过滤器 + 检测 Brewing 状态跳过通知 |
| 优先级 | 高（影响用户体验） |
