//! Agent 监控模块 - 监控 Agent 状态、JSONL 事件和输入等待

use crate::agent::{AgentManager, AgentRecord};
use crate::input_detector::{InputWaitDetector, InputWaitResult};
use crate::jsonl_parser::{JsonlEvent, JsonlParser};
use crate::tmux::TmuxManager;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 监控事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum WatchEvent {
    /// Agent 退出
    AgentExited {
        agent_id: String,
        project_path: String,
    },
    /// 工具调用
    ToolUse {
        agent_id: String,
        tool_name: String,
        tool_target: Option<String>,
        timestamp: Option<String>,
    },
    /// 工具调用批次（多个工具调用合并）
    ToolUseBatch {
        agent_id: String,
        tools: Vec<String>,
        timestamp: Option<String>,
    },
    /// 错误
    Error {
        agent_id: String,
        message: String,
        timestamp: Option<String>,
    },
    /// 等待输入
    WaitingForInput {
        agent_id: String,
        pattern_type: String,
        context: String,
    },
    /// Agent 恢复运行（从等待状态）
    AgentResumed {
        agent_id: String,
    },
}

/// Agent 状态快照
#[derive(Debug, Clone)]
pub struct AgentSnapshot {
    /// Agent 记录
    pub record: AgentRecord,
    /// 最近的工具调用
    pub recent_tools: Vec<JsonlEvent>,
    /// 最近的错误
    pub recent_errors: Vec<JsonlEvent>,
    /// 是否在等待输入
    pub waiting_for_input: Option<InputWaitResult>,
    /// 最后活动时间
    pub last_activity: Option<String>,
}

/// Agent 监控器
pub struct AgentWatcher {
    /// Agent 管理器
    agent_manager: AgentManager,
    /// tmux 管理器
    tmux: TmuxManager,
    /// 输入等待检测器
    input_detector: InputWaitDetector,
    /// 每个 agent 的 JSONL 解析器
    jsonl_parsers: HashMap<String, JsonlParser>,
    /// 每个 agent 的上次等待状态
    last_waiting_state: HashMap<String, bool>,
}

impl AgentWatcher {
    /// 创建新的监控器
    pub fn new() -> Self {
        Self {
            agent_manager: AgentManager::new(),
            tmux: TmuxManager::new(),
            input_detector: InputWaitDetector::new(),
            jsonl_parsers: HashMap::new(),
            last_waiting_state: HashMap::new(),
        }
    }

    /// 创建用于测试的监控器
    pub fn new_for_test() -> Self {
        Self {
            agent_manager: AgentManager::new_for_test(),
            tmux: TmuxManager::new(),
            input_detector: InputWaitDetector::new(),
            jsonl_parsers: HashMap::new(),
            last_waiting_state: HashMap::new(),
        }
    }

