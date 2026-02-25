# cam start 命令设计

## 概述

`cam start` 命令用于启动 AI 编码代理（Claude Code 或 Codex），并自动注册到 CAM 进行监控。

## 命令格式

```
cam start [OPTIONS] [PROMPT]
```

## 参数设计

### 位置参数

| 参数 | 类型 | 说明 |
|------|------|------|
| `PROMPT` | String (可选) | 初始 prompt，启动后自动发送给 agent |

### 选项参数

| 参数 | 短选项 | 类型 | 默认值 | 说明 |
|------|--------|------|--------|------|
| `--agent` | `-a` | String | `claude-code` | Agent 类型：`claude-code`, `codex` |
| `--cwd` | `-c` | Path | 当前目录 | 工作目录 |
| `--name` | `-n` | String | 自动生成 | tmux session 名称 |
| `--resume` | `-r` | String | - | 恢复指定 session ID |
| `--json` | - | Flag | false | 输出 JSON 格式 |

### 参数验证

1. `--agent` 必须是支持的类型之一
2. `--cwd` 必须是存在的目录
3. `--resume` 与 `PROMPT` 互斥（恢复会话时不能发送初始 prompt）

## 执行流程

```
┌─────────────────────────────────────────────────────────────┐
│                      cam start                               │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ 1. 参数验证                                                  │
│    - 检查 agent 类型是否支持                                  │
│    - 检查 cwd 是否存在                                        │
│    - 检查 resume 与 prompt 互斥                               │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. 检查依赖                                                  │
│    - tmux 是否可用                                           │
│    - agent 命令是否存在 (claude / codex)                     │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. 生成 agent_id 和 tmux_session                             │
│    - agent_id: cam-{timestamp}-{counter}                    │
│    - tmux_session: 使用 --name 或 agent_id                   │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. 创建 tmux session                                         │
│    - tmux new-session -d -s {session} -c {cwd}              │
│    - 执行 agent 启动命令                                      │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ 5. 注册到 agents.json                                        │
│    - 保存 agent_id, agent_type, project_path, tmux_session  │
│    - 设置初始状态为 Processing                                │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ 6. 发送初始 prompt (如果提供)                                 │
│    - 等待 agent 就绪（检测提示符）                            │
│    - 通过 tmux send-keys 发送                                │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ 7. 启动 watcher daemon                                       │
│    - 确保 watcher 在运行                                      │
│    - 开始监控 agent 状态                                      │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ 8. 输出结果                                                  │
│    - 显示 agent_id 和 tmux_session                           │
│    - 提示如何 attach 到 session                              │
└─────────────────────────────────────────────────────────────┘
```

## 输出格式

### 成功输出（文本）

```
已启动 Claude Code agent
  agent_id: cam-1740000000000-0
  tmux_session: cam-1740000000000-0
  工作目录: /Users/admin/workspace/myproject

查看输出: tmux attach -t cam-1740000000000-0
```

### 成功输出（JSON）

```json
{
  "agent_id": "cam-1740000000000-0",
  "tmux_session": "cam-1740000000000-0",
  "agent_type": "claude",
  "project_path": "/Users/admin/workspace/myproject"
}
```

### 错误输出

```
错误: tmux 未安装或不可用
请先安装 tmux: brew install tmux
```

```
错误: claude 命令未找到
请先安装 Claude Code: npm install -g @anthropic-ai/claude-code
```

```
错误: 工作目录不存在: /path/to/nonexistent
```

## 错误处理

| 错误场景 | 错误信息 | 退出码 |
|----------|----------|--------|
| tmux 不可用 | `tmux 未安装或不可用` | 1 |
| agent 命令不存在 | `{agent} 命令未找到` | 1 |
| 工作目录不存在 | `工作目录不存在: {path}` | 1 |
| tmux session 创建失败 | `创建 tmux session 失败: {error}` | 1 |
| resume session 不存在 | `会话不存在: {session_id}` | 1 |

## 实现细节

### Agent 启动命令

| Agent 类型 | 启动命令 | 恢复命令 |
|------------|----------|----------|
| claude-code | `claude` | `claude --resume {session_id}` |
| codex | `codex` | `codex --resume {session_id}` |

### 就绪检测

Claude Code 就绪标志：
- 提示符 `❯` 或 `>`
- 输出包含 "Welcome to" 或 "Claude Code"

Codex 就绪标志：
- 提示符 `>`
- 输出包含 "Codex"

### 与现有命令的关系

`cam start` 是 `AgentManager.start_agent()` 的 CLI 封装，与 `cam team-spawn` 的区别：

| 特性 | cam start | cam team-spawn |
|------|-----------|----------------|
| 用途 | 独立启动 agent | 在 Team 中启动 agent |
| Team 关联 | 无 | 注册到 Team config |
| 成员名称 | 无 | 必须指定 |
| 适用场景 | 单 agent 任务 | 多 agent 协作 |

## clap 定义

```rust
/// 启动 AI 编码代理
Start {
    /// Agent 类型
    #[arg(long, short, default_value = "claude-code")]
    agent: String,

    /// 工作目录
    #[arg(long, short)]
    cwd: Option<String>,

    /// tmux session 名称
    #[arg(long, short)]
    name: Option<String>,

    /// 恢复指定 session
    #[arg(long, short, conflicts_with = "prompt")]
    resume: Option<String>,

    /// 输出 JSON 格式
    #[arg(long)]
    json: bool,

    /// 初始 prompt
    prompt: Option<String>,
}
```

## 使用示例

### 基本用法

```bash
# 在当前目录启动 Claude Code
cam start

# 在指定目录启动
cam start --cwd /path/to/project

# 启动 Codex
cam start --agent codex

# 带初始 prompt
cam start "帮我实现一个 TODO 应用"

# 指定 session 名称
cam start --name my-project "开始开发"

# 恢复会话
cam start --resume 862c4b15-f02a-45d6-b349-995d4d848765

# JSON 输出
cam start --json
```

### 组合使用

```bash
# 在指定目录启动 Codex 并发送 prompt
cam start --agent codex --cwd ~/projects/api "实现用户认证"

# 启动后立即 attach
cam start && tmux attach -t $(cam list --json | jq -r '.[0].tmux_session')
```

## 测试计划

### 单元测试

1. 参数解析测试
   - 默认值
   - 各种参数组合
   - 互斥参数验证

2. 错误处理测试
   - tmux 不可用
   - agent 命令不存在
   - 工作目录不存在

### 集成测试

1. 启动 Claude Code
   - 验证 tmux session 创建
   - 验证 agents.json 记录
   - 验证 watcher 启动

2. 启动 Codex
   - 同上

3. 发送初始 prompt
   - 验证 prompt 发送成功
   - 验证 agent 响应

4. 恢复会话
   - 验证 session 恢复
   - 验证状态同步
