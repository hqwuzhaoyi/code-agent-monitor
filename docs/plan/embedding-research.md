# Embedding API 研究报告

## 1. 配置信息

从 `~/.openclaw/openclaw.json` 中提取的 memorySearch 配置：

```json
{
  "memorySearch": {
    "provider": "openai",
    "remote": {
      "baseUrl": "https://dashscope.aliyuncs.com/compatible-mode/v1/",
      "apiKey": "sk-ebb20a0c581e4cbaa49299dcccd26ed7"
    },
    "model": "text-embedding-v4"
  }
}
```

### 关键参数

| 参数 | 值 |
|------|-----|
| Provider | openai (OpenAI 兼容模式) |
| Base URL | `https://dashscope.aliyuncs.com/compatible-mode/v1/` |
| Model | text-embedding-v4 |
| 向量维度 | 1024 (默认), 可选 64-2048 |
| 最大行数 | 10 |
| 单行最大 Token | 8192 |

## 2. API 调用方式

### 2.1 HTTP 接口

**Endpoint:**
```
POST https://dashscope.aliyuncs.com/compatible-mode/v1/embeddings
```

**Headers:**
```
Authorization: Bearer <API_KEY>
Content-Type: application/json
```

**Request Body:**
```json
{
  "model": "text-embedding-v4",
  "input": "要计算 embedding 的文本",
  "encoding_format": "float",
  "dimensions": 1024
}
```

**Response:**
```json
{
  "data": [
    {
      "embedding": [0.0023064255, -0.009327292, ...],
      "index": 0,
      "object": "embedding"
    }
  ],
  "model": "text-embedding-v4",
  "object": "list",
  "usage": {"prompt_tokens": 23, "total_tokens": 23},
  "id": "f62c2ae7-0906-9758-ab34-47c5764f07e2"
}
```

### 2.2 cURL 示例

```bash
curl --location 'https://dashscope.aliyuncs.com/compatible-mode/v1/embeddings' \
  --header "Authorization: Bearer $DASHSCOPE_API_KEY" \
  --header 'Content-Type: application/json' \
  --data '{
    "model": "text-embedding-v4",
    "input": "衣服的质量杠杠的，很漂亮",
    "encoding_format": "float",
    "dimensions": 1024
  }'
```

### 2.3 批量输入

API 支持批量输入（最多 10 行）：

```json
{
  "model": "text-embedding-v4",
  "input": [
    "第一行文本",
    "第二行文本",
    "第三行文本"
  ],
  "encoding_format": "float"
}
```

## 3. 问题提取设计方案

### 3.1 核心思路

使用 embedding 相似度匹配来识别终端输出中的问题行：

1. **预定义问题模板** - 创建一组代表"问题"的模板文本
2. **计算终端行 embedding** - 对终端输出的每一行计算 embedding
3. **相似度匹配** - 找出与问题模板最相似的行
4. **阈值过滤** - 只返回相似度超过阈值的行

### 3.2 预定义问题模板

```rust
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

    // 英文问题模式
    "Which option do you prefer?",
    "Do you want to continue?",
    "Please select one:",
    "Is this okay?",
    "What would you like to do?",
    "Enter your choice:",
    "Confirm?",

    // Claude Code 特定模式
    "Write to file?",
    "Run bash command?",
    "Apply changes?",
    "Delete file?",
    "Allow this action?",
];
```

### 3.3 相似度计算

使用余弦相似度：

```rust
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b)
}
```

### 3.4 提取流程

```
终端输出 (多行)
    │
    ▼
过滤噪音行 (状态栏、分隔线等)
    │
    ▼
取最后 N 行 (避免 API 限制)
    │
    ▼
批量计算 embedding
    │
    ▼
与问题模板计算相似度
    │
    ▼
返回最相似的行 (阈值 > 0.7)
```

## 4. Rust 实现建议

### 4.1 依赖

```toml
[dependencies]
reqwest = { version = "0.11", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
```

### 4.2 数据结构

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
    encoding_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    model: String,
    usage: Usage,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: u32,
    total_tokens: u32,
}
```

### 4.3 API 客户端

```rust
pub struct EmbeddingClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl EmbeddingClient {
    /// 从 openclaw.json 配置创建客户端
    pub fn from_config() -> Result<Self> {
        let config_path = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("No home dir"))?
            .join(".openclaw/openclaw.json");

