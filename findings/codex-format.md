# OpenAI Codex CLI TUI 渲染格式研究

## 概述

OpenAI Codex CLI 使用 Rust 实现，基于 ratatui 0.29.0 构建全屏 TUI。源码位于 `codex-rs/tui/` 目录。

## 1. 消息边界标记

### 消息类型区分

Codex 使用 `HistoryCell` trait 作为核心抽象，不同消息类型有独立的 Cell 实现：

| Cell 类型 | 用途 |
|-----------|------|
| `UserHistoryCell` | 用户输入消息 |
| `AgentMessageCell` | AI 助手响应 |
| `ExecCell` | 命令执行（含输出） |
| `McpToolCallCell` | MCP 工具调用 |
| `WebSearchCell` | 网页搜索操作 |
| `PlanUpdateCell` | 计划更新 |
| `SessionHeaderHistoryCell` | 会话配置信息 |
| `FinalMessageSeparator` | 回合分隔符（含耗时/指标） |

### 视觉区分

- **用户消息**: 使用 `user_message_style()` 函数，根据终端背景自适应：
  - 深色背景：白色叠加 12% 透明度
  - 浅色背景：黑色叠加 4% 透明度
- **AI 消息**: 可选 bullet 前缀
- **回合分隔**: `FinalMessageSeparator` 显示耗时和指标

### 渲染架构

```
ChatWidget
├── active_cell (当前流式消息，可变)
└── committed HistoryCell[] (已完成的历史消息)
```

## 2. 状态指示

### StatusIndicatorWidget

位于 composer 上方，显示 agent 忙碌状态：

```
[动画] Working... 1m 23s  (esc to interrupt)
 └ 详情文本（最多3行，超出显示省略号）
```

**组件**:
- 动画 spinner（36 帧，80ms/帧）
- 可配置标题（默认 "Working"）
- 计时器（支持暂停/恢复）
- 中断提示 "esc to interrupt"
- 可选内联上下文消息

**动画变体**:
- `FRAMES_DEFAULT`, `FRAMES_CODEX`, `FRAMES_OPENAI`
- `FRAMES_BLOCKS`, `FRAMES_DOTS`, `FRAMES_HASH`
- `FRAMES_HBARS`, `FRAMES_VBARS`, `FRAMES_SHAPES`, `FRAMES_SLUG`

### 时间格式化

```rust
// fmt_elapsed_compact() 输出格式
< 60s:  "0s", "59s"
分钟:   "1m 00s", "59m 59s"
小时:   "1h 00m 00s", "25h 02m 03s"
```

### 处理中 vs 等待输入

- **处理中**: `StatusIndicatorWidget` 可见，显示动画和计时
- **等待输入**: 无状态指示器，显示输入 composer

## 3. 权限请求格式

### ApprovalRequest 类型

```rust
enum ApprovalRequest {
    Exec {
        command: Vec<String>,
        network_context: Option<...>,
        permission_amendments: Option<...>,
    },
    ApplyPatch {
        changes: Vec<FileChange>,
    },
    McpElicitation {
        server_name: String,
        message: String,
    },
}
```

### 视觉格式

**命令执行请求**:
```
[原因说明（如有）]
[权限规则 - 青色显示]
$ command args...
```

**文件修改请求**:
```
[原因说明]
[Diff 摘要]
```

**MCP 请求**:
```
[服务器名称]
[消息内容]
```

### 键盘快捷键

| 按键 | 操作 |
|------|------|
| `y` | 批准 |
| `n` | 拒绝 |
| `a` | 本次会话批准 |
| `p` | 修改执行策略 |
| `Esc` | 取消 |

### 决策类型

- `Approved` - 单次批准
- `ApprovedForSession` - 会话期间批准
- `Abort` - 中止

## 4. 错误显示格式

### 命令执行状态

```rust
struct CommandOutput {
    exit_code: i32,
    aggregated_output: String,  // stderr + stdout 交错
    formatted_output: String,   // 模型可见输出
}
```

