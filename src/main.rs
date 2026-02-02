//! Code Agent Monitor CLI
//! 
//! 监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)

use clap::{Parser, Subcommand};
use code_agent_monitor::{ProcessScanner, SessionManager, McpServer, Watcher};
use anyhow::Result;

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
    /// 列出所有正在运行的代理进程
    List {
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },
    /// 获取指定进程的详细信息
    Info {
        /// 进程 PID
        pid: u32,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },
    /// 列出所有会话
    Sessions {
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },
    /// 在 tmux 中恢复指定会话
    Resume {
        /// 会话 ID
        session_id: String,
        /// 自定义 tmux 会话名称 (默认: cam-<session_id前8位>)
        #[arg(long, short)]
        name: Option<String>,
    },
    /// 终止指定进程
    Kill {
        /// 进程 PID
        pid: u32,
    },
    /// 启动 MCP Server 模式
    Serve {
        /// 监听端口
        #[arg(long, default_value = "3000")]
        port: u16,
    },
    /// 监控代理进程状态并发送通知
    Watch {
        /// 轮询间隔（秒）
        #[arg(long, short, default_value = "5")]
        interval: u64,
        /// 使用 Clawdbot 发送通知
        #[arg(long)]
        clawdbot: bool,
    },
    /// 查看会话的最近消息
    Logs {
        /// 会话 ID
        session_id: String,
        /// 显示最近 N 条消息
        #[arg(long, short, default_value = "5")]
        limit: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List { json } => {
            let scanner = ProcessScanner::new();
            let agents = scanner.scan_agents()?;
            
            if json {
                println!("{}", serde_json::to_string_pretty(&agents)?);
            } else {
                println!("发现 {} 个代理进程:\n", agents.len());
                for agent in agents {
                    println!("  PID: {} | 类型: {} | 工作目录: {}", 
                        agent.pid, agent.agent_type, agent.working_dir);
                }
            }
        }
        Commands::Info { pid, json } => {
            let scanner = ProcessScanner::new();
            if let Some(agent) = scanner.get_agent_info(pid)? {
                if json {
                    println!("{}", serde_json::to_string_pretty(&agent)?);
                } else {
                    println!("进程信息:");
                    println!("  PID: {}", agent.pid);
                    println!("  类型: {}", agent.agent_type);
                    println!("  命令: {}", agent.command);
                    println!("  工作目录: {}", agent.working_dir);
                    println!("  会话 ID: {:?}", agent.session_id);
                }
            } else {
                eprintln!("未找到 PID {} 的代理进程", pid);
            }
        }
        Commands::Sessions { json } => {
            let manager = SessionManager::new();
            let sessions = manager.list_sessions()?;
            
            if json {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            } else {
                println!("发现 {} 个会话:\n", sessions.len());
                for session in sessions {
                    println!("  ID: {} | 项目: {} | 状态: {}", 
                        session.id, session.project_path, session.status);
                }
            }
        }
        Commands::Resume { session_id, name } => {
            let manager = SessionManager::new();
            let tmux_session = manager.resume_in_tmux(&session_id, name.as_deref())?;
            println!("已在 tmux 中恢复会话");
            println!("tmux_session: {}", tmux_session);
            println!("查看输出: /opt/homebrew/bin/tmux attach -t {}", tmux_session);
        }
        Commands::Kill { pid } => {
            let scanner = ProcessScanner::new();
            scanner.kill_agent(pid)?;
            println!("已终止进程: {}", pid);
        }
        Commands::Serve { port } => {
            println!("启动 MCP Server 在端口 {}...", port);
            let server = McpServer::new(port);
            server.run().await?;
        }
        Commands::Watch { interval, clawdbot } => {
            let mut watcher = Watcher::new(interval, clawdbot);
            watcher.watch().await?;
        }
        Commands::Logs { session_id, limit } => {
            let manager = SessionManager::new();
            let messages = manager.get_session_logs(&session_id, limit)?;
            
            if messages.is_empty() {
                println!("未找到会话 {} 的消息", session_id);
            } else {
                println!("会话 {} 的最近 {} 条消息:\n", session_id, messages.len());
                for (i, msg) in messages.iter().enumerate() {
                    println!("--- 消息 {} ({}) ---", i + 1, msg.role);
                    println!("{}\n", msg.content);
                }
            }
        }
    }

    Ok(())
}
