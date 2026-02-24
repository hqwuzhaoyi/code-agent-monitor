# Adapter 配置管理设计分析

## 概述

本文档分析 CAM 多 Agent 工具支持中 `setup_hooks()` 和 `remove_hooks()` 的实现策略，涵盖 Claude Code、Codex CLI 和 OpenCode 三种工具的配置管理。

## 各工具配置位置

| 工具 | 配置文件 | 格式 | 配置项 |
|------|---------|------|--------|
| Claude Code | `~/.claude/settings.json` 或 `.claude/settings.json` | JSON | `hooks` 数组 |
| Codex CLI | `~/.codex/config.toml` | TOML | `notify` 数组 |
| OpenCode | `~/.config/opencode/plugins/` | TypeScript | Plugin 文件 |

## 风险评估

### 1. 自动修改用户配置文件的风险

| 风险类型 | 严重程度 | 说明 |
|---------|---------|------|
| 配置损坏 | 高 | JSON/TOML 解析错误可能导致工具无法启动 |
| 数据丢失 | 高 | 覆盖用户自定义配置 |
| 格式破坏 | 中 | 注释丢失、格式变化 |
| 权限问题 | 中 | 文件权限不足导致写入失败 |
| 并发冲突 | 低 | 多进程同时修改配置 |

### 2. 各工具特定风险

#### Claude Code (`settings.json`)
- **风险**: JSON 不支持注释，但用户可能使用 JSONC
- **复杂度**: hooks 是数组，需要合并而非覆盖
- **示例配置**:
```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": ".*",
        "hooks": ["cam notify --event tool_use"]
      }
    ],
    "Stop": [
      {
        "matcher": ".*",
        "hooks": ["cam notify --event stop"]
      }
    ]
  }
}
```

#### Codex CLI (`config.toml`)
- **风险**: TOML 格式严格，语法错误会导致解析失败
- **复杂度**: `notify` 是简单数组，相对容易处理
- **示例配置**:
```toml
notify = ["cam", "codex-notify"]
```

#### OpenCode (Plugin 文件)
- **风险**: 需要创建 TypeScript 文件，可能与用户 Plugin 冲突
- **复杂度**: 最高，需要管理独立文件
- **示例 Plugin**:
```typescript
// ~/.config/opencode/plugins/cam-plugin.ts
export const CAMPlugin = async (ctx) => {
  return {
    event: async ({ event }) => {
      if (["session.idle", "permission.asked"].includes(event.type)) {
        // 调用 CAM
      }
    }
  }
}
```

## 备份策略设计

### 推荐方案: 时间戳备份 + 最近 N 份保留

```
~/.config/code-agent-monitor/backups/
├── claude/
│   ├── settings.json.2026-02-24T10-30-00.bak
│   ├── settings.json.2026-02-24T09-15-00.bak
│   └── settings.json.2026-02-23T14-20-00.bak
├── codex/
│   └── config.toml.2026-02-24T10-30-00.bak
└── opencode/
    └── cam-plugin.ts.2026-02-24T10-30-00.bak
```

### 备份实现

```rust
pub struct BackupManager {
    backup_dir: PathBuf,
    max_backups: usize,  // 默认 5
}

impl BackupManager {
    /// 创建备份
    pub fn backup(&self, tool: &str, original_path: &Path) -> Result<PathBuf> {
        let timestamp = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S");
        let filename = original_path.file_name().unwrap();
        let backup_path = self.backup_dir
            .join(tool)
            .join(format!("{}.{}.bak", filename.to_string_lossy(), timestamp));

        fs::create_dir_all(backup_path.parent().unwrap())?;
        fs::copy(original_path, &backup_path)?;

        self.cleanup_old_backups(tool)?;
        Ok(backup_path)
    }

    /// 回滚到最近备份
    pub fn rollback(&self, tool: &str, target_path: &Path) -> Result<()> {
        let latest = self.get_latest_backup(tool, target_path)?;
        fs::copy(&latest, target_path)?;
        Ok(())
    }

    /// 清理旧备份，保留最近 N 份
    fn cleanup_old_backups(&self, tool: &str) -> Result<()> {
        // 按时间排序，删除超出 max_backups 的旧文件
    }
}
```

## 配置冲突处理

### 策略 1: 合并模式（推荐）

保留用户现有配置，只添加 CAM 相关条目。

```rust
pub fn merge_claude_hooks(existing: &Value, cam_hooks: &Value) -> Value {
    let mut result = existing.clone();

    // 获取或创建 hooks 对象
    let hooks = result.get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .unwrap_or_else(|| {
            result["hooks"] = json!({});
            result["hooks"].as_object_mut().unwrap()
        });

    // 合并每个事件类型的 hooks
    for (event, cam_hook_list) in cam_hooks.as_object().unwrap() {
        if let Some(existing_list) = hooks.get_mut(event) {
            // 追加到现有列表，避免重复
            if let Some(arr) = existing_list.as_array_mut() {
                for hook in cam_hook_list.as_array().unwrap() {
                    if !arr.contains(hook) {
                        arr.push(hook.clone());
                    }
                }
            }
        } else {
            // 新增事件类型
            hooks.insert(event.clone(), cam_hook_list.clone());
        }
    }

    result
}
```

### 策略 2: 标记模式

使用特殊标记标识 CAM 管理的配置。

```json
{
  "hooks": {
    "Stop": [
      {
        "matcher": ".*",
        "hooks": ["user-custom-hook"],
        "_cam_managed": false
      },
      {
        "matcher": ".*",
        "hooks": ["cam notify --event stop"],
        "_cam_managed": true,
        "_cam_version": "0.1.0"
      }
    ]
  }
}
```

