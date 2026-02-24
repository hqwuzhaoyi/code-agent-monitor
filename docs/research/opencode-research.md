# OpenCode 调研报告

## 概述

OpenCode 是一个开源的 AI 编码代理，提供终端 TUI 界面。项目最初由 opencode-ai 组织开发（Go 语言实现），后来被 Anomaly 公司接手并用 TypeScript 重写，现在是一个活跃的大型项目（109k stars）。

- **当前活跃仓库**: https://github.com/anomalyco/opencode
- **官方文档**: https://opencode.ai/docs
- **语言**: TypeScript (Bun 运行时)
- **许可证**: MIT

## Hooks 机制

OpenCode 有完善的 **Plugins 系统**，类似于 Claude Code 的 hooks 机制，但更加强大。

### Plugin 事件类型

OpenCode 支持以下事件类型：

| 类别 | 事件 |
|------|------|
| **Session** | `session.created`, `session.idle`, `session.status`, `session.error`, `session.compacted`, `session.deleted`, `session.diff`, `session.updated` |
| **Message** | `message.updated`, `message.removed`, `message.part.updated`, `message.part.removed` |
| **Tool** | `tool.execute.before`, `tool.execute.after` |
| **Permission** | `permission.asked`, `permission.replied` |
| **File** | `file.edited`, `file.watcher.updated` |
| **Shell** | `shell.env` |
| **TUI** | `tui.prompt.append`, `tui.command.execute`, `tui.toast.show` |
| **其他** | `command.executed`, `server.connected`, `todo.updated`, `lsp.*`, `installation.updated` |

### Plugin 示例

```typescript
// 发送通知的 Plugin
export const NotificationPlugin = async ({ project, client, $, directory, worktree }) => {
  return {
    event: async ({ event }) => {
      if (event.type === "session.idle") {
        await $`osascript -e 'display notification "Session completed!" with title "opencode"'`
      }
    },
  }
}

// 拦截工具执行
export const EnvProtection = async ({ project, client, $, directory, worktree }) => {
  return {
    "tool.execute.before": async (input, output) => {
      if (input.tool === "read" && output.args.filePath.includes(".env")) {
        throw new Error("Do not read .env files")
      }
    },
  }
}
```

### Plugin 加载位置

1. **项目级**: `.opencode/plugins/`
2. **全局级**: `~/.config/opencode/plugins/`
3. **npm 包**: 在 `opencode.json` 中配置

## 配置文件

### 位置和优先级

1. **Remote config** - `.well-known/opencode` (组织默认)
2. **Global config** - `~/.config/opencode/opencode.json`
3. **Custom config** - `OPENCODE_CONFIG` 环境变量
4. **Project config** - `opencode.json` (项目根目录)
5. **Inline config** - `OPENCODE_CONFIG_CONTENT` 环境变量

### 配置格式

```json
{
  "$schema": "https://opencode.ai/config.json",
  "theme": "opencode",
  "model": "anthropic/claude-sonnet-4-5",
  "autoupdate": true,
  "permission": {
    "*": "allow",
    "bash": "ask",
    "edit": "allow"
  },
  "plugin": ["opencode-helicone-session"],
  "mcp": {}
}
```

## 终端交互模式

### TUI 特征

OpenCode 使用 **Bubble Tea** 框架构建的 TUI 界面：

- 全屏 TUI 模式（非纯文本输出）
- 支持 Tab 键切换 Agent（build/plan）
- 支持 `/` 命令（如 `/help`, `/models`, `/sessions`）
- 支持 `@` 文件引用
- 支持 `!` 执行 shell 命令
- 快捷键前缀: `ctrl+x`

### 权限请求 UI

当需要用户确认时，OpenCode 提供三个选项：
- `once` - 仅批准本次
- `always` - 批准匹配模式的所有请求（当前会话）
- `reject` - 拒绝

### 非交互模式

```bash
# 单次提示模式
opencode -p "Explain the use of context in Go"

# JSON 输出
opencode -p "..." -f json

# 静默模式（无 spinner）
opencode -p "..." -q
```

## 状态检测

### 关键事件用于 CAM 监控

