# OpenClaw Webhook 研究计划

## Goal
研究 CAM 如何使用 OpenClaw Webhook 触发 Agent，替代当前有 Bug 的 CLI 方案。

## Background
- 当前使用 `openclaw system event` CLI 有 Bug #14527
- Webhook (`POST /hooks/agent` 或 `/hooks/wake`) 不受此 Bug 影响
- 需要研究如何在 CAM 中实现 Webhook 调用

## Research Questions
1. OpenClaw Webhook API 的认证机制是什么？
2. 如何通过 Webhook 触发 Agent 并传递 CAM 事件数据？
3. CAM 端应该如何实现 HTTP 调用？
4. CLI vs Webhook 各有什么优劣？

## Phases

### Phase 1: OpenClaw Webhook API 研究 `pending`
- [ ] 分析 OpenClaw Webhook 端点 (`/hooks/agent`, `/hooks/wake`)
- [ ] 研究认证机制（API Key、Token、Cookie 等）
- [ ] 了解请求格式和必需参数
- [ ] 研究响应格式和错误处理

### Phase 2: CAM 当前实现分析 `pending`
- [ ] 分析 CAM 当前如何调用 OpenClaw CLI
- [ ] 了解传递的事件数据结构
- [ ] 识别需要迁移的功能点

### Phase 3: 实现方案设计 `pending`
- [ ] 设计 HTTP 调用方案
- [ ] 考虑认证配置管理
- [ ] 设计错误处理和重试机制
- [ ] 对比 CLI vs Webhook 优劣

### Phase 4: 汇总报告 `pending`
- [ ] 整合研究发现
- [ ] 提供实现建议
- [ ] 列出潜在风险和注意事项

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|

## Files Modified
- docs/research/webhook-research-plan.md (created)
- docs/research/findings.md (to create)
