# ReAct 提取器 E2E 测试方案

## 概述

验证 CAM 的 ReAct 提取器在真实 AI 编码工具场景下的表现：从 `cam start` 启动 Agent，到提取器正确识别问题并生成通知。

---

## 1. 测试环境准备

### 1.1 依赖

| 依赖 | 用途 | 检查命令 |
|------|------|---------|
| tmux | 终端会话管理 | `tmux -V` |
| cam (release build) | CAM CLI | `cam --version` |
| Claude Code | AI 编码工具 | `claude --version` |
| Codex CLI | AI 编码工具 | `codex --version` |
| Anthropic API Key | Haiku 提取器 | 检查 config.json |
| jq | JSON 解析 | `jq --version` |

### 1.2 环境变量 & 配置

```bash
# 确保 Haiku API 可用
cat ~/.config/code-agent-monitor/config.json
# 需要包含:
# {
#   "anthropic_api_key": "sk-xxx",
#   "anthropic_base_url": "..."
# }

# 确保 cam 二进制是最新的
cargo build --release
cp target/release/cam plugins/cam/bin/cam
```

### 1.3 测试目录

```bash
# 创建隔离的测试工作区
export E2E_TEST_DIR="/tmp/cam-e2e-test-$(date +%s)"
mkdir -p "$E2E_TEST_DIR"
cd "$E2E_TEST_DIR"
git init
echo '# Test Project' > README.md
git add . && git commit -m "init"
```

### 1.4 清理旧状态

```bash
# 清理去重状态，避免干扰
rm -f ~/.config/code-agent-monitor/dedup_state.json
# 清理旧 agent 记录
cam list  # 确认无残留 agent
```

---

## 2. 测试场景

### 场景 TC-01: Claude Code + brainstorm 选择题

**目标**: 验证提取器能从 Claude Code brainstorm 流程中提取选择题。

**触发 prompt**: `brainstorm 创建笔记 web app`

**预期终端输出模式**:
```
⏺ Skill(brainstorming)
  ⎿  Successfully loaded skill

⏺ 让我先了解一下当前项目的上下文。

⏺ 第一个问题：项目的主要用途是什么？

  A) 个人学习项目
  B) 作品集展示
  C) 实际使用的工具
  D) 其他

❯
```

**验证点**:
| 字段 | 预期值 |
|------|--------|
| message_type | `choice` |
| has_question | `true` |
| context_complete | `true` |
| fingerprint | 包含 `note`/`web-app`/`purpose` 等关键词 |
| message | 包含问题文本和选项 A/B/C/D |
| options | 4 个选项 |

---

### 场景 TC-02: Claude Code + brainstorm 确认题

**目标**: 验证提取器能识别 brainstorm 流程中的确认题。

**前置**: TC-01 完成后回复选项，等待下一个问题。

**预期终端输出模式**:
```
⏺ 好的，这是一个学习项目。

  你想使用 React 还是 Vue？

  A) React
  B) Vue
  C) 其他

❯
```

**验证点**:
| 字段 | 预期值 |
|------|--------|
| message_type | `choice` |
| fingerprint | 与 TC-01 不同（新问题） |
| context_complete | `true` |

---

### 场景 TC-03: Claude Code + 权限请求

**目标**: 验证提取器能识别权限请求（Bash 命令确认）。

**触发**: brainstorm 完成后 Agent 开始执行，遇到需要确认的命令。

**预期终端输出模式**:
```
⏺ Bash(npm init -y && npm install react react-dom)

  Allow? [Y/n]
```

**验证点**:
| 字段 | 预期值 |
|------|--------|
| message_type | `confirmation` |
| has_question | `true` |
| message | 包含命令内容 |
| fingerprint | 包含 `npm-init`/`install` 等关键词 |

---

### 场景 TC-04: Codex + brainstorm 选择题

**目标**: 验证提取器对 Codex TUI 格式的兼容性。

**触发 prompt**: `brainstorm 创建笔记 web app`

**预期终端输出模式** (ratatui TUI):
```
[动画] Working... 0s  (esc to interrupt)

--- Agent 输出 ---
项目的主要用途是什么？

1. 个人学习项目
2. 作品集展示
3. 实际使用的工具
```

