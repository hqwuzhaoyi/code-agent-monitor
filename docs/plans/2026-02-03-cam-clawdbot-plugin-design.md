# CAM Clawdbot Plugin 设计文档

> **状态**: 已完成
> **日期**: 2026-02-03

## 背景

测试发现 Clawdbot 的 main agent 没有使用 CAM 的 MCP 工具，而是直接操作 tmux，导致：
- agents.json 始终为空（CAM 的 AgentManager 未被调用）
- 无法追踪和管理 agent
- 行为不符合预期（过于主动、不询问缺失参数）

## 目标

让 Clawdbot 通过 CAM 的工具来管理 agent，实现：
1. 所有 agent 操作通过 CAM 追踪
2. agents.json 正确记录运行中的 agent
3. 行为符合预期（询问缺失参数、区分查看和操作）

## 架构

```
┌─────────────────────────────────────────────────────────┐
│                     Clawdbot Agent                       │
│                                                         │
│  用户消息 → 意图识别 → 选择工具 → 执行 → 返回结果        │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                    CAM Plugin                            │
│                                                         │
│  cam_agent_start    cam_agent_stop    cam_agent_list    │
│  cam_agent_send     cam_agent_status  cam_agent_logs    │
│  cam_list_sessions  cam_resume_session                  │
│  cam_list_agents    cam_kill_agent    cam_send_input    │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼ spawn + stdio
┌─────────────────────────────────────────────────────────┐
│                 CAM Binary (MCP Server)                  │
│                                                         │
│  /Users/admin/workspace/code-agent-monitor/target/      │
│  release/cam serve                                      │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│              tmux sessions + agents.json                 │
└─────────────────────────────────────────────────────────┘
```

**数据流**：
1. 用户发送自然语言消息给 Clawdbot
2. Agent 根据 SKILL.md 指引识别意图，选择 `cam_*` 工具
3. Plugin 将工具调用转换为 MCP 请求，spawn CAM 进程
4. CAM 执行操作，返回 JSON 结果
5. Plugin 将结果返回给 Agent，Agent 生成人类可读响应

## 文件结构

```
~/clawd/plugins/cam/
├── package.json          # Plugin 元数据
└── src/
    └── index.ts          # 工具注册

~/clawd/skills/code-agent-monitor/
└── SKILL.md              # 行为指引（更新）
```

## Plugin 实现

### package.json

```json
{
  "name": "cam-plugin",
  "version": "1.0.0",
  "openclaw": {
    "extensions": ["./src/index.ts"]
  }
}
```

### src/index.ts

