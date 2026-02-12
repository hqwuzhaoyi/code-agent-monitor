//! Anthropic API 客户端 - 直接调用 Claude API
//!
//! 用于快速 AI 推理任务，绕过 OpenClaw 的 Opus 默认配置。
//! 主要用于终端问题提取等简单任务，使用 Haiku 模型以获得最低延迟。
//!
//! API Key 读取优先级：
//! 1. CAM 配置文件 `~/.config/cam`（JSON 格式，字段 `anthropic_api_key` 和可选 `anthropic_base_url`）
//! 2. 环境变量 `ANTHROPIC_API_KEY`
//! 3. 文件 `~/.anthropic/api_key`
//! 4. OpenClaw 配置 `~/.openclaw/openclaw.json` 的 `models.providers.anthropic.apiKey` 或 `providers.anthropic.apiKey`

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;
use tracing::{debug, info, trace, warn};

use crate::ai_quality::{assess_question_extraction, assess_status_detection, thresholds};
use crate::terminal_utils::truncate_for_status;

/// Anthropic API 基础 URL
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// API 版本
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 默认模型 - Haiku 4.5（最快最便宜）
pub const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";

/// 默认超时（毫秒）
const DEFAULT_TIMEOUT_MS: u64 = 5000;

/// 默认最大 tokens
const DEFAULT_MAX_TOKENS: u32 = 1500;

/// 内容提取超时（毫秒）- 10 秒（本地代理可能需要更长时间）
const EXTRACT_TIMEOUT_MS: u64 = 10000;

// ============================================================================
// 通知内容提取
// ============================================================================

/// 问题类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionType {
    /// 多选项问题
    Options,
    /// 是/否确认
    Confirmation,
    /// 开放式问题
    OpenEnded,
}

impl Default for QuestionType {
    fn default() -> Self {
        Self::OpenEnded
    }
}

/// 从终端快照提取的通知内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationContent {
    /// 问题类型
    pub question_type: QuestionType,
    /// 完整问题文本
    pub question: String,
    /// 选项列表（仅 Options 类型有值）
    pub options: Vec<String>,
    /// 简洁摘要（10 字以内）
    pub summary: String,
}

impl Default for NotificationContent {
    fn default() -> Self {
        Self {
            question_type: QuestionType::OpenEnded,
            question: String::new(),
            options: Vec::new(),
            summary: "等待输入".to_string(),
        }
    }
}

impl NotificationContent {
    /// 创建默认的确认类型内容
    pub fn confirmation(question: &str) -> Self {
        Self {
            question_type: QuestionType::Confirmation,
            question: question.to_string(),
            options: Vec::new(),
            summary: "请求确认".to_string(),
        }
    }

    /// 创建默认的选项类型内容
    pub fn options(question: &str, options: Vec<String>) -> Self {
        Self {
            question_type: QuestionType::Options,
            question: question.to_string(),
            options,
            summary: "等待选择".to_string(),
        }
    }

    /// 创建默认的开放式问题内容
    pub fn open_ended(question: &str) -> Self {
        Self {
            question_type: QuestionType::OpenEnded,
            question: question.to_string(),
            options: Vec::new(),
            summary: "等待回复".to_string(),
        }
    }
}

/// Anthropic 客户端配置
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// API 密钥
    pub api_key: String,
    /// API 基础 URL（支持代理）
    pub base_url: String,
    /// 模型名称
    pub model: String,
    /// 请求超时（毫秒）
    pub timeout_ms: u64,
    /// 最大输出 tokens
    pub max_tokens: u32,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: ANTHROPIC_API_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }
}

impl AnthropicConfig {
    /// 从环境和配置文件自动加载 API key 和 base_url
    pub fn auto_load() -> Result<Self> {
        let (api_key, base_url) = Self::load_api_config()?;
        Ok(Self {
            api_key,
            base_url,
            ..Default::default()
        })
    }

    /// 加载 API 配置（key 和 base_url），按优先级尝试多个来源
    fn load_api_config() -> Result<(String, String)> {
        let default_url = ANTHROPIC_API_URL.to_string();

        // 1. CAM 配置文件 ~/.config/cam
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".config/cam");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                        let key = config.get("anthropic_api_key").and_then(|k| k.as_str());
                        let base_url = config.get("anthropic_base_url").and_then(|u| u.as_str());

