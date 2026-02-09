# Claude Code Terminal Output Format Analysis

Date: 2026-02-09

## Overview

This document analyzes the actual terminal output format of Claude Code agents to improve notification content extraction.

## Sample 1: Active Conversation with Options

**Agent**: `cam-1770600296`
**Project**: `/Users/admin/workspace`

### Terminal Snapshot

```
  技术栈偏好？

  1. Vite + React + TypeScript - 现代标准配置，快速启动，类型安全（推荐）
  2. Create React App + TypeScript - 传统方案，配置较重
  3. Vite + React + JavaScript - 无类型检查，更轻量

❯ 1

⏺ 好的，Vite + React + TypeScript。

  样式方案偏好？

  1. Tailwind CSS - 原子化 CSS，快速开发，无需写 CSS 文件（推荐）
  2. CSS Modules - 传统方案，样式隔离，需要写 CSS
  3. styled-components - CSS-in-JS，组件化样式
  4. 纯 CSS - 最简单，全局样式

❯ 1

⏺ 好的，Tailwind CSS。

  需要哪些核心功能？（可多选）

  1. 基础功能 - 添加、完成、删除任务
  2. 基础 + 编辑 - 上述 + 修改任务内容
  3. 基础 + 筛选 - 上述 + 按状态筛选（全部/待办/已完成）
  4. 全部 - 添加、完成、删除、编辑、筛选

❯ 1

⏺ 好的，保持最简单 - 添加、完成、删除。

  我现在对需求有了清晰的理解，让我分段呈现设计方案。

  ---
  设计方案 - 第一部分：项目结构

  react-todo/
  ├── src/
  │   ├── components/
  │   │   ├── TodoInput.tsx    # 输入框组件
  │   │   ├── TodoItem.tsx     # 单个任务项
  │   │   └── TodoList.tsx     # 任务列表容器
  │   ├── hooks/
  │   │   └── useTodos.ts      # 任务状态管理 + localStorage
  │   ├── types/
  │   │   └── todo.ts          # Todo 类型定义
  │   ├── App.tsx              # 主应用
  │   ├── main.tsx             # 入口
  │   └── index.css            # Tailwind 入口
  ├── index.html
  ├── package.json
  ├── tailwind.config.js
  ├── tsconfig.json
  └── vite.config.ts

  设计思路：
  - 组件职责单一：输入、展示、列表各自独立
  - 状态集中在自定义 Hook useTodos 中管理
  - 类型定义独立文件，便于复用

  这部分结构看起来合适吗？

────────────────────────────────────────────────────────────────────────────────
❯ 
────────────────────────────────────────────────────────────────────────────────
  [Opus 4.6] ███░░░░░░░ 27% | ⏱️  1h 6m
  workspace git:(main*)
  2 MCPs | 5 hooks
  ✓ Skill ×1 | ✓ Bash ×1
```

### Structure Analysis

1. **Question Block**: Ends with `？` (Chinese question mark)
2. **Options Format**: Numbered list `1.`, `2.`, `3.`, etc.
3. **User Input Marker**: `❯` followed by user's choice
4. **Agent Response Marker**: `⏺` followed by acknowledgment
5. **Section Divider**: `---` line
6. **Code Block**: Directory tree with `├──`, `│`, `└──` characters
7. **Status Bar** (bottom):
   - Separator line: `────────────────────────────────────────`
   - Prompt: `❯ ` (empty, waiting for input)
   - Another separator line
   - Model info: `[Opus 4.6] ███░░░░░░░ 27% | ⏱️  1h 6m`
   - Project info: `workspace git:(main*)`
   - MCP/hooks info: `2 MCPs | 5 hooks`
   - Tool usage: `✓ Skill ×1 | ✓ Bash ×1`

---

## Sample 2: Welcome Screen (Idle State)

**Agent**: `cam-1770569033`
**Project**: `/private/tmp`

### Terminal Snapshot

```
╭─── Claude Code v2.1.33 ──────────────────────────────────────────────────────╮
│                                  │ Tips for getting started                  │
│           Welcome back!          │ Run /init to create a CLAUDE.md file wit… │
│                                  │ ───────────────────────────────────────── │
│                                  │ Recent activity                           │
│              ▐▛███▜▌             │ No recent activity                        │
│             ▝▜█████▛▘            │                                           │
│               ▘▘ ▝▝              │                                           │
│   Opus 4.6 · API Usage Billing   │                                           │
│           /private/tmp           │                                           │
╰──────────────────────────────────────────────────────────────────────────────╯

  Welcome to Opus 4.6

────────────────────────────────────────────────────────────────────────────────
❯ Try "fix lint errors"
────────────────────────────────────────────────────────────────────────────────
  [Opus 4.6] ██░░░░░░░░ 23%
  tmp
  2 MCPs | 5 hooks
```

