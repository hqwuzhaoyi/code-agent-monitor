# Codex 启动方式调研

## 版本信息

- CLI 版本: `codex-cli 0.101.0`
- 配置文件: `~/.codex/config.toml`

## 命令行参数

### 主要参数

| 参数 | 说明 |
|------|------|
| `[PROMPT]` | 可选的初始 prompt |
| `-C, --cd <DIR>` | 指定工作目录 |
| `-m, --model <MODEL>` | 指定模型 |
| `-p, --profile <PROFILE>` | 使用配置文件中的 profile |
| `-c, --config <key=value>` | 覆盖配置值 |
| `-i, --image <FILE>...` | 附加图片 |
| `-s, --sandbox <MODE>` | 沙箱模式: `read-only`, `workspace-write`, `danger-full-access` |
| `-a, --ask-for-approval <POLICY>` | 审批策略: `untrusted`, `on-failure`, `on-request`, `never` |
| `--full-auto` | 自动执行模式 (`-a on-request --sandbox workspace-write`) |
| `--search` | 启用 web 搜索 |
| `--no-alt-screen` | 禁用备用屏幕模式（适用于 tmux） |

### 本地模型支持

| 参数 | 说明 |
|------|------|
| `--oss` | 使用本地开源模型 |
| `--local-provider <PROVIDER>` | 指定本地提供者: `lmstudio`, `ollama` |

## 非交互模式

Codex 通过 `exec` 子命令支持非交互模式：

```bash
codex exec [OPTIONS] [PROMPT]
```

### exec 特有参数

| 参数 | 说明 |
|------|------|
| `--json` | 以 JSONL 格式输出事件 |
| `--ephemeral` | 不持久化会话文件 |
| `--output-schema <FILE>` | 指定输出 JSON Schema |
| `-o, --output-last-message <FILE>` | 将最后消息写入文件 |
| `--skip-git-repo-check` | 允许在非 Git 仓库运行 |
| `--color <COLOR>` | 颜色设置: `always`, `never`, `auto` |

### 从 stdin 读取 prompt

```bash
echo "分析这个项目" | codex exec -
```

## 会话管理

### 恢复会话

```bash
codex resume [SESSION_ID] [PROMPT]
codex resume --last              # 恢复最近会话
codex resume --all               # 显示所有会话（不限当前目录）
```

### Fork 会话

```bash
codex fork [SESSION_ID]
codex fork --last
```

## 环境变量

Codex 主要通过配置文件管理，但支持以下环境变量：

| 变量 | 说明 |
|------|------|
| `OPENAI_API_KEY` | OpenAI API 密钥 |
| `OPENAI_BASE_URL` | API 基础 URL |

## 配置文件

位置: `~/.codex/config.toml`

```toml
model = "gpt-5.3-codex"
model_reasoning_effort = "xhigh"

[mcp_servers.context7]
type = "http"
url = "https://mcp.context7.com/mcp"

[projects."/path/to/project"]
trust_level = "trusted"
```

## 启动命令示例

### 基本启动（交互模式）

```bash
codex
```

### 带 prompt 启动

```bash
codex "分析这个项目的架构"
```

### 指定目录启动

```bash
codex -C /path/to/project
codex -C /path/to/project "修复 bug"
```

### 非交互模式

```bash
# 基本非交互执行
codex exec "列出所有 TODO"

# 从 stdin 读取 prompt
echo "分析代码质量" | codex exec -

# 输出为 JSONL
codex exec --json "检查安全问题"

# 将结果写入文件
codex exec -o result.txt "生成 README"

# 临时会话（不保存）
codex exec --ephemeral "快速检查"
```

### 自动执行模式

```bash
# 完全自动（沙箱内）
codex --full-auto "重构这个函数"

# 危险模式（无沙箱，需谨慎）
codex --dangerously-bypass-approvals-and-sandbox "执行任务"
```

### 指定模型

```bash
codex -m o3 "复杂推理任务"
codex -m gpt-4o "快速任务"
```

### 使用本地模型

```bash
codex --oss "本地任务"
codex --oss --local-provider ollama "使用 Ollama"
```

### 沙箱模式

```bash
codex -s read-only "只读分析"
codex -s workspace-write "允许写入工作区"
codex -s danger-full-access "完全访问"
```

### 审批策略

```bash
codex -a untrusted "只信任安全命令"
codex -a on-failure "失败时才询问"
codex -a on-request "模型决定何时询问"
codex -a never "从不询问"
```

## CAM 集成要点

### 启动命令

```bash
# 交互模式（适合 tmux 监控）
codex -C <workdir> --no-alt-screen "<prompt>"

# 非交互模式（适合脚本）
codex exec -C <workdir> "<prompt>"
```

### 关键参数

1. `-C <DIR>` - 指定工作目录
2. `--no-alt-screen` - 禁用备用屏幕，便于 tmux 捕获输出
3. `--full-auto` - 自动执行模式
4. `-a never` - 完全自动审批

### 与 Claude Code 对比

| 特性 | Codex | Claude Code |
|------|-------|-------------|
| 工作目录 | `-C <DIR>` | `--cwd <DIR>` |
| 初始 prompt | 位置参数 | `-p <PROMPT>` |
| 非交互模式 | `codex exec` | `claude -p` |
| 自动执行 | `--full-auto` | `--dangerously-skip-permissions` |
| 备用屏幕 | `--no-alt-screen` | 默认禁用 |
| 会话恢复 | `codex resume` | `claude --resume` |