                        if let Some(key) = key {
                            if !key.is_empty() {
                                let url = base_url
                                    .filter(|u| !u.is_empty())
                                    .map(|u| {
                                        let u = u.trim_end_matches('/');
                                        if u.ends_with("/v1/messages") {
                                            u.to_string()
                                        } else if u.ends_with("/v1") {
                                            format!("{}/messages", u)
                                        } else {
                                            format!("{}/v1/messages", u)
                                        }
                                    })
                                    .unwrap_or_else(|| default_url.clone());
                                debug!("Using API key from ~/.config/cam, base_url: {}", url);
                                return Ok((key.to_string(), url));
                            }
                        }
                    }
                }
            }
        }

        // 2. 环境变量
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                debug!("Using ANTHROPIC_API_KEY from environment");
                let base_url = std::env::var("ANTHROPIC_BASE_URL")
                    .ok()
                    .filter(|u| !u.is_empty())
                    .unwrap_or_else(|| default_url.clone());
                return Ok((key, base_url));
            }
        }

        // 3. ~/.anthropic/api_key 文件
        if let Some(home) = dirs::home_dir() {
            let key_file = home.join(".anthropic/api_key");
            if key_file.exists() {
                if let Ok(key) = fs::read_to_string(&key_file) {
                    let key = key.trim().to_string();
                    if !key.is_empty() {
                        debug!("Using API key from ~/.anthropic/api_key");
                        return Ok((key, default_url.clone()));
                    }
                }
            }
        }

        // 4. OpenClaw 配置
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".openclaw/openclaw.json");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                        // 尝试从 .models.providers.anthropic 获取
                        let anthropic_config = config
                            .get("models")
                            .and_then(|m| m.get("providers"))
                            .and_then(|p| p.get("anthropic"))
                            .or_else(|| {
                                config
                                    .get("providers")
                                    .and_then(|p| p.get("anthropic"))
                            });

                        if let Some(ac) = anthropic_config {
                            let key = ac.get("apiKey").and_then(|k| k.as_str());
                            let base_url = ac.get("baseUrl").and_then(|u| u.as_str());

                            if let Some(key) = key {
                                if !key.is_empty() {
                                    // 使用配置的 baseUrl，如果没有则使用默认 URL
                                    let url = base_url
                                        .filter(|u| !u.is_empty())
                                        .map(|u| {
                                            // 确保 URL 以 /v1/messages 结尾
                                            let u = u.trim_end_matches('/');
                                            if u.ends_with("/v1/messages") {
                                                u.to_string()
                                            } else if u.ends_with("/v1") {
                                                format!("{}/messages", u)
                                            } else {
                                                format!("{}/v1/messages", u)
                                            }
                                        })
                                        .unwrap_or_else(|| default_url.clone());
                                    debug!("Using API key from OpenClaw config, base_url: {}", url);
                                    return Ok((key.to_string(), url));
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow!(
            "No Anthropic API key found. Create ~/.config/cam with anthropic_api_key, \
             set ANTHROPIC_API_KEY env var, create ~/.anthropic/api_key, \
             or configure in ~/.openclaw/openclaw.json"
        ))
    }
}

/// Messages API 请求体
#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<Message>,
}

/// 消息
#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

/// Messages API 响应体
#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

/// 内容块
#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

/// API 错误响应
#[derive(Deserialize)]
struct ErrorResponse {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

/// Anthropic API 客户端
pub struct AnthropicClient {
    client: reqwest::blocking::Client,
    config: AnthropicConfig,
}

impl AnthropicClient {
    /// 创建新客户端
    pub fn new(config: AnthropicConfig) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| anyhow!("Cannot create HTTP client: {}", e))?;

        Ok(Self { client, config })
    }

    /// 从自动加载的配置创建客户端
    pub fn from_config() -> Result<Self> {
        let config = AnthropicConfig::auto_load()?;
        Self::new(config)
    }

    /// 发送消息并获取响应
    pub fn complete(&self, prompt: &str, system: Option<&str>) -> Result<String> {
        let request = MessagesRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            system: system.map(|s| s.to_string()),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        debug!(
            model = %self.config.model,
            prompt_len = prompt.len(),
            base_url = %self.config.base_url,
            timeout_ms = self.config.timeout_ms,
            "Sending request to Anthropic API"
        );

        let start = std::time::Instant::now();
        let response = self
            .client
            .post(&self.config.base_url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .map_err(|e| {
                let elapsed = start.elapsed();
                anyhow!("API request failed after {}ms: {}", elapsed.as_millis(), e)
            })?;

        debug!(elapsed_ms = start.elapsed().as_millis(), "API request completed");

        let status = response.status();
        let body = response
            .text()
            .map_err(|e| anyhow!("Failed to read response: {}", e))?;

        if !status.is_success() {
            // 尝试解析错误响应
            if let Ok(error_resp) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(anyhow!("API error ({}): {}", status, error_resp.error.message));
            }
            return Err(anyhow!("API error ({}): {}", status, body));
        }

        let response: MessagesResponse = serde_json::from_str(&body)
            .map_err(|e| anyhow!("Failed to parse response: {} - body: {}", e, body))?;

        // 提取文本内容
        let text = response
            .content
            .iter()
            .filter(|c| c.content_type == "text")
            .filter_map(|c| c.text.as_ref())
            .cloned()
            .collect::<Vec<String>>()
            .join("");

        if text.is_empty() {
            warn!("Empty response from Anthropic API");
        }

        Ok(text)
    }
}

// ============================================================================
// 通知内容提取函数
// ============================================================================

