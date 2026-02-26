#[cfg(test)]
mod tests {
    use crate::tui::{AgentItem, App, LogLevel, LogsState, View};
    use crate::AgentStatus;

    #[test]
    fn test_app_navigation() {
        let mut app = App::new();
        app.agents = vec![
            AgentItem {
                id: "1".to_string(),
                agent_type: "claude".to_string(),
                project: "test".to_string(),
                state: AgentStatus::Processing,
                started_at: chrono::Local::now(),
                tmux_session: None,
            },
            AgentItem {
                id: "2".to_string(),
                agent_type: "claude".to_string(),
                project: "test2".to_string(),
                state: AgentStatus::Unknown,
                started_at: chrono::Local::now(),
                tmux_session: None,
            },
        ];

        assert_eq!(app.selected_index, 0);
        app.next_agent();
        assert_eq!(app.selected_index, 1);
        app.next_agent();
        assert_eq!(app.selected_index, 0); // wrap around
        app.prev_agent();
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_view_toggle() {
        let mut app = App::new();
        assert_eq!(app.view, View::Dashboard);
        app.toggle_view();
        assert_eq!(app.view, View::Logs);
        app.toggle_view();
        assert_eq!(app.view, View::Dashboard);
    }

    #[test]
    fn test_agent_state_icon() {
        assert_eq!(AgentStatus::Processing.icon(), "🟢");
        assert_eq!(AgentStatus::WaitingForInput.icon(), "🟡");
        assert_eq!(AgentStatus::Unknown.icon(), "❓");
    }

    #[test]
    fn test_agents_sorted_by_start_time() {
        let mut app = App::new();
        let now = chrono::Local::now();

        app.agents = vec![
            AgentItem {
                id: "old".to_string(),
                agent_type: "claude".to_string(),
                project: "test".to_string(),
                state: AgentStatus::Processing,
                started_at: now - chrono::Duration::hours(2),
                tmux_session: None,
            },
            AgentItem {
                id: "new".to_string(),
                agent_type: "claude".to_string(),
                project: "test".to_string(),
                state: AgentStatus::Processing,
                started_at: now,
                tmux_session: None,
            },
            AgentItem {
                id: "mid".to_string(),
                agent_type: "claude".to_string(),
                project: "test".to_string(),
                state: AgentStatus::Processing,
                started_at: now - chrono::Duration::hours(1),
                tmux_session: None,
            },
        ];

        // Sort manually to test the sorting logic
        app.agents.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        assert_eq!(app.agents[0].id, "new");
        assert_eq!(app.agents[1].id, "mid");
        assert_eq!(app.agents[2].id, "old");
    }

    #[test]
    fn test_filter() {
        let mut app = App::new();
        app.agents = vec![
            AgentItem {
                id: "cam-123".to_string(),
                agent_type: "claude".to_string(),
                project: "my-project".to_string(),
                state: AgentStatus::Processing,
                started_at: chrono::Local::now(),
                tmux_session: None,
            },
            AgentItem {
                id: "cam-456".to_string(),
                agent_type: "claude".to_string(),
                project: "other-project".to_string(),
                state: AgentStatus::Unknown,
                started_at: chrono::Local::now(),
                tmux_session: None,
            },
        ];

        // No filter
        assert_eq!(app.filtered_agents().len(), 2);

        // Filter by ID (实时过滤)
        app.filter_input.set_text("123");
        assert_eq!(app.filtered_agents().len(), 1);
        assert_eq!(app.filtered_agents()[0].id, "cam-123");

        // Filter by project
        app.filter_input.set_text("other");
        assert_eq!(app.filtered_agents().len(), 1);
        assert_eq!(app.filtered_agents()[0].project, "other-project");

        // Case insensitive
        app.filter_input.set_text("MY-PROJECT");
        assert_eq!(app.filtered_agents().len(), 1);

        // Clear filter
        app.clear_filter();
        assert_eq!(app.filtered_agents().len(), 2);
    }

    #[test]
    fn test_filter_mode() {
        let mut app = App::new();

        assert!(!app.filter_mode);
        assert!(app.filter_input.is_empty());

        app.enter_filter_mode();
        assert!(app.filter_mode);

        app.filter_input.set_text("test");
        app.exit_filter_mode();
        assert!(!app.filter_mode);
        assert_eq!(app.filter_input.text(), "test"); // 保留过滤内容

        // Esc 清除过滤
        app.enter_filter_mode();
        app.clear_filter();
        assert!(!app.filter_mode);
        assert!(app.filter_input.is_empty());
    }

    #[test]
    fn test_log_level_filter() {
        let level = LogLevel::Error;
        assert!(level.matches("2024-01-01 ERROR something"));
        assert!(level.matches("2024-01-01 ❌ something"));
        assert!(!level.matches("2024-01-01 INFO something"));

        let level = LogLevel::All;
        assert!(level.matches("anything"));
    }

    #[test]
    fn test_log_level_next() {
        let level = LogLevel::All;
        assert_eq!(level.next(), LogLevel::Error);
        assert_eq!(LogLevel::Error.next(), LogLevel::Warn);
        assert_eq!(LogLevel::Warn.next(), LogLevel::Info);
        assert_eq!(LogLevel::Info.next(), LogLevel::Debug);
        assert_eq!(LogLevel::Debug.next(), LogLevel::All);
    }

    #[test]
    fn test_logs_state_scroll() {
        let mut logs = LogsState::new();
        logs.lines.push_back("line1".to_string());
        logs.lines.push_back("line2".to_string());
        logs.lines.push_back("line3".to_string());

        assert_eq!(logs.scroll_offset, 0);
        logs.scroll_down();
        assert_eq!(logs.scroll_offset, 1);
        logs.scroll_up();
        assert_eq!(logs.scroll_offset, 0);
        logs.scroll_up(); // should not go negative
        assert_eq!(logs.scroll_offset, 0);
    }

    #[test]
    fn test_app_quit() {
        let mut app = App::new();
        assert!(!app.should_quit);
        app.quit();
        assert!(app.should_quit);
    }

    #[test]
    fn test_selected_agent_empty() {
        let app = App::new();
        assert!(app.selected_agent().is_none());
    }

    #[test]
    fn test_selected_agent() {
        let mut app = App::new();
        app.agents = vec![AgentItem {
            id: "test-1".to_string(),
            agent_type: "claude".to_string(),
            project: "project".to_string(),
            state: AgentStatus::Processing,
            started_at: chrono::Local::now(),
            tmux_session: Some("cam-test".to_string()),
        }];

        let agent = app.selected_agent().unwrap();
        assert_eq!(agent.id, "test-1");
        assert_eq!(agent.tmux_session, Some("cam-test".to_string()));
    }

    #[test]
    fn test_close_selected_agent_returns_id() {
        let mut app = App::new();
        app.agents = vec![AgentItem {
            id: "cam-test-close".to_string(),
            agent_type: "claude".to_string(),
            project: "test".to_string(),
            state: AgentStatus::Processing,
            started_at: chrono::Local::now(),
            tmux_session: Some("cam-test-close".to_string()),
        }];

        // close_selected_agent should return the agent ID
        let result = app.close_selected_agent();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("cam-test-close".to_string()));
    }

