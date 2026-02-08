//! Code Agent Monitor CLI
//!
//! 监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)

use clap::{Parser, Subcommand};
use code_agent_monitor::{
    ProcessScanner, SessionManager, McpServer, Watcher, AgentManager, StartAgentRequest,
    AgentWatcher, WatchEvent, OpenclawNotifier, WatcherDaemon,
    discover_teams, get_team_members,
    list_tasks, list_team_names
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
        /// Dry-run 模式（只打印不发送）
        #[arg(long)]
        dry_run: bool,
    },
    /// 列出所有 Claude Code Agent Teams
    Teams {
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },
    /// 列出指定 Team 的成员
    TeamMembers {
        /// Team 名称
        team: String,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },
    /// 列出指定 Team 的任务
    Tasks {
        /// Team 名称（可选，不指定则列出所有 team）
        team: Option<String>,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
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
                            match notifier.send_event(agent_id, "AgentExited", project_path, "") {
                                Ok(_) => eprintln!("✅ 通知发送成功: {}", agent_id),
                                Err(e) => eprintln!("❌ 通知发送失败: {} - {}", agent_id, e),
                            }
                        }
                        WatchEvent::Error { agent_id, message, .. } => {
                            eprintln!("检测到错误: {} - {}", agent_id, message);
                            match notifier.send_event(agent_id, "Error", "", message) {
                                Ok(_) => eprintln!("✅ 通知发送成功: {}", agent_id),
                                Err(e) => eprintln!("❌ 通知发送失败: {} - {}", agent_id, e),
                            }
                        }
                        WatchEvent::WaitingForInput { agent_id, pattern_type, context } => {
                            eprintln!("检测到等待输入: {} ({})", agent_id, pattern_type);
                            match notifier.send_event(agent_id, "WaitingForInput", pattern_type, context) {
                                Ok(_) => eprintln!("✅ 通知发送成功: {}", agent_id),
                                Err(e) => eprintln!("❌ 通知发送失败: {} - {}", agent_id, e),
                            }
                        }
                        WatchEvent::ToolUse { agent_id, tool_name, tool_target, .. } => {
                            eprintln!("检测到工具调用: {} - {}", agent_id, tool_name);
                            let context = tool_target.as_deref().unwrap_or("");
                            match notifier.send_event(agent_id, "ToolUse", tool_name, context) {
                                Ok(_) => eprintln!("✅ 通知发送成功: {}", agent_id),
                                Err(e) => eprintln!("❌ 通知发送失败: {} - {}", agent_id, e),
                            }
                        }
                        _ => {} // 忽略其他事件 (ToolUseBatch, AgentResumed)
                    }
                }

                sleep(Duration::from_secs(interval)).await;
            }
        }
        Commands::Notify { event, agent_id, dry_run } => {
            use std::fs::{OpenOptions, create_dir_all};
            use std::io::Write;

            let log_dir = dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".claude-monitor");
            let log_path = log_dir.join("hook.log");

            // 确保日志目录存在
            if let Err(e) = create_dir_all(&log_dir) {
                eprintln!("无法创建日志目录: {}", e);
            }

            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");

            // 从 stdin 读取 hook 输入（Claude Code 通过 stdin 传递 JSON）
            let context = std::io::read_to_string(std::io::stdin()).unwrap_or_default();

            // 解析 JSON 获取 session_id 和 cwd
            let json: Option<serde_json::Value> = serde_json::from_str(&context).ok();
            let session_id = json.as_ref()
                .and_then(|j| j.get("session_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let cwd = json.as_ref()
                .and_then(|j| j.get("cwd"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let agent_manager = AgentManager::new();

            // 如果是 session_start 事件，建立 session_id 与 agent_id 的映射
            if event == "session_start" {
                if let (Some(ref sid), Some(ref cwd_path)) = (&session_id, &cwd) {
                    match agent_manager.update_session_id_by_cwd(cwd_path, sid) {
                        Ok(true) => {
                            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                                let _ = writeln!(file, "[{}] ✅ Mapped session_id {} to agent by cwd {}", timestamp, sid, cwd_path);
                            }
                        }
                        Ok(false) => {
                            // 没有匹配的 CAM agent，注册为外部会话
                            match agent_manager.register_external_session(sid, cwd_path) {
                                Ok(ext_id) => {
                                    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                                        let _ = writeln!(file, "[{}] ✅ Registered external session {} as {}", timestamp, sid, ext_id);
                                    }
                                }
                                Err(e) => {
                                    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                                        let _ = writeln!(file, "[{}] ❌ Failed to register external session: {}", timestamp, e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                                let _ = writeln!(file, "[{}] ❌ Failed to map session_id: {}", timestamp, e);
                            }
                        }
                    }
                }
            }

            // 查找对应的 agent_id（优先通过 session_id，其次通过 cwd）
            // 如果找不到且有 session_id + cwd，自动注册为外部会话
            let resolved_agent_id = if let Some(ref sid) = session_id {
                // 先尝试通过 session_id 查找
                if let Ok(Some(agent)) = agent_manager.find_agent_by_session_id(sid) {
                    agent.agent_id
                } else if let Some(ref cwd_path) = cwd {
                    // 再尝试通过 cwd 查找
                    if let Ok(Some(agent)) = agent_manager.find_agent_by_cwd(cwd_path) {
                        agent.agent_id
                    } else {
                        // 找不到 agent，自动注册为外部会话（不仅限于 session_start 事件）
                        match agent_manager.register_external_session(sid, cwd_path) {
                            Ok(ext_id) => {
                                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                                    let _ = writeln!(file, "[{}] ✅ Auto-registered external session {} as {} (event: {})", timestamp, sid, ext_id, event);
                                }
                                ext_id
                            }
                            Err(_) => sid.clone() // 注册失败，回退到 session_id
                        }
                    }
                } else {
                    sid.clone()
                }
            } else {
                agent_id.unwrap_or_else(|| "unknown".to_string())
            };

            // 记录 hook 触发日志
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                let _ = writeln!(file, "[{}] Hook triggered: event={}, agent_id={}, session_id={:?}",
                    timestamp, event, resolved_agent_id, session_id);
            }

            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                let _ = writeln!(file, "[{}] Context: {}", timestamp, context.trim());
            }

            // 判断是否需要获取终端快照（HIGH/MEDIUM urgency 事件）
            let needs_snapshot = match event.as_str() {
                "permission_request" | "Error" | "WaitingForInput" => true,
                "stop" | "session_end" | "AgentExited" => true,
                "notification" => {
                    let notification_type = json.as_ref()
                        .and_then(|j| j.get("notification_type"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    notification_type == "idle_prompt" || notification_type == "permission_prompt"
                }
                _ => false,
            };

            // 获取终端快照
            let terminal_snapshot = if needs_snapshot {
                // 尝试通过 session_id 查找 agent
                if let Ok(Some(agent)) = agent_manager.find_agent_by_session_id(session_id.as_deref().unwrap_or("")) {
                    agent_manager.get_logs(&agent.agent_id, 30).ok()
                } else if let Some(ref cwd_path) = cwd {
                    // 通过 cwd 查找
                    if let Ok(Some(agent)) = agent_manager.find_agent_by_cwd(cwd_path) {
                        agent_manager.get_logs(&agent.agent_id, 30).ok()
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // 构建 enriched context
            let enriched_context = if let Some(snapshot) = terminal_snapshot {
                let mut enriched = context.clone();
                enriched.push_str("\n\n--- 终端快照 ---\n");
                enriched.push_str(&snapshot);
                enriched
            } else {
                context.clone()
            };

            let notifier = OpenclawNotifier::new().with_dry_run(dry_run);
            match notifier.send_event(&resolved_agent_id, &event, "", &enriched_context) {
                Ok(_) => {
                    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                        let _ = writeln!(file, "[{}] ✅ Notification sent successfully", timestamp);
                    }
                    if dry_run {
                        eprintln!("[DRY-RUN] 通知预览完成: {} - {}", resolved_agent_id, event);
                    } else {
                        eprintln!("已发送通知: {} - {}", resolved_agent_id, event);
                    }

                    // 如果是 session_end/stop 事件且是外部会话（ext-xxx），清理记录
                    if (event == "session_end" || event == "stop") && resolved_agent_id.starts_with("ext-") {
                        if let Err(e) = agent_manager.remove_agent(&resolved_agent_id) {
                            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                                let _ = writeln!(file, "[{}] ⚠️ Failed to cleanup external session {}: {}", timestamp, resolved_agent_id, e);
                            }
                        } else if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                            let _ = writeln!(file, "[{}] ✅ Cleaned up external session {}", timestamp, resolved_agent_id);
                        }
                    }
                }
                Err(e) => {
                    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                        let _ = writeln!(file, "[{}] ❌ Notification failed: {}", timestamp, e);
                    }
                    eprintln!("通知发送失败: {}", e);
                    return Err(e);
                }
            }
        }
        Commands::Teams { json } => {
            let teams = discover_teams();

            if json {
                println!("{}", serde_json::to_string_pretty(&teams)?);
            } else {
                if teams.is_empty() {
                    println!("未发现任何 Team");
                } else {
                    println!("发现 {} 个 Team:\n", teams.len());
                    for team in teams {
                        println!("  {} ({} 成员)", team.team_name, team.members.len());
                    }
                }
            }
        }
        Commands::TeamMembers { team, json } => {
            match get_team_members(&team) {
                Some(members) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&members)?);
                    } else {
                        println!("Team '{}' 的成员 ({}):\n", team, members.len());
                        for member in members {
                            println!("  {} | ID: {} | 类型: {}",
                                member.name, member.agent_id, member.agent_type);
                        }
                    }
                }
                None => {
                    eprintln!("未找到 Team: {}", team);
                    std::process::exit(1);
                }
            }
        }
        Commands::Tasks { team, json } => {
            match team {
                Some(team_name) => {
                    let tasks = list_tasks(&team_name);
                    if json {
                        println!("{}", serde_json::to_string_pretty(&tasks)?);
                    } else {
                        if tasks.is_empty() {
                            println!("Team '{}' 没有任务", team_name);
                        } else {
                            println!("Team '{}' 的任务 ({}):\n", team_name, tasks.len());
                            for task in tasks {
                                let owner_str = task.owner.as_deref().unwrap_or("-");
                                let blocked_str = if task.blocked_by.is_empty() {
                                    String::new()
                                } else {
                                    format!(" [blocked by: {}]", task.blocked_by.join(", "))
                                };
                                println!("  #{} [{}] {} (owner: {}){}",
                                    task.id, task.status, task.subject, owner_str, blocked_str);
                            }
                        }
                    }
                }
                None => {
                    // 列出所有 team 的任务
                    let team_names = list_team_names();
                    if team_names.is_empty() {
                        println!("未发现任何 Team");
                    } else {
                        for team_name in team_names {
                            let tasks = list_tasks(&team_name);
                            if !tasks.is_empty() {
                                println!("Team '{}' ({} 任务):", team_name, tasks.len());
                                for task in tasks {
                                    let owner_str = task.owner.as_deref().unwrap_or("-");
                                    println!("  #{} [{}] {} (owner: {})",
                                        task.id, task.status, task.subject, owner_str);
                                }
                                println!();
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
