//! Embedding 模块 - 使用 embedding 模型提取问题内容
//!
//! 通过计算终端输出与预定义问题模板的语义相似度，
//! 识别并提取用户正在被询问的问题。
//!
//! 配置来源：~/.openclaw/openclaw.json 的 agents.defaults.memorySearch

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::OnceLock;
use std::time::Duration;

/// 预定义问题模板
/// 用于与终端输出行计算相似度
const QUESTION_TEMPLATES: &[&str] = &[
    // 中文问题模式
    "这个方案可以吗？",
    "你想要哪个选项？",
    "请选择一个：",
    "是否继续？",
    "确认执行吗？",
    "你的目标是什么？",
    "需要我帮你做什么？",
    "请输入：",
    "你想要哪些？",
    "这个结构看起来合适吗？",
    "你觉得怎么样？",
    "可以吗？",
    "行吗？",
    "好吗？",
    // 英文问题模式
    "Which option do you prefer?",
    "Do you want to continue?",
    "Please select one:",
    "Is this okay?",
    "What would you like to do?",
    "Enter your choice:",
    "Confirm?",
    "Does this look good?",
    "What do you think?",
    // Claude Code 特定模式
    "Write to file?",
    "Run bash command?",
    "Apply changes?",
    "Delete file?",
    "Allow this action?",
];

/// Embedding API 请求体
#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
    encoding_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
}

/// Embedding API 响应体
#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    usage: Usage,
}

/// 单个 embedding 数据
#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    #[allow(dead_code)]
    index: usize,
}

/// API 使用量
#[derive(Deserialize)]
struct Usage {
    #[allow(dead_code)]
    prompt_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

/// Embedding 客户端配置
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// API 基础 URL
    pub base_url: String,
    /// API 密钥
    pub api_key: String,
    /// 模型名称
    pub model: String,
    /// 向量维度
    pub dimensions: u32,
    /// 请求超时（毫秒）
    pub timeout_ms: u64,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            api_key: String::new(),
            model: "text-embedding-v4".to_string(),
            dimensions: 1024,
            timeout_ms: 2000,
        }
    }
}

impl EmbeddingConfig {
    /// 从 ~/.openclaw/openclaw.json 读取配置
    pub fn from_openclaw_config() -> Result<Self> {
        let config_path = dirs::home_dir()
            .ok_or_else(|| anyhow!("Cannot find home directory"))?
            .join(".openclaw/openclaw.json");

        let content = fs::read_to_string(&config_path)
            .map_err(|e| anyhow!("Cannot read openclaw config: {}", e))?;

        let config: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Cannot parse openclaw config: {}", e))?;

        let memory_search = config
            .get("agents")
            .and_then(|a| a.get("defaults"))
            .and_then(|d| d.get("memorySearch"))
            .ok_or_else(|| anyhow!("No memorySearch config found"))?;

        let base_url = memory_search
            .get("remote")
            .and_then(|r| r.get("baseUrl"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("No baseUrl in memorySearch config"))?;

        let api_key = memory_search
            .get("remote")
            .and_then(|r| r.get("apiKey"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("No apiKey in memorySearch config"))?;

        let model = memory_search
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("text-embedding-v4");

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            dimensions: 1024,
            timeout_ms: 2000,
        })
    }
}

/// Embedding 客户端
pub struct EmbeddingClient {
    client: reqwest::blocking::Client,
    config: EmbeddingConfig,
}

impl EmbeddingClient {
    /// 从配置创建客户端
    pub fn new(config: EmbeddingConfig) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| anyhow!("Cannot create HTTP client: {}", e))?;

        Ok(Self { client, config })
    }

    /// 从 openclaw 配置创建客户端
    pub fn from_config() -> Result<Self> {
        let config = EmbeddingConfig::from_openclaw_config()?;
        Self::new(config)
    }

    /// 计算文本的 embedding 向量
    /// 自动分批处理，每批最多 10 个（API 限制）
    pub fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        const BATCH_SIZE: usize = 10;
        let mut all_embeddings = Vec::with_capacity(texts.len());

        // 分批处理
        for chunk in texts.chunks(BATCH_SIZE) {
            let request = EmbeddingRequest {
                model: self.config.model.clone(),
                input: chunk.to_vec(),
                encoding_format: "float".to_string(),
                dimensions: Some(self.config.dimensions),
            };

            let response = self
                .client
                .post(format!("{}/embeddings", self.config.base_url))
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .map_err(|e| anyhow!("Embedding API request failed: {}", e))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().unwrap_or_default();
                return Err(anyhow!(
                    "Embedding API returned error {}: {}",
                    status,
                    body
                ));
            }

            let result: EmbeddingResponse = response
                .json()
                .map_err(|e| anyhow!("Cannot parse embedding response: {}", e))?;

            all_embeddings.extend(result.data.into_iter().map(|d| d.embedding));
        }

        Ok(all_embeddings)
    }
}

