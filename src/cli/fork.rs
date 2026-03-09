// src/cli/fork.rs
//! Fork 命令 - 从现有 agent 会话分叉出新会话（继承完整对话历史）

use crate::agent::{AgentManager, AgentRecord, StartAgentRequest};
use anyhow::{anyhow, Result};
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;
use uuid::Uuid;

/// Fork 命令参数
#[derive(Args)]
pub struct ForkArgs {
    /// 原始 Agent ID 或 tmux session 名称
    pub agent_id: String,

    /// 新 agent 的 tmux session 名称（可选，默认自动生成）
    #[arg(long, short)]
    pub name: Option<String>,

    /// 输出 JSON 格式
    #[arg(long)]
    pub json: bool,
}

/// Fork 命令输出
#[derive(Debug, Serialize)]
pub struct ForkOutput {
    pub original_agent_id: String,
    pub original_session_id: String,
    pub new_agent_id: String,
    pub new_session_id: String,
    pub new_tmux_session: String,
    pub project_path: String,
}

/// 处理 fork 命令
pub fn handle_fork(args: ForkArgs) -> Result<()> {
    let agent_manager = AgentManager::new();

    // 1. 查找原始 agent
    let agents = agent_manager.list_agents()?;
    let original = agents
        .iter()
        .find(|a| a.agent_id == args.agent_id || a.tmux_session == args.agent_id)
        .ok_or_else(|| anyhow!("找不到 agent: {}", args.agent_id))?;

    let project_path = original.project_path.clone();
    let agent_type = original.agent_type.to_string();

    // 2. 找到原始会话的 session_id
    let original_session_id = find_session_id(original, &project_path)?;

    // 3. 找到原始 .jsonl 文件
    let project_dir = get_claude_project_dir(&project_path)?;
    let original_jsonl = project_dir.join(format!("{}.jsonl", original_session_id));

    if !original_jsonl.exists() {
        return Err(anyhow!(
            "找不到会话文件: {}\n会话可能已过期或被清理",
            original_jsonl.display()
        ));
    }

    // 4. 生成新 UUID，复制 .jsonl
    let new_session_id = Uuid::new_v4().to_string();
    let new_jsonl = project_dir.join(format!("{}.jsonl", new_session_id));

    // 逐行复制 .jsonl 并替换内部 sessionId 引用
    // Claude Code 要求文件名 UUID 与内部 sessionId 一致，否则无法恢复会话
    // 使用 BufReader/BufWriter 流式处理，避免大文件 OOM
    {
        use std::io::{BufRead, BufReader, BufWriter, Write as IoWrite};

        let src = std::fs::File::open(&original_jsonl).map_err(|e| {
            anyhow!("读取会话文件失败: {}: {}", original_jsonl.display(), e)
        })?;
        let dst = std::fs::File::create(&new_jsonl).map_err(|e| {
            anyhow!("创建会话文件失败: {}: {}", new_jsonl.display(), e)
        })?;

        let reader = BufReader::new(src);
        let mut writer = BufWriter::new(dst);

        for line in reader.lines() {
            let line = line.map_err(|e| anyhow!("读取行失败: {}", e))?;
            let replaced = line.replace(&original_session_id, &new_session_id);
            writeln!(writer, "{}", replaced)
                .map_err(|e| anyhow!("写入行失败: {}", e))?;
        }
        writer.flush().map_err(|e| anyhow!("flush 失败: {}", e))?;
    }

    let original_size = std::fs::metadata(&original_jsonl)
        .map(|m| m.len())
        .unwrap_or(0);
    eprintln!(
        "✅ 已复制会话文件并替换 sessionId ({:.1} KB)",
        original_size as f64 / 1024.0
    );

    // 5. 用 --resume 启动新 agent
    let request = StartAgentRequest {
        project_path: project_path.clone(),
        agent_type: Some(agent_type),
        resume_session: Some(new_session_id.clone()),
        initial_prompt: None,
        agent_id: None,
        tmux_session: args.name,
    };

    let response = agent_manager.start_agent(request)?;

    // 6. 输出结果
    let output = ForkOutput {
        original_agent_id: original.agent_id.clone(),
        original_session_id: original_session_id.clone(),
        new_agent_id: response.agent_id.clone(),
        new_session_id: new_session_id.clone(),
        new_tmux_session: response.tmux_session.clone(),
        project_path,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        eprintln!("🔀 会话分叉成功");
        eprintln!("  原始 agent: {}", output.original_agent_id);
        eprintln!("  新 agent:   {}", output.new_agent_id);
        eprintln!("  tmux:       {}", output.new_tmux_session);
        eprintln!("  项目:       {}", output.project_path);
        eprintln!();
        eprintln!(
            "查看分身: tmux attach -t {}",
            output.new_tmux_session
        );
    }

    Ok(())
}