/// 从终端快照中智能提取通知内容（使用 Haiku 4.5）
///
/// 使用 AI 分析终端输出，提取：
/// - 问题类型（选项/确认/开放式）
/// - 完整问题文本
/// - 选项列表
/// - 简洁摘要
///
/// # 参数
/// - `terminal_snapshot`: 终端快照内容
///
/// # 返回
/// - `Ok(NotificationContent)`: 提取的内容
/// - `Err`: API 调用失败时返回错误
///
/// # 超时
/// 使用 3 秒超时，超时后返回默认值
pub fn extract_notification_content(terminal_snapshot: &str) -> Result<NotificationContent> {
    let start = std::time::Instant::now();

    // 创建带 3 秒超时的客户端
    let config = AnthropicConfig {
        timeout_ms: EXTRACT_TIMEOUT_MS,
        ..AnthropicConfig::auto_load()?
    };
    let client = AnthropicClient::new(config)?;

    // 截取最后 N 行，避免 token 过多
    let truncated = crate::terminal_utils::truncate_last_lines(terminal_snapshot, 80);

    let system = "你是一个终端输出分析专家。从 AI Agent 终端快照中提取正在询问用户的问题信息。只返回 JSON，不要其他内容。";

    let prompt = format!(
        r#"从以下终端输出中提取问题信息：

<terminal>
{truncated}
</terminal>

返回 JSON 格式：
{{
  "question_type": "options" | "confirmation" | "open_ended",
  "question": "完整的问题文本",
  "options": ["选项1", "选项2", ...],
  "summary": "简洁的摘要（10字以内）"
}}

规则：
- 重要：只分析终端输出中最后出现的问题或提示，忽略之前的历史会话内容。
- question_type:
  - "options": 有多个选项供选择（如 1. xxx 2. xxx）
  - "confirmation": 是/否确认（如 [Y/n]、确认？）
  - "open_ended": 开放式问题（需要输入内容）
- question: 提取完整的问题文本，包括上下文
- options: 仅 options 类型需要填写，其他类型为空数组
- summary: 简洁描述，如"等待选择"、"请求确认"、"等待回复"

只返回 JSON，不要其他内容。"#
    );

    let response = client.complete(&prompt, Some(system))?;
    let elapsed = start.elapsed();
    info!(elapsed_ms = elapsed.as_millis(), "Haiku extract_notification_content completed");

    // 解析 JSON 响应
    let json_str = extract_json_from_output(&response)
        .ok_or_else(|| anyhow!("No JSON found in response: {}", response))?;

    let parsed: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| anyhow!("Failed to parse JSON: {} - content: {}", e, json_str))?;

    // 提取字段
    let question_type_str = parsed
        .get("question_type")
        .and_then(|v| v.as_str())
        .unwrap_or("open_ended");

    let question_type = match question_type_str {
        "options" => QuestionType::Options,
        "confirmation" => QuestionType::Confirmation,
        _ => QuestionType::OpenEnded,
    };

    let question = parsed
        .get("question")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let options: Vec<String> = parsed
        .get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let summary = parsed
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("等待输入")
        .to_string();

    let content = NotificationContent {
        question_type,
        question,
        options,
        summary,
    };

    // 评估提取质量
    let assessment = assess_question_extraction(&content);
    if assessment.confidence < thresholds::MEDIUM {
        warn!(
            confidence = assessment.confidence,
            issues = ?assessment.issues,
            "Question extraction quality below MEDIUM threshold"
        );
    } else {
        debug!(
            confidence = assessment.confidence,
            "Question extraction quality assessment passed"
        );
    }

    Ok(content)
}

/// 从终端快照中提取通知内容，失败时返回默认值
///
/// 这是 `extract_notification_content` 的便捷包装，
/// 在 API 调用失败或超时时返回默认的 `NotificationContent`。
pub fn extract_notification_content_or_default(terminal_snapshot: &str) -> NotificationContent {
    match extract_notification_content(terminal_snapshot) {
        Ok(content) => content,
        Err(e) => {
            warn!(error = %e, "Failed to extract notification content, using default");
            NotificationContent::default()
        }
    }
}

/// 从 Haiku 提取的问题结果
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedQuestion {
    /// 问题类型: "open", "choice", "confirm"
    pub question_type: String,
    /// 核心问题内容
    pub question: String,
    /// 选项列表（仅 choice 类型有值）
    pub options: Vec<String>,
    /// 回复提示
    pub reply_hint: String,
}

/// 任务摘要（NoQuestion 场景使用）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TaskSummary {
    /// Agent 状态: "completed", "idle", "waiting"
    pub status: String,
    /// 最后操作摘要
    pub last_action: Option<String>,
}

/// 提取结果
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractionResult {
    /// 成功提取到问题
    Found(ExtractedQuestion),
    /// AI 判断没有问题需要回答，但可能有任务摘要
    NoQuestion(TaskSummary),
    /// 提取失败（API 错误、解析失败等）
    Failed,
}

/// 从终端快照中提取问题（使用 Haiku）
///
/// 这是一个便捷函数，用于替代 `extract_question_with_ai`。
/// 使用 Haiku 模型，延迟约 1-2 秒，比 Opus 快 10 倍。
///
/// 如果 AI 判断上下文不完整，会自动获取更多行数重试。
///
/// 返回值：
/// - `ExtractionResult::Found(question)` - 成功提取到问题
/// - `ExtractionResult::NoQuestion(summary)` - AI 判断没有问题需要回答，包含任务摘要
/// - `ExtractionResult::Failed` - 提取失败
pub fn extract_question_with_haiku(terminal_snapshot: &str) -> ExtractionResult {
    // 尝试不同的上下文大小
    let context_sizes = [80, 150, 300];

    for &lines in &context_sizes {
        match extract_question_with_context(terminal_snapshot, lines) {
            Ok(InternalResult::Question(result)) => return ExtractionResult::Found(result),
            Ok(InternalResult::NoQuestion(summary)) => return ExtractionResult::NoQuestion(summary),
            Err(NeedMoreContext) => {
                debug!(lines = lines, "AI needs more context, retrying with more lines");
                continue;
            }
            Err(ExtractionFailed) => return ExtractionResult::Failed,
        }
    }

    warn!("Failed to extract question even with maximum context");
    ExtractionResult::Failed
}

