# Team 模块

Agent Teams 编排和管理模块，提供多 Agent 协作能力。

## 模块结构

```
src/team/
├── mod.rs           # 模块入口，re-export 常用类型
├── discovery.rs     # Team 配置发现和成员管理
├── bridge.rs        # Team 文件系统操作（创建/删除/inbox）
├── orchestrator.rs  # Agent 编排和任务分配
└── inbox_watcher.rs # Inbox 目录监控和通知触发
```

## 数据存储

Team 数据存储在 `~/.claude/teams/{team-name}/` 目录：

```
~/.claude/teams/{team-name}/
├── config.json                    # Team 配置和成员列表
└── inboxes/
    └── {member-name}.json         # 成员 inbox 消息
```

## 子模块说明

### discovery

Team 配置发现和成员管理。

```rust
use cam::team::{TeamConfig, TeamMember, discover_teams, get_team_members};

// 发现所有 Team
let teams = discover_teams();

// 获取 Team 成员
if let Some(members) = get_team_members("my-team") {
    for member in members {
        println!("{}: {}", member.name, member.agent_type);
    }
}
```

### bridge

Team 文件系统操作，负责 Team 创建/删除和 Inbox 读写。

```rust
use cam::team::{TeamBridge, InboxMessage};

let bridge = TeamBridge::new();

// 创建 Team
bridge.create_team("my-team", Some("项目描述"))?;

// 发送消息到 inbox
let msg = InboxMessage {
    from: "leader".to_string(),
    text: "开始任务".to_string(),
    summary: Some("任务分配".to_string()),
    timestamp: chrono::Utc::now(),
    color: None,
    read: false,
};
bridge.send_to_inbox("my-team", "worker", msg)?;

// 读取 inbox
let messages = bridge.read_inbox("my-team", "worker")?;
```

### orchestrator

Agent 编排和任务分配，在 Team 中启动和管理 Claude Code agents。

```rust
use cam::team::{TeamOrchestrator, SpawnResult};

let orchestrator = TeamOrchestrator::new();

// 在 Team 中启动 Agent
let result = orchestrator.spawn_agent(
    "my-team",
    "worker-1",
    "/workspace/project",
    Some("实现登录功能"),
)?;

// 获取 Team 进度
let progress = orchestrator.get_team_progress("my-team")?;
println!("活跃成员: {}/{}", progress.active_members, progress.total_members);
```

### inbox_watcher

Inbox 目录监控，检测新消息并触发通知。

```rust
use cam::team::{InboxWatcher, Urgency, NotifyDecision};

let watcher = InboxWatcher::new();

// 检查是否需要通知
match watcher.should_notify(&message) {
    NotifyDecision::Notify { urgency, summary } => {
        println!("[{:?}] {}", urgency, summary);
    }
    NotifyDecision::Silent => {
        // 静默处理
    }
}
```

## 通知优先级

| Urgency | 场景 | 行为 |
|---------|------|------|
| High | 权限请求、错误 | 立即通知 |
| Medium | 任务完成、空闲 | 通知 |
| Low | 普通消息 | 静默 |

## CLI 命令

```bash
# Team 管理
cam team-create <name>            # 创建 Team
cam team-spawn <team> <name>      # 启动 Agent
cam team-progress <team>          # 查看进度
cam team-shutdown <team>          # 关闭 Team

# 消息管理
cam team-send <team> <member> <message>  # 发送消息
cam team-inbox <team> <member>           # 查看 inbox
```
