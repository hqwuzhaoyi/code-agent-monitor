# OpenAI Codex CLI 调研报告

## 概述

OpenAI Codex CLI 是一个开源的终端编码代理，使用 Rust 构建。它可以在本地运行，读取、修改和执行代码。

- **GitHub 仓库**: https://github.com/openai/codex
- **官方文档**: https://developers.openai.com/codex/cli/
- **安装方式**: `npm i -g @openai/codex` 或 `brew install --cask codex`
- **开源协议**: Apache-2.0

## Hooks 机制

### 通知系统 (notify)

Codex CLI 提供了一个 `notify` 配置项，可以在特定事件发生时调用外部命令：

```toml
# ~/.codex/config.toml
notify = ["your-command", "arg1", "arg2"]
```

**工作方式**：
- 配置的命令会接收来自 Codex 的 JSON payload
- 可用于实现自定义通知逻辑

**与 Claude Code hooks 对比**：
| 特性 | Claude Code | Codex CLI |
|------|-------------|-----------|
| 事件类型 | session_start, stop, notification, PreToolUse, PostToolUse | 通过 notify 配置统一处理 |
| 配置方式 | ~/.claude/settings.json 中的 hooks 数组 | ~/.codex/config.toml 中的 notify 数组 |
| 事件粒度 | 细粒度（每个事件类型单独配置） | 粗粒度（统一的 notify 命令） |

### 结论

Codex CLI 的 hooks 机制相对简单，只有一个 `notify` 配置项。**没有** Claude Code 那样丰富的事件类型（如 session_start, stop, PreToolUse, PostToolUse 等）。

## 配置文件

### 位置

- **用户级配置**: `~/.codex/config.toml`
- **项目级配置**: `.codex/config.toml`（项目根目录）
- **会话存储**: `~/.codex/sessions/`
- **日志目录**: `~/.codex/log/` (可通过 `log_dir` 配置)

### 配置格式

使用 TOML 格式，主要配置项包括：

```toml
# 模型配置
model = "gpt-5-codex"
model_provider = "openai"

# 审批策略
approval_policy = "on-request"  # untrusted | on-request | never

# 沙箱模式
sandbox_mode = "workspace-write"  # read-only | workspace-write | danger-full-access

# 通知命令
notify = ["your-notify-command"]

# TUI 配置
[tui]
notifications = true
animations = true

# 功能开关
[features]
multi_agent = false
shell_tool = true
web_search = true
```

### JSON Schema

官方提供了配置文件的 JSON Schema：
https://developers.openai.com/codex/config-schema.json

## 终端交互模式

### 交互式 TUI

Codex CLI 使用全屏终端 UI (TUI)：

```bash
codex                           # 启动交互式会话
codex "Explain this codebase"   # 带初始提示启动
```

**TUI 特征**：
- 全屏模式（使用 alternate screen）
- 语法高亮的代码块和 diff
- 支持主题切换 (`/theme`)
- 支持图片输入
- 草稿历史导航（Up/Down 键）

### 非交互式模式

```bash
codex exec "task description"           # 非交互式执行
codex exec --json "task"                # JSON Lines 输出
codex exec --full-auto "task"           # 自动审批模式
codex exec resume --last "continue"     # 恢复上次会话
```

**输出格式**：
- 进度信息输出到 stderr
- 最终结果输出到 stdout
- `--json` 模式输出 JSON Lines 流

### 审批模式

| 模式 | 说明 |
|------|------|
| Auto (默认) | 可以在工作目录内读写文件和执行命令，超出范围需确认 |
| Read-only | 只能浏览文件，修改和命令需要确认 |
| Full Access | 完全访问权限，包括网络访问，无需确认 |

可通过 `/permissions` 命令在会话中切换模式。

## 状态检测

### 终端 UI 状态

Codex CLI 的 TUI 状态可能包括：
- 等待用户输入（composer 激活）
- 正在处理（streaming 响应）
- 等待审批（approval prompt）
- 空闲状态

**检测方法**：
1. 使用 `codex exec --json` 模式获取结构化事件流
2. 监控 JSON Lines 输出中的事件类型：
   - `thread.started` - 线程开始
   - `turn.started` - 轮次开始
   - `turn.completed` - 轮次完成
   - `turn.failed` - 轮次失败
   - `item.*` - 各种项目事件

### SDK 集成

Codex 提供 TypeScript SDK 用于程序化控制：

```typescript
import { Codex } from "@openai/codex-sdk";

const codex = new Codex();
const thread = codex.startThread();
const result = await thread.run("Make a plan...");
```

## CAM 集成建议

### 监控方案

由于 Codex CLI 没有 Claude Code 那样的 hooks 机制，CAM 需要采用不同的监控策略：

1. **终端快照方式**（当前 CAM 方案）
   - 使用 tmux capture-pane 获取终端内容
   - 使用 Haiku API 分析终端状态
   - 适用于交互式 TUI 模式

2. **JSON 事件流方式**（推荐用于自动化）
   - 使用 `codex exec --json` 模式
   - 解析 JSON Lines 事件流
   - 可获取结构化的状态信息

3. **SDK 集成方式**（最完整）
   - 使用 TypeScript SDK 控制 Codex
   - 可获取完整的线程和轮次信息
   - 需要额外的 Node.js 运行时

### 配置检测

```bash
# 配置文件位置
~/.codex/config.toml

# 会话目录
~/.codex/sessions/

# 日志目录
~/.codex/log/
```

### 进程识别

```bash
# Codex CLI 进程名
codex

# 可能的进程参数
codex exec --json ...
codex --model gpt-5-codex ...
```

## 与 Claude Code 对比

| 特性 | Claude Code | Codex CLI |
|------|-------------|-----------|
| Hooks 机制 | 丰富（session_start, stop, notification, PreToolUse, PostToolUse） | 简单（仅 notify） |
| 配置格式 | JSON | TOML |
| 配置位置 | ~/.claude/ | ~/.codex/ |
| 非交互模式 | 无原生支持 | codex exec |
| SDK | 无 | TypeScript SDK |
| 事件流 | 无 | JSON Lines (--json) |
| 开源 | 否 | 是 (Apache-2.0) |

## 参考链接

- [Codex CLI 文档](https://developers.openai.com/codex/cli/)
- [配置参考](https://developers.openai.com/codex/config-reference)
- [非交互模式](https://developers.openai.com/codex/noninteractive)
- [Codex SDK](https://developers.openai.com/codex/sdk)
- [GitHub 仓库](https://github.com/openai/codex)
