# Codex 集成验证报告

测试日期: 2026-02-25
验证者: codex-integration-validator
状态: 等待 E2E 测试结果

## 验证目标

验证 ReAct 提取器在 Codex 场景下的表现：
1. 上下文扩展机制
2. fingerprint 去重
3. 消息类型识别

## Codex 特性说明

根据 `findings/codex-format.md` 调研，Codex 与 Claude Code 有以下差异：

| 特性 | Codex | Claude Code |
|------|-------|-------------|
| TUI 框架 | ratatui (Rust) | Ink (React/Node) |
| 状态指示 | StatusIndicatorWidget | 文本动画 |
| 权限请求 | ApprovalOverlay | 内联提示 |
| 消息分隔 | FinalMessageSeparator | 无明确分隔 |
| 动画 | 36帧 ASCII 动画 | 文本 spinner |

### Codex 状态指示器

```
[动画] Working... 1m 23s  (esc to interrupt)
 └ 详情文本（最多3行，超出显示省略号）
```

### Codex 权限请求格式

```
[原因说明（如有）]
[权限规则 - 青色显示]
$ command args...
```

快捷键: `y` (批准), `n` (拒绝), `a` (本次会话批准), `p` (修改策略)

---

## 1. 上下文扩展验证

### 1.1 默认配置

| 参数 | 值 |
|------|-----|
| context_sizes | [80, 150, 300, 500, 800] |
| max_iterations | 5 |
| timeout_ms | 10000 |

### 1.2 Codex 特定考虑

由于 Codex 使用 ratatui 全屏 TUI：
- 终端快照可能包含更多 UI 元素
- 状态指示器位于 composer 上方
- 消息边界由 `FinalMessageSeparator` 标记

### 1.3 验证场景

| 场景 | 预期行数 | 实际行数 | 扩展次数 | 状态 |
|------|---------|---------|---------|------|
| 简单选择题 | 80 | - | - | 待测试 |
| 带上下文的选择题 | 150-300 | - | - | 待测试 |
| 命令执行请求 | 80-150 | - | - | 待测试 |
| 文件修改请求 | 150-300 | - | - | 待测试 |
| 复杂多轮对话 | 500-800 | - | - | 待测试 |

### 1.4 验证标准

- [ ] 80 行足够提取简单问题
- [ ] 上下文不完整时正确返回 `NeedMoreContext`
- [ ] 扩展后能成功提取完整问题
- [ ] 不超过 max_iterations 次迭代
- [ ] 正确处理 Codex TUI 元素

---

## 2. Fingerprint 去重验证

### 2.1 相同问题测试

| 问题表述 | 预期 fingerprint | 实际 fingerprint | 匹配 |
|---------|-----------------|-----------------|------|
| "项目用途是什么？A) 学习 B) 作品集" | project-purpose-* | - | 待测试 |
| "这个项目的主要用途？A) 学习项目 B) 作品集展示" | project-purpose-* | - | 待测试 |

### 2.2 不同问题测试

| 问题 1 | 问题 2 | fingerprint 1 | fingerprint 2 | 不同 |
|--------|--------|--------------|--------------|------|
| 项目用途 | 技术栈选择 | - | - | 待测试 |
| 确认删除 | 确认创建 | - | - | 待测试 |

### 2.3 Codex 权限请求 fingerprint

| 命令 | 预期 fingerprint | 实际 fingerprint | 状态 |
|------|-----------------|-----------------|------|
| `ls -la` | exec-ls-la | - | 待测试 |
| `cargo test` | exec-cargo-test | - | 待测试 |
| `rm -rf /tmp/test` | exec-rm-rf-tmp | - | 待测试 |

### 2.4 验证标准

- [ ] 语义相同的问题生成相同 fingerprint
- [ ] 语义不同的问题生成不同 fingerprint
- [ ] fingerprint 格式为英文短横线连接
- [ ] 权限请求生成合理的 fingerprint

---

## 3. 消息类型识别验证

### 3.1 Choice 类型

| 终端内容 | 预期类型 | 实际类型 | 状态 |
|---------|---------|---------|------|
| A) 选项一 B) 选项二 | Choice | - | 待测试 |
| 1. 选项一 2. 选项二 | Choice | - | 待测试 |

### 3.2 Confirmation 类型

| 终端内容 | 预期类型 | 实际类型 | 状态 |
|---------|---------|---------|------|
| [Y/n] | Confirmation | - | 待测试 |
| (yes/no) | Confirmation | - | 待测试 |
| Allow? [y/N] | Confirmation | - | 待测试 |
| Codex 权限请求 (y/n/a/p) | Confirmation | - | 待测试 |

