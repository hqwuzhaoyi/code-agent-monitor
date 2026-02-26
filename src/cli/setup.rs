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

/// 生成 hook 配置
fn generate_hook_config(tool: &str) -> Result<String> {
    match tool {
        "codex" => Ok(r#"notify = ["cam", "codex-notify"]"#.to_string()),
        "claude" => Ok(r#"{
  "hooks": {
    "Stop": [
      {
        "matcher": ".*",
        "hooks": ["cam notify --event stop"]
      }
    ],
    "notification": [
      {
        "matcher": ".*",
        "hooks": ["cam notify --event notification"]
      }
    ],
    "session_start": [
      {
        "matcher": ".*",
        "hooks": ["cam notify --event session_start"]
      }
    ]
  }
}"#
        .to_string()),
        "opencode" => Ok(r#"# OpenCode hook configuration
[hooks]
on_idle = "cam notify --event WaitingForInput"
on_error = "cam notify --event Error"
"#
        .to_string()),
        _ => Err(anyhow::anyhow!("Unsupported tool: {}", tool)),
    }
}

/// 应用 hook 配置
fn apply_hook_config(tool: &str, config_path: &Path, new_config: &str) -> Result<()> {
    match tool {
        "codex" => {
            // 追加到 TOML
            let mut content = if config_path.exists() {
                fs::read_to_string(config_path)?
            } else {
                String::new()
            };
            if !content.contains("notify") {
                if !content.is_empty() && !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(new_config);
                content.push('\n');
            } else {
                println!("⚠️  notify already configured, skipping");
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
            // 追加到 TOML
            let mut content = if config_path.exists() {
                fs::read_to_string(config_path)?
            } else {
                String::new()
            };
            if !content.contains("[hooks]") {
                if !content.is_empty() && !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(new_config);
            } else {
                println!("⚠️  hooks already configured, skipping");
            }
            fs::write(config_path, content)?;
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
        assert!(config.contains("cam"));
        assert!(config.contains("codex-notify"));
    }

    #[test]
    fn test_generate_claude_config() {
        let config = generate_hook_config("claude").unwrap();
        assert!(config.contains("hooks"));
        assert!(config.contains("Stop"));
        assert!(config.contains("notification"));
        assert!(config.contains("cam notify"));
    }

    #[test]
    fn test_generate_opencode_config() {
        let config = generate_hook_config("opencode").unwrap();
        assert!(config.contains("[hooks]"));
        assert!(config.contains("on_idle"));
        assert!(config.contains("on_error"));
    }

    #[test]
    fn test_generate_unsupported() {
        let result = generate_hook_config("unknown");
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_claude_config_empty() {
        let existing = "{}";
        let new_config = r#"{"hooks": {"Stop": [{"matcher": ".*", "hooks": ["cam notify"]}]}}"#;
        let merged = merge_claude_config(existing, new_config).unwrap();
        let json: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert!(json.get("hooks").is_some());
        assert!(json["hooks"].get("Stop").is_some());
    }

    #[test]
    fn test_merge_claude_config_existing_hooks() {
        let existing = r#"{"hooks": {"PreToolUse": [{"matcher": ".*", "hooks": ["echo test"]}]}}"#;
        let new_config = r#"{"hooks": {"Stop": [{"matcher": ".*", "hooks": ["cam notify"]}]}}"#;
        let merged = merge_claude_config(existing, new_config).unwrap();
        let json: serde_json::Value = serde_json::from_str(&merged).unwrap();
        // 保留现有的 PreToolUse
        assert!(json["hooks"].get("PreToolUse").is_some());
        // 添加新的 Stop
        assert!(json["hooks"].get("Stop").is_some());
    }

    #[test]
    fn test_merge_claude_config_skip_existing() {
        let existing = r#"{"hooks": {"Stop": [{"matcher": ".*", "hooks": ["existing"]}]}}"#;
        let new_config = r#"{"hooks": {"Stop": [{"matcher": ".*", "hooks": ["cam notify"]}]}}"#;
        let merged = merge_claude_config(existing, new_config).unwrap();
        let json: serde_json::Value = serde_json::from_str(&merged).unwrap();
        // 保留现有的 Stop，不覆盖
        let stop_hooks = &json["hooks"]["Stop"][0]["hooks"][0];
        assert_eq!(stop_hooks.as_str().unwrap(), "existing");
    }
}
