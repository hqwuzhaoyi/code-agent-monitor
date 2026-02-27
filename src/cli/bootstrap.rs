// src/cli/bootstrap.rs
//! Bootstrap 命令 - 交互式引导完成所有配置
//!
//! 一次性完成 webhook、AI 监控、agent hooks 的配置，
//! 自动检测 OpenClaw 已有配置进行复用。

use anyhow::{Context, Result};
use clap::Args;
use dialoguer::{Confirm, Input, Select};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

/// Bootstrap 命令参数
#[derive(Args)]
pub struct BootstrapArgs {
    /// 使用检测到的默认值，跳过交互式提示
    #[arg(long)]
    pub auto: bool,
}

/// 检测到的 AI Provider
#[derive(Debug, Clone)]
struct DetectedProvider {
    name: String,
    api_key: String,
    base_url: String,
    api_type: String,
    model: Option<String>,
}

/// Bootstrap 配置结果
#[derive(Default)]
struct BootstrapConfig {
    gateway_url: Option<String>,
    hook_token: Option<String>,
    timeout_secs: u64,
    providers: Vec<ProviderEntry>,
}

#[derive(Clone)]
struct ProviderEntry {
    api_key: String,
    base_url: String,
    model: String,
    api_type: String,
}

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/code-agent-monitor")
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// 处理 bootstrap 命令
pub fn handle_bootstrap(args: BootstrapArgs) -> Result<()> {
    println!("CAM Bootstrap - 交互式配置向导\n");

    // 检查已有配置
    let existing = detect_existing_config();
    if let Some(ref _existing) = existing {
        println!("检测到已有配置: {}", config_path().display());
        if !args.auto {
            let overwrite = Confirm::new()
                .with_prompt("是否覆盖现有配置？（会保留未修改的字段）")
                .default(false)
                .interact()
                .unwrap_or(false);
            if !overwrite {
                println!("已取消。");
                return Ok(());
            }
        }
        println!();
    }

    let mut config = BootstrapConfig {
        timeout_secs: 30,
        ..Default::default()
    };

    // Step 1: Webhook 配置
    step_webhook(&mut config, &existing, args.auto)?;

    // Step 2: AI 监控配置
    step_ai_monitoring(&mut config, &existing, args.auto)?;

    // Step 3: Agent Hooks 配置
    step_agent_hooks(args.auto)?;

    // 写入配置
    write_config(&config, &existing)?;

    // 输出下一步提示
    print_next_steps();

    Ok(())
}

/// 检测已有的 config.json
fn detect_existing_config() -> Option<serde_json::Value> {
    let path = config_path();
    if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    } else {
        None
    }
}

/// 从 OpenClaw 配置读取 webhook 信息
fn read_openclaw_webhook() -> Option<(String, String)> {
    let home = dirs::home_dir()?;
    let path = home.join(".openclaw/openclaw.json");
    let content = fs::read_to_string(&path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&content).ok()?;

    let token = config
        .get("hooks")
        .and_then(|h| h.get("token"))
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())?
        .to_string();

    let port = config
        .get("gateway")
        .and_then(|g| g.get("port"))
        .and_then(|p| p.as_u64())
        .unwrap_or(18789);

    let gateway_url = format!("http://localhost:{}", port);
    Some((gateway_url, token))
}

/// 扫描 OpenClaw providers
fn scan_openclaw_providers() -> Vec<DetectedProvider> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let path = home.join(".openclaw/openclaw.json");
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let config: serde_json::Value = match serde_json::from_str(&content) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let providers_obj = config
        .get("models")
        .and_then(|m| m.get("providers"))
        .and_then(|p| p.as_object());

    let providers_obj = match providers_obj {
        Some(p) => p,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    for (name, provider) in providers_obj {
        let api = provider.get("api").and_then(|a| a.as_str()).unwrap_or("");
        let api_type = match api {
            "anthropic-messages" => "anthropic",
            "openai-responses" | "openai-chat" => "openai",
            _ => continue,
        };

        let api_key = provider
            .get("apiKey")
            .and_then(|k| k.as_str())
            .unwrap_or("")
            .to_string();
        if api_key.is_empty() {
            continue;
        }

        let base_url = provider
            .get("baseUrl")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();

        // 读取 models 数组中的第一个模型
        let model = provider
            .get("models")
            .and_then(|m| m.as_array())
            .and_then(|arr| arr.first())
            .and_then(|m| m.as_str())
            .map(|s| s.to_string());

        result.push(DetectedProvider {
            name: name.clone(),
            api_key,
            base_url,
            api_type: api_type.to_string(),
            model,
        });
    }
    result
}

