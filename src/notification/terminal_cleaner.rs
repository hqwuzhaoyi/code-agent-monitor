//! 终端输出清理模块 - 从终端快照中提取有意义的内容
//!
//! 主要功能：
//! - 移除终端噪音（状态栏、分隔线、进度指示器等）
//! - 提取问题和选项内容
//! - 保留开放式问题的上下文
//!
//! 设计原则：
//! 1. 保留用户需要看到的内容（问题、选项、代码块）
//! 2. 移除干扰信息（状态栏、工具调用状态、分隔线）
//! 3. 智能识别问题和选项的位置关系

use std::sync::LazyLock;
use regex::Regex;

/// Compiled noise patterns for terminal context cleaning
pub static NOISE_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // Status bar (MCPs, hooks, %, timer, context window)
        r"(?m)^.*\d+\s*MCPs.*$",
        r"(?m)^.*\d+\s*hooks.*$",
        r"(?m)^.*\d+%.*context.*$",
        r"(?m)^.*⏱️.*$",
        r"(?m)^.*\[Opus.*\].*$",
        r"(?m)^.*git:\(.*\).*$",
        // Separator lines
        r"(?m)^[─━═\-]{3,}$",
        // Empty lines and standalone prompts
        r"(?m)^[>❯]\s*$",
        r"(?m)^\s*$",
        // Direct marker
        r"(?m)^.*📡\s*via\s*direct.*$",
        // Claude Code frame lines (only pure frame chars, not directory trees)
        r"(?m)^[╭╮╰╯][─━═\s]*[╭╮╰╯]?$",
        r"(?m)^│[^├└│]*│$",
        // Tool call status and thinking status
        r"(?m)^.*[✓◐⏺✻✶✽].*$",
        // Claude Code thinking/generating status
        r"(?m)^.*Brewing.*$",
        r"(?m)^.*Thinking.*$",
        r"(?m)^.*Actioning.*$",
    ]
    .iter()
    .filter_map(|p| Regex::new(p).ok())
    .collect()
});

/// 清洗终端上下文，移除噪音内容，只保留最近的问题和选项
pub fn clean_terminal_context(raw: &str) -> String {
    let raw_lines: Vec<&str> = raw.lines().collect();

    // 第一步：找到处理起始位置（跳过已回答的问题）
    let start_idx = find_content_start_index(&raw_lines);
    let content_to_process = raw_lines[start_idx..].join("\n");

    // 第二步：应用噪音模式过滤
    let filtered = apply_noise_filters(&content_to_process);

    // 第三步：移除空行，获取有效行
    let lines: Vec<&str> = filtered.lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if lines.is_empty() {
        return String::new();
    }

    // 第四步：查找问题和选项位置
    let last_question_idx = find_last_question_index(&lines);
    let option_groups = find_option_groups(&lines);
    let (first_option_idx, last_option_idx) = option_groups.last()
        .map(|(s, e)| (Some(*s), Some(*e)))
        .unwrap_or((None, None));

    // 第五步：查找与选项相关的问题
    let relevant_question_idx = find_relevant_question_index(
        &lines, first_option_idx, last_option_idx, last_question_idx
    );

    // 第六步：根据问题和选项的位置关系决定返回内容
    extract_final_content(&lines, relevant_question_idx, first_option_idx, last_option_idx)
}

/// 找到内容处理的起始位置（跳过已回答的问题，但保留当前问题的上下文）
fn find_content_start_index(raw_lines: &[&str]) -> usize {
    let last_user_input_idx = find_last_user_input_index(raw_lines);

    if let Some(last_input_idx) = last_user_input_idx {
        // 向前查找最近的问题行（最多 10 行）
        let search_start = last_input_idx.saturating_sub(10);
        for i in (search_start..last_input_idx).rev() {
            if is_question_line(raw_lines[i]) {
                return i;
            }
        }
        // 如果找不到问题行，从用户输入后开始
        last_input_idx + 1
    } else {
        0
    }
}

