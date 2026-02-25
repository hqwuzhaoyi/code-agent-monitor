# Claude Code 终端输出格式研究报告

## 概述

本报告研究 Claude Code 的终端输出格式，为 CAM 的消息边界识别和状态检测提供参考。

---

## 1. 消息边界标记

### 1.1 `⏺` 符号（U+23FA BLACK CIRCLE FOR RECORD）

**用途**：标记 Agent 的回复/操作开始

**出现位置**：
- Agent 开始新的回复时
- 工具调用结果显示时
- Skill 加载成功时

**示例**：
```
⏺ Skill(brainstorming)
  ⎿  Successfully loaded skill

⏺ 让我先了解一下当前项目的上下文。

⏺ Read 1 file (ctrl+o to expand)

⏺ 这是一个新项目。让我开始头脑风暴。
```

**识别规则**：
- `⏺` 在行首表示新的 Agent 回复开始
- 可用于判断问题之后是否有新的 Agent 回复（用于去重）

### 1.2 用户输入提示符

**主提示符**：`❯`（U+276F HEAVY RIGHT-POINTING ANGLE QUOTATION MARK ORNAMENT）

**备用提示符**：`>`（ASCII 大于号）

**识别正则**：
```rust
r"(?m)^[❯>]\s*$"
```

**示例**：
```
❯ 使用 brainstorm 模式，帮我创建一个 React todo list 项目。

❯
```

**注意**：
- 空的 `❯` 行表示等待用户输入
- 有内容的 `❯` 行表示用户已输入的命令

---

## 2. 状态指示器

### 2.1 处理中状态

Claude Code 使用多种动画状态词表示正在处理：

| 状态词 | 含义 |
|--------|------|
| `Thinking…` | 思考中 |
| `Brewing…` | 生成中 |
| `Hatching…` | 孵化中（启动） |
| `Grooving…` | 处理中 |
| `Streaming…` | 流式输出中 |

**状态指示器符号**：
- `✶`（U+2736 SIX POINTED BLACK STAR）- Brewing 状态
- `✻`（U+273B TEARDROP-SPOKED ASTERISK）- 其他处理状态
- `✢`（U+2722 FOUR TEARDROP-SPOKED ASTERISK）- Processing 状态

**旋转动画字符**：
```
✢✻✶✽◐◑◒◓⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏
```

**示例**：
```
✶ Brewing…
✻ Thinking…
✢ Processing your request
```

### 2.2 判断 Agent 是否在处理中

**CAM 的做法**：使用 AI（Haiku）判断，不硬编码模式

```rust
// src/ai/extractor.rs
pub fn is_agent_processing(terminal_snapshot: &str) -> AgentStatus {
    // 使用 AI 分析终端输出
    // 返回: Processing / WaitingForInput / DecisionRequired / Unknown
}
```

**AI 判断规则**（来自 prompt）：
```
- PROCESSING: 如果看到以下任一指示器：
  * 带省略号的状态词（如 Thinking…、Brewing…、Hatching…）
  * 任何 "动词ing…" 或 "动词ing..." 格式的状态提示
  * 括号内的运行提示（如 (running stop hook)）
  * 旋转动画字符
  * 进度条（注意：底部状态栏的上下文使用量不算）
- WAITING: 等待用户输入
- DECISION: 等待用户做关键决策
```

---

## 3. 权限请求格式

### 3.1 Hook 事件格式

Claude Code 通过 Hook 发送权限请求事件：

```json
{
  "event": "PreToolUse",
  "tool_name": "Bash",
  "tool_input": {
    "command": "npm install && npm run build"
  },
  "cwd": "/path/to/project",
  "session_id": "abc123"
}
```

### 3.2 终端显示格式

权限请求在终端中的显示：

```
⏺ Bash(npm install && npm run build)
  Allow this command? [Y/n]
```

或更详细的格式：

```
⏺ Bash
  ⎿  Command: npm install && npm run build

  Allow? [Y/n]
```

### 3.3 文件操作请求

```
⏺ Write(src/utils.rs)
  ⎿  Creating new file

  Allow? [Y/n]
```

```
⏺ Edit(src/main.rs)
  ⎿  Modifying lines 10-25

  Allow? [Y/n]
```

---

## 4. 错误输出格式

### 4.1 编译错误

```
⏺ Bash(cargo build)
  ⎿  error[E0382]: borrow of moved value: `x`
     --> src/main.rs:10:5
      |
   10 |     println!("{}", x);
      |                    ^ value borrowed here after move
```

### 4.2 运行时错误

```
⏺ Bash(npm test)
  ⎿  FAIL src/App.test.js
     ● Test suite failed to run

     Cannot find module './utils'
```

### 4.3 工具调用失败

```
⏺ Read(nonexistent.txt)
  ⎿  Error: File not found: nonexistent.txt
```

---

## 5. 多选项问题格式

### 5.1 字母选项格式