### 3.3 OpenEnded 类型

| 终端内容 | 预期类型 | 实际类型 | 状态 |
|---------|---------|---------|------|
| 请描述你的需求 | OpenEnded | - | 待测试 |
| 你想要什么功能？ | OpenEnded | - | 待测试 |

### 3.4 验证标准

- [ ] 选择题正确识别为 Choice
- [ ] 确认题正确识别为 Confirmation
- [ ] 开放式问题正确识别为 OpenEnded
- [ ] Codex 权限请求正确识别为 Confirmation
- [ ] 识别准确率 > 90%

---

## 4. 状态检测验证

### 4.1 Processing 状态 (Codex 特定)

Codex 使用 `StatusIndicatorWidget` 显示处理状态：

| 终端内容 | 预期状态 | 实际状态 | 状态 |
|---------|---------|---------|------|
| Working... 1m 23s | Processing | - | 待测试 |
| 36帧 ASCII 动画 | Processing | - | 待测试 |
| (esc to interrupt) | Processing | - | 待测试 |

### 4.2 Idle 状态

| 终端内容 | 预期状态 | 实际状态 | 状态 |
|---------|---------|---------|------|
| 无 StatusIndicatorWidget | Idle | - | 待测试 |
| 显示输入 composer | Idle | - | 待测试 |

### 4.3 ApprovalOverlay 状态

| 终端内容 | 预期状态 | 实际状态 | 状态 |
|---------|---------|---------|------|
| $ command args... | WaitingForInput | - | 待测试 |
| 青色权限规则文本 | WaitingForInput | - | 待测试 |

### 4.4 验证标准

- [ ] Processing 状态正确跳过通知
- [ ] Idle 状态正确返回 None
- [ ] ApprovalOverlay 正确识别为等待输入
- [ ] 不误判 Codex UI 元素为 Processing

---

## 5. Codex 特定测试场景

### 5.1 命令执行请求 (Exec)

```
[原因说明]
[权限规则]
$ cargo test --all
```

验证点：
- [ ] 正确提取命令内容
- [ ] 正确识别为 Confirmation 类型
- [ ] fingerprint 包含命令关键词

### 5.2 文件修改请求 (ApplyPatch)

```
[原因说明]
[Diff 摘要]
```

验证点：
- [ ] 正确提取修改摘要
- [ ] 正确识别消息类型
- [ ] fingerprint 反映修改内容

### 5.3 MCP 请求 (McpElicitation)

```
[服务器名称]
[消息内容]
```

验证点：
- [ ] 正确提取 MCP 请求内容
- [ ] 正确识别消息类型

---

## 6. 依赖状态

### 6.1 阻塞任务

| 任务 ID | 任务名称 | 状态 |
|---------|---------|------|
| #3 | 实现 cam start 命令 | in_progress |
| #7 | 设计 E2E 测试方案 | in_progress |
| #9 | 执行 Codex E2E 测试 | pending (blocked) |

### 6.2 等待文件

- `findings/codex-e2e-result.md` - 待 codex-e2e-tester 生成

---

## 7. 验证方法

### 7.1 数据来源

1. 从 `findings/codex-e2e-result.md` 读取测试数据
2. 分析 AI 提取结果
3. 对比预期值和实际值

### 7.2 验证脚本

```bash
# 手动触发验证
cam watch-trigger --agent-id <test-session> --force

# 查看提取结果
tail -f ~/.config/code-agent-monitor/hook.log | grep -E "(fingerprint|message_type|context_complete)"
```

### 7.3 Codex 启动命令

```bash
# 交互模式（适合 tmux 监控）
codex -C <workdir> --no-alt-screen "<prompt>"

# 非交互模式（适合脚本）
codex exec -C <workdir> "<prompt>"
```

---

## 8. 结论

**状态**: 等待 E2E 测试结果

待 `findings/codex-e2e-result.md` 生成后，将填充上述验证表格并给出最终结论。

### 预期关注点

基于 Codex 与 Claude Code 的差异，重点关注：
1. Codex TUI 元素是否干扰提取
2. StatusIndicatorWidget 是否正确识别为 Processing
3. ApprovalOverlay 权限请求是否正确提取
4. 36帧 ASCII 动画是否被误判

---

## 版本历史

| 版本 | 日期 | 变更 |
|------|------|------|
| 0.1 | 2026-02-25 | 创建验证框架，等待测试数据 |