```typescript
import { Type } from "@sinclair/typebox";
import { spawn } from "child_process";

const CAM_BIN = "/Users/admin/workspace/code-agent-monitor/target/release/cam";

// 通用 MCP 调用函数
async function callCamMcp(toolName: string, args: object): Promise<object> {
  return new Promise((resolve, reject) => {
    const request = JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: { name: toolName, arguments: args }
    });

    const proc = spawn(CAM_BIN, ["serve"], { timeout: 5000 });
    let stdout = "";
    let stderr = "";

    proc.stdout.on("data", (data) => stdout += data);
    proc.stderr.on("data", (data) => stderr += data);

    proc.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`CAM exited with code ${code}: ${stderr}`));
        return;
      }
      try {
        const response = JSON.parse(stdout);
        if (response.error) {
          reject(new Error(response.error.message));
        } else {
          resolve(response.result);
        }
      } catch (e) {
        reject(new Error(`Invalid JSON response: ${stdout}`));
      }
    });

    proc.stdin.write(request);
    proc.stdin.end();
  });
}

export default function (api) {
  // Agent 生命周期管理
  api.registerTool({
    name: "cam_agent_start",
    description: "启动新的 Claude Code agent。必须提供 project_path。",
    parameters: Type.Object({
      project_path: Type.String({ description: "项目目录路径（必填）" }),
      agent_type: Type.Optional(Type.String({ description: "代理类型: claude/opencode/codex，默认 claude" })),
      prompt: Type.Optional(Type.String({ description: "初始提示词" })),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("agent_start", {
          agent_type: params.agent_type || "claude",
          project_path: params.project_path,
          prompt: params.prompt,
        });
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_start failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_agent_stop",
    description: "停止一个运行中的 agent",
    parameters: Type.Object({
      agent_id: Type.String({ description: "Agent ID (如 cam-xxxxxxxx)" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("agent_stop", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_stop failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_agent_list",
    description: "列出所有 CAM 管理的运行中 agent",
    parameters: Type.Object({}),
    async execute() {
      try {
        const result = await callCamMcp("agent_list", {});
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_list failed", { error });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // Agent 交互
  api.registerTool({
    name: "cam_agent_send",
    description: "向 agent 发送消息/输入（用于确认、拒绝或发送指令）",
    parameters: Type.Object({
      agent_id: Type.String({ description: "Agent ID" }),
      message: Type.String({ description: "要发送的消息" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("agent_send", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_send failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_agent_status",
    description: "获取 agent 的结构化状态（是否等待输入、最近工具调用等）。用于诊断 agent 状态。",
    parameters: Type.Object({
      agent_id: Type.String({ description: "Agent ID" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("agent_status", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_status failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_agent_logs",
    description: "获取 agent 的终端输出日志。只用于查看，不发送任何输入。",
    parameters: Type.Object({
      agent_id: Type.String({ description: "Agent ID" }),
      lines: Type.Optional(Type.Number({ description: "返回行数，默认 50" })),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("agent_logs", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_agent_logs failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // 会话管理
  api.registerTool({
    name: "cam_list_sessions",
    description: "列出历史 Claude Code 会话（包括 CAM 启动的和 Mac 直接打开的）。可用于恢复之前的工作。",
    parameters: Type.Object({
      project_path: Type.Optional(Type.String({ description: "按项目路径过滤（模糊匹配）" })),
      days: Type.Optional(Type.Number({ description: "只返回最近 N 天的会话" })),
      limit: Type.Optional(Type.Number({ description: "限制返回数量" })),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("list_sessions", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_list_sessions failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_resume_session",
    description: "恢复一个历史会话到 tmux，并注册到 CAM 管理。支持恢复任意 Claude Code 会话。",
    parameters: Type.Object({
      session_id: Type.String({ description: "会话 ID" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("resume_session", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_resume_session failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  // 进程管理（低级）
  api.registerTool({
    name: "cam_list_agents",
    description: "列出系统中所有 Claude Code 进程（包括非 CAM 管理的）",
    parameters: Type.Object({}),
    async execute() {
      try {
        const result = await callCamMcp("list_agents", {});
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_list_agents failed", { error });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_kill_agent",
    description: "终止一个 agent 进程（通过 PID）",
    parameters: Type.Object({
      pid: Type.Number({ description: "进程 ID" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("kill_agent", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_kill_agent failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });

  api.registerTool({
    name: "cam_send_input",
    description: "向 tmux 会话发送原始输入",
    parameters: Type.Object({
      tmux_session: Type.String({ description: "tmux 会话名称" }),
      input: Type.String({ description: "要发送的输入" }),
    }),
    async execute(_id, params) {
      try {
        const result = await callCamMcp("send_input", params);
        return { content: [{ type: "text", text: JSON.stringify(result) }] };
      } catch (error) {
        api.logger.error("cam_send_input failed", { error, params });
        return { content: [{ type: "text", text: JSON.stringify({ error: true, message: error.message }) }] };
      }
    },
  });
}
```

## SKILL.md 更新

```markdown
---
name: code-agent-monitor
description: 监控和管理 AI 编码代理进程。使用 cam_* 工具操作。
---

# Code Agent Monitor (CAM)

使用 `cam_*` 系列工具管理 Claude Code agents。

## 工具选择指南

| 用户意图 | 使用工具 |
|----------|----------|
| 启动新 agent | `cam_agent_start` |
| 停止 agent | `cam_agent_stop` |
| 查看运行中的 agent | `cam_agent_list` |
| 发送输入/确认 | `cam_agent_send` |
| 查看状态（是否等待输入） | `cam_agent_status` |
| 查看终端输出 | `cam_agent_logs` |
| 查看历史会话 | `cam_list_sessions` |
| 恢复历史会话 | `cam_resume_session` |

## 行为规范

### 必须询问的情况

1. **启动时缺少 project_path**：询问"要在哪个项目启动？"
2. **多个 agent 时操作不明确**：列出选项让用户选择
3. **恢复会话但未指定**：列出最近 3 个会话供选择

### 查看 vs 操作

- **"看看输出"、"什么情况"** → 只调用 `cam_agent_status` 或 `cam_agent_logs`，不发送任何输入
- **"y"、"继续"、"好的"** → 调用 `cam_agent_send` 发送确认

### 上下文推断

- 单 agent 场景：所有操作默认指向它
- 多 agent 场景：优先最近交互的，不确定时询问
- 无 agent 场景：用户说"继续"时，询问是否恢复历史会话
```

## 开发与安装步骤

### 开发阶段（在项目目录）

```bash
# 1. 在项目目录创建 plugin 结构
mkdir -p /Users/admin/workspace/code-agent-monitor/plugins/cam/src

# 2. 开发 package.json 和 src/index.ts

# 3. 本地测试（可选）
cd /Users/admin/workspace/code-agent-monitor/plugins/cam
npm install  # 如果有依赖
```

### 部署阶段（软链接到 clawd）

```bash
# 4. 创建软链接（推荐，修改后立即生效）
ln -s /Users/admin/workspace/code-agent-monitor/plugins/cam ~/clawd/plugins/cam

# 或者复制（需要每次修改后重新复制）
# cp -r /Users/admin/workspace/code-agent-monitor/plugins/cam ~/clawd/plugins/

# 5. 安装 plugin
clawdbot plugins install ~/clawd/plugins/cam

# 6. 验证 plugin 已加载
clawdbot plugins list | grep cam

# 7. 诊断检查
clawdbot plugins doctor
```

