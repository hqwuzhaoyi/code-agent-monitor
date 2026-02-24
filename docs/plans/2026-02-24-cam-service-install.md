# CAM Service Install Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `cam install/uninstall/service` commands to manage watcher as a launchd service, ensuring it auto-starts and survives gateway restarts.

**Architecture:** Generate a launchd plist file at `~/Library/LaunchAgents/com.cam.watcher.plist` that runs `cam watch` as a daemon. The service auto-restarts on crash (KeepAlive) and starts on login (RunAtLoad). Commands mirror OpenClaw's `gateway install` pattern.

**Tech Stack:** Rust, clap (CLI), launchd (macOS), plist XML generation

---

### Task 1: Add Service Subcommand Structure

**Files:**
- Modify: `src/main.rs:28-263` (Commands enum)

**Step 1: Add Service subcommand with nested commands**

Add to the `Commands` enum after `Tui`:

```rust
    /// 管理 CAM watcher 服务
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
```

**Step 2: Define ServiceAction enum**

Add after the `Commands` enum:

```rust
#[derive(Subcommand)]
enum ServiceAction {
    /// 安装 watcher 为系统服务
    Install {
        /// 强制重新安装
        #[arg(long)]
        force: bool,
    },
    /// 卸载 watcher 服务
    Uninstall,
    /// 重启 watcher 服务
    Restart,
    /// 查看服务状态
    Status,
    /// 查看服务日志
    Logs {
        /// 显示最近 N 行
        #[arg(long, short, default_value = "50")]
        lines: usize,
        /// 持续跟踪日志
        #[arg(long, short)]
        follow: bool,
    },
}
```

**Step 3: Run to verify compilation**

Run: `cargo check`
Expected: Compiles with warnings about unused ServiceAction

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "$(cat <<'EOF'
feat(cli): add service subcommand structure

Add Service command with Install/Uninstall/Restart/Status/Logs actions
for managing CAM watcher as a launchd service.
EOF
)"
```

---

### Task 2: Create Service Module

**Files:**
- Create: `src/service/mod.rs`
- Create: `src/service/launchd.rs`
- Modify: `src/lib.rs`

**Step 1: Create service module structure**

Create `src/service/mod.rs`:

```rust
//! Service management for CAM watcher daemon

mod launchd;

pub use launchd::{LaunchdService, ServiceStatus};
```

**Step 2: Create launchd service implementation**

Create `src/service/launchd.rs`:

```rust
//! macOS launchd service management

use anyhow::{Result, Context, bail};
use std::path::PathBuf;
use std::process::Command;

const LABEL: &str = "com.cam.watcher";
const PLIST_FILENAME: &str = "com.cam.watcher.plist";

#[derive(Debug)]
pub struct ServiceStatus {
    pub installed: bool,
    pub running: bool,
    pub pid: Option<u32>,
}

pub struct LaunchdService {
    plist_path: PathBuf,
    log_dir: PathBuf,
}