    #[test]
    fn test_close_selected_agent_empty_list() {
        let mut app = App::new();
        let result = app.close_selected_agent();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_focus_toggle() {
        use crate::tui::Focus;
        let mut app = App::new();
        assert_eq!(app.focus, Focus::AgentList);
        app.toggle_focus();
        assert_eq!(app.focus, Focus::Notifications);
        app.toggle_focus();
        assert_eq!(app.focus, Focus::AgentList);
    }

    #[test]
    fn test_notification_navigation() {
        use crate::tui::Focus;
        let mut app = App::new();
        app.notifications = vec![
            crate::tui::NotificationItem {
                timestamp: chrono::Local::now(),
                agent_id: "cam-1".to_string(),
                message: "msg1".to_string(),
                urgency: crate::notification::Urgency::High,
                event_type: "permission_request".to_string(),
                project: None,
                event_detail: None,
                terminal_snapshot: None,
                risk_level: None,
            },
            crate::tui::NotificationItem {
                timestamp: chrono::Local::now(),
                agent_id: "cam-2".to_string(),
                message: "msg2".to_string(),
                urgency: crate::notification::Urgency::Medium,
                event_type: "AgentExited".to_string(),
                project: None,
                event_detail: None,
                terminal_snapshot: None,
                risk_level: None,
            },
        ];

        assert_eq!(app.notification_selected, 0);
        app.next_notification();
        assert_eq!(app.notification_selected, 1);
        app.next_notification();
        assert_eq!(app.notification_selected, 0); // wrap
        app.prev_notification();
        assert_eq!(app.notification_selected, 1);
    }
}
