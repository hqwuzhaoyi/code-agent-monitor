//! `cam summary` 命令 - CEO 视角的 agent 状态汇总

use anyhow::Result;
use clap::Args;
use tracing::warn;

use crate::agent::extractor::prompts::{blocking_context_prompt, progress_summary_prompt};
use crate::agent::{AgentManager, AgentStatus};
use crate::ai::client::AnthropicClient;
use crate::notification::store::NotificationStore;
use crate::notification::webhook::{load_webhook_config_from_file, WebhookClient};
use crate::notification::ProgressSnapshot;

#[derive(Args, Debug)]
pub struct SummaryArgs {
    /// 打印消息但不发送（调试用）
    #[arg(long)]
    pub dry_run: bool,
    /// 无论是否有需关注内容都发送
    #[arg(long)]
    pub always: bool,
}

/// Agent 分类后的汇总信息（供消息构建使用）
#[derive(Debug)]
pub struct AgentSummaryItem {
    pub agent_id: String,
    pub project_path: String,
    pub detail: String,
    pub progress: Option<ProgressSnapshot>,
}

/// 构建 CEO 汇总消息（纯函数，便于测试）
pub fn build_summary_message(
    total_active: usize,
    blocking: &[AgentSummaryItem],
    running: &[AgentSummaryItem],
    errors: &[AgentSummaryItem],
    exits: &[AgentSummaryItem],
) -> String {
    use chrono::Local;

    let now = Local::now().format("%H:%M");
    let error_count = errors.len() + exits.len();

    let mut msg = format!(
        "🤖 Agent 汇总 · {}\n━━━━━━━━━━━━━━━━━━━\n活跃: {} 个  |  待确认: {} 个  |  异常: {} 个",
        now, total_active, blocking.len(), error_count
    );

    if !blocking.is_empty() {
        msg.push_str("\n\n🚧 待确认");
        for item in blocking {
            msg.push_str(&format!(
                "\n  {} · {}\n  → {}",
                item.agent_id, item.project_path, item.detail
            ));
            append_progress_block(&mut msg, item.progress.as_ref());
        }
    }

    if !running.is_empty() {
        msg.push_str("\n\n✅ 最近进展");
        for item in running {
            msg.push_str(&format!(
                "\n  {} · {} → {}",
                item.agent_id, item.project_path, item.detail
            ));
            append_progress_block(&mut msg, item.progress.as_ref());
        }
    }

    if !errors.is_empty() || !exits.is_empty() {
        msg.push_str("\n\n⚠️ 需关注");
        for item in errors {
            msg.push_str(&format!(
                "\n  {} · {} → {}",
                item.agent_id, item.project_path, item.detail
            ));
            append_progress_block(&mut msg, item.progress.as_ref());
        }
        for item in exits {
            msg.push_str(&format!(
                "\n  {} · {} → {}",
                item.agent_id, item.project_path, item.detail
            ));
            append_progress_block(&mut msg, item.progress.as_ref());
        }
    }

    msg
}

fn append_progress_block(msg: &mut String, progress: Option<&ProgressSnapshot>) {
    let Some(progress) = progress else {
        return;
    };

    msg.push_str(&format!(
        "\n    进度: {}/{} ({}%)",
        progress.completed_tasks, progress.total_tasks, progress.completion_rate
    ));

    if !progress.remaining_top3.is_empty() {
        msg.push_str("\n    剩余重点:");
        for (idx, item) in progress.remaining_top3.iter().enumerate() {
            msg.push_str(&format!("\n      {}. {}", idx + 1, item.subject));
        }
    }

    if progress.needs_confirmation {
        msg.push_str(&format!(
            "\n    待确认项: {}",
            progress.pending_confirmations_count
        ));
    }
}

