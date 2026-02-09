//! OpenClaw 通知模块 - 通过 openclaw CLI 发送事件到 channel 或 agent
//!
//! 通知路由策略：
//! - HIGH/MEDIUM urgency → 通过 system event 发送结构化 payload（触发 heartbeat）
//! - LOW urgency → 静默处理（避免上下文累积）
//!
//! Payload 格式：
//! ```json
//! {
//!   "type": "cam_notification",
//!   "version": "1.0",
//!   "urgency": "HIGH",
//!   "event_type": "permission_request",
//!   "agent_id": "cam-xxx",
//!   "project": "/path/to/project",
//!   "event": { ... },
//!   "summary": "简短摘要"
//! }
//! ```
//!
//! 通知格式设计原则：
//! 1. 简洁 - 一眼看懂，核心内容不超过 5 行
//! 2. 可操作 - 明确告诉用户怎么做
//! 3. 专业 - 现代机器人风格，无冗余信息
//! 4. 友好 ID - 用项目名替代 cam-xxxxxxxxxx

use anyhow::Result;
use std::process::Command;
use std::fs;
use chrono::Utc;
use regex::Regex;
use tracing::{info, error, debug};
use crate::embedding::extract_question_with_embedding;
use crate::notification_summarizer::NotificationSummarizer;

/// Channel 配置
#[derive(Debug, Clone)]
pub struct ChannelConfig {
    /// channel 类型: telegram, whatsapp, discord, slack 等
    pub channel: String,
    /// 目标: chat_id, phone number, channel id 等
    pub target: String,
}

/// 通知发送结果
#[derive(Debug, Clone, PartialEq)]
pub enum SendResult {
    /// 通知已发送
    Sent,
    /// 静默跳过（LOW urgency 或外部会话）
    Skipped(String),
}

/// OpenClaw 通知器
pub struct OpenclawNotifier {
    /// openclaw 命令路径
    openclaw_cmd: String,
    /// 目标 session id（用于发送给 Agent）
    session_id: String,
    /// Channel 配置（用于直接发送）
    channel_config: Option<ChannelConfig>,
    /// 是否为 dry-run 模式（只打印不发送）
    dry_run: bool,
    /// 是否禁用 AI 提取（用于测试/调试）
    no_ai: bool,
}

impl OpenclawNotifier {
    /// 创建新的通知器
    pub fn new() -> Self {
        let channel_config = Self::detect_channel();
        Self {
            openclaw_cmd: Self::find_openclaw_path(),
            session_id: "main".to_string(),
            channel_config,
            dry_run: false,
            no_ai: false,
        }
    }

    /// 创建指定 session 的通知器
    pub fn with_session(session_id: &str) -> Self {
        let channel_config = Self::detect_channel();
        Self {
            openclaw_cmd: Self::find_openclaw_path(),
            session_id: session_id.to_string(),
            channel_config,
            dry_run: false,
            no_ai: false,
        }
    }

    /// 设置 dry-run 模式
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// 设置是否禁用 AI 提取
    pub fn with_no_ai(mut self, no_ai: bool) -> Self {
        self.no_ai = no_ai;
        self
    }

    /// 从 OpenClaw 配置自动检测 channel
    /// 按优先级检测: telegram > whatsapp > discord > slack > 其他
    fn detect_channel() -> Option<ChannelConfig> {
        let config_path = dirs::home_dir()?.join(".openclaw/openclaw.json");
        let content = fs::read_to_string(&config_path).ok()?;
        let config: serde_json::Value = serde_json::from_str(&content).ok()?;
        let channels = config.get("channels")?;

        // 按优先级尝试检测各个 channel
        // 1. Telegram
        if let Some(target) = Self::extract_telegram_target(channels) {
            return Some(ChannelConfig {
                channel: "telegram".to_string(),
                target,
            });
        }

        // 2. WhatsApp
        if let Some(target) = Self::extract_allow_from(channels, "whatsapp") {
            return Some(ChannelConfig {
                channel: "whatsapp".to_string(),
                target,
            });
        }

        // 3. Discord
        if let Some(target) = Self::extract_default_channel(channels, "discord") {
            return Some(ChannelConfig {
                channel: "discord".to_string(),
                target,
            });
        }

        // 4. Slack
        if let Some(target) = Self::extract_default_channel(channels, "slack") {
            return Some(ChannelConfig {
                channel: "slack".to_string(),
                target,
            });
        }

        // 5. Signal
        if let Some(target) = Self::extract_allow_from(channels, "signal") {
            return Some(ChannelConfig {
                channel: "signal".to_string(),
                target,
            });
        }

        None
    }

    /// 提取 Telegram target (chat_id)
    fn extract_telegram_target(channels: &serde_json::Value) -> Option<String> {
        let allow_from = channels
            .get("telegram")?
            .get("allowFrom")?
            .as_array()?;

        // allowFrom 本质是“入站发送者 allowlist”。这里用作出站通知收件人时，只能做启发式：
        // 取第一个“具体的”条目，并跳过 "*" 这种通配符（常见于 dmPolicy/groupPolicy="open" 配置）。
        for entry in allow_from {
            if let Some(s) = entry.as_str() {
                let s = s.trim();
                if s.is_empty() || s == "*" {
                    continue;
                }
                return Some(s.to_string());
            }
            if let Some(n) = entry.as_i64() {
                return Some(n.to_string());
            }
        }

        None
    }

    /// 提取 allowFrom 数组的第一个元素
    fn extract_allow_from(channels: &serde_json::Value, channel_name: &str) -> Option<String> {
        let allow_from = channels
            .get(channel_name)?
            .get("allowFrom")?
            .as_array()?;

        // 同 extract_telegram_target：跳过 "*" 这种通配符，选择第一个具体条目。
        for entry in allow_from {
            if let Some(s) = entry.as_str() {
                let s = s.trim();
                if s.is_empty() || s == "*" {
                    continue;
                }
                return Some(s.to_string());
            }
            if let Some(n) = entry.as_i64() {
                return Some(n.to_string());
            }
        }

        None
    }

    /// 提取 defaultChannel
    fn extract_default_channel(channels: &serde_json::Value, channel_name: &str) -> Option<String> {
        channels
            .get(channel_name)?
            .get("defaultChannel")?
            .as_str()
            .map(|s| s.to_string())
    }

    /// 查找 openclaw 可执行文件路径
    fn find_openclaw_path() -> String {
        let possible_paths = [
            "/Users/admin/.volta/bin/openclaw",
            "/opt/homebrew/bin/openclaw",
            "/usr/local/bin/openclaw",
            "openclaw",
        ];

        for path in possible_paths {
            if std::path::Path::new(path).exists() || path == "openclaw" {
                return path.to_string();
            }
        }

        "openclaw".to_string()
    }

    // ==================== 日志辅助函数 ====================

