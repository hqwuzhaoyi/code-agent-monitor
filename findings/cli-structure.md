# CAM CLI 命令结构分析

## 1. 目录结构

```
src/
├── main.rs              # CLI 入口，定义所有命令和执行逻辑
└── cli/
    ├── mod.rs           # 模块导出
    ├── codex_notify.rs  # Codex CLI notify 命令处理
    ├── setup.rs         # Setup 命令 - 自动配置 hook
    └── output.rs        # 输出格式化工具
```

### 文件职责

| 文件 | 职责 |
|------|------|
| `main.rs` | CLI 入口点，使用 clap 定义所有命令和参数，包含命令执行逻辑 |
| `cli/mod.rs` | 模块导出，re-export 子模块的公共 API |
| `cli/codex_notify.rs` | 处理 Codex CLI 的 `agent-turn-complete` 事件 |
| `cli/setup.rs` | 自动配置 CAM hook（支持 claude/codex/opencode） |
| `cli/output.rs` | 输出格式化工具（JSON/表格） |

## 2. 命令实现模式

### clap 参数定义方式

使用 clap derive 宏定义命令结构：

```rust
#[derive(Parser)]
#[command(name = "cam")]
#[command(about = "Code Agent Monitor - 监控和管理 AI 编码代理进程")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 命令描述（显示在帮助中）
    CommandName {
        /// 参数描述
        #[arg(long)]
        flag: bool,

        /// 带默认值的参数
        #[arg(long, default_value = "5")]
        interval: u64,

        /// 位置参数
        name: String,

        /// 可选参数
        optional: Option<String>,
    },

    /// 嵌套子命令
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },

    /// 使用外部 Args 结构
    Setup(SetupArgs),
}
```

### 外部 Args 结构定义

在 `cli/` 子模块中定义复杂参数：

```rust
// cli/setup.rs
#[derive(Args)]
pub struct SetupArgs {
    pub tool: String,

    #[arg(short, long)]
    pub yes: bool,

    #[arg(long)]
    pub dry_run: bool,
}
```

### 子命令注册方式

1. 在 `Commands` enum 中添加变体
2. 如果参数复杂，在 `cli/` 下创建独立模块
3. 在 `cli/mod.rs` 中 re-export
4. 在 `main.rs` 中 import 并在 match 中处理

### 命令执行流程

```
main()
  → Cli::parse()           # clap 解析命令行参数
  → match cli.command      # 匹配命令
    → Commands::Xxx { .. } # 解构参数
      → 执行业务逻辑       # 调用库函数或内联处理
```

## 3. 现有命令列表

### 进程管理

| 命令 | 功能 |
|------|------|
| `list` | 列出所有正在运行的代理进程 |
| `info <pid>` | 获取指定进程的详细信息 |
| `kill <pid>` | 终止指定进程 |
| `sessions` | 列出所有会话 |
| `resume <session_id>` | 在 tmux 中恢复指定会话 |
| `logs <session_id>` | 查看会话的最近消息 |

### 监控服务

| 命令 | 功能 |
|------|------|
| `watch` | 监控代理进程状态并发送通知 |
| `watch-daemon` | 后台监控 daemon（内部使用） |
| `watch-trigger` | 手动触发 watcher 检测并发送通知 |
| `serve` | 启动 MCP Server 模式 |
| `tui` | 启动 TUI 仪表盘 |

### 通知处理

| 命令 | 功能 |
|------|------|
| `notify` | 接收 Claude Code Hook 通知（内部使用） |
| `codex-notify` | 接收 Codex CLI notify 事件 |
| `pending-confirmations` | 获取待处理的确认请求 |
| `reply` | 回复待处理的确认请求 |

### Team 管理