/// 生成汇总消息（核心逻辑，供 CLI 和 MCP 工具共用）
///
/// 返回 `Ok(Some(message))` 表示有需要关注的内容，
/// 返回 `Ok(None)` 表示一切正常无需汇报。
pub fn generate_summary() -> Result<Option<String>> {
    let manager = AgentManager::new();
    let all_agents = manager.list_agents().unwrap_or_default();

    // 过滤掉外部会话（ext-xxx），只保留 CAM 管理的 agent
    let agents: Vec<_> = all_agents
        .iter()
        .filter(|a| !a.agent_id.starts_with("ext-"))
        .collect();

    // 读取近期通知（最近 50 条，用于找异常退出和错误）
    let recent_records = NotificationStore::read_recent(50);
    let thirty_min_ago = chrono::Utc::now() - chrono::Duration::minutes(30);

    // 找近期异常退出（也过滤 ext-）
    let mut exits: Vec<AgentSummaryItem> = Vec::new();
    for record in &recent_records {
        if record.agent_id.starts_with("ext-") {
            continue;
        }
        if record.event == "AgentExited" && record.ts > thirty_min_ago {
            if !agents.iter().any(|a| a.agent_id == record.agent_id) {
                let mins_ago = (chrono::Utc::now() - record.ts).num_minutes();
                let project = record.project.clone().unwrap_or_else(|| "unknown".to_string());
                exits.push(AgentSummaryItem {
                    agent_id: record.agent_id.clone(),
                    project_path: project,
                    detail: format!("异常退出（{}分钟前）", mins_ago),
                    progress: ProgressSnapshot::from_agent(&record.agent_id),
                });
            }
        }
    }

    // 找近期错误（活跃的 agent 中，也过滤 ext-）
    let mut errors: Vec<AgentSummaryItem> = Vec::new();
    for record in &recent_records {
        if record.agent_id.starts_with("ext-") {
            continue;
        }
        if record.event == "Error" && record.ts > thirty_min_ago {
            if agents.iter().any(|a| a.agent_id == record.agent_id) {
                if !errors.iter().any(|e| e.agent_id == record.agent_id) {
                    errors.push(AgentSummaryItem {
                        agent_id: record.agent_id.clone(),
                        project_path: record.project.clone().unwrap_or_else(|| "unknown".to_string()),
                        detail: format!("错误: {}", record.summary.chars().take(60).collect::<String>()),
                        progress: ProgressSnapshot::from_agent(&record.agent_id),
                    });
                }
            }
        }
    }

    // 判断是否有需要关注的内容
    let has_blocking = agents.iter().any(|a| a.status.is_waiting());
    let has_issues = !errors.is_empty() || !exits.is_empty();

    if !has_blocking && !has_issues && agents.is_empty() {
        return Ok(None);
    }

    // 创建 Haiku 客户端（可选，失败时回退到默认文本）
    let haiku = AnthropicClient::from_config().ok();

    let mut blocking: Vec<AgentSummaryItem> = Vec::new();
    let mut running: Vec<AgentSummaryItem> = Vec::new();

    for agent in &agents {
        let snapshot = manager
            .tmux
            .capture_pane(&agent.tmux_session, 100)
            .unwrap_or_default();
        let progress = ProgressSnapshot::from_agent(&agent.agent_id);

        match &agent.status {
            AgentStatus::WaitingForInput | AgentStatus::DecisionRequired => {
                let detail = if let Some(ref client) = haiku {
                    let prompt = blocking_context_prompt(&snapshot);
                    match client.complete(&prompt, None) {
                        Ok(resp) => resp.trim().to_string(),
                        Err(e) => {
                            warn!(error = %e, "Haiku blocking context extraction failed");
                            "待确认".to_string()
                        }
                    }
                } else {
                    "待确认".to_string()
                };
                blocking.push(AgentSummaryItem {
                    agent_id: agent.agent_id.clone(),
                    project_path: agent.project_path.clone(),
                    detail,
                    progress,
                });
            }
            AgentStatus::Processing | AgentStatus::Running => {
                let progress_summary = if let Some(ref client) = haiku {
                    let prompt = progress_summary_prompt(&snapshot);
                    match client.complete(&prompt, None) {
                        Ok(resp) => resp.trim().to_string(),
                        Err(e) => {
                            warn!(error = %e, "Haiku progress summary failed");
                            "正在处理中".to_string()
                        }
                    }
                } else {
                    "正在处理中".to_string()
                };
                running.push(AgentSummaryItem {
                    agent_id: agent.agent_id.clone(),
                    project_path: agent.project_path.clone(),
                    detail: progress_summary,
                    progress,
                });
            }
            AgentStatus::Unknown => {
                if !errors.iter().any(|e| e.agent_id == agent.agent_id) {
                    errors.push(AgentSummaryItem {
                        agent_id: agent.agent_id.clone(),
                        project_path: agent.project_path.clone(),
                        detail: "状态未知".to_string(),
                        progress,
                    });
                }
            }
        }
    }

    Ok(Some(build_summary_message(
        agents.len(),
        &blocking,
        &running,
        &errors,
        &exits,
    )))
}