**验证点**:
| 字段 | 预期值 |
|------|--------|
| message_type | `choice` |
| has_question | `true` |
| context_complete | `true` |
| fingerprint | 与 TC-01 语义相近（同一问题不同工具） |

---

### 场景 TC-05: Claude Code + 开放式问题

**目标**: 验证提取器能识别开放式问题。

**触发**: Agent 在执行过程中提出开放式问题。

**预期终端输出模式**:
```
⏺ 我注意到项目中已经有一些组件了。

  你希望笔记应用支持哪些核心功能？请描述你的需求。

❯
```

**验证点**:
| 字段 | 预期值 |
|------|--------|
| message_type | `open_ended` |
| has_question | `true` |
| message | 包含问题文本 |

---

### 场景 TC-06: Agent 处理中状态（不应触发通知）

**目标**: 验证提取器在 Agent 处理中时不会误触发通知。

**触发**: Agent 正在执行任务，终端显示处理动画。

**预期终端输出模式**:
```
✶ Brewing…
```

**验证点**:
| 字段 | 预期值 |
|------|--------|
| status | `processing` |
| 通知 | 不应发送 |

---

### 场景 TC-07: 上下文不完整 → 自动扩展

**目标**: 验证 ReAct 循环在上下文不完整时自动扩展。

**触发**: Agent 输出很长的方案说明后提问"这个方案可以吗？"

**预期行为**:
1. 80 行截取 → AI 返回 `context_complete: false`
2. 150 行截取 → AI 返回 `context_complete: true`，提取成功

**验证点**:
| 指标 | 预期值 |
|------|--------|
| 迭代次数 | > 1 |
| 最终 context_complete | `true` |
| message | 包含方案摘要和问题 |

---

### 场景 TC-08: Fingerprint 去重

**目标**: 验证相同问题不会重复发送通知。

**步骤**:
1. Agent 提出问题 → 触发通知
2. 不回复，等待 watcher 再次轮询
3. 验证第二次不发送通知（fingerprint 去重）

**验证点**:
| 指标 | 预期值 |
|------|--------|
| 第一次 | NotifyAction::Send |
| 第二次 | NotifyAction::Suppressed |
| fingerprint | 两次相同 |

---

### 场景 TC-09: Agent 空闲状态

**目标**: 验证 Agent 完成任务后空闲状态的识别。

**触发**: Agent 完成所有操作，显示完成信息。

**预期终端输出模式**:
```
⏺ 项目创建完成！你可以运行 npm start 启动开发服务器。

❯
```

**验证点**:
| 字段 | 预期值 |
|------|--------|
| message_type | `idle` |
| agent_status | `completed` |
| 通知 | 不应发送（空闲不通知） |

---

## 3. 测试步骤

### 3.1 手动测试流程（Claude Code）

```bash
# Step 1: 启动 Agent
cd "$E2E_TEST_DIR"
cam start claude -- "brainstorm 创建笔记 web app"
# 记录 agent_id，例如 cam-abc12345

# Step 2: 等待 Agent 提出第一个问题（约 10-30 秒）
# 观察 watcher 日志
cam service logs -f
# 或手动触发检测
cam watch-trigger --agent-id cam-abc12345

# Step 3: 验证提取结果
# 方法 A: 查看 watcher 日志中的提取信息
cam service logs | grep "Message extracted"
# 期望看到: fingerprint, iterations, message_type

# 方法 B: 使用 dry-run 模式验证通知 payload
cam watch-trigger --agent-id cam-abc12345 --force 2>&1 | jq .

# Step 4: 验证通知内容
# 检查 notifications.jsonl
tail -1 ~/.config/code-agent-monitor/notifications.jsonl | jq .
# 验证字段:
#   - event_type: "waiting_for_input"
#   - context.extracted_message: 包含问题和选项
#   - context.question_fingerprint: 非空

# Step 5: 回复并等待下一个问题
cam reply A --agent cam-abc12345
# 等待下一个问题，重复 Step 3-4

# Step 6: 清理
cam stop cam-abc12345
```

### 3.2 手动测试流程（Codex）

