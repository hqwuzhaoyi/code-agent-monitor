# 通知内容丢失问题

## 问题描述

2026-02-10 测试发现：当 Claude Code 显示多行选项时，通知内容不完整。

### 症状

1. **Telegram 消息缺少选项**：
   ```
   ⏸️ workspace 等待输入

   这部分看起来对吗？

   回复内容
   ```

   期望显示：
   ```
   ⏸️ workspace 等待输入

   Todo 项需要哪些功能？
   1. 最小功能 - 添加、完成（勾选）、删除
   2. 稍多一点 - 添加、完成、删除 + 编辑任务文本
   3. 其他 - 请描述

   回复数字选择
   ```

2. **Dashboard payload 不完整**：
   - `terminal_snapshot` 只包含最后几行
   - 选项列表被截断
   - `summary` 是通用的 `等待用户输入`，没有具体问题

### 根本原因

1. **终端快照行数不足**：
   - `terminal_cleaner.rs` 的 `clean_terminal_context()` 提取行数有限
   - 当问题+选项跨越 10+ 行时，只捕获到最后的确认问题

2. **选项提取逻辑问题**：
   - `find_context_start()` 可能没有正确识别选项列表的起始位置
   - 多轮对话时，之前的选项可能干扰当前选项的提取

3. **噪音过滤过度**：
   - 状态栏、分隔线被过滤后，有效内容行数减少
   - 但行数限制是在过滤前应用的

### 影响范围

- Brainstorming 多选项场景
- 任何超过 5-6 行的问题内容
- Dashboard 和 Telegram 都受影响

## 修复方案

### 方案 A：增加终端快照行数

在 `main.rs` 的 `get_logs()` 中增加行数：
```rust
// 当前
let logs = tmux::get_logs(&agent_id, 30)?;

// 修改为
let logs = tmux::get_logs(&agent_id, 50)?;
```

### 方案 B：改进选项提取逻辑

在 `terminal_cleaner.rs` 中：
1. 先过滤噪音
2. 然后从过滤后的内容中提取选项
3. 确保选项列表完整

### 方案 C：智能内容提取

使用 AI（Haiku）提取关键内容：
1. 识别问题类型（选项/确认/开放式）
2. 提取完整的问题和选项
3. 生成结构化摘要

## 验证方法

```bash
# 1. 启动 brainstorming agent
openclaw agent --agent main --message "使用 cam_agent_start 在 /tmp 启动 Claude Code，prompt 为：使用 brainstorm 创建一个项目"

# 2. 等待出现多选项问题

# 3. 测试 dry-run 查看通知内容
echo '{"notification_type": "idle_prompt", "cwd": "/tmp"}' | \
  ./target/release/cam notify --event notification --agent-id <agent_id> --dry-run

# 4. 检查是否包含完整选项
```

## 优先级

**中等** - 影响用户体验，但不阻塞核心功能

## 相关文件

- `src/notification/terminal_cleaner.rs` - 终端内容清理
- `src/notification/formatter.rs` - 消息格式化
- `src/main.rs` - 终端快照获取
- `src/openclaw_notifier.rs` - 通知发送

## 下一步

在下一个 sprint 中修复，优先考虑方案 A + B 组合。
