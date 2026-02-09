# CAM Agent Teams 架构设计

## 1. 概述

### 1.1 目标和定位

CAM Agent Teams 集成的目标是实现 **Remote Team Lead** 模式：用户通过 Telegram/WhatsApp 远程管理 Claude Code Agent Teams，无需坐在电脑前。

**核心价值**：
- 纯消息式自然语言交互
- 完全托管（用户只描述任务，系统自动创建 team、分配任务、启动 agents）
- 逐个通知（每个 agent 单独发送，不批量）
- 带上下文的摘要（不是原始日志）
- 仅关键消息可见（过滤噪音）

### 1.2 与 Agent Teams 的关系

**设计原则**：
1. **不重复造轮子** - 复用 Agent Teams 的 Team/Task/Mailbox 机制
2. **补充而非替代** - CAM 作为 Agent Teams 的"通知层"和"监控层"
3. **兼容现有流程** - 不破坏 Agent Teams 原生工作方式

**CAM 独有优势**：
- Watcher Daemon 实时状态监控
- OpenClaw 通知集成（Telegram/WhatsApp）
- 终端快照获取
- 权限请求远程响应

## 2. 架构图

### 2.1 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                         用户 (Telegram/WhatsApp)                 │
└─────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                         OpenClaw Agent                           │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                    CAM MCP Tools                             ││
│  │  team_create | team_delete | team_status | inbox_read/send  ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
            ┌───────────┐   ┌───────────┐   ┌───────────┐
            │Team Bridge│   │Inbox      │   │Notification│
            │           │   │Watcher    │   │Router      │
            └───────────┘   └───────────┘   └───────────┘
                    │               │               │
                    ▼               ▼               ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Claude Code Agent Teams                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │config.json  │  │tasks/*.json │  │inboxes/*.json│              │
│  └─────────────┘  └─────────────┘  └─────────────┘              │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 数据流

#### 创建任务流程
```
用户: "帮我修复 xxx bug"
        │
        ▼
OpenClaw Agent (AI 理解意图)
        │
        ▼
team_create → Team Bridge → ~/.claude/teams/{name}/config.json
        │
        ▼
TaskCreate → ~/.claude/tasks/{name}/*.json
        │
        ▼
spawn_member → Agent Teams 启动 agents
        │
        ▼
Inbox Watcher 开始监控
```

#### 权限请求流程
```
Agent 请求权限 (permission_request)
        │
        ▼
Inbox Watcher 检测到新消息
        │
        ▼
should_notify() → HIGH urgency
        │
        ▼
Notification Router → OpenClaw system event
        │
        ▼
OpenClaw Agent (AI 处理) → Telegram/WhatsApp
        │
        ▼
用户回复: "y" / "n"
        │
        ▼
OpenClaw Agent → inbox_send → Agent inbox
        │
        ▼
Agent 继续执行
```

## 3. 核心模块

### 3.1 Team Bridge (`src/team_bridge.rs`)

负责桥接 OpenClaw 命令与 Agent Teams 文件系统。

```rust
pub struct TeamBridge {
    teams_dir: PathBuf,    // ~/.claude/teams/
    tasks_dir: PathBuf,    // ~/.claude/tasks/
}

impl TeamBridge {
    /// 创建新 Team
    pub fn create_team(&self, name: &str, description: &str, project_path: &str) -> Result<TeamConfig>;

    /// 删除 Team 及其资源
    pub fn delete_team(&self, name: &str) -> Result<()>;

    /// 添加成员到 Team
    pub fn spawn_member(&self, team: &str, member: TeamMember) -> Result<()>;

    /// 发送消息到成员 inbox
    pub fn send_to_inbox(&self, team: &str, member: &str, message: InboxMessage) -> Result<()>;

    /// 读取成员 inbox
    pub fn read_inbox(&self, team: &str, member: &str) -> Result<Vec<InboxMessage>>;

    /// 获取 Team 完整状态
    pub fn get_team_status(&self, team: &str) -> Result<TeamStatus>;
}
```

### 3.2 Inbox Watcher (`src/inbox_watcher.rs`)

监控 inbox 目录变化，触发通知。

```rust
pub struct InboxWatcher {
    team_bridge: TeamBridge,
    notifier: OpenclawNotifier,
}

impl InboxWatcher {
    /// 开始监控指定 Team
    pub async fn watch_team(&self, team: &str) -> Result<()>;

    /// 处理新消息
    fn process_new_messages(&self, team: &str, member: &str, messages: Vec<InboxMessage>) -> Result<()>;

    /// 判断是否需要通知用户
    fn should_notify(&self, message: &InboxMessage) -> NotifyDecision;
}

pub enum NotifyDecision {
    Notify { urgency: Urgency, summary: String },
    Silent,
}
```

**通知过滤规则**：

| 消息类型 | 决策 | 说明 |
|----------|------|------|
| permission_request | Notify(HIGH) | 权限请求必须通知 |
| error | Notify(HIGH) | 错误必须通知 |
| task_completed | Notify(MEDIUM) | 任务完成通知 |
| idle_notification | Silent | 普通空闲不通知 |
| shutdown_approved | Silent | 关闭确认不通知 |
| 普通消息 | Notify(LOW) | 可选通知 |

### 3.3 Notification Router 增强

在现有 `openclaw_notifier.rs` 基础上增强：

```rust
impl OpenclawNotifier {
    /// 发送 Team 相关通知
    pub fn notify_team_event(&self, event: TeamEvent) -> Result<()>;
}

pub enum TeamEvent {
    PermissionRequest { team: String, member: String, tool: String, input: Value },
    TaskCompleted { team: String, member: String, task_id: String, summary: String },
    MemberError { team: String, member: String, error: String },
    TeamCompleted { team: String, summary: String },
}
```

## 4. 新增接口

### 4.1 MCP 工具

| 工具 | 参数 | 描述 |
|------|------|------|
| `team_create` | name, description, project_path | 创建新 Team |
| `team_delete` | name | 删除 Team |
| `team_status` | name | 获取 Team 状态（成员、任务、消息） |
| `inbox_read` | team, member | 读取成员 inbox |
| `inbox_send` | team, member, message | 发送消息到成员 inbox |
| `team_pending_requests` | team? | 获取等待中的权限请求 |

### 4.2 CLI 命令

```bash
# Team 管理
cam team-create <name> --project <path> [--description <desc>]
cam team-delete <name>
cam team-status <name>

# Inbox 操作
cam inbox <team> [--member <name>]
cam inbox-send <team> <member> <message>

# 实时监控
cam team-watch <team>
```

## 5. 数据结构

### 5.1 Agent ID 映射

Agent Teams 使用 `{name}@{team}` 格式，CAM 使用 `cam-{timestamp}` 格式。

**映射策略**：CAM 不创建独立 ID，直接使用 Agent Teams 的 ID 格式。

```rust
pub struct AgentId {
    pub name: String,      // e.g., "developer"
    pub team: String,      // e.g., "my-project"
}

impl AgentId {
    pub fn to_string(&self) -> String {
        format!("{}@{}", self.name, self.team)
    }
}
```

### 5.2 Inbox 消息格式

```rust
#[derive(Serialize, Deserialize)]
pub struct InboxMessage {
    pub from: String,
    pub text: String,
    pub summary: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub color: Option<String>,
    pub read: bool,
}

// 特殊消息类型（通过 text 字段的 JSON 内容区分）
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SpecialMessage {
    #[serde(rename = "task_assignment")]
    TaskAssignment { task_id: String, subject: String },

    #[serde(rename = "idle_notification")]
    IdleNotification { idle_reason: String },

    #[serde(rename = "shutdown_approved")]
    ShutdownApproved { request_id: String },

    #[serde(rename = "permission_request")]
    PermissionRequest { tool: String, input: Value },
}
```

### 5.3 通知 Payload

```json
{
  "type": "cam_team_notification",
  "version": "1.0",
  "urgency": "HIGH",
  "event_type": "permission_request",
  "team": "my-project",
  "member": "developer",
  "project": "/path/to/project",
  "summary": "developer 请求执行 Bash: npm install",
  "event": {
    "tool": "Bash",
    "input": { "command": "npm install" }
  },
  "timestamp": "2026-02-08T00:00:00Z"
}
```

## 6. 用户交互流程

### 6.1 创建任务

```
用户: "帮我在 myapp 项目修复登录 bug"

OpenClaw Agent:
1. 解析意图 → 创建 Team 执行任务
2. team_create("myapp-login-fix", "修复登录 bug", "/path/to/myapp")
3. TaskCreate("分析登录流程", "定位 bug 原因")
4. TaskCreate("修复 bug", "实现修复方案")
5. TaskCreate("测试验证", "确保修复有效")
6. spawn_member("developer", prompt="修复登录 bug...")
7. 启动 Inbox Watcher

回复用户: "已创建 Team myapp-login-fix，developer 正在分析问题..."
```

### 6.2 权限请求

```
developer 请求执行: git commit -m "fix: login bug"

Inbox Watcher 检测到 permission_request
        │
        ▼
Notification Router → Telegram

用户收到: "🔐 myapp-login-fix/developer 请求执行:
git commit -m 'fix: login bug'
回复 y 允许，n 拒绝"

用户: "y"

OpenClaw Agent:
1. 识别为权限回复
2. inbox_send("myapp-login-fix", "developer", "y")

developer 继续执行
```

### 6.3 任务完成

```
developer 完成所有任务

Inbox Watcher 检测到 task_completed
        │
        ▼
Notification Router → Telegram

用户收到: "✅ myapp-login-fix 任务完成
- 修复了 session 过期导致的登录失败
- 已提交 commit: fix: login bug
- 建议: 部署到测试环境验证"
```

## 7. 实现计划

### 阶段 1: Team Bridge 模块 (Task #8)

**目标**: 实现 Team 创建/删除和 Inbox 读写

**文件**: `src/team_bridge.rs`

**依赖**: 无

**验收标准**:
- `cargo test team_bridge` 通过
- 能创建/删除 Team 目录
- 能读写 inbox 消息

### 阶段 2: Inbox Watcher 模块 (Task #9)

**目标**: 实现 inbox 监控和通知过滤

**文件**: `src/inbox_watcher.rs`

**依赖**: Team Bridge

**验收标准**:
- 能检测 inbox 文件变化
- 正确过滤通知（HIGH/MEDIUM/LOW）
- 集成 OpenclawNotifier

### 阶段 3: MCP 工具扩展 (Task #10)

**目标**: 添加 Team 相关 MCP 工具

**文件**: `src/mcp.rs`

**依赖**: Team Bridge

**验收标准**:
- 6 个新工具可用
- OpenClaw Agent 能调用

### 阶段 4: CLI 命令扩展 (Task #11)

**目标**: 添加 Team 相关 CLI 命令

**文件**: `src/main.rs`

**依赖**: Team Bridge

**验收标准**:
- 6 个新命令可用
- 帮助文档完整

### 阶段 5: 测试 (Task #12, #13)

**目标**: 单元测试和集成测试

**依赖**: 阶段 1-4

**验收标准**:
- 单元测试覆盖核心逻辑
- 端到端场景测试通过

### 阶段 6: UX 优化 (Task #14, #15)

**目标**: 更新 Skills 和文档

**依赖**: 阶段 5

**验收标准**:
- OpenClaw Skill 支持自然语言 Team 管理
- CLAUDE.md 文档完整

## 8. 风险和缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Inbox 文件轮询延迟 | 通知不及时 | 使用 FSEvents/inotify 实时监控 |
| Agent ID 冲突 | 消息路由错误 | 统一使用 Agent Teams ID 格式 |
| 大量 agents 通知轰炸 | 用户体验差 | 智能聚合 + 优先级过滤 |
| Team 目录残留 | 磁盘占用 | 定期清理 + team-delete 命令 |
