# 问题提取优先级策略设计

## 概述

CAM 需要从 Claude Code 终端输出中提取问题内容，用于生成简洁的通知消息。本文档定义三种提取方法的优先级、触发条件、超时处理和回退逻辑。

## 提取方法优先级

```
优先级 1: Embedding 模型（语义相似度）
    ↓ 失败/超时
优先级 2: AI 提取（LLM 分析）
    ↓ 失败/超时
优先级 3: 结构化规则（正则匹配）
    ↓ 失败
回退: 显示原始快照
```

## 方法详解

### 优先级 1: Embedding 模型

**原理**：使用预训练的 embedding 模型计算终端输出中每行与"问题模板"的语义相似度，选择最相似的行作为问题。

**触发条件**：
- 终端快照非空
- embedding 服务可用（配置开关启用）
- 快照行数 <= 50（避免大量计算）

**实现方案**：

```rust
/// Embedding 提取配置
pub struct EmbeddingConfig {
    /// 是否启用 embedding 提取
    pub enabled: bool,
    /// embedding 服务端点（本地或远程）
    pub endpoint: String,
    /// 超时时间（毫秒）
    pub timeout_ms: u64,
    /// 相似度阈值（0.0-1.0）
    pub similarity_threshold: f32,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,  // 默认禁用，需显式启用
            endpoint: "http://localhost:11434/api/embeddings".to_string(),  // Ollama 默认端点
            timeout_ms: 1000,  // 1 秒超时
            similarity_threshold: 0.75,
        }
    }
}
```

**问题模板向量**（预计算并缓存）：
```
- "请问..."
- "你想要..."
- "选择..."
- "是否..."
- "确认..."
- "What would you like..."
- "Do you want to..."
- "Please select..."
- "Continue?"
```

**算法流程**：
1. 将终端快照按行分割
2. 过滤噪音行（状态栏、分隔线等）
3. 对每行计算 embedding 向量
4. 与问题模板向量计算余弦相似度
5. 选择相似度最高且超过阈值的行
6. 如果是选择题，向前/向后扫描提取选项

**超时处理**：
- 超时时间：1000ms（可配置）
- 超时后立即回退到 AI 提取
- 记录超时日志用于调优

**回退逻辑**：
- embedding 服务不可用 → 跳过，进入 AI 提取
- 相似度低于阈值 → 跳过，进入 AI 提取
- 超时 → 跳过，进入 AI 提取

**配置开关**：
```toml
# ~/.claude-monitor/config.toml
[extraction]
embedding_enabled = true
embedding_endpoint = "http://localhost:11434/api/embeddings"
embedding_timeout_ms = 1000
embedding_similarity_threshold = 0.75
```

---

### 优先级 2: AI 提取（现有实现）

**原理**：调用 LLM 分析终端输出，提取问题类型、核心问题和回复提示。

**触发条件**：
- embedding 提取失败/跳过
- AI 提取未被禁用（`--no-ai` 标志）
- 非 dry-run 模式

**当前实现**：`extract_question_with_ai()` 函数

**超时处理**：
- 超时时间：5 秒（`AI_EXTRACT_TIMEOUT_SECS`）
- 使用 `spawn` + `try_wait` 轮询实现
- 超时后 kill 进程并回退

**回退逻辑**：
- 超时 → 进入结构化规则
- 解析失败 → 进入结构化规则
- question_type == "none" → 进入结构化规则

**配置开关**：
- CLI: `--no-ai` 禁用 AI 提取
- 代码: `with_no_ai(true)` 方法

---

### 优先级 3: 结构化规则（现有实现）

**原理**：使用正则表达式匹配已知的问题模式。

**触发条件**：
- embedding 和 AI 提取都失败/跳过
- 始终可用（无外部依赖）

**当前实现**：
| 函数 | 匹配模式 |
|------|---------|
| `is_numbered_choice()` | `^\s*[1-9]\.\s+` |
| `is_confirmation_prompt()` | `[Y]es / [N]o`, `[Y/n]`, `[是/否]` 等 |
| `is_colon_prompt()` | 以 `:` 或 `：` 结尾 |

**超时处理**：
- 无超时（纯本地计算，毫秒级）

**回退逻辑**：
- 所有模式都不匹配 → 显示原始快照

**配置开关**：
- 无（始终启用作为最终回退）

---

## 统一提取接口

