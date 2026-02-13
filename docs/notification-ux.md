# 通知系统 UX 设计

## 概述

CAM 通知系统使用 AI (Claude Haiku) 分析终端输出，提取问题内容和 Agent 状态，生成用户友好的通知消息。

## 通知场景

### 1. 问题类型通知

| 类型 | Emoji | 标签 | 示例 |
|------|-------|------|------|
| 选择题 | 📋 | 请选择 | `📋 React Todo List 请选择`<br>`这个项目的主要用途是什么？`<br>`1. 学习项目`<br>`2. 个人使用`<br>`回复数字 (1-2)` |
| 确认题 | 🔔 | 请确认 | `🔔 React Todo List 请确认`<br>`是否继续执行？`<br>`y 确认 / n 取消` |
| 开放问题 | ❓ | 有问题 | `❓ React Todo List 有问题`<br>`你想实现什么功能？`<br>`直接回复你的答案` |

### 2. 无问题场景 (NoQuestion)

当 AI 判断终端没有需要用户回答的问题时，显示任务摘要：

| 状态 | Emoji | 示例 |
|------|-------|------|
| 任务完成 | ✅ | `✅ React Todo List 已完成`<br>`创建了 TodoList 组件`<br>`回复继续` |
| 空闲等待 | 💤 | `💤 React Todo List 空闲中`<br>`最后操作：修复了登录 bug`<br>`回复继续` |
| 简洁模式 | 💤 | `💤 React Todo List 等待指令` |

### 3. 权限请求

| 风险等级 | Emoji | 示例 |
|----------|-------|------|
| 低风险 | ✅ | `✅ myproject 请求权限`<br>`Bash: npm install`<br>`📦 安装依赖，安全操作` |
| 中风险 | ⚠️ | `⚠️ myproject 请求权限`<br>`Edit: src/App.tsx`<br>`✏️ 修改文件，请确认` |
| 高风险 | 🔴 | `🔴 myproject 请求权限`<br>`Bash: rm -rf node_modules`<br>`⚠️ 删除操作，请仔细确认` |

### 4. 其他场景

| 场景 | Emoji | 示例 |
|------|-------|------|
| 会话启动 | 🚀 | `🚀 myproject 已启动` |
| 会话结束 | 🔚 | `🔚 myproject 会话结束` |
| 错误 | ❌ | `❌ myproject 出错了`<br>`API 请求超限`<br>`💡 建议：稍后重试` |

## AI 提取逻辑

### 提取字段

```json
{
  "question_type": "open|choice|confirm|none",
  "question": "问题内容",
  "options": ["选项1", "选项2"],
  "reply_hint": "回复提示",
  "agent_status": "completed|idle|waiting",
  "last_action": "最后操作摘要",
  "context_complete": true,
  "contains_ui_noise": false
}
```

### 数据结构

```rust
/// 提取的问题
pub struct ExtractedQuestion {
    pub question_type: String,  // "open", "choice", "confirm"
    pub question: String,
    pub options: Vec<String>,
    pub reply_hint: String,
}

/// 任务摘要（NoQuestion 场景）
pub struct TaskSummary {
    pub status: String,           // "completed", "idle", "waiting"
    pub last_action: Option<String>,
}

/// 提取结果
pub enum ExtractionResult {
    Found(ExtractedQuestion),
    NoQuestion(TaskSummary),
    Failed,
}
```

### 上下文扩展策略

当 AI 判断上下文不完整时，自动扩展：
- 第一次：80 行
- 第二次：150 行
- 第三次：300 行

## 相关代码

| 文件 | 功能 |
|------|------|
| `src/anthropic.rs` | AI 提取逻辑，Haiku API 调用 |
| `src/notification/formatter.rs` | 消息格式化，文案模板 |
| `src/notification/urgency.rs` | 紧急程度分类 |

## 配置

AI 提取使用 Claude Haiku 4.5，配置位置：

1. `~/.config/code-agent-monitor/config.json` (推荐)
2. 环境变量 `ANTHROPIC_API_KEY`
3. `~/.anthropic/api_key`
4. `~/.openclaw/openclaw.json`

```json
{
  "anthropic_api_key": "sk-xxx",
  "anthropic_base_url": "http://localhost:23000/"
}
```
