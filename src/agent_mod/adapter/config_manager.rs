// src/agent_mod/adapter/config_manager.rs
//! 配置备份管理器
//!
//! 提供配置文件的备份和回滚功能，确保配置修改的安全性。

use anyhow::{Context, Result};
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};

/// 默认最大备份数量
const DEFAULT_MAX_BACKUPS: usize = 5;

/// 配置备份管理器
pub struct BackupManager {
    /// 备份目录
    pub backup_dir: PathBuf,
    /// 最大保留备份数
    max_backups: usize,
}

impl BackupManager {
    /// 创建新的备份管理器
    pub fn new() -> Self {
        let backup_dir = dirs::config_dir()
            .unwrap_or_else(|| {
                tracing::warn!("Could not determine config directory, using current directory");
                PathBuf::from(".")
            })
            .join("code-agent-monitor/backups");
        Self {
            backup_dir,
            max_backups: DEFAULT_MAX_BACKUPS,
        }
    }

    /// 创建配置文件备份
    ///
    /// 备份文件保存到 `~/.config/code-agent-monitor/backups/{tool}/{filename}.{timestamp}.bak`
    pub fn backup(&self, tool: &str, original_path: &Path) -> Result<PathBuf> {
        // 如果原文件不存在，直接返回
        if !original_path.exists() {
            return Ok(original_path.to_path_buf());
        }

        // 生成时间戳（包含毫秒以避免冲突）
        let timestamp = Local::now().format("%Y-%m-%dT%H-%M-%S%.3f");
        let filename = original_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config");

        // 构建备份路径
        let tool_backup_dir = self.backup_dir.join(tool);
        let backup_path = tool_backup_dir.join(format!("{}.{}.bak", filename, timestamp));

        // 创建备份目录
        fs::create_dir_all(&tool_backup_dir)
            .with_context(|| format!("Failed to create backup directory: {:?}", tool_backup_dir))?;

        // 复制文件
        fs::copy(original_path, &backup_path)
            .with_context(|| format!("Failed to backup {:?} to {:?}", original_path, backup_path))?;

        // 清理旧备份
        self.cleanup_old_backups(tool, filename)?;

        Ok(backup_path)
    }

    /// 回滚到最近的备份
    pub fn rollback(&self, tool: &str, target_path: &Path) -> Result<()> {
        let filename = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config");

        let latest = self
            .get_latest_backup(tool, filename)?
            .ok_or_else(|| anyhow::anyhow!("No backup found for {} in {}", filename, tool))?;

        fs::copy(&latest, target_path).with_context(|| {
            format!("Failed to rollback {:?} from {:?}", target_path, latest)
        })?;

        Ok(())
    }

    /// 获取最近的备份文件
    fn get_latest_backup(&self, tool: &str, filename: &str) -> Result<Option<PathBuf>> {
        let tool_dir = self.backup_dir.join(tool);

        if !tool_dir.exists() {
            return Ok(None);
        }

        let mut backups = self.list_backups_for_file(&tool_dir, filename)?;

        // 按修改时间排序（最新的在最后）
        backups.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());

        Ok(backups.last().map(|e| e.path()))
    }

    /// 清理旧备份，保留最近 max_backups 个
    fn cleanup_old_backups(&self, tool: &str, filename: &str) -> Result<()> {
        let tool_dir = self.backup_dir.join(tool);

        if !tool_dir.exists() {
            return Ok(());
        }

        let mut backups = self.list_backups_for_file(&tool_dir, filename)?;

        // 按修改时间排序（最旧的在前面）
        backups.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());

        // 删除超出限制的旧备份
        while backups.len() > self.max_backups {
            if let Some(oldest) = backups.first() {
                fs::remove_file(oldest.path())
                    .with_context(|| format!("Failed to remove old backup: {:?}", oldest.path()))?;
                backups.remove(0);
            }
        }

        Ok(())
    }

    /// 列出指定文件的所有备份
    fn list_backups_for_file(
        &self,
        tool_dir: &Path,
        filename: &str,
    ) -> Result<Vec<fs::DirEntry>> {
        let entries = fs::read_dir(tool_dir)
            .with_context(|| format!("Failed to read backup directory: {:?}", tool_dir))?;

        let backups: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{}.", filename))
            })
            .collect();

        Ok(backups)
    }

    /// 列出指定工具的所有备份
    pub fn list_backups(&self, tool: &str) -> Result<Vec<PathBuf>> {
        let tool_dir = self.backup_dir.join(tool);

        if !tool_dir.exists() {
            return Ok(vec![]);
        }

        let entries = fs::read_dir(&tool_dir)
            .with_context(|| format!("Failed to read backup directory: {:?}", tool_dir))?;

        let mut backups: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();

        backups.sort();
        Ok(backups)
    }
}

