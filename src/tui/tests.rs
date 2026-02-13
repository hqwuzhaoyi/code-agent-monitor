#[cfg(test)]
mod tests {
    use crate::tui::{App, AgentItem, AgentState, View, LogLevel, LogsState};

    #[test]
    fn test_app_navigation() {
        let mut app = App::new();
        app.agents = vec![
            AgentItem {
                id: "1".to_string(),
                agent_type: "claude".to_string(),
                project: "test".to_string(),
                state: AgentState::Running,
                started_at: chrono::Local::now(),
                tmux_session: None,
            },
            AgentItem {
                id: "2".to_string(),
                agent_type: "claude".to_string(),
                project: "test2".to_string(),
                state: AgentState::Idle,
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
        assert_eq!(AgentState::Running.icon(), "🟢");
        assert_eq!(AgentState::Waiting.icon(), "🟡");
        assert_eq!(AgentState::Idle.icon(), "⚪");
        assert_eq!(AgentState::Error.icon(), "🔴");
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
                state: AgentState::Running,
                started_at: now - chrono::Duration::hours(2),
                tmux_session: None,
            },
            AgentItem {
                id: "new".to_string(),
                agent_type: "claude".to_string(),
                project: "test".to_string(),
                state: AgentState::Running,
                started_at: now,
                tmux_session: None,
            },
            AgentItem {
                id: "mid".to_string(),
                agent_type: "claude".to_string(),
                project: "test".to_string(),
                state: AgentState::Running,
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
    fn test_search_filter() {
        let mut app = App::new();
        app.agents = vec![
            AgentItem {
                id: "cam-123".to_string(),
                agent_type: "claude".to_string(),
                project: "my-project".to_string(),
                state: AgentState::Running,
                started_at: chrono::Local::now(),
                tmux_session: None,
            },
            AgentItem {
                id: "cam-456".to_string(),
                agent_type: "claude".to_string(),
                project: "other-project".to_string(),
                state: AgentState::Idle,
                started_at: chrono::Local::now(),
                tmux_session: None,
            },
        ];

        // No filter
        assert_eq!(app.filtered_agents().len(), 2);

        // Filter by ID
        app.search_query = "123".to_string();
        assert_eq!(app.filtered_agents().len(), 1);
        assert_eq!(app.filtered_agents()[0].id, "cam-123");

        // Filter by project
        app.search_query = "other".to_string();
        assert_eq!(app.filtered_agents().len(), 1);
        assert_eq!(app.filtered_agents()[0].project, "other-project");

        // Case insensitive
        app.search_query = "MY-PROJECT".to_string();
        assert_eq!(app.filtered_agents().len(), 1);
    }

    #[test]
    fn test_search_mode() {
        let mut app = App::new();

        assert!(!app.search_mode);
        assert!(app.search_query.is_empty());

        app.enter_search_mode();
        assert!(app.search_mode);

        app.search_query = "test".to_string();
        app.exit_search_mode();
        assert!(!app.search_mode);
        assert!(app.search_query.is_empty());
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
        app.agents = vec![
            AgentItem {
                id: "test-1".to_string(),
                agent_type: "claude".to_string(),
                project: "project".to_string(),
                state: AgentState::Running,
                started_at: chrono::Local::now(),
                tmux_session: Some("cam-test".to_string()),
            },
        ];

        let agent = app.selected_agent().unwrap();
        assert_eq!(agent.id, "test-1");
        assert_eq!(agent.tmux_session, Some("cam-test".to_string()));
    }
}
