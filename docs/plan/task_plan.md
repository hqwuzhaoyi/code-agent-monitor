# Task Plan: 修复通知内容缺少上下文问题

## Goal
修复当 Claude Code 显示"这部分结构看起来合适吗？"这类问题时，通知只显示问题本身，缺少必要的上下文信息的问题。

## Problem Description
当前通知格式：
```
⏸️ workspace 等待输入

这部分结构看起来合适吗？

回复内容 cam-1770600296
```

问题：用户无法知道"这部分结构"指的是什么，缺少上下文。

## Phases

### Phase 1: 分析当前逻辑 [pending]
- [ ] 分析 `format_notification` 函数
- [ ] 分析 `clean_terminal_context` 函数
- [ ] 理解当前问题提取逻辑

### Phase 2: 查找类似案例 [pending]
- [ ] 搜索 GitHub issues 中类似问题
- [ ] 查看其他通知系统如何处理上下文
- [ ] 收集测试用例

### Phase 3: 设计解决方案 [pending]
- [ ] 确定需要保留的上下文类型
- [ ] 设计上下文提取算法
- [ ] 考虑通知长度限制

### Phase 4: 实现修复 [pending]
- [ ] 修改 `clean_terminal_context` 函数
- [ ] 添加上下文提取逻辑
- [ ] 处理边界情况

### Phase 5: 测试验证 [pending]
- [ ] 添加单元测试
- [ ] 端到端测试
- [ ] 验证各种场景

## Success Criteria
1. 通知包含足够的上下文让用户理解问题
2. 通知不会过长（保持简洁）
3. 所有现有测试通过
4. 新增测试覆盖边界情况

## Files to Modify
- `src/openclaw_notifier.rs` - 主要修改文件

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| (none yet) | | |
