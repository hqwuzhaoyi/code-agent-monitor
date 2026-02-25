# Codex 信任确认界面调研报告

## 信任确认界面特征

当 Codex 启动到新目录时，会显示以下信任确认界面：

```
> You are in /private/tmp/codex-trust-test

  Do you trust the contents of this directory? Working with untrusted contents
  comes with higher risk of prompt injection.

› 1. Yes, continue
  2. No, quit

  Press enter to continue
```

### 关键特征

| 特征 | 值 |
|------|-----|
| 提示文本 | `Do you trust the contents of this directory?` |
| 选项 1 | `1. Yes, continue` |
| 选项 2 | `2. No, quit` |
| 底部提示 | `Press enter to continue` |
| 选择指示符 | `›` 在选中项前 |

## 正常就绪状态特征

确认信任后，Codex 显示正常界面：

```
╭───────────────────────────────────────────────────╮
│ >_ OpenAI Codex (v0.104.0)                        │
│                                                   │
│ model:     gpt-5.3-codex xhigh   /model to change │
│ directory: /private/tmp/codex-trust-test          │
╰───────────────────────────────────────────────────╯

  Tip: New Try the Codex App with 2x rate limits until April 2nd.

› Find and fix a bug in @filename

  ? for shortcuts                                            100% context left
```

### 正常就绪状态关键特征

| 特征 | 值 |
|------|-----|
| 版本框 | `╭──────` 开头的 box |
| 版本标识 | `>_ OpenAI Codex` |
| 输入提示 | `› Find and fix a bug in @filename` |
| 快捷键提示 | `? for shortcuts` |
| 上下文指示 | `100% context left` |

## 区分方法

### 信任确认界面独有特征

1. 包含 `Do you trust the contents of this directory?`
2. 包含 `1. Yes, continue`
3. 包含 `2. No, quit`
4. 包含 `Press enter to continue`

### 正常就绪状态独有特征

1. 包含 `>_ OpenAI Codex`
2. 包含 `? for shortcuts`
3. 包含 `context left`
4. 包含 `Find and fix a bug`（默认 placeholder）

## 当前 detect_ready() 问题

当前实现（`src/agent_mod/adapter/codex.rs:92-97`）：

```rust
fn detect_ready(&self, terminal_output: &str) -> bool {
    terminal_output.contains("codex")
        || terminal_output.contains("Ready")
        || terminal_output.contains(">")
}
```

**问题**：信任确认界面包含 `>` 字符（如 `> You are in`），导致误判为就绪。

## 建议修复方案

```rust
fn detect_ready(&self, terminal_output: &str) -> bool {
    // 排除信任确认界面
    if terminal_output.contains("Do you trust the contents of this directory?")
        || terminal_output.contains("1. Yes, continue")
    {
        return false;
    }

    // 检测正常就绪状态
    terminal_output.contains(">_ OpenAI Codex")
        || terminal_output.contains("? for shortcuts")
        || terminal_output.contains("context left")
}
```

### 方案优点

1. **明确排除**：先检查是否为信任确认界面
2. **精确匹配**：使用 Codex 特有的 UI 元素（`>_ OpenAI Codex`）
3. **多重验证**：`? for shortcuts` 和 `context left` 是正常界面独有的

## 测试用例建议

```rust
#[test]
fn test_detect_ready_excludes_trust_dialog() {
    let adapter = CodexAdapter;
    let trust_dialog = r#"> You are in /tmp/test

  Do you trust the contents of this directory? Working with untrusted contents
  comes with higher risk of prompt injection.

› 1. Yes, continue
  2. No, quit

  Press enter to continue"#;

    assert!(!adapter.detect_ready(trust_dialog));
}

#[test]
fn test_detect_ready_normal_state() {
    let adapter = CodexAdapter;
    let normal = r#"╭───────────────────────────────────────────────────╮
│ >_ OpenAI Codex (v0.104.0)                        │
│                                                   │
│ model:     gpt-5.3-codex xhigh   /model to change │
│ directory: /tmp/test                              │
╰───────────────────────────────────────────────────╯

› Find and fix a bug in @filename

  ? for shortcuts                                            100% context left"#;

    assert!(adapter.detect_ready(normal));
}
```