/// 找到最后一个用户输入行的索引
fn find_last_user_input_index(raw_lines: &[&str]) -> Option<usize> {
    let mut last_user_input_idx = None;
    for (i, line) in raw_lines.iter().enumerate() {
        if is_user_input_line(line) {
            last_user_input_idx = Some(i);
        }
    }
    last_user_input_idx
}

/// 判断是否为用户输入行（❯ <content>，content 不为空）
fn is_user_input_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with('❯') && trimmed.len() > 2 {
        let after_prompt = trimmed[3..].trim();
        !after_prompt.is_empty() && !after_prompt.starts_with("Try \"")
    } else {
        false
    }
}

/// 判断是否为问题行
pub fn is_question_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.contains('?') || trimmed.contains('？')
        || trimmed.ends_with(':') || trimmed.ends_with('：')
        || trimmed.contains("[Y]es") || trimmed.contains("[Y/n]")
        || trimmed.contains("[y/N]") || trimmed.contains("[是/否]")
}

/// 应用噪音过滤模式
fn apply_noise_filters(content: &str) -> String {
    let mut result = content.to_string();
    for re in NOISE_PATTERNS.iter() {
        result = re.replace_all(&result, "").to_string();
    }
    result
}

/// 查找最后一个问题/提示行的索引
fn find_last_question_index(lines: &[&str]) -> Option<usize> {
    let mut last_question_idx = None;
    for (i, line) in lines.iter().enumerate() {
        if is_question_line(line) {
            last_question_idx = Some(i);
        }
    }
    last_question_idx
}

/// 查找所有选项组，返回每组选项的 (起始索引, 结束索引)
fn find_option_groups(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut option_groups: Vec<(usize, usize)> = Vec::new();
    let mut current_group_start: Option<usize> = None;
    let mut current_group_end: Option<usize> = None;
    let mut last_option_num: Option<u32> = None;

    for (i, line) in lines.iter().enumerate() {
        let option_num = extract_option_number(line);

        if let Some(num) = option_num {
            let is_new_group = last_option_num.map(|last| num <= last).unwrap_or(false);

            if is_new_group && current_group_start.is_some() {
                if let (Some(start), Some(end)) = (current_group_start, current_group_end) {
                    option_groups.push((start, end));
                }
                current_group_start = Some(i);
                current_group_end = Some(i);
            } else if current_group_start.is_none() {
                current_group_start = Some(i);
                current_group_end = Some(i);
            } else {
                current_group_end = Some(i);
            }
            last_option_num = Some(num);
        } else if current_group_start.is_some() {
            if let (Some(start), Some(end)) = (current_group_start, current_group_end) {
                option_groups.push((start, end));
            }
            current_group_start = None;
            current_group_end = None;
            last_option_num = None;
        }
    }

    if let (Some(start), Some(end)) = (current_group_start, current_group_end) {
        option_groups.push((start, end));
    }

    option_groups
}

/// 从行中提取选项编号（选项行格式：数字 + "." + 内容）
fn extract_option_number(line: &str) -> Option<u32> {
    let trimmed = line.trim();
    if let Some(first_char) = trimmed.chars().next() {
        if first_char.is_ascii_digit() && trimmed.contains('.') {
            return trimmed.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .ok();
        }
    }
    None
}

/// 查找与选项相关的问题行索引
fn find_relevant_question_index(
    lines: &[&str],
    first_option_idx: Option<usize>,
    last_option_idx: Option<usize>,
    last_question_idx: Option<usize>,
) -> Option<usize> {
    if let (Some(first_opt), Some(last_opt)) = (first_option_idx, last_option_idx) {
        let before_idx = find_question_before(lines, first_opt);
        let after_idx = find_question_after(lines, last_opt);
        after_idx.or(before_idx)
    } else {
        last_question_idx
    }
}

/// 在指定位置之前查找问题行
fn find_question_before(lines: &[&str], before_idx: usize) -> Option<usize> {
    (0..before_idx).rev().find(|&i| is_question_line(lines[i]))
}

