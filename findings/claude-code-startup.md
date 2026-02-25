# Claude Code 启动方式调研

## 版本信息

- 版本: 2.1.44 (Claude Code)
- 调研日期: 2026-02-25

## 命令行参数

### 基本用法

```bash
claude [options] [command] [prompt]
```

### 核心参数

| 参数 | 说明 |
|------|------|
| `prompt` | 直接传递初始 prompt（位置参数） |
| `-p, --print` | 非交互模式，输出后退出 |
| `-c, --continue` | 继续当前目录最近的会话 |
| `-r, --resume [id]` | 恢复指定会话 |
| `--model <model>` | 指定模型（sonnet, opus 等） |
| `--add-dir <dirs>` | 添加额外的工作目录 |

### 权限控制

| 参数 | 说明 |
|------|------|
| `--dangerously-skip-permissions` | 跳过所有权限检查（仅限沙箱） |
| `--allow-dangerously-skip-permissions` | 启用跳过权限选项 |
| `--permission-mode <mode>` | 权限模式：acceptEdits, bypassPermissions, default, delegate, dontAsk, plan |
| `--allowedTools <tools>` | 允许的工具列表 |
| `--disallowedTools <tools>` | 禁止的工具列表 |

### 输入输出格式

| 参数 | 说明 |
|------|------|
| `--input-format <format>` | 输入格式：text（默认）, stream-json |
| `--output-format <format>` | 输出格式：text（默认）, json, stream-json |
| `--json-schema <schema>` | 结构化输出的 JSON Schema |
| `--include-partial-messages` | 包含部分消息块（需配合 stream-json） |

### 系统提示词

| 参数 | 说明 |
|------|------|
| `--system-prompt <prompt>` | 自定义系统提示词 |
| `--append-system-prompt <prompt>` | 追加到默认系统提示词 |

### MCP 配置

| 参数 | 说明 |
|------|------|
| `--mcp-config <configs>` | 加载 MCP 服务器配置 |
| `--strict-mcp-config` | 仅使用指定的 MCP 配置 |

### 其他

| 参数 | 说明 |
|------|------|
| `--agent <agent>` | 指定 agent |
| `--agents <json>` | 自定义 agents JSON |
| `--max-budget-usd <amount>` | API 调用预算限制（仅 --print） |
| `--fallback-model <model>` | 过载时的备用模型（仅 --print） |
| `--session-id <uuid>` | 指定会话 ID |
| `--no-session-persistence` | 禁用会话持久化（仅 --print） |
| `--debug [filter]` | 调试模式 |
| `--verbose` | 详细输出 |

## 非交互模式

### --print 模式

```bash
# 基本用法
claude -p "你的问题"

# 指定输出格式
claude -p --output-format json "分析这段代码"

# 流式输出
claude -p --output-format stream-json "生成代码"

# 设置预算限制
claude -p --max-budget-usd 1.0 "复杂任务"

# 禁用会话持久化
claude -p --no-session-persistence "一次性任务"
```

### 管道使用

```bash
# 从 stdin 读取
echo "分析这段代码" | claude -p

# 流式输入输出
echo '{"type":"user","content":"hello"}' | claude -p --input-format stream-json --output-format stream-json
```

## 环境变量

| 变量 | 说明 |
|------|------|
| `CLAUDECODE` | 标识 Claude Code 环境（值为 1） |
| `CLAUDE_CODE_ENTRYPOINT` | 入口点（cli, ide 等） |
| `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` | 启用 Agent Teams 功能 |
| `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` | 禁用非必要网络流量 |
| `ANTHROPIC_API_KEY` | API 密钥（API 用户） |

**注意**: 当 `CLAUDECODE=1` 时，Claude Code 会拒绝启动以防止嵌套会话。需要 `unset CLAUDECODE` 才能在 Claude Code 内启动新实例。

## 启动命令示例

### 基本启动

```bash
# 交互模式
claude

# 指定模型
claude --model sonnet

# 指定工作目录
cd /path/to/project && claude
claude --add-dir /additional/path
```

### 带 Prompt 启动

```bash
# 交互模式带初始 prompt
claude "帮我分析这个项目"

# 非交互模式
claude -p "列出所有 TODO"

# 带系统提示词
claude --system-prompt "你是代码审查专家" "审查这段代码"
```

### 会话管理

```bash
# 继续最近会话
claude -c

# 恢复指定会话
claude -r abc123

# 恢复并 fork 新会话
claude -r abc123 --fork-session
```

### 脚本集成

```bash
# 非交互 + JSON 输出
claude -p --output-format json "分析代码结构" > result.json

# 流式处理
claude -p --output-format stream-json "生成代码" | while read line; do
  echo "$line" | jq .
done

# 预算控制
claude -p --max-budget-usd 0.5 "简单任务"
```

### 权限控制

```bash
# 跳过权限（仅沙箱环境）
claude --dangerously-skip-permissions "执行任务"

# 指定权限模式
claude --permission-mode acceptEdits "修改代码"

# 限制工具
claude --allowedTools "Read,Grep" "只读分析"
```

## CAM 集成建议

### 启动命令

```bash
# CAM 启动 Claude Code 推荐方式
tmux new-session -d -s "cam-$SESSION_ID" \
  "claude --permission-mode default '初始任务'"
```

### 非交互任务

```bash
# 一次性任务
claude -p --no-session-persistence "任务描述"
```

### 环境隔离

```bash
# 在 CAM 内启动需要清除环境变量
unset CLAUDECODE && claude "任务"
```

## 子命令

| 命令 | 说明 |
|------|------|
| `claude auth login` | 登录 |
| `claude auth logout` | 登出 |
| `claude auth status` | 认证状态 |
| `claude mcp add` | 添加 MCP 服务器 |
| `claude mcp list` | 列出 MCP 服务器 |
| `claude mcp remove` | 移除 MCP 服务器 |
| `claude doctor` | 健康检查 |
| `claude update` | 更新 |
