use code_agent_monitor::{
    AgentManager, StartAgentRequest, WatcherDaemon,
    OpenclawNotifier, InputWaitDetector
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
        agent_id: None,
        tmux_session: None,
    }).unwrap();

    assert!(response.agent_id.starts_with("cam-"));

    // 3. 验证 agent 被记录
    let agents = manager.list_agents().unwrap();
    assert!(agents.iter().any(|a| a.agent_id == response.agent_id));

    // 4. 测试通知格式化
    // 注意：format_event 现在使用项目名而非 agent_id
    // 使用 with_no_ai(true) 避免调用 Haiku API
    let notifier = OpenclawNotifier::new().with_no_ai(true);
    let message = notifier.format_event(
        &response.agent_id,
        "WaitingForInput",
        "Confirmation",
        "Continue? [Y/n]",
    );
    // 验证消息包含等待输入提示（no_ai 模式下返回简洁提示）
    assert!(message.contains("等待输入") || message.contains("无法解析"), "Message should contain '等待输入' or '无法解析': {}", message);

    // 5. 停止 agent
    manager.stop_agent(&response.agent_id).unwrap();

    // 6. 验证 agent 已移除
    let agents = manager.list_agents().unwrap();
    assert!(!agents.iter().any(|a| a.agent_id == response.agent_id));

    // 7. 清理
    let _ = daemon.remove_pid();
}

/// 中文输入检测测试 - 需要 API
///
/// 注意：input_detector 已重构为使用 AI 判断，不再使用硬编码正则。
/// 这个测试需要 Anthropic API key 才能运行。
#[test]
#[ignore = "requires Anthropic API key - input_detector now uses AI"]
fn test_chinese_input_detection() {
    let detector = InputWaitDetector::new();

    // 测试各种中文模式
    // 注意：AI 判断时统一返回 InputWaitPattern::Other
    let test_cases = vec![
        ("是否继续？[是/否]", true),
        ("请输入文件名：", true),
        ("是否继续执行？", true),
        ("正在处理中...", false),
        ("确认？", true),
        ("按回车继续", true),
        ("是否授权此操作", true),
    ];

    for (input, expected_waiting) in test_cases {
        let result = detector.detect_immediate(input);
        assert_eq!(
            result.is_waiting, expected_waiting,
            "Failed for input: {} - expected is_waiting={}, got={}",
            input, expected_waiting, result.is_waiting
        );
    }
}

/// 通知格式化测试
///
/// 注意：format_event 现在使用项目名（从 cwd 或 agent_id 提取）而非直接显示 agent_id。
/// 消息格式已更新为使用 AI 提取问题内容。
#[test]
fn test_notifier_format_events() {
    let notifier = OpenclawNotifier::new().with_no_ai(true);

    // Test WaitingForInput format
    // 使用 JSON context 提供 cwd 信息
    let context = r#"{"cwd": "/workspace/my-project"}"#;
    let msg = notifier.format_event("cam-123", "WaitingForInput", "Confirmation", context);
    assert!(msg.contains("等待输入"), "WaitingForInput should contain '等待输入': {}", msg);
    assert!(msg.contains("my-project"), "WaitingForInput should contain project name: {}", msg);

    // Test Error format
    let msg = notifier.format_event("cam-456", "Error", "", "API rate limit");
    assert!(msg.contains("错误"), "Error should contain '错误': {}", msg);
    assert!(msg.contains("API rate limit"), "Error should contain error message: {}", msg);

    // Test AgentExited format
    // 注意：AgentExited 现在显示 "已完成" 而非 "已退出"
    let msg = notifier.format_event("cam-789", "AgentExited", "/workspace/app", "");
    assert!(msg.contains("已完成"), "AgentExited should contain '已完成': {}", msg);
    assert!(msg.contains("app"), "AgentExited should contain project name: {}", msg);
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