| 命令 | 功能 |
|------|------|
| `teams` | 列出所有 Claude Code Agent Teams |
| `team-members <team>` | 列出指定 Team 的成员 |
| `tasks [team]` | 列出指定 Team 的任务 |
| `team-create <name>` | 创建新的 Agent Team |
| `team-delete <name>` | 删除 Agent Team |
| `team-status <name>` | 获取 Team 状态 |
| `team-spawn <team> <name>` | 在 Team 中启动新的 Agent |
| `team-progress <team>` | 获取 Team 聚合进度 |
| `team-shutdown <team>` | 优雅关闭 Team |
| `team-watch <team>` | 实时监控 Team inbox |
| `inbox` | 读取成员 inbox |
| `inbox-send` | 发送消息到成员 inbox |

### 配置管理

| 命令 | 功能 |
|------|------|
| `setup <tool>` | 配置 CAM hooks（支持 claude/codex/opencode） |
| `install` | 安装 watcher 服务（快捷方式） |
| `uninstall` | 卸载 watcher 服务（快捷方式） |
| `service <action>` | 管理 CAM watcher 服务 |

### Service 子命令

| 命令 | 功能 |
|------|------|
| `service install` | 安装 watcher 为系统服务 |
| `service uninstall` | 卸载 watcher 服务 |
| `service restart` | 重启 watcher 服务 |
| `service status` | 查看服务状态 |
| `service logs` | 查看服务日志 |

## 4. 扩展点

### 添加新子命令的步骤

#### 方式一：简单命令（内联在 main.rs）

1. 在 `Commands` enum 中添加新变体：
```rust
#[derive(Subcommand)]
enum Commands {
    // ... 现有命令

    /// 新命令描述
    NewCommand {
        /// 参数描述
        #[arg(long)]
        some_flag: bool,

        /// 位置参数
        name: String,
    },
}
```

2. 在 `main()` 的 match 中添加处理逻辑：
```rust
Commands::NewCommand { some_flag, name } => {
    // 执行逻辑
}
```

#### 方式二：复杂命令（独立模块）

1. 创建 `src/cli/new_command.rs`：
```rust
use clap::Args;
use anyhow::Result;

#[derive(Args)]
pub struct NewCommandArgs {
    pub name: String,

    #[arg(long)]
    pub flag: bool,
}

pub fn handle_new_command(args: NewCommandArgs) -> Result<()> {
    // 执行逻辑
    Ok(())
}
```

2. 在 `src/cli/mod.rs` 中添加：
```rust
pub mod new_command;
pub use new_command::*;
```

3. 在 `src/main.rs` 中：
```rust
use code_agent_monitor::cli::NewCommandArgs;

#[derive(Subcommand)]
enum Commands {
    NewCommand(NewCommandArgs),
}

// match 中
Commands::NewCommand(args) => {
    code_agent_monitor::cli::handle_new_command(args)?;
}
```

#### 方式三：嵌套子命令

1. 定义子命令 enum：
```rust
#[derive(Subcommand)]
enum NewAction {
    SubCmd1 { ... },
    SubCmd2 { ... },
}
```

2. 在 Commands 中引用：
```rust
NewCommand {
    #[command(subcommand)]
    action: NewAction,
}
```

### 需要修改的文件

| 场景 | 需要修改的文件 |
|------|---------------|
| 简单命令 | `src/main.rs` |
| 复杂命令 | `src/cli/new_command.rs`, `src/cli/mod.rs`, `src/main.rs` |
| 嵌套子命令 | `src/main.rs`（定义子命令 enum） |
| 需要新依赖 | `Cargo.toml`, `src/lib.rs`（如果需要导出） |

### 常用模式

1. **JSON 输出支持**：添加 `--json` flag
```rust
#[arg(long)]
json: bool,
```

2. **异步命令**：使用 `async fn` 和 `#[tokio::main]`
```rust
Commands::AsyncCmd { .. } => {
    some_async_function().await?;
}
```

3. **互斥参数**：使用 `conflicts_with`
```rust
#[arg(long, conflicts_with = "other")]
flag: bool,
```

4. **默认值**：使用 `default_value`
```rust
#[arg(long, default_value = "5")]
interval: u64,
```
