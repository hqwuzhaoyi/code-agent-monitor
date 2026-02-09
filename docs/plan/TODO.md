# CAM 2.0 待实现计划

## 已完成 ✅

### P0 - 核心修复
- [x] 修复通知去重问题（改用 system event）
- [x] 更新插件二进制
- [x] 添加 ToolUse 事件支持
- [x] 结构化 JSON payload 格式
- [x] 快捷回复机制 (y/n/1/2/3)

### P1 - Agent Teams 集成
- [x] Team 发现功能 (`cam teams`)
- [x] Team 成员查询 (`cam team-members <team>`)
- [x] Task List 集成 (`cam tasks <team>`)
- [x] 新增 `team_discovery.rs` 和 `task_list.rs` 模块

### P2 - 通知系统架构
- [x] AI 处理层设计（通过 `openclaw system event` 发送结构化 payload）
- [x] Urgency 分级路由（HIGH/MEDIUM → channel，LOW → 静默）
- [x] Channel 自动检测（telegram > whatsapp > discord > slack > signal）
- [x] `--dry-run` 调试支持

### P3 - UX 改进
- [x] 更新 cam-notify skill
- [x] 更新 code-agent-monitor skill
- [x] 移除 `[CAM]` 前缀，简化消息格式
- [x] 用项目名替代 agent_id 显示

---

## 待实现 📋

### P4 - Mailbox 集成（Agent Teams 通信）
- [ ] 读取 `~/.claude/teams/{team}/inboxes/` 目录
- [ ] 发送消息给 teammates
- [ ] MCP 工具: `mailbox_read`, `mailbox_send`
- [ ] CLI 命令: `cam inbox <team>`, `cam send <team> <agent> <message>`

### P5 - Remote Lead 模式
- [ ] 通过 Telegram 充当 team lead
- [ ] 创建/管理 Agent Teams
- [ ] 分配任务、协调工作
- [ ] 查看 team 进度和状态

### P6 - 交互模式优化（参考 interaction-patterns.md）
- [ ] 对话状态机（IDLE → RUNNING → WAITING → RUNNING → IDLE）
- [ ] 上下文记忆（current_agent, pending_confirmations）
- [ ] 智能进度查询（"看看进度" → 状态摘要）
- [ ] 多 agent 场景优化（列出等待中的 agent 让用户选择）

### P7 - 通知智能汇总（参考 notification-ux-analysis.md）
- [ ] 权限请求：AI 评估风险等级，简化命令描述
- [ ] 错误通知：AI 分析错误原因，提供修复建议
- [ ] 完成通知：AI 总结完成的工作，标注成功/失败
- [ ] 等待输入：AI 解析等待原因，提供建议回复

### P8 - 高级功能
- [ ] 跨机器 agent 同步
- [ ] 批量操作（合并同类通知）
- [ ] 智能权限管理（项目级信任规则）
- [ ] 任务上下文记录（启动时记录初始 prompt，完成时引用）

---

## 已知问题 🐛

| 问题 | 描述 | 优先级 |
|------|------|--------|
| 终端快照过长 | 快照可能包含大量无关输出，影响消息可读性 | P2 |
| 多 agent 回复歧义 | 用户回复 "y" 时，系统不知道回复哪个 agent | P1 |
| 错误信息技术性强 | 原始错误输出对非技术用户不友好 | P2 |
| 任务完成信息不足 | 不知道完成了什么任务，是否成功 | P2 |

---

## 技术债务 🔧

| 项目 | 描述 | 建议 |
|------|------|------|
| 通知格式不统一 | `permission_request` 和 `notification(permission_prompt)` 格式不一致 | 统一为单一格式 |
| 硬编码 urgency 分类 | urgency 分类逻辑分散在多处 | 集中到配置文件 |
| 缺少单元测试 | `team_discovery.rs` 和 `task_list.rs` 缺少测试 | 添加测试覆盖 |
| MCP 工具文档 | 新增的 teams/tasks 命令缺少 MCP 工具暴露 | 添加 MCP 工具定义 |

---

## 研究文档索引

| 文档 | 内容 |
|------|------|
| `ai-processing-layer.md` | AI 处理层架构设计，payload 格式，gateway wake 机制 |
| `interaction-patterns.md` | 用户交互模式研究，对话状态机，快捷回复处理逻辑 |
| `notification-ux-analysis.md` | 通知 UX 分析，消息格式对比，改进方案 |

---

## 版本规划

| 版本 | 目标 | 状态 |
|------|------|------|
| v1.0 | 基础通知系统 + Agent Teams 集成 | ✅ 已完成 |
| v1.1 | Mailbox 集成 + Remote Lead 模式 | 📋 待实现 |
| v2.0 | 智能通知汇总 + 交互优化 | 📋 待实现 |
