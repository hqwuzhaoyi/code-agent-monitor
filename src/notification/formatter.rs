//! 消息格式化模块 - 将事件转换为用户可读的通知消息
//!
//! 主要功能：
//! - 格式化各类事件为简洁的通知消息
//! - 智能提取项目名和问题内容
//! - 使用 AI 辅助提取（Haiku）
//!
//! 设计原则：
//! 1. 简洁 - 一眼看懂，核心内容不超过 5 行
//! 2. 可操作 - 明确告诉用户怎么做
//! 3. 专业 - 现代机器人风格，无冗余信息
//! 4. 友好 ID - 用项目名替代 cam-xxxxxxxxxx
//! 5. 无硬编码 - 使用 AI 判断，兼容多种 AI 编码工具

use std::fs;

use super::event::{NotificationEvent, NotificationEventType};
use crate::anthropic::{extract_question_with_haiku, ExtractedQuestion, ExtractionResult, TaskSummary};
use crate::notification_summarizer::NotificationSummarizer;

/// Notification message constants (Chinese)
pub mod msg {
    // Reply hints
    pub const REPLY_YN: &str = "回复 y 允许 / n 拒绝";
    #[allow(dead_code)]
    pub const REPLY_CONTENT: &str = "回复内容";
    #[allow(dead_code)]
    pub const REPLY_NUMBER: &str = "回复数字选择";

    // Status labels
    pub const WAITING_INPUT: &str = "等待输入";
    #[allow(dead_code)]
    pub const WAITING_SELECT: &str = "等待选择";
    pub const NEED_CONFIRM: &str = "需要确认";
    pub const REQUEST_PERMISSION: &str = "请求权限";
    pub const COMPLETED: &str = "已完成";
    pub const ERROR_OCCURRED: &str = "发生错误";
    #[allow(dead_code)]
    pub const AGENT_EXITED: &str = "Agent 已退出";
    pub const STOPPED: &str = "已停止";
    pub const SESSION_ENDED: &str = "会话已结束";
    #[allow(dead_code)]
    pub const SESSION_STARTED: &str = "会话已启动";
    #[allow(dead_code)]
    pub const NOTIFICATION: &str = "通知";
    #[allow(dead_code)]
    pub const NEED_PERMISSION_CONFIRM: &str = "需要权限确认";
    #[allow(dead_code)]
    pub const WAITING_USER_INPUT: &str = "等待用户输入";

    // Action labels
    pub const EXECUTE: &str = "执行";
    #[allow(dead_code)]
    pub const EXECUTE_TOOL: &str = "执行工具";
    #[allow(dead_code)]
    pub const REQUEST_EXECUTE_TOOL: &str = "请求执行";
}

/// 消息格式化器
pub struct MessageFormatter {
    /// 是否禁用 AI 提取
    no_ai: bool,
}

impl MessageFormatter {
    /// 创建新的 MessageFormatter
    pub fn new() -> Self {
        Self { no_ai: false }
    }

    /// 设置是否禁用 AI 提取
    pub fn with_no_ai(mut self, no_ai: bool) -> Self {
        self.no_ai = no_ai;
        self
    }

    /// 从路径提取项目名（最后一个目录名）
    pub fn extract_project_name(path: &str) -> String {
        if path.is_empty() {
            return "unknown".to_string();
        }
        std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    }