```
第一个问题：项目的主要用途是什么？

A) 个人学习项目 - 用于学习 React 基础，功能简单即可
B) 作品集展示 - 需要精美的 UI 和完整功能，展示技术能力
C) 实际使用的工具 - 真正用来管理日常任务，需要实用性
D) 其他 - 请说明
```

### 5.2 数字选项格式

```
Which option do you prefer?

1. Option A - Description
2. Option B - Description
3. Option C - Description
```

### 5.3 确认格式

```
Allow this command? [Y/n]
```

```
Continue? [y/N]
```

```
Are you sure? (yes/no)
```

### 5.4 用户选择方式

- 字母选项：输入 `A`、`B`、`C`、`D`
- 数字选项：输入 `1`、`2`、`3`
- 确认：输入 `y`、`n`、`yes`、`no`

---

## 6. 终端布局结构

### 6.1 完整终端示例

```
 ▐▛███▜▌   Claude Code v2.1.33
▝▜█████▛▘  Opus 4.6 · API Usage Billing
  ▘▘ ▝▝    ~/workspace

  Welcome to Opus 4.6

❯ 使用 brainstorm 模式，帮我创建一个 React todo list 项目。

⏺ Skill(brainstorming)
  ⎿  Successfully loaded skill

⏺ 让我先了解一下当前项目的上下文。

⏺ Read 1 file (ctrl+o to expand)

⏺ 这是一个新项目。让我开始头脑风暴。

  第一个问题：项目的主要用途是什么？

  A) 个人学习项目
  B) 作品集展示
  C) 实际使用的工具
  D) 其他

────────────────────────────────────────────────────────────────────────────────
❯
────────────────────────────────────────────────────────────────────────────────
  [Opus 4.6] ██░░░░░░░░ 22% | ⏱️  <1m
  workspace git:(main*)
  2 MCPs | 5 hooks
  ✓ Skill ×1 | ✓ Bash ×1
```

### 6.2 布局区域

| 区域 | 内容 |
|------|------|
| 顶部 Logo | ASCII art + 版本信息 |
| 对话区 | 用户输入 + Agent 回复 |
| 分隔线 | `─` 字符组成的水平线 |
| 输入区 | `❯` 提示符 |
| 状态栏 | 模型、上下文使用量、时间、git 状态、MCP/hooks 统计 |

---

## 7. CAM 识别策略

### 7.1 消息边界识别

```rust
// 检查问题之后是否有新的 Agent 回复
// 如果没有新的 ⏺ 回复 → has_question = true
fn has_new_agent_reply(content: &str, question_position: usize) -> bool {
    content[question_position..].contains("⏺")
}
```

### 7.2 状态检测

```rust
// 使用 AI 判断，不硬编码
pub fn is_processing(content: &str) -> bool {
    match is_agent_processing(content) {
        AgentStatus::Processing | AgentStatus::Running => true,
        AgentStatus::WaitingForInput => false,
        AgentStatus::DecisionRequired => false,
        AgentStatus::Unknown => false,
    }
}
```

### 7.3 提示符检测

```rust
// 检测空闲等待状态
static PROMPT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^[❯>]\s*$").expect("Invalid prompt regex"));

fn detect_ready(terminal_output: &str) -> bool {
    PROMPT_RE.is_match(terminal_output)
        || terminal_output.contains("Welcome to")
        || terminal_output.contains("Claude Code")
}
```

---

## 8. 已知问题和注意事项

### 8.1 Brewing 状态误触发

**问题**：通知在 Claude Code 显示 "✶ Brewing…" 时触发，导致通知内容是中间状态而非实际问题。

**解决方案**：
1. 使用 AI 检测 Brewing 状态并跳过通知
2. 不硬编码状态模式，完全依赖 AI 判断

### 8.2 Unicode 字符差异

| 字符 | Unicode | 名称 |
|------|---------|------|
| ✶ | U+2736 | SIX POINTED BLACK STAR |
| ✻ | U+273B | TEARDROP-SPOKED ASTERISK |
| ✢ | U+2722 | FOUR TEARDROP-SPOKED ASTERISK |
| ❯ | U+276F | HEAVY RIGHT-POINTING ANGLE QUOTATION MARK ORNAMENT |
| ⏺ | U+23FA | BLACK CIRCLE FOR RECORD |

### 8.3 上下文完整性

AI 提取问题时需要判断上下文是否完整：
- 如果问题引用了"这个"、"上面的"等内容，需要检查被引用内容是否可见
- 上下文不完整时自动扩展：80 行 → 150 行 → 300 行

---

## 9. 总结

| 元素 | 标识 | 用途 |
|------|------|------|
| `⏺` | Agent 回复开始 | 消息边界识别 |
| `❯` / `>` | 用户输入提示符 | 等待输入检测 |
| `✶✻✢` + `…` | 处理中状态 | 状态检测 |
| `[Y/n]` | 确认请求 | 权限请求识别 |
| `A) B) C)` / `1. 2. 3.` | 选项列表 | 问题类型识别 |

**核心原则**：CAM 使用 AI（Haiku）进行智能判断，不硬编码 Claude Code 特定模式，以保持对多种 AI 编码工具的兼容性。