```bash
# Step 1: 启动 Codex Agent
cd "$E2E_TEST_DIR"
cam start codex -- "brainstorm 创建笔记 web app"

# Step 2-6: 同 Claude Code 流程
# 注意: Codex 使用 ratatui TUI，tmux capture 的内容格式不同
```

### 3.3 对比测试

对同一个 prompt，分别用 Claude Code 和 Codex 执行，对比：

| 对比项 | Claude Code | Codex |
|--------|-------------|-------|
| 提取成功率 | 记录 | 记录 |
| 平均迭代次数 | 记录 | 记录 |
| fingerprint 一致性 | 记录 | 记录 |
| message_type 准确性 | 记录 | 记录 |
| AI 调用延迟 | 记录 | 记录 |

---

## 4. 验证检查清单

### 4.1 ReAct 提取器输出

- [ ] `extract_message()` 返回 `Some(ExtractedMessage)` 而非 `None`
- [ ] `content` 字段包含可读的问题文本
- [ ] `content` 不包含 UI 噪音（ASCII art、进度条、状态栏）
- [ ] `content` 长度 < 500 字符

### 4.2 通知内容完整性

- [ ] 选择题：问题 + 所有选项 + 回复指引
- [ ] 确认题：操作描述 + y/n 指引
- [ ] 开放式：问题文本 + 回复指引
- [ ] 用户不看终端也能理解问题并做出决策

### 4.3 Fingerprint 正确性

- [ ] 同一问题多次提取 → 相同 fingerprint
- [ ] 不同问题 → 不同 fingerprint
- [ ] fingerprint 格式：英文短横线连接关键词
- [ ] fingerprint 不包含特殊字符或空格

### 4.4 消息类型识别

- [ ] `A) B) C)` 或 `1. 2. 3.` 格式 → `Choice`
- [ ] `[Y/n]` 或 `yes/no` 格式 → `Confirmation`
- [ ] 无选项的问题 → `OpenEnded`
- [ ] 无问题的完成状态 → `Idle`

### 4.5 上下文扩展

- [ ] 80 行不够时自动扩展到 150 行
- [ ] 扩展后 `context_complete` 变为 `true`
- [ ] 最多 5 次迭代后停止
- [ ] 日志中可见迭代过程

### 4.6 状态检测

- [ ] `✶ Brewing…` → 不触发通知
- [ ] `✻ Thinking…` → 不触发通知
- [ ] 空 `❯` 提示符 + 问题 → 触发通知
- [ ] Codex `Working...` 动画 → 不触发通知

---

## 5. 成功/失败判断标准

### 通过标准

| 指标 | 阈值 |
|------|------|
| 选择题提取成功率 | ≥ 90% |
| 确认题提取成功率 | ≥ 90% |
| 开放式问题提取成功率 | ≥ 80% |
| 处理中状态误报率 | ≤ 5% |
| fingerprint 一致性 | ≥ 95% |
| 单次提取延迟 | < 10 秒 |
| 上下文扩展成功率 | ≥ 80% |

### 失败标准

以下任一情况视为测试失败：
- Agent 处理中时触发通知（误报）
- 选择题选项丢失或截断
- fingerprint 包含非法字符
- 提取器崩溃或 panic
- AI API 调用超时率 > 20%

---

## 6. 自动化考虑

### 6.1 可自动化部分

| 步骤 | 自动化方式 | 难度 |
|------|-----------|------|
| 环境准备 | Shell 脚本 | 低 |
| 启动 Agent | `cam start` CLI | 低 |
| 等待问题出现 | 轮询 `cam watch-trigger` | 中 |
| 验证提取结果 | 解析 JSON + 断言 | 中 |
| 回复并继续 | `cam reply` CLI | 低 |
| 清理 | `cam stop` + rm | 低 |

### 6.2 难以自动化部分

| 步骤 | 原因 | 替代方案 |
|------|------|---------|
| 验证通知可读性 | 需要人工判断 | 检查字段非空 + 长度合理 |
| Agent 输出不确定性 | AI 输出不可预测 | 多次运行取统计 |
| Codex TUI 渲染 | ratatui 全屏模式 | tmux capture 验证 |

### 6.3 自动化测试脚本框架