    /// 从 agent_id 获取项目名
    /// 优先从 agents.json 查找，否则返回 agent_id
    pub fn get_project_name_for_agent(agent_id: &str) -> String {
        // 尝试从 agents.json 读取项目路径
        if let Some(home) = dirs::home_dir() {
            let agents_path = home.join(".config/code-agent-monitor/agents.json");
            if let Ok(content) = fs::read_to_string(&agents_path) {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(agents) = data.get("agents").and_then(|a| a.as_array()) {
                        for agent in agents {
                            if agent.get("agent_id").and_then(|v| v.as_str()) == Some(agent_id) {
                                if let Some(path) = agent.get("project_path").and_then(|v| v.as_str()) {
                                    return Self::extract_project_name(path);
                                }
                            }
                        }
                    }
                }
            }
        }
        // 如果找不到，返回简化的 agent_id
        if agent_id.starts_with("cam-") && agent_id.len() > 8 {
            format!("agent-{}", &agent_id[4..8])
        } else if agent_id.starts_with("ext-") && agent_id.len() > 8 {
            // 外部会话：ext-xxxxxxxx -> session-xxxx
            format!("session-{}", &agent_id[4..8])
        } else {
            agent_id.to_string()
        }
    }

    /// 格式化提取的问题（包含选项）
    fn format_extracted_question(
        project_name: &str,
        extracted: &ExtractedQuestion,
    ) -> String {
        // 根据问题类型选择不同的 emoji 和标签
        let (emoji, label) = match extracted.question_type.as_str() {
            "choice" => ("📋", "请选择"),
            "confirm" => ("🔔", "请确认"),
            "open" => ("❓", "有问题"),
            _ => ("⏸️", msg::WAITING_INPUT),
        };

        let mut result = format!(
            "{} {} {}\n\n{}",
            emoji, project_name, label, extracted.question
        );

        // 如果有选项，添加选项列表
        if !extracted.options.is_empty() {
            result.push_str("\n");
            for option in &extracted.options {
                result.push_str(&format!("\n{}", option));
            }
            // 选择题显示回复数字提示
            let n = extracted.options.len();
            result.push_str(&format!("\n\n回复数字 (1-{})", n));
        } else if extracted.question_type == "confirm" {
            result.push_str("\n\ny 确认 / n 取消");
        } else {
            result.push_str(&format!("\n\n{}", extracted.reply_hint));
        }

        result
    }

    /// 格式化无问题场景（显示任务摘要）
    fn format_no_question(project_name: &str, summary: &TaskSummary) -> String {
        match (summary.status.as_str(), &summary.last_action) {
            ("completed", Some(action)) => {
                format!("✅ {} 已完成\n\n{}\n\n回复继续", project_name, action)
            }
            ("completed", None) => {
                format!("✅ {} 已完成任务\n\n回复继续", project_name)
            }
            (_, Some(action)) => {
                format!("💤 {} 空闲中\n\n最后操作：{}\n\n回复继续", project_name, action)
            }
            _ => {
                format!("💤 {} 等待指令", project_name)
            }
        }
    }

    /// 格式化事件消息（新设计：简洁、可操作、专业）
    ///
    /// 设计原则：
    /// 1. 用项目名替代 agent_id
    /// 2. 智能提取问题和选项
    /// 3. 移除技术细节
    /// 4. 简化回复指引
    pub fn format_event(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> String {
        // 分离终端快照和原始 context
        let (raw_context, terminal_snapshot) = if let Some(idx) = context.find("\n\n--- 终端快照 ---\n") {
            let (before, after) = context.split_at(idx);
            let snapshot = after.trim_start_matches("\n\n--- 终端快照 ---\n");
            (before, Some(snapshot))
        } else {
            (context, None)
        };

        // 尝试解析 JSON context 获取更多信息
        let json: Option<serde_json::Value> = serde_json::from_str(raw_context).ok();

        // 获取项目名（优先从 JSON 的 cwd，否则从 agent_id 查找）
        let project_name = json.as_ref()
            .and_then(|j| j.get("cwd"))
            .and_then(|v| v.as_str())
            .map(Self::extract_project_name)
            .unwrap_or_else(|| {
                if !pattern_or_path.is_empty() {
                    Self::extract_project_name(pattern_or_path)
                } else {
                    Self::get_project_name_for_agent(agent_id)
                }
            });

        // 终端快照（保留原始内容，AI 提取时使用）
        let snapshot = terminal_snapshot
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        match event_type {
            "permission_request" => {
                self.format_permission_request(&project_name, &json, &snapshot)
            }
            "notification" => {
                self.format_notification(&project_name, &json, &snapshot)
            }
            "session_start" => {
                format!("🚀 {} 已启动", project_name)
            }
            "session_end" => {
                format!("🔚 {} {}", project_name, msg::SESSION_ENDED)
            }
            "stop" => {
                format!("⏹️ {} {}", project_name, msg::STOPPED)
            }
            "WaitingForInput" => {
                self.format_waiting_for_input(&project_name, pattern_or_path, raw_context, &snapshot)
            }
            "Error" => {
                self.format_error(&project_name, raw_context, &snapshot)
            }
            "AgentExited" => {
                format!("✅ {} {}", project_name, msg::COMPLETED)
            }
            "ToolUse" => {
                // pattern_or_path = tool_name, raw_context = tool_target
                if raw_context.is_empty() {
                    format!("🔧 {} {} {}", project_name, msg::EXECUTE, pattern_or_path)
                } else {
                    format!("🔧 {} {} {} → {}", project_name, msg::EXECUTE, pattern_or_path, raw_context)
                }
            }
            _ => format!("{} - {}", project_name, event_type),
        }
    }

    /// 格式化权限请求通知
    fn format_permission_request(
        &self,
        project_name: &str,
        json: &Option<serde_json::Value>,
        _snapshot: &Option<String>,
    ) -> String {
        let tool_name = json.as_ref()
            .and_then(|j| j.get("tool_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let tool_input = json.as_ref()
            .and_then(|j| j.get("tool_input"))
            .cloned()
            .unwrap_or(serde_json::json!({}));

        // 使用 NotificationSummarizer 进行风险评估
        let summarizer = NotificationSummarizer::new();
        let summary = summarizer.summarize_permission(tool_name, &tool_input);

        // 提取关键参数用于显示
        let key_param = match tool_name {
            "Bash" => tool_input.get("command").and_then(|v| v.as_str()),
            "Write" | "Edit" | "Read" => tool_input.get("file_path").and_then(|v| v.as_str()),
            _ => tool_input.get("file_path")
                .or_else(|| tool_input.get("path"))
                .or_else(|| tool_input.get("command"))
                .and_then(|v| v.as_str())
        };

        let param_line = key_param
            .map(|p| {
                // 截断过长的参数
                if p.len() > 60 {
                    format!("{}...", &p[..57])
                } else {
                    p.to_string()
                }
            })
            .map(|p| format!("\n{}", p))
            .unwrap_or_default();

        // 根据风险等级选择 emoji
        let risk_emoji = summary.risk_level.emoji();

        format!(
            "{} {} {}\n\n{}\n{}: {}{}\n\n{}",
            risk_emoji, project_name, msg::REQUEST_PERMISSION,
            summary.recommendation, msg::EXECUTE, tool_name, param_line,
            msg::REPLY_YN
        )
    }

    /// 格式化通知事件
    fn format_notification(
        &self,
        project_name: &str,
        json: &Option<serde_json::Value>,
        snapshot: &Option<String>,
    ) -> String {
        let notification_type = json.as_ref()
            .and_then(|j| j.get("notification_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let message = json.as_ref()
            .and_then(|j| j.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match notification_type {
            "idle_prompt" => {
                // 空闲等待 - 使用 Haiku 提取或显示原始内容
                if let Some(snap) = snapshot {
                    if snap.trim().is_empty() {
                        return format!("⏸️ {} {}", project_name, msg::WAITING_INPUT);
                    }

                    // 尝试使用 Haiku 提取问题
                    if !self.no_ai {
                        match extract_question_with_haiku(snap) {
                            ExtractionResult::Found(extracted) => {
                                return Self::format_extracted_question(&project_name, &extracted);
                            }
                            ExtractionResult::NoQuestion(summary) => {
                                // AI 判断没有问题，显示任务摘要
                                return Self::format_no_question(&project_name, &summary);
                            }
                            ExtractionResult::Failed => {
                                // AI 提取失败，提示用户查看终端
                                return format!(
                                    "⏸️ {} {}\n\n无法解析通知内容，请查看终端",
                                    project_name, msg::WAITING_INPUT
                                );
                            }
                        }
                    }

                    // AI 禁用时，显示简洁提示
                    format!(
                        "⏸️ {} {}\n\n无法解析通知内容，请查看终端",
                        project_name, msg::WAITING_INPUT
                    )
                } else if !message.is_empty() {
                    format!("⏸️ {} {}\n\n{}", project_name, msg::WAITING_INPUT, message)
                } else {
                    format!("⏸️ {} {}", project_name, msg::WAITING_INPUT)
                }
            }
            "permission_prompt" => {
                // 权限确认 - 优先使用 AI 提取
                if !self.no_ai {
                    if let Some(snap) = snapshot {
                        if !snap.trim().is_empty() {
                            if let ExtractionResult::Found(extracted) = extract_question_with_haiku(snap) {
                                return format!(
                                    "🔐 {} {}\n\n{}\n\n{}",
                                    project_name, msg::NEED_CONFIRM, extracted.question, msg::REPLY_YN
                                );
                            }
                        }
                    }
                }

                // AI 提取失败，使用 message 或简洁提示
                if !message.is_empty() {
                    format!(
                        "🔐 {} {}\n\n{}\n\n{}",
                        project_name, msg::NEED_CONFIRM, message, msg::REPLY_YN
                    )
                } else {
                    format!(
                        "🔐 {} {}\n\n{}",
                        project_name, msg::NEED_CONFIRM, msg::REPLY_YN
                    )
                }
            }
            _ => {
                if !message.is_empty() {
                    format!("📢 {} {}", project_name, message)
                } else {
                    format!("📢 {} 通知", project_name)
                }
            }
        }
    }

    /// 格式化等待输入事件
    fn format_waiting_for_input(
        &self,
        project_name: &str,
        _pattern_type: &str,
        raw_context: &str,
        snapshot: &Option<String>,
    ) -> String {
        let context = snapshot.as_deref().unwrap_or(raw_context);

        if context.trim().is_empty() {
            return format!("⏸️ {} {}", project_name, msg::WAITING_INPUT);
        }

        // 使用 Haiku 提取问题
        if !self.no_ai {
            match extract_question_with_haiku(context) {
                ExtractionResult::Found(extracted) => {
                    return Self::format_extracted_question(project_name, &extracted);
                }
                ExtractionResult::NoQuestion(summary) => {
                    // AI 判断没有问题，显示任务摘要
                    return Self::format_no_question(project_name, &summary);
                }
                ExtractionResult::Failed => {
                    // AI 提取失败，提示用户查看终端
                }
            }
        }

        // AI 提取失败或禁用，返回简洁提示
        format!("⏸️ {} {}\n\n无法解析通知内容，请查看终端", project_name, msg::WAITING_INPUT)
    }

    /// 格式化错误通知
    fn format_error(
        &self,
        project_name: &str,
        raw_context: &str,
        _snapshot: &Option<String>,
    ) -> String {
        // 提取错误摘要（第一行或前 100 字符）
        let summary = raw_context.lines().next()
            .map(|line| {
                if line.len() > 100 {
                    format!("{}...", &line[..97])
                } else {
                    line.to_string()
                }
            })
            .unwrap_or_else(|| {
                if raw_context.len() > 100 {
                    format!("{}...", &raw_context[..97])
                } else {
                    raw_context.to_string()
                }
            });

        format!(
            "❌ {} {}\n\n{}",
            project_name, msg::ERROR_OCCURRED, summary
        )
    }

    /// 格式化统一的 NotificationEvent（新 API）
    ///
    /// 这是新的统一入口，使用 NotificationEvent 结构体替代多个参数。
    /// 优势：
    /// 1. 项目名从 event.project_path 获取，不再依赖 pattern_or_path
    /// 2. 终端快照从 event.terminal_snapshot 获取，数据来源清晰
    /// 3. 类型安全，避免参数混淆
    pub fn format_notification_event(&self, event: &NotificationEvent) -> String {
        let project_name = event.project_name().to_string();
        let snapshot = event.terminal_snapshot.clone();

        match &event.event_type {
            NotificationEventType::WaitingForInput { pattern_type } => {
                self.format_waiting_for_input_event(&project_name, pattern_type, &snapshot)
            }
            NotificationEventType::PermissionRequest { tool_name, tool_input } => {
                self.format_permission_request_event(&project_name, tool_name, tool_input)
            }
            NotificationEventType::Notification { notification_type, message } => {
                self.format_notification_type_event(&project_name, notification_type, message, &snapshot)
            }
            NotificationEventType::AgentExited => {
                format!("✅ {} {}", project_name, msg::COMPLETED)
            }
            NotificationEventType::Error { message } => {
                self.format_error_event(&project_name, message)
            }
            NotificationEventType::Stop => {
                format!("⏹️ {} {}", project_name, msg::STOPPED)
            }
            NotificationEventType::SessionStart => {
                format!("🚀 {} 已启动", project_name)
            }
            NotificationEventType::SessionEnd => {
                format!("🔚 {} {}", project_name, msg::SESSION_ENDED)
            }
        }
    }

    /// 格式化等待输入事件（新 API 内部方法）
    fn format_waiting_for_input_event(
        &self,
        project_name: &str,
        _pattern_type: &str,
        snapshot: &Option<String>,
    ) -> String {
        if let Some(snap) = snapshot {
            if snap.trim().is_empty() {
                return format!("⏸️ {} {}", project_name, msg::WAITING_INPUT);
            }

            // 尝试使用 Haiku 提取问题
            if !self.no_ai {
                match extract_question_with_haiku(snap) {
                    ExtractionResult::Found(extracted) => {
                        return Self::format_extracted_question(project_name, &extracted);
                    }
                    ExtractionResult::NoQuestion(summary) => {
                        // AI 判断没有问题，显示任务摘要
                        return Self::format_no_question(project_name, &summary);
                    }
                    ExtractionResult::Failed => {
                        // AI 提取失败，提示用户查看终端
                    }
                }
            }

            // AI 提取失败或禁用，显示简洁提示
            format!("⏸️ {} {}\n\n无法解析通知内容，请查看终端", project_name, msg::WAITING_INPUT)
        } else {
            format!("⏸️ {} {}", project_name, msg::WAITING_INPUT)
        }
    }

    /// 格式化权限请求事件（新 API 内部方法）
    fn format_permission_request_event(
        &self,
        project_name: &str,
        tool_name: &str,
        tool_input: &serde_json::Value,
    ) -> String {
        // 使用 NotificationSummarizer 进行风险评估
        let summarizer = NotificationSummarizer::new();
        let summary = summarizer.summarize_permission(tool_name, tool_input);

        // 提取关键参数用于显示
        let key_param = match tool_name {
            "Bash" => tool_input.get("command").and_then(|v| v.as_str()),
            "Write" | "Edit" | "Read" => tool_input.get("file_path").and_then(|v| v.as_str()),
            _ => tool_input.get("file_path")
                .or_else(|| tool_input.get("path"))
                .or_else(|| tool_input.get("command"))
                .and_then(|v| v.as_str())
        };

        let param_line = key_param
            .map(|p| {
                if p.len() > 60 {
                    format!("{}...", &p[..57])
                } else {
                    p.to_string()
                }
            })
            .map(|p| format!("\n{}", p))
            .unwrap_or_default();

        let risk_emoji = summary.risk_level.emoji();

        format!(
            "{} {} {}\n\n{}\n{}: {}{}\n\n{}",
            risk_emoji, project_name, msg::REQUEST_PERMISSION,
            summary.recommendation, msg::EXECUTE, tool_name, param_line,
            msg::REPLY_YN
        )
    }

    /// 格式化通知类型事件（新 API 内部方法）
    fn format_notification_type_event(
        &self,
        project_name: &str,
        notification_type: &str,
        message: &str,
        snapshot: &Option<String>,
    ) -> String {
        match notification_type {
            "idle_prompt" => {
                if let Some(snap) = snapshot {
                    if snap.trim().is_empty() {
                        return format!("⏸️ {} {}", project_name, msg::WAITING_INPUT);
                    }

                    // 尝试使用 Haiku 提取问题
                    if !self.no_ai {
                        match extract_question_with_haiku(snap) {
                            ExtractionResult::Found(extracted) => {
                                return Self::format_extracted_question(&project_name, &extracted);
                            }
                            ExtractionResult::NoQuestion(summary) => {
                                // AI 判断没有问题，显示任务摘要
                                return Self::format_no_question(&project_name, &summary);
                            }
                            ExtractionResult::Failed => {
                                // AI 提取失败，提示用户查看终端
                            }
                        }
                    }

                    // AI 提取失败或禁用
                    format!(
                        "⏸️ {} {}\n\n无法解析通知内容，请查看终端",
                        project_name, msg::WAITING_INPUT
                    )
                } else if !message.is_empty() {
                    format!("⏸️ {} {}\n\n{}", project_name, msg::WAITING_INPUT, message)
                } else {
                    format!("⏸️ {} {}", project_name, msg::WAITING_INPUT)
                }
            }
            "permission_prompt" => {
                // 优先使用 AI 提取问题内容
                if !self.no_ai {
                    if let Some(snap) = snapshot {
                        if !snap.trim().is_empty() {
                            if let ExtractionResult::Found(extracted) = extract_question_with_haiku(snap) {
                                return format!(
                                    "🔐 {} {}\n\n{}\n\n{}",
                                    project_name, msg::NEED_CONFIRM, extracted.question, msg::REPLY_YN
                                );
                            }
                        }
                    }
                }

                // AI 提取失败，使用 message 或简洁提示
                if !message.is_empty() {
                    format!(
                        "🔐 {} {}\n\n{}\n\n{}",
                        project_name, msg::NEED_CONFIRM, message, msg::REPLY_YN
                    )
                } else {
                    format!(
                        "🔐 {} {}\n\n{}",
                        project_name, msg::NEED_CONFIRM, msg::REPLY_YN
                    )
                }
            }
            _ => {
                if !message.is_empty() {
                    format!("📢 {} {}", project_name, message)
                } else {
                    format!("📢 {} 通知", project_name)
                }
            }
        }
    }

    /// 格式化错误事件（新 API 内部方法）
    fn format_error_event(&self, project_name: &str, error_message: &str) -> String {
        let summary = error_message.lines().next()
            .map(|line| {
                if line.len() > 100 {
                    format!("{}...", &line[..97])
                } else {
                    line.to_string()
                }
            })
            .unwrap_or_else(|| {
                if error_message.len() > 100 {
                    format!("{}...", &error_message[..97])
                } else {
                    error_message.to_string()
                }
            });

        format!(
            "❌ {} {}\n\n{}",
            project_name, msg::ERROR_OCCURRED, summary
        )
    }
}

impl Default for MessageFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_waiting_event() {
        let formatter = MessageFormatter::new().with_no_ai(true);

        let message = formatter.format_event(
            "cam-1234567890",
            "WaitingForInput",
            "Confirmation",
            "Do you want to continue? [Y/n]",
        );

        // AI 禁用时，返回简洁提示而非原始内容
        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
        // 新行为：AI 提取失败时显示简洁提示
        assert!(message.contains("无法解析通知内容，请查看终端"));
    }

    #[test]
    fn test_format_error_event() {
        let formatter = MessageFormatter::new();

        let message = formatter.format_event(
            "cam-1234567890",
            "Error",
            "",
            "API rate limit exceeded",
        );

        assert!(message.contains("❌"));
        assert!(message.contains("错误"));
        assert!(message.contains("API rate limit"));
    }

    #[test]
    fn test_format_exited_event() {
        let formatter = MessageFormatter::new();

        let message = formatter.format_event(
            "cam-1234567890",
            "AgentExited",
            "/workspace/myapp",
            "",
        );

        // 新格式：使用项目名
        assert!(message.contains("✅"));
        assert!(message.contains("myapp") || message.contains("已完成"));
    }

    #[test]
    fn test_format_event_with_terminal_snapshot() {
        let formatter = MessageFormatter::new();

        // 模拟带终端快照的 context
        let context_with_snapshot = r#"{"cwd": "/workspace"}

--- 终端快照 ---
$ cargo build
   Compiling myapp v0.1.0
    Finished release target"#;

        let message = formatter.format_event(
            "cam-123",
            "stop",
            "",
            context_with_snapshot,
        );

        // 新格式：简洁，不再显示终端快照
        assert!(message.contains("⏹️"));
        assert!(message.contains("已停止") || message.contains("workspace"));
    }

    #[test]
    fn test_format_event_snapshot_truncation() {
        let formatter = MessageFormatter::new();

        // 创建超过 15 行的终端输出
        let mut long_output = String::from(r#"{"cwd": "/tmp"}

--- 终端快照 ---
"#);
        for i in 1..=20 {
            long_output.push_str(&format!("line {}\n", i));
        }

        let message = formatter.format_event(
            "cam-123",
            "stop",
            "",
            &long_output,
        );

        // 新格式：简洁，不再显示终端快照
        assert!(message.contains("⏹️"));
        assert!(message.contains("已停止") || message.contains("tmp"));
    }

    #[test]
    fn test_format_event_without_snapshot() {
        let formatter = MessageFormatter::new();

        let message = formatter.format_event(
            "cam-123",
            "stop",
            "",
            r#"{"cwd": "/workspace"}"#,
        );

        assert!(message.contains("⏹️"));
        assert!(message.contains("已停止") || message.contains("workspace"));
    }

    #[test]
    fn test_format_permission_request() {
        let formatter = MessageFormatter::new();

        let context = r#"{"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/test"}, "cwd": "/workspace"}"#;
        let message = formatter.format_event("cam-123", "permission_request", "", context);

        // 新格式：使用风险等级 emoji（✅/⚠️/🔴）替代固定的 🔐
        // rm -rf /tmp/test 是低风险（/tmp 路径）
        assert!(message.contains("✅") || message.contains("⚠️") || message.contains("🔴"));
        assert!(message.contains("请求权限"));
        assert!(message.contains("Bash"));
        assert!(message.contains("rm -rf /tmp/test"));
        // 新格式：简化回复指引
        assert!(message.contains("y 允许") || message.contains("n 拒绝"));
    }

    #[test]
    fn test_format_notification_idle_prompt() {
        let formatter = MessageFormatter::new();

        let context = r#"{"notification_type": "idle_prompt", "message": "Task completed, waiting for next instruction"}"#;
        let message = formatter.format_event("cam-123", "notification", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
    }

    #[test]
    fn test_format_notification_permission_prompt() {
        let formatter = MessageFormatter::new();

        let context = r#"{"notification_type": "permission_prompt", "message": "Allow file write?"}"#;
        let message = formatter.format_event("cam-123", "notification", "", context);

        assert!(message.contains("🔐"));
        assert!(message.contains("确认") || message.contains("需要"));
        assert!(message.contains("Allow file write?"));
        // 新格式：简化回复指引
        assert!(message.contains("y") && message.contains("n"));
    }

    #[test]
    fn test_format_session_start() {
        let formatter = MessageFormatter::new();

        let context = r#"{"cwd": "/Users/admin/project"}"#;
        let message = formatter.format_event("cam-123", "session_start", "", context);

        assert!(message.contains("🚀"));
        assert!(message.contains("已启动"));
        // 新格式：使用项目名
        assert!(message.contains("project"));
    }

    #[test]
    fn test_format_stop_event() {
        let formatter = MessageFormatter::new();

        let context = r#"{"cwd": "/workspace/app"}"#;
        let message = formatter.format_event("cam-123", "stop", "", context);

        assert!(message.contains("⏹️"));
        assert!(message.contains("已停止") || message.contains("app"));
    }

    #[test]
    fn test_format_session_end() {
        let formatter = MessageFormatter::new();

        let context = r#"{"cwd": "/workspace"}"#;
        let message = formatter.format_event("cam-123", "session_end", "", context);

        assert!(message.contains("🔚"));
        assert!(message.contains("会话结束") || message.contains("workspace"));
    }

    #[test]
    fn test_format_agent_exited_with_snapshot() {
        let formatter = MessageFormatter::new();

        let context = r#"

--- 终端快照 ---
All tests passed!
Build successful."#;

        let message = formatter.format_event("cam-123", "AgentExited", "/myproject", context);

        // 新格式：简洁，使用项目名
        assert!(message.contains("✅"));
        assert!(message.contains("myproject") || message.contains("已完成"));
    }

    #[test]
    fn test_format_tool_use() {
        let formatter = MessageFormatter::new();

        // 带 target 的工具调用
        let message = formatter.format_event("cam-123", "ToolUse", "Edit", "src/main.rs");
        assert!(message.contains("🔧"));
        assert!(message.contains("Edit"));
        assert!(message.contains("src/main.rs"));

        // 不带 target 的工具调用
        let message2 = formatter.format_event("cam-456", "ToolUse", "Read", "");
        assert!(message2.contains("🔧"));
        assert!(message2.contains("Read"));
    }

    #[test]
    fn test_extract_project_name() {
        assert_eq!(MessageFormatter::extract_project_name("/Users/admin/workspace/myapp"), "myapp");
        assert_eq!(MessageFormatter::extract_project_name("/workspace"), "workspace");
        assert_eq!(MessageFormatter::extract_project_name(""), "unknown");
        // Root path returns "/" as the file_name
        assert_eq!(MessageFormatter::extract_project_name("/"), "/");
    }

    #[test]
    fn test_get_project_name_for_agent() {
        // 测试 agent_id 简化（当 agents.json 中找不到时）
        let name = MessageFormatter::get_project_name_for_agent("cam-1234567890");
        assert_eq!(name, "agent-1234");

        // 短 agent_id 不简化
        let name2 = MessageFormatter::get_project_name_for_agent("cam-123");
        assert_eq!(name2, "cam-123");

        // 外部会话 agent_id 简化（当 agents.json 中找不到时）
        // 注意：如果 agents.json 中有此 agent，会返回实际项目名
        let name3 = MessageFormatter::get_project_name_for_agent("ext-nonexist");
        assert_eq!(name3, "session-none");

        // 短外部会话 agent_id 不简化
        let name4 = MessageFormatter::get_project_name_for_agent("ext-123");
        assert_eq!(name4, "ext-123");
    }

    #[test]
    fn test_format_notification_with_no_ai_fallback() {
        // 测试当 AI 禁用时，回退到简洁提示（不显示原始快照，避免 UI 元素泄露）
        let formatter = MessageFormatter::new().with_no_ai(true);

        let context = r#"{"notification_type": "idle_prompt", "message": ""}

--- 终端快照 ---
Some unrecognized prompt format that doesn't match any pattern
Please provide your input here"#;

        let message = formatter.format_event("cam-123", "notification", "", context);

        // 应该显示简洁提示，不显示原始快照内容
        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
        // 新行为：AI 提取失败时显示简洁提示，不显示原始快照
        assert!(message.contains("无法解析通知内容，请查看终端"));
    }

    #[test]
    fn test_format_notification_ai_extraction_path() {
        // 测试 AI 提取路径（不实际调用 AI，只验证代码路径）
        let formatter = MessageFormatter::new();

        let context = r#"{"notification_type": "idle_prompt", "message": ""}

--- 终端快照 ---
Some complex terminal output
That doesn't match standard patterns
But contains a question somewhere"#;

        // 默认模式下会尝试 AI 提取，如果失败则回退到显示原始快照
        let message = formatter.format_event("cam-123", "notification", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
    }

    // ==================== 修复验证测试：终端快照泄露问题 ====================

    #[test]
    fn test_ai_extraction_failure_does_not_leak_terminal_snapshot() {
        // 验证修复：当 AI 提取失败时，不应该将原始终端快照作为通知内容发送
        // 这是为了防止 UI 元素（如 ANSI 转义序列、进度条等）泄露到通知中
        let formatter = MessageFormatter::new().with_no_ai(true);

        // 模拟包含 UI 元素的终端快照
        let terminal_snapshot_with_ui = r#"
╭──────────────────────────────────────────────────────────────────────────────╮
│ Claude Code                                                                   │
├──────────────────────────────────────────────────────────────────────────────┤
│ > What would you like me to do?                                              │
│                                                                              │
│ [Thinking...] ████████████░░░░░░░░ 60%                                       │
╰──────────────────────────────────────────────────────────────────────────────╯"#;

        let context = format!(
            r#"{{"notification_type": "idle_prompt", "message": ""}}

--- 终端快照 ---
{}"#,
            terminal_snapshot_with_ui
        );

        let message = formatter.format_event("cam-123", "notification", "", &context);

        // 验证：不应该包含 UI 元素
        assert!(!message.contains("╭"));
        assert!(!message.contains("╰"));
        assert!(!message.contains("████"));
        assert!(!message.contains("░░░░"));

        // 验证：应该显示简洁的回退提示
        assert!(message.contains("无法解析通知内容，请查看终端"));
    }

    #[test]
    fn test_waiting_for_input_fallback_message() {
        // 验证 WaitingForInput 事件在 AI 提取失败时的回退行为
        let formatter = MessageFormatter::new().with_no_ai(true);

        let event = NotificationEvent::waiting_for_input("cam-test", "ClaudePrompt")
            .with_project_path("/workspace/myproject")
            .with_terminal_snapshot("Some unrecognized terminal content\nWith multiple lines\nAnd no clear question");

        let message = formatter.format_notification_event(&event);

        // 验证：显示项目名和状态
        assert!(message.contains("myproject"));
        assert!(message.contains("等待输入"));

        // 验证：显示回退提示而非原始快照
        assert!(message.contains("无法解析通知内容，请查看终端"));
        assert!(!message.contains("unrecognized terminal content"));
    }

    #[test]
    fn test_idle_prompt_fallback_message() {
        // 验证 idle_prompt 通知在 AI 提取失败时的回退行为
        let formatter = MessageFormatter::new().with_no_ai(true);

        let event = NotificationEvent::notification("cam-test", "idle_prompt", "")
            .with_project_path("/workspace/backend")
            .with_terminal_snapshot("Random terminal output that AI cannot parse");

        let message = formatter.format_notification_event(&event);

        // 验证：显示回退提示
        assert!(message.contains("无法解析通知内容，请查看终端"));
        assert!(!message.contains("Random terminal output"));
    }

    #[test]
    fn test_empty_snapshot_shows_simple_message() {
        // 验证空快照时显示简洁消息
        let formatter = MessageFormatter::new().with_no_ai(true);

        let event = NotificationEvent::waiting_for_input("cam-test", "ClaudePrompt")
            .with_project_path("/workspace/app");
        // 不设置 terminal_snapshot

        let message = formatter.format_notification_event(&event);

        // 验证：只显示基本状态，不显示回退提示
        assert!(message.contains("app"));
        assert!(message.contains("等待输入"));
        assert!(!message.contains("无法解析"));
    }

    #[test]
    fn test_whitespace_only_snapshot_treated_as_empty() {
        // 验证只有空白字符的快照被视为空
        let formatter = MessageFormatter::new().with_no_ai(true);

        let event = NotificationEvent::waiting_for_input("cam-test", "ClaudePrompt")
            .with_project_path("/workspace/app")
            .with_terminal_snapshot("   \n\n   \t  ");

        let message = formatter.format_notification_event(&event);

        // 验证：空白快照不触发回退提示
        assert!(message.contains("等待输入"));
        assert!(!message.contains("无法解析"));
    }

    // ========== 新 API (format_notification_event) 测试 ==========

    #[test]
    fn test_format_notification_event_waiting_for_input() {
        // 测试当 AI 禁用时，回退到简洁提示（不显示原始快照）
        let formatter = MessageFormatter::new().with_no_ai(true);

        let event = NotificationEvent::waiting_for_input("cam-123", "ClaudePrompt")
            .with_project_path("/Users/admin/workspace/myproject")
            .with_terminal_snapshot("Do you want to continue? [Y/n]");

        let message = formatter.format_notification_event(&event);

        assert!(message.contains("⏸️"));
        assert!(message.contains("myproject")); // 使用项目名而非 agent_id
        assert!(message.contains("等待输入"));
        // 新行为：AI 禁用时显示简洁提示，不显示原始快照
        assert!(message.contains("无法解析通知内容，请查看终端"));
    }

    #[test]
    fn test_format_notification_event_permission_request() {
        let formatter = MessageFormatter::new();

        let event = NotificationEvent::permission_request(
            "cam-456",
            "Bash",
            serde_json::json!({"command": "npm install"}),
        ).with_project_path("/workspace/frontend");

        let message = formatter.format_notification_event(&event);

        assert!(message.contains("frontend")); // 使用项目名
        assert!(message.contains("请求权限"));
        assert!(message.contains("Bash"));
        assert!(message.contains("npm install"));
    }

    #[test]
    fn test_format_notification_event_idle_prompt() {
        // 测试当 AI 禁用时，回退到简洁提示（不显示原始快照）
        let formatter = MessageFormatter::new().with_no_ai(true);

        let event = NotificationEvent::notification("cam-789", "idle_prompt", "")
            .with_project_path("/workspace/backend")
            .with_terminal_snapshot("What would you like me to do next?");

        let message = formatter.format_notification_event(&event);

        assert!(message.contains("⏸️"));
        assert!(message.contains("backend"));
        assert!(message.contains("等待输入"));
        // 新行为：AI 禁用时显示简洁提示，不显示原始快照
        assert!(message.contains("无法解析通知内容，请查看终端"));
    }

    #[test]
    fn test_format_notification_event_agent_exited() {
        let formatter = MessageFormatter::new();

        let event = NotificationEvent::agent_exited("cam-abc")
            .with_project_path("/workspace/api-server");

        let message = formatter.format_notification_event(&event);

        assert!(message.contains("✅"));
        assert!(message.contains("api-server"));
        assert!(message.contains("已完成"));
    }

    #[test]
    fn test_format_notification_event_error() {
        let formatter = MessageFormatter::new();

        let event = NotificationEvent::error("cam-def", "Connection timeout: API server unreachable")
            .with_project_path("/workspace/client");

        let message = formatter.format_notification_event(&event);

        assert!(message.contains("❌"));
        assert!(message.contains("client"));
        assert!(message.contains("错误"));
        assert!(message.contains("Connection timeout"));
    }

    #[test]
    fn test_format_notification_event_stop() {
        let formatter = MessageFormatter::new();

        let event = NotificationEvent::stop("cam-ghi")
            .with_project_path("/workspace/service");

        let message = formatter.format_notification_event(&event);

        assert!(message.contains("⏹️"));
        assert!(message.contains("service"));
        assert!(message.contains("已停止"));
    }

    #[test]
    fn test_format_notification_event_session_start() {
        let formatter = MessageFormatter::new();

        let event = NotificationEvent::session_start("cam-jkl")
            .with_project_path("/workspace/new-project");

        let message = formatter.format_notification_event(&event);

        assert!(message.contains("🚀"));
        assert!(message.contains("new-project"));
        assert!(message.contains("已启动"));
    }

    #[test]
    fn test_format_notification_event_session_end() {
        let formatter = MessageFormatter::new();

        let event = NotificationEvent::session_end("cam-mno")
            .with_project_path("/workspace/finished-project");

        let message = formatter.format_notification_event(&event);

        assert!(message.contains("🔚"));
        assert!(message.contains("finished-project"));
        assert!(message.contains("会话结束") || message.contains("会话已结束"));
    }

    #[test]
    fn test_format_notification_event_uses_agent_id_as_fallback() {
        let formatter = MessageFormatter::new();

        // 没有设置 project_path，应该使用 agent_id 作为项目名
        let event = NotificationEvent::agent_exited("cam-xyz");

        let message = formatter.format_notification_event(&event);

        assert!(message.contains("✅"));
        assert!(message.contains("cam-xyz")); // 回退到 agent_id
    }

    #[test]
    fn test_format_notification_event_permission_prompt() {
        let formatter = MessageFormatter::new().with_no_ai(true);

        let event = NotificationEvent::notification("cam-pqr", "permission_prompt", "Allow file write?")
            .with_project_path("/workspace/editor");

        let message = formatter.format_notification_event(&event);

        assert!(message.contains("🔐"));
        assert!(message.contains("editor"));
        assert!(message.contains("确认") || message.contains("需要"));
        assert!(message.contains("Allow file write?"));
    }
}
