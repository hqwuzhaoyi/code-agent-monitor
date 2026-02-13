# CAM 文件结构评审报告

## 1. 当前结构概览

```
src/
├── lib.rs                      (43 行)   - 库入口，模块导出
├── main.rs                     (1193 行) - CLI 入口，命令处理
│
├── # 核心功能模块
├── agent.rs                    (794 行)  - Agent 管理（注册、状态、启动）
├── agent_watcher.rs            (941 行)  - Agent 状态监控
├── process.rs                  (未统计)  - 进程扫描
├── session.rs                  (408 行)  - 会话管理
├── tmux.rs                     (未统计)  - tmux 操作封装
│
├── # 通知相关（分散）
├── notify.rs                   (未统计)  - 旧版通知模块
├── openclaw_notifier.rs        (1266 行) - OpenClaw 通知门面
├── notification_summarizer.rs  (661 行)  - 通知摘要生成
├── throttle.rs                 (353 行)  - 通知节流
│
├── # AI 相关
├── anthropic.rs                (1374 行) - Haiku API 客户端
├── ai_quality.rs               (未统计)  - AI 质量评估
│
├── # 解析器
├── jsonl_parser.rs             (493 行)  - JSONL 事件解析
├── input_detector.rs           (未统计)  - 输入等待检测
├── terminal_utils.rs           (未统计)  - 终端工具函数
│
├── # 状态管理
├── conversation_state.rs       (614 行)  - 对话状态管理
├── task_list.rs                (未统计)  - 任务列表
│
├── # 服务
├── mcp.rs                      (1829 行) - MCP Server 实现
├── watcher_daemon.rs           (未统计)  - 后台监控守护进程
│
├── notification/               - 通知抽象层（新架构）
│   ├── mod.rs                  (41 行)   - 模块导出
│   ├── channel.rs              (未统计)  - Channel trait
│   ├── dispatcher.rs           (未统计)  - 通知分发器
│   ├── builder.rs              (未统计)  - 构建器
│   ├── urgency.rs              (未统计)  - 紧急程度
│   ├── payload.rs              (453 行)  - Payload 构建
│   ├── event.rs                (442 行)  - 事件定义
│   ├── formatter.rs            (1250 行) - 消息格式化
│   ├── deduplicator.rs         (666 行)  - 去重器
│   ├── terminal_cleaner.rs     (未统计)  - 终端清理
│   └── channels/               - 具体渠道实现
│       ├── mod.rs
│       ├── telegram.rs
│       ├── whatsapp.rs
│       ├── dashboard.rs
│       └── openclaw_message.rs
│
└── team/                       - Agent Teams 模块
    ├── mod.rs                  (28 行)   - 模块导出
    ├── discovery.rs            (未统计)  - Team 发现
    ├── bridge.rs               (793 行)  - 文件系统操作
    ├── orchestrator.rs         (771 行)  - Agent 编排
    └── inbox_watcher.rs        (517 行)  - Inbox 监控
```

## 2. 问题清单（按严重程度排序）

### 严重问题 (High)

#### H1. 文件过大需要拆分

| 文件 | 行数 | 问题 |
|------|------|------|
| `mcp.rs` | 1829 | 包含所有 MCP 工具实现，职责过多 |
| `anthropic.rs` | 1374 | 混合了 API 客户端、问题提取、状态检测等多个功能 |
| `openclaw_notifier.rs` | 1266 | 虽然是门面模式，但仍包含过多实现细节 |
| `notification/formatter.rs` | 1250 | 格式化逻辑过于复杂 |
| `main.rs` | 1193 | CLI 命令处理全部在一个文件 |

#### H2. 通知功能分散混乱

通知相关代码分散在多个位置，职责边界不清：

- `notify.rs` - 旧版通知模块（似乎已废弃但仍存在）
- `openclaw_notifier.rs` - OpenClaw 特定实现
- `notification/` - 新的抽象层
- `notification_summarizer.rs` - 摘要生成（为何不在 notification/ 下？）
- `throttle.rs` - 节流逻辑（为何不在 notification/ 下？）
- `agent_watcher.rs` - 也包含通知相关逻辑

### 中等问题 (Medium)

#### M1. 命名不一致

| 问题 | 示例 |
|------|------|
| 下划线 vs 驼峰 | `agent_watcher.rs` vs `AgentWatcher` |
| 单复数不一致 | `notification/` (单数) vs `channels/` (复数) |
| 缩写不统一 | `mcp.rs` (缩写) vs `openclaw_notifier.rs` (全称) |

#### M2. 模块层级不合理

- `notification_summarizer.rs` 应该在 `notification/` 目录下
- `throttle.rs` 应该在 `notification/` 目录下
- `ai_quality.rs` 和 `anthropic.rs` 应该组成 `ai/` 子模块

#### M3. 根目录文件过多