### 视觉指示

- **成功**: 绿色粗体 bullet
- **失败**: 红色粗体 bullet
- **运行中**: 动画 spinner

### 输出截断

```rust
TOOL_CALL_MAX_LINES: 5        // agent 命令
USER_SHELL_TOOL_CALL_MAX_LINES: 50  // 用户命令
```

使用 `truncate_lines_middle()` 保留头尾，中间截断。

### 输出格式

```
• command args...
  └ output line 1
  │ output line 2
  │ ...
```

## 5. TUI 组件

### 技术栈

| 库 | 版本 | 用途 |
|----|------|------|
| ratatui | 0.29.0 (patched) | TUI 框架 |
| crossterm | custom fork | 终端控制 |
| syntect | 5.x | 语法高亮 |
| ansi-to-tui | 7.0.0 | ANSI 转换 |
| vt100 | 0.16.2 | 终端模拟 |
| textwrap | workspace | 文本换行 |
| unicode-width | workspace | Unicode 宽度 |

### 核心模块

```
tui/src/
├── app.rs              # 应用主逻辑
├── tui.rs              # 终端管理
├── chatwidget.rs       # 聊天组件
├── history_cell.rs     # 历史消息渲染
├── status_indicator_widget.rs  # 状态指示器
├── bottom_pane/
│   └── approval_overlay.rs  # 权限请求 UI
├── exec_cell/
│   ├── model.rs        # 执行模型
│   └── render.rs       # 执行渲染
├── markdown_render.rs  # Markdown 渲染
├── diff_render.rs      # Diff 渲染
├── frames.rs           # 动画帧
└── style.rs            # 样式定义
```

### 渲染特性

1. **自适应换行**: `adaptive_wrap_line()` 处理终端宽度
2. **Unicode 支持**: 正确处理 CJK 字符（2列）和 Tab（4列）
3. **主题感知**: 根据终端背景自动调整颜色
4. **颜色级别**: 支持 truecolor、256色、16色终端
5. **语法高亮**: 使用 syntect 处理代码块

### 事件架构

```rust
enum AppEvent {
    // 会话管理
    NewSession, ClearUi, OpenResumePicker, ForkCurrentSession,

    // 退出处理
    Exit(ExitMode), FatalExitRequest,

    // 权限/审批
    OpenFullAccessConfirmation,
    OpenApprovalsPopup,
    FullScreenApprovalRequest(ApprovalRequest),
    UpdateAskForApprovalPolicy,

    // 模型/推理
    UpdateModel, UpdateReasoningEffort, PersistModelSelection,

    // 用户交互
    VoiceTranscription, FileSearch, UrlHandling, FeedbackSubmission,
}
```

## 6. CAM 集成建议

### 状态检测

Codex 的状态指示相对明确：
- `StatusIndicatorWidget` 可见 → 处理中
- 无状态指示器 → 等待输入
- `ApprovalOverlay` 可见 → 等待权限确认

### 权限请求识别

关键特征：
- `$ ` 前缀表示命令执行请求
- 青色文本表示权限规则
- `y/n/a/p` 快捷键提示

### 消息边界

- `FinalMessageSeparator` 标记回合结束
- 不同 Cell 类型有不同的视觉样式

### 与 Claude Code 的差异

| 特性 | Codex | Claude Code |
|------|-------|-------------|
| TUI 框架 | ratatui (Rust) | Ink (React/Node) |
| 状态指示 | StatusIndicatorWidget | 文本动画 |
| 权限请求 | ApprovalOverlay | 内联提示 |
| 消息分隔 | FinalMessageSeparator | 无明确分隔 |
| 动画 | 36帧 ASCII 动画 | 文本 spinner |

## 参考资料

- [OpenAI Codex GitHub](https://github.com/openai/codex)
- [ratatui 文档](https://ratatui.rs/)
- 源码路径: `codex-rs/tui/src/`
