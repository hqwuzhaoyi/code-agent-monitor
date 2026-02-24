// src/agent_mod/adapter/config_manager.rs
//! 配置备份管理器
//!
//! TODO: 由 teammate 实现

use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct BackupManager {
    pub backup_dir: PathBuf,
    max_backups: usize,
}

impl BackupManager {
    pub fn new() -> Self {
        let backup_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("code-agent-monitor/backups");
        Self {
            backup_dir,
            max_backups: 5,
        }
    }

    pub fn backup(&self, _tool: &str, original_path: &Path) -> Result<PathBuf> {
        Ok(original_path.to_path_buf())
    }

    pub fn rollback(&self, _tool: &str, _target_path: &Path) -> Result<()> {
        Ok(())
    }
}

impl Default for BackupManager {
    fn default() -> Self {
        Self::new()
    }
}
