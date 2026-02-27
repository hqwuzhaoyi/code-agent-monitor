// src/cli/setup.rs
//! Setup 命令 - 自动配置 hook
//!
//! 为不同的 AI 编码工具自动配置 CAM hook。

use crate::agent::adapter::{config_manager::BackupManager, get_adapter};
use crate::agent::AgentType;
use anyhow::Result;
use clap::Args;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::str::FromStr;

/// Setup 命令参数
#[derive(Args)]
pub struct SetupArgs {
    /// Target tool: claude, codex, opencode
    pub tool: String,

    /// Skip confirmation prompt
    #[arg(short, long)]
    pub yes: bool,

    /// Show changes without applying
    #[arg(long)]
    pub dry_run: bool,
}

/// 处理 setup 命令
pub fn handle_setup(args: SetupArgs) -> Result<()> {
    let agent_type = AgentType::from_str(&args.tool)?;
    let adapter = get_adapter(&agent_type);

    let config_path = adapter
        .paths()
        .config
        .ok_or_else(|| anyhow::anyhow!("No config path for {}", args.tool))?;

    println!("Setting up CAM hooks for {}", args.tool);
    println!("Config file: {}", config_path.display());

    // 检查工具是否已安装
    if !adapter.is_installed() {
        println!(
            "⚠️  {} is not installed, but will configure anyway",
            args.tool
        );
    }

    // 生成新配置
    let new_config = generate_hook_config(&args.tool)?;

    if args.dry_run {
        println!("\n--- Changes to apply ---");
        println!("{}", new_config);
        return Ok(());
    }

    // 确认
    if !args.yes {
        print!("\nApply changes? [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    // 备份
    let backup_manager = BackupManager::new();
    if config_path.exists() {
        let backup_path = backup_manager.backup(&args.tool, &config_path)?;
        println!("✓ Backed up to {}", backup_path.display());
    }

    // 确保配置目录存在
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // 应用配置
    apply_hook_config(&args.tool, &config_path, &new_config)?;
    println!("✓ Updated {}", config_path.display());

    Ok(())
}

/// 解析 cam 二进制的绝对路径
/// 优先级: plugin 位置 > current_exe > which > fallback
fn get_cam_binary_path() -> String {
    // 1. Check plugin location (~/.claude/plugins/cam/bin/cam)
    if let Some(home) = dirs::home_dir() {
        let plugin_path = home.join(".claude/plugins/cam/bin/cam");
        if plugin_path.exists() {
            return plugin_path.to_string_lossy().to_string();
        }
    }
    // 2. current_exe
    if let Ok(exe) = std::env::current_exe() {
        return exe.to_string_lossy().to_string();
    }
    // 3. which
    if let Ok(path) = which::which("cam") {
        return path.to_string_lossy().to_string();
    }
    // 4. fallback
    "cam".to_string()
}

/// 生成 hook 配置
fn generate_hook_config(tool: &str) -> Result<String> {
    let cam_path = get_cam_binary_path();
    match tool {
        "codex" => Ok(format!(r#"notify = ["{}", "codex-notify"]"#, cam_path)),
        "claude" => {
            let events = [
                ("Notification", "notification"),
                ("PermissionRequest", "permission_request"),
                ("SessionEnd", "session_end"),
                ("SessionStart", "session_start"),
                ("Stop", "stop"),
            ];
            let mut hooks = serde_json::Map::new();
            for (event_name, event_arg) in &events {
                let command = format!(
                    "\"{}\" notify --event {} --agent-id ${{SESSION_ID:-unknown}}",
                    cam_path, event_arg
                );
                let hook_entry = serde_json::json!([
                    {
                        "matcher": "",
                        "hooks": [{"type": "command", "command": command}]
                    }
                ]);
                hooks.insert(event_name.to_string(), hook_entry);
            }
            let config = serde_json::json!({ "hooks": hooks });
            Ok(serde_json::to_string_pretty(&config)?)
        }
        "opencode" => Err(anyhow::anyhow!(
            "OpenCode hook configuration is not yet supported. Please configure manually."
        )),
        _ => Err(anyhow::anyhow!("Unsupported tool: {}", tool)),
    }
}

/// 检查 TOML 内容是否在顶层（任何 [section] 之前）已有 notify 配置
fn has_toplevel_notify(content: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && !trimmed.starts_with("[[") {
            // 遇到 section header，后面的都不是顶层了
            return false;
        }
        if trimmed.starts_with("notify = [") || trimmed.starts_with("notify=[") {
            return true;
        }
    }
    false
}

/// 应用 hook 配置
fn apply_hook_config(tool: &str, config_path: &Path, new_config: &str) -> Result<()> {
    match tool {
        "codex" => {
            // 写入 TOML（必须放在顶层，不能在任何 [section] 下面）
            let mut content = if config_path.exists() {
                fs::read_to_string(config_path)?
            } else {
                String::new()
            };
            if has_toplevel_notify(&content) {
                println!("⚠️  notify already configured at top level, skipping");
            } else {
                // 移除嵌套在 section 内的错误 notify 行
                if content.contains("notify = [") {
                    println!("⚠️  Found notify nested inside a [section], moving to top level");
                    let lines: Vec<&str> = content.lines().collect();
                    let filtered: Vec<&str> = lines
                        .into_iter()
                        .filter(|line| !line.trim_start().starts_with("notify = ["))
                        .collect();
                    content = filtered.join("\n");
                    if !content.ends_with('\n') {
                        content.push('\n');
                    }
                }
                // 插入到顶层（第一个 [section] 之前）
                let insert_line = format!("{}\n", new_config);
                if content.starts_with('[') {
                    content.insert_str(0, &insert_line);
                } else if let Some(pos) = content.find("\n[") {
                    content.insert_str(pos + 1, &insert_line);
                } else {
                    if !content.is_empty() && !content.ends_with('\n') {
                        content.push('\n');
                    }
                    content.push_str(new_config);
                    content.push('\n');
                }
            }
            fs::write(config_path, content)?;
        }
        "claude" => {
            // 合并 JSON
            if config_path.exists() {
                let existing = fs::read_to_string(config_path)?;
                let merged = merge_claude_config(&existing, new_config)?;
                fs::write(config_path, merged)?;
            } else {
                fs::write(config_path, new_config)?;
            }
        }
        "opencode" => {
            return Err(anyhow::anyhow!(
                "OpenCode hook configuration is not yet supported. Please configure manually."
            ));
        }
        _ => {
            return Err(anyhow::anyhow!("Unsupported tool: {}", tool));
        }
    }
    Ok(())
}

/// 合并 Claude 配置（保留现有配置，添加 CAM hooks）
fn merge_claude_config(existing: &str, new_config: &str) -> Result<String> {
    let mut existing_json: serde_json::Value =
        serde_json::from_str(existing).unwrap_or_else(|_| serde_json::json!({}));
    let new_json: serde_json::Value = serde_json::from_str(new_config)?;

    // 获取或创建 hooks 对象
    let hooks = existing_json
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid existing config"))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    // 如果 hooks 不是对象（null、数组等），替换为空对象
    if !hooks.is_object() {
        println!("⚠️  Existing 'hooks' value is not an object, replacing");
        *hooks = serde_json::json!({});
    }

    // 合并新的 hooks
    if let (Some(hooks_obj), Some(new_hooks)) = (
        hooks.as_object_mut(),
        new_json.get("hooks").and_then(|h| h.as_object()),
    ) {
        for (key, value) in new_hooks {
            if !hooks_obj.contains_key(key) {
                hooks_obj.insert(key.clone(), value.clone());
            } else {
                println!("⚠️  Hook '{}' already configured, skipping", key);
            }
        }
    }

    Ok(serde_json::to_string_pretty(&existing_json)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_codex_config() {
        let config = generate_hook_config("codex").unwrap();
        assert!(config.contains("notify"));
        assert!(config.contains("codex-notify"));
        // Should contain an absolute path or fallback, not bare "cam"
        assert!(config.starts_with("notify = [\""));
    }

    #[test]
    fn test_generate_claude_config() {
        let config = generate_hook_config("claude").unwrap();
        let json: serde_json::Value = serde_json::from_str(&config).unwrap();

        // Must have hooks object
        let hooks = json.get("hooks").expect("missing hooks key");

        // All 5 PascalCase events
        let expected_events = [
            "Notification",
            "PermissionRequest",
            "SessionEnd",
            "SessionStart",
            "Stop",
        ];
        for event in &expected_events {
            assert!(
                hooks.get(event).is_some(),
                "missing event: {}",
                event
            );
        }

        // Each event has correct structure: array of objects with matcher and hooks array
        for event in &expected_events {
            let entries = hooks[event].as_array().expect("event should be array");
            assert_eq!(entries.len(), 1);
            let entry = &entries[0];
            assert_eq!(entry["matcher"], "");
            let hook_list = entry["hooks"].as_array().expect("hooks should be array");
            assert_eq!(hook_list.len(), 1);
            assert_eq!(hook_list[0]["type"], "command");
            let cmd = hook_list[0]["command"].as_str().unwrap();
            assert!(cmd.contains("notify --event"), "command missing 'notify --event': {}", cmd);
            assert!(cmd.contains("--agent-id ${SESSION_ID:-unknown}"), "command missing agent-id: {}", cmd);
        }
    }

    #[test]
    fn test_generate_opencode_config() {
        let result = generate_hook_config("opencode");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not yet supported"));
    }

    #[test]
    fn test_generate_unsupported() {
        let result = generate_hook_config("unknown");
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_claude_config_empty() {
        let existing = "{}";
        let new_config = generate_hook_config("claude").unwrap();
        let merged = merge_claude_config(existing, &new_config).unwrap();
        let json: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert!(json.get("hooks").is_some());
        assert!(json["hooks"].get("Stop").is_some());
        assert!(json["hooks"].get("Notification").is_some());
        assert!(json["hooks"].get("PermissionRequest").is_some());
    }

    #[test]
    fn test_merge_claude_config_existing_hooks() {
        let existing =
            r#"{"hooks": {"PreToolUse": [{"matcher": ".*", "hooks": ["echo test"]}]}}"#;
        let new_config = generate_hook_config("claude").unwrap();
        let merged = merge_claude_config(existing, &new_config).unwrap();
        let json: serde_json::Value = serde_json::from_str(&merged).unwrap();
        // 保留现有的 PreToolUse
        assert!(json["hooks"].get("PreToolUse").is_some());
        // 添加新的事件
        assert!(json["hooks"].get("Stop").is_some());
        assert!(json["hooks"].get("Notification").is_some());
    }

    #[test]
    fn test_merge_claude_config_skip_existing() {
        let existing = r#"{"hooks": {"Stop": [{"matcher": ".*", "hooks": ["existing"]}]}}"#;
        let new_config = generate_hook_config("claude").unwrap();
        let merged = merge_claude_config(existing, &new_config).unwrap();
        let json: serde_json::Value = serde_json::from_str(&merged).unwrap();
        // 保留现有的 Stop，不覆盖
        let stop_hooks = &json["hooks"]["Stop"][0]["hooks"][0];
        assert_eq!(stop_hooks.as_str().unwrap(), "existing");
        // 但新事件应被添加
        assert!(json["hooks"].get("Notification").is_some());
    }

    #[test]
    fn test_apply_codex_config_before_sections() {
        // notify 必须放在 [section] 之前，否则会被嵌套到 section 内部
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        // 模拟用户已有带 section 的配置
        let existing = r#"[notice.model_migrations]
"gpt-5.2" = "gpt-5.2-codex"
"#;
        fs::write(&config_path, existing).unwrap();

        let new_config = r#"notify = ["/path/to/cam", "codex-notify"]"#;
        apply_hook_config("codex", &config_path, new_config).unwrap();

        let result = fs::read_to_string(&config_path).unwrap();
        // notify 必须出现在 [notice.model_migrations] 之前
        let notify_pos = result.find("notify = [").unwrap();
        let section_pos = result.find("[notice.model_migrations]").unwrap();
        assert!(
            notify_pos < section_pos,
            "notify must be before [section], got:\n{}",
            result
        );
    }

    #[test]
    fn test_apply_codex_config_file_starts_with_section() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let existing = r#"[model]
name = "gpt-5.2"
"#;
        fs::write(&config_path, existing).unwrap();

        let new_config = r#"notify = ["/path/to/cam", "codex-notify"]"#;
        apply_hook_config("codex", &config_path, new_config).unwrap();

        let result = fs::read_to_string(&config_path).unwrap();
        assert!(
            result.starts_with("notify"),
            "notify must be at file start, got:\n{}",
            result
        );
        // [model] section 应保留
        assert!(result.contains("[model]"));
        assert!(result.contains("name = \"gpt-5.2\""));
    }

    #[test]
    fn test_apply_codex_config_no_sections() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let existing = "model = \"gpt-5.2\"\n";
        fs::write(&config_path, existing).unwrap();

        let new_config = r#"notify = ["/path/to/cam", "codex-notify"]"#;
        apply_hook_config("codex", &config_path, new_config).unwrap();

        let result = fs::read_to_string(&config_path).unwrap();
        assert!(result.contains("notify"));
        assert!(result.contains("model = \"gpt-5.2\""));
    }

    #[test]
    fn test_apply_codex_config_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let new_config = r#"notify = ["/path/to/cam", "codex-notify"]"#;
        apply_hook_config("codex", &config_path, new_config).unwrap();

        let result = fs::read_to_string(&config_path).unwrap();
        assert!(result.contains("notify"));
    }

    #[test]
    fn test_apply_codex_config_skip_existing_notify() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        // notify 已在顶层，不应被覆盖
        let existing = r#"notify = ["/old/path", "codex-notify"]
[model]
name = "gpt-5.2"
"#;
        fs::write(&config_path, existing).unwrap();

        let new_config = r#"notify = ["/new/path", "codex-notify"]"#;
        apply_hook_config("codex", &config_path, new_config).unwrap();

        let result = fs::read_to_string(&config_path).unwrap();
        // 应保留旧的 notify，不覆盖
        assert!(result.contains("/old/path"));
        assert!(!result.contains("/new/path"));
    }

    #[test]
    fn test_apply_codex_config_moves_nested_notify_to_toplevel() {
        // 模拟用户实际遇到的 bug：notify 被嵌套在 [notice.model_migrations] 下
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let existing = r#"model = "gpt-5.2"

[notice.model_migrations]
"gpt-5.2" = "gpt-5.2-codex"
notify = ["cam", "codex-notify"]
"#;
        fs::write(&config_path, existing).unwrap();

        let new_config = r#"notify = ["/path/to/cam", "codex-notify"]"#;
        apply_hook_config("codex", &config_path, new_config).unwrap();

        let result = fs::read_to_string(&config_path).unwrap();
        // 新的 notify 必须在 [section] 之前
        let notify_pos = result.find("notify = [").unwrap();
        let section_pos = result.find("[notice.model_migrations]").unwrap();
        assert!(
            notify_pos < section_pos,
            "notify must be before [section], got:\n{}",
            result
        );
        // 旧的嵌套 notify 应被移除（只出现一次）
        assert_eq!(
            result.matches("notify = [").count(),
            1,
            "should have exactly one notify line, got:\n{}",
            result
        );
    }

    #[test]
    fn test_has_toplevel_notify() {
        // 顶层 notify
        assert!(has_toplevel_notify("notify = [\"/path\"]\n[section]\nfoo=1"));
        assert!(has_toplevel_notify("model = \"x\"\nnotify = [\"/path\"]\n[section]"));

        // section 内的 notify 不算顶层
        assert!(!has_toplevel_notify("[section]\nnotify = [\"/path\"]"));
        assert!(!has_toplevel_notify("model = \"x\"\n[section]\nnotify = [\"/path\"]"));

        // 空文件
        assert!(!has_toplevel_notify(""));

        // 无 notify
        assert!(!has_toplevel_notify("model = \"x\"\n[section]\nfoo = 1"));
    }
}