/// Step 1: Webhook 配置
fn step_webhook(
    config: &mut BootstrapConfig,
    existing: &Option<serde_json::Value>,
    auto: bool,
) -> Result<()> {
    println!("── Step 1/3: Webhook 配置 ──\n");

    // 尝试从 OpenClaw 自动检测
    let openclaw = read_openclaw_webhook();

    // 从已有配置读取默认值
    let existing_url = existing
        .as_ref()
        .and_then(|e| e.get("webhook"))
        .and_then(|w| w.get("gateway_url"))
        .and_then(|u| u.as_str())
        .map(|s| s.to_string());
    let existing_token = existing
        .as_ref()
        .and_then(|e| e.get("webhook"))
        .and_then(|w| w.get("hook_token"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());

    if let Some((ref url, ref token)) = openclaw {
        println!("  从 OpenClaw 检测到:");
        println!("    gateway_url: {}", url);
        println!("    hook_token:  {}...", &token[..token.len().min(8)]);

        if auto {
            config.gateway_url = Some(url.clone());
            config.hook_token = Some(token.clone());
            println!("  [auto] 使用 OpenClaw 配置\n");
            return Ok(());
        }

        let use_detected = Confirm::new()
            .with_prompt("使用检测到的 OpenClaw 配置？")
            .default(true)
            .interact()
            .unwrap_or(true);

        if use_detected {
            config.gateway_url = Some(url.clone());
            config.hook_token = Some(token.clone());
            println!();
            return Ok(());
        }
    } else {
        println!("  未检测到 OpenClaw 配置 (~/.openclaw/openclaw.json)");
        if auto {
            // auto 模式下使用已有配置或默认值
            config.gateway_url = existing_url.or_else(|| Some("http://localhost:18789".to_string()));
            config.hook_token = existing_token;
            println!("  [auto] 使用默认值\n");
            return Ok(());
        }
    }

    // 手动输入
    let default_url = existing_url
        .or_else(|| openclaw.as_ref().map(|(u, _)| u.clone()))
        .unwrap_or_else(|| "http://localhost:18789".to_string());

    let url: String = Input::new()
        .with_prompt("Gateway URL")
        .default(default_url)
        .interact_text()
        .context("读取 gateway URL 失败")?;

    let default_token = existing_token
        .or_else(|| openclaw.map(|(_, t)| t))
        .unwrap_or_default();

    let token: String = Input::new()
        .with_prompt("Hook Token")
        .default(default_token)
        .interact_text()
        .context("读取 hook token 失败")?;

    config.gateway_url = Some(url);
    config.hook_token = if token.is_empty() {
        None
    } else {
        Some(token)
    };
    println!();
    Ok(())
}

/// Step 2: AI 监控配置
fn step_ai_monitoring(
    config: &mut BootstrapConfig,
    _existing: &Option<serde_json::Value>,
    auto: bool,
) -> Result<()> {
    println!("── Step 2/3: AI 监控配置 ──\n");

    // 扫描 OpenClaw providers
    let detected = scan_openclaw_providers();

    // 也检查环境变量和 ~/.anthropic/api_key
    let env_key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty());
    let file_key = dirs::home_dir()
        .map(|h| h.join(".anthropic/api_key"))
        .and_then(|p| fs::read_to_string(&p).ok())
        .map(|s| s.trim().to_string())
        .filter(|k| !k.is_empty());

    if !detected.is_empty() {
        println!("  从 OpenClaw 检测到 {} 个 provider:", detected.len());
        for p in &detected {
            let model_info = p
                .model
                .as_deref()
                .unwrap_or("(default)");
            println!(
                "    - {} [{}] model: {}",
                p.name, p.api_type, model_info
            );
        }
        println!();
    }

    if let Some(ref key) = env_key {
        println!("  检测到环境变量 ANTHROPIC_API_KEY: {}...", &key[..key.len().min(12)]);
    }
    if let Some(ref key) = file_key {
        println!("  检测到 ~/.anthropic/api_key: {}...", &key[..key.len().min(12)]);
    }

    if auto {
        // auto 模式：优先用 OpenClaw providers，否则用环境变量/文件
        if !detected.is_empty() {
            for p in &detected {
                let model = default_model_for(&p.api_type, p.model.as_deref());
                config.providers.push(ProviderEntry {
                    api_key: p.api_key.clone(),
                    base_url: p.base_url.clone(),
                    model,
                    api_type: p.api_type.clone(),
                });
            }
            println!("  [auto] 使用 OpenClaw providers\n");
        } else if env_key.is_some() || file_key.is_some() {
            println!("  [auto] 将使用环境变量/文件中的 API key（无需写入配置）\n");
        } else {
            println!("  [auto] 未检测到 AI provider，跳过\n");
        }
        return Ok(());
    }

    // 交互模式
    if detected.is_empty() && env_key.is_none() && file_key.is_none() {
        println!("  未检测到任何 AI provider。");
        let manual = Confirm::new()
            .with_prompt("手动输入 API key？")
            .default(false)
            .interact()
            .unwrap_or(false);

        if manual {
            prompt_manual_provider(config)?;
        } else {
            println!("  跳过 AI 监控配置（通知将缺少 AI 分析能力）");
        }
        println!();
        return Ok(());
    }

    // 有检测到的 providers
    if !detected.is_empty() {
        let selected = if detected.len() == 1 {
            let use_it = Confirm::new()
                .with_prompt(format!("使用 {} provider？", detected[0].name))
                .default(true)
                .interact()
                .unwrap_or(true);
            if use_it {
                Some(0)
            } else {
                None
            }
        } else {
            let items: Vec<String> = detected
                .iter()
                .map(|p| {
                    format!(
                        "{} [{}]",
                        p.name,
                        p.api_type
                    )
                })
                .collect();

            let selection = Select::new()
                .with_prompt("选择 AI provider")
                .items(&items)
                .default(0)
                .interact_opt()
                .unwrap_or(None);
            selection
        };

        if let Some(idx) = selected {
            let p = &detected[idx];
            let default_model = default_model_for(&p.api_type, p.model.as_deref());

            println!("\n  配置 {} provider:\n", p.name);

            let api_type: String = Input::new()
                .with_prompt("API Type")
                .default(p.api_type.clone())
                .interact_text()
                .context("读取 api_type 失败")?;

            let api_key: String = Input::new()
                .with_prompt("API Key")
                .default(p.api_key.clone())
                .interact_text()
                .context("读取 api_key 失败")?;

            let base_url: String = Input::new()
                .with_prompt("Base URL")
                .default(if p.base_url.is_empty() {
                    default_base_url(&api_type)
                } else {
                    p.base_url.clone()
                })
                .interact_text()
                .context("读取 base_url 失败")?;

            let model: String = Input::new()
                .with_prompt("Model")
                .default(default_model)
                .interact_text()
                .context("读取 model 失败")?;

            config.providers.push(ProviderEntry {
                api_key,
                base_url,
                model,
                api_type,
            });
        }
    } else if env_key.is_some() || file_key.is_some() {
        println!("  将使用环境变量/文件中的 API key（无需写入配置）");
    }

    println!();
    Ok(())
}