### 策略 3: 独立配置文件（OpenCode 专用）

OpenCode 的 Plugin 系统天然支持独立文件，无需合并。

```
~/.config/opencode/plugins/
├── user-plugin.ts      # 用户自定义
└── cam-plugin.ts       # CAM 管理（独立文件）
```

## 用户体验设计

### 推荐: 交互式确认 + 静默模式选项

```bash
# 交互式模式（默认）
$ cam setup claude
检测到现有配置: ~/.claude/settings.json
当前 hooks 配置:
  - Stop: ["my-custom-hook"]

CAM 将添加以下 hooks:
  - Stop: ["cam notify --event stop"]
  - PostToolUse: ["cam notify --event tool_use"]

是否继续? [y/N/d(diff)/b(backup only)]
> y

✓ 已备份到 ~/.config/code-agent-monitor/backups/claude/settings.json.2026-02-24T10-30-00.bak
✓ 已更新 ~/.claude/settings.json

# 静默模式（CI/自动化）
$ cam setup claude --yes
$ cam setup claude -y

# 仅备份，不修改
$ cam setup claude --backup-only

# 显示将要做的更改
$ cam setup claude --dry-run
```

### CLI 接口设计

```rust
#[derive(Parser)]
pub struct SetupCommand {
    /// 目标工具: claude, codex, opencode
    tool: String,

    /// 静默模式，自动确认
    #[arg(short, long)]
    yes: bool,

    /// 仅显示更改，不实际修改
    #[arg(long)]
    dry_run: bool,

    /// 仅创建备份
    #[arg(long)]
    backup_only: bool,

    /// 强制覆盖（不合并）
    #[arg(long)]
    force: bool,
}
```

## 卸载/清理策略

### `cam remove` 命令

```bash
# 移除 CAM hooks
$ cam remove claude
将移除以下 CAM hooks:
  - Stop: ["cam notify --event stop"]
  - PostToolUse: ["cam notify --event tool_use"]

用户自定义 hooks 将保留:
  - Stop: ["my-custom-hook"]

是否继续? [y/N]
> y

✓ 已移除 CAM hooks
✓ 用户配置已保留

# 完全清理（包括备份）
$ cam remove claude --purge
```

### 清理实现

```rust
pub fn remove_cam_hooks(tool: &str, config: &mut Value) -> Result<()> {
    match tool {
        "claude" => {
            // 遍历 hooks，移除包含 "cam notify" 的条目
            if let Some(hooks) = config.get_mut("hooks").and_then(|h| h.as_object_mut()) {
                for (_, hook_list) in hooks.iter_mut() {
                    if let Some(arr) = hook_list.as_array_mut() {
                        arr.retain(|h| {
                            !h.get("hooks")
                                .and_then(|hs| hs.as_array())
                                .map(|hs| hs.iter().any(|cmd|
                                    cmd.as_str().map(|s| s.contains("cam notify")).unwrap_or(false)
                                ))
                                .unwrap_or(false)
                        });
                    }
                }
            }
        }
        "codex" => {
            // 从 notify 数组移除 "cam" 相关条目
        }
        "opencode" => {
            // 删除 cam-plugin.ts 文件
        }
        _ => {}
    }
    Ok(())
}
```

## 实现建议

### 1. 分阶段实现

| 阶段 | 内容 | 优先级 |
|------|------|--------|
| Phase 1 | Claude Code setup/remove | 高 |
| Phase 2 | Codex CLI setup/remove | 中 |
| Phase 3 | OpenCode Plugin 管理 | 低 |

### 2. 核心模块结构

```
src/adapter/
├── mod.rs              # Adapter trait 定义
├── config_manager.rs   # 配置管理通用逻辑
├── backup.rs           # 备份管理
├── claude.rs           # Claude Code adapter
├── codex.rs            # Codex CLI adapter
└── opencode.rs         # OpenCode adapter
```

### 3. Adapter Trait

```rust
pub trait AgentAdapter {
    /// 工具名称
    fn name(&self) -> &str;

    /// 检测工具是否已安装
    fn is_installed(&self) -> bool;

    /// 获取配置文件路径
    fn config_path(&self) -> Option<PathBuf>;

    /// 设置 hooks
    fn setup_hooks(&self, options: &SetupOptions) -> Result<SetupResult>;

    /// 移除 hooks
    fn remove_hooks(&self, options: &RemoveOptions) -> Result<RemoveResult>;

    /// 验证配置是否正确
    fn validate_config(&self) -> Result<ValidationResult>;
}
```

## 安全考虑

1. **文件权限**: 备份文件应保持与原文件相同的权限
2. **原子写入**: 使用临时文件 + rename 确保写入原子性
3. **配置验证**: 写入前验证配置格式正确
4. **回滚机制**: 写入失败时自动回滚

```rust
pub fn safe_write_config(path: &Path, content: &str) -> Result<()> {
    // 1. 写入临时文件
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, content)?;

    // 2. 验证临时文件
    validate_config(&temp_path)?;

    // 3. 原子替换
    fs::rename(&temp_path, path)?;

    Ok(())
}
```

## 总结

| 方面 | 推荐方案 |
|------|---------|
| 备份策略 | 时间戳备份 + 保留最近 5 份 |
| 冲突处理 | 合并模式（保留用户配置） |
| 用户交互 | 交互式确认 + `--yes` 静默选项 |
| 卸载策略 | 精确移除 CAM 条目，保留用户配置 |
| 实现顺序 | Claude Code → Codex → OpenCode |

关键原则：
1. **不破坏用户配置** - 合并而非覆盖
2. **可回滚** - 始终创建备份
3. **透明** - 显示将要做的更改
4. **可选静默** - 支持自动化场景
