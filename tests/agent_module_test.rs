//! agent 模块测试 - TDD 先写测试

use code_agent_monitor::agent::{AgentManager, AgentRecord, AgentWatcher, WatcherDaemon};

#[test]
fn test_agent_module_exports_manager() {
    // 验证 AgentManager 可以从 agent 模块导入
    let _manager = AgentManager::new();
}

#[test]
fn test_agent_module_exports_record() {
    // 验证 AgentRecord 类型存在
    fn _check_type(_record: AgentRecord) {}
}

#[test]
fn test_agent_module_exports_watcher() {
    // 验证 AgentWatcher 可以从 agent 模块导入
    fn _check_type(_watcher: AgentWatcher) {}
}

#[test]
fn test_agent_module_exports_daemon() {
    // 验证 WatcherDaemon 可以从 agent 模块导入
    fn _check_type(_daemon: WatcherDaemon) {}
}
