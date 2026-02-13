//! 日志视图模块

use std::path::PathBuf;
use std::collections::VecDeque;
use anyhow::Result;

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LogLevel {
    #[default]
    All,
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    pub fn next(&self) -> Self {
        match self {
            LogLevel::All => LogLevel::Error,
            LogLevel::Error => LogLevel::Warn,
            LogLevel::Warn => LogLevel::Info,
            LogLevel::Info => LogLevel::Debug,
            LogLevel::Debug => LogLevel::All,
        }
    }

    pub fn matches(&self, line: &str) -> bool {
        match self {
            LogLevel::All => true,
            LogLevel::Error => line.contains("ERROR") || line.contains("❌"),
            LogLevel::Warn => line.contains("WARN") || line.contains("⚠") || line.contains("ERROR") || line.contains("❌"),
            LogLevel::Info => line.contains("INFO") || line.contains("✅") || line.contains("WARN") || line.contains("⚠") || line.contains("ERROR") || line.contains("❌"),
            LogLevel::Debug => true,
        }
    }
}

/// 日志状态
pub struct LogsState {
    pub lines: VecDeque<String>,
    pub filter: LogLevel,
    pub scroll_offset: usize,
    pub search_query: String,
}

impl LogsState {
    pub fn new() -> Self {
        Self {
            lines: VecDeque::with_capacity(1000),
            filter: LogLevel::All,
            scroll_offset: 0,
            search_query: String::new(),
        }
    }

    /// 加载日志文件
    pub fn load(&mut self) -> Result<()> {
        let log_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config/code-agent-monitor/hook.log");

        if log_path.exists() {
            let content = std::fs::read_to_string(&log_path)?;
            self.lines.clear();
            for line in content.lines().rev().take(500) {
                self.lines.push_front(line.to_string());
            }
        }
        Ok(())
    }

    /// 获取过滤后的行
    pub fn filtered_lines(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|line| self.filter.matches(line))
            .filter(|line| {
                self.search_query.is_empty() || line.contains(&self.search_query)
            })
            .map(|s| s.as_str())
            .collect()
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_to_bottom(&mut self) {
        let filtered = self.filtered_lines();
        self.scroll_offset = filtered.len().saturating_sub(20);
    }

    pub fn toggle_filter(&mut self) {
        self.filter = self.filter.next();
    }
}

impl Default for LogsState {
    fn default() -> Self {
        Self::new()
    }
}