```rust
/// 问题提取结果
pub struct ExtractionResult {
    /// 问题类型: "choice", "confirm", "open", "none"
    pub question_type: String,
    /// 核心问题内容
    pub question: String,
    /// 回复提示
    pub reply_hint: String,
    /// 提取方法: "embedding", "ai", "rule", "fallback"
    pub method: String,
}

/// 问题提取器
pub struct QuestionExtractor {
    embedding_config: EmbeddingConfig,
    no_ai: bool,
    dry_run: bool,
}

impl QuestionExtractor {
    /// 提取问题（按优先级尝试各方法）
    pub fn extract(&self, terminal_snapshot: &str) -> ExtractionResult {
        // 1. 尝试 embedding 提取
        if self.embedding_config.enabled {
            if let Some(result) = self.extract_with_embedding(terminal_snapshot) {
                return result;
            }
        }

        // 2. 尝试 AI 提取
        if !self.no_ai && !self.dry_run {
            if let Some(result) = self.extract_with_ai(terminal_snapshot) {
                return result;
            }
        }

        // 3. 尝试结构化规则
        if let Some(result) = self.extract_with_rules(terminal_snapshot) {
            return result;
        }

        // 4. 回退：返回原始内容
        ExtractionResult {
            question_type: "unknown".to_string(),
            question: terminal_snapshot.trim().to_string(),
            reply_hint: "回复内容".to_string(),
            method: "fallback".to_string(),
        }
    }
}
```

---

## 配置优先级

配置来源优先级（高到低）：
1. CLI 参数（`--no-ai`, `--no-embedding`）
2. 环境变量（`CAM_EMBEDDING_ENABLED`, `CAM_AI_ENABLED`）
3. 配置文件（`~/.claude-monitor/config.toml`）
4. 默认值

---

## 性能考量

| 方法 | 典型延迟 | 外部依赖 | 准确率 |
|------|---------|---------|--------|
| Embedding | 50-200ms | Ollama/API | 中高 |
| AI 提取 | 1-5s | OpenClaw Agent | 高 |
| 结构化规则 | <1ms | 无 | 中（已知模式高） |

**建议配置**：
- 本地开发：启用 embedding（Ollama 本地运行）
- 生产环境：根据延迟容忍度选择
- 低延迟场景：禁用 embedding 和 AI，仅用规则

---

## 实现计划

### Phase 1: 重构现有代码
1. 将现有提取逻辑抽取到 `QuestionExtractor` 结构体
2. 统一返回 `ExtractionResult` 类型
3. 添加 `method` 字段用于调试和监控

### Phase 2: 添加 Embedding 支持
1. 实现 `EmbeddingConfig` 配置
2. 实现 `extract_with_embedding()` 方法
3. 添加问题模板向量预计算
4. 集成 Ollama API 调用

### Phase 3: 配置和监控
1. 添加配置文件支持
2. 添加提取方法统计（用于调优阈值）
3. 添加 dry-run 输出显示使用的方法

---

## 测试策略

### 单元测试
```rust
#[test]
fn test_extraction_priority_embedding_first() {
    let extractor = QuestionExtractor::new()
        .with_embedding_enabled(true);
    // Mock embedding 服务返回高相似度
    let result = extractor.extract("你想要哪个选项？\n1. A\n2. B");
    assert_eq!(result.method, "embedding");
}

#[test]
fn test_extraction_fallback_to_ai() {
    let extractor = QuestionExtractor::new()
        .with_embedding_enabled(true);
    // Mock embedding 服务返回低相似度
    let result = extractor.extract("Some ambiguous text");
    assert_eq!(result.method, "ai");
}

#[test]
fn test_extraction_fallback_to_rules() {
    let extractor = QuestionExtractor::new()
        .with_embedding_enabled(false)
        .with_no_ai(true);
    let result = extractor.extract("Continue? [Y/n]");
    assert_eq!(result.method, "rule");
    assert_eq!(result.question_type, "confirm");
}
```

### 集成测试
- 测试 Ollama embedding API 调用
- 测试超时处理
- 测试配置文件加载

---

## 附录：Embedding 模型选择

| 模型 | 维度 | 大小 | 推荐场景 |
|------|------|------|---------|
| nomic-embed-text | 768 | 274MB | 通用，推荐 |
| mxbai-embed-large | 1024 | 670MB | 高精度 |
| all-minilm | 384 | 46MB | 低资源 |

**推荐**：`nomic-embed-text`，平衡精度和速度。

```bash
# 安装模型
ollama pull nomic-embed-text
```