/// 计算两个向量的余弦相似度
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// 问题提取器
/// 使用 embedding 相似度匹配从终端输出中提取问题
pub struct QuestionExtractor {
    client: EmbeddingClient,
    template_embeddings: Vec<Vec<f32>>,
    #[allow(dead_code)]
    templates: Vec<String>,
    /// 相似度阈值（0.0-1.0）
    similarity_threshold: f32,
}

/// 全局问题提取器实例（懒加载）
static GLOBAL_EXTRACTOR: OnceLock<Option<QuestionExtractor>> = OnceLock::new();

impl QuestionExtractor {
    /// 创建新的问题提取器
    /// 会预计算所有模板的 embedding
    pub fn new() -> Result<Self> {
        Self::with_threshold(0.7)
    }

    /// 创建指定相似度阈值的问题提取器
    pub fn with_threshold(threshold: f32) -> Result<Self> {
        let client = EmbeddingClient::from_config()?;

        let templates: Vec<String> = QUESTION_TEMPLATES.iter().map(|s| s.to_string()).collect();

        // 预计算模板 embedding
        let template_embeddings = client.embed(templates.clone())?;

        Ok(Self {
            client,
            template_embeddings,
            templates,
            similarity_threshold: threshold,
        })
    }

    /// 从终端输出中提取问题
    /// 返回最相似的问题行（如果相似度超过阈值）
    pub fn extract_question(&self, terminal_output: &str) -> Result<Option<String>> {
        // 1. 过滤噪音行
        let lines: Vec<&str> = terminal_output
            .lines()
            .filter(|line| !is_noise_line(line))
            .collect();

        if lines.is_empty() {
            return Ok(None);
        }

        // 2. 取最后 10 行（API 限制）
        let last_lines: Vec<String> = lines
            .iter()
            .rev()
            .take(10)
            .rev()
            .map(|s| s.to_string())
            .collect();

        if last_lines.is_empty() {
            return Ok(None);
        }

        // 3. 计算 embedding
        let line_embeddings = self.client.embed(last_lines.clone())?;

        // 4. 找最相似的行
        let mut best_match: Option<(usize, f32)> = None;

        for (line_idx, line_emb) in line_embeddings.iter().enumerate() {
            for template_emb in &self.template_embeddings {
                let similarity = cosine_similarity(line_emb, template_emb);

                if similarity > self.similarity_threshold {
                    if best_match.is_none() || similarity > best_match.unwrap().1 {
                        best_match = Some((line_idx, similarity));
                    }
                }
            }
        }

        // 5. 返回结果
        Ok(best_match.map(|(idx, _)| last_lines[idx].clone()))
    }

    /// 获取全局问题提取器实例
    /// 懒加载，首次调用时初始化
    pub fn global() -> Option<&'static QuestionExtractor> {
        GLOBAL_EXTRACTOR
            .get_or_init(|| {
                match QuestionExtractor::new() {
                    Ok(extractor) => Some(extractor),
                    Err(e) => {
                        eprintln!("[Embedding] Failed to initialize QuestionExtractor: {}", e);
                        None
                    }
                }
            })
            .as_ref()
    }
}

/// 判断是否为噪音行
fn is_noise_line(line: &str) -> bool {
    let trimmed = line.trim();

    // 空行
    if trimmed.is_empty() {
        return true;
    }

    // 分隔线
    if trimmed
        .chars()
        .all(|c| matches!(c, '─' | '━' | '═' | '-' | '│' | '╭' | '╮' | '╰' | '╯'))
    {
        return true;
    }

    // 状态栏
    if trimmed.contains("MCPs")
        || trimmed.contains("hooks")
        || trimmed.contains("context")
        || trimmed.contains("⏱️")
        || trimmed.contains("[Opus")
        || trimmed.contains("git:(")
    {
        return true;
    }

    // 进度条
    if trimmed.contains("███") || trimmed.contains("░░░") {
        return true;
    }

    // 工具调用状态
    if trimmed.starts_with('✓') || trimmed.starts_with('◐') || trimmed.starts_with('⏺') {
        return true;
    }

    // Claude Code 思考/生成状态
    if trimmed.starts_with('✶') || trimmed.starts_with('✽')
        || trimmed.contains("Brewing") || trimmed.contains("Thinking")
        || trimmed.contains("Actioning") {
        return true;
    }

    // 单独的提示符
    if trimmed == ">" || trimmed == "❯" || trimmed == "$" {
        return true;
    }

    false
}

