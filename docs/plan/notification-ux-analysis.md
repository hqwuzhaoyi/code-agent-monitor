# CAM 通知 UX 分析

## 当前通知类型

| 类型 | Urgency | 当前格式 | 问题 | 改进建议 |
|------|---------|----------|------|----------|
| `permission_request` | HIGH | `🔐 [CAM] {agent_id} 请求权限`<br>工具: {tool_name}<br>目录: {cwd}<br>参数: {tool_input}<br>终端快照<br>请回复: 1=允许 2=允许并记住 3=拒绝 | 1. 信息过多，参数 JSON 难以快速阅读<br>2. 回复指令格式繁琐 `{agent_id} 1`<br>3. 缺少风险评估 | 1. AI 预处理：提取关键信息，评估风险等级<br>2. 简化回复：`1` `2` `3` 或 `y` `n`<br>3. 高风险命令高亮警告 |
| `notification` (idle_prompt) | MEDIUM | `⏸️ [CAM] {agent_id} 等待输入`<br>{message}<br>终端快照 | 1. message 内容不可控，可能很长<br>2. 用户不知道 agent 在做什么任务 | 1. AI 总结当前任务状态<br>2. 提供快捷操作建议 |
| `notification` (permission_prompt) | HIGH | `🔐 [CAM] {agent_id} 需要权限确认`<br>{message}<br>终端快照<br>请回复: 1=允许 2=允许并记住 3=拒绝 | 与 permission_request 重复，格式不一致 | 统一权限请求格式 |
| `session_start` | LOW | `🚀 [CAM] {agent_id} 已启动`<br>目录: {cwd} | 1. 信息过于简单<br>2. 不知道启动的任务是什么 | 1. 包含初始 prompt 摘要<br>2. 或直接不发送（当前已静默） |
| `session_end` / `stop` | MEDIUM | `✅ [CAM] {agent_id} 已停止`<br>目录: {cwd}<br>终端快照 | 1. 不知道完成了什么<br>2. 不知道是否成功 | 1. AI 总结完成的工作<br>2. 标注成功/失败状态 |
| `WaitingForInput` | HIGH | `⏸️ [CAM] {agent_id} 等待输入`<br>类型: {pattern_or_path}<br>上下文: {context}<br>终端快照 | 1. "类型" 字段含义不明<br>2. 上下文可能是原始终端输出，难以理解 | 1. AI 解析等待原因<br>2. 提供建议的回复选项 |
| `Error` | HIGH | `❌ [CAM] {agent_id} 发生错误`<br>错误信息: {context}<br>终端快照<br>请问如何处理？ | 1. 错误信息可能很长且技术性强<br>2. 用户不知道如何处理 | 1. AI 分析错误原因<br>2. 提供具体的修复建议 |
| `AgentExited` | MEDIUM | `✅ [CAM] {agent_id} 已退出`<br>项目: {pattern_or_path}<br>终端快照 | 与 stop 类似，信息不足 | 同 stop 改进建议 |

## 消息格式详细分析

### 1. permission_request - 权限请求

**当前格式示例：**
```
🔐 [CAM] cam-abc123 请求权限

工具: Bash
目录: /workspace/myapp
参数:
```json
{
  "command": "rm -rf /tmp/test"
}
```

📸 终端快照:
```
$ ls -la
total 0
drwxr-xr-x  2 user  staff  64 Feb  8 10:00 .
```

请回复:
cam-abc123 1 = 允许
cam-abc123 2 = 允许并记住
cam-abc123 3 = 拒绝
```

**问题：**
- 消息长度：~300-500 字符，在移动端阅读困难
- 参数 JSON 格式化后占用大量空间
- 回复指令需要输入完整 agent_id，容易出错
- 缺少风险评估，用户需要自己判断命令是否安全

**理想格式：**
```
🔐 cam-abc123 请求执行:
rm -rf /tmp/test

⚠️ 风险: 删除文件操作

回复 1=允许 2=记住 3=拒绝
```

### 2. WaitingForInput - 等待输入

**当前格式示例：**
```
⏸️ [CAM] cam-abc123 等待输入

类型: Confirmation
上下文: Do you want to continue? [Y/n]

📸 终端快照:
```
Building project...
✓ Compiled successfully
Do you want to continue? [Y/n]
```
```

**问题：**
- "类型: Confirmation" 对用户无意义
- 上下文和终端快照内容重复
- 用户不知道应该回复什么