| 事件 | 用途 |
|------|------|
| `session.idle` | Agent 空闲，等待输入 |
| `session.status` | 状态变化 |
| `permission.asked` | 需要用户确认 |
| `permission.replied` | 用户已回复 |
| `tool.execute.before/after` | 工具执行前后 |

### 通过 Plugin 实现监控

可以创建一个 CAM Plugin 来监控 OpenCode 状态：

```typescript
export const CAMPlugin = async ({ project, client, $ }) => {
  return {
    event: async ({ event }) => {
      // 发送状态到 CAM
      if (event.type === "session.idle") {
        // 通知 CAM agent 空闲
      }
      if (event.type === "permission.asked") {
        // 通知 CAM 需要用户确认
      }
    },
    "permission.asked": async (input, output) => {
      // 可以在这里实现自动审批逻辑
    }
  }
}
```

## Server 模式

OpenCode 支持 client/server 架构：

```bash
opencode serve  # 启动服务器
opencode web    # 启动 Web 界面
```

配置：
```json
{
  "server": {
    "port": 4096,
    "hostname": "0.0.0.0",
    "mdns": true
  }
}
```

这意味着可以通过 HTTP API 与 OpenCode 交互，而不仅仅是 TUI。

## 与 Claude Code 对比

| 特性 | Claude Code | OpenCode |
|------|-------------|----------|
| Hooks 机制 | Shell 脚本 hooks | TypeScript Plugin 系统 |
| 事件类型 | 有限（PreToolUse, PostToolUse 等） | 丰富（30+ 事件类型） |
| 配置格式 | TOML (settings.json) | JSON/JSONC |
| 配置位置 | `~/.claude/` | `~/.config/opencode/` |
| 终端模式 | 纯文本 + TUI 混合 | 全屏 TUI |
| 权限系统 | 简单的 allow/deny | 细粒度 pattern matching |
| API/IPC | 无 | HTTP Server + SDK |

## CAM 集成建议

### 方案 1: Plugin 集成（推荐）

创建 `cam-opencode-plugin.ts`：

```typescript
import type { Plugin } from "@opencode-ai/plugin"

export const CAMPlugin: Plugin = async (ctx) => {
  const CAM_WEBHOOK = process.env.CAM_WEBHOOK_URL

  return {
    event: async ({ event }) => {
      if (["session.idle", "permission.asked", "session.error"].includes(event.type)) {
        await fetch(CAM_WEBHOOK, {
          method: "POST",
          body: JSON.stringify({
            event: event.type,
            data: event,
            agent: "opencode",
            cwd: ctx.directory
          })
        })
      }
    }
  }
}
```

### 方案 2: Server API 集成

使用 OpenCode SDK 连接到运行中的 OpenCode server：

```typescript
import { OpenCodeClient } from "@opencode-ai/sdk"

const client = new OpenCodeClient({ port: 4096 })
// 订阅事件...
```

### 方案 3: 终端监控（备选）

如果无法使用 Plugin，可以通过 tmux 监控终端输出，但需要处理 TUI 的 ANSI 转义序列。

## 数据存储

| 路径 | 说明 |
|------|------|
| `~/.config/opencode/opencode.json` | 全局配置 |
| `~/.config/opencode/plugins/` | 全局 Plugin |
| `~/.config/opencode/agents/` | 自定义 Agent |
| `~/.config/opencode/commands/` | 自定义命令 |
| `.opencode/` | 项目级配置和 Plugin |
| `opencode.json` | 项目配置文件 |

## 结论

OpenCode 的 Plugin 系统比 Claude Code 的 hooks 更强大和灵活：

1. **原生支持**: Plugin 可以直接订阅 `permission.asked` 事件，实现自动审批
2. **丰富事件**: 30+ 事件类型，覆盖所有关键状态变化
3. **Server 模式**: 可以通过 HTTP API 远程监控和控制
4. **TypeScript**: Plugin 用 TypeScript 编写，比 shell 脚本更强大

**推荐集成方式**: 开发一个 CAM OpenCode Plugin，通过 Plugin 事件系统实现状态监控和通知推送。
