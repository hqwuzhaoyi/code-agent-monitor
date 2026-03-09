//! 钉钉消息推送模块
//!
//! 支持两种推送方式：
//! - **群消息**：通过 `groupMessages/send` 发送到群聊（用于 `cam summary`）
//! - **单聊消息**：通过 `oToMessages/batchSend` 发送到个人（用于 watcher 实时通知）
//!
//! 配置 (`~/.config/code-agent-monitor/config.json`):
//! ```json
//! {
//!   "dingtalk": {
//!     "app_key": "ding4k1twyu7oerlrt28",
//!     "app_secret": "xxx",
//!     "robot_code": "ding4k1twyu7oerlrt28",
//!     "conversation_id": "cidwyGvTF2ku0/OQHdhFbp5Tw==",
//!     "user_ids": ["1024524"]
//!   }
//! }
//! ```

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingtalkConfig {
    pub app_key: String,
    pub app_secret: String,
    /// 机器人 robotCode，通常和 app_key 相同
    pub robot_code: String,
    /// 目标群的 openConversationId（群消息用）
    pub conversation_id: String,
    /// 单聊目标用户 ID 列表（单聊用）
    #[serde(default)]
    pub user_ids: Vec<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct SendResponse {
    #[serde(rename = "processQueryKey")]
    process_query_key: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct OtoSendResponse {
    #[serde(rename = "processQueryKey")]
    process_query_key: Option<String>,
    #[serde(rename = "invalidStaffIdList", default)]
    invalid_staff_id_list: Vec<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// 从 config.json 加载钉钉配置
pub fn load_dingtalk_config() -> Option<DingtalkConfig> {
    let config_path = dirs::home_dir()?
        .join(".config")
        .join("code-agent-monitor")
        .join("config.json");

    if !config_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let dt = json.get("dingtalk")?;

    let user_ids = dt
        .get("user_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Some(DingtalkConfig {
        app_key: dt.get("app_key")?.as_str()?.to_string(),
        app_secret: dt.get("app_secret")?.as_str()?.to_string(),
        robot_code: dt
            .get("robot_code")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| dt.get("app_key").and_then(|v| v.as_str()).unwrap_or(""))
            .to_string(),
        conversation_id: dt.get("conversation_id")?.as_str()?.to_string(),
        user_ids,
    })
}

/// 获取钉钉 access_token
fn get_access_token(config: &DingtalkConfig) -> Result<String, String> {
    use std::time::Duration;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let body = serde_json::json!({
        "appKey": config.app_key,
        "appSecret": config.app_secret,
    });

    let resp: TokenResponse = client
        .post("https://api.dingtalk.com/v1.0/oauth2/accessToken")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("获取 access_token 请求失败: {}", e))?
        .json()
        .map_err(|e| format!("解析 access_token 响应失败: {}", e))?;

    resp.access_token.ok_or_else(|| {
        format!(
            "获取 access_token 失败: {} - {}",
            resp.code.unwrap_or_default(),
            resp.message.unwrap_or_default()
        )
    })
}

/// 发送文本消息到钉钉群
pub fn send_to_group(config: &DingtalkConfig, message: &str) -> Result<(), String> {
    use std::time::Duration;

    let token = get_access_token(config)?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let msg_param = serde_json::json!({ "content": message }).to_string();

    let body = serde_json::json!({
        "robotCode": config.robot_code,
        "openConversationId": config.conversation_id,
        "msgKey": "sampleText",
        "msgParam": msg_param,
    });

    let resp: SendResponse = client
        .post("https://api.dingtalk.com/v1.0/robot/groupMessages/send")
        .header("Content-Type", "application/json")
        .header("x-acs-dingtalk-access-token", &token)
        .json(&body)
        .send()
        .map_err(|e| format!("发送钉钉群消息失败: {}", e))?
        .json()
        .map_err(|e| format!("解析钉钉响应失败: {}", e))?;

    if resp.process_query_key.is_some() {
        Ok(())
    } else {
        Err(format!(
            "钉钉群消息发送失败: {} - {}",
            resp.code.unwrap_or_default(),
            resp.message.unwrap_or_default()
        ))
    }
}