/// 执行 summary 命令主逻辑
pub fn run_summary(args: &SummaryArgs) -> Result<()> {
    let message = if args.always {
        // --always: 强制生成汇总，即使一切正常
        match generate_summary()? {
            Some(msg) => msg,
            None => build_summary_message(0, &[], &[], &[], &[]),
        }
    } else {
        match generate_summary()? {
            Some(msg) => msg,
            None => return Ok(()), // 一切正常，静默退出
        }
    };

    if args.dry_run {
        println!("{}", message);
        return Ok(());
    }

    // 发送钉钉（独立于 webhook，失败不阻断）
    crate::notification::dingtalk::try_send_to_dingtalk(&message);

    // 发送 webhook
    let config = load_webhook_config_from_file().ok_or_else(|| {
        anyhow::anyhow!("Webhook 未配置，请运行 `cam bootstrap` 完成配置")
    })?;

    let client = WebhookClient::new(config).map_err(|e| anyhow::anyhow!("{}", e))?;

    client
        .send_notification_blocking(message, None, None, None)
        .map_err(|e| anyhow::anyhow!("发送失败: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(id: &str, path: &str, detail: &str) -> AgentSummaryItem {
        AgentSummaryItem {
            agent_id: id.to_string(),
            project_path: path.to_string(),
            detail: detail.to_string(),
            progress: None,
        }
    }

    #[test]
    fn test_build_message_with_blocking_agent() {
        let blocking = vec![make_item("cam-abc", "/workspace/auth", "请求执行 rm -rf /tmp")];
        let msg = build_summary_message(1, &blocking, &[], &[], &[]);
        assert!(msg.contains("🚧 待确认"));
        assert!(msg.contains("cam-abc"));
        assert!(msg.contains("/workspace/auth"));
        assert!(msg.contains("请求执行 rm -rf /tmp"));
    }

    #[test]
    fn test_build_message_with_running_agents() {
        let running = vec![
            make_item("cam-def", "/workspace/api", "完成了 JWT 认证模块"),
            make_item("cam-ghi", "/workspace/ui", "实现了登录表单组件"),
        ];
        let msg = build_summary_message(2, &[], &running, &[], &[]);
        assert!(msg.contains("✅ 最近进展"));
        assert!(msg.contains("cam-def"));
        assert!(msg.contains("完成了 JWT 认证模块"));
        assert!(msg.contains("cam-ghi"));
    }

    #[test]
    fn test_build_message_with_errors() {
        let errors = vec![make_item("cam-xyz", "/workspace/db", "API 调用失败")];
        let msg = build_summary_message(1, &[], &[], &errors, &[]);
        assert!(msg.contains("⚠️ 需关注"));
        assert!(msg.contains("cam-xyz"));
        assert!(msg.contains("API 调用失败"));
    }

    #[test]
    fn test_build_message_with_recent_exits() {
        let exits = vec![make_item("cam-old", "/workspace/db", "异常退出（18分钟前）")];
        let msg = build_summary_message(0, &[], &[], &[], &exits);
        assert!(msg.contains("⚠️ 需关注"));
        assert!(msg.contains("cam-old"));
        assert!(msg.contains("异常退出"));
    }

    #[test]
    fn test_build_message_header_shows_counts() {
        let blocking = vec![make_item("cam-1", "/a", "waiting")];
        let errors = vec![make_item("cam-2", "/b", "error")];
        let msg = build_summary_message(3, &blocking, &[], &errors, &[]);
        assert!(msg.contains("活跃: 3 个"));
        assert!(msg.contains("待确认: 1 个"));
        assert!(msg.contains("异常: 1 个"));
    }

    #[test]
    fn test_build_message_includes_progress_block() {
        let running = vec![AgentSummaryItem {
            agent_id: "cam-progress@team-a".to_string(),
            project_path: "/workspace/api".to_string(),
            detail: "完成了接口整理".to_string(),
            progress: Some(ProgressSnapshot {
                total_tasks: 5,
                completed_tasks: 2,
                pending_tasks: 2,
                in_progress_tasks: 1,
                completion_rate: 40,
                remaining_top3: vec![
                    crate::notification::progress::RemainingTaskItem {
                        subject: "补测试".to_string(),
                        status: "in_progress".to_string(),
                        owner: Some("alice".to_string()),
                    },
                    crate::notification::progress::RemainingTaskItem {
                        subject: "清理错误处理".to_string(),
                        status: "pending".to_string(),
                        owner: None,
                    },
                ],
                pending_confirmations_count: 1,
                needs_confirmation: true,
            }),
        }];

        let msg = build_summary_message(1, &[], &running, &[], &[]);
        assert!(msg.contains("进度: 2/5 (40%)"));
        assert!(msg.contains("剩余重点:"));
        assert!(msg.contains("1. 补测试"));
        assert!(msg.contains("2. 清理错误处理"));
        assert!(msg.contains("待确认项: 1"));
    }

    #[test]
    fn test_build_message_contains_timestamp() {
        let msg = build_summary_message(0, &[], &[], &[], &[]);
        assert!(msg.contains("Agent 汇总 ·"));
        assert!(msg.contains("━━━"));
    }

    #[test]
    fn test_build_message_no_sections_when_empty() {
        let running = vec![make_item("cam-1", "/a", "处理中")];
        let msg = build_summary_message(1, &[], &running, &[], &[]);
        assert!(!msg.contains("🚧 需要你决策"));
        assert!(!msg.contains("⚠️ 需关注"));
    }
}