### Structure Analysis

1. **Welcome Box**: Bordered with `╭`, `╮`, `│`, `╰`, `╯` characters
2. **ASCII Art Logo**: Claude logo using block characters
3. **Status Bar** (bottom):
   - Separator line
   - Prompt with placeholder: `❯ Try "fix lint errors"`
   - Separator line
   - Model info: `[Opus 4.6] ██░░░░░░░░ 23%`
   - Project name: `tmp`
   - MCP/hooks info: `2 MCPs | 5 hooks`

---

## Key Patterns Identified

### Noise Lines (Should be filtered)

| Pattern | Description |
|---------|-------------|
| `^────+$` | Horizontal separator lines |
| `^\s*\[Opus.*\].*%` | Model and context usage bar |
| `^\s*\d+ MCPs \| \d+ hooks` | MCP/hooks count |
| `^\s*✓.*×\d+` | Tool usage summary |
| `^\s*[a-z-]+ git:\(.*\)` | Git branch info |
| `^╭.*╮$`, `^│.*│$`, `^╰.*╯$` | Welcome box borders |
| `^\s*▐▛.*▜▌` | ASCII art logo |

### Content Markers

| Pattern | Meaning |
|---------|---------|
| `❯ ` | User input prompt (empty = waiting for input) |
| `⏺ ` | Agent response start |
| `^\s*\d+\.\s+` | Numbered option |
| `？$` or `?$` | Question ending |
| `---` | Section divider |

### Waiting for Input Detection

The agent is waiting for input when:
1. There's a line matching `^❯\s*$` (empty prompt)
2. OR there's a line matching `^❯ Try ".*"$` (placeholder prompt)

### Option Group Detection

Options are presented as:
```
  问题内容？

  1. 选项一 - 描述
  2. 选项二 - 描述
  3. 选项三 - 描述
```

Pattern: Lines starting with `^\s*\d+\.\s+` after a question line.

### Code Block Detection

Directory trees use:
- `├──` for middle items
- `└──` for last items
- `│` for vertical lines
- Indentation with spaces

---

## Recommendations for Notification Extraction

### 1. Filter Status Bar

Remove the last 5-6 lines that contain:
- Separator lines (`────`)
- Model info (`[Opus 4.6]`)
- Project/git info
- MCP/hooks info
- Tool usage summary

### 2. Extract Last Question + Options

1. Find the last question (line ending with `？` or `?`)
2. Collect subsequent numbered options until:
   - Another `❯` line (user input)
   - A `⏺` line (agent response)
   - End of content

### 3. Handle Multi-Round Conversations

When there are multiple Q&A rounds, only extract the LAST unanswered question:
- Skip questions that already have `❯ <number>` responses
- Only include the final question block

### 4. Preserve Code Blocks

When the content includes code/directory structure:
- Keep the tree structure intact
- Use monospace formatting in notifications

---

## Example Extraction

From Sample 1, the notification should extract:

**Question**: 这部分结构看起来合适吗？

**Context** (optional, truncated):
```
设计方案 - 第一部分：项目结构

react-todo/
├── src/
│   ├── components/
...
```

**Notification Format**:
```
⏸️ workspace 等待确认

这部分结构看起来合适吗？

回复 y/n `cam-1770600296`
```

---

## Similar Cases (Industry Research)

Date: 2026-02-09

This section documents how other notification systems handle context extraction, message truncation, and structured content.

### 1. Telegram Bot Message Limits

**Platform Limits**:
- Telegram message limit: **4096 characters**
- Slack attachment field limit: **2048 characters** (text field: **4000 characters**)
- Discord message limit: **2000 characters**

**Key Insight**: Different message fields have different limits. Using the correct field type can significantly increase available space.

### 2. ccbot (Telegram-Claude Code Bridge)