impl LaunchdService {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            plist_path: home.join("Library/LaunchAgents").join(PLIST_FILENAME),
            log_dir: home.join(".config/code-agent-monitor/logs"),
        }
    }

    /// Get the path to the CAM binary
    fn get_cam_binary_path() -> Result<PathBuf> {
        // 1. Check if running from plugins/cam/bin/cam (OpenClaw plugin)
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let plugin_path = home.join(".openclaw/plugins/cam/bin/cam");
        if plugin_path.exists() {
            return Ok(plugin_path);
        }

        // 2. Use current executable path
        let current_exe = std::env::current_exe()
            .context("Failed to get current executable path")?;

        Ok(current_exe)
    }

    /// Generate plist XML content
    fn generate_plist(&self) -> Result<String> {
        let cam_path = Self::get_cam_binary_path()?;
        let cam_path_str = cam_path.to_string_lossy();

        // Ensure log directory exists
        std::fs::create_dir_all(&self.log_dir)?;

        let stdout_log = self.log_dir.join("watcher.log");
        let stderr_log = self.log_dir.join("watcher.err.log");

        Ok(format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>{}</string>

    <key>Comment</key>
    <string>CAM Watcher Service</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProgramArguments</key>
    <array>
      <string>{}</string>
      <string>watch</string>
    </array>

    <key>StandardOutPath</key>
    <string>{}</string>
    <key>StandardErrorPath</key>
    <string>{}</string>
    <key>EnvironmentVariables</key>
    <dict>
      <key>HOME</key>
      <string>{}</string>
      <key>PATH</key>
      <string>/usr/local/bin:/usr/bin:/bin:/opt/homebrew/bin</string>
    </dict>
  </dict>
</plist>
"#,
            LABEL,
            cam_path_str,
            stdout_log.to_string_lossy(),
            stderr_log.to_string_lossy(),
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).to_string_lossy(),
        ))
    }

    /// Install the service
    pub fn install(&self, force: bool) -> Result<()> {
        if self.plist_path.exists() && !force {
            bail!("Service already installed. Use --force to reinstall.");
        }

        // Unload if already loaded
        if self.plist_path.exists() {
            let _ = self.unload();
        }

        // Write plist file
        let plist_content = self.generate_plist()?;
        std::fs::write(&self.plist_path, plist_content)
            .context("Failed to write plist file")?;

        // Load the service
        self.load()?;

        Ok(())
    }

    /// Uninstall the service
    pub fn uninstall(&self) -> Result<()> {
        if !self.plist_path.exists() {
            bail!("Service not installed");
        }

        // Unload first
        self.unload()?;

        // Remove plist file
        std::fs::remove_file(&self.plist_path)
            .context("Failed to remove plist file")?;

        Ok(())
    }

    /// Load (start) the service
    fn load(&self) -> Result<()> {
        let status = Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&self.plist_path)
            .status()
            .context("Failed to run launchctl load")?;

        if !status.success() {
            bail!("launchctl load failed");
        }
        Ok(())
    }

    /// Unload (stop) the service
    fn unload(&self) -> Result<()> {
        let status = Command::new("launchctl")
            .args(["unload"])
            .arg(&self.plist_path)
            .status()
            .context("Failed to run launchctl unload")?;

        if !status.success() {
            bail!("launchctl unload failed");
        }
        Ok(())
    }

    /// Restart the service
    pub fn restart(&self) -> Result<()> {
        if !self.plist_path.exists() {
            bail!("Service not installed. Run 'cam service install' first.");
        }

        self.unload()?;
        self.load()?;
        Ok(())
    }

    /// Get service status
    pub fn status(&self) -> Result<ServiceStatus> {
        if !self.plist_path.exists() {
            return Ok(ServiceStatus {
                installed: false,
                running: false,
                pid: None,
            });
        }

        // Check if running via launchctl list
        let output = Command::new("launchctl")
            .args(["list", LABEL])
            .output()
            .context("Failed to run launchctl list")?;

        if output.status.success() {
            // Parse PID from output (format: "PID\tStatus\tLabel")
            let stdout = String::from_utf8_lossy(&output.stdout);
            let pid = stdout
                .lines()
                .next()
                .and_then(|line| line.split('\t').next())
                .and_then(|pid_str| pid_str.trim().parse::<u32>().ok())
                .filter(|&pid| pid > 0);

            Ok(ServiceStatus {
                installed: true,
                running: pid.is_some(),
                pid,
            })
        } else {
            Ok(ServiceStatus {
                installed: true,
                running: false,
                pid: None,
            })
        }
    }

    /// Get log file paths
    pub fn log_paths(&self) -> (PathBuf, PathBuf) {
        (
            self.log_dir.join("watcher.log"),
            self.log_dir.join("watcher.err.log"),
        )
    }
}
```

**Step 3: Export from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod service;
pub use service::{LaunchdService, ServiceStatus};
```

**Step 4: Run to verify compilation**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add src/service/mod.rs src/service/launchd.rs src/lib.rs
git commit -m "$(cat <<'EOF'
feat(service): add launchd service management module