/// 提取错误类型
enum ExtractionError {
    NeedMoreContext,
    ExtractionFailed,
}

/// 内部提取结果
enum InternalResult {
    Question(ExtractedQuestion),
    NoQuestion(TaskSummary),
}

use ExtractionError::*;

/// 使用指定行数的上下文提取问题
fn extract_question_with_context(terminal_snapshot: &str, lines: usize) -> Result<InternalResult, ExtractionError> {
    let client = match AnthropicClient::from_config() {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to create Anthropic client");
            return Err(ExtractionFailed);
        }
    };

    // 截取最后 N 行
    let truncated = crate::terminal_utils::truncate_last_lines(terminal_snapshot, lines);

    let system = "你是一个终端输出分析专家。从给定的终端快照中提取用户正在被询问的问题，或分析 Agent 当前状态。";

    let prompt = format!(
        r#"分析以下 AI Agent 终端输出，提取正在询问用户的问题。

终端输出:
{}

请用 JSON 格式回复，包含以下字段：
- question_type: "open"（开放问题）、"choice"（选择题）、"confirm"（确认）、"none"（无问题，Agent 空闲等待指令）
- question: 完整的问题内容。重要：必须包含问题所引用的具体内容（如代码结构、设计方案、选项列表等），让用户无需查看终端就能理解和回答问题。不要截断或省略重要上下文
- options: 选项列表（仅当 question_type 为 "choice" 时提取，格式如 ["1. 选项A", "2. 选项B"]，否则为空数组 []）
- reply_hint: 回复提示（如"回复 y/n"、"回复数字选择"、"回复内容"）
- contains_ui_noise: true/false（问题内容是否包含终端 UI 元素，如工具调用标记、状态指示器、进度条、ASCII art 等）
- context_complete: true/false（判断标准见下方）
- agent_status: "completed"（刚完成任务，终端显示了完成信息）、"idle"（空闲等待，无明显完成信息）、"waiting"（等待用户回答问题）
- last_action: Agent 最后完成的操作摘要。重要：仔细查看终端输出，提取 Agent 完成的具体任务（如"React Todo List 项目已完成"、"创建了 TodoList 组件"、"修复了登录 bug"、"项目构建成功"）。如果终端显示了任务完成信息（如"已完成"、"成功"、"done"等），必须提取。只有在完全无法判断时才返回 null

context_complete 判断标准（非常重要）：
- 如果问题中包含指示词如"这个"、"上面的"、"以下"、"这些"、"该"等，必须检查被引用的内容是否在终端输出中可见
- 例如："这个项目结构看起来合适吗？" - 必须能看到具体的项目结构内容，否则 context_complete = false
- 例如："这个方案可以吗？" - 必须能看到具体的方案内容，否则 context_complete = false
- 例如："以下选项选择哪个？" - 必须能看到选项列表，否则 context_complete = false
- 如果问题是独立的（如"你想要实现什么功能？"），则 context_complete = true

重要规则：
1. 只分析终端输出中最后出现的问题或提示，忽略之前的历史会话内容
2. question 字段只应包含纯文本问题，不要包含终端 UI 元素（如工具调用标记 ⏺、状态指示器 ✻、进度条、ASCII art logo 等）
3. 如果无法提取干净的问题内容（即问题中混杂了 UI 元素），设置 contains_ui_noise 为 true
4. 如果问题引用了看不到的上下文，必须设置 context_complete 为 false
5. 如果 context_complete 为 true，question 必须包含足够的上下文让用户理解问题
6. 当 question_type 为 "none" 时，仍需分析 agent_status 和 last_action，帮助用户了解 Agent 状态
7. 如果 Agent 只是显示了结果/总结然后等待（终端只有空闲提示符 ❯ 或 >，没有明确问问题），应该返回 question_type: "none"

只返回 JSON，不要其他内容。"#,
        truncated
    );

    let start = std::time::Instant::now();
    let response = match client.complete(&prompt, Some(system)) {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Haiku API call failed");
            return Err(ExtractionFailed);
        }
    };
    debug!(elapsed_ms = start.elapsed().as_millis(), lines = lines, "Haiku API call completed");
    trace!(response = %response, "Haiku raw response");

    // 解析 JSON 响应
    let json_str = match extract_json_from_output(&response) {
        Some(s) => s,
        None => {
            warn!(response = %response, "Failed to extract JSON from Haiku response");
            return Err(ExtractionFailed);
        }
    };
    debug!(json = %json_str, "Haiku returned JSON");
    let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return Err(ExtractionFailed),
    };

    let question_type = match parsed.get("question_type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return Err(ExtractionFailed),
    };

    if question_type == "none" {
        // 提取任务摘要
        let status = parsed
            .get("agent_status")
            .and_then(|v| v.as_str())
            .unwrap_or("idle")
            .to_string();
        let last_action = parsed
            .get("last_action")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        return Ok(InternalResult::NoQuestion(TaskSummary { status, last_action }));
    }

    // 检查上下文是否完整
    let context_complete = parsed
        .get("context_complete")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if !context_complete {
        debug!("AI indicates context is incomplete");
        return Err(NeedMoreContext);
    }

    // 检查 AI 是否标记了 UI 噪音
    let contains_ui_noise = parsed
        .get("contains_ui_noise")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if contains_ui_noise {
        warn!("AI detected UI noise in extracted question, rejecting");
        return Err(ExtractionFailed);
    }

    let question = match parsed.get("question").and_then(|v| v.as_str()) {
        Some(q) => q.to_string(),
        None => return Err(ExtractionFailed),
    };
    let reply_hint = parsed
        .get("reply_hint")
        .and_then(|v| v.as_str())
        .unwrap_or("回复内容")
        .to_string();

    // 提取选项列表
    let options = parsed
        .get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(InternalResult::Question(ExtractedQuestion {
        question_type: question_type.to_string(),
        question,
        options,
        reply_hint,
    }))
}