/// 在指定位置之后查找问题行
fn find_question_after(lines: &[&str], after_idx: usize) -> Option<usize> {
    ((after_idx + 1)..lines.len()).find(|&i| is_question_line(lines[i]))
}

/// 根据问题和选项的位置关系提取最终内容
fn extract_final_content(
    lines: &[&str],
    question_idx: Option<usize>,
    first_option_idx: Option<usize>,
    last_option_idx: Option<usize>,
) -> String {
    match (question_idx, first_option_idx, last_option_idx) {
        (Some(q_idx), Some(first_opt), Some(last_opt)) => {
            if q_idx < first_opt {
                lines[q_idx..=last_opt].join("\n")
            } else if q_idx > last_opt {
                lines[first_opt..=q_idx].join("\n")
            } else {
                lines[first_opt..=q_idx.max(last_opt)].join("\n")
            }
        }
        (Some(q_idx), None, None) => {
            let context_start = find_context_start(lines, q_idx);
            lines[context_start..].join("\n")
        }
        (None, Some(first_opt), Some(last_opt)) => {
            lines[first_opt..=last_opt].join("\n")
        }
        _ => lines.join("\n")
    }
}

/// 查找问题前上下文的起始位置
///
/// 对于开放式问题（如"这部分结构看起来合适吗？"），需要保留问题前的相关上下文。
/// 上下文包括：代码块、目录结构、设计说明等。
///
/// 策略：
/// 1. 从问题行向前查找，直到遇到分隔符（---）或用户输入（❯）
/// 2. 最多保留 15 行上下文（避免通知过长）
/// 3. 如果找到代码块/目录结构，保留完整块
pub fn find_context_start(lines: &[&str], question_idx: usize) -> usize {
    const MAX_CONTEXT_LINES: usize = 15;

    // 最早可能的起始位置
    let earliest_start = question_idx.saturating_sub(MAX_CONTEXT_LINES);

    // 从问题行向前查找
    let mut context_start = question_idx;

    for i in (earliest_start..question_idx).rev() {
        let trimmed = lines[i].trim();

        // 遇到分隔符，停止（不包含分隔符）
        if trimmed == "---" || trimmed.starts_with("───") {
            break;
        }

        // 遇到用户输入行（❯ 后跟内容），停止（不包含用户输入）
        if trimmed.starts_with('❯') && trimmed.len() > 2 {
            break;
        }

        // 遇到 agent 响应开始（⏺），停止（不包含）
        if trimmed.starts_with('⏺') {
            break;
        }

        // 更新起始位置
        context_start = i;
    }

    context_start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_terminal_context() {
        // 测试：保留选项和问题（Claude Code 格式：选项在前，问题在后）
        let raw = "Old content\n─────────────\n> \n📡 via direct\n1. Option one\n2. Option two\nActual question?";
        let cleaned = clean_terminal_context(raw);
        // 应该保留选项和问题
        assert!(cleaned.contains("Actual question?"));
        assert!(cleaned.contains("1. Option one"));
        assert!(cleaned.contains("2. Option two"));
        assert!(!cleaned.contains("─────"));
        assert!(!cleaned.contains("📡 via direct"));
        // Old content 应该被过滤掉（因为在选项之前）
        assert!(!cleaned.contains("Old content"));
    }

    #[test]
    fn test_clean_terminal_context_real_output() {
        // 测试实际的 Claude Code 终端输出
        let raw = r#"  1. 核心功能 - 添加、删除、标记完成/未完成
  2. 筛选功能 - 全部/已完成/未完成 切换显示
  3. 编辑功能 - 双击编辑任务标题
  4. 清空已完成 - 一键删除所有已完成任务

  推荐选 1 和 2，保持简单实用。你想要哪些？

❯ 1

⏺ 好的，只保留核心功能：添加、删除、标记完成。

  我现在对需求有清晰的理解了，让我呈现设计方案。

  ---
  设计方案 - 第一部分：项目结构

  react-todo/
  ├── src/
  │   ├── components/
  │   │   ├── TodoInput.tsx      # 输入框组件
  │   │   ├── TodoItem.tsx       # 单个任务项
  │   │   └── TodoList.tsx       # 任务列表容器
  │   ├── hooks/
  │   │   └── useTodos.ts        # Todo 逻辑 + localStorage 持久化
  │   ├── types/
  │   │   └── todo.ts            # Todo 类型定义
  │   ├── App.tsx                # 主应用组件
  │   ├── main.tsx               # 入口文件
  │   └── index.css              # Tailwind 入口
  ├── index.html
  ├── package.json
  ├── tailwind.config.js
  ├── tsconfig.json
  └── vite.config.ts

  核心设计决策：
  - 使用自定义 Hook useTodos 封装所有状态逻辑和 localStorage 操作
  - 组件保持纯展示，逻辑集中在 Hook 中
  - 扁平结构，不过度拆分

  这个结构看起来合适吗？

────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
❯
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  [Opus 4.6] ███░░░░░░░ 27% | ⏱️  1h 44m
  workspace git:(main*)
  2 MCPs | 5 hooks
  ✓ Skill ×1 | ✓ Bash ×1"#;

        let cleaned = clean_terminal_context(raw);
        println!("=== Cleaned output ===");
        println!("{}", cleaned);
        println!("=== End ===");

        // 应该包含最后一个问题
        assert!(cleaned.contains("这个结构看起来合适吗？"), "Should contain the question");
    }

    #[test]
    fn test_clean_terminal_context_open_question_with_context() {
        // 测试开放式问题（无选项）保留前面的上下文
        let context = r#"❯ 1

⏺ 好的，保持最简单。

我现在对需求有了清晰的理解，让我分段呈现设计方案。

---
设计方案 - 第一部分：项目结构

react-todo/
├── src/
│   ├── components/
│   │   ├── TodoInput.tsx
│   │   ├── TodoItem.tsx
│   │   └── TodoList.tsx
│   ├── hooks/
│   │   └── useTodos.ts
│   └── App.tsx

设计思路：
- 组件职责单一
- 状态集中管理

这部分结构看起来合适吗？"#;

        let cleaned = clean_terminal_context(context);

        // 应该包含问题
        assert!(cleaned.contains("这部分结构看起来合适吗"), "Should contain the question");
        // 应该包含目录结构（上下文）
        assert!(cleaned.contains("react-todo/"), "Should contain directory structure");
        assert!(cleaned.contains("├── src/"), "Should contain tree structure");
        assert!(cleaned.contains("TodoInput.tsx"), "Should contain file names");
        // 应该包含设计说明
        assert!(cleaned.contains("设计方案"), "Should contain section title");
        // 不应该包含分隔符之前的内容
        assert!(!cleaned.contains("好的，保持最简单"), "Should NOT contain content before separator");
        assert!(!cleaned.contains("❯ 1"), "Should NOT contain user input");
    }

    #[test]
    fn test_clean_terminal_context_open_question_with_code_block() {
        // 测试开放式问题保留代码块上下文
        let context = r#"⏺ 修改后的代码：

fn main() {
    let items = vec![1, 2, 3];
    for item in items {
        println!("{}", item);
    }
}

这样修改可以吗？"#;

        let cleaned = clean_terminal_context(context);

        // 应该包含问题
        assert!(cleaned.contains("这样修改可以吗"), "Should contain the question");
        // 应该包含代码
        assert!(cleaned.contains("fn main()"), "Should contain code");
        assert!(cleaned.contains("println!"), "Should contain code content");
        // 不应该包含 agent 响应标记
        assert!(!cleaned.contains("⏺"), "Should NOT contain agent marker");
    }

    #[test]
    fn test_clean_terminal_context_open_question_max_lines() {
        // 测试上下文行数限制（最多 15 行）
        // 实际场景：有分隔符的情况下，从分隔符后开始
        let mut lines = Vec::new();
        // 添加早期内容
        for i in 1..=5 {
            lines.push(format!("Early line {}", i));
        }
        // 添加分隔符
        lines.push("---".to_string());
        // 添加 20 行内容（超过 15 行限制）
        for i in 1..=20 {
            lines.push(format!("Content line {}", i));
        }
        lines.push("这个方案可以吗？".to_string());

        let context = lines.join("\n");
        let cleaned = clean_terminal_context(&context);

        // 应该包含问题
        assert!(cleaned.contains("这个方案可以吗"), "Should contain the question");
        // 应该包含分隔符后的内容
        assert!(cleaned.contains("Content line 20"), "Should contain recent content");
        // 不应该包含分隔符之前的内容
        assert!(!cleaned.contains("Early line"), "Should NOT contain content before separator");
    }

    #[test]
    fn test_find_context_start_stops_at_separator() {
        // 测试 find_context_start 在分隔符处停止
        let lines = vec![
            "早期内容",
            "---",
            "设计方案",
            "代码结构",
            "这个可以吗？",
        ];

        let start = find_context_start(&lines, 4);

        // 应该从分隔符后开始（索引 2）
        assert_eq!(start, 2, "Should start after separator");
    }

    #[test]
    fn test_find_context_start_stops_at_user_input() {
        // 测试 find_context_start 在用户输入处停止
        let lines = vec![
            "之前的问题",
            "❯ 1",
            "新的内容",
            "代码结构",
            "这个可以吗？",
        ];

        let start = find_context_start(&lines, 4);

        // 应该从用户输入后开始（索引 2）
        assert_eq!(start, 2, "Should start after user input");
    }

    #[test]
    fn test_find_context_start_stops_at_agent_response() {
        // 测试 find_context_start 在 agent 响应处停止
        let lines = vec![
            "之前的内容",
            "⏺ 好的，我来处理",
            "新的设计",
            "代码结构",
            "这个可以吗？",
        ];

        let start = find_context_start(&lines, 4);

        // 应该从 agent 响应后开始（索引 2）
        assert_eq!(start, 2, "Should start after agent response");
    }

    #[test]
    fn test_clean_terminal_context_preserves_question_before_user_input() {
        // 测试修复：当用户已输入回复时，保留问题内容
        // 场景：Agent 问"这部分结构看起来合适吗？"，用户回复"y"
        // 修复前：问题会被丢弃，只剩下用户输入后的内容
        // 修复后：应该保留问题内容
        let context = r#"
这是一个设计方案：

1. 组件 A
2. 组件 B
3. 组件 C

这部分结构看起来合适吗？
❯ y
好的，我继续执行
❯ "#;

        let cleaned = clean_terminal_context(context);

        // 应该包含问题
        assert!(cleaned.contains("这部分结构看起来合适吗"),
            "Should preserve the question before user input. Got: {}", cleaned);
    }

    #[test]
    fn test_clean_terminal_context_preserves_confirmation_before_user_input() {
        // 测试修复：当用户已输入回复时，保留确认提示
        let context = r#"
Write to /tmp/test.txt?
[Y]es / [N]o / [A]lways / [D]on't ask
❯ y
File written successfully
❯ "#;

        let cleaned = clean_terminal_context(context);

        // 应该包含确认提示
        assert!(cleaned.contains("[Y]es") || cleaned.contains("Write to"),
            "Should preserve the confirmation prompt. Got: {}", cleaned);
    }

    #[test]
    fn test_is_question_line() {
        assert!(is_question_line("这个可以吗？"));
        assert!(is_question_line("Continue? [Y/n]"));
        assert!(is_question_line("请输入文件名:"));
        assert!(is_question_line("[Y]es / [N]o"));
        assert!(is_question_line("[是/否]"));
        assert!(!is_question_line("普通文本"));
        assert!(!is_question_line("1. 选项一"));
    }

    #[test]
    fn test_extract_option_number() {
        assert_eq!(extract_option_number("1. 选项一"), Some(1));
        assert_eq!(extract_option_number("  2. 选项二"), Some(2));
        assert_eq!(extract_option_number("10. 选项十"), Some(10));
        assert_eq!(extract_option_number("普通文本"), None);
        assert_eq!(extract_option_number("1 没有点"), None);
    }
}