/// 发送单聊消息到指定用户
pub fn send_to_user(config: &DingtalkConfig, message: &str) -> Result<(), String> {
    use std::time::Duration;

    if config.user_ids.is_empty() {
        return Err("未配置 user_ids".to_string());
    }

    let token = get_access_token(config)?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let msg_param = serde_json::json!({ "content": message }).to_string();

    let body = serde_json::json!({
        "robotCode": config.robot_code,
        "userIds": config.user_ids,
        "msgKey": "sampleText",
        "msgParam": msg_param,
    });

    let resp: OtoSendResponse = client
        .post("https://api.dingtalk.com/v1.0/robot/oToMessages/batchSend")
        .header("Content-Type", "application/json")
        .header("x-acs-dingtalk-access-token", &token)
        .json(&body)
        .send()
        .map_err(|e| format!("发送钉钉单聊消息失败: {}", e))?
        .json()
        .map_err(|e| format!("解析钉钉响应失败: {}", e))?;

    if resp.process_query_key.is_some() {
        if !resp.invalid_staff_id_list.is_empty() {
            warn!(invalid = ?resp.invalid_staff_id_list, "部分用户 ID 无效");
        }
        Ok(())
    } else {
        Err(format!(
            "钉钉单聊发送失败: {} - {}",
            resp.code.unwrap_or_default(),
            resp.message.unwrap_or_default()
        ))
    }
}

/// 兼容旧接口：发送到群
pub fn send_to_dingtalk(config: &DingtalkConfig, message: &str) -> Result<(), String> {
    send_to_group(config, message)
}

/// 尝试发送到钉钉群，失败只 warn 不阻断（cam summary 用）
pub fn try_send_to_dingtalk(message: &str) {
    if let Some(config) = load_dingtalk_config() {
        if let Err(e) = send_to_group(&config, message) {
            warn!(error = %e, "钉钉群消息发送失败");
        }
    }
}

/// 尝试发送单聊通知，失败只 warn 不阻断（watcher 用）
pub fn try_send_to_user(message: &str) {
    if let Some(config) = load_dingtalk_config() {
        if config.user_ids.is_empty() {
            return;
        }
        info!(user_ids = ?config.user_ids, "发送钉钉单聊通知");
        match send_to_user(&config, message) {
            Ok(()) => info!("钉钉单聊通知发送成功"),
            Err(e) => warn!(error = %e, "钉钉单聊消息发送失败"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config_missing_file() {
        let config = DingtalkConfig {
            app_key: "test_key".to_string(),
            app_secret: "test_secret".to_string(),
            robot_code: "test_key".to_string(),
            conversation_id: "cid_test".to_string(),
            user_ids: vec!["123".to_string()],
        };
        assert_eq!(config.app_key, "test_key");
        assert_eq!(config.robot_code, "test_key");
        assert_eq!(config.user_ids, vec!["123"]);
    }

    #[test]
    #[ignore] // 真实 API 调用，手动运行
    fn test_send_oto_real() {
        let config = load_dingtalk_config().expect("dingtalk config not found");
        eprintln!("config: app_key={}, user_ids={:?}", config.app_key, config.user_ids);
        assert!(!config.user_ids.is_empty(), "user_ids is empty");
        send_to_user(&config, "CAM Rust 集成测试 - send_to_user 直接调用").unwrap();
        eprintln!("send_to_user succeeded");
    }

    #[test]
    fn test_config_empty_user_ids() {
        let config = DingtalkConfig {
            app_key: "k".to_string(),
            app_secret: "s".to_string(),
            robot_code: "k".to_string(),
            conversation_id: "c".to_string(),
            user_ids: vec![],
        };
        assert!(config.user_ids.is_empty());
    }
}