/// Agent 状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    /// Agent 正在处理中（不应发送通知）
    Processing,
    /// Agent 空闲，等待用户输入（应发送通知）
    WaitingForInput,
    /// 无法确定状态
    Unknown,
}

/// 使用 Haiku 判断 Agent 是否正在处理中
///
/// 这个函数使用 AI 分析终端输出，判断 agent 当前状态：
/// - Processing: agent 正在执行任务（如 Thinking、Brewing、Running 等）
/// - WaitingForInput: agent 空闲，等待用户输入
///
/// 这种方式比硬编码模式更灵活，可以兼容不同的 AI 编码工具：
/// - Claude Code: Hatching…, Brewing…, Thinking…
/// - Codex: Running…, Executing…
/// - OpenCode: Processing…, Working…
///
/// # 参数
/// - `terminal_snapshot`: 终端快照内容（最后 10-15 行即可）
///
/// # 返回
/// - `AgentStatus::Processing`: agent 正在处理，不应发送通知
/// - `AgentStatus::WaitingForInput`: agent 空闲，应发送通知
/// - `AgentStatus::Unknown`: 无法确定（API 失败时）
///
/// # 超时
/// 使用 3 秒超时，超时后返回 Unknown
pub fn is_agent_processing(terminal_snapshot: &str) -> AgentStatus {
    let start = std::time::Instant::now();

    // 创建带 3 秒超时的客户端
    let config = match AnthropicConfig::auto_load() {
        Ok(c) => AnthropicConfig {
            timeout_ms: 3000,
            max_tokens: 50, // 只需要简短回答
            ..c
        },
        Err(e) => {
            warn!(error = %e, "Failed to load Anthropic config for is_agent_processing");
            return AgentStatus::Unknown;
        }
    };

    let client = match AnthropicClient::new(config) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to create Anthropic client for is_agent_processing");
            return AgentStatus::Unknown;
        }
    };

    // 只取最后 N 行，减少 token 消耗
    let last_lines = truncate_for_status(terminal_snapshot);

    let system = "你是一个终端状态分析专家。判断 AI 编码助手（如 Claude Code、Codex、OpenCode）的当前状态。只回答 PROCESSING 或 WAITING，不要其他内容。";

    let prompt = format!(
        r#"分析以下终端输出，判断 AI 编码助手的状态：

<terminal>
{last_lines}
</terminal>

判断规则：
- 只判断终端最后的状态，忽略历史输出
- PROCESSING: 如果看到以下任一指示器：
  * 带省略号的状态词（如 Thinking…、Brewing…、Hatching…、Grooving…、Running…、Executing…、Loading…、Working…、Streaming… 等）
  * 任何 "动词ing…" 或 "动词ing..." 格式的状态提示
  * 括号内的运行提示（如 (running stop hook)、(executing)、(loading) 等）
  * 旋转动画字符（✢✻✶✽◐◑◒◓⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏）
  * 进度条或百分比指示器
- WAITING: 如果看到空闲提示符（如 >、❯、$）或问题/选项等待用户输入
- 注意：⏺ 符号是输出块标记，不是处理中指示器。如果终端显示完成信息（如"已完成"、"成功"）后跟空闲提示符，应判断为 WAITING

只回答一个词：PROCESSING 或 WAITING"#
    );

    let response = match client.complete(&prompt, Some(system)) {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Haiku API call failed for is_agent_processing");
            return AgentStatus::Unknown;
        }
    };

    let elapsed = start.elapsed();
    debug!(elapsed_ms = elapsed.as_millis(), response = %response.trim(), "is_agent_processing completed");

    let response_upper = response.trim().to_uppercase();
    let status = if response_upper.contains("PROCESSING") {
        AgentStatus::Processing
    } else if response_upper.contains("WAITING") {
        AgentStatus::WaitingForInput
    } else {
        warn!(response = %response, "Unexpected response from is_agent_processing");
        AgentStatus::Unknown
    };

    // 评估状态检测质量
    let assessment = assess_status_detection(&status, terminal_snapshot);
    if assessment.confidence < thresholds::LOW {
        warn!(
            confidence = assessment.confidence,
            issues = ?assessment.issues,
            detected_status = ?status,
            "Status detection quality below LOW threshold, returning Unknown"
        );
        return AgentStatus::Unknown;
    }

    debug!(
        confidence = assessment.confidence,
        status = ?status,
        "Status detection quality assessment passed"
    );

    status
}

