//! launchd service management for macOS

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

/// Service status information
#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub installed: bool,
    pub running: bool,
    pub pid: Option<u32>,
}

/// launchd service manager for CAM watcher daemon
pub struct LaunchdService {
    plist_path: PathBuf,
    log_dir: PathBuf,
}

impl LaunchdService {
    const SERVICE_LABEL: &'static str = "com.cam.watcher";
    const PLIST_NAME: &'static str = "com.cam.watcher.plist";

    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        let plist_path = home.join("Library/LaunchAgents").join(Self::PLIST_NAME);
        let log_dir = home.join(".config/code-agent-monitor/logs");

        Ok(Self { plist_path, log_dir })
    }

    /// Get the CAM binary path, preferring plugin location
    fn get_cam_binary_path() -> Result<PathBuf> {
        // Check plugin location first
        if let Some(home) = dirs::home_dir() {
            let plugin_path = home.join(".claude/plugins/cam/bin/cam");
            if plugin_path.exists() {
                return Ok(plugin_path);
            }
        }

        // Fall back to current executable
        std::env::current_exe().context("Failed to get current executable path")
    }

    /// Generate plist content for launchd
    fn generate_plist(&self) -> Result<String> {
        let cam_path = Self::get_cam_binary_path()?;
        let stdout_log = self.log_dir.join("watcher.stdout.log");
        let stderr_log = self.log_dir.join("watcher.stderr.log");
        let home = dirs::home_dir().context("Failed to get home directory")?;

        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{cam_path}</string>
        <string>watch</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{stdout}</string>
    <key>StandardErrorPath</key>
    <string>{stderr}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>{home}</string>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin</string>
    </dict>
</dict>
</plist>
"#,
            label = Self::SERVICE_LABEL,
            cam_path = cam_path.display(),
            stdout = stdout_log.display(),
            stderr = stderr_log.display(),
            home = home.display(),
        ))
    }

    /// Install the launchd service
    pub fn install(&self) -> Result<()> {
        // Create log directory
        std::fs::create_dir_all(&self.log_dir)
            .context("Failed to create log directory")?;

        // Create LaunchAgents directory if needed
        if let Some(parent) = self.plist_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create LaunchAgents directory")?;
        }

        // Generate and write plist
        let plist_content = self.generate_plist()?;
        std::fs::write(&self.plist_path, &plist_content)
            .context("Failed to write plist file")?;

        // Load the service, cleanup on failure
        if let Err(e) = self.load() {
            let _ = std::fs::remove_file(&self.plist_path);
            return Err(e);
        }

        Ok(())
    }

    /// Uninstall the launchd service
    pub fn uninstall(&self) -> Result<()> {
        // Unload first if running
        let _ = self.unload();

        // Wait for launchd to fully unload the service
        // launchd operations are asynchronous
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Remove plist file
        if self.plist_path.exists() {
            std::fs::remove_file(&self.plist_path)
                .context("Failed to remove plist file")?;
        }

        Ok(())
    }

    /// Load (start) the service
    pub fn load(&self) -> Result<()> {
        let status = Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&self.plist_path)
            .status()
            .context("Failed to execute launchctl load")?;

        if !status.success() {
            anyhow::bail!("launchctl load failed with status: {}", status);
        }

        Ok(())
    }

    /// Unload (stop) the service
    pub fn unload(&self) -> Result<()> {
        let status = Command::new("launchctl")
            .args(["unload"])
            .arg(&self.plist_path)
            .status()
            .context("Failed to execute launchctl unload")?;

        if !status.success() {
            anyhow::bail!("launchctl unload failed with status: {}", status);
        }

        Ok(())
    }

    /// Restart the service
    pub fn restart(&self) -> Result<()> {
        let _ = self.unload();
        self.load()
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

        // Check if service is running using launchctl list
        let output = Command::new("launchctl")
            .args(["list", Self::SERVICE_LABEL])
            .output()
            .context("Failed to execute launchctl list")?;

        if !output.status.success() {
            return Ok(ServiceStatus {
                installed: true,
                running: false,
                pid: None,
            });
        }

        // Parse PID from output (format: "PID" = 12345;)
        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid = stdout
            .lines()
            .find(|line| line.contains("\"PID\""))
            .and_then(|line| {
                line.split('=')
                    .nth(1)
                    .map(|s| s.trim().trim_end_matches(';').trim())
                    .and_then(|s| s.parse::<u32>().ok())
            })
            .filter(|&pid| pid > 0);

        Ok(ServiceStatus {
            installed: true,
            running: pid.is_some(),
            pid,
        })
    }

    /// Get log file paths
    pub fn log_paths(&self) -> (PathBuf, PathBuf) {
        (
            self.log_dir.join("watcher.stdout.log"),
            self.log_dir.join("watcher.stderr.log"),
        )
    }
}