**Source**: [github.com/six-ddc/ccbot](https://github.com/six-ddc/ccbot)

**Key Design Principles**:

1. **No Message Truncation at Parse Layer**
   - Historical messages are always kept in full
   - Truncation only happens at the send layer
   - `split_message` splits by Telegram's 4096-character limit

2. **Message Splitting Strategy**
   - Real-time messages get `[1/N]` text suffixes
   - History pages use inline keyboard navigation
   - Expandable quotes for long content

3. **Message Merging**
   - Consecutive content messages are merged to reduce API calls
   - Merging stops when combined length exceeds **3800 characters** (buffer for pagination)
   - `tool_use` breaks the merge chain (sent separately)
   - `tool_result` edits into the `tool_use` message (prevents order confusion)

4. **MarkdownV2 with Fallback**
   - All messages use `parse_mode="MarkdownV2"`
   - Auto fallback to plain text on parse failure
   - Uses `telegramify-markdown` library for conversion

**Applicable to CAM**:
- Consider message splitting with `[1/N]` suffixes for long notifications
- Implement fallback to plain text when markdown parsing fails
- Use 3800 character threshold as safe limit (not 4096)

### 3. Prometheus Alertmanager Telegram Integration

**Source**: [github.com/prometheus/alertmanager/issues/2923](https://github.com/prometheus/alertmanager/issues/2923)

**Problem**: Message truncation breaks HTML/Markdown tags, causing parse errors.

**Solution Template** (by KonstantinNosov):

```go
// Pseudo-code for smart truncation
1. Build full notification message with all alerts
2. If too big, fall back to less verbose format (no labels)
3. If still too big, remove alerts as WHOLE UNITS (not mid-tag)
4. Add truncation notice to message
```

**Key Techniques**:

1. **Graceful Degradation**
   - Full format → Reduced format → Minimal format
   - Each level removes less critical information

2. **Atomic Truncation**
   - Never truncate in the middle of a tag/block
   - Remove complete units (alerts, sections) to maintain valid markup

3. **Truncation Notice**
   - Always inform user when content is truncated
   - Example: `Message is truncated to fit telegram API limit`

4. **Safe Length Threshold**
   - Use **3900 characters** as max (not 4096)
   - Leaves buffer for truncation notice and formatting

**Applicable to CAM**:
- Implement atomic truncation (remove complete option groups, not partial)
- Add truncation indicator when content is cut
- Use 3900 character threshold for Telegram

### 4. Context Truncation Strategies (Chatbots)

**Source**: [Tencent Cloud Techpedia](https://www.tencentcloud.com/techpedia/127693)

**Strategies**:

1. **Summarization**
   - Condense earlier parts into shorter snippets
   - Preserve key points (user goals, decisions)
   - Example: "You previously reported a login issue and tried resetting your password."

2. **Session-Based Memory**
   - Store critical information in external storage
   - Retrieve on demand, not in every message

3. **Clarification Requests**
   - When context is lost, ask clarifying questions
   - Example: "Earlier you mentioned a delivery issue—would you like me to check its status again?"

**Applicable to CAM**:
- For multi-round conversations, summarize earlier rounds
- Only include the LAST unanswered question in notifications
- Store conversation context separately, reference by ID if needed

### 5. GitHub Copilot CLI Context Management

**Source**: [github.com/github/copilot-cli/issues/828](https://github.com/github/copilot-cli/issues/828)

**Requested Features**:
- Smart context summarization
- Context folding (collapse less relevant sections)
- Recursive summarization for long histories

**Key Insight**: Users want intelligent context management, not just truncation.

### 6. LibreChat Context Compaction

**Source**: [github.com/danny-avila/LibreChat/discussions/7484](https://github.com/danny-avila/LibreChat/discussions/7484)

**Principles**:
- **Token-level precision**: Every token should be relevant
- **Recursive summarization**: Summarize in chunks, not just truncate
- **Preserve essential information**: User goals, decisions, key context

---

## Recommendations Based on Research

### 1. Message Length Strategy

| Platform | Safe Limit | Strategy |
|----------|------------|----------|
| Telegram | 3900 chars | Split with `[1/N]` suffix |
| Slack | 3800 chars | Use attachment text field |
| Discord | 1900 chars | Split or use embeds |

### 2. Truncation Hierarchy

When content exceeds limit, remove in this order:
1. Status bar / noise lines (already implemented)
2. Historical Q&A rounds (keep only last question)
3. Code block details (show first/last lines with `...`)
4. Option descriptions (keep only option numbers and titles)
5. Add truncation notice

### 3. Atomic Truncation Rules

Never truncate:
- In the middle of a code block
- In the middle of an option item
- In the middle of a markdown/HTML tag

Always truncate:
- At section boundaries (`---`)
- At complete option groups
- At complete Q&A rounds

### 4. Fallback Chain

```
Full Content
    ↓ (if > 3900 chars)
Remove historical rounds
    ↓ (if still > 3900 chars)
Truncate code blocks with "..."
    ↓ (if still > 3900 chars)
Remove option descriptions
    ↓ (if still > 3900 chars)
Split into multiple messages [1/N]
```

### 5. Implementation Checklist

- [ ] Add safe length threshold constant (3900 for Telegram)
- [ ] Implement atomic truncation (complete units only)
- [ ] Add truncation notice when content is cut
- [ ] Implement message splitting with `[1/N]` suffix
- [ ] Add markdown fallback to plain text
- [ ] Test with various content lengths