impl Default for BackupManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_manager(temp_dir: &Path) -> BackupManager {
        BackupManager {
            backup_dir: temp_dir.join("backups"),
            max_backups: 5,
        }
    }

    #[test]
    fn test_backup_creates_file() {
        let temp = tempdir().unwrap();
        let manager = create_test_manager(temp.path());

        // 创建原始配置文件
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "original content").unwrap();

        // 创建备份
        let backup_path = manager.backup("test-tool", &config_path).unwrap();

        // 验证备份文件存在
        assert!(backup_path.exists());
        assert!(backup_path.to_string_lossy().contains("config.toml"));
        assert!(backup_path.to_string_lossy().contains(".bak"));

        // 验证内容一致
        let backup_content = fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, "original content");
    }

    #[test]
    fn test_backup_nonexistent_file_returns_original_path() {
        let temp = tempdir().unwrap();
        let manager = create_test_manager(temp.path());

        let config_path = temp.path().join("nonexistent.toml");
        let result = manager.backup("test-tool", &config_path).unwrap();

        assert_eq!(result, config_path);
    }

    #[test]
    fn test_rollback_restores_content() {
        let temp = tempdir().unwrap();
        let manager = create_test_manager(temp.path());

        // 创建原始配置文件
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "original content").unwrap();

        // 创建备份
        manager.backup("test-tool", &config_path).unwrap();

        // 修改原文件
        fs::write(&config_path, "modified content").unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "modified content");

        // 回滚
        manager.rollback("test-tool", &config_path).unwrap();

        // 验证内容恢复
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "original content");
    }

    #[test]
    fn test_rollback_no_backup_returns_error() {
        let temp = tempdir().unwrap();
        let manager = create_test_manager(temp.path());

        let config_path = temp.path().join("config.toml");
        let result = manager.rollback("test-tool", &config_path);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No backup found"));
    }

    #[test]
    fn test_cleanup_keeps_max_backups() {
        let temp = tempdir().unwrap();
        let mut manager = create_test_manager(temp.path());
        manager.max_backups = 3;

        let config_path = temp.path().join("config.toml");

        // 创建 5 个备份
        for i in 0..5 {
            fs::write(&config_path, format!("content {}", i)).unwrap();
            manager.backup("test-tool", &config_path).unwrap();
            // 确保时间戳不同
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // 验证只保留 3 个备份
        let backups = manager.list_backups("test-tool").unwrap();
        assert_eq!(backups.len(), 3);
    }

    #[test]
    fn test_rollback_uses_latest_backup() {
        let temp = tempdir().unwrap();
        let manager = create_test_manager(temp.path());

        let config_path = temp.path().join("config.toml");

        // 创建多个备份
        fs::write(&config_path, "version 1").unwrap();
        manager.backup("test-tool", &config_path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        fs::write(&config_path, "version 2").unwrap();
        manager.backup("test-tool", &config_path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        fs::write(&config_path, "version 3").unwrap();
        manager.backup("test-tool", &config_path).unwrap();

        // 修改文件
        fs::write(&config_path, "current").unwrap();

        // 回滚应该恢复到 version 3
        manager.rollback("test-tool", &config_path).unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "version 3");
    }

    #[test]
    fn test_list_backups() {
        let temp = tempdir().unwrap();
        let manager = create_test_manager(temp.path());

        let config_path = temp.path().join("config.toml");

        // 创建备份
        fs::write(&config_path, "content").unwrap();
        manager.backup("test-tool", &config_path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        manager.backup("test-tool", &config_path).unwrap();

        let backups = manager.list_backups("test-tool").unwrap();
        assert_eq!(backups.len(), 2);
    }

    #[test]
    fn test_list_backups_empty_tool() {
        let temp = tempdir().unwrap();
        let manager = create_test_manager(temp.path());

        let backups = manager.list_backups("nonexistent-tool").unwrap();
        assert!(backups.is_empty());
    }

    #[test]
    fn test_backup_different_tools_isolated() {
        let temp = tempdir().unwrap();
        let manager = create_test_manager(temp.path());

        let config1 = temp.path().join("config1.toml");
        let config2 = temp.path().join("config2.toml");

        fs::write(&config1, "tool1 content").unwrap();
        fs::write(&config2, "tool2 content").unwrap();

        manager.backup("tool1", &config1).unwrap();
        manager.backup("tool2", &config2).unwrap();

        // 验证备份隔离
        let tool1_backups = manager.list_backups("tool1").unwrap();
        let tool2_backups = manager.list_backups("tool2").unwrap();

        assert_eq!(tool1_backups.len(), 1);
        assert_eq!(tool2_backups.len(), 1);
    }
}
