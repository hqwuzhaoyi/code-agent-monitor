# CAM 自动处理通知设计方案

## 概述

当前 CAM 的所有通知都需要人工处理，在 Agent Swarm 场景下（多个 Agent 并行工作）会产生大量重复性确认请求，严重影响效率。本方案设计一套自动处理机制，让 OpenClaw 智能判断哪些请求可以自动批准，哪些需要人工介入。

## 目标

- 减少 80% 的重复性人工确认
- 保持安全性，危险操作必须人工确认
- 用户对自动处理有感知（简短通知）
- Swarm 场景下不刷屏（聚合通知）

## 架构

### 整体流程

```
CAM 发送通知 → POST /hooks/agent → OpenClaw 对话
                                        ↓
                              CAM Skill 接收通知
                                        ↓
                              三层决策判断
                                        ↓
                    ┌───────────┴───────────┐
                    ↓                       ↓
              可自动处理               需人工介入
                    ↓                       ↓
        cam reply + 简短通知          正常通知用户
```

### 三层决策模型

```
命令/确认请求
    ↓
┌─────────────────────────────────────┐
│ 第一层：白名单 → 直接批准            │
└─────────────────────────────────────┘
    ↓ 不在白名单
┌─────────────────────────────────────┐
│ 第二层：黑名单 → 必须人工确认        │
└─────────────────────────────────────┘
    ↓ 不在黑名单
┌─────────────────────────────────────┐
│ 第三层：LLM 判断 → 智能决策          │
└─────────────────────────────────────┘
```

## 详细规则

### 第一层：白名单（直接批准）

低风险的只读或测试命令，可以直接自动批准。

```
# 只读命令
git status
git diff
git log
ls
pwd
which
cat
head
tail

# 测试命令
cargo test
cargo check
cargo clippy
npm test
npm run lint
npm run build
yarn test
pytest
go test
tsc --noEmit
```

**⚠️ 参数安全检查**：即使命令在白名单，如果参数包含以下敏感路径，仍需人工确认：

```
# 敏感路径黑名单
/etc/
~/.ssh/
~/.aws/
~/.config/
.env
credentials
secret
token
password
id_rsa
```

**示例**：
- `cat README.md` → ✅ 自动批准
- `cat /etc/passwd` → ⚠️ 人工确认（敏感路径）
- `ls ~/.ssh/` → ⚠️ 人工确认（敏感路径）

### 第二层：黑名单（必须人工确认）

高风险操作，无论如何都需要人工确认。

```
# 删除类命令
rm, rmdir, delete, drop, truncate

# 决策类提示（需要用户判断）
- 包含 "brainstorm" 的提示
- 包含 "选择方案"、"which approach"、"你想要" 的提示
- 包含 "是否继续" 且上下文涉及重大变更

# 生产/部署相关
deploy, push --force, production, release

# 命令链（可能隐藏危险命令）
包含 &&, ||, ;, | 的命令

# 重定向和子 shell（新增）
包含 >, >>, <, $(), `` 的命令

# 环境变量展开（新增）
包含 $VAR 形式的变量引用（无法预知展开后的值）
```

### 第三层：LLM 判断

不在白名单也不在黑名单的命令，由 LLM 分析风险后决策。

**LLM 判断 Prompt：**
```
分析以下命令的风险等级：
命令: {command}
上下文: {context}
项目路径: {project_path}

判断标准：
- LOW: 只读操作、不影响系统状态、可逆操作、/tmp/ 路径 → 自动批准
- MEDIUM: 写入操作但影响范围有限、项目内文件 → 自动批准并通知
- HIGH: 删除、覆盖、不可逆、影响生产、敏感路径 → 人工确认

**额外检查**：
1. 命令参数是否包含敏感路径？
2. 是否涉及环境变量展开？
3. 是否包含管道或重定向？
4. 上下文是否完整（能否理解命令意图）？

返回: {
  "risk": "LOW|MEDIUM|HIGH",
  "reason": "...",
  "sensitive_paths_found": [],
  "requires_human": true/false
}
```

### 重复确认机制

首次人工批准的命令，5 分钟内相同命令自动批准。

**规则：**
- 命令必须**完全相等**（包括所有参数）
- 检测到命令链符号（`&&`, `||`, `;`, `|`）时不自动批准
- 状态存储在 OpenClaw 会话中（会话结束清空）

**示例：**
```
0:00 用户批准 "cargo build --release"
0:02 再次请求 "cargo build --release" → 自动批准
0:03 请求 "cargo build" → 不自动批准（命令不同）
0:06 请求 "cargo build --release" → 不自动批准（超过 5 分钟）
```

## 通知格式

### 单 Agent 场景

每次自动处理单独发送简短通知：

```
✅ [code-agent-monitor] 已自动批准: git status
✅ [code-agent-monitor] 已自动批准: cargo test
```

