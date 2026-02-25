//! Code Agent Monitor CLI
//!
//! 监控和管理 AI 编码代理进程 (Claude Code, OpenCode, Codex)

use clap::{Parser, Subcommand};
use tracing::{info, warn, error, debug};
use tracing_subscriber::{fmt, EnvFilter};
use code_agent_monitor::{
    ProcessScanner, SessionManager, McpServer, Watcher, AgentManager, StartAgentRequest,
    AgentWatcher, WatchEvent, OpenclawNotifier, WatcherDaemon, SendResult, TmuxManager,
    discover_teams, get_team_members,
    list_tasks, list_team_names,
    TeamBridge, InboxMessage, TeamOrchestrator,
    ConversationStateManager, ReplyResult, BatchFilter, RiskLevel,
    NotificationEvent, NotificationEventType,
    LaunchdService,
    cli::{CodexNotifyArgs, SetupArgs, StartArgs},
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
    /// 启动 AI 编码代理 (Claude Code 或 Codex)
    Start(StartArgs),
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
    /// 手动触发 watcher 检测并发送通知
    WatchTrigger {
        /// Agent ID
        #[arg(long)]
        agent_id: String,
        /// 强制发送（绕过 AI 检测失败）
        #[arg(long)]
        force: bool,
        /// 跳过去重（总是发送）
        #[arg(long)]
        no_dedup: bool,
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
        /// 禁用 AI 提取（用于测试/调试）
        #[arg(long)]
        no_ai: bool,
        /// Use delegation mode (only send system event, let OpenClaw decide)
        #[arg(long)]
        delegation: bool,
    },
    /// 接收 Codex CLI notify 事件
    CodexNotify(CodexNotifyArgs),
    /// 配置 CAM hooks
    Setup(SetupArgs),
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
    /// 创建新的 Agent Team
    TeamCreate {
        /// Team 名称
        name: String,
        /// Team 描述
        #[arg(long, short)]
        description: Option<String>,
        /// 项目路径
        #[arg(long, short)]
        project: Option<String>,
    },
    /// 删除 Agent Team
    TeamDelete {
        /// Team 名称
        name: String,
    },
    /// 获取 Team 状态
    TeamStatus {
        /// Team 名称
        name: String,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },
    /// 读取成员 inbox
    Inbox {
        /// Team 名称
        team: String,
        /// 成员名称
        #[arg(long, short)]
        member: Option<String>,
        /// 只显示未读消息
        #[arg(long)]
        unread: bool,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },
    /// 发送消息到成员 inbox
    InboxSend {
        /// Team 名称
        team: String,
        /// 成员名称
        member: String,
        /// 消息内容
        message: String,
        /// 发送者名称
        #[arg(long, default_value = "cam")]
        from: String,
    },
    /// 实时监控 Team inbox
    TeamWatch {
        /// Team 名称
        team: String,
        /// 轮询间隔（秒）
        #[arg(long, short, default_value = "2")]
        interval: u64,
    },
    /// 在 Team 中启动新的 Agent
    TeamSpawn {
        /// Team 名称
        team: String,
        /// 成员名称
        name: String,
        /// Agent 类型
        #[arg(long, short = 't', default_value = "general-purpose")]
        agent_type: String,
        /// 启动后立即发送的消息
        #[arg(long, short)]
        prompt: Option<String>,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },
    /// 获取 Team 聚合进度
    TeamProgress {
        /// Team 名称
        team: String,
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },
    /// 优雅关闭 Team（停止所有 agents）
    TeamShutdown {
        /// Team 名称
        team: String,
    },
    /// 获取待处理的确认请求
    PendingConfirmations {
        /// 输出 JSON 格式
        #[arg(long)]
        json: bool,
    },
    /// 回复待处理的确认请求
    Reply {
        /// 回复内容（y/n/1/2/3 或自定义文本）
        reply: String,
        /// 目标 agent_id 或 confirmation_id（可选）
        #[arg(long, short)]
        target: Option<String>,
        /// 批量回复所有待处理请求
        #[arg(long, conflicts_with = "target")]
        all: bool,
        /// 批量回复匹配的 agent（支持 glob，如 "cam-*"）
        #[arg(long, conflicts_with_all = ["target", "all"])]
        agent: Option<String>,
        /// 批量回复指定风险等级的请求 (low/medium/high)
        #[arg(long, conflicts_with_all = ["target", "all", "agent"])]
        risk: Option<String>,
    },
    /// 启动 TUI 仪表盘
    Tui {
        /// 空闲刷新间隔（毫秒）
        #[arg(long, default_value = "10000")]
        refresh_interval: u64,
        /// 不显示通知流
        #[arg(long)]
        no_notifications: bool,
    },
    /// 管理 CAM watcher 服务
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// 安装 watcher 服务（cam service install 的快捷方式）
    Install {
        /// 强制重新安装
        #[arg(long)]
        force: bool,
    },
    /// 卸载 watcher 服务（cam service uninstall 的快捷方式）
    Uninstall,
}