**理想格式：**
```
⏸️ cam-abc123 等待确认

Claude Code 询问: 是否继续？

建议回复:
- "y" 继续
- "n" 取消
```

### 3. Error - 错误通知

**当前格式示例：**
```
❌ [CAM] cam-abc123 发生错误

错误信息:
---
error[E0433]: failed to resolve: use of undeclared crate or module `tokio`
 --> src/main.rs:1:5
  |
1 | use tokio::runtime::Runtime;
  |     ^^^^^ use of undeclared crate or module `tokio`
---

📸 终端快照:
```
$ cargo build
   Compiling myapp v0.1.0
error[E0433]: ...
```

请问如何处理？
```

**问题：**
- 错误信息原样输出，技术性强
- "请问如何处理？" 过于开放，用户不知道选项
- 缺少错误分析和建议

**理想格式：**
```
❌ cam-abc123 编译错误

缺少依赖: tokio

建议:
1. 添加依赖: cargo add tokio
2. 忽略继续
3. 停止 agent
```

### 4. stop/AgentExited - 完成通知

**当前格式示例：**
```
✅ [CAM] cam-abc123 已停止

目录: /workspace/myapp

📸 终端快照:
```
All tests passed!
Build successful.
```
```

**问题：**
- 不知道完成了什么任务
- 不知道是成功完成还是异常退出
- 终端快照可能不包含关键信息

**理想格式：**
```
✅ cam-abc123 完成

任务: 修复登录 bug
结果: 成功，所有测试通过
耗时: 5 分钟
```

## 与 coding-agent skill 对比

coding-agent skill 的通知设计理念：

| 特点 | coding-agent | CAM 当前 |
|------|--------------|----------|
| 通知触发 | 调用方自己说 | 系统自动推送 |
| 消息格式 | 简洁一次性 | 详细带快照 |
| 上下文累积 | 无（一次性） | 有（发到 agent session） |
| 用户操作 | 无需回复 | 需要回复指令 |

**coding-agent 的优势：**
1. 通知由调用方控制，避免重复
2. 消息简洁，适合移动端
3. 不累积上下文，避免去重问题

**CAM 的优势：**
1. 自动检测状态变化
2. 提供终端快照，信息完整
3. 支持交互式回复

## 改进方案

### 方案 A: AI 预处理层

在发送通知前，通过 AI 处理原始信息：

```
原始事件 → AI 处理 → 格式化消息 → 发送
```

**处理内容：**
1. 提取关键信息
2. 评估风险/紧急程度
3. 生成建议操作
4. 压缩消息长度

**优点：** 消息质量高，用户体验好
**缺点：** 增加延迟和成本

### 方案 B: 模板优化

优化现有模板，不引入 AI：

1. 简化消息格式
2. 移除冗余信息
3. 统一回复指令格式
4. 添加预定义的风险标签

**优点：** 无额外成本，响应快
**缺点：** 无法处理复杂场景

### 方案 C: 混合方案（推荐）

- HIGH urgency: AI 预处理（权限请求、错误）
- MEDIUM/LOW urgency: 模板优化（完成、启动）

**理由：**
1. HIGH urgency 需要用户决策，值得 AI 处理
2. MEDIUM/LOW 是信息通知，简单模板即可
3. 平衡成本和体验

## 具体改进建议

### 1. 统一消息头格式

```
{emoji} {agent_id} {动作}
```

移除 `[CAM]` 前缀，减少视觉噪音。

### 2. 简化回复指令

当前：`cam-abc123 1 = 允许`
改进：`回复 1=允许 2=记住 3=拒绝`

系统自动关联最近的权限请求，无需指定 agent_id。

### 3. 终端快照可选

- 默认不发送快照
- 用户可请求 `{agent_id} logs` 查看详情

### 4. 添加任务上下文

在 agent 启动时记录初始 prompt，在通知中引用：

```
✅ cam-abc123 完成

任务: "修复登录页面的 CSS 问题"
```

### 5. 错误分类和建议

预定义常见错误类型和建议操作：

| 错误类型 | 建议 |
|----------|------|
| 编译错误 | 查看错误详情 / 让 agent 修复 |
| 网络错误 | 重试 / 检查网络 |
| 权限错误 | 授权 / 跳过 |

## 下一步

1. 确定采用哪个方案
2. 设计 AI 预处理层的 prompt（如果采用方案 A/C）
3. 实现消息格式优化
4. 用户测试和迭代