- LaunchdService for plist generation and launchctl operations
- Auto-detect CAM binary path (plugin or current exe)
- Install/uninstall/restart/status operations
EOF
)"
```

---

### Task 3: Implement Service Command Handlers

**Files:**
- Modify: `src/main.rs`

**Step 1: Add import for service module**

Add to imports at top of `src/main.rs`:

```rust
use code_agent_monitor::{
    // ... existing imports ...
    LaunchdService,
};
```

**Step 2: Add Service command handler**

Add to the match block in `main()`, after the `Tui` handler:

```rust
        Commands::Service { action } => {
            let service = LaunchdService::new();

            match action {
                ServiceAction::Install { force } => {
                    match service.install(force) {
                        Ok(_) => {
                            println!("✅ CAM watcher 服务已安装并启动");
                            println!("   服务会在系统启动时自动运行");
                            println!("   查看状态: cam service status");
                            println!("   查看日志: cam service logs");
                        }
                        Err(e) => {
                            eprintln!("❌ 安装失败: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ServiceAction::Uninstall => {
                    match service.uninstall() {
                        Ok(_) => {
                            println!("✅ CAM watcher 服务已卸载");
                        }
                        Err(e) => {
                            eprintln!("❌ 卸载失败: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ServiceAction::Restart => {
                    match service.restart() {
                        Ok(_) => {
                            println!("✅ CAM watcher 服务已重启");
                        }
                        Err(e) => {
                            eprintln!("❌ 重启失败: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ServiceAction::Status => {
                    match service.status() {
                        Ok(status) => {
                            if !status.installed {
                                println!("⚪ 服务未安装");
                                println!("   运行 'cam service install' 安装服务");
                            } else if status.running {
                                println!("🟢 服务运行中");
                                if let Some(pid) = status.pid {
                                    println!("   PID: {}", pid);
                                }
                            } else {
                                println!("🔴 服务已安装但未运行");
                                println!("   运行 'cam service restart' 启动服务");
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ 获取状态失败: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ServiceAction::Logs { lines, follow } => {
                    let (stdout_log, stderr_log) = service.log_paths();

                    if follow {
                        // Use tail -f for following
                        println!("📋 跟踪日志 (Ctrl+C 退出)...\n");
                        let _ = std::process::Command::new("tail")
                            .args(["-f", "-n"])
                            .arg(lines.to_string())
                            .arg(&stdout_log)
                            .status();
                    } else {
                        // Show recent logs
                        println!("📋 最近 {} 行日志:\n", lines);

                        if stdout_log.exists() {
                            let output = std::process::Command::new("tail")
                                .args(["-n"])
                                .arg(lines.to_string())
                                .arg(&stdout_log)
                                .output();

                            if let Ok(output) = output {
                                print!("{}", String::from_utf8_lossy(&output.stdout));
                            }
                        } else {
                            println!("(日志文件不存在: {})", stdout_log.display());
                        }

                        // Also show errors if any
                        if stderr_log.exists() {
                            let output = std::process::Command::new("tail")
                                .args(["-n", "10"])
                                .arg(&stderr_log)
                                .output();

                            if let Ok(output) = output {
                                let stderr_content = String::from_utf8_lossy(&output.stdout);
                                if !stderr_content.trim().is_empty() {
                                    println!("\n--- 错误日志 ---");
                                    print!("{}", stderr_content);
                                }
                            }
                        }
                    }
                }
            }
        }
```

**Step 3: Run to verify compilation**

Run: `cargo build`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "$(cat <<'EOF'
feat(cli): implement service command handlers

- cam service install [--force]
- cam service uninstall
- cam service restart
- cam service status
- cam service logs [-n lines] [-f]
EOF
)"
```

---

### Task 4: Add Shortcut Commands

**Files:**
- Modify: `src/main.rs:28-263` (Commands enum)

**Step 1: Add install/uninstall as top-level shortcuts**

Add to `Commands` enum:

```rust
    /// 安装 watcher 服务（cam service install 的快捷方式）
    Install {
        /// 强制重新安装
        #[arg(long)]
        force: bool,
    },
    /// 卸载 watcher 服务（cam service uninstall 的快捷方式）
    Uninstall,
```

**Step 2: Add handlers for shortcuts**

Add to the match block in `main()`:

```rust
        Commands::Install { force } => {
            let service = LaunchdService::new();
            match service.install(force) {
                Ok(_) => {
                    println!("✅ CAM watcher 服务已安装并启动");
                    println!("   服务会在系统启动时自动运行");
                    println!("   查看状态: cam service status");
                }
                Err(e) => {
                    eprintln!("❌ 安装失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Uninstall => {
            let service = LaunchdService::new();
            match service.uninstall() {
                Ok(_) => {
                    println!("✅ CAM watcher 服务已卸载");
                }
                Err(e) => {
                    eprintln!("❌ 卸载失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
```

**Step 3: Run to verify**

Run: `cargo build --release`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "$(cat <<'EOF'
feat(cli): add install/uninstall shortcuts

- cam install [--force] as shortcut for cam service install
- cam uninstall as shortcut for cam service uninstall
EOF
)"
```

---

### Task 5: Manual Integration Test

**Files:**
- None (manual testing)

**Step 1: Build release binary**

Run: `cargo build --release`
Expected: Build succeeds

**Step 2: Copy to plugin location**

Run: `cp target/release/cam plugins/cam/bin/cam`
Expected: File copied

**Step 3: Test install command**

Run: `plugins/cam/bin/cam install`
Expected: Output shows "✅ CAM watcher 服务已安装并启动"

**Step 4: Verify plist created**

Run: `cat ~/Library/LaunchAgents/com.cam.watcher.plist`
Expected: Shows valid plist XML with cam watch command

**Step 5: Test status command**

Run: `plugins/cam/bin/cam service status`
Expected: Shows "🟢 服务运行中" with PID

**Step 6: Test restart command**

Run: `plugins/cam/bin/cam service restart`
Expected: Shows "✅ CAM watcher 服务已重启"

**Step 7: Test logs command**

Run: `plugins/cam/bin/cam service logs -n 20`
Expected: Shows recent watcher logs

**Step 8: Test uninstall command**

Run: `plugins/cam/bin/cam uninstall`
Expected: Shows "✅ CAM watcher 服务已卸载"

**Step 9: Verify plist removed**

Run: `ls ~/Library/LaunchAgents/com.cam.watcher.plist 2>&1`
Expected: "No such file or directory"

**Step 10: Reinstall for production use**

Run: `plugins/cam/bin/cam install`
Expected: Service installed and running

**Step 11: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
test: verify cam service install/uninstall works

Manual integration test passed:
- install creates plist and starts service
- status shows running state with PID
- restart reloads service
- logs shows watcher output
- uninstall removes plist and stops service
EOF
)"
```

---

### Task 6: Update Documentation

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Add service commands to quick reference**

Add to the "常用命令" section in CLAUDE.md:

```markdown
# 服务管理
cam install                       # 安装 watcher 为系统服务
cam install --force               # 强制重新安装
cam uninstall                     # 卸载服务
cam service status                # 查看服务状态
cam service restart               # 重启服务（开发后使用）
cam service logs                  # 查看服务日志
cam service logs -f               # 跟踪日志
```

**Step 2: Add development workflow note**

Add to "构建和更新" section:

```markdown
# 开发后更新服务
cargo build --release
cp target/release/cam plugins/cam/bin/cam
cam service restart               # 重启服务加载新二进制
```

**Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: add service management commands to CLAUDE.md

Document cam install/uninstall and cam service subcommands
for managing watcher as a launchd service.
EOF
)"
```

---

Plan complete and saved to `docs/plans/2026-02-24-cam-service-install.md`. Two execution options:

**1. Subagent-Driven (this session)** - I dispatch fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** - Open new session with executing-plans, batch execution with checkpoints

Which approach?