    /// 执行一次轮询，返回检测到的事件
    pub fn poll_once(&mut self) -> Result<Vec<WatchEvent>> {
        let mut events = Vec::new();

        // 获取所有活跃的 agent
        let agents = self.agent_manager.list_agents()?;
        eprintln!("轮询 {} 个 agent", agents.len());
        for agent in &agents {
            eprintln!("  - {}", agent.agent_id);
        }

        // 检查每个 agent
        for agent in &agents {
            // 1. 检查 tmux session 是否存活
            if !self.tmux.session_exists(&agent.tmux_session) {
                events.push(WatchEvent::AgentExited {
                    agent_id: agent.agent_id.clone(),
                    project_path: agent.project_path.clone(),
                });
                self.cleanup_agent(&agent.agent_id);
                continue;
            }

            // 2. 解析 JSONL 新事件
            if let Some(ref jsonl_path) = agent.jsonl_path {
                let parser = self.jsonl_parsers
                    .entry(agent.agent_id.clone())
                    .or_insert_with(|| {
                        let mut p = JsonlParser::new(jsonl_path);
                        p.set_position(agent.jsonl_offset);
                        p
                    });

                if let Ok(new_events) = parser.read_new_events() {
                    for event in new_events {
                        match &event {
                            JsonlEvent::ToolUse { tool_name, input, timestamp, .. } => {
                                let tool_target = crate::jsonl_parser::extract_tool_target_from_input(tool_name, input);
                                events.push(WatchEvent::ToolUse {
                                    agent_id: agent.agent_id.clone(),
                                    tool_name: tool_name.clone(),
                                    tool_target,
                                    timestamp: timestamp.clone(),
                                });
                            }
                            JsonlEvent::Error { message, timestamp } => {
                                events.push(WatchEvent::Error {
                                    agent_id: agent.agent_id.clone(),
                                    message: message.clone(),
                                    timestamp: timestamp.clone(),
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }

            // 3. 检测输入等待状态
            if let Ok(output) = self.tmux.capture_pane(&agent.tmux_session, 30) {
                let wait_result = self.input_detector.detect_immediate(&output);
                eprintln!("  {} 检测结果: is_waiting={}, pattern={:?}",
                    agent.agent_id,
                    wait_result.is_waiting,
                    wait_result.pattern_type);
                let was_waiting = self.last_waiting_state.get(&agent.agent_id).copied().unwrap_or(false);
                eprintln!("  {} was_waiting={}", agent.agent_id, was_waiting);

                if wait_result.is_waiting && !was_waiting {
                    // 新进入等待状态
                    let pattern_type = wait_result.pattern_type
                        .as_ref()
                        .map(|p| format!("{:?}", p))
                        .unwrap_or_else(|| "Unknown".to_string());

                    events.push(WatchEvent::WaitingForInput {
                        agent_id: agent.agent_id.clone(),
                        pattern_type,
                        context: wait_result.context.clone(),
                    });
                } else if !wait_result.is_waiting && was_waiting {
                    // 从等待状态恢复
                    events.push(WatchEvent::AgentResumed {
                        agent_id: agent.agent_id.clone(),
                    });
                }

                self.last_waiting_state.insert(agent.agent_id.clone(), wait_result.is_waiting);
            }
        }

        Ok(events)
    }

    /// 获取 agent 的当前状态快照
    pub fn get_agent_snapshot(&mut self, agent_id: &str) -> Result<Option<AgentSnapshot>> {
        let agent = match self.agent_manager.get_agent(agent_id)? {
            Some(a) => a,
            None => return Ok(None),
        };

        // 获取最近的工具调用和错误
        let (recent_tools, recent_errors) = if let Some(ref jsonl_path) = agent.jsonl_path {
            let mut parser = JsonlParser::new(jsonl_path);
            let tools = parser.get_recent_tool_calls(5).unwrap_or_default();
            let errors = parser.get_recent_errors(3).unwrap_or_default();
            (tools, errors)
        } else {
            (Vec::new(), Vec::new())
        };

        // 检测输入等待状态
        let waiting_for_input = if let Ok(output) = self.tmux.capture_pane(&agent.tmux_session, 20) {
            let result = self.input_detector.detect_immediate(&output);
            if result.is_waiting {
                Some(result)
            } else {
                None
            }
        } else {
            None
        };

        // 获取最后活动时间
        let last_activity = recent_tools.last()
            .and_then(|e| {
                if let JsonlEvent::ToolUse { timestamp, .. } = e {
                    timestamp.clone()
                } else {
                    None
                }
            });

        Ok(Some(AgentSnapshot {
            record: agent,
            recent_tools,
            recent_errors,
            waiting_for_input,
            last_activity,
        }))
    }

    /// 获取所有 agent 的状态快照
    pub fn get_all_snapshots(&mut self) -> Result<Vec<AgentSnapshot>> {
        let agents = self.agent_manager.list_agents()?;
        let mut snapshots = Vec::new();

        for agent in agents {
            if let Some(snapshot) = self.get_agent_snapshot(&agent.agent_id)? {
                snapshots.push(snapshot);
            }
        }

        Ok(snapshots)
    }

    /// 清理 agent 相关状态
    fn cleanup_agent(&mut self, agent_id: &str) {
        self.jsonl_parsers.remove(agent_id);
        self.last_waiting_state.remove(agent_id);
        self.input_detector.clear_session(agent_id);
    }

    /// 获取 agent 管理器引用
    pub fn agent_manager(&self) -> &AgentManager {
        &self.agent_manager
    }

    /// 获取 agent 管理器可变引用
    pub fn agent_manager_mut(&mut self) -> &mut AgentManager {
        &mut self.agent_manager
    }

    /// 轮询一次并只返回关键事件（退出、错误、等待输入）
    pub fn poll_critical_events(&mut self) -> Result<Vec<WatchEvent>> {
        let all_events = self.poll_once()?;

        Ok(all_events
            .into_iter()
            .filter(|e| matches!(
                e,
                WatchEvent::AgentExited { .. } |
                WatchEvent::Error { .. } |
                WatchEvent::WaitingForInput { .. }
            ))
            .collect())
    }
}

impl Default for AgentWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// 格式化 WatchEvent 为人类可读的通知消息
pub fn format_watch_event(event: &WatchEvent) -> String {
    match event {
        WatchEvent::AgentExited { agent_id, project_path } => {
            format!("✅ Agent 退出: {} ({})", agent_id, project_path)
        }
        WatchEvent::ToolUse { agent_id, tool_name, tool_target, .. } => {
            if let Some(target) = tool_target {
                format!("🔧 {} 执行: {} {}", agent_id, tool_name, target)
            } else {
                format!("🔧 {} 执行: {}", agent_id, tool_name)
            }
        }
        WatchEvent::ToolUseBatch { agent_id, tools, .. } => {
            format!("🔧 {} 执行: {}", agent_id, tools.join(", "))
        }
        WatchEvent::Error { agent_id, message, .. } => {
            let preview = if message.len() > 100 {
                format!("{}...", &message[..97])
            } else {
                message.clone()
            };
            format!("❌ {} 错误: {}", agent_id, preview)
        }
        WatchEvent::WaitingForInput { agent_id, pattern_type, context } => {
            let preview = if context.len() > 200 {
                format!("{}...", &context[..197])
            } else {
                context.clone()
            };
            format!("⏸️ {} 等待输入 ({}):\n{}", agent_id, pattern_type, preview)
        }
        WatchEvent::AgentResumed { agent_id } => {
            format!("▶️ {} 继续执行", agent_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_watch_event_agent_exited() {
        let event = WatchEvent::AgentExited {
            agent_id: "cam-123".to_string(),
            project_path: "/workspace/myapp".to_string(),
        };

        let formatted = format_watch_event(&event);
        assert!(formatted.contains("cam-123"));
        assert!(formatted.contains("退出"));
    }

    #[test]
    fn test_format_watch_event_tool_use() {
        let event = WatchEvent::ToolUse {
            agent_id: "cam-123".to_string(),
            tool_name: "Edit".to_string(),
            tool_target: Some("main.rs".to_string()),
            timestamp: None,
        };

        let formatted = format_watch_event(&event);
        assert!(formatted.contains("Edit"));
        assert!(formatted.contains("main.rs"));
    }

    #[test]
    fn test_format_watch_event_waiting() {
        let event = WatchEvent::WaitingForInput {
            agent_id: "cam-123".to_string(),
            pattern_type: "Confirmation".to_string(),
            context: "Continue? [Y/n]".to_string(),
        };

        let formatted = format_watch_event(&event);
        assert!(formatted.contains("等待输入"));
        assert!(formatted.contains("Confirmation"));
    }

    #[test]
    fn test_poll_critical_events_filters() {
        // 这个测试验证过滤逻辑的正确性
        let events = vec![
            WatchEvent::ToolUse {
                agent_id: "cam-123".to_string(),
                tool_name: "Read".to_string(),
                tool_target: None,
                timestamp: None,
            },
            WatchEvent::AgentExited {
                agent_id: "cam-123".to_string(),
                project_path: "/tmp".to_string(),
            },
            WatchEvent::AgentResumed {
                agent_id: "cam-123".to_string(),
            },
            WatchEvent::Error {
                agent_id: "cam-123".to_string(),
                message: "error".to_string(),
                timestamp: None,
            },
        ];

        let critical: Vec<_> = events
            .into_iter()
            .filter(|e| matches!(
                e,
                WatchEvent::AgentExited { .. } |
                WatchEvent::Error { .. } |
                WatchEvent::WaitingForInput { .. }
            ))
            .collect();

        assert_eq!(critical.len(), 2);
    }
}
