# 钉钉 AI Card 无文本内容问题调查报告

调查日期: 2026-02-26
调查人: Agent Team (log-analyzer, code-analyzer, config-checker, arch-analyst, cam-reviewer)

## 1. 问题描述

当用户通过钉钉发送消息后，如果 Agent 只执行工具调用（如 `cam_agent_status`、`cam_agent_send`）而没有产生文本回复，钉钉 AI Card 会显示：

```
[DingTalk] Skipping AI Card finalization because no textual content was produced.
```

导致用户看不到任何反馈。

## 2. 时间线分析

| 时间 | 事件 | 详情 |
|------|------|------|
| 04:01:04.295 | 收到钉钉消息 | from=吴兆毅, text="1" |
| 04:01:04.983 | 创建 AI Card | outTrackId=card_7f8da174-... |
| 04:01:05.965 | Agent 运行开始 | provider=openai, model=gpt-5.2 |
| 04:01:12.009 | 工具调用开始 | tool=cam_agent_status |
| 04:01:15.168 | 工具调用结束 | tool=cam_agent_status (~3.1s) |
| 04:01:21.987 | 工具调用开始 | tool=cam_agent_send |
| 04:01:22.096 | 工具调用结束 | tool=cam_agent_send (~0.1s) |
| 04:01:24.128 | Agent 运行结束 | isError=false |
| 04:01:24.505 | **跳过 Card 最终化** | 无文本内容 |

## 3. 根本原因

### 3.1 DingTalk 插件问题

文件: `~/.openclaw/extensions/dingtalk/src/inbound-handler.ts:454-459`

```typescript
} else {
  log?.debug?.(
    "[DingTalk] Skipping AI Card finalization because no textual content was produced.",
  );
  currentAICard.state = AICardStatus.FINISHED;
  currentAICard.lastUpdated = Date.now();
}
```

当 `lastCardContent` 和 `queuedFinal` 都为空时，跳过 `finishAICard()` 调用。

### 3.2 CAM 插件问题

文件: `/Users/admin/workspace/code-agent-monitor/plugins/cam/src/index.ts`

`cam_agent_send` 返回值过于简单：
```json
{"success": true}
```

缺乏上下文信息引导 Agent 生成有意义的回复。

## 4. 其他渠道处理方式

| 渠道 | 处理方式 | 有默认回复 |
|------|----------|------------|
| 飞书 | 无特殊处理 | ❌ |
| Telegram | 无 AI Card 概念 | N/A |
| MS Teams | 无特殊处理 | ❌ |
| OpenClaw 核心 | `SILENT_REPLY_TOKEN = "NO_REPLY"` | 用于明确不回复 |

**结论**: 钉钉的 AI Card 是特殊功能，其他渠道没有类似的"无文本内容"问题。

## 5. 修复方案

### 方案 A: DingTalk 插件添加默认回复（推荐）

```typescript
} else {
  const defaultContent = "✅ 操作已完成";
  log?.debug?.(
    "[DingTalk] No textual content produced, using default completion message.",
  );
  await finishAICard(currentAICard, defaultContent, log);
}
```

**文案选项**:
- `✅ 操作已完成` - 简洁明了
- `✅ 已处理` - 更简短
- `🤖 任务已执行` - 强调机器人执行
- `📋 工具调用已完成` - 更详细

### 方案 B: CAM 插件增强返回值

```json
{
  "success": true,
  "agent_id": "cam-xxx",
  "input_sent": "1",
  "message": "已向 Agent cam-xxx 发送输入 '1'，Agent 正在处理中。"
}
```

### 方案 C: 组合方案

同时实施 A 和 B，A 作为兜底，B 作为改进。

## 6. 责任归属

| 组件 | 责任 | 说明 |
|------|------|------|
| DingTalk 插件 | **主要** | 没有处理边界情况 |
| CAM 插件 | **次要** | 返回值不够丰富 |
| Agent 配置 | 无 | 这是代码层面问题 |

## 7. 待讨论

- [ ] 默认回复文案选择
- [ ] 是否需要配置化默认文案
- [ ] 是否需要区分不同工具类型

## 8. 相关文件

- `~/.openclaw/extensions/dingtalk/src/inbound-handler.ts` - DingTalk 回复处理
- `/Users/admin/workspace/code-agent-monitor/plugins/cam/src/index.ts` - CAM OpenClaw 插件
- `/Users/admin/workspace/code-agent-monitor/src/mcp_mod/server.rs` - CAM MCP 服务器
- `/tmp/openclaw/openclaw-2026-02-26.log` - 问题日志
