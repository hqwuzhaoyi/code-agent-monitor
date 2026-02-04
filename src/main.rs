//! Code Agent Monitor CLI
//!
//! 监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)

use clap::{Parser, Subcommand};
use code_agent_monitor::{
    ProcessScanner, SessionManager, McpServer, Watcher, AgentManager, StartAgentRequest,
    AgentWatcher, WatchEvent, OpenclawNotifier, WatcherDaemon
};
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
        /// 使用 OpenClaw 发送通知
        #[arg(long)]
        openclaw: bool,
    },
    /// 查看会话的最近消息
    Logs {
        /// 会话 ID
        session_id: String,
        /// 显示最近 N 条消息
        #[arg(long, short, default_value = "5")]
        limit: usize,
    },
    /// 后台监控 daemon（内部使用，由 agent_start 自动启动）
    WatchDaemon {
        /// 轮询间隔（秒）
        #[arg(long, short, default_value = "3")]
        interval: u64,
    },
    /// 接收 Claude Code Hook 通知（内部使用）
    Notify {
        /// 事件类型
        #[arg(long)]
        event: String,
        /// Agent ID
        #[arg(long)]
        agent_id: Option<String>,
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
            let session_manager = SessionManager::new();

            // 获取会话信息以获取 project_path
            let session = session_manager.get_session(&session_id)?
                .ok_or_else(|| anyhow::anyhow!("会话 {} 不存在", session_id))?;

            let project_path = if session.project_path.is_empty() {
                ".".to_string()
            } else {
                session.project_path
            };

            // 使用 AgentManager 启动，这样会被监控系统追踪
            let agent_manager = AgentManager::new();
            let response = agent_manager.start_agent(StartAgentRequest {
                project_path,
                agent_type: Some("claude".to_string()),
                resume_session: Some(session_id.clone()),
                initial_prompt: None,
            })?;

            // 如果用户指定了自定义名称，重命名 tmux session
            let final_tmux_session = if let Some(custom_name) = name {
                // 重命名 tmux session
                let _ = std::process::Command::new("tmux")
                    .args(["rename-session", "-t", &response.tmux_session, &custom_name])
                    .output();
                custom_name
            } else {
                response.tmux_session
            };

            println!("已在 tmux 中恢复会话");
            println!("agent_id: {}", response.agent_id);
            println!("tmux_session: {}", final_tmux_session);
            println!("查看输出: /opt/homebrew/bin/tmux attach -t {}", final_tmux_session);
        }
        Commands::Kill { pid } => {
            let scanner = ProcessScanner::new();
            scanner.kill_agent(pid)?;
            println!("已终止进程: {}", pid);
        }
        Commands::Serve { port } => {
            eprintln!("启动 MCP Server 在端口 {}...", port);
            let server = McpServer::new(port);
            server.run().await?;
        }
        Commands::Watch { interval, openclaw } => {
            let mut watcher = Watcher::new(interval, openclaw);
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
        Commands::WatchDaemon { interval } => {
            use std::time::Duration;
            use tokio::time::sleep;

            let daemon = WatcherDaemon::new();
            let notifier = OpenclawNotifier::new();
            let mut watcher = AgentWatcher::new();

            // 写入当前进程 PID
            daemon.write_pid(std::process::id())?;

            eprintln!("CAM Watcher Daemon 启动，轮询间隔: {}秒", interval);

            loop {
                // 检查是否还有 agent 在运行
                let agents = watcher.agent_manager().list_agents()?;
                if agents.is_empty() {
                    eprintln!("所有 agent 已退出，watcher 停止");
                    daemon.remove_pid()?;
                    break;
                }

                // 轮询一次
                let events = watcher.poll_once()?;

                // 只处理关键事件
                for event in events {
                    match &event {
                        WatchEvent::AgentExited { agent_id, project_path } => {
                            eprintln!("检测到 agent 退出: {}", agent_id);
                            let _ = notifier.send_event(agent_id, "AgentExited", project_path, "");
                        }
                        WatchEvent::Error { agent_id, message, .. } => {
                            eprintln!("检测到错误: {} - {}", agent_id, message);
                            let _ = notifier.send_event(agent_id, "Error", "", message);
                        }
                        WatchEvent::WaitingForInput { agent_id, pattern_type, context } => {
                            eprintln!("检测到等待输入: {} ({})", agent_id, pattern_type);
                            let _ = notifier.send_event(agent_id, "WaitingForInput", pattern_type, context);
                        }
                        _ => {} // 忽略其他事件
                    }
                }

                sleep(Duration::from_secs(interval)).await;
            }
        }
        Commands::Notify { event, agent_id } => {
            let notifier = OpenclawNotifier::new();
            let agent_id = agent_id.unwrap_or_else(|| "unknown".to_string());

            // 从 stdin 读取 hook 输入（如果有）
            let context = std::io::read_to_string(std::io::stdin()).unwrap_or_default();

            notifier.send_event(&agent_id, &event, "", &context)?;
            eprintln!("已发送通知: {} - {}", agent_id, event);
        }
    }

    Ok(())
}