### Swarm 场景（聚合通知）

分层聚合，根据 urgency 级别使用不同窗口：

```
# HIGH urgency - 不聚合，立即发送
⚠️ [dev1] 需要确认: rm -rf old/

# MEDIUM urgency - 30 秒聚合窗口
💬 [refactor-team] 2 个 Agent 已退出:
  - dev1: 任务完成
  - dev2: 任务完成

# LOW urgency - 5 分钟聚合窗口（或静默）
✅ [refactor-team] 已自动批准 5 个操作:
  - git status (dev1, dev2, dev3)
  - cargo check (dev1, dev2)
```

**聚合规则：**
- HIGH urgency：不聚合，立即发送
- MEDIUM urgency：30 秒窗口
- LOW urgency：5 分钟窗口（可配置静默）
- 按 Team 聚合
- 失败时立即单独通知，不等聚合

### 批量回复支持（新增）

用户可以一次批准多个待处理请求：

```bash
# 批准所有待处理请求
cam reply y --all

# 批准指定 agent 的请求
cam reply y --agent cam-*

# 批准所有 LOW 风险请求
cam reply y --risk low

# 查看待处理请求列表
cam pending-confirmations
```

## 实现位置

### OpenClaw CAM Skill

所有决策逻辑在 OpenClaw CAM Skill 中实现：

1. **接收通知** - 解析 webhook payload
2. **提取命令** - 从 `event_data.tool_input.command` 获取
3. **三层判断** - 白名单 → 黑名单 → LLM
4. **执行动作** - `cam reply y` 或通知用户
5. **发送简短通知** - 告知用户自动处理结果

### 为什么不放在 CAM 侧

- 灵活性：Skill 中的规则可以用自然语言调整，无需重新编译
- LLM 判断：OpenClaw 本身就是 LLM，可以直接做风险分析
- 统一入口：所有通知都经过 OpenClaw，便于统一管理

## 安全考虑

1. **白名单保守原则** - 只包含明确安全的命令
2. **参数安全检查** - 白名单命令 + 敏感路径参数 = 需人工确认（新增）
3. **黑名单优先** - 黑名单检查在 LLM 判断之前
4. **命令链检测** - 防止通过 `&&`、`|`、`>`、`$()` 等注入危险命令
5. **环境变量检测** - 包含 `$VAR` 的命令需人工确认（无法预知展开值）（新增）
6. **完整日志** - 所有自动处理记录到日志，便于审计
7. **用户可控** - 提供 `cam auto --disable` 临时关闭

## AgentExited 处理（新增）

区分正常退出和异常退出：

| 退出类型 | 判断条件 | Urgency | 行为 |
|----------|----------|---------|------|
| 正常完成 | exit code 0 | LOW | 静默或简短通知 |
| 异常退出 | exit code != 0 | HIGH | 立即通知用户 |
| 超时退出 | 超过配置时间 | MEDIUM | 通知用户 |

## 效果预估

以 3 人 Team 重构任务为例：

| 时间 | Agent | 事件 | 当前 | 自动处理后 |
|------|-------|------|------|-----------|
| 0:01 | dev1 | git status | 人工 | ✅ 自动 |
| 0:01 | dev2 | git status | 人工 | ✅ 自动 |
| 0:02 | dev1 | cat src/a.rs | 人工 | ✅ 自动 |
| 0:05 | dev1 | cargo test | 人工 | ✅ 自动 |
| 0:10 | dev1 | npm run build | 人工 | ✅ 自动 |
| 0:15 | dev1 | rm -rf old/ | 人工 | ⚠️ 人工 |
| 0:20 | dev2 | 选择方案 A 还是 B？ | 人工 | ⚠️ 人工 |

**结果：** 7 次通知 → 2 次需要人工，减少 ~70% 打扰

## 后续扩展

1. **用户自定义白名单** - 配置文件支持添加信任命令
2. **项目级信任** - 某些项目标记为完全信任
3. **学习用户偏好** - 根据历史批准记录优化判断
4. **自动处理统计** - `cam auto-stats` 查看自动处理效果
5. **离开模式** - 用户可设置"我离开了，自动批准 LOW/MEDIUM"（新增）
6. **信任度渐进** - 新用户手动确认，熟练用户逐步放权（新增）

## 与现有代码的关系

### CAM 侧（Rust）

现有 `src/notification/summarizer.rs` 的风险评估逻辑作为**参考**，但不直接用于自动批准决策：

- CAM 侧的 `assess_bash_risk()` 用于通知格式化（显示风险等级）
- 自动批准决策在 OpenClaw Skill 中实现（更灵活）

### OpenClaw Skill 侧

`skills/cam-notify/SKILL.md` 需要同步更新，包含：

1. 三层决策模型
2. 参数安全检查规则
3. 聚合通知格式
4. 批量回复命令