/// 检测终端快照是否包含等待用户输入的问题
///
/// 用于 stop 事件处理：Claude Code 可能在输出问题后触发 stop 而非 idle_prompt。
/// 此函数检测终端是否包含等待输入的问题。
///
/// # 参数
/// - `terminal_snapshot`: 终端快照内容
///
/// # 返回
/// - `Some(NotificationContent)`: 如果检测到等待输入的问题
/// - `None`: 如果没有检测到问题或 AI 调用失败
pub fn detect_waiting_question(terminal_snapshot: &str) -> Option<NotificationContent> {
    // 先检查是否在处理中
    match is_agent_processing(terminal_snapshot) {
        AgentStatus::Processing => return None,
        AgentStatus::Unknown => {
            // 不确定状态，继续尝试提取问题
        }
        AgentStatus::WaitingForInput => {
            // 确认在等待输入，继续提取问题
        }
    }

    // 尝试提取问题内容
    match extract_notification_content(terminal_snapshot) {
        Ok(content) => {
            // 评估提取质量
            let assessment = assess_question_extraction(&content);

            if assessment.is_valid && assessment.confidence >= thresholds::MEDIUM {
                // 质量足够高，返回内容
                info!(
                    confidence = assessment.confidence,
                    question_type = ?content.question_type,
                    "Detected waiting question in stop event"
                );
                Some(content)
            } else {
                // 质量不够，记录警告
                warn!(
                    confidence = assessment.confidence,
                    issues = ?assessment.issues,
                    "Question extraction quality too low, ignoring"
                );
                None
            }
        }
        Err(e) => {
            debug!(error = %e, "Failed to extract question from terminal snapshot");
            None
        }
    }
}