    /// 记录耗时日志到 hook.log
    fn log_timing(stage: &str, result: &str, duration: std::time::Duration) {
        use std::fs::OpenOptions;
        use std::io::Write;

        if let Some(home) = dirs::home_dir() {
            let log_path = home.join(".claude-monitor/hook.log");
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                let _ = writeln!(
                    file,
                    "[{}] ⏱️ {} {} took {}ms",
                    timestamp,
                    stage,
                    result,
                    duration.as_millis()
                );
            }
        }
    }

    // ==================== 通知格式化辅助函数 ====================

    /// 从路径提取项目名（最后一个目录名）
    fn extract_project_name(path: &str) -> String {
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
    fn get_project_name_for_agent(agent_id: &str) -> String {
        // 尝试从 agents.json 读取项目路径
        if let Some(home) = dirs::home_dir() {
            let agents_path = home.join(".claude-monitor/agents.json");
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

    /// 清洗终端上下文，移除噪音内容，只保留最近的问题和选项
    fn clean_terminal_context(raw: &str) -> String {
        // 第一步：找到最后一个用户输入行的位置
        // 用户输入行格式：❯ <content>（content 不为空）
        // 这样可以跳过已回答的问题，只处理当前等待回答的内容
        let raw_lines: Vec<&str> = raw.lines().collect();
        let mut last_user_input_idx = None;

        for (i, line) in raw_lines.iter().enumerate() {
            let trimmed = line.trim();
            // 匹配用户输入行：❯ 后跟非空内容（不是单独的 ❯）
            if trimmed.starts_with('❯') && trimmed.len() > 2 {
                let after_prompt = trimmed[3..].trim(); // 跳过 "❯ "
                // 排除占位符提示（如 Try "fix lint errors"）
                if !after_prompt.is_empty() && !after_prompt.starts_with("Try \"") {
                    last_user_input_idx = Some(i);
                }
            }
        }

        // 从最后一个用户输入行之后开始处理
        let start_idx = last_user_input_idx.map(|i| i + 1).unwrap_or(0);
        let content_to_process = raw_lines[start_idx..].join("\n");

        // 需要过滤的模式
        let noise_patterns = [
            // 状态栏（包含 MCPs, hooks, %, ⏱️, context window）
            r"(?m)^.*\d+\s*MCPs.*$",
            r"(?m)^.*\d+\s*hooks.*$",
            r"(?m)^.*\d+%.*context.*$",
            r"(?m)^.*⏱️.*$",
            r"(?m)^.*\[Opus.*\].*$",
            r"(?m)^.*git:\(.*\).*$",
            // 分隔线
            r"(?m)^[─━═\-]{3,}$",
            // 空行和单独提示符
            r"(?m)^[>❯]\s*$",
            r"(?m)^\s*$",
            // 📡 via direct 标记
            r"(?m)^.*📡\s*via\s*direct.*$",
            // Claude Code 框架线（只匹配纯框架字符行，不匹配目录树）
            // 目录树格式：│   ├── filename.txt（包含空格和文件名）
            // 框架线格式：╭───────────────╮ 或 │ content │（两端都有框架字符）
            r"(?m)^[╭╮╰╯][─━═\s]*[╭╮╰╯]?$",
            r"(?m)^│[^├└│]*│$",
            // 工具调用状态和思考状态
            r"(?m)^.*[✓◐⏺✻✶✽].*$",
            // Claude Code 思考/生成状态
            r"(?m)^.*Brewing.*$",
            r"(?m)^.*Thinking.*$",
            r"(?m)^.*Actioning.*$",
        ];

        let mut result = content_to_process;
        for pattern in &noise_patterns {
            if let Ok(re) = Regex::new(pattern) {
                result = re.replace_all(&result, "").to_string();
            }
        }

        // 移除多余空行，保留最多一个
        let lines: Vec<&str> = result.lines()
            .filter(|line| !line.trim().is_empty())
            .collect();

        if lines.is_empty() {
            return String::new();
        }

        // 查找最后一个问题/提示行
        // 问题行特征：包含 ? 或 ？，以 : 或 ： 结尾，或包含确认提示模式
        let mut last_question_idx = None;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            // 检查是否是问题行（包含 ? 或 ？）
            // 注意：使用 contains 而不是 ends_with，因为问题后可能有括号说明
            // 例如：需要哪些核心功能？（可多选）
            if trimmed.contains('?') || trimmed.contains('？') {
                last_question_idx = Some(i);
            }
            // 检查是否是提示行（以 : 或 ： 结尾）
            else if trimmed.ends_with(':') || trimmed.ends_with('：') {
                last_question_idx = Some(i);
            }
            // 检查是否是确认提示行（[Y]es / [N]o 等）
            else if trimmed.contains("[Y]es") || trimmed.contains("[Y/n]")
                || trimmed.contains("[y/N]") || trimmed.contains("[是/否]") {
                last_question_idx = Some(i);
            }
        }

        // 查找最后一组连续的选项块
        // 关键改进：
        // 1. 只提取最后一组连续的选项
        // 2. 检测选项编号重置（如 4 后面出现 1）来分割不同的选项组
        let mut option_groups: Vec<(usize, usize)> = Vec::new();
        let mut current_group_start: Option<usize> = None;
        let mut current_group_end: Option<usize> = None;
        let mut last_option_num: Option<u32> = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            // 检查是否是选项行，并提取选项编号
            let option_num = if let Some(first_char) = trimmed.chars().next() {
                if first_char.is_ascii_digit() && trimmed.contains('.') {
                    // 提取选项编号
                    trimmed.chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<u32>()
                        .ok()
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(num) = option_num {
                // 检测选项编号重置（新组开始）
                // 如果当前编号小于等于上一个编号，说明是新的一组
                let is_new_group = last_option_num.map(|last| num <= last).unwrap_or(false);

                if is_new_group && current_group_start.is_some() {
                    // 保存当前组，开始新组
                    if let (Some(start), Some(end)) = (current_group_start, current_group_end) {
                        option_groups.push((start, end));
                    }
                    current_group_start = Some(i);
                    current_group_end = Some(i);
                } else if current_group_start.is_none() {
                    // 第一个选项
                    current_group_start = Some(i);
                    current_group_end = Some(i);
                } else {
                    // 继续当前组
                    current_group_end = Some(i);
                }
                last_option_num = Some(num);
            } else if current_group_start.is_some() {
                // 非选项行，结束当前组
                if let (Some(start), Some(end)) = (current_group_start, current_group_end) {
                    option_groups.push((start, end));
                }
                current_group_start = None;
                current_group_end = None;
                last_option_num = None;
            }
        }
        // 处理最后一组（如果存在）
        if let (Some(start), Some(end)) = (current_group_start, current_group_end) {
            option_groups.push((start, end));
        }

        // 使用最后一组选项
        let (first_option_idx, last_option_idx) = option_groups.last()
            .map(|(s, e)| (Some(*s), Some(*e)))
            .unwrap_or((None, None));

        // 查找与最后一组选项相关的问题行
        // 问题可能在选项之前或之后
        let relevant_question_idx = if let (Some(first_opt), Some(last_opt)) = (first_option_idx, last_option_idx) {
            // 先在选项之前查找
            let mut before_idx = None;
            for i in (0..first_opt).rev() {
                let trimmed = lines[i].trim();
                if trimmed.contains('?') || trimmed.contains('？')
                    || trimmed.ends_with(':') || trimmed.ends_with('：')
                    || trimmed.contains("[Y]es") || trimmed.contains("[Y/n]")
                    || trimmed.contains("[y/N]") || trimmed.contains("[是/否]") {
                    before_idx = Some(i);
                    break;
                }
            }

            // 再在选项之后查找
            let mut after_idx = None;
            for i in (last_opt + 1)..lines.len() {
                let trimmed = lines[i].trim();
                if trimmed.contains('?') || trimmed.contains('？')
                    || trimmed.ends_with(':') || trimmed.ends_with('：')
                    || trimmed.contains("[Y]es") || trimmed.contains("[Y/n]")
                    || trimmed.contains("[y/N]") || trimmed.contains("[是/否]") {
                    after_idx = Some(i);
                    break;
                }
            }

            // 优先使用选项之后的问题（更接近当前状态）
            after_idx.or(before_idx)
        } else {
            // 没有选项，使用最后一个问题行
            last_question_idx
        };

        // 根据问题和选项的位置关系决定返回内容
        match (relevant_question_idx, first_option_idx, last_option_idx) {
            // 有问题和选项
            (Some(q_idx), Some(first_opt), Some(last_opt)) => {
                if q_idx < first_opt {
                    // 格式1：问题在前，选项在后
                    // 返回从问题到最后一个选项
                    lines[q_idx..=last_opt].join("\n")
                } else if q_idx > last_opt {
                    // 格式2：选项在前，问题在后
                    // 返回从第一个选项到问题
                    lines[first_opt..=q_idx].join("\n")
                } else {
                    // 问题在选项中间（异常情况），返回全部
                    lines[first_opt..=q_idx.max(last_opt)].join("\n")
                }
            }
            // 只有问题，没有选项 - 需要保留问题前的上下文
            (Some(q_idx), None, None) => {
                // 向前查找上下文的起始位置
                let context_start = Self::find_context_start(&lines, q_idx);
                lines[context_start..].join("\n")
            }
            // 只有选项，没有问题
            (None, Some(first_opt), Some(last_opt)) => {
                lines[first_opt..=last_opt].join("\n")
            }
            // 都没有，返回全部
            _ => lines.join("\n")
        }
    }

    /// 查找问题前上下文的起始位置
    ///
    /// 对于开放式问题（如"这部分结构看起来合适吗？"），需要保留问题前的相关上下文。
    /// 上下文包括：代码块、目录结构、设计说明等。
    ///
    /// 策略：
    /// 1. 从问题行向前查找，直到遇到分隔符（---）或用户输入（❯）
    /// 2. 最多保留 15 行上下文（避免通知过长）
    /// 3. 如果找到代码块/目录结构，保留完整块
    fn find_context_start(lines: &[&str], question_idx: usize) -> usize {
        const MAX_CONTEXT_LINES: usize = 15;

        // 最早可能的起始位置
        let earliest_start = question_idx.saturating_sub(MAX_CONTEXT_LINES);

        // 从问题行向前查找
        let mut context_start = question_idx;

        for i in (earliest_start..question_idx).rev() {
            let trimmed = lines[i].trim();

            // 遇到分隔符，停止（不包含分隔符）
            if trimmed == "---" || trimmed.starts_with("───") {
                break;
            }

            // 遇到用户输入行（❯ 后跟内容），停止（不包含用户输入）
            if trimmed.starts_with('❯') && trimmed.len() > 2 {
                break;
            }

            // 遇到 agent 响应开始（⏺），停止（不包含）
            if trimmed.starts_with('⏺') {
                break;
            }

            // 更新起始位置
            context_start = i;
        }

        context_start
    }

    /// 检测是否为编号选择题
    fn is_numbered_choice(context: &str) -> bool {
        Regex::new(r"(?m)^\s*[1-9]\.\s+")
            .map(|re| re.is_match(context))
            .unwrap_or(false)
    }

    /// 提取编号选项
    fn extract_choices(context: &str) -> Vec<String> {
        Regex::new(r"(?m)^\s*([1-9])\.\s+(.+)$")
            .map(|re| {
                re.captures_iter(context)
                    .map(|cap| format!("{}. {}", &cap[1], cap[2].trim()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 提取选择题的问题标题
    /// 支持两种格式：
    /// - 格式1：问题在前，选项在后
    /// - 格式2：选项在前，问题在后
    fn extract_choice_question(context: &str) -> Option<String> {
        let lines: Vec<&str> = context.lines().collect();

        // 找到第一个和最后一个选项的位置
        let mut first_choice_idx = None;
        let mut last_choice_idx = None;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                && trimmed.contains('.') {
                if first_choice_idx.is_none() {
                    first_choice_idx = Some(i);
                }
                last_choice_idx = Some(i);
            }
        }

        // 先尝试向后查找问题行（格式2：选项在前，问题在后）
        if let Some(idx) = last_choice_idx {
            for i in (idx + 1)..lines.len() {
                let line = lines[i].trim();
                // 使用 contains 而不是 ends_with，因为问题后可能有括号说明
                if !line.is_empty() && (line.contains('?') || line.contains('？')
                    || line.ends_with(':') || line.ends_with('：')) {
                    return Some(line.to_string());
                }
            }
        }

        // 再尝试向前查找问题行（格式1：问题在前，选项在后）
        if let Some(idx) = first_choice_idx {
            for i in (0..idx).rev() {
                let line = lines[i].trim();
                if !line.is_empty() && !line.chars().all(|c| c == '─' || c == '━' || c == '=' || c == '-') {
                    // 检查是否是问题/提示行
                    // 使用 contains 而不是 ends_with，因为问题后可能有括号说明
                    if line.contains('?') || line.contains('？')
                        || line.ends_with(':') || line.ends_with('：') {
                        return Some(line.to_string());
                    }
                }
            }
        }

        None
    }

    /// 检测是否为确认提示 [Y/n] 类型
    fn is_confirmation_prompt(context: &str) -> bool {
        let patterns = [
            r"\[Y\]es\s*/\s*\[N\]o",
            r"\[Y/n\]",
            r"\[y/N\]",
            r"\[yes/no\]",
            r"\[是/否\]",
        ];
        patterns.iter().any(|p| {
            Regex::new(p)
                .map(|re| re.is_match(context))
                .unwrap_or(false)
        })
    }

    /// 提取确认问题（去掉选项行）
    fn extract_confirmation_question(context: &str) -> String {
        let mut result = context.to_string();

        // 移除 [Y]es / [N]o 等选项行
        if let Ok(re) = Regex::new(r"(?m)^\s*\[Y\]es\s*/\s*\[N\]o.*$") {
            result = re.replace_all(&result, "").to_string();
        }
        if let Ok(re) = Regex::new(r"\s*\[Y/n\]|\[y/N\]|\[yes/no\]|\[是/否\]") {
            result = re.replace_all(&result, "").to_string();
        }

        // 提取最后一个问题行
        let lines: Vec<&str> = result.lines()
            .filter(|line| !line.trim().is_empty())
            .collect();

        if let Some(last) = lines.last() {
            last.trim().to_string()
        } else {
            context.trim().to_string()
        }
    }

    /// 检测是否为冒号结尾的自由输入提示
    fn is_colon_prompt(context: &str) -> bool {
        let trimmed = context.trim();
        trimmed.ends_with(':') || trimmed.ends_with('：')
    }

    /// 提取冒号提示的问题
    fn extract_colon_question(context: &str) -> String {
        let lines: Vec<&str> = context.lines()
            .filter(|line| !line.trim().is_empty())
            .collect();

        if let Some(last) = lines.last() {
            last.trim().to_string()
        } else {
            context.trim().to_string()
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

        // 清洗终端快照
        let cleaned_snapshot = terminal_snapshot
            .map(Self::clean_terminal_context)
            .filter(|s| !s.is_empty());

        match event_type {
            "permission_request" => {
                self.format_permission_request(&project_name, &json, &cleaned_snapshot)
            }
            "notification" => {
                self.format_notification(&project_name, &json, &cleaned_snapshot)
            }
            "session_start" => {
                format!("🚀 {} 已启动", project_name)
            }
            "session_end" => {
                format!("🔚 {} 会话结束", project_name)
            }
            "stop" => {
                format!("⏹️ {} 已停止", project_name)
            }
            "WaitingForInput" => {
                self.format_waiting_for_input(&project_name, pattern_or_path, raw_context, &cleaned_snapshot)
            }
            "Error" => {
                self.format_error(&project_name, raw_context, &cleaned_snapshot)
            }
            "AgentExited" => {
                format!("✅ {} 已完成", project_name)
            }
            "ToolUse" => {
                // pattern_or_path = tool_name, raw_context = tool_target
                if raw_context.is_empty() {
                    format!("🔧 {} 执行 {}", project_name, pattern_or_path)
                } else {
                    format!("🔧 {} 执行 {} → {}", project_name, pattern_or_path, raw_context)
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
            "{} {} 请求权限\n\n{}\n执行: {}{}\n\n回复 y 允许 / n 拒绝",
            risk_emoji, project_name, summary.recommendation, tool_name, param_line
        )
    }

    /// AI 提取超时时间（秒）
    const AI_EXTRACT_TIMEOUT_SECS: u64 = 5;

    /// 使用 AI 提取终端快照中的问题内容
    ///
    /// 当硬编码模式匹配失败时，调用 openclaw agent 进行智能提取。
    /// 返回结构化的提取结果：(问题类型, 核心问题, 回复提示)
    ///
    /// 超时机制：如果 AI 提取超过 5 秒，自动终止并返回 None，
    /// 调用方会回退到显示原始快照。
    fn extract_question_with_ai(&self, terminal_snapshot: &str) -> Option<(String, String, String)> {
        // 如果禁用 AI 提取，直接返回 None
        if self.no_ai {
            return None;
        }

        if self.dry_run {
            eprintln!("[DRY-RUN] Would call AI to extract question from snapshot");
            return None;
        }

        // 截取最后 30 行，避免 prompt 过长
        let lines: Vec<&str> = terminal_snapshot.lines().collect();
        let truncated = if lines.len() > 30 {
            lines[lines.len() - 30..].join("\n")
        } else {
            terminal_snapshot.to_string()
        };

        let prompt = format!(
            r#"分析以下 AI Agent 终端输出，提取正在询问用户的问题。

终端输出:
{}

请用 JSON 格式回复，包含以下字段：
- question_type: "open"（开放问题）、"choice"（选择题）、"confirm"（确认）、"none"（无问题）
- question: 核心问题内容（简洁，不超过 100 字）
- reply_hint: 回复提示（如"回复 y/n"、"回复数字选择"、"回复内容"）

只返回 JSON，不要其他内容。如果没有问题，question_type 设为 "none"。"#,
            truncated
        );

        // 使用 spawn + try_wait 实现超时机制
        let mut child = Command::new(&self.openclaw_cmd)
            .args([
                "agent",
                "--agent", "main",
                "--session-id", "cam-extract",
                "--message", &prompt,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .ok()?;

        // 轮询等待，每 100ms 检查一次，最多等待 AI_EXTRACT_TIMEOUT_SECS 秒
        let timeout = std::time::Duration::from_secs(Self::AI_EXTRACT_TIMEOUT_SECS);
        let poll_interval = std::time::Duration::from_millis(100);
        let start = std::time::Instant::now();

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // 进程已结束
                    if !status.success() {
                        return None;
                    }
                    break;
                }
                Ok(None) => {
                    // 进程仍在运行，检查是否超时
                    if start.elapsed() >= timeout {
                        eprintln!("[AI-EXTRACT] Timeout after {}s, killing process", Self::AI_EXTRACT_TIMEOUT_SECS);
                        let _ = child.kill();
                        let _ = child.wait(); // 回收子进程
                        return None;
                    }
                    std::thread::sleep(poll_interval);
                }
                Err(_) => {
                    return None;
                }
            }
        }

        // 读取输出
        let output = child.wait_with_output().ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        // 尝试从输出中提取 JSON
        let json_str = Self::extract_json_from_output(&stdout)?;
        let parsed: serde_json::Value = serde_json::from_str(&json_str).ok()?;

        let question_type = parsed.get("question_type")?.as_str()?;
        if question_type == "none" {
            return None;
        }

        let question = parsed.get("question")?.as_str()?.to_string();
        let reply_hint = parsed.get("reply_hint")?.as_str()?.to_string();

        Some((question_type.to_string(), question, reply_hint))
    }

    /// 从 AI 输出中提取 JSON 字符串
    fn extract_json_from_output(output: &str) -> Option<String> {
        // 尝试找到 JSON 对象的开始和结束
        let start = output.find('{')?;
        let end = output.rfind('}')?;
        if end > start {
            Some(output[start..=end].to_string())
        } else {
            None
        }
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
                // 空闲等待 - 显示终端快照中的问题
                if let Some(snap) = snapshot {
                    // 记录模式匹配开始时间
                    let pattern_start = std::time::Instant::now();

                    if Self::is_numbered_choice(snap) {
                        let question = Self::extract_choice_question(snap);
                        let choices = Self::extract_choices(snap);
                        let choices_text = choices.join("\n");
                        Self::log_timing("pattern_match", "numbered_choice", pattern_start.elapsed());
                        if let Some(q) = question {
                            format!(
                                "⏸️ {} 等待选择\n\n{}\n\n{}\n\n回复数字选择",
                                project_name, q, choices_text
                            )
                        } else {
                            format!(
                                "⏸️ {} 等待选择\n\n{}\n\n回复数字选择",
                                project_name, choices_text
                            )
                        }
                    } else if Self::is_confirmation_prompt(snap) {
                        let question = Self::extract_confirmation_question(snap);
                        Self::log_timing("pattern_match", "confirmation_prompt", pattern_start.elapsed());
                        format!(
                            "⏸️ {} 请求确认\n\n{}\n\n回复 y/n",
                            project_name, question
                        )
                    } else if Self::is_colon_prompt(snap) {
                        let question = Self::extract_colon_question(snap);
                        Self::log_timing("pattern_match", "colon_prompt", pattern_start.elapsed());
                        format!(
                            "⏸️ {} 等待输入\n\n{}\n\n回复内容",
                            project_name, question
                        )
                    } else if !snap.trim().is_empty() {
                        // 有快照内容但不匹配特定模式
                        Self::log_timing("pattern_match", "no_match", pattern_start.elapsed());

                        // 优先级：1. Embedding 提取 → 2. AI 提取 → 3. 显示原始快照
                        let embedding_start = std::time::Instant::now();
                        if let Some(question) = extract_question_with_embedding(snap) {
                            Self::log_timing("embedding_extract", "success", embedding_start.elapsed());
                            format!(
                                "⏸️ {} 等待输入\n\n{}\n\n回复内容",
                                project_name, question
                            )
                        } else {
                            Self::log_timing("embedding_extract", "failed", embedding_start.elapsed());

                            let ai_start = std::time::Instant::now();
                            if let Some((question_type, question, reply_hint)) = self.extract_question_with_ai(snap) {
                                Self::log_timing("ai_extract", "success", ai_start.elapsed());
                                let emoji = match question_type.as_str() {
                                    "confirm" => "⏸️",
                                    "choice" => "⏸️",
                                    _ => "⏸️",
                                };
                                format!(
                                    "{} {} 等待输入\n\n{}\n\n{}",
                                    emoji, project_name, question, reply_hint
                                )
                            } else {
                                Self::log_timing("ai_extract", "failed_or_timeout", ai_start.elapsed());
                                // AI 提取也失败，回退到显示原始快照
                                format!(
                                    "⏸️ {} 等待输入\n\n{}\n\n回复内容",
                                    project_name, snap.trim()
                                )
                            }
                        }
                    } else {
                        Self::log_timing("pattern_match", "empty_snapshot", pattern_start.elapsed());
                        format!("⏸️ {} 等待输入", project_name)
                    }
                } else if !message.is_empty() {
                    format!("⏸️ {} 等待输入\n\n{}", project_name, message)
                } else {
                    format!("⏸️ {} 等待输入", project_name)
                }
            }
            "permission_prompt" => {
                // 权限确认 - 优先使用终端快照，其次使用 message
                let content = if let Some(snap) = snapshot {
                    if Self::is_confirmation_prompt(snap) {
                        Self::extract_confirmation_question(snap)
                    } else if !snap.trim().is_empty() {
                        snap.trim().to_string()
                    } else if !message.is_empty() {
                        message.to_string()
                    } else {
                        String::new()
                    }
                } else if !message.is_empty() {
                    message.to_string()
                } else {
                    String::new()
                };

                if content.is_empty() {
                    format!(
                        "🔐 {} 需要确认\n\n回复 y 允许 / n 拒绝",
                        project_name
                    )
                } else {
                    format!(
                        "🔐 {} 需要确认\n\n{}\n\n回复 y 允许 / n 拒绝",
                        project_name, content
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
        pattern_type: &str,
        raw_context: &str,
        snapshot: &Option<String>,
    ) -> String {
        // 优先使用终端快照
        let context_to_analyze = snapshot.as_deref().unwrap_or(raw_context);
        let cleaned = Self::clean_terminal_context(context_to_analyze);

        // 根据模式类型格式化
        match pattern_type {
            "Confirmation" | "PermissionRequest" => {
                if Self::is_confirmation_prompt(&cleaned) {
                    let question = Self::extract_confirmation_question(&cleaned);
                    format!(
                        "⏸️ {} 请求确认\n\n{}\n\n回复 y/n",
                        project_name, question
                    )
                } else {
                    format!(
                        "⏸️ {} 请求确认\n\n回复 y/n",
                        project_name
                    )
                }
            }
            "ClaudePrompt" => {
                // Claude 主提示符 - 检查是否有选项或问题
                if Self::is_numbered_choice(&cleaned) {
                    let question = Self::extract_choice_question(&cleaned);
                    let choices = Self::extract_choices(&cleaned);
                    let choices_text = choices.join("\n");
                    if let Some(q) = question {
                        format!(
                            "⏸️ {} 等待选择\n\n{}\n\n{}\n\n回复数字选择",
                            project_name, q, choices_text
                        )
                    } else {
                        format!(
                            "⏸️ {} 等待选择\n\n{}\n\n回复数字选择",
                            project_name, choices_text
                        )
                    }
                } else if Self::is_confirmation_prompt(&cleaned) {
                    let question = Self::extract_confirmation_question(&cleaned);
                    format!(
                        "⏸️ {} 请求确认\n\n{}\n\n回复 y/n",
                        project_name, question
                    )
                } else if !cleaned.trim().is_empty() {
                    // 有内容但不匹配特定模式
                    // 优先级：1. Embedding 提取 → 2. AI 提取 → 3. 显示清洗后的内容
                    if let Some(question) = extract_question_with_embedding(&cleaned) {
                        format!(
                            "⏸️ {} 等待输入\n\n{}\n\n回复内容",
                            project_name, question
                        )
                    } else if let Some((question_type, question, reply_hint)) = self.extract_question_with_ai(&cleaned) {
                        let emoji = match question_type.as_str() {
                            "confirm" => "⏸️",
                            "choice" => "⏸️",
                            _ => "⏸️",
                        };
                        format!(
                            "{} {} 等待输入\n\n{}\n\n{}",
                            emoji, project_name, question, reply_hint
                        )
                    } else {
                        // AI 提取也失败，回退到显示清洗后的内容
                        format!(
                            "⏸️ {} 等待输入\n\n{}\n\n回复内容",
                            project_name, cleaned.trim()
                        )
                    }
                } else {
                    format!("⏸️ {} 等待输入", project_name)
                }
            }
            "ColonPrompt" => {
                let question = Self::extract_colon_question(&cleaned);
                format!(
                    "⏸️ {} 等待输入\n\n{}\n\n回复内容",
                    project_name, question
                )
            }
            "PressEnter" | "Continue" => {
                format!(
                    "⏸️ {} 等待继续\n\n回复 Enter 继续",
                    project_name
                )
            }
            _ => {
                // 通用处理（包括 pattern_type 为空的情况）
                if Self::is_numbered_choice(&cleaned) {
                    let question = Self::extract_choice_question(&cleaned);
                    let choices = Self::extract_choices(&cleaned);
                    let choices_text = choices.join("\n");
                    if let Some(q) = question {
                        format!(
                            "⏸️ {} 等待选择\n\n{}\n\n{}\n\n回复数字选择",
                            project_name, q, choices_text
                        )
                    } else {
                        format!(
                            "⏸️ {} 等待选择\n\n{}\n\n回复数字选择",
                            project_name, choices_text
                        )
                    }
                } else if Self::is_confirmation_prompt(&cleaned) {
                    let question = Self::extract_confirmation_question(&cleaned);
                    format!(
                        "⏸️ {} 请求确认\n\n{}\n\n回复 y/n",
                        project_name, question
                    )
                } else if Self::is_colon_prompt(&cleaned) {
                    let question = Self::extract_colon_question(&cleaned);
                    format!(
                        "⏸️ {} 等待输入\n\n{}\n\n回复内容",
                        project_name, question
                    )
                } else if !cleaned.trim().is_empty() {
                    // 有内容但不匹配特定模式
                    // 优先级：1. Embedding 提取 → 2. AI 提取 → 3. 显示清洗后的内容
                    if let Some(question) = extract_question_with_embedding(&cleaned) {
                        format!(
                            "⏸️ {} 等待输入\n\n{}\n\n回复内容",
                            project_name, question
                        )
                    } else if let Some((question_type, question, reply_hint)) = self.extract_question_with_ai(&cleaned) {
                        let emoji = match question_type.as_str() {
                            "confirm" => "⏸️",
                            "choice" => "⏸️",
                            _ => "⏸️",
                        };
                        format!(
                            "{} {} 等待输入\n\n{}\n\n{}",
                            emoji, project_name, question, reply_hint
                        )
                    } else {
                        // AI 提取也失败，回退到显示清洗后的内容
                        format!(
                            "⏸️ {} 等待输入\n\n{}\n\n回复内容",
                            project_name, cleaned.trim()
                        )
                    }
                } else {
                    format!("⏸️ {} 等待输入", project_name)
                }
            }
        }
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
            "❌ {} 发生错误\n\n{}",
            project_name, summary
        )
    }

    /// 判断事件是否需要用户关注（用于提示 OpenClaw agent）
    ///
    /// 20 个 AI 并行时的关注优先级:
    /// - HIGH: 必须立即响应（权限请求、错误）→ 阻塞任务进度
    /// - MEDIUM: 需要知道（完成、空闲）→ 可以分配新任务
    /// - LOW: 可选（启动）→ 通常不需要通知
    pub fn get_urgency(event_type: &str, context: &str) -> &'static str {
        // `cam notify` 会把终端快照追加到 JSON context 后面，导致直接解析失败。
        // 这里先剥离快照部分，保证 urgency 判断稳定。
        let raw_context = if let Some(idx) = context.find("\n\n--- 终端快照 ---\n") {
            &context[..idx]
        } else {
            context
        };

        match event_type {
            // 权限请求必须转发 - 阻塞任务进度
            "permission_request" => "HIGH",
            // notification 类型需要检查具体类型
            "notification" => {
                let json: Option<serde_json::Value> = serde_json::from_str(raw_context).ok();
                let notification_type = json.as_ref()
                    .and_then(|j| j.get("notification_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match notification_type {
                    "permission_prompt" => "HIGH",  // 权限确认
                    "idle_prompt" => "MEDIUM",      // 空闲等待
                    _ => "LOW"
                }
            }
            // 错误必须转发 - 需要干预
            "Error" => "HIGH",
            // 等待输入必须转发
            "WaitingForInput" => "HIGH",
            // Agent 异常退出 - 需要知道（可能是崩溃或被杀死）
            "AgentExited" => "MEDIUM",
            // stop/session_end - 用户自己触发的停止，无需通知（用户已知道）
            "stop" | "session_end" => "LOW",
            // 启动通知 - 可选
            "session_start" => "LOW",
            // 工具调用 - 太频繁，静默处理
            "ToolUse" => "LOW",
            // 其他
            _ => "LOW",
        }
    }

    /// 创建结构化 payload 用于 gateway wake
    fn create_payload(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> serde_json::Value {
        let urgency = Self::get_urgency(event_type, context);

        // 分离终端快照和原始 context
        let (raw_context, terminal_snapshot) = if let Some(idx) = context.find("\n\n--- 终端快照 ---\n") {
            let (before, after) = context.split_at(idx);
            let snapshot = after.trim_start_matches("\n\n--- 终端快照 ---\n");
            (before, Some(snapshot.to_string()))
        } else {
            (context, None)
        };

        // 尝试解析 JSON context
        let json: Option<serde_json::Value> = serde_json::from_str(raw_context).ok();

        // 提取项目路径
        let project = json.as_ref()
            .and_then(|j| j.get("cwd"))
            .and_then(|v| v.as_str())
            .unwrap_or(pattern_or_path);

        // 构建 event 对象
        let event = self.build_event_object(event_type, pattern_or_path, &json, raw_context);

        // 生成简短摘要
        let summary = self.generate_summary(event_type, &json, pattern_or_path);

        // 对于权限请求，添加风险评估
        let risk_level = if event_type == "permission_request" {
            let tool_name = json.as_ref()
                .and_then(|j| j.get("tool_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let tool_input = json.as_ref()
                .and_then(|j| j.get("tool_input"))
                .cloned()
                .unwrap_or(serde_json::json!({}));

            let summarizer = NotificationSummarizer::new();
            let perm_summary = summarizer.summarize_permission(tool_name, &tool_input);
            Some(format!("{:?}", perm_summary.risk_level).to_uppercase())
        } else {
            None
        };

        let mut payload = serde_json::json!({
            "type": "cam_notification",
            "version": "1.0",
            "urgency": urgency,
            "event_type": event_type,
            "agent_id": agent_id,
            "project": project,
            "timestamp": Utc::now().to_rfc3339(),
            "event": event,
            "summary": summary
        });

        // 添加风险等级（如果有）
        if let Some(risk) = risk_level {
            payload["risk_level"] = serde_json::Value::String(risk);
        }

        // 添加终端快照（如果有）
        if let Some(snapshot) = terminal_snapshot {
            // 截取最后 15 行
            let lines: Vec<&str> = snapshot.lines().collect();
            let truncated = if lines.len() > 15 {
                lines[lines.len() - 15..].join("\n")
            } else {
                snapshot
            };
            payload["terminal_snapshot"] = serde_json::Value::String(truncated);
        }

        payload
    }

    /// 构建 event 对象
    fn build_event_object(
        &self,
        event_type: &str,
        pattern_or_path: &str,
        json: &Option<serde_json::Value>,
        raw_context: &str,
    ) -> serde_json::Value {
        match event_type {
            "permission_request" => {
                let tool_name = json.as_ref()
                    .and_then(|j| j.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let tool_input = json.as_ref()
                    .and_then(|j| j.get("tool_input"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                serde_json::json!({
                    "tool_name": tool_name,
                    "tool_input": tool_input
                })
            }
            "notification" => {
                let message = json.as_ref()
                    .and_then(|j| j.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let notification_type = json.as_ref()
                    .and_then(|j| j.get("notification_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                serde_json::json!({
                    "notification_type": notification_type,
                    "message": message
                })
            }
            "WaitingForInput" => {
                serde_json::json!({
                    "pattern_type": pattern_or_path,
                    "prompt": raw_context
                })
            }
            "Error" => {
                serde_json::json!({
                    "message": raw_context
                })
            }
            "AgentExited" => {
                serde_json::json!({
                    "project_path": pattern_or_path
                })
            }
            "ToolUse" => {
                serde_json::json!({
                    "tool_name": pattern_or_path,
                    "tool_target": raw_context
                })
            }
            _ => {
                serde_json::json!({
                    "raw_context": raw_context
                })
            }
        }
    }

    /// 生成简短摘要
    fn generate_summary(
        &self,
        event_type: &str,
        json: &Option<serde_json::Value>,
        pattern_or_path: &str,
    ) -> String {
        match event_type {
            "permission_request" => {
                let tool_name = json.as_ref()
                    .and_then(|j| j.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format!("请求执行 {} 工具", tool_name)
            }
            "notification" => {
                let notification_type = json.as_ref()
                    .and_then(|j| j.get("notification_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match notification_type {
                    "idle_prompt" => "等待用户输入".to_string(),
                    "permission_prompt" => "需要权限确认".to_string(),
                    _ => "通知".to_string()
                }
            }
            "WaitingForInput" => format!("等待输入: {}", pattern_or_path),
            "Error" => "发生错误".to_string(),
            "AgentExited" => "Agent 已退出".to_string(),
            "ToolUse" => format!("执行工具: {}", pattern_or_path),
            "stop" | "session_end" => "会话已结束".to_string(),
            "session_start" => "会话已启动".to_string(),
            _ => event_type.to_string()
        }
    }

    /// 发送事件到 channel
    /// HIGH/MEDIUM urgency → 通过 gateway wake 发送结构化 payload
    /// LOW urgency → 静默处理（避免 agent session 上下文累积导致去重问题）
    /// 返回 SendResult 以区分发送成功和静默跳过
    pub fn send_event(
        &self,
        agent_id: &str,
        event_type: &str,
        pattern_or_path: &str,
        context: &str,
    ) -> Result<SendResult> {
        let total_start = std::time::Instant::now();

        // 外部会话（ext-xxx）不发送通知
        // 原因：外部会话无法远程回复，通知只会造成打扰
        if agent_id.starts_with("ext-") {
            if self.dry_run {
                eprintln!("[DRY-RUN] External session (cannot reply remotely), skipping: {} {}", agent_id, event_type);
            }
            debug!(agent_id = %agent_id, event_type = %event_type, "Skipping external session notification");
            return Ok(SendResult::Skipped("external session".to_string()));
        }

        let urgency = Self::get_urgency(event_type, context);

        debug!(
            agent_id = %agent_id,
            event_type = %event_type,
            urgency = %urgency,
            "Processing notification event"
        );

        match urgency {
            "HIGH" | "MEDIUM" => {
                // 直接发送到 Telegram（不经过 system event，因为 Agent 可能不处理 cam_notification）
                if self.channel_config.is_some() {
                    let format_start = std::time::Instant::now();
                    let message = self.format_event(agent_id, event_type, pattern_or_path, context);
                    Self::log_timing("format_event", event_type, format_start.elapsed());

                    // 只有需要用户回复的事件才添加 agent_id 标记
                    let needs_reply = matches!(event_type,
                        "permission_request" | "WaitingForInput" | "Error" | "notification"
                    );

                    let send_start = std::time::Instant::now();
                    if needs_reply {
                        self.send_direct(&message, agent_id)?;
                    } else {
                        // stop/session_end 等不需要回复的事件，不添加标记
                        self.send_direct_text(&message)?;
                    }
                    Self::log_timing("send_direct", "telegram", send_start.elapsed());
                    Self::log_timing("send_event_total", event_type, total_start.elapsed());

                    info!(
                        agent_id = %agent_id,
                        event_type = %event_type,
                        urgency = %urgency,
                        "Notification sent via direct channel"
                    );
                    return Ok(SendResult::Sent);
                }

                // 如果没有 channel 配置，尝试 system event
                let payload = self.create_payload(agent_id, event_type, pattern_or_path, context);
                self.send_via_gateway_wake_payload(&payload)?;
                Self::log_timing("send_event_total", event_type, total_start.elapsed());
                info!(
                    agent_id = %agent_id,
                    event_type = %event_type,
                    urgency = %urgency,
                    "Notification sent via system event"
                );
                Ok(SendResult::Sent)
            }
            _ => {
                // LOW urgency: 静默处理，不发送通知
                // 参考 coding-agent skill 设计：启动通知由调用方自己说，不需要系统推送
                if self.dry_run {
                    eprintln!("[DRY-RUN] LOW urgency, skipping: {} {}", event_type, agent_id);
                }
                debug!(
                    agent_id = %agent_id,
                    event_type = %event_type,
                    "Notification skipped (LOW urgency)"
                );
                Ok(SendResult::Skipped(format!("LOW urgency ({})", event_type)))
            }
        }
    }

    /// 直接发送消息到 channel
    /// agent_id 用于在消息末尾添加路由标记 [agent_id]，方便用户回复时路由到正确的 agent
    fn send_direct(&self, message: &str, agent_id: &str) -> Result<()> {
        let config = self.channel_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No channel configured"))?;

        if self.dry_run {
            eprintln!("[DRY-RUN] Would send to channel={} target={}", config.channel, config.target);
            eprintln!("[DRY-RUN] Message: {}", message);
            eprintln!("[DRY-RUN] Agent ID tag: {}", agent_id);
            return Ok(());
        }

        // 添加 agent_id 标记用于回复路由
        // 使用 Telegram markdown 的 monospace 格式，方便用户点击复制
        let tagged_message = format!("{} `{}`", message, agent_id);

        let result = Command::new(&self.openclaw_cmd)
            .args([
                "message", "send",
                "--channel", &config.channel,
                "--target", &config.target,
                "--message", &tagged_message,
            ])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    error!(channel = %config.channel, error = %stderr, "OpenClaw direct send failed");
                    return Err(anyhow::anyhow!("OpenClaw send failed: {}", stderr));
                }
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Failed to execute OpenClaw message send");
                Err(e.into())
            }
        }
    }

    /// 通过 system event 发送结构化 payload
    /// 参考 coding-agent skill 设计：一次性事件，触发 heartbeat
    fn send_via_gateway_wake_payload(&self, payload: &serde_json::Value) -> Result<()> {
        if self.dry_run {
            eprintln!("[DRY-RUN] Would send via system event");
            eprintln!("[DRY-RUN] Payload: {}", serde_json::to_string_pretty(payload).unwrap_or_default());
            return Ok(());
        }

        let payload_text = payload.to_string();

        let result = Command::new(&self.openclaw_cmd)
            .args([
                "system", "event",
                "--text", &payload_text,
                "--mode", "now",
            ])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    error!(error = %stderr, "System event send failed");
                    return Err(anyhow::anyhow!("System event failed: {}", stderr));
                }
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Failed to execute system event");
                Err(e.into())
            }
        }
    }

    /// 通过 system event 发送通知（旧接口，保留兼容性）
    /// 参考 coding-agent skill 设计：一次性事件，触发 heartbeat
    #[allow(dead_code)]
    fn send_via_gateway_wake(&self, message: &str) -> Result<()> {
        if self.dry_run {
            eprintln!("[DRY-RUN] Would send via system event");
            eprintln!("[DRY-RUN] Message: {}", message);
            return Ok(());
        }

        let result = Command::new(&self.openclaw_cmd)
            .args([
                "system", "event",
                "--text", message,
                "--mode", "now",
            ])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("System event 发送失败: {}", stderr);
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("无法执行 system event: {}", e);
                Err(e.into())
            }
        }
    }

    /// 发送消息给 Agent (已废弃，保留兼容性)
    #[allow(dead_code)]
    fn send_to_agent(&self, message: &str) -> Result<()> {
        if self.dry_run {
            eprintln!("[DRY-RUN] Would send to agent session={}", self.session_id);
            eprintln!("[DRY-RUN] Message: {}", message);
            return Ok(());
        }

        let result = Command::new(&self.openclaw_cmd)
            .args([
                "agent",
                "--session-id",
                &self.session_id,
                "--message",
                message,
            ])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("OpenClaw Agent 发送失败: {}", stderr);
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("无法执行 OpenClaw agent: {}", e);
                Err(e.into())
            }
        }
    }

    /// 为 Agent 包装消息（添加元数据）- 已废弃
    #[allow(dead_code)]
    fn wrap_for_agent(&self, message: &str, urgency: &str, event_type: &str, agent_id: &str) -> String {
        format!(
            "{}\n\n---\n[CAM_META] urgency={} event_type={} agent_id={}",
            message, urgency, event_type, agent_id
        )
    }

    /// 发送消息到 clawdbot (已废弃，保留兼容性)
    #[allow(dead_code)]
    pub fn send_message(&self, message: &str) -> Result<()> {
        self.send_to_agent(message)
    }

    /// 直接发送纯文本到检测到的 channel。
    ///
    /// 主要用于老的 `cam watch --openclaw` 路径，避免在多个模块里重复实现
    /// `openclaw message send` 的参数拼装和 channel detection。
    /// 注意：此方法不添加 agent_id 标记，因为调用方通常没有 agent_id 上下文。
    pub fn send_direct_text(&self, message: &str) -> Result<()> {
        let config = self.channel_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No channel configured"))?;

        if self.dry_run {
            eprintln!("[DRY-RUN] Would send to channel={} target={}", config.channel, config.target);
            eprintln!("[DRY-RUN] Message: {}", message);
            return Ok(());
        }

        let result = Command::new(&self.openclaw_cmd)
            .args([
                "message", "send",
                "--channel", &config.channel,
                "--target", &config.target,
                "--message", message,
            ])
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("OpenClaw 直接发送失败: {}", stderr);
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("无法执行 OpenClaw message send: {}", e);
                Err(e.into())
            }
        }
    }
}

impl Default for OpenclawNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_urgency_high() {
        assert_eq!(OpenclawNotifier::get_urgency("permission_request", ""), "HIGH");
        assert_eq!(OpenclawNotifier::get_urgency("Error", ""), "HIGH");
        assert_eq!(OpenclawNotifier::get_urgency("WaitingForInput", ""), "HIGH");

        // notification with permission_prompt
        let context = r#"{"notification_type": "permission_prompt"}"#;
        assert_eq!(OpenclawNotifier::get_urgency("notification", context), "HIGH");
    }

    #[test]
    fn test_get_urgency_medium() {
        // AgentExited 是 MEDIUM（可能是异常退出，用户需要知道）
        assert_eq!(OpenclawNotifier::get_urgency("AgentExited", ""), "MEDIUM");

        // notification with idle_prompt
        let context = r#"{"notification_type": "idle_prompt"}"#;
        assert_eq!(OpenclawNotifier::get_urgency("notification", context), "MEDIUM");
    }

    #[test]
    fn test_get_urgency_low() {
        // stop/session_end 是 LOW（用户自己触发的，无需通知）
        assert_eq!(OpenclawNotifier::get_urgency("stop", ""), "LOW");
        assert_eq!(OpenclawNotifier::get_urgency("session_end", ""), "LOW");
        assert_eq!(OpenclawNotifier::get_urgency("session_start", ""), "LOW");
        // ToolUse 是 LOW（太频繁，静默处理）
        assert_eq!(OpenclawNotifier::get_urgency("ToolUse", ""), "LOW");
        assert_eq!(OpenclawNotifier::get_urgency("unknown_event", ""), "LOW");

        // notification with unknown type
        let context = r#"{"notification_type": "other"}"#;
        assert_eq!(OpenclawNotifier::get_urgency("notification", context), "LOW");
    }

    #[test]
    fn test_get_urgency_notification_idle_prompt_with_terminal_snapshot() {
        let context = r#"{"notification_type": "idle_prompt", "message": "waiting"}

--- 终端快照 ---
line 1"#;
        assert_eq!(OpenclawNotifier::get_urgency("notification", context), "MEDIUM");
    }

    #[test]
    fn test_get_urgency_notification_permission_prompt_with_terminal_snapshot() {
        let context = r#"{"notification_type": "permission_prompt", "message": "confirm?"}

--- 终端快照 ---
line 1"#;
        assert_eq!(OpenclawNotifier::get_urgency("notification", context), "HIGH");
    }

    #[test]
    fn test_wrap_for_agent() {
        let notifier = OpenclawNotifier::new();
        let wrapped = notifier.wrap_for_agent("Test message", "HIGH", "Error", "cam-123");

        assert!(wrapped.contains("Test message"));
        assert!(wrapped.contains("[CAM_META]"));
        assert!(wrapped.contains("urgency=HIGH"));
        assert!(wrapped.contains("event_type=Error"));
        assert!(wrapped.contains("agent_id=cam-123"));
    }

    #[test]
    fn test_format_waiting_event() {
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
            "cam-1234567890",
            "WaitingForInput",
            "Confirmation",
            "Do you want to continue? [Y/n]",
        );

        // 新格式：使用项目名（从 agent_id 简化）
        assert!(message.contains("⏸️"));
        assert!(message.contains("请求确认") || message.contains("等待"));
        assert!(message.contains("y/n"));
    }

    #[test]
    fn test_format_error_event() {
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
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
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
            "cam-1234567890",
            "AgentExited",
            "/workspace/myapp",
            "",
        );

        // 新格式：使用项目名
        assert!(message.contains("✅"));
        assert!(message.contains("myapp") || message.contains("已完成"));
    }

    // ==================== 终端快照测试 ====================

    #[test]
    fn test_format_event_with_terminal_snapshot() {
        let notifier = OpenclawNotifier::new();

        // 模拟带终端快照的 context
        let context_with_snapshot = r#"{"cwd": "/workspace"}

--- 终端快照 ---
$ cargo build
   Compiling myapp v0.1.0
    Finished release target"#;

        let message = notifier.format_event(
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
        let notifier = OpenclawNotifier::new();

        // 创建超过 15 行的终端输出
        let mut long_output = String::from(r#"{"cwd": "/tmp"}

--- 终端快照 ---
"#);
        for i in 1..=20 {
            long_output.push_str(&format!("line {}\n", i));
        }

        let message = notifier.format_event(
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
        let notifier = OpenclawNotifier::new();

        let message = notifier.format_event(
            "cam-123",
            "stop",
            "",
            r#"{"cwd": "/workspace"}"#,
        );

        assert!(message.contains("⏹️"));
        assert!(message.contains("已停止") || message.contains("workspace"));
    }

    // ==================== 各事件类型格式化测试 ====================

    #[test]
    fn test_format_permission_request() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/test"}, "cwd": "/workspace"}"#;
        let message = notifier.format_event("cam-123", "permission_request", "", context);

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
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "idle_prompt", "message": "Task completed, waiting for next instruction"}"#;
        let message = notifier.format_event("cam-123", "notification", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
    }

    #[test]
    fn test_format_notification_permission_prompt() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "permission_prompt", "message": "Allow file write?"}"#;
        let message = notifier.format_event("cam-123", "notification", "", context);

        assert!(message.contains("🔐"));
        assert!(message.contains("确认") || message.contains("需要"));
        assert!(message.contains("Allow file write?"));
        // 新格式：简化回复指引
        assert!(message.contains("y") && message.contains("n"));
    }

    #[test]
    fn test_format_session_start() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"cwd": "/Users/admin/project"}"#;
        let message = notifier.format_event("cam-123", "session_start", "", context);

        assert!(message.contains("🚀"));
        assert!(message.contains("已启动"));
        // 新格式：使用项目名
        assert!(message.contains("project"));
    }

    #[test]
    fn test_format_stop_event() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"cwd": "/workspace/app"}"#;
        let message = notifier.format_event("cam-123", "stop", "", context);

        assert!(message.contains("⏹️"));
        assert!(message.contains("已停止") || message.contains("app"));
    }

    #[test]
    fn test_format_session_end() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"cwd": "/workspace"}"#;
        let message = notifier.format_event("cam-123", "session_end", "", context);

        assert!(message.contains("🔚"));
        assert!(message.contains("会话结束") || message.contains("workspace"));
    }

    #[test]
    fn test_format_agent_exited_with_snapshot() {
        let notifier = OpenclawNotifier::new();

        let context = r#"

--- 终端快照 ---
All tests passed!
Build successful."#;

        let message = notifier.format_event("cam-123", "AgentExited", "/myproject", context);

        // 新格式：简洁，使用项目名
        assert!(message.contains("✅"));
        assert!(message.contains("myproject") || message.contains("已完成"));
    }

    #[test]
    fn test_format_tool_use() {
        let notifier = OpenclawNotifier::new();

        // 带 target 的工具调用
        let message = notifier.format_event("cam-123", "ToolUse", "Edit", "src/main.rs");
        assert!(message.contains("🔧"));
        assert!(message.contains("Edit"));
        assert!(message.contains("src/main.rs"));

        // 不带 target 的工具调用
        let message2 = notifier.format_event("cam-456", "ToolUse", "Read", "");
        assert!(message2.contains("🔧"));
        assert!(message2.contains("Read"));
    }

    // ==================== Channel 检测测试 ====================

    #[test]
    fn test_extract_telegram_target_string() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "telegram": {
                "allowFrom": ["123456789"]
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_telegram_target(&channels);
        assert_eq!(target, Some("123456789".to_string()));
    }

    #[test]
    fn test_extract_telegram_target_number() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "telegram": {
                "allowFrom": [123456789]
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_telegram_target(&channels);
        assert_eq!(target, Some("123456789".to_string()));
    }

    #[test]
    fn test_extract_telegram_target_skips_wildcard() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "telegram": {
                "allowFrom": ["*", "123456789"]
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_telegram_target(&channels);
        assert_eq!(target, Some("123456789".to_string()));
    }

    #[test]
    fn test_extract_default_channel() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "discord": {
                "defaultChannel": "general"
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_default_channel(&channels, "discord");
        assert_eq!(target, Some("general".to_string()));
    }

    #[test]
    fn test_extract_allow_from() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "whatsapp": {
                "allowFrom": ["+1234567890"]
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_allow_from(&channels, "whatsapp");
        assert_eq!(target, Some("+1234567890".to_string()));
    }

    #[test]
    fn test_extract_allow_from_skips_wildcard() {
        let channels: serde_json::Value = serde_json::from_str(r#"{
            "whatsapp": {
                "allowFrom": ["*", "+1234567890"]
            }
        }"#).unwrap();

        let target = OpenclawNotifier::extract_allow_from(&channels, "whatsapp");
        assert_eq!(target, Some("+1234567890".to_string()));
    }

    // ==================== Wrap for Agent 测试 ====================

    #[test]
    fn test_wrap_for_agent_low_urgency() {
        let notifier = OpenclawNotifier::new();
        let wrapped = notifier.wrap_for_agent("Session started", "LOW", "session_start", "cam-456");

        assert!(wrapped.contains("Session started"));
        assert!(wrapped.contains("urgency=LOW"));
        assert!(wrapped.contains("event_type=session_start"));
        assert!(wrapped.contains("agent_id=cam-456"));
    }

    #[test]
    fn test_wrap_for_agent_contains_separator() {
        let notifier = OpenclawNotifier::new();
        let wrapped = notifier.wrap_for_agent("Test", "HIGH", "Error", "cam-789");

        // 应该包含分隔符
        assert!(wrapped.contains("---"));
        assert!(wrapped.contains("[CAM_META]"));
    }

    // ==================== Payload 创建测试 ====================

    #[test]
    fn test_create_payload_permission_request() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"tool_name": "Bash", "tool_input": {"command": "rm -rf /tmp/test"}, "cwd": "/workspace"}"#;
        let payload = notifier.create_payload("cam-123", "permission_request", "", context);

        assert_eq!(payload["type"], "cam_notification");
        assert_eq!(payload["version"], "1.0");
        assert_eq!(payload["urgency"], "HIGH");
        assert_eq!(payload["event_type"], "permission_request");
        assert_eq!(payload["agent_id"], "cam-123");
        assert_eq!(payload["project"], "/workspace");
        assert_eq!(payload["event"]["tool_name"], "Bash");
        assert!(payload["event"]["tool_input"]["command"].as_str().unwrap().contains("rm -rf"));
        assert!(payload["summary"].as_str().unwrap().contains("Bash"));
        assert!(payload["timestamp"].as_str().is_some());
    }

    #[test]
    fn test_create_payload_error() {
        let notifier = OpenclawNotifier::new();

        let payload = notifier.create_payload("cam-456", "Error", "", "API rate limit exceeded");

        assert_eq!(payload["type"], "cam_notification");
        assert_eq!(payload["urgency"], "HIGH");
        assert_eq!(payload["event_type"], "Error");
        assert_eq!(payload["event"]["message"], "API rate limit exceeded");
        assert_eq!(payload["summary"], "发生错误");
    }

    #[test]
    fn test_create_payload_waiting_for_input() {
        let notifier = OpenclawNotifier::new();

        let payload = notifier.create_payload("cam-789", "WaitingForInput", "Confirmation", "Continue? [Y/n]");

        assert_eq!(payload["urgency"], "HIGH");
        assert_eq!(payload["event_type"], "WaitingForInput");
        assert_eq!(payload["event"]["pattern_type"], "Confirmation");
        assert_eq!(payload["event"]["prompt"], "Continue? [Y/n]");
        assert!(payload["summary"].as_str().unwrap().contains("Confirmation"));
    }

    #[test]
    fn test_create_payload_agent_exited() {
        let notifier = OpenclawNotifier::new();

        let payload = notifier.create_payload("cam-abc", "AgentExited", "/myproject", "");

        assert_eq!(payload["urgency"], "MEDIUM");
        assert_eq!(payload["event_type"], "AgentExited");
        assert_eq!(payload["event"]["project_path"], "/myproject");
        assert_eq!(payload["summary"], "Agent 已退出");
    }

    #[test]
    fn test_create_payload_notification_idle_prompt() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "idle_prompt", "message": "Task completed"}"#;
        let payload = notifier.create_payload("cam-def", "notification", "", context);

        assert_eq!(payload["urgency"], "MEDIUM");
        assert_eq!(payload["event"]["notification_type"], "idle_prompt");
        assert_eq!(payload["event"]["message"], "Task completed");
        assert_eq!(payload["summary"], "等待用户输入");
    }

    #[test]
    fn test_create_payload_with_terminal_snapshot() {
        let notifier = OpenclawNotifier::new();

        // 使用 AgentExited 测试（MEDIUM urgency），因为 stop 现在是 LOW
        let context = r#"{"cwd": "/workspace"}

--- 终端快照 ---
$ cargo build
   Compiling myapp v0.1.0
    Finished release target"#;

        let payload = notifier.create_payload("cam-123", "AgentExited", "", context);

        assert_eq!(payload["urgency"], "MEDIUM");
        assert!(payload["terminal_snapshot"].as_str().is_some());
        assert!(payload["terminal_snapshot"].as_str().unwrap().contains("cargo build"));
    }

    #[test]
    fn test_create_payload_snapshot_truncation() {
        let notifier = OpenclawNotifier::new();

        // 创建超过 15 行的终端输出
        let mut long_output = String::from(r#"{"cwd": "/tmp"}

--- 终端快照 ---
"#);
        for i in 1..=20 {
            long_output.push_str(&format!("line {}\n", i));
        }

        let payload = notifier.create_payload("cam-123", "stop", "", &long_output);

        let snapshot = payload["terminal_snapshot"].as_str().unwrap();
        // 应该只包含最后 15 行
        assert!(snapshot.contains("line 20"));
        assert!(snapshot.contains("line 6"));
        assert!(!snapshot.contains("line 5\n"));
    }

    #[test]
    fn test_generate_summary() {
        let notifier = OpenclawNotifier::new();

        // permission_request
        let json: Option<serde_json::Value> = serde_json::from_str(r#"{"tool_name": "Write"}"#).ok();
        assert!(notifier.generate_summary("permission_request", &json, "").contains("Write"));

        // Error
        assert_eq!(notifier.generate_summary("Error", &None, ""), "发生错误");

        // AgentExited
        assert_eq!(notifier.generate_summary("AgentExited", &None, ""), "Agent 已退出");

        // WaitingForInput
        assert!(notifier.generate_summary("WaitingForInput", &None, "Confirmation").contains("Confirmation"));
    }

    // ==================== 新格式辅助函数测试 ====================

    #[test]
    fn test_extract_project_name() {
        assert_eq!(OpenclawNotifier::extract_project_name("/Users/admin/workspace/myapp"), "myapp");
        assert_eq!(OpenclawNotifier::extract_project_name("/workspace"), "workspace");
        assert_eq!(OpenclawNotifier::extract_project_name(""), "unknown");
        // Root path returns "/" as the file_name
        assert_eq!(OpenclawNotifier::extract_project_name("/"), "/");
    }

    #[test]
    fn test_get_project_name_for_agent() {
        // 测试 agent_id 简化（当 agents.json 中找不到时）
        let name = OpenclawNotifier::get_project_name_for_agent("cam-1234567890");
        assert_eq!(name, "agent-1234");

        // 短 agent_id 不简化
        let name2 = OpenclawNotifier::get_project_name_for_agent("cam-123");
        assert_eq!(name2, "cam-123");

        // 外部会话 agent_id 简化（当 agents.json 中找不到时）
        // 注意：如果 agents.json 中有此 agent，会返回实际项目名
        let name3 = OpenclawNotifier::get_project_name_for_agent("ext-nonexist");
        assert_eq!(name3, "session-none");

        // 短外部会话 agent_id 不简化
        let name4 = OpenclawNotifier::get_project_name_for_agent("ext-123");
        assert_eq!(name4, "ext-123");
    }

    #[test]
    fn test_clean_terminal_context() {
        // 测试：保留选项和问题（Claude Code 格式：选项在前，问题在后）
        let raw = "Old content\n─────────────\n> \n📡 via direct\n1. Option one\n2. Option two\nActual question?";
        let cleaned = OpenclawNotifier::clean_terminal_context(raw);
        // 应该保留选项和问题
        assert!(cleaned.contains("Actual question?"));
        assert!(cleaned.contains("1. Option one"));
        assert!(cleaned.contains("2. Option two"));
        assert!(!cleaned.contains("─────"));
        assert!(!cleaned.contains("📡 via direct"));
        // Old content 应该被过滤掉（因为在选项之前）
        assert!(!cleaned.contains("Old content"));
    }

    #[test]
    fn test_is_numbered_choice() {
        assert!(OpenclawNotifier::is_numbered_choice("1. Option one\n2. Option two"));
        assert!(OpenclawNotifier::is_numbered_choice("  1. Indented option"));
        assert!(!OpenclawNotifier::is_numbered_choice("No numbers here"));
        assert!(!OpenclawNotifier::is_numbered_choice("10. Double digit")); // 只匹配 1-9
    }

    #[test]
    fn test_extract_choices() {
        let context = "Choose:\n1. First option\n2. Second option\n3. Third";
        let choices = OpenclawNotifier::extract_choices(context);
        assert_eq!(choices.len(), 3);
        assert_eq!(choices[0], "1. First option");
        assert_eq!(choices[1], "2. Second option");
        assert_eq!(choices[2], "3. Third");
    }

    #[test]
    fn test_is_confirmation_prompt() {
        assert!(OpenclawNotifier::is_confirmation_prompt("Continue? [Y/n]"));
        assert!(OpenclawNotifier::is_confirmation_prompt("Delete? [y/N]"));
        assert!(OpenclawNotifier::is_confirmation_prompt("[Y]es / [N]o / [A]lways"));
        assert!(OpenclawNotifier::is_confirmation_prompt("确认？[是/否]"));
        assert!(!OpenclawNotifier::is_confirmation_prompt("Enter your name:"));
    }

    #[test]
    fn test_extract_confirmation_question() {
        let context = "Write to /tmp/test.txt?\n[Y]es / [N]o / [A]lways";
        let question = OpenclawNotifier::extract_confirmation_question(context);
        assert!(question.contains("Write to /tmp/test.txt"));
        assert!(!question.contains("[Y]es"));
    }

    #[test]
    fn test_is_colon_prompt() {
        assert!(OpenclawNotifier::is_colon_prompt("Enter your name:"));
        assert!(OpenclawNotifier::is_colon_prompt("请输入文件名："));
        assert!(!OpenclawNotifier::is_colon_prompt("Continue? [Y/n]"));
    }

    #[test]
    fn test_extract_colon_question() {
        let context = "Some info\nEnter your name:";
        let question = OpenclawNotifier::extract_colon_question(context);
        assert_eq!(question, "Enter your name:");
    }

    // ==================== 新格式集成测试 ====================

    #[test]
    fn test_format_numbered_choice_notification() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "idle_prompt", "message": ""}

--- 终端快照 ---
Choose an option:
1. Create new file
2. Edit existing
3. Delete file
❯ "#;

        let message = notifier.format_event("cam-123", "notification", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待选择"));
        assert!(message.contains("1. Create new file"));
        assert!(message.contains("2. Edit existing"));
        assert!(message.contains("回复数字"));
    }

    #[test]
    fn test_format_confirmation_notification() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "idle_prompt", "message": ""}