/// 从 agent 记录或运行时状态中查找 Claude session_id
fn find_session_id(agent: &AgentRecord, project_path: &str) -> Result<String> {
    // 策略 1: agents.json 中已有 session_id
    if let Some(ref sid) = agent.session_id {
        eprintln!("📎 使用 agents.json 中的 session_id: {}", sid);
        return Ok(sid.to_string());
    }

    // 策略 2: 从 tmux 中运行的 Claude 进程命令行提取 session_id
    if !agent.tmux_session.is_empty() {
        if let Ok(sid) = detect_session_from_tmux(&agent.tmux_session) {
            eprintln!("🔍 从 tmux 进程中检测到 session_id: {}", sid);
            return Ok(sid);
        }
    }

    // 策略 3: 匹配项目目录下最近修改的 .jsonl 文件
    if let Ok(sid) = find_most_recent_session(project_path) {
        eprintln!("📂 使用最近修改的会话文件: {}", sid);
        return Ok(sid);
    }

    Err(anyhow!(
        "无法找到 agent \"{}\" 的 Claude 会话 ID\n\
         可能原因:\n\
         1. agent 尚未完成初始化（session_start hook 未触发）\n\
         2. agent 不是 Claude Code 类型\n\
         3. 会话文件已被清理",
        agent.agent_id
    ))
}

/// 从 tmux session 中的 Claude 进程提取 session_id
fn detect_session_from_tmux(tmux_session: &str) -> Result<String> {
    // 获取 tmux session 中的进程 PID
    let output = std::process::Command::new("tmux")
        .args(["list-panes", "-t", tmux_session, "-F", "#{pane_pid}"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!("tmux list-panes 失败"));
    }

    let pane_pid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();
    if pane_pid.is_empty() {
        return Err(anyhow!("无法获取 pane PID"));
    }

    // 递归查找子进程中的 claude 进程
    let ps_output = std::process::Command::new("pgrep")
        .args(["-P", &pane_pid, "-a"])
        .output()?;

    let ps_str = String::from_utf8_lossy(&ps_output.stdout);

    // 在进程参数中查找 session_id (UUID 格式)
    // Claude 进程可能包含 --resume <session_id> 或内部使用 session_id
    let uuid_re = regex::Regex::new(
        r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
    )?;

    // 先在 pgrep 输出中找
    if let Some(m) = uuid_re.find(&ps_str) {
        return Ok(m.as_str().to_string());
    }

    // 也检查 lsof 找到正在写入的 .jsonl 文件
    let lsof_output = std::process::Command::new("lsof")
        .args(["-p", &pane_pid, "-Fn"])
        .output();

    if let Ok(lsof) = lsof_output {
        let lsof_str = String::from_utf8_lossy(&lsof.stdout);
        for line in lsof_str.lines() {
            if line.contains(".jsonl") {
                if let Some(m) = uuid_re.find(line) {
                    return Ok(m.as_str().to_string());
                }
            }
        }
    }

    Err(anyhow!("在 tmux 进程中未找到 session_id"))
}

/// 查找项目目录下最近修改的 .jsonl 文件
fn find_most_recent_session(project_path: &str) -> Result<String> {
    let project_dir = get_claude_project_dir(project_path)?;
    if !project_dir.exists() {
        return Err(anyhow!(
            "Claude 项目目录不存在: {}",
            project_dir.display()
        ));
    }

    let uuid_re = regex::Regex::new(
        r"^([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.jsonl$",
    )?;

    let mut best: Option<(String, std::time::SystemTime)> = None;

    for entry in std::fs::read_dir(&project_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(caps) = uuid_re.captures(&name) {
            let sid = caps[1].to_string();
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if best.as_ref().map_or(true, |(_, t)| modified > *t) {
                        best = Some((sid, modified));
                    }
                }
            }
        }
    }

    best.map(|(sid, _)| sid)
        .ok_or_else(|| anyhow!("项目目录下没有 .jsonl 会话文件"))
}

/// 将项目路径转换为 Claude Code 的项目目录路径
/// /Users/carve/project/foo -> ~/.claude/projects/-Users-carve-project-foo/
fn get_claude_project_dir(project_path: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("无法获取 HOME 目录"))?;

    // Claude Code 的目录命名规则: 将路径中的 / 替换为 -
    let dir_name = project_path.replace('/', "-");

    let dir = home.join(".claude").join("projects").join(&dir_name);
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_claude_project_dir() {
        let dir = get_claude_project_dir("/Users/carve/project/code-agent-monitor").unwrap();
        assert!(dir
            .to_string_lossy()
            .contains("-Users-carve-project-code-agent-monitor"));
    }
}
