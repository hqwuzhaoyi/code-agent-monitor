# CAM Plugin for Clawdbot

Code Agent Monitor (CAM) plugin for Clawdbot - 通过自然语言管理 AI 编码代理。

## 功能

通过 Clawdbot 的 main agent 使用自然语言管理 Claude Code、OpenCode、Codex 等 AI 编码代理：

- **启动/停止代理** - 在指定项目目录启动新代理或停止运行中的代理
- **查看状态** - 获取代理的结构化状态（是否等待输入、最近工具调用等）
- **发送输入** - 向代理发送确认、拒绝或其他指令
- **会话管理** - 列出和恢复历史 Claude Code 会话
- **进程管理** - 列出系统中所有 Claude Code 进程

## 测试结果

| 测试 | 命令 | 结果 |
|------|------|------|
| 查看运行中的 agent | `"现在跑着什么"` | ✅ 通过 |
| 启动新 agent | `"在 /tmp 启动一个 Claude"` | ✅ 通过 |
| 查看 agent 状态 | `"什么情况"` | ✅ 通过 |
| 查看输出日志 | `"看看输出"` | ✅ 通过 |
| 停止 agent | `"停掉"` | ✅ 通过 |
| 列出历史会话 | `"看看最近的会话"` | ✅ 通过 |
| 恢复会话 | `"恢复 <session_id>"` | ✅ 通过 |

## 安装

### 前置要求

1. CAM 二进制文件已编译：
   ```bash
   cd /Users/admin/workspace/code-agent-monitor
   cargo build --release
   ```

2. Clawdbot 已安装

### 安装步骤

```bash
# 1. 创建软链接到 clawd/plugins（可选，便于管理）
ln -s /Users/admin/workspace/code-agent-monitor/plugins/cam ~/clawd/plugins/cam

# 2. 使用软链接方式安装（推荐，修改后立即生效）
clawdbot plugins install -l ~/clawd/plugins/cam

# 3. 验证安装
clawdbot plugins list | grep cam
clawdbot plugins doctor
```

## 工具列表

| 工具名 | 描述 |
|--------|------|
| `cam_agent_start` | 启动新的 Claude Code agent |
| `cam_agent_stop` | 停止运行中的 agent |
| `cam_agent_list` | 列出所有 CAM 管理的 agent |
| `cam_agent_send` | 向 agent 发送消息/输入 |
| `cam_agent_status` | 获取 agent 的结构化状态 |
| `cam_agent_logs` | 获取 agent 的终端输出 |
| `cam_list_sessions` | 列出历史 Claude Code 会话 |
| `cam_resume_session` | 恢复历史会话 |
| `cam_list_agents` | 列出系统中所有 Claude Code 进程 |
| `cam_kill_agent` | 终止 agent 进程（通过 PID） |
| `cam_send_input` | 向 tmux 会话发送原始输入 |

## 使用示例

通过 Clawdbot 的 main agent 发送自然语言命令：

```bash
# 查看运行中的代理
clawdbot agent --agent main --message "现在跑着什么"

# 启动新代理
clawdbot agent --agent main --message "在 /Users/admin/my-project 启动一个 Claude"

# 查看代理状态
clawdbot agent --agent main --message "什么情况"

# 发送确认
clawdbot agent --agent main --message "y"

# 查看输出
clawdbot agent --agent main --message "看看输出"

# 停止代理
clawdbot agent --agent main --message "停掉"

# 查看历史会话
clawdbot agent --agent main --message "看看最近的会话"

# 恢复会话
clawdbot agent --agent main --message "恢复第一个"
```

## 自然语言映射

| 用户可能说的 | 意图 | 对应工具 |
|-------------|------|----------|
| "看看在干嘛" / "现在跑着什么" | 查看状态 | `cam_agent_list` |
| "继续" / "y" / "好" | 确认 | `cam_agent_send` |
| "n" / "不要" | 拒绝 | `cam_agent_send` |
| "在 xxx 启动" / "开个新的" | 启动 | `cam_agent_start` |
| "停" / "停掉" | 停止 | `cam_agent_stop` |
| "看看输出" / "干了什么" | 查看日志 | `cam_agent_logs` |
| "什么情况" / "卡住了吗" | 诊断 | `cam_agent_status` |
| "继续之前的" / "恢复" | 恢复会话 | `cam_resume_session` |

## 文件结构

```
plugins/cam/
├── README.md              # 本文件
├── package.json           # 包配置
├── clawdbot.plugin.json   # Plugin manifest
├── node_modules/          # 依赖
└── src/
    └── index.ts           # 工具注册
```

## 开发

修改 `src/index.ts` 后无需重新安装（软链接方式），但可能需要重启 gateway：

```bash
clawdbot gateway restart
```

### 依赖

- `@sinclair/typebox` - 用于定义工具参数 schema

### CAM 二进制位置

```
/Users/admin/workspace/code-agent-monitor/target/release/cam
```

## 相关文档

- [CAM 主项目](../../README.md)
- [CAM SKILL.md](~/clawd/skills/code-agent-monitor/SKILL.md)
- [设计文档](../../docs/plans/2026-02-03-cam-clawdbot-plugin-design.md)
