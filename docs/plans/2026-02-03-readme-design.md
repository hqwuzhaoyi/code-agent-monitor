# README 设计文档

## 概述

为 Code Agent Monitor (CAM) 项目创建双语 README 文档。

## 设计决策

| 决策项 | 选择 | 理由 |
|--------|------|------|
| 目标受众 | 用户 + 贡献者 | 兼顾使用和开发 |
| 语言 | 双语 (英文 + 中文) | 英文 README.md，中文 README_zh.md |
| 结构 | 标准版 | 简介、特性、安装、用法、配置、开发指南、许可证 |
| 安装方式 | 仅源码编译 | 项目早期，避免维护多渠道 |
| 用法展示 | 自然语言优先 | 突出核心价值，6-8 个场景映射 |

## README 结构

### 1. 简介与特性

```markdown
# Code Agent Monitor (CAM)

通过 Telegram 用自然语言监控和控制 AI 编码代理（Claude Code、OpenCode、Codex）。

## 为什么需要 CAM？

AI 编码代理在执行过程中经常需要人工输入——确认、决策或错误处理。
CAM 让你可以在手机上回复，而不用一直守在电脑前。

**核心工作流：**
1. Agent 需要输入 → CAM 检测并通过 Telegram 通知你
2. 你用自然语言回复 → CAM 将输入发送给 Agent
3. Agent 继续执行

## 特性

- **自然语言控制** - 通过 Telegram 回复 Agent
- **实时监控** - 检测工具调用、错误、输入提示
- **多代理支持** - Claude Code、OpenCode、Codex
- **MCP Server** - 与 Clawdbot 或其他 MCP 客户端集成
- **会话管理** - 在 tmux 中恢复中断的会话
```

### 2. 用法 - 自然语言控制

```markdown
## 用法

### 自然语言控制（通过 Telegram + Clawdbot）

CAM 的核心价值是让你用自然语言远程控制 AI 代理：

| 你说的话 | CAM 执行的操作 |
|---------|---------------|
| "在 /workspace/myapp 启动 claude" | `agent/start` - 在 tmux 中启动 Agent |
| "继续" / "y" / "没问题" | `agent/send` - 发送确认输入 |
| "看看现在在干什么" | `agent/logs` - 获取最近输出 |
| "停掉那个代理" | `agent/stop` - 终止 Agent |
| "有哪些代理在跑" | `agent/list` - 列出运行中的 Agent |
| "恢复刚才的会话" | `agent/start` + `resume_session` - 恢复已退出的会话 |
| "把这段代码发给它：..." | `agent/send` - 发送多行输入 |
| "它现在什么状态" | `agent/status` - 获取 Agent 状态（running/waiting/stopped） |

### 本地 CLI 命令（开发/调试用）

```bash
# 进程监控
cam list                    # 列出所有运行中的代理进程
cam info <pid>              # 获取指定进程详情
cam watch                   # 持续监控并发送通知

# 会话管理
cam sessions                # 列出所有 Claude 会话
cam resume <session_id>     # 在 tmux 中恢复会话
cam logs <session_id>       # 查看会话最近消息

# MCP 服务
cam serve --port 3000       # 启动 MCP Server
```
```

### 3. 安装、配置与 Skills

```markdown
## 安装

### 前置要求

- Rust 1.70+
- tmux

### 从源码编译

```bash
git clone https://github.com/user/code-agent-monitor.git
cd code-agent-monitor
cargo build --release

# 安装到系统
cargo install --path .
```

## 配置

### 数据目录

CAM 的数据存储在 `~/.claude-monitor/`：

```
~/.claude-monitor/
├── agents.json          # 运行中的 Agent 列表
├── config.json          # 配置文件
└── logs/                # Agent 输出日志
```

### Claude Code Skills 配置

将 skills 文件放置到 Claude Code 的 skills 目录，让 Claude Code 能够使用 CAM：

```bash
# 创建 skills 目录
mkdir -p ~/clawd/skills/code-agent-monitor

# 复制 skills 文件
cp docs/SKILL.md ~/clawd/skills/code-agent-monitor/SKILL.md
```

Skills 提供的能力：
- 列出和恢复 Claude Code 会话
- 启动、监控、停止 AI 代理
- 向运行中的代理发送输入
- 获取代理状态和日志

### 与 Clawdbot 集成

CAM 作为 MCP Server 运行，Clawdbot 作为 Telegram 桥接：

```
Telegram ←→ Clawdbot ←→ CAM MCP Server ←→ tmux sessions
```

详细架构见 [设计文档](docs/plans/2026-02-01-telegram-remote-control-design.md)。
```

### 4. 开发指南与许可证

```markdown
## 开发

### 项目结构

```
src/
├── main.rs           # CLI 入口
├── lib.rs            # 库导出
├── process.rs        # 进程扫描
├── session.rs        # 会话管理
├── agent.rs          # Agent 生命周期
├── tmux.rs           # tmux 操作封装
├── mcp.rs            # MCP Server
├── agent_watcher.rs  # 状态监控
├── input_detector.rs # 输入等待检测
├── jsonl_parser.rs   # JSONL 日志解析
├── throttle.rs       # 通知限流
└── notify.rs         # 通知发送
```

### 运行测试

```bash
# 全部测试
cargo test

# 按模块测试
cargo test tmux
cargo test agent
cargo test mcp
cargo test jsonl
```

### 本地开发

```bash
# 开发模式运行
cargo run -- list
cargo run -- watch --interval 3

# 启动 MCP Server
cargo run -- serve --port 3000
```

## 许可证

MIT License
```

## 输出文件

- `README.md` - 英文版
- `README_zh.md` - 中文版

## 实施说明

1. 英文版为主文件，中文版为翻译
2. Skills 文件需要同步复制到 `docs/SKILL.md` 供用户参考
3. 保持两个版本内容同步