/// 便捷函数：使用全局提取器提取问题
/// 如果提取器未初始化或提取失败，返回 None
pub fn extract_question_with_embedding(terminal_output: &str) -> Option<String> {
    QuestionExtractor::global()
        .and_then(|extractor| extractor.extract_question(terminal_output).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_length() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_is_noise_line_empty() {
        assert!(is_noise_line(""));
        assert!(is_noise_line("   "));
    }

    #[test]
    fn test_is_noise_line_separator() {
        assert!(is_noise_line("─────────────"));
        assert!(is_noise_line("━━━━━━━━━━━━━"));
        assert!(is_noise_line("═════════════"));
        assert!(is_noise_line("-------------"));
    }

    #[test]
    fn test_is_noise_line_status_bar() {
        assert!(is_noise_line("2 MCPs | 5 hooks"));
        assert!(is_noise_line("[Opus 4.6] ███░░░░░░░ 27% | ⏱️  1h 44m"));
        assert!(is_noise_line("workspace git:(main*)"));
    }

    #[test]
    fn test_is_noise_line_progress() {
        assert!(is_noise_line("███████░░░░░░░░"));
        assert!(is_noise_line("░░░░░░░░░░░░░░░"));
    }

    #[test]
    fn test_is_noise_line_tool_status() {
        assert!(is_noise_line("✓ Skill ×1 | ✓ Bash ×1"));
        assert!(is_noise_line("◐ Running..."));
        assert!(is_noise_line("⏺ Processing"));
    }

    #[test]
    fn test_is_noise_line_prompt_only() {
        assert!(is_noise_line(">"));
        assert!(is_noise_line("❯"));
        assert!(is_noise_line("$"));
    }

    #[test]
    fn test_is_noise_line_normal_content() {
        assert!(!is_noise_line("这个方案可以吗？"));
        assert!(!is_noise_line("1. Option one"));
        assert!(!is_noise_line("Please select:"));
        assert!(!is_noise_line("Write to /tmp/test.txt?"));
    }

    #[test]
    fn test_embedding_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(
            config.base_url,
            "https://dashscope.aliyuncs.com/compatible-mode/v1"
        );
        assert_eq!(config.model, "text-embedding-v4");
        assert_eq!(config.dimensions, 1024);
        assert_eq!(config.timeout_ms, 2000);
    }

    // 集成测试（需要网络和配置）
    #[test]
    #[ignore] // 需要配置文件和网络，默认跳过
    fn test_embedding_client_from_config() {
        let client = EmbeddingClient::from_config();
        assert!(client.is_ok());
    }

    #[test]
    #[ignore] // 需要配置文件和网络，默认跳过
    fn test_embedding_client_embed() {
        let client = EmbeddingClient::from_config().unwrap();
        let embeddings = client.embed(vec!["测试文本".to_string()]).unwrap();

        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].len(), 1024);
    }

    #[test]
    #[ignore] // 需要配置文件和网络，默认跳过
    fn test_question_extractor_new() {
        let extractor = QuestionExtractor::new();
        assert!(extractor.is_ok());
    }

    #[test]
    #[ignore] // 需要配置文件和网络，默认跳过
    fn test_question_extractor_extract() {
        let extractor = QuestionExtractor::new().unwrap();

        let output = "这个方案可以吗？\n❯ ";
        let question = extractor.extract_question(output).unwrap();

        assert!(question.is_some());
        assert!(question.unwrap().contains("方案"));
    }

    #[test]
    #[ignore] // 需要配置文件和网络，默认跳过
    fn test_question_extractor_extract_with_noise() {
        let extractor = QuestionExtractor::new().unwrap();

        let output = r#"─────────────
2 MCPs | 5 hooks
这个结构看起来合适吗？
❯ "#;

        let question = extractor.extract_question(output).unwrap();

        assert!(question.is_some());
        assert!(question.unwrap().contains("结构"));
    }
}