/// 手动输入 provider
fn prompt_manual_provider(config: &mut BootstrapConfig) -> Result<()> {
    let items = vec!["anthropic", "openai"];
    let selection = Select::new()
        .with_prompt("API 类型")
        .items(&items)
        .default(0)
        .interact()
        .unwrap_or(0);
    let api_type = items[selection].to_string();

    let api_key: String = Input::new()
        .with_prompt("API Key")
        .interact_text()
        .context("读取 API key 失败")?;

    if api_key.is_empty() {
        return Ok(());
    }

    let base_url: String = Input::new()
        .with_prompt("Base URL")
        .default(default_base_url(&api_type))
        .interact_text()
        .context("读取 base URL 失败")?;

    let default_model = default_model_for(&api_type, None);
    let model: String = Input::new()
        .with_prompt("Model")
        .default(default_model)
        .interact_text()
        .context("读取 model 失败")?;

    config.providers.push(ProviderEntry {
        api_key,
        base_url,
        model,
        api_type,
    });
    Ok(())
}

/// 根据 api_type 返回默认模型
fn default_model_for(api_type: &str, configured: Option<&str>) -> String {
    if let Some(m) = configured {
        if !m.is_empty() {
            return m.to_string();
        }
    }
    match api_type {
        "anthropic" => "claude-haiku-4-5-20251001".to_string(),
        "openai" => "gpt-4.1-mini".to_string(),
        _ => "claude-haiku-4-5-20251001".to_string(),
    }
}

