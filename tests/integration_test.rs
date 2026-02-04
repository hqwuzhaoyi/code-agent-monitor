use code_agent_monitor::{
    AgentManager, StartAgentRequest, WatcherDaemon,
    OpenclawNotifier, InputWaitDetector, InputWaitPattern
};

#[test]
fn test_full_workflow() {
    // 1. 创建测试环境
    let manager = AgentManager::new_for_test();
    let daemon = WatcherDaemon::new_for_test();

    // 清理之前的测试残留
    let _ = daemon.remove_pid();

    // 2. 启动 mock agent
    let response = manager.start_agent(StartAgentRequest {
        project_path: "/tmp".to_string(),
        agent_type: Some("mock".to_string()),
        resume_session: None,
        initial_prompt: None,
    }).unwrap();

    assert!(response.agent_id.starts_with("cam-"));

    // 3. 验证 agent 被记录
    let agents = manager.list_agents().unwrap();
    assert!(agents.iter().any(|a| a.agent_id == response.agent_id));

    // 4. 测试通知格式化
    let notifier = OpenclawNotifier::new();
    let message = notifier.format_event(
        &response.agent_id,
        "WaitingForInput",
        "Confirmation",
        "Continue? [Y/n]",
    );
    assert!(message.contains(&response.agent_id));
    assert!(message.contains("等待输入"));

    // 5. 停止 agent
    manager.stop_agent(&response.agent_id).unwrap();

    // 6. 验证 agent 已移除
    let agents = manager.list_agents().unwrap();
    assert!(!agents.iter().any(|a| a.agent_id == response.agent_id));

    // 7. 清理
    let _ = daemon.remove_pid();
}

#[test]
fn test_chinese_input_detection() {
    let detector = InputWaitDetector::new();

    // 测试各种中文模式
    let test_cases = vec![
        ("是否继续？[是/否]", true, Some(InputWaitPattern::Confirmation)),
        ("请输入文件名：", true, Some(InputWaitPattern::ColonPrompt)),
        ("是否继续执行？", true, Some(InputWaitPattern::Continue)),
        ("正在处理中...", false, None),
        ("确认？", true, Some(InputWaitPattern::Confirmation)),
        ("按回车继续", true, Some(InputWaitPattern::PressEnter)),
        ("是否授权此操作", true, Some(InputWaitPattern::PermissionRequest)),
    ];

    for (input, expected_waiting, expected_pattern) in test_cases {
        let result = detector.detect_immediate(input);
        assert_eq!(
            result.is_waiting, expected_waiting,
            "Failed for input: {} - expected is_waiting={}, got={}",
            input, expected_waiting, result.is_waiting
        );
        if expected_waiting {
            assert_eq!(
                result.pattern_type, expected_pattern,
                "Failed for input: {} - expected pattern={:?}, got={:?}",
                input, expected_pattern, result.pattern_type
            );
        }
    }
}

#[test]
fn test_notifier_format_events() {
    let notifier = OpenclawNotifier::new();

    // Test WaitingForInput format
    let msg = notifier.format_event("cam-123", "WaitingForInput", "Confirmation", "[Y/n]");
    assert!(msg.contains("等待输入"));
    assert!(msg.contains("cam-123"));
    assert!(msg.contains("Confirmation"));
    assert!(msg.contains("[Y/n]"));

    // Test Error format
    let msg = notifier.format_event("cam-456", "Error", "", "API rate limit");
    assert!(msg.contains("错误"));
    assert!(msg.contains("cam-456"));
    assert!(msg.contains("API rate limit"));

    // Test AgentExited format
    let msg = notifier.format_event("cam-789", "AgentExited", "/workspace/app", "");
    assert!(msg.contains("已退出"));
    assert!(msg.contains("cam-789"));
    assert!(msg.contains("/workspace/app"));
}

#[test]
fn test_watcher_daemon_pid_management() {
    let daemon = WatcherDaemon::new_for_test();

    // 确保初始状态
    let _ = daemon.remove_pid();
    assert!(!daemon.is_running());

    // 写入当前进程 PID
    let current_pid = std::process::id();
    daemon.write_pid(current_pid).unwrap();

    // 验证 is_running 返回 true（因为当前进程存在）
    assert!(daemon.is_running());

    // 读取 PID
    let read_pid = daemon.read_pid().unwrap();
    assert_eq!(read_pid, Some(current_pid));

    // 清理
    daemon.remove_pid().unwrap();
    assert!(!daemon.is_running());
}