--- 终端快照 ---
Write to /tmp/test.txt?
[Y]es / [N]o / [A]lways / [D]on't ask"#;

        let message = notifier.format_event("cam-123", "notification", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("请求确认") || message.contains("确认"));
        assert!(message.contains("y/n"));
    }

    #[test]
    fn test_format_colon_prompt_notification() {
        let notifier = OpenclawNotifier::new();

        let context = r#"{"notification_type": "idle_prompt", "message": ""}

--- 终端快照 ---
Enter the file name:"#;

        let message = notifier.format_event("cam-123", "notification", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
        assert!(message.contains("Enter the file name:"));
        assert!(message.contains("回复内容"));
    }

    #[test]
    fn test_clean_terminal_context_real_output() {
        // 测试实际的 Claude Code 终端输出
        let raw = r#"  1. 核心功能 - 添加、删除、标记完成/未完成
  2. 筛选功能 - 全部/已完成/未完成 切换显示
  3. 编辑功能 - 双击编辑任务标题
  4. 清空已完成 - 一键删除所有已完成任务

  推荐选 1 和 2，保持简单实用。你想要哪些？

❯ 1

⏺ 好的，只保留核心功能：添加、删除、标记完成。

  我现在对需求有清晰的理解了，让我呈现设计方案。

  ---
  设计方案 - 第一部分：项目结构

  react-todo/
  ├── src/
  │   ├── components/
  │   │   ├── TodoInput.tsx      # 输入框组件
  │   │   ├── TodoItem.tsx       # 单个任务项
  │   │   └── TodoList.tsx       # 任务列表容器
  │   ├── hooks/
  │   │   └── useTodos.ts        # Todo 逻辑 + localStorage 持久化
  │   ├── types/
  │   │   └── todo.ts            # Todo 类型定义
  │   ├── App.tsx                # 主应用组件
  │   ├── main.tsx               # 入口文件
  │   └── index.css              # Tailwind 入口
  ├── index.html
  ├── package.json
  ├── tailwind.config.js
  ├── tsconfig.json
  └── vite.config.ts

  核心设计决策：
  - 使用自定义 Hook useTodos 封装所有状态逻辑和 localStorage 操作
  - 组件保持纯展示，逻辑集中在 Hook 中
  - 扁平结构，不过度拆分

  这个结构看起来合适吗？

────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
❯
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  [Opus 4.6] ███░░░░░░░ 27% | ⏱️  1h 44m
  workspace git:(main*)
  2 MCPs | 5 hooks
  ✓ Skill ×1 | ✓ Bash ×1"#;

        let cleaned = OpenclawNotifier::clean_terminal_context(raw);
        println!("=== Cleaned output ===");
        println!("{}", cleaned);
        println!("=== End ===");

        // 应该包含最后一个问题
        assert!(cleaned.contains("这个结构看起来合适吗？"), "Should contain the question");
    }

    // ==================== AI 提取测试 ====================

    #[test]
    fn test_extract_json_from_output() {
        // 测试从 AI 输出中提取 JSON
        let output = r#"Here is the extracted question:
{"question_type": "open", "question": "这个结构看起来合适吗？", "reply_hint": "回复内容"}
That's the result."#;

        let json = OpenclawNotifier::extract_json_from_output(output);
        assert!(json.is_some());
        let json_str = json.unwrap();
        assert!(json_str.contains("question_type"));
        assert!(json_str.contains("open"));
    }

    #[test]
    fn test_extract_json_from_output_no_json() {
        let output = "No JSON here, just plain text.";
        let json = OpenclawNotifier::extract_json_from_output(output);
        assert!(json.is_none());
    }

    #[test]
    fn test_extract_json_from_output_malformed() {
        // 只有开括号没有闭括号
        let output = "Some text { incomplete json";
        let json = OpenclawNotifier::extract_json_from_output(output);
        assert!(json.is_none());
    }

    #[test]
    fn test_with_no_ai_flag() {
        let notifier = OpenclawNotifier::new().with_no_ai(true);
        assert!(notifier.no_ai);

        // AI 提取应该返回 None
        let result = notifier.extract_question_with_ai("Some terminal output");
        assert!(result.is_none());
    }

    #[test]
    fn test_format_notification_with_no_ai_fallback() {
        // 测试当 AI 禁用时，回退到显示原始快照
        let notifier = OpenclawNotifier::new().with_no_ai(true);

        let context = r#"{"notification_type": "idle_prompt", "message": ""}

--- 终端快照 ---
Some unrecognized prompt format that doesn't match any pattern
Please provide your input here"#;

        let message = notifier.format_event("cam-123", "notification", "", context);

        // 应该回退到显示原始快照内容
        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
        // 应该包含原始快照内容（因为 AI 被禁用）
        assert!(message.contains("Please provide your input here") || message.contains("回复内容"));
    }

    #[test]
    fn test_format_notification_ai_extraction_path() {
        // 测试 AI 提取路径（不实际调用 AI，只验证代码路径）
        let notifier = OpenclawNotifier::new().with_dry_run(true);

        let context = r#"{"notification_type": "idle_prompt", "message": ""}

--- 终端快照 ---
Some complex terminal output
That doesn't match standard patterns
But contains a question somewhere"#;

        // dry_run 模式下 AI 提取会跳过，回退到显示原始快照
        let message = notifier.format_event("cam-123", "notification", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
    }

    #[test]
    fn test_ai_extract_timeout_constant() {
        // 验证超时常量设置为 5 秒
        assert_eq!(OpenclawNotifier::AI_EXTRACT_TIMEOUT_SECS, 5);
    }

    // ==================== ClaudePrompt 等待输入测试 ====================

    #[test]
    fn test_format_claude_prompt_with_question() {
        // 测试 ClaudePrompt 类型能正确显示问题内容
        let notifier = OpenclawNotifier::new().with_no_ai(true);

        // 模拟实际的 Claude Code 终端输出
        let context = r#"这个结构看起来合适吗？
❯ "#;

        let message = notifier.format_event("cam-123", "WaitingForInput", "ClaudePrompt", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
        // 关键：应该包含问题内容
        assert!(message.contains("这个结构看起来合适吗？"), "Should contain the question");
        assert!(message.contains("回复内容"));
    }

    #[test]
    fn test_format_claude_prompt_with_numbered_choices() {
        // 测试 ClaudePrompt 类型能正确显示编号选项和问题
        let notifier = OpenclawNotifier::new().with_no_ai(true);

        let context = r#"选择一个选项：
1. 选项一
2. 选项二
3. 选项三
❯ "#;

        let message = notifier.format_event("cam-123", "WaitingForInput", "ClaudePrompt", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待选择"));
        assert!(message.contains("选择一个选项"));
        assert!(message.contains("1. 选项一"));
        assert!(message.contains("2. 选项二"));
        assert!(message.contains("回复数字"));
    }

    #[test]
    fn test_format_claude_prompt_with_confirmation() {
        // 测试 ClaudePrompt 类型能正确识别确认提示
        let notifier = OpenclawNotifier::new().with_no_ai(true);

        let context = r#"Write to /tmp/test.txt?
[Y]es / [N]o / [A]lways
❯ "#;

        let message = notifier.format_event("cam-123", "WaitingForInput", "ClaudePrompt", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("请求确认") || message.contains("确认"));
        assert!(message.contains("y/n"));
    }

    #[test]
    fn test_format_waiting_input_empty_pattern_type() {
        // 测试空 pattern_type（通用处理分支）也能正确显示问题内容
        let notifier = OpenclawNotifier::new().with_no_ai(true);

        // 模拟 notify 命令的场景：pattern_type 为空，但有终端快照
        // 注意：Claude Code 格式是选项在前，问题在后
        let context = r#"{"cwd": "/workspace"}

--- 终端快照 ---
1. 个人学习/练习
2. 作品集展示
3. 实际使用
4. 技术探索

你的目标是哪个？

❯ "#;

        // 空 pattern_type
        let message = notifier.format_event("cam-123", "WaitingForInput", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待选择"));
        assert!(message.contains("你的目标是哪个？"));
        assert!(message.contains("1. 个人学习/练习"));
        assert!(message.contains("回复数字"));
    }

    #[test]
    fn test_format_waiting_input_empty_pattern_type_with_question() {
        // 测试空 pattern_type 时普通问题也能显示
        let notifier = OpenclawNotifier::new().with_no_ai(true);

        let context = r#"{"cwd": "/workspace"}

--- 终端快照 ---
这个结构看起来合适吗？
❯ "#;

        let message = notifier.format_event("cam-123", "WaitingForInput", "", context);

        assert!(message.contains("⏸️"));
        assert!(message.contains("等待输入"));
        // 关键：应该包含问题内容
        assert!(message.contains("这个结构看起来合适吗？"), "Should contain the question");
        assert!(message.contains("回复内容"));
    }

    // ==================== 多轮对话选项提取测试 ====================

    #[test]
    fn test_clean_terminal_context_multi_round_same_numbers() {
        // 测试多轮对话，每轮选项编号都是 1-4
        let raw = r#"样式方案偏好？
1. Tailwind CSS
2. CSS Modules
3. styled-components
4. 纯 CSS
需要哪些核心功能？（可多选）
1. 基础功能
2. 基础 + 编辑
3. 基础 + 筛选
4. 全部"#;

        let cleaned = OpenclawNotifier::clean_terminal_context(raw);

        // 应该只包含最后一组选项和对应的问题
        assert!(cleaned.contains("需要哪些核心功能？"), "Should contain the last question");
        assert!(cleaned.contains("1. 基础功能"), "Should contain last group option 1");
        assert!(cleaned.contains("4. 全部"), "Should contain last group option 4");
        // 不应该包含第一组的选项
        assert!(!cleaned.contains("Tailwind"), "Should NOT contain first group options");
        assert!(!cleaned.contains("styled-components"), "Should NOT contain first group options");
    }

    #[test]
    fn test_clean_terminal_context_multi_round_different_numbers() {
        // 测试多轮对话，选项编号不同（1-4, 1-3）
        let raw = r#"使用场景？
1. 个人任务管理
2. 项目任务跟踪
3. 团队协作
4. 学习演示
技术栈偏好？
1. Vite + React + TypeScript
2. Vite + React + JavaScript
3. Next.js"#;

        let cleaned = OpenclawNotifier::clean_terminal_context(raw);

        // 应该只包含最后一组选项
        assert!(cleaned.contains("技术栈偏好？"), "Should contain the last question");
        assert!(cleaned.contains("1. Vite + React + TypeScript"), "Should contain last group option 1");
        assert!(cleaned.contains("3. Next.js"), "Should contain last group option 3");
        // 不应该包含第一组的选项
        assert!(!cleaned.contains("个人任务管理"), "Should NOT contain first group options");
        assert!(!cleaned.contains("学习演示"), "Should NOT contain first group options");
    }

    #[test]
    fn test_clean_terminal_context_question_with_parentheses() {
        // 测试问题后有括号说明的情况
        let raw = r#"需要哪些核心功能？（可多选）
1. 基础功能
2. 基础 + 编辑
3. 全部"#;

        let cleaned = OpenclawNotifier::clean_terminal_context(raw);

        // 应该包含带括号的问题
        assert!(cleaned.contains("需要哪些核心功能？（可多选）"), "Should contain question with parentheses");
        assert!(cleaned.contains("1. 基础功能"), "Should contain option 1");
    }

    #[test]
    fn test_extract_choice_question_with_parentheses() {
        // 测试 extract_choice_question 能正确提取带括号的问题
        let context = r#"需要哪些核心功能？（可多选）
1. 基础功能
2. 基础 + 编辑
3. 全部"#;

        let question = OpenclawNotifier::extract_choice_question(context);

        assert!(question.is_some(), "Should find the question");
        assert!(question.unwrap().contains("需要哪些核心功能？"), "Should contain the question text");
    }

    #[test]
    fn test_extract_choices_only_last_group() {
        // 测试 extract_choices 在清洗后的内容上只提取最后一组
        // 注意：extract_choices 是在 clean_terminal_context 清洗后的内容上调用的
        let cleaned = r#"技术栈偏好？
1. Vite + React + TypeScript
2. Vite + React + JavaScript
3. Next.js"#;

        let choices = OpenclawNotifier::extract_choices(cleaned);

        assert_eq!(choices.len(), 3, "Should have 3 choices");
        assert!(choices[0].contains("Vite + React + TypeScript"));
        assert!(choices[2].contains("Next.js"));
    }

    #[test]
    fn test_format_notification_multi_round() {
        // 端到端测试：多轮对话的通知格式化
        let notifier = OpenclawNotifier::new().with_no_ai(true);

        let context = r#"{"notification_type": "idle_prompt", "cwd": "/workspace"}

--- 终端快照 ---
样式方案偏好？

1. Tailwind CSS
2. CSS Modules
3. styled-components
4. 纯 CSS

❯ 1

⏺ 好的，Tailwind CSS。

需要哪些核心功能？（可多选）

1. 基础功能
2. 基础 + 编辑
3. 基础 + 筛选
4. 全部

❯"#;

        let message = notifier.format_event("cam-123", "notification", "", context);

        // 应该只显示最后一组选项
        assert!(message.contains("需要哪些核心功能"), "Should contain the last question");
        assert!(message.contains("1. 基础功能"), "Should contain last group option 1");
        assert!(message.contains("4. 全部"), "Should contain last group option 4");
        // 不应该包含第一组的选项
        assert!(!message.contains("Tailwind"), "Should NOT contain first group options");
        assert!(!message.contains("styled-components"), "Should NOT contain first group options");
        // 应该有回复提示
        assert!(message.contains("回复数字选择"), "Should have reply hint");
    }

    #[test]
    fn test_clean_terminal_context_open_question_with_context() {
        // 测试开放式问题（无选项）保留前面的上下文
        let context = r#"❯ 1

⏺ 好的，保持最简单。

我现在对需求有了清晰的理解，让我分段呈现设计方案。

---
设计方案 - 第一部分：项目结构

react-todo/
├── src/
│   ├── components/
│   │   ├── TodoInput.tsx
│   │   ├── TodoItem.tsx
│   │   └── TodoList.tsx
│   ├── hooks/
│   │   └── useTodos.ts
│   └── App.tsx

设计思路：
- 组件职责单一
- 状态集中管理

这部分结构看起来合适吗？"#;

        let cleaned = OpenclawNotifier::clean_terminal_context(context);

        // 应该包含问题
        assert!(cleaned.contains("这部分结构看起来合适吗"), "Should contain the question");
        // 应该包含目录结构（上下文）
        assert!(cleaned.contains("react-todo/"), "Should contain directory structure");
        assert!(cleaned.contains("├── src/"), "Should contain tree structure");
        assert!(cleaned.contains("TodoInput.tsx"), "Should contain file names");
        // 应该包含设计说明
        assert!(cleaned.contains("设计方案"), "Should contain section title");
        // 不应该包含分隔符之前的内容
        assert!(!cleaned.contains("好的，保持最简单"), "Should NOT contain content before separator");
        assert!(!cleaned.contains("❯ 1"), "Should NOT contain user input");
    }

    #[test]
    fn test_clean_terminal_context_open_question_with_code_block() {
        // 测试开放式问题保留代码块上下文
        let context = r#"⏺ 修改后的代码：

fn main() {
    let items = vec![1, 2, 3];
    for item in items {
        println!("{}", item);
    }
}

这样修改可以吗？"#;

        let cleaned = OpenclawNotifier::clean_terminal_context(context);

        // 应该包含问题
        assert!(cleaned.contains("这样修改可以吗"), "Should contain the question");
        // 应该包含代码
        assert!(cleaned.contains("fn main()"), "Should contain code");
        assert!(cleaned.contains("println!"), "Should contain code content");
        // 不应该包含 agent 响应标记
        assert!(!cleaned.contains("⏺"), "Should NOT contain agent marker");
    }

    #[test]
    fn test_clean_terminal_context_open_question_max_lines() {
        // 测试上下文行数限制（最多 15 行）
        // 实际场景：有分隔符的情况下，从分隔符后开始
        let mut lines = Vec::new();
        // 添加早期内容
        for i in 1..=5 {
            lines.push(format!("Early line {}", i));
        }
        // 添加分隔符
        lines.push("---".to_string());
        // 添加 20 行内容（超过 15 行限制）
        for i in 1..=20 {
            lines.push(format!("Content line {}", i));
        }
        lines.push("这个方案可以吗？".to_string());

        let context = lines.join("\n");
        let cleaned = OpenclawNotifier::clean_terminal_context(&context);

        // 应该包含问题
        assert!(cleaned.contains("这个方案可以吗"), "Should contain the question");
        // 应该包含分隔符后的内容
        assert!(cleaned.contains("Content line 20"), "Should contain recent content");
        // 不应该包含分隔符之前的内容
        assert!(!cleaned.contains("Early line"), "Should NOT contain content before separator");
    }

    #[test]
    fn test_find_context_start_stops_at_separator() {
        // 测试 find_context_start 在分隔符处停止
        let lines = vec![
            "早期内容",
            "---",
            "设计方案",
            "代码结构",
            "这个可以吗？",
        ];

        let start = OpenclawNotifier::find_context_start(&lines, 4);

        // 应该从分隔符后开始（索引 2）
        assert_eq!(start, 2, "Should start after separator");
    }

    #[test]
    fn test_find_context_start_stops_at_user_input() {
        // 测试 find_context_start 在用户输入处停止
        let lines = vec![
            "之前的问题",
            "❯ 1",
            "新的内容",
            "代码结构",
            "这个可以吗？",
        ];

        let start = OpenclawNotifier::find_context_start(&lines, 4);

        // 应该从用户输入后开始（索引 2）
        assert_eq!(start, 2, "Should start after user input");
    }

    #[test]
    fn test_find_context_start_stops_at_agent_response() {
        // 测试 find_context_start 在 agent 响应处停止
        let lines = vec![
            "之前的内容",
            "⏺ 好的，我来处理",
            "新的设计",
            "代码结构",
            "这个可以吗？",
        ];

        let start = OpenclawNotifier::find_context_start(&lines, 4);

        // 应该从 agent 响应后开始（索引 2）
        assert_eq!(start, 2, "Should start after agent response");
    }
}