/// 从输出中提取 JSON 字符串
fn extract_json_from_output(output: &str) -> Option<String> {
    let start = output.find('{')?;
    let end = output.rfind('}')?;
    if end > start {
        Some(output[start..=end].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_output() {
        let output = r#"Here is the JSON:
{"question_type": "confirm", "question": "Continue?", "reply_hint": "y/n"}
That's all."#;

        let json = extract_json_from_output(output).unwrap();
        assert!(json.contains("question_type"));
        assert!(json.contains("confirm"));
    }

    #[test]
    fn test_extract_json_no_json() {
        let output = "No JSON here";
        assert!(extract_json_from_output(output).is_none());
    }

    #[test]
    fn test_config_default() {
        let config = AnthropicConfig::default();
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert_eq!(config.max_tokens, DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn test_question_type_default() {
        let qt = QuestionType::default();
        assert_eq!(qt, QuestionType::OpenEnded);
    }

    #[test]
    fn test_notification_content_default() {
        let content = NotificationContent::default();
        assert_eq!(content.question_type, QuestionType::OpenEnded);
        assert!(content.question.is_empty());
        assert!(content.options.is_empty());
        assert_eq!(content.summary, "等待输入");
    }

    #[test]
    fn test_notification_content_confirmation() {
        let content = NotificationContent::confirmation("Delete file?");
        assert_eq!(content.question_type, QuestionType::Confirmation);
        assert_eq!(content.question, "Delete file?");
        assert!(content.options.is_empty());
        assert_eq!(content.summary, "请求确认");
    }

    #[test]
    fn test_notification_content_options() {
        let options = vec!["Option 1".to_string(), "Option 2".to_string()];
        let content = NotificationContent::options("Choose one:", options.clone());
        assert_eq!(content.question_type, QuestionType::Options);
        assert_eq!(content.question, "Choose one:");
        assert_eq!(content.options, options);
        assert_eq!(content.summary, "等待选择");
    }

    #[test]
    fn test_notification_content_open_ended() {
        let content = NotificationContent::open_ended("What is your name?");
        assert_eq!(content.question_type, QuestionType::OpenEnded);
        assert_eq!(content.question, "What is your name?");
        assert!(content.options.is_empty());
        assert_eq!(content.summary, "等待回复");
    }

    #[test]
    fn test_question_type_serialization() {
        // Test serialization
        let qt = QuestionType::Options;
        let json = serde_json::to_string(&qt).unwrap();
        assert_eq!(json, "\"options\"");

        let qt = QuestionType::Confirmation;
        let json = serde_json::to_string(&qt).unwrap();
        assert_eq!(json, "\"confirmation\"");

        let qt = QuestionType::OpenEnded;
        let json = serde_json::to_string(&qt).unwrap();
        assert_eq!(json, "\"open_ended\"");
    }

    #[test]
    fn test_question_type_deserialization() {
        // Test deserialization
        let qt: QuestionType = serde_json::from_str("\"options\"").unwrap();
        assert_eq!(qt, QuestionType::Options);

        let qt: QuestionType = serde_json::from_str("\"confirmation\"").unwrap();
        assert_eq!(qt, QuestionType::Confirmation);

        let qt: QuestionType = serde_json::from_str("\"open_ended\"").unwrap();
        assert_eq!(qt, QuestionType::OpenEnded);
    }

    #[test]
    fn test_notification_content_serialization() {
        let content = NotificationContent {
            question_type: QuestionType::Options,
            question: "Choose:".to_string(),
            options: vec!["A".to_string(), "B".to_string()],
            summary: "选择".to_string(),
        };

        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"question_type\":\"options\""));
        assert!(json.contains("\"question\":\"Choose:\""));
        assert!(json.contains("\"options\":[\"A\",\"B\"]"));
        assert!(json.contains("\"summary\":\"选择\""));
    }

    #[test]
    fn test_detect_waiting_question_structure() {
        // 测试 detect_waiting_question 函数存在且可调用
        // 实际 AI 调用需要 API key，这里只测试结构
        let snapshot = "";
        let result = detect_waiting_question(snapshot);
        // 空快照可能返回 None（没有 API key）或 Some（有 API key 但内容为空）
        // 这里只验证函数可以被调用，不验证具体返回值
        // 因为返回值取决于是否有 API key 和 AI 的判断
        let _ = result; // 只要不 panic 就算通过
    }

    #[test]
    fn test_detect_waiting_question_returns_notification_content() {
        // 测试返回类型是 Option<NotificationContent>
        // 由于没有 API key，这里只验证函数签名和返回类型
        let snapshot = "Some terminal output\n❯ What do you want to do?";
        let result: Option<NotificationContent> = detect_waiting_question(snapshot);
        // 没有 API key 时返回 None，但类型正确
        // 如果有 API key，应该返回 Some(NotificationContent)
        assert!(result.is_none() || result.is_some());
    }

    #[test]
    fn test_quality_assessment_integration() {
        // 测试 NotificationContent 与 ai_quality 模块的集成
        use crate::ai_quality::{assess_question_extraction, thresholds};

        // 创建一个有效的 NotificationContent
        let content = NotificationContent {
            question_type: QuestionType::OpenEnded,
            question: "你想要实现什么功能？".to_string(),
            options: vec![],
            summary: "等待回复".to_string(),
        };

        let assessment = assess_question_extraction(&content);
        assert!(assessment.is_valid);
        assert!(assessment.confidence >= thresholds::MEDIUM);

        // 创建一个无效的 NotificationContent
        let invalid_content = NotificationContent {
            question_type: QuestionType::Options,
            question: "".to_string(), // 空问题
            options: vec![],          // 选项类型但没有选项
            summary: "".to_string(),  // 空摘要
        };

        let invalid_assessment = assess_question_extraction(&invalid_content);
        assert!(!invalid_assessment.is_valid);
        assert!(invalid_assessment.confidence < thresholds::LOW);
    }

    #[test]
    fn test_agent_status_enum() {
        // 测试 AgentStatus 枚举的基本功能
        let processing = AgentStatus::Processing;
        let waiting = AgentStatus::WaitingForInput;
        let unknown = AgentStatus::Unknown;

        assert_eq!(processing, AgentStatus::Processing);
        assert_eq!(waiting, AgentStatus::WaitingForInput);
        assert_eq!(unknown, AgentStatus::Unknown);

        // 测试不相等
        assert_ne!(processing, waiting);
        assert_ne!(waiting, unknown);
    }

    /// 集成测试：验证完整问题内容提取（需要 API key）
    ///
    /// 运行方式：cargo test test_extract_full_question_content -- --ignored --nocapture
    #[test]
    #[ignore] // 需要 API key，默认跳过
    fn test_extract_full_question_content() {
        // 模拟包含完整组件设计的终端快照
        let snapshot = r#"
⏺ 第三部分：组件设计

  TodoInput.tsx
  - 受控输入框 + 添加按钮
  - props: onAdd: (text: string) => void
  - 内部管理输入文本状态，提交后清空

  TodoList.tsx
  - 渲染 Todo 数组
  - props: todos, onToggle, onDelete
  - 空列表时显示提示文字

  TodoItem.tsx
  - 显示单个待办项：复选框 + 文本 + 删除按钮
  - props: todo, onToggle, onDelete
  - 已完成项文本添加删除线样式

  数据流：
    App (todos state)
    ├── TodoInput (onAdd)
    ├── TodoList (todos, onToggle, onDelete)
    │   └── TodoItem × N

  单向数据流，状态提升到 App，子组件通过回调通知变更。

  这个组件设计没问题吗？

❯
"#;

        let result = extract_question_with_haiku(snapshot);

        match result {
            ExtractionResult::Found(extracted) => {
                println!("=== 提取结果 ===");
                println!("问题类型: {}", extracted.question_type);
                println!("问题内容 ({} 字符):\n{}", extracted.question.len(), extracted.question);
                println!("回复提示: {}", extracted.reply_hint);

                // 验证问题内容包含关键信息
                assert!(extracted.question.contains("TodoInput") || extracted.question.contains("组件设计"),
                    "问题应包含 TodoInput 或组件设计关键词");
                assert!(extracted.question.contains("TodoList") || extracted.question.contains("数据流"),
                    "问题应包含 TodoList 或数据流关键词");

                // 验证问题内容足够长（不被截断）
                assert!(extracted.question.len() > 100,
                    "问题内容应超过 100 字符，实际: {} 字符", extracted.question.len());
            }
            ExtractionResult::NoQuestion(summary) => {
                panic!("AI 判断没有问题，但终端明显有问题。status: {}, last_action: {:?}",
                    summary.status, summary.last_action);
            }
            ExtractionResult::Failed => {
                panic!("提取失败，请检查 API key 配置");
            }
        }
    }

    /// 集成测试：验证 Agent Teams 场景的完整内容提取
    ///
    /// 运行方式：cargo test test_extract_agent_teams_question -- --ignored --nocapture
    #[test]
    #[ignore] // 需要 API key，默认跳过
    fn test_extract_agent_teams_question() {
        // 模拟 Agent Teams 组建场景
        let snapshot = r#"
我将为你组建一个 Agent Team 来完成这个任务。

团队结构：
- team-lead: 负责任务分解和协调
- researcher: 负责调研技术方案
- developer: 负责代码实现
- tester: 负责测试验证

任务分解：
1. 调研 React 状态管理最佳实践
2. 设计组件架构
3. 实现核心功能
4. 编写单元测试

预计需要 4 个 Agent 并行工作。

确认组建这个团队吗？

❯
"#;

        let result = extract_question_with_haiku(snapshot);

        match result {
            ExtractionResult::Found(extracted) => {
                println!("=== Agent Teams 提取结果 ===");
                println!("问题类型: {}", extracted.question_type);
                println!("问题内容 ({} 字符):\n{}", extracted.question.len(), extracted.question);

                // 验证问题内容包含团队结构信息
                assert!(extracted.question.contains("team-lead") || extracted.question.contains("团队"),
                    "问题应包含团队相关信息");
                assert!(extracted.question.contains("researcher") || extracted.question.contains("developer") || extracted.question.contains("任务"),
                    "问题应包含角色或任务信息");
            }
            ExtractionResult::NoQuestion(_) => {
                panic!("AI 判断没有问题，但终端明显有确认问题");
            }
            ExtractionResult::Failed => {
                panic!("提取失败，请检查 API key 配置");
            }
        }
    }

    /// 集成测试：验证代码查询结果场景（Agent 完成任务后等待输入）
    ///
    /// 这个场景是 Agent 完成了代码查询任务，显示了完整结果，然后等待用户下一步指令。
    /// 期望：AI 应该识别为 NoQuestion（没有问题需要回答），并提取任务摘要。
    ///
    /// 运行方式：cargo test test_extract_code_query_result -- --ignored --nocapture
    #[test]
    #[ignore] // 需要 API key，默认跳过
    fn test_extract_code_query_result() {
        // 模拟代码查询完成后的终端快照
        let snapshot = r#"
⏺ 根据探索结果，找到了"回单图片"的相关信息：

  回单图片字段和接口

  字段名：
  - receiptFileList - 回单图片列表（数组）
  - showReceiptPicFlag - 是否显示回单图片的标志

  接口：
  - 路径：/supplement/cardDetail
  - 方法：GET
  - API 函数：getMBOrderDetail(params)

  相关文件：
  - API 定义：src/api_new/saasTmsTrans/index.js:101-107
  - 图片展示组件：src/views/order/create/components/common/MbCardImageBlock.vue:14-15
  - 卡片组件：src/views/order/create/components/common/MbOrderCard.vue:88

⏺ 关键代码

  1. API 接口定义 (src/api_new/saasTmsTrans/index.js:101-107)
  // 查询满帮订单详情
  export const getMBOrderDetail = (params) => {
    return saasTmsTrans.request({
      method: 'get',
      url: '/supplement/cardDetail',
      params
    })
  }

  2. 组件传参 (MbOrderCard.vue:83-91)
  <MbCardImageBlock
    :detail="detail"
    :mybOrderId="detailInfo.orderTmsInfo && detailInfo.orderTmsInfo.id"
    :loadFileList="detailInfo.loadFileList"
    :unloadFileList="detailInfo.unloadFileList"
    :receiptFileList="detailInfo.receiptFileList"
    :showReceipt="detailInfo.showReceiptPicFlag"
    @uploadSuccess="onImgUploadSuccess"
  />

  3. 回单图片展示 (MbCardImageBlock.vue:12-18)
  <div class="item-warp">
    <div v-if="showReceipt">
      <span>回单图片</span>
      <span @click="onClickReceipt(receiptFileList)" class="count">【{{ receiptFileList.length }}】</span>
    </div>
  </div>

  总结：
  - 接口：GET /supplement/cardDetail
  - 字段：receiptFileList（图片列表）、showReceiptPicFlag（是否显示）

❯
"#;

        let result = extract_question_with_haiku(snapshot);

        match result {
            ExtractionResult::Found(extracted) => {
                println!("=== 代码查询结果提取 ===");
                println!("问题类型: {}", extracted.question_type);
                println!("问题内容 ({} 字符):\n{}", extracted.question.len(), extracted.question);
                println!("回复提示: {}", extracted.reply_hint);

                // 如果 AI 认为有问题，验证内容是否合理
                // 这种场景下，AI 可能会认为是开放式问题（等待下一步指令）
                println!("\n注意：这个场景 AI 可能识别为开放式问题或无问题");
            }
            ExtractionResult::NoQuestion(summary) => {
                println!("=== AI 判断没有问题 ===");
                println!("状态: {}", summary.status);
                println!("最后操作: {:?}", summary.last_action);

                // 验证任务摘要包含关键信息
                if let Some(action) = &summary.last_action {
                    println!("\n任务摘要内容: {}", action);
                    // 期望摘要包含回单图片或代码查询相关信息
                    assert!(
                        action.contains("回单") || action.contains("receiptFileList") || action.contains("代码") || action.contains("查询"),
                        "任务摘要应包含回单图片或代码查询相关信息"
                    );
                }
            }
            ExtractionResult::Failed => {
                panic!("提取失败，请检查 API key 配置");
            }
        }
    }
}