#[derive(Subcommand)]
enum ServiceAction {
    /// 安装 watcher 为系统服务
    Install {
        /// 强制重新安装
        #[arg(long)]
        force: bool,
    },
    /// 卸载 watcher 服务
    Uninstall,
    /// 重启 watcher 服务
    Restart,
    /// 查看服务状态
    Status,
    /// 查看服务日志
    Logs {
        /// 显示最近 N 行
        #[arg(long, short, default_value = "50")]
        lines: usize,
        /// 持续跟踪日志
        #[arg(long, short)]
        follow: bool,
    },
}

/// Record hook event timestamp for cross-process coordination with watcher
fn record_hook_event(agent_id: &str) -> Result<()> {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::collections::HashMap;

    let hook_file = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".config/code-agent-monitor")
        .join("last_hook_events.json");

    // Read existing events
    let mut events: HashMap<String, u64> = if hook_file.exists() {
        std::fs::read_to_string(&hook_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    // Update timestamp
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    events.insert(agent_id.to_string(), now);

    // Atomic write via temp file
    let temp_file = hook_file.with_extension("tmp");
    std::fs::write(&temp_file, serde_json::to_string(&events)?)?;
    std::fs::rename(&temp_file, &hook_file)?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // 清除代理环境变量，避免 API 请求超时
    use std::env;
    env::remove_var("HTTP_PROXY");
    env::remove_var("HTTPS_PROXY");
    env::remove_var("http_proxy");
    env::remove_var("https_proxy");
    env::remove_var("ALL_PROXY");
    env::remove_var("all_proxy");
    // 设置 NO_PROXY 绕过所有代理
    env::set_var("NO_PROXY", "*");
    env::set_var("no_proxy", "*");
    
    // 初始化 tracing 日志系统
    // 通过 RUST_LOG 环境变量控制日志级别，默认为 info
    // 例如: RUST_LOG=debug cam watch-daemon
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("code_agent_monitor=info,cam=info"));

    fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start(args) => {
            code_agent_monitor::cli::handle_start(args)?;
        }
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
                agent_id: None,
                tmux_session: None,
            })?;

            // 如果用户指定了自定义名称，重命名 tmux session
            let final_tmux_session = if let Some(custom_name) = name {
                // 重命名 tmux session
                let tmux_manager = TmuxManager::new();
                let _ = tmux_manager.rename_session(&response.tmux_session, &custom_name);
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
            let notifier = match code_agent_monitor::notification::load_webhook_config_from_file() {
                Some(config) => OpenclawNotifier::with_webhook(config).unwrap_or_else(|_| OpenclawNotifier::new()),
                None => OpenclawNotifier::new(),
            };
            let mut watcher = AgentWatcher::new();

            // 写入当前进程 PID
            daemon.write_pid(std::process::id())?;

            eprintln!("CAM Watcher Daemon 启动，轮询间隔: {}秒", interval);

            // 连续错误计数器
            let mut consecutive_errors = 0;
            const MAX_CONSECUTIVE_ERRORS: u32 = 10;

            loop {
                // 检查是否还有 agent 在运行
                let agents = match watcher.agent_manager().list_agents() {
                    Ok(agents) => {
                        consecutive_errors = 0; // 重置错误计数
                        agents
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        eprintln!("❌ 获取 agent 列表失败 ({}/{}): {}", consecutive_errors, MAX_CONSECUTIVE_ERRORS, e);
                        if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                            eprintln!("❌ 连续错误次数过多，watcher 停止");
                            daemon.remove_pid()?;
                            break;
                        }
                        sleep(Duration::from_secs(interval)).await;
                        continue;
                    }
                };

                if agents.is_empty() {
                    info!("All agents exited, watcher stopping");
                    daemon.remove_pid()?;
                    break;
                }

                // 轮询一次
                let events = match watcher.poll_once() {
                    Ok(events) => {
                        consecutive_errors = 0; // 重置错误计数
                        events
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        error!(
                            error = %e,
                            consecutive = consecutive_errors,
                            max = MAX_CONSECUTIVE_ERRORS,
                            "Poll failed"
                        );
                        if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                            error!("Too many consecutive errors, watcher stopping");
                            daemon.remove_pid()?;
                            break;
                        }
                        sleep(Duration::from_secs(interval)).await;
                        continue;
                    }
                };

                // 只处理关键事件
                for event in events {
                    match &event {
                        WatchEvent::AgentExited { agent_id, project_path } => {
                            info!(agent_id = %agent_id, "Agent exited, sending notification");
                            let notification_event = NotificationEvent::agent_exited(agent_id)
                                .with_project_path(project_path.clone());
                            match notifier.send_notification_event(&notification_event) {
                                Ok(result) => info!(agent_id = %agent_id, result = ?result, "Notification result"),
                                Err(e) => error!(agent_id = %agent_id, error = %e, "Notification failed"),
                            }
                        }
                        WatchEvent::Error { agent_id, message, .. } => {
                            info!(agent_id = %agent_id, message = %message, "Error detected, sending notification");
                            let notification_event = NotificationEvent::error(agent_id, message);
                            match notifier.send_notification_event(&notification_event) {
                                Ok(result) => info!(agent_id = %agent_id, result = ?result, "Notification result"),
                                Err(e) => error!(agent_id = %agent_id, error = %e, "Notification failed"),
                            }
                        }
                        WatchEvent::WaitingForInput { agent_id, pattern_type, context, dedup_key, is_decision_required } => {
                            info!(
                                agent_id = %agent_id,
                                pattern_type = %pattern_type,
                                is_decision_required = is_decision_required,
                                context_len = context.len(),
                                "Waiting for input detected, sending notification"
                            );
                            // 从 agent_manager 获取项目路径
                            let project_path = watcher.agent_manager()
                                .get_agent(agent_id)
                                .ok()
                                .flatten()
                                .map(|a| a.project_path)
                                .unwrap_or_default();
                            let notification_event = NotificationEvent::waiting_for_input_with_decision(agent_id, pattern_type, *is_decision_required)
                                .with_project_path(project_path)
                                .with_terminal_snapshot(context.clone())
                                .with_dedup_key(dedup_key.clone());
                            match notifier.send_notification_event(&notification_event) {
                                Ok(result) => info!(agent_id = %agent_id, result = ?result, "Notification result"),
                                Err(e) => error!(agent_id = %agent_id, error = %e, "Notification failed"),
                            }
                        }
                        WatchEvent::ToolUse { agent_id, tool_name, tool_target, .. } => {
                            debug!(agent_id = %agent_id, tool_name = %tool_name, "Tool use detected");
                            let context = tool_target.as_deref().unwrap_or("");
                            match notifier.send_event(agent_id, "ToolUse", tool_name, context) {
                                Ok(result) => debug!(agent_id = %agent_id, result = ?result, "Notification result"),
                                Err(e) => warn!(agent_id = %agent_id, error = %e, "Notification failed"),
                            }
                        }
                        _ => {} // 忽略其他事件 (ToolUseBatch, AgentResumed)
                    }
                }

                sleep(Duration::from_secs(interval)).await;
            }
        }
        Commands::WatchTrigger { agent_id, force, no_dedup } => {
            let notifier = match code_agent_monitor::notification::load_webhook_config_from_file() {
                Some(config) => OpenclawNotifier::with_webhook(config).unwrap_or_else(|_| OpenclawNotifier::new()),
                None => OpenclawNotifier::new(),
            };
            let mut watcher = AgentWatcher::new();
            match watcher.trigger_wait_check(&agent_id, force)? {
                Some(WatchEvent::WaitingForInput { agent_id, pattern_type, context, dedup_key, is_decision_required }) => {
                    let project_path = watcher.agent_manager()
                        .get_agent(&agent_id)
                        .ok()
                        .flatten()
                        .map(|a| a.project_path)
                        .unwrap_or_default();
                    let event = NotificationEvent::waiting_for_input_with_decision(&agent_id, &pattern_type, is_decision_required)
                        .with_project_path(project_path)
                        .with_terminal_snapshot(context)
                        .with_dedup_key(dedup_key);
                    let notification_event = if no_dedup { event.with_skip_dedup(true) } else { event };
                    match notifier.send_notification_event(&notification_event) {
                        Ok(result) => println!("Notification sent: {:?}", result),
                        Err(e) => eprintln!("Notification failed: {}", e),
                    }
                }
                _ => {
                    println!("No waiting input detected for agent: {}", agent_id);
                }
            }
        }
        #[allow(unused_variables)]
        Commands::Notify { event, agent_id, dry_run, no_ai, delegation } => {
            use std::fs::{OpenOptions, create_dir_all};
            use std::io::Write;

            let log_dir = dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".config/code-agent-monitor");
            let log_path = log_dir.join("hook.log");

            // 确保日志目录存在
            if let Err(e) = create_dir_all(&log_dir) {
                eprintln!("无法创建日志目录: {}", e);
            }

            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");

            // 从 stdin 读取 hook 输入（Claude Code 通过 stdin 传递 JSON）
            let context = std::io::read_to_string(std::io::stdin()).unwrap_or_default();

            // 分离终端快照部分，确保 JSON 解析成功
            // 测试命令可能通过管道传入 JSON + 终端快照
            let raw_context = if let Some(idx) = context.find("\n\n--- 终端快照 ---\n") {
                &context[..idx]
            } else {
                &context
            };

            // 解析 JSON 获取 session_id 和 cwd
            let json: Option<serde_json::Value> = serde_json::from_str(raw_context).ok();
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

            // Record hook event for watcher coordination
            let _ = record_hook_event(&resolved_agent_id);

            // 记录 hook 触发日志
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                let _ = writeln!(file, "[{}] Hook triggered: event={}, agent_id={}, session_id={:?}",
                    timestamp, event, resolved_agent_id, session_id);
            }

            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                let _ = writeln!(file, "[{}] Context: {}", timestamp, context.trim());
            }

            // 判断是否需要获取终端快照
            // 注意：permission_request 不需要终端快照，因为 stdin 已包含完整的 tool_name 和 tool_input
            let needs_snapshot = match event.as_str() {
                "Error" | "WaitingForInput" => true,
                "stop" | "session_end" | "AgentExited" => true,
                "notification" => {
                    let notification_type = json.as_ref()
                        .and_then(|j| j.get("notification_type"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    // idle_prompt 需要终端快照来获取当前问题
                    // permission_prompt 不需要，stdin 已有完整信息
                    notification_type == "idle_prompt"
                }
                _ => false,
            };

            // 获取终端快照
            // 优先使用 stdin 中的终端快照（测试命令可能通过管道传入）
            let terminal_snapshot = if needs_snapshot {
                // 1. 检查 JSON 中的 terminal_snapshot 字段
                if let Some(snapshot) = json.as_ref()
                    .and_then(|j| j.get("terminal_snapshot"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    Some(snapshot.to_string())
                // 2. 检查 stdin 中是否包含终端快照标记
                } else if let Some(idx) = context.find("\n\n--- 终端快照 ---\n") {
                    Some(context[idx + "\n\n--- 终端快照 ---\n".len()..].to_string())
                // 3. 通过 agent_id 获取日志
                } else if let Ok(logs) = agent_manager.get_logs(&resolved_agent_id, 50) {
                    // 通过 resolved_agent_id 直接获取
                    Some(logs)
                } else if let Ok(Some(agent)) = agent_manager.find_agent_by_session_id(session_id.as_deref().unwrap_or("")) {
                    // 尝试通过 session_id 查找 agent
                    agent_manager.get_logs(&agent.agent_id, 50).ok()
                } else if let Some(ref cwd_path) = cwd {
                    // 通过 cwd 查找
                    if let Ok(Some(agent)) = agent_manager.find_agent_by_cwd(cwd_path) {
                        agent_manager.get_logs(&agent.agent_id, 50).ok()
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // 记录终端快照到日志（用于调试）
            if let Some(ref snapshot) = terminal_snapshot {
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                    let _ = writeln!(file, "[{}] Terminal snapshot ({} chars):\n{}", timestamp, snapshot.len(), snapshot);
                }
            }

            // 构建统一的 NotificationEvent
            let notification_event = {
                // 解析事件类型
                let event_type = match event.as_str() {
                    "WaitingForInput" => NotificationEventType::WaitingForInput {
                        pattern_type: "unknown".to_string(),
                        is_decision_required: false,
                    },
                    "permission_request" => {
                        let tool_name = json.as_ref()
                            .and_then(|j| j.get("tool_name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let tool_input = json.as_ref()
                            .and_then(|j| j.get("tool_input"))
                            .cloned()
                            .unwrap_or(serde_json::json!({}));
                        NotificationEventType::PermissionRequest { tool_name, tool_input }
                    }
                    "notification" => {
                        let notification_type = json.as_ref()
                            .and_then(|j| j.get("notification_type"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let message = json.as_ref()
                            .and_then(|j| j.get("message"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        NotificationEventType::Notification { notification_type, message }
                    }
                    "AgentExited" => NotificationEventType::AgentExited,
                    "Error" => NotificationEventType::Error {
                        message: context.clone(),
                    },
                    "stop" => NotificationEventType::Stop,
                    "session_start" => NotificationEventType::SessionStart,
                    "session_end" => NotificationEventType::SessionEnd,
                    _ => NotificationEventType::Notification {
                        notification_type: event.clone(),
                        message: String::new(),
                    },
                };

                let mut evt = NotificationEvent::new(resolved_agent_id.clone(), event_type);
                // 设置项目路径（从 cwd 获取）
                if let Some(ref cwd_path) = cwd {
                    evt = evt.with_project_path(cwd_path.clone());
                }
                // 设置终端快照
                if let Some(ref snapshot) = terminal_snapshot {
                    evt = evt.with_terminal_snapshot(snapshot.clone());
                }
                evt
            };

            let notifier = match code_agent_monitor::notification::load_webhook_config_from_file() {
                Some(config) => OpenclawNotifier::with_webhook(config)
                    .unwrap_or_else(|_| OpenclawNotifier::new())
                    .with_dry_run(dry_run)
                    .with_no_ai(no_ai),
                None => OpenclawNotifier::new()
                    .with_dry_run(dry_run)
                    .with_no_ai(no_ai),
            };
            // 使用新的统一 API
            match notifier.send_notification_event(&notification_event) {
                Ok(result) => {
                    let end_timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                    match &result {
                        SendResult::Sent => {
                            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                                let _ = writeln!(file, "[{}] ✅ Notification sent: {} {}", end_timestamp, event, resolved_agent_id);
                            }
                            if dry_run {
                                eprintln!("[DRY-RUN] 通知预览完成: {} - {}", resolved_agent_id, event);
                            } else {
                                eprintln!("已发送通知: {} - {}", resolved_agent_id, event);
                            }
                        }
                        SendResult::Skipped(reason) => {
                            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                                let _ = writeln!(file, "[{}] ⏭️ Notification skipped: {} {} ({})", end_timestamp, event, resolved_agent_id, reason);
                            }
                            if dry_run {
                                eprintln!("[DRY-RUN] 通知已跳过: {} - {} ({})", resolved_agent_id, event, reason);
                            }
                        }
                        SendResult::Failed(error) => {
                            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                                let _ = writeln!(file, "[{}] ❌ Notification failed: {} {} ({})", end_timestamp, event, resolved_agent_id, error);
                            }
                            eprintln!("通知发送失败: {} - {} ({})", resolved_agent_id, event, error);
                        }
                    }

                    // 如果是 session_end/stop 事件且是外部会话（ext-xxx），清理记录
                    if (event == "session_end" || event == "stop") && resolved_agent_id.starts_with("ext-") {
                        let cleanup_timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                        if let Err(e) = agent_manager.remove_agent(&resolved_agent_id) {
                            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                                let _ = writeln!(file, "[{}] ⚠️ Failed to cleanup external session {}: {}", cleanup_timestamp, resolved_agent_id, e);
                            }
                        } else if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                            let _ = writeln!(file, "[{}] ✅ Cleaned up external session {}", cleanup_timestamp, resolved_agent_id);
                        }
                    }
                }
                Err(e) => {
                    let err_timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                        let _ = writeln!(file, "[{}] ❌ Notification failed: {}", err_timestamp, e);
                    }
                    eprintln!("通知发送失败: {}", e);
                    return Err(e);
                }
            }
        }
        Commands::CodexNotify(args) => {
            code_agent_monitor::cli::handle_codex_notify(args).await?;
        }
        Commands::Setup(args) => {
            code_agent_monitor::cli::handle_setup(args)?;
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
        Commands::TeamCreate { name, description, project } => {
            let bridge = TeamBridge::new();
            let desc = description.as_deref().unwrap_or("Created by CAM");
            let proj = project.as_deref().unwrap_or(".");

            match bridge.create_team(&name, desc, proj) {
                Ok(_) => {
                    println!("已创建 Team: {}", name);
                    println!("  描述: {}", desc);
                    println!("  项目路径: {}", proj);
                }
                Err(e) => {
                    eprintln!("创建 Team 失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::TeamDelete { name } => {
            let bridge = TeamBridge::new();

            match bridge.delete_team(&name) {
                Ok(_) => {
                    println!("已删除 Team: {}", name);
                }
                Err(e) => {
                    eprintln!("删除 Team 失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::TeamStatus { name, json } => {
            let bridge = TeamBridge::new();

            match bridge.get_team_status(&name) {
                Ok(status) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&status)?);
                    } else {
                        println!("Team: {}", status.team_name);
                        if let Some(desc) = &status.description {
                            println!("  描述: {}", desc);
                        }
                        if let Some(path) = &status.project_path {
                            println!("  项目路径: {}", path);
                        }
                        println!("  成员: {} 人", status.members.len());
                        for member in &status.members {
                            let active = if member.is_active { "活跃" } else { "空闲" };
                            println!("    - {} ({}) [未读: {}]", member.name, active, member.unread_count);
                        }
                        println!("  任务: {} 待处理, {} 已完成", status.pending_tasks, status.completed_tasks);
                        println!("  未读消息: {}", status.unread_messages);
                    }
                }
                Err(e) => {
                    eprintln!("获取 Team 状态失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Inbox { team, member, unread, json } => {
            let bridge = TeamBridge::new();

            // 如果指定了成员，只读取该成员的 inbox
            if let Some(member_name) = member {
                match bridge.read_inbox(&team, &member_name) {
                    Ok(messages) => {
                        let filtered: Vec<_> = if unread {
                            messages.into_iter().filter(|m| !m.read).collect()
                        } else {
                            messages
                        };

                        if json {
                            println!("{}", serde_json::to_string_pretty(&filtered)?);
                        } else {
                            if filtered.is_empty() {
                                println!("{}@{} 没有{}消息", member_name, team, if unread { "未读" } else { "" });
                            } else {
                                println!("{}@{} 的消息 ({}):\n", member_name, team, filtered.len());
                                for msg in filtered {
                                    let read_mark = if msg.read { "✓" } else { "●" };
                                    println!("{} [{}] {}: {}", read_mark, msg.timestamp.format("%H:%M"), msg.from, msg.text);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("读取 inbox 失败: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                // 读取所有成员的 inbox
                match bridge.get_team_status(&team) {
                    Ok(status) => {
                        for member_status in &status.members {
                            if let Ok(messages) = bridge.read_inbox(&team, &member_status.name) {
                                let filtered: Vec<_> = if unread {
                                    messages.into_iter().filter(|m| !m.read).collect()
                                } else {
                                    messages
                                };

                                if !filtered.is_empty() {
                                    println!("{}@{} ({} 条):", member_status.name, team, filtered.len());
                                    for msg in filtered.iter().take(3) {
                                        let read_mark = if msg.read { "✓" } else { "●" };
                                        let text_preview = code_agent_monitor::truncate_str(&msg.text, 50);
                                        println!("  {} {}: {}", read_mark, msg.from, text_preview);
                                    }
                                    if filtered.len() > 3 {
                                        println!("  ... 还有 {} 条消息", filtered.len() - 3);
                                    }
                                    println!();
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("获取 Team 状态失败: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::InboxSend { team, member, message, from } => {
            let bridge = TeamBridge::new();

            let msg = InboxMessage {
                from,
                text: message.clone(),
                summary: None,
                timestamp: chrono::Utc::now(),
                color: None,
                read: false,
            };

            match bridge.send_to_inbox(&team, &member, msg) {
                Ok(_) => {
                    println!("已发送消息到 {}@{}", member, team);
                }
                Err(e) => {
                    eprintln!("发送消息失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::TeamWatch { team, interval } => {
            use std::time::Duration;
            use tokio::time::sleep;

            let bridge = TeamBridge::new();
            let notifier = match code_agent_monitor::notification::load_webhook_config_from_file() {
                Some(config) => OpenclawNotifier::with_webhook(config).unwrap_or_else(|_| OpenclawNotifier::new()),
                None => OpenclawNotifier::new(),
            };

            // 验证 team 存在
            if !bridge.team_exists(&team) {
                eprintln!("Team '{}' 不存在", team);
                std::process::exit(1);
            }

            println!("开始监控 Team '{}' (间隔: {}秒)", team, interval);
            println!("按 Ctrl+C 停止\n");

            let mut last_message_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

            loop {
                if let Ok(status) = bridge.get_team_status(&team) {
                    for member in &status.members {
                        if let Ok(messages) = bridge.read_inbox(&team, &member.name) {
                            let last_count = last_message_counts.get(&member.name).copied().unwrap_or(0);

                            if messages.len() > last_count {
                                // 有新消息
                                for msg in messages.iter().skip(last_count) {
                                    println!("[{}] {}@{}: {}",
                                        chrono::Local::now().format("%H:%M:%S"),
                                        msg.from, member.name,
                                        code_agent_monitor::truncate_str(&msg.text, 80)
                                    );

                                    // 检查是否需要通知
                                    let text_lower = msg.text.to_lowercase();
                                    if text_lower.contains("error") || text_lower.contains("错误") || text_lower.contains("permission") {
                                        let _ = notifier.send_event(
                                            &format!("{}@{}", member.name, team),
                                            "inbox_message",
                                            &msg.from,
                                            &msg.text
                                        );
                                    }
                                }

                                last_message_counts.insert(member.name.clone(), messages.len());
                            }
                        }
                    }
                }

                sleep(Duration::from_secs(interval)).await;
            }
        }
        Commands::TeamSpawn { team, name, agent_type, prompt, json } => {
            let orchestrator = TeamOrchestrator::new();

            match orchestrator.spawn_agent(&team, &name, &agent_type, prompt.as_deref()) {
                Ok(result) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("已在 Team '{}' 中启动 Agent", team);
                        println!("  成员名称: {}", result.member_name);
                        println!("  agent_id: {}", result.agent_id);
                        println!("  tmux_session: {}", result.tmux_session);
                        println!("\n查看输出: /opt/homebrew/bin/tmux attach -t {}", result.tmux_session);
                    }
                }
                Err(e) => {
                    eprintln!("启动 Agent 失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::TeamProgress { team, json } => {
            let orchestrator = TeamOrchestrator::new();

            match orchestrator.get_team_progress(&team) {
                Ok(progress) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&progress)?);
                    } else {
                        println!("Team: {}", progress.team_name);
                        println!("  成员: {} 总计, {} 活跃", progress.total_members, progress.active_members);
                        println!("  任务: {} 待处理, {} 已完成", progress.pending_tasks, progress.completed_tasks);
                        if !progress.waiting_for_input.is_empty() {
                            println!("  等待输入: {}", progress.waiting_for_input.join(", "));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("获取 Team 进度失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::TeamShutdown { team } => {
            let orchestrator = TeamOrchestrator::new();

            match orchestrator.shutdown_team(&team) {
                Ok(_) => {
                    println!("已关闭 Team: {}", team);
                }
                Err(e) => {
                    eprintln!("关闭 Team 失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::PendingConfirmations { json } => {
            let state_manager = ConversationStateManager::new();

            match state_manager.get_pending_confirmations() {
                Ok(pending) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&pending)?);
                    } else {
                        if pending.is_empty() {
                            println!("没有待处理的确认请求");
                        } else {
                            println!("待处理的确认请求 ({}):\n", pending.len());
                            for (i, conf) in pending.iter().enumerate() {
                                println!("  {}. [{}] {}", i + 1, conf.agent_id, conf.context);
                                println!("     ID: {} | 创建时间: {}", conf.id, conf.created_at.format("%H:%M:%S"));
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("获取待处理确认失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Reply { reply, target, all, agent, risk } => {
            let state_manager = ConversationStateManager::new();

            // Determine batch filter
            let batch_filter = if all {
                Some(BatchFilter::All)
            } else if let Some(pattern) = agent {
                Some(BatchFilter::Agent(pattern))
            } else if let Some(risk_str) = risk {
                let risk_level = match risk_str.to_lowercase().as_str() {
                    "low" => RiskLevel::Low,
                    "medium" => RiskLevel::Medium,
                    "high" => RiskLevel::High,
                    _ => {
                        eprintln!("无效的风险等级: {}，可选: low, medium, high", risk_str);
                        std::process::exit(1);
                    }
                };
                Some(BatchFilter::Risk(risk_level))
            } else {
                None
            };

            if let Some(filter) = batch_filter {
                // Batch reply mode
                match state_manager.handle_reply_batch(&reply, filter) {
                    Ok(results) => {
                        if results.is_empty() {
                            println!("没有待处理的确认请求");
                        } else {
                            let success_count = results.iter().filter(|r| r.success).count();
                            let fail_count = results.len() - success_count;
                            println!(
                                "已处理 {} 个请求 (成功: {}, 失败: {})",
                                results.len(),
                                success_count,
                                fail_count
                            );
                            for result in &results {
                                if result.success {
                                    println!("  ✅ {} <- {}", result.agent_id, result.reply);
                                } else {
                                    println!(
                                        "  ❌ {} - {}",
                                        result.agent_id,
                                        result.error.as_deref().unwrap_or("unknown error")
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("批量回复失败: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                // Single reply mode (existing logic)
                match state_manager.handle_reply(&reply, target.as_deref()) {
                    Ok(result) => {
                        match result {
                            ReplyResult::Sent { agent_id, reply } => {
                                println!("已发送回复 '{}' 到 {}", reply, agent_id);
                            }
                            ReplyResult::NeedSelection { options } => {
                                println!("有多个待处理的确认，请指定目标：\n");
                                for (i, opt) in options.iter().enumerate() {
                                    println!("  {}. [{}] {}", i + 1, opt.agent_id, opt.context);
                                }
                                println!("\n使用 --target <agent_id> 指定目标，或使用 --all 批量处理");
                            }
                            ReplyResult::NoPending => {
                                println!("没有待处理的确认请求");
                            }
                            ReplyResult::InvalidSelection(msg) => {
                                eprintln!("无效的选择: {}", msg);
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("发送回复失败: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::Tui { refresh_interval, no_notifications: _ } => {
            use code_agent_monitor::tui::{App, init_terminal, restore_terminal, run};

            let mut terminal = init_terminal()?;
            let mut app = App::new();

            let result = run(&mut terminal, &mut app, refresh_interval);

            restore_terminal(&mut terminal)?;

            result?;
        }
        Commands::Service { action } => {
            let service = match LaunchdService::new() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("❌ 初始化服务失败: {}", e);
                    std::process::exit(1);
                }
            };

            match action {
                ServiceAction::Install { force } => {
                    // If force, uninstall first
                    if force {
                        let _ = service.uninstall();
                    }
                    match service.install() {
                        Ok(_) => {
                            println!("✅ CAM watcher 服务已安装并启动");
                            println!("   服务会在系统启动时自动运行");
                            println!("   查看状态: cam service status");
                            println!("   查看日志: cam service logs");
                        }
                        Err(e) => {
                            eprintln!("❌ 安装失败: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ServiceAction::Uninstall => {
                    match service.uninstall() {
                        Ok(_) => {
                            println!("✅ CAM watcher 服务已卸载");
                        }
                        Err(e) => {
                            eprintln!("❌ 卸载失败: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ServiceAction::Restart => {
                    match service.restart() {
                        Ok(_) => {
                            println!("✅ CAM watcher 服务已重启");
                        }
                        Err(e) => {
                            eprintln!("❌ 重启失败: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ServiceAction::Status => {
                    match service.status() {
                        Ok(status) => {
                            if !status.installed {
                                println!("⚪ 服务未安装");
                                println!("   运行 'cam service install' 安装服务");
                            } else if status.running {
                                println!("🟢 服务运行中");
                                if let Some(pid) = status.pid {
                                    println!("   PID: {}", pid);
                                }
                            } else {
                                println!("🔴 服务已安装但未运行");
                                println!("   运行 'cam service restart' 启动服务");
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ 获取状态失败: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ServiceAction::Logs { lines, follow } => {
                    let (stdout_log, stderr_log) = service.log_paths();

                    if follow {
                        println!("📋 跟踪日志 (Ctrl+C 退出)...\n");
                        let _ = std::process::Command::new("tail")
                            .args(["-f", "-n"])
                            .arg(lines.to_string())
                            .arg(&stdout_log)
                            .status();
                    } else {
                        println!("📋 最近 {} 行日志:\n", lines);

                        if stdout_log.exists() {
                            let output = std::process::Command::new("tail")
                                .args(["-n"])
                                .arg(lines.to_string())
                                .arg(&stdout_log)
                                .output();

                            if let Ok(output) = output {
                                print!("{}", String::from_utf8_lossy(&output.stdout));
                            }
                        } else {
                            println!("(日志文件不存在: {})", stdout_log.display());
                        }

                        if stderr_log.exists() {
                            let output = std::process::Command::new("tail")
                                .args(["-n", "10"])
                                .arg(&stderr_log)
                                .output();

                            if let Ok(output) = output {
                                let stderr_content = String::from_utf8_lossy(&output.stdout);
                                if !stderr_content.trim().is_empty() {
                                    println!("\n--- 错误日志 ---");
                                    print!("{}", stderr_content);
                                }
                            }
                        }
                    }
                }
            }
        }
        Commands::Install { force } => {
            let service = match LaunchdService::new() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("❌ 初始化服务失败: {}", e);
                    std::process::exit(1);
                }
            };
            if force {
                let _ = service.uninstall();
            }
            match service.install() {
                Ok(_) => {
                    println!("✅ CAM watcher 服务已安装并启动");
                    println!("   服务会在系统启动时自动运行");
                    println!("   查看状态: cam service status");
                }
                Err(e) => {
                    eprintln!("❌ 安装失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Uninstall => {
            let service = match LaunchdService::new() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("❌ 初始化服务失败: {}", e);
                    std::process::exit(1);
                }
            };
            match service.uninstall() {
                Ok(_) => {
                    println!("✅ CAM watcher 服务已卸载");
                }
                Err(e) => {
                    eprintln!("❌ 卸载失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
