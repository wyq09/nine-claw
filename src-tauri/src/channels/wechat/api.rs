use base64::{engine::general_purpose::STANDARD as BASE64_ENGINE, Engine as Base64Engine};
use rand::Rng;
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

use super::types::*;

/// Timeout for each getUpdates request. Keep short for reliable periodic polling.
const GET_UPDATES_TIMEOUT_MS: u64 = 8_000;
const API_TIMEOUT_MS: u64 = 15_000;

/// HTTP client for the iLink Bot API (async).
#[derive(Clone)]
pub struct WeChatApi {
    client: reqwest::Client,
    base_url: String,
    token: String,
    route_tag: Option<String>,
}

impl WeChatApi {
    pub fn new(base_url: &str, token: &str, route_tag: Option<&str>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            token: token.to_string(),
            route_tag: route_tag.map(|s| s.to_string()),
        }
    }

    fn random_uin() -> String {
        let mut rng = rand::thread_rng();
        let uint_val: u32 = rng.gen();
        BASE64_ENGINE.encode(uint_val.to_string().as_bytes())
    }

    fn build_headers(&self, body_len: usize) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("AuthorizationType", "ilink_bot_token".parse().unwrap());
        if !self.token.is_empty() {
            headers.insert(
                "Authorization",
                format!("Bearer {}", self.token).parse().unwrap(),
            );
        }
        headers.insert("Content-Length", body_len.to_string().parse().unwrap());
        headers.insert("X-WECHAT-UIN", Self::random_uin().parse().unwrap());
        if let Some(tag) = &self.route_tag {
            headers.insert("SKRouteTag", tag.parse().unwrap());
        }
        headers
    }

    /// Long-poll for new messages.
    pub async fn get_updates(&self, get_updates_buf: &str) -> Result<GetUpdatesResp, String> {
        let url = format!("{}/ilink/bot/getupdates", self.base_url);

        let body = json!({
            "get_updates_buf": get_updates_buf,
            "base_info": { "channel_version": CHANNEL_VERSION }
        });
        let body_str = serde_json::to_string(&body).map_err(|e| format!("序列化失败: {e}"))?;

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers(body_str.len()))
            .body(body_str)
            .timeout(Duration::from_millis(GET_UPDATES_TIMEOUT_MS))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    return "timeout".to_string();
                }
                format!("getUpdates 请求失败: {e}")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("getUpdates HTTP {status}: {text}"));
        }

        response
            .json::<GetUpdatesResp>()
            .await
            .map_err(|e| format!("解析 getUpdates 响应失败: {e}"))
    }

    /// Send a text message to a user.
    /// All "ghost fields" (from_user_id, client_id, message_type, message_state)
    /// are REQUIRED by the iLink protocol — without them the API returns 200 but
    /// silently drops the message.
    pub async fn send_message(
        &self,
        to_user_id: &str,
        content: &str,
        context_token: Option<&str>,
    ) -> Result<(), String> {
        self.send_message_with_state(to_user_id, content, context_token, MSG_STATE_FINISH)
            .await
    }

    /// Send a text message with explicit `message_state`.
    pub async fn send_message_with_state(
        &self,
        to_user_id: &str,
        content: &str,
        context_token: Option<&str>,
        message_state: i32,
    ) -> Result<(), String> {
        let url = format!("{}/ilink/bot/sendmessage", self.base_url);
        let client_id = format!("nineclaw-{}", &Uuid::new_v4().to_string()[..12]);

        let body = json!({
            "msg": {
                "from_user_id": "",
                "to_user_id": to_user_id,
                "client_id": client_id,
                "message_type": MSG_TYPE_BOT,
                "message_state": message_state,
                "item_list": [{
                    "type": MSG_ITEM_TYPE_TEXT,
                    "text_item": { "text": content }
                }],
                "context_token": context_token,
            },
            "base_info": { "channel_version": CHANNEL_VERSION }
        });

        let body_str = serde_json::to_string(&body).map_err(|e| format!("序列化失败: {e}"))?;

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers(body_str.len()))
            .body(body_str)
            .timeout(Duration::from_millis(API_TIMEOUT_MS))
            .send()
            .await
            .map_err(|e| format!("sendMessage 请求失败: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("sendMessage HTTP {status}: {text}"));
        }
        Ok(())
    }

    /// Send a media message (image / file / video).
    pub async fn send_media_message(
        &self,
        to_user_id: &str,
        item_type: i32,
        item_json: serde_json::Value,
        context_token: Option<&str>,
    ) -> Result<(), String> {
        let url = format!("{}/ilink/bot/sendmessage", self.base_url);
        let client_id = format!("nineclaw-{}", &Uuid::new_v4().to_string()[..12]);

        let mut item = serde_json::Map::new();
        item.insert("type".to_string(), json!(item_type));
        // Merge the media-specific fields into the item object.
        if let serde_json::Value::Object(inner) = item_json {
            for (k, v) in inner {
                item.insert(k, v);
            }
        }

        let body = json!({
            "msg": {
                "from_user_id": "",
                "to_user_id": to_user_id,
                "client_id": client_id,
                "message_type": MSG_TYPE_BOT,
                "message_state": MSG_STATE_FINISH,
                "item_list": [serde_json::Value::Object(item)],
                "context_token": context_token,
            },
            "base_info": { "channel_version": CHANNEL_VERSION }
        });

        let body_str = serde_json::to_string(&body).map_err(|e| format!("序列化失败: {e}"))?;

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers(body_str.len()))
            .body(body_str)
            .timeout(Duration::from_millis(API_TIMEOUT_MS))
            .send()
            .await
            .map_err(|e| format!("sendMediaMessage 请求失败: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("sendMediaMessage HTTP {status}: {text}"));
        }
        Ok(())
    }

    /// Fetch QR code for login (static — no auth needed).
    pub async fn get_bot_qrcode(base_url: &str, bot_type: &str) -> Result<QRCodeResponse, String> {
        let base = base_url.trim().trim_end_matches('/');
        let url = format!(
            "{base}/ilink/bot/get_bot_qrcode?bot_type={}",
            urlencoding::encode(bot_type)
        );

        let response = reqwest::Client::new()
            .get(&url)
            .timeout(Duration::from_millis(15_000))
            .send()
            .await
            .map_err(|e| format!("获取 QR 码失败: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("获取 QR 码失败: HTTP {}", response.status()));
        }

        response
            .json::<QRCodeResponse>()
            .await
            .map_err(|e| format!("解析 QR 码响应失败: {e}"))
    }

    /// Poll QR code login status (static — no auth needed).
    pub async fn get_qrcode_status(
        base_url: &str,
        qrcode: &str,
    ) -> Result<QRCodeStatusResponse, String> {
        let base = base_url.trim().trim_end_matches('/');
        let url = format!(
            "{base}/ilink/bot/get_qrcode_status?qrcode={}",
            urlencoding::encode(qrcode)
        );

        let response = reqwest::Client::new()
            .get(&url)
            .header("iLink-App-ClientVersion", "1")
            .timeout(Duration::from_millis(35_000))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    return "timeout".to_string();
                }
                format!("查询扫码状态失败: {e}")
            })?;

        if !response.status().is_success() {
            return Err(format!("查询扫码状态失败: HTTP {}", response.status()));
        }

        response
            .json::<QRCodeStatusResponse>()
            .await
            .map_err(|e| format!("解析扫码状态响应失败: {e}"))
    }
}
