// src/cli/codex_notify.rs
//! Codex CLI notify 命令处理
//!
//! 处理 Codex CLI 的 `agent-turn-complete` 事件，触发 CAM 状态检测。

use crate::agent::adapter::{get_adapter, HookEvent};
use crate::agent::AgentType;
use anyhow::Result;
use clap::Args;
use tracing::info;

/// Codex notify 命令参数
#[derive(Args)]
pub struct CodexNotifyArgs {
    /// JSON payload from Codex notify (passed as command line argument)
    pub payload: String,
}

/// 处理 Codex notify 事件
pub async fn handle_codex_notify(args: CodexNotifyArgs) -> Result<()> {
    let adapter = get_adapter(&AgentType::Codex);

    // 解析事件
    let event = adapter
        .parse_hook_event(&args.payload)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse Codex payload: {}", args.payload))?;

    info!(?event, "Received Codex notify event");

    // 根据事件类型处理
    match event {
        HookEvent::TurnComplete {
            thread_id,
            turn_id,
            cwd,
        } => {
            info!(
                thread_id = %thread_id,
                turn_id = %turn_id,
                cwd = %cwd,
                "Processing turn-complete event"
            );

            // 查找对应的 agent 并触发状态检测
            let agent_manager = crate::AgentManager::new();
            if let Ok(Some(agent)) = agent_manager.find_agent_by_cwd(&cwd) {
                info!(agent_id = %agent.agent_id, "Found agent, triggering status check");

                // 触发 watcher 检测
                let mut watcher = crate::AgentWatcher::new();
                if let Ok(Some(watch_event)) = watcher.trigger_wait_check(&agent.agent_id, false) {
                    info!(?watch_event, "Watch event detected");

                    // 发送通知
                    let notifier =
                        match crate::notification::load_webhook_config_from_file() {
                            Some(config) => crate::OpenclawNotifier::with_webhook(config)
                                .unwrap_or_else(|_| crate::OpenclawNotifier::new()),
                            None => crate::OpenclawNotifier::new(),
                        };

                    if let crate::WatchEvent::WaitingForInput {
                        agent_id,
                        pattern_type,
                        context,
                        dedup_key,
                        is_decision_required,
                    } = watch_event
                    {
                        let notification_event = crate::NotificationEvent::waiting_for_input_with_decision(
                            &agent_id,
                            &pattern_type,
                            is_decision_required,
                        )
                        .with_project_path(cwd)
                        .with_terminal_snapshot(context)
                        .with_dedup_key(dedup_key);

                        match notifier.send_notification_event(&notification_event) {
                            Ok(result) => info!(?result, "Notification sent"),
                            Err(e) => tracing::error!(error = %e, "Notification failed"),
                        }
                    }
                }
            } else {
                info!(cwd = %cwd, "No agent found for cwd");
            }
        }
        _ => {
            info!(?event, "Ignoring non-turn-complete event");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codex_notify_args() {
        // 验证参数结构
        let payload = r#"{"type":"agent-turn-complete","thread-id":"abc","turn-id":"def","cwd":"/tmp"}"#;
        let args = CodexNotifyArgs {
            payload: payload.to_string(),
        };
        assert!(args.payload.contains("agent-turn-complete"));
    }
}