        let content = std::fs::read_to_string(&config_path)?;
        let config: serde_json::Value = serde_json::from_str(&content)?;

        let memory_search = config
            .get("agents")
            .and_then(|a| a.get("defaults"))
            .and_then(|d| d.get("memorySearch"))
            .ok_or_else(|| anyhow::anyhow!("No memorySearch config"))?;

        let base_url = memory_search
            .get("remote")
            .and_then(|r| r.get("baseUrl"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No baseUrl"))?;

        let api_key = memory_search
            .get("remote")
            .and_then(|r| r.get("apiKey"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No apiKey"))?;

        let model = memory_search
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("text-embedding-v4");

        Ok(Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        })
    }

    /// 计算文本的 embedding
    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let request = EmbeddingRequest {
            model: self.model.clone(),
            input: texts,
            encoding_format: "float".to_string(),
            dimensions: Some(1024),
        };

        let response = self.client
            .post(format!("{}/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let result: EmbeddingResponse = response.json().await?;

        Ok(result.data.into_iter().map(|d| d.embedding).collect())
    }
}
```

### 4.4 问题提取器

```rust
pub struct QuestionExtractor {
    client: EmbeddingClient,
    template_embeddings: Vec<Vec<f32>>,
    templates: Vec<String>,
}

impl QuestionExtractor {
    /// 创建提取器并预计算模板 embedding
    pub async fn new() -> Result<Self> {
        let client = EmbeddingClient::from_config()?;

        let templates: Vec<String> = QUESTION_TEMPLATES
            .iter()
            .map(|s| s.to_string())
            .collect();

        let template_embeddings = client.embed(templates.clone()).await?;

        Ok(Self {
            client,
            template_embeddings,
            templates,
        })
    }

    /// 从终端输出中提取问题
    pub async fn extract_question(&self, terminal_output: &str) -> Result<Option<String>> {
        // 1. 过滤噪音行
        let lines: Vec<&str> = terminal_output
            .lines()
            .filter(|line| !self.is_noise_line(line))
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

        // 3. 计算 embedding
        let line_embeddings = self.client.embed(last_lines.clone()).await?;

        // 4. 找最相似的行
        let mut best_match: Option<(usize, f32)> = None;

        for (line_idx, line_emb) in line_embeddings.iter().enumerate() {
            for template_emb in &self.template_embeddings {
                let similarity = cosine_similarity(line_emb, template_emb);

                if similarity > 0.7 {
                    if best_match.is_none() || similarity > best_match.unwrap().1 {
                        best_match = Some((line_idx, similarity));
                    }
                }
            }
        }

        // 5. 返回结果
        Ok(best_match.map(|(idx, _)| last_lines[idx].clone()))
    }

    fn is_noise_line(&self, line: &str) -> bool {
        let trimmed = line.trim();

        // 空行
        if trimmed.is_empty() {
            return true;
        }

        // 分隔线
        if trimmed.chars().all(|c| matches!(c, '─' | '━' | '═' | '-' | '─')) {
            return true;
        }

        // 状态栏
        if trimmed.contains("MCPs") || trimmed.contains("hooks") || trimmed.contains("context") {
            return true;
        }

        // 进度条
        if trimmed.contains("███") || trimmed.contains("░░░") {
            return true;
        }

        false
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}
```

## 5. 集成到现有流程

### 5.1 优先级策略

```
终端输出
    │
    ▼
┌─────────────────────────────────────┐
│ 1. 硬编码模式匹配 (最快)            │
│    - [Y/n], [是/否] 等确认模式      │
│    - 1. 2. 3. 等编号选项            │
│    - : 或 ： 结尾的输入提示         │
└─────────────────────────────────────┘
    │ 匹配失败
    ▼
┌─────────────────────────────────────┐
│ 2. Embedding 相似度匹配 (快速)      │  <-- 新增
│    - 预计算模板 embedding           │
│    - 批量计算终端行 embedding       │
│    - 余弦相似度 > 0.7               │
└─────────────────────────────────────┘
    │ 匹配失败
    ▼
┌─────────────────────────────────────┐
│ 3. AI 提取 (最慢，5秒超时)          │
│    - 调用 openclaw agent            │
│    - 返回结构化 JSON                │
└─────────────────────────────────────┘
    │ 超时或失败
    ▼
┌─────────────────────────────────────┐
│ 4. 回退：显示原始快照               │
└─────────────────────────────────────┘
```

### 5.2 修改 `openclaw_notifier.rs`

在 `extract_question_with_ai` 之前添加 embedding 提取：

```rust
impl OpenclawNotifier {
    /// 使用 embedding 提取问题（新增）
    async fn extract_question_with_embedding(&self, terminal_snapshot: &str) -> Option<String> {
        // 懒加载 QuestionExtractor
        static EXTRACTOR: OnceCell<QuestionExtractor> = OnceCell::new();

        let extractor = EXTRACTOR.get_or_init(|| {
            tokio::runtime::Runtime::new()
                .ok()
                .and_then(|rt| rt.block_on(QuestionExtractor::new()).ok())
        })?;

        extractor.extract_question(terminal_snapshot).await.ok()?
    }

    /// 格式化通知事件（修改）
    fn format_notification(...) -> String {
        // ... 现有代码 ...

        // 在 AI 提取之前尝试 embedding 提取
        if let Some(question) = self.extract_question_with_embedding(snap).await {
            return format!(
                "⏸️ {} 等待输入\n\n{}\n\n回复内容",
                project_name, question
            );
        }

        // 回退到 AI 提取
        if let Some((question_type, question, reply_hint)) = self.extract_question_with_ai(snap) {
            // ...
        }
    }
}
```

## 6. 性能考虑

### 6.1 延迟

| 方法 | 预期延迟 |
|------|---------|
| 硬编码模式匹配 | < 1ms |
| Embedding API 调用 | 100-500ms |
| AI 提取 | 2-5s |

### 6.2 优化建议

1. **模板 embedding 缓存** - 启动时预计算，存储在内存中
2. **批量请求** - 一次请求计算多行 embedding
3. **异步执行** - 不阻塞主线程
4. **超时控制** - embedding 请求设置 2 秒超时

### 6.3 成本

- text-embedding-v4: 0.0005 元/千 tokens
- 每次提取约 100-500 tokens
- 成本约 0.00005-0.00025 元/次

## 7. 测试计划

### 7.1 单元测试

```rust
#[tokio::test]
async fn test_embedding_client() {
    let client = EmbeddingClient::from_config().unwrap();
    let embeddings = client.embed(vec!["测试文本".to_string()]).await.unwrap();

    assert_eq!(embeddings.len(), 1);
    assert_eq!(embeddings[0].len(), 1024);
}

#[tokio::test]
async fn test_question_extraction() {
    let extractor = QuestionExtractor::new().await.unwrap();

    let output = "这个方案可以吗？\n❯ ";
    let question = extractor.extract_question(output).await.unwrap();

    assert!(question.is_some());
    assert!(question.unwrap().contains("方案"));
}
```

### 7.2 集成测试

```bash
# 测试 embedding API 连通性
curl -s 'https://dashscope.aliyuncs.com/compatible-mode/v1/embeddings' \
  -H "Authorization: Bearer sk-ebb20a0c581e4cbaa49299dcccd26ed7" \
  -H 'Content-Type: application/json' \
  -d '{"model": "text-embedding-v4", "input": "测试"}' | jq '.data[0].embedding | length'
# 预期输出: 1024
```

## 8. 总结

### 优势

1. **比 AI 提取快** - 100-500ms vs 2-5s
2. **成本低** - 0.0005 元/千 tokens
3. **语义理解** - 能识别变体问题（如"可以吗？" vs "行吗？"）
4. **多语言支持** - text-embedding-v4 支持 100+ 语言

### 局限

1. **需要网络请求** - 比硬编码模式匹配慢
2. **需要预定义模板** - 无法处理完全新颖的问题格式
3. **阈值调优** - 需要实验确定最佳相似度阈值

### 建议

将 embedding 作为硬编码模式匹配和 AI 提取之间的中间层，在保持响应速度的同时提高问题识别的准确性。