## 测试用例

### Agent 生命周期

| 编号 | 场景 | 测试命令 | 预期行为 | 验证点 |
|------|------|----------|----------|--------|
| 1.1 | 启动 agent（明确路径） | `"在 /tmp 启动 Claude"` | 调用 `cam_agent_start` | agents.json 有记录，tmux session 存在 |
| 1.2 | 启动 agent（缺少路径） | `"开个新的"` | 询问项目路径 | 不直接执行，等待用户回复 |
| 1.3 | 停止 agent | `"停掉"` | 调用 `cam_agent_stop` | agents.json 移除记录，tmux session 消失 |
| 1.4 | 列出 agent | `"现在跑着什么"` | 调用 `cam_agent_list` | 返回运行中的 agent 列表 |

### Agent 交互

| 编号 | 场景 | 测试命令 | 预期行为 | 验证点 |
|------|------|----------|----------|--------|
| 2.1 | 发送确认 y | `"y"` | 调用 `cam_agent_send` | 输入被发送到 agent |
| 2.2 | 发送拒绝 n | `"n"` | 调用 `cam_agent_send` | 输入被发送到 agent |
| 2.3 | 自然语言确认 | `"好的，继续"` | 调用 `cam_agent_send` | 输入被发送，不是查看状态 |
| 2.4 | 查看状态 | `"什么情况"` | 调用 `cam_agent_status` | 只返回状态，不发送输入 |
| 2.5 | 查看输出 | `"看看输出"` | 调用 `cam_agent_logs` | 只返回日志，不发送输入 |

### 会话管理

| 编号 | 场景 | 测试命令 | 预期行为 | 验证点 |
|------|------|----------|----------|--------|
| 3.1 | 列出历史会话 | `"继续之前的"` | 调用 `cam_list_sessions` | 包含所有 Claude Code 会话（CAM 启动的 + Mac 直接打开的） |
| 3.2 | 恢复指定会话 | `"1"` (选择后) | 调用 `cam_resume_session` | agents.json 有记录，tmux session 存在 |
| 3.3 | 按项目过滤会话 | `"看看 myapp 项目的会话"` | 调用 `cam_list_sessions` + project_path | 只返回匹配的会话 |

### 多 Agent 管理

| 编号 | 场景 | 测试命令 | 预期行为 | 验证点 |
|------|------|----------|----------|--------|
| 4.1 | 启动第二个 agent | `"再启动一个在 /var/tmp"` | 调用 `cam_agent_start` | 两个 agent 都在 agents.json |
| 4.2 | 列出多个 agent | `"现在有几个在跑"` | 调用 `cam_agent_list` | 返回编号列表 |
| 4.3 | 指定 agent 查看 | `"看看 1 的输出"` | 调用 `cam_agent_logs` + 正确 agent_id | 返回指定 agent 的日志 |
| 4.4 | 指定 agent 停止 | `"把 2 停了"` | 调用 `cam_agent_stop` + 正确 agent_id | 只停止第二个 |

### 异常处理

| 编号 | 场景 | 测试命令 | 预期行为 | 验证点 |
|------|------|----------|----------|--------|
| 5.1 | 无 agent 时操作 | `"看看输出"` (无 agent) | 友好提示 | 返回"目前没有运行中的任务" |
| 5.2 | 无效路径启动 | `"在 /nonexistent 启动"` | 返回错误或询问确认 | 不静默失败 |
| 5.3 | 无效 agent_id | `"停掉 cam-invalid"` | 返回错误 | 提示 agent 不存在 |

### 进程管理（低级）

| 编号 | 场景 | 测试命令 | 预期行为 | 验证点 |
|------|------|----------|----------|--------|
| 6.1 | 列出系统进程 | `"列出所有 Claude 进程"` | 调用 `cam_list_agents` | 返回 PID 列表 |
| 6.2 | 终止进程 | `"杀掉进程 12345"` | 调用 `cam_kill_agent` | 进程被终止 |

## 诊断检查项

`clawdbot plugins doctor` 应检查：

| 检查项 | 预期结果 |
|--------|----------|
| Plugin 已加载 | `cam-plugin: loaded` |
| CAM 二进制存在 | `/Users/admin/workspace/code-agent-monitor/target/release/cam` 可执行 |
| CAM MCP 响应正常 | `cam serve` 能正确响应 JSON-RPC |
| tmux 可用 | `/opt/homebrew/bin/tmux` 存在 |

## 实现任务

- [x] Task 1: 创建 plugin 目录结构
- [x] Task 2: 实现 package.json
- [x] Task 3: 实现 src/index.ts（11 个工具）
- [x] Task 4: 更新 SKILL.md
- [x] Task 5: 安装并验证 plugin
- [x] Task 6: 执行完整测试用例