```bash
#!/bin/bash
# scripts/e2e-react-extractor.sh

set -euo pipefail

AGENT_TYPE="${1:-claude}"
PROMPT="brainstorm 创建笔记 web app"
TIMEOUT=120  # 秒
RESULTS_FILE="/tmp/cam-e2e-results-$(date +%s).json"

echo "=== CAM ReAct Extractor E2E Test ==="
echo "Agent: $AGENT_TYPE"
echo "Prompt: $PROMPT"

# 准备测试目录
TEST_DIR="/tmp/cam-e2e-$(date +%s)"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"
git init && echo '# Test' > README.md && git add . && git commit -m "init"

# 清理去重状态
rm -f ~/.config/code-agent-monitor/dedup_state.json

# 启动 Agent
AGENT_ID=$(cam start "$AGENT_TYPE" -- "$PROMPT" 2>&1 | grep -oP 'cam-\w+')
echo "Agent started: $AGENT_ID"

# 等待问题出现
ELAPSED=0
EXTRACTED=""
while [ $ELAPSED -lt $TIMEOUT ]; do
    sleep 5
    ELAPSED=$((ELAPSED + 5))

    # 尝试触发检测
    RESULT=$(cam watch-trigger --agent-id "$AGENT_ID" --force 2>&1 || true)

    if echo "$RESULT" | jq -e '.context.extracted_message' > /dev/null 2>&1; then
        EXTRACTED="$RESULT"
        echo "Question detected after ${ELAPSED}s"
        break
    fi

    echo "Waiting... (${ELAPSED}s)"
done

if [ -z "$EXTRACTED" ]; then
    echo "FAIL: No question detected within ${TIMEOUT}s"
    cam stop "$AGENT_ID" 2>/dev/null || true
    exit 1
fi

# 验证提取结果
echo "$EXTRACTED" | jq . > "$RESULTS_FILE"

# 断言检查
MSG=$(echo "$EXTRACTED" | jq -r '.context.extracted_message // empty')
FP=$(echo "$EXTRACTED" | jq -r '.context.question_fingerprint // empty')

PASS=true

if [ -z "$MSG" ]; then
    echo "FAIL: extracted_message is empty"
    PASS=false
fi

if [ -z "$FP" ]; then
    echo "FAIL: question_fingerprint is empty"
    PASS=false
fi

if [ ${#MSG} -gt 500 ]; then
    echo "WARN: extracted_message too long (${#MSG} chars)"
fi

# 清理
cam stop "$AGENT_ID" 2>/dev/null || true
rm -rf "$TEST_DIR"

if [ "$PASS" = true ]; then
    echo "PASS: E2E test completed"
    echo "  Message: ${MSG:0:100}..."
    echo "  Fingerprint: $FP"
else
    echo "FAIL: E2E test failed"
    exit 1
fi
```

### 6.4 所需工具

| 工具 | 用途 |
|------|------|
| bash | 测试脚本 |
| jq | JSON 解析和断言 |
| tmux | 会话管理（cam 内部使用） |
| cam CLI | Agent 管理和检测触发 |

---

## 7. 测试记录模板

每次测试执行后填写：

```markdown
## 测试记录 - YYYY-MM-DD

### 环境
- cam 版本: x.x.x
- Claude Code 版本: x.x.x
- Codex 版本: x.x.x
- OS: macOS xx.x

### 结果

| 场景 | 状态 | 迭代次数 | 延迟 | 备注 |
|------|------|---------|------|------|
| TC-01 CC 选择题 | PASS/FAIL | N | Xs | |
| TC-02 CC 确认题 | PASS/FAIL | N | Xs | |
| TC-03 CC 权限请求 | PASS/FAIL | N | Xs | |
| TC-04 Codex 选择题 | PASS/FAIL | N | Xs | |
| TC-05 CC 开放式 | PASS/FAIL | N | Xs | |
| TC-06 处理中状态 | PASS/FAIL | - | - | |
| TC-07 上下文扩展 | PASS/FAIL | N | Xs | |
| TC-08 去重 | PASS/FAIL | - | - | |
| TC-09 空闲状态 | PASS/FAIL | - | - | |

### 发现的问题
1. ...

### 提取结果样本
(粘贴 JSON)
```