根目录有 22 个 `.rs` 文件，难以快速理解项目结构。建议按功能域分组。

### 轻微问题 (Low)

#### L1. 缺少子模块划分

- `agent.rs` (794 行) 可以拆分为 `agent/manager.rs`, `agent/record.rs`, `agent/types.rs`
- `conversation_state.rs` (614 行) 可以拆分

#### L2. 旧代码未清理

- `notify.rs` 似乎是旧版实现，与 `notification/` 功能重叠

## 3. 重构建议

### 建议 1: 拆分 mcp.rs（优先级：高）

```
src/mcp/
├── mod.rs              - 模块导出和 McpServer 结构
├── types.rs            - McpRequest, McpResponse, McpError
├── tools/
│   ├── mod.rs          - 工具注册
│   ├── agent.rs        - agent_* 工具
│   ├── session.rs      - session_* 工具
│   ├── team.rs         - team_* 工具
│   └── task.rs         - task_* 工具
└── handlers.rs         - 请求处理逻辑
```

### 建议 2: 整合通知模块（优先级：高）

```
src/notification/
├── mod.rs
├── channel.rs
├── dispatcher.rs
├── builder.rs
├── urgency.rs
├── payload.rs
├── event.rs
├── formatter.rs
├── deduplicator.rs
├── terminal_cleaner.rs
├── summarizer.rs       ← 从 notification_summarizer.rs 移入
├── throttle.rs         ← 从根目录移入
├── channels/
│   └── ...
└── legacy/             ← 旧代码隔离
    └── notify.rs       ← 从根目录移入，标记废弃
```

同时删除或重构 `openclaw_notifier.rs`，将其功能整合到 `notification/` 中。

### 建议 3: 拆分 anthropic.rs（优先级：中）

```
src/ai/
├── mod.rs              - 模块导出
├── client.rs           - AnthropicClient, 配置读取
├── extraction.rs       - 问题提取、通知内容提取
├── status.rs           - Agent 状态检测
├── quality.rs          ← 从 ai_quality.rs 移入
└── types.rs            - NotificationContent, QuestionType 等
```

### 建议 4: 拆分 main.rs（优先级：中）

```
src/cli/
├── mod.rs              - Cli 结构和 Commands 枚举
├── commands/
│   ├── mod.rs
│   ├── list.rs         - list, info 命令
│   ├── session.rs      - sessions, resume, logs 命令
│   ├── agent.rs        - kill, watch 命令
│   ├── team.rs         - team-* 命令
│   ├── notify.rs       - notify 命令
│   └── daemon.rs       - watch-daemon, serve 命令
└── output.rs           - 输出格式化（JSON/表格）

src/main.rs             - 仅保留入口点
```

### 建议 5: 整理根目录（优先级：低）

最终目标结构：

```
src/
├── lib.rs
├── main.rs
│
├── cli/                - CLI 命令处理
├── mcp/                - MCP Server
├── ai/                 - AI 相关（Anthropic API）
├── notification/       - 通知系统
├── team/               - Agent Teams
│
├── agent/              - Agent 管理
│   ├── mod.rs
│   ├── manager.rs
│   ├── watcher.rs      ← 从 agent_watcher.rs
│   └── types.rs
│
├── session/            - 会话管理
│   ├── mod.rs
│   ├── manager.rs
│   └── state.rs        ← 从 conversation_state.rs
│
├── parser/             - 解析器
│   ├── mod.rs
│   ├── jsonl.rs        ← 从 jsonl_parser.rs
│   └── input.rs        ← 从 input_detector.rs
│
├── infra/              - 基础设施
│   ├── mod.rs
│   ├── process.rs
│   ├── tmux.rs
│   └── terminal.rs     ← 从 terminal_utils.rs
│
└── task/               - 任务管理
    └── mod.rs          ← 从 task_list.rs
```

## 4. 实施优先级

| 阶段 | 任务 | 影响范围 | 风险 |
|------|------|----------|------|
| 1 | 整合通知模块 | 高 | 中 |
| 2 | 拆分 mcp.rs | 中 | 低 |
| 3 | 拆分 anthropic.rs | 中 | 低 |
| 4 | 拆分 main.rs | 低 | 低 |
| 5 | 整理根目录 | 高 | 高 |

建议采用渐进式重构，每次只移动一个模块，确保测试通过后再继续。

## 5. 总结

当前项目存在以下主要问题：

1. **大文件问题**：5 个文件超过 1000 行，最大的 `mcp.rs` 达 1829 行
2. **通知功能分散**：相关代码分布在 6+ 个不同位置
3. **根目录过于扁平**：22 个文件堆积在 src/ 根目录
4. **新旧代码混杂**：`notify.rs` 与 `notification/` 功能重叠

建议优先整合通知模块，这是当前最混乱的部分，整合后可以显著提升代码可维护性。
