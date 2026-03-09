use code_agent_monitor::notification::event::{NotificationEvent, NotificationEventType};

fn event_type_name(event_type: &NotificationEventType) -> &'static str {
    match event_type {
        NotificationEventType::PermissionRequest { .. } => "permission_request",
        NotificationEventType::WaitingForInput { .. } => "waiting_for_input",
        NotificationEventType::Notification { .. } => "notification",
        NotificationEventType::AgentExited => "agent_exited",
        NotificationEventType::Error { .. } => "error",
        NotificationEventType::Stop => "stop",
        NotificationEventType::SessionStart => "session_start",
        NotificationEventType::SessionEnd => "session_end",
    }
}

fn bridge_detail(event: &NotificationEvent) -> String {
    match &event.event_type {
        NotificationEventType::PermissionRequest {
            tool_name,
            tool_input,
        } => {
            let cmd = tool_input
                .get("command")
                .or_else(|| tool_input.get("file_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(snapshot) = &event.terminal_snapshot {
                let lines: Vec<&str> = snapshot.lines().collect();
                let start = lines.len().saturating_sub(30);
                format!("{} {}\n\n{}", tool_name, cmd, lines[start..].join("\n"))
            } else {
                format!("{} {}", tool_name, cmd)
            }
        }
        NotificationEventType::WaitingForInput { .. } => {
            if let Some(snapshot) = &event.terminal_snapshot {
                let lines: Vec<&str> = snapshot.lines().collect();
                let start = lines.len().saturating_sub(20);
                lines[start..].join("\n")
            } else {
                String::new()
            }
        }
        NotificationEventType::Error { message } => message.clone(),
        NotificationEventType::Notification { message, .. } => message.clone(),
        _ => String::new(),
    }
}

fn notification_fingerprint(event: &NotificationEvent) -> String {
    let event_type = event_type_name(&event.event_type);

    match &event.event_type {
        NotificationEventType::PermissionRequest {
            tool_name,
            tool_input,
        } => {
            let cmd = tool_input
                .get("command")
                .or_else(|| tool_input.get("file_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let snapshot = event.terminal_snapshot.as_deref().unwrap_or("");
            format!("{}::{}::{}::{}", event_type, tool_name, cmd, snapshot)
        }
        NotificationEventType::WaitingForInput { pattern_type, .. } => {
            let snapshot = event.terminal_snapshot.as_deref().unwrap_or("");
            format!("{}::{}::{}", event_type, pattern_type, snapshot)
        }
        NotificationEventType::Error { message } => format!("{}::{}", event_type, message),
        NotificationEventType::Notification {
            notification_type,
            message,
        } => format!("{}::{}::{}", event_type, notification_type, message),
        _ => format!("{}::", event_type),
    }
}

fn new_dedup_key(event: &NotificationEvent) -> String {
    format!("{}::{}", event.agent_id, notification_fingerprint(event))
}

fn old_dedup_key(event: &NotificationEvent) -> String {
    let event_type = event_type_name(&event.event_type);
    let project = event.project_path.as_deref().unwrap_or("unknown");
    let detail = bridge_detail(event);
    let mut message = format!(
        "🔴 [CAM] {}\n{} | HIGH\n项目: {}",
        event.agent_id,
        if event_type == "permission_request" {
            "权限请求"
        } else {
            "等待输入"
        },
        project
    );
    if !detail.is_empty() {
        message.push_str("\n\n");
        message.push_str(&detail);
    }
    message.push_str(if event_type == "permission_request" {
        "\n\n回复 y 允许 / n 拒绝"
    } else {
        "\n\n回复你的选择或输入"
    });
    format!("{}::{}::{}", event.agent_id, event_type, message)
}

#[test]
fn permission_request_messages_change_with_terminal_snapshot() {
    let event1 = NotificationEvent::permission_request(
        "cam-123",
        "Bash",
        serde_json::json!({ "command": "git status" }),
    )
    .with_project_path("~/project/code-agent-monitor")
    .with_terminal_snapshot("Question A\nApprove this command?");

    let event2 = NotificationEvent::permission_request(
        "cam-123",
        "Bash",
        serde_json::json!({ "command": "git status" }),
    )
    .with_project_path("~/project/code-agent-monitor")
    .with_terminal_snapshot("Question B\nApprove this command?");

    assert_ne!(bridge_detail(&event1), bridge_detail(&event2));
    assert_ne!(old_dedup_key(&event1), old_dedup_key(&event2));
    assert_ne!(new_dedup_key(&event1), new_dedup_key(&event2));
}

#[test]
fn bridge_should_not_dedup_repeated_waiting_message_text() {
    let event1 = NotificationEvent::waiting_for_input("cam-123", "Confirmation")
        .with_project_path("~/project/code-agent-monitor")
        .with_terminal_snapshot("Please confirm");
    let event2 = NotificationEvent::waiting_for_input("cam-123", "Confirmation")
        .with_project_path("~/project/code-agent-monitor")
        .with_terminal_snapshot("Please confirm");

    assert_eq!(old_dedup_key(&event1), old_dedup_key(&event2));
    assert_eq!(new_dedup_key(&event1), new_dedup_key(&event2));
}
