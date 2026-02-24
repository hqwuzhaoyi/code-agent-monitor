# OpenClaw Webhook 研究发现

## OpenClaw Webhook API

### 端点信息
- **Base URL**: `http://localhost:9080/hooks` (默认端口 9080)
- **端点**:
  - `POST /hooks/wake` - 唤醒主 session
  - `POST /hooks/agent` - 运行 isolated agent

### 认证机制
```json
{
  "hooks": {
    "enabled": true,
    "token": "shared-secret",
    "path": "/hooks"
  }
}
```
- Header: `Authorization: Bearer <token>`
- 或 `x-openclaw-token: <token>`

### 请求格式

#### POST /hooks/wake
```json
{
  "text": "System line",
  "mode": "now"  // 或 "next-heartbeat"
}
```

#### POST /hooks/agent (推荐)
```json
{
  "message": "处理这个 CAM 事件",
  "name": "CAM",
  "agentId": "cam-handler",
  "wakeMode": "now",
  "deliver": true,
  "channel": "telegram",
  "to": "1440537501"
}
```

## CAM 当前实现

### CLI 调用方式
- 使用 `openclaw system event --text <payload> --mode now`
- 问题: 受 Bug #14527 影响，HEARTBEAT.md 为空时被跳过

### 事件数据结构
当前 CAM 发送的 SystemEventPayload:
```json
{
  "source": "cam",
  "version": "1.0",
  "agent_id": "cam-xxx",
  "event_type": "permission_request",
  "urgency": "HIGH",
  "project_path": "/path/to/project",
  "event_data": { "tool_name": "Bash", "tool_input": {...} },
  "context": { "risk_level": "MEDIUM" }
}
```

## 实现方案

### HTTP 调用设计
使用 reqwest 或 ureq 库发送 HTTP 请求:

```rust
async fn send_via_webhook(&self, payload: &serde_json::Value) -> Result<()> {
    let url = "http://localhost:9080/hooks/agent";
    
    let client = reqwest::Client::new();
    client.post(url)
        .header("Authorization", format!("Bearer {}", self.hook_token))
        .json(&WebhookPayload {
            message: format!("CAM Event: {:?}", payload),
            name: "CAM",
            agent_id: Some("cam-handler".to_string()),
            wake_mode: Some("now".to_string()),
            deliver: Some(true),
            channel: Some("telegram".to_string()),
            to: Some("1440537501".to_string()),
        })
        .send()
        .await?;
}
```

### 认证配置
需要用户配置:
1. 在 openclaw.json 中启用 hooks
2. 设置 hooks.token
3. 在 CAM 配置中提供 hook_token

### 错误处理
- 连接失败: 重试 3 次，指数退避
- 认证失败: 返回错误，不重试
- 超时: 30 秒超时

## CLI vs Webhook 对比

| 维度 | CLI | Webhook |
|------|-----|---------|
| 可靠性 | ❌ 受 Bug #14527 影响 | ✅ 不受影响 |
| 性能 | 快 (本地进程) | 稍慢 (HTTP) |
| 配置复杂度 | 无需配置 | 需要配置 hooks |
| 错误处理 | 简单 | 需要处理 HTTP 错误 |
| 调试便利性 | 容易 | 需要查看 HTTP 日志 |

## 建议和结论

**推荐方案: 使用 Webhook**

理由:
1. 不受 Bug #14527 影响
2. 可以直接控制 agent 行为
3. 支持更丰富的配置 (指定 agent、channel 等)

**实施步骤:**
1. 在 openclaw.json 中启用 hooks 并设置 token
2. 在 CAM 中添加 HTTP client 依赖
3. 实现 webhook 发送逻辑
4. 测试完整链路
