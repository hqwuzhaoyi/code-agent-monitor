# Codex detect_ready() 修复验证报告

## 验证日期
2026-02-25

## 修复内容
修改 `src/agent_mod/adapter/codex.rs` 中的 `detect_ready()` 方法，排除信任确认界面的误判。

## 验证结果

### 1. 编译验证 ✅
```
cargo build --release  # 成功
cargo test codex       # 19 passed, 0 failed
```

关键测试：
- `test_detect_ready_excludes_trust_dialog` - 验证信任对话框不被误判为就绪

### 2. E2E 验证 ✅

**测试场景**：新目录启动 Codex（触发信任确认）

**测试命令**：
```bash
rm -rf /tmp/codex-verify-test
mkdir -p /tmp/codex-verify-test
cam start --agent codex --cwd /tmp/codex-verify-test "brainstorm 创建笔记 web app"
```

**观察结果**：

| 阶段 | 预期行为 | 实际行为 | 状态 |
|------|----------|----------|------|
| 信任确认界面 | detect_ready() 返回 false | 30s 超时，prompt 未发送 | ✅ |
| 确认信任后 | Codex 进入就绪状态 | 显示输入提示符 | ✅ |
| 手动发送 prompt | Codex 正常处理 | 开始执行 brainstorm | ✅ |

### 3. 信任确认界面特征

终端快照显示的信任确认界面：
```
> You are in /private/tmp/codex-verify-test

  Do you trust the contents of this directory? Working with untrusted contents
  comes with higher risk of prompt injection.

› 1. Yes, continue
  2. No, quit

  Press enter to continue
```

修复后的 `detect_ready()` 正确识别此界面并返回 `false`。

## 结论

修复有效。信任确认界面不再被误判为就绪状态，避免了 prompt 在用户确认前被发送的问题。

## 后续建议

1. 考虑增加信任确认的自动处理选项（可选配置）
2. 30s 超时可能需要调整，或在检测到信任确认时给出提示