/// 根据 api_type 返回默认 base URL
fn default_base_url(api_type: &str) -> String {
    match api_type {
        "anthropic" => "https://api.anthropic.com".to_string(),
        "openai" => "https://api.openai.com".to_string(),
        _ => "https://api.anthropic.com".to_string(),
    }
}

/// Step 3: Agent Hooks 配置
fn step_agent_hooks(auto: bool) -> Result<()> {
    println!("── Step 3/3: Agent Hooks 配置 ──\n");

    // 检测已安装的 agent 工具
    let claude_installed = which::which("claude").is_ok();
    let codex_installed = which::which("codex").is_ok();
    let opencode_installed = which::which("opencode").is_ok();

    let mut detected = Vec::new();
    if claude_installed {
        detected.push("claude");
    }
    if codex_installed {
        detected.push("codex");
    }
    if opencode_installed {
        detected.push("opencode");
    }

    if detected.is_empty() {
        println!("  未检测到已安装的 AI 编码工具。");
        println!("  安装后可运行 `cam setup <tool>` 配置 hooks。\n");
        return Ok(());
    }

    println!("  检测到: {}", detected.join(", "));

    for tool in &detected {
        if *tool == "opencode" {
            println!("  ⚠️  OpenCode hooks 暂不支持自动配置，请手动配置。");
            continue;
        }
        if auto {
            println!("  [auto] 配置 {} hooks...", tool);
            run_setup(tool)?;
        } else {
            let setup = Confirm::new()
                .with_prompt(format!("配置 {} hooks？", tool))
                .default(true)
                .interact()
                .unwrap_or(false);

            if setup {
                run_setup(tool)?;
            }
        }
    }

    println!();
    Ok(())
}

/// 调用 handle_setup 配置 hooks
fn run_setup(tool: &str) -> Result<()> {
    use crate::cli::setup::{handle_setup, SetupArgs};
    handle_setup(SetupArgs {
        tool: tool.to_string(),
        yes: true,
        dry_run: false,
    })
}

/// 写入 config.json
fn write_config(config: &BootstrapConfig, existing: &Option<serde_json::Value>) -> Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir).context("创建配置目录失败")?;

    // 从已有配置开始，合并新值
    let mut output = existing.clone().unwrap_or_else(|| json!({}));
    let obj = output.as_object_mut().unwrap();

    // Webhook
    if config.gateway_url.is_some() || config.hook_token.is_some() {
        let mut webhook = obj
            .get("webhook")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let wh = webhook.as_object_mut().unwrap();

        if let Some(ref url) = config.gateway_url {
            wh.insert("gateway_url".to_string(), json!(url));
        }
        if let Some(ref token) = config.hook_token {
            wh.insert("hook_token".to_string(), json!(token));
        }
        if !wh.contains_key("timeout_secs") {
            wh.insert("timeout_secs".to_string(), json!(config.timeout_secs));
        }
        obj.insert("webhook".to_string(), webhook);
    }

    // Providers
    if !config.providers.is_empty() {
        let providers: Vec<serde_json::Value> = config
            .providers
            .iter()
            .map(|p| {
                json!({
                    "api_key": p.api_key,
                    "base_url": p.base_url,
                    "model": p.model,
                    "api_type": p.api_type,
                })
            })
            .collect();
        obj.insert("providers".to_string(), json!(providers));
    }

    let path = config_path();
    let content = serde_json::to_string_pretty(&output)?;
    fs::write(&path, &content).context("写入配置文件失败")?;

    println!("配置已写入: {}", path.display());
    Ok(())
}

/// 输出下一步提示
fn print_next_steps() {
    println!("\n── 下一步 ──\n");
    println!("  1. 安装 watcher 服务:  cam install");
    println!("  2. 确认服务运行:       cam service status");
    println!("  3. 启动你的第一个 Agent: cam start \"你的任务\"");
    println!();
    println!("  查看完整文档: cam --help");
}
