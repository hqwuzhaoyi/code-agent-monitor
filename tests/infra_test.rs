//! infra 模块测试 - TDD 先写测试

use code_agent_monitor::infra::{TmuxManager, ProcessScanner};
use code_agent_monitor::infra::jsonl::{JsonlParser, JsonlEvent};
use code_agent_monitor::infra::input::{InputWaitDetector, InputWaitResult};

#[test]
fn test_infra_module_exports_tmux_manager() {
    // 验证 TmuxManager 可以从 infra 模块导入
    let _tmux = TmuxManager::new();
}

#[test]
fn test_infra_module_exports_process_scanner() {
    // 验证 ProcessScanner 可以从 infra 模块导入
    let _scanner = ProcessScanner::new();
}

#[test]
fn test_infra_jsonl_module_exists() {
    // 验证 jsonl 子模块存在且可导入类型
    fn _check_types(_parser: JsonlParser, _event: JsonlEvent) {}
}

#[test]
fn test_infra_input_module_exists() {
    // 验证 input 子模块存在且可导入类型
    fn _check_types(_detector: InputWaitDetector, _result: InputWaitResult) {}
}
