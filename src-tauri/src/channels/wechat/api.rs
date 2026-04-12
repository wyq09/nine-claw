use base64::{engine::general_purpose::STANDARD as BASE64_ENGINE, Engine as Base64Engine};
use md5::{Digest, Md5};
use openssl::symm::Cipher;
use rand::Rng;
use serde::Deserialize;
use serde_json::json;
use std::net::TcpStream;
use std::time::Duration;
use uuid::Uuid;

use super::types::*;

/// Timeout for each getUpdates request. Keep short for reliable periodic polling.
const GET_UPDATES_TIMEOUT_MS: u64 = 8_000;
const API_TIMEOUT_MS: u64 = 15_000;
const CDN_UPLOAD_TIMEOUT_MS: u64 = 30_000;
const CDN_UPLOAD_MAX_RETRIES: usize = 3;
const WECHAT_CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";
const ILINK_APP_ID: &str = "bot";
const ILINK_APP_CLIENT_VERSION: &str = "131335";
const UPLOAD_MEDIA_TYPE_IMAGE: i32 = 1;
const UPLOAD_MEDIA_TYPE_VIDEO: i32 = 2;
const UPLOAD_MEDIA_TYPE_FILE: i32 = 3;

#[derive(Debug, Deserialize)]
struct UploadUrlResponse {
    upload_param: Option<String>,
    upload_full_url: Option<String>,
}

#[derive(Debug)]
struct UploadedMediaInfo {
    encrypt_query_param: String,
    aeskey_hex: String,
    file_size: usize,
    file_size_ciphertext: usize,
}

/// Build a reqwest client that auto-detects proxy availability.
fn build_http_client() -> reqwest::Client {
    let proxy_available = std::env::var("http_proxy")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("all_proxy"))
        .ok()
        .and_then(|proxy_url| {
            let stripped = proxy_url
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .trim_start_matches("socks5://")
                .trim_start_matches("socks5h://");
            TcpStream::connect_timeout(&stripped.parse().ok()?, Duration::from_millis(500)).ok()
        })
        .is_some();

    let mut builder = reqwest::Client::builder();
    if !proxy_available {
        builder = builder.no_proxy();
    }
    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}

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
            client: build_http_client(),
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

    fn build_auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("iLink-App-Id", ILINK_APP_ID.parse().unwrap());
        headers.insert(
            "iLink-App-ClientVersion",
            ILINK_APP_CLIENT_VERSION.parse().unwrap(),
        );
        headers.insert("AuthorizationType", "ilink_bot_token".parse().unwrap());
        if !self.token.is_empty() {
            headers.insert(
                "Authorization",
                format!("Bearer {}", self.token).parse().unwrap(),
            );
        }
        headers.insert("X-WECHAT-UIN", Self::random_uin().parse().unwrap());
        if let Some(tag) = &self.route_tag {
            headers.insert("SKRouteTag", tag.parse().unwrap());
        }
        headers
    }

    fn build_cdn_upload_url(upload_param: &str, filekey: &str) -> String {
        format!(
            "{WECHAT_CDN_BASE_URL}/upload?encrypted_query_param={}&filekey={}",
            urlencoding::encode(upload_param),
            urlencoding::encode(filekey)
        )
    }

    fn aeskey_message_value(aeskey_hex: &str) -> String {
        BASE64_ENGINE.encode(aeskey_hex.as_bytes())
    }

    fn encrypt_cdn_payload(data: &[u8], aes_key: &[u8]) -> Result<Vec<u8>, String> {
        openssl::symm::encrypt(Cipher::aes_128_ecb(), aes_key, None, data)
            .map_err(|error| format!("AES-128-ECB 加密失败: {error}"))
    }

    fn build_headers(&self, body_len: usize) -> reqwest::header::HeaderMap {
        let mut headers = self.build_auth_headers();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("Content-Length", body_len.to_string().parse().unwrap());
        headers
    }

    async fn download_url(&self, url: &str, with_auth_headers: bool) -> Result<Vec<u8>, String> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err("附件 URL 为空".to_string());
        }

        let resolved_url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else {
            let base = reqwest::Url::parse(&format!("{}/", self.base_url.trim_end_matches('/')))
                .map_err(|error| format!("解析微信 base_url 失败: {error}"))?;
            base.join(trimmed)
                .map_err(|error| format!("拼接微信附件 URL 失败: {error}"))?
                .to_string()
        };

        let request = self.client.get(&resolved_url);
        let request = if with_auth_headers {
            request.headers(self.build_auth_headers())
        } else {
            request
        };

        let response = request
            .timeout(Duration::from_millis(API_TIMEOUT_MS))
            .send()
            .await
            .map_err(|error| format!("下载附件失败: {error}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("下载附件失败: HTTP {status} {text}"));
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| format!("读取附件失败: {error}"))
    }

    pub async fn download_attachment(&self, url: &str) -> Result<Vec<u8>, String> {
        self.download_url(url, true).await
    }

    pub async fn download_public_attachment(&self, url: &str) -> Result<Vec<u8>, String> {
        self.download_url(url, false).await
    }

    async fn get_upload_url(
        &self,
        filekey: &str,
        to_user_id: &str,
        upload_media_type: i32,
        raw_size: usize,
        raw_md5: &str,
        encrypted_size: usize,
        aeskey_hex: &str,
    ) -> Result<UploadUrlResponse, String> {
        let url = format!("{}/ilink/bot/getuploadurl", self.base_url);
        let body = json!({
            "filekey": filekey,
            "media_type": upload_media_type,
            "to_user_id": to_user_id,
            "rawsize": raw_size,
            "rawfilemd5": raw_md5,
            "filesize": encrypted_size,
            "no_need_thumb": true,
            "aeskey": aeskey_hex,
            "base_info": { "channel_version": CHANNEL_VERSION }
        });
        let body_str =
            serde_json::to_string(&body).map_err(|error| format!("序列化上传请求失败: {error}"))?;

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers(body_str.len()))
            .body(body_str)
            .timeout(Duration::from_millis(API_TIMEOUT_MS))
            .send()
            .await
            .map_err(|error| format!("getUploadUrl 请求失败: {error}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("getUploadUrl HTTP {status}: {text}"));
        }

        response
            .json::<UploadUrlResponse>()
            .await
            .map_err(|error| format!("解析 getUploadUrl 响应失败: {error}"))
    }

    async fn upload_encrypted_media_to_cdn(
        &self,
        ciphertext: &[u8],
        upload_full_url: Option<&str>,
        upload_param: Option<&str>,
        filekey: &str,
    ) -> Result<String, String> {
        let resolved_url = if let Some(full_url) = upload_full_url
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            full_url.to_string()
        } else if let Some(upload_param) = upload_param
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Self::build_cdn_upload_url(upload_param, filekey)
        } else {
            return Err("微信 CDN 上传地址为空".to_string());
        };

        let mut last_error = None;
        for attempt in 1..=CDN_UPLOAD_MAX_RETRIES {
            match self
                .client
                .post(&resolved_url)
                .header("Content-Type", "application/octet-stream")
                .body(ciphertext.to_vec())
                .timeout(Duration::from_millis(CDN_UPLOAD_TIMEOUT_MS))
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_client_error() {
                        let status = response.status();
                        let err = response
                            .headers()
                            .get("x-error-message")
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("HTTP {status}"));
                        return Err(format!("微信 CDN 上传失败: {err}"));
                    }

                    if response.status() != reqwest::StatusCode::OK {
                        let status = response.status();
                        let err = response
                            .headers()
                            .get("x-error-message")
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("HTTP {status}"));
                        last_error = Some(format!("微信 CDN 上传失败: {err}"));
                    } else if let Some(download_param) = response
                        .headers()
                        .get("x-encrypted-param")
                        .and_then(|value| value.to_str().ok())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        return Ok(download_param.to_string());
                    } else {
                        last_error = Some("微信 CDN 上传成功但缺少 x-encrypted-param".to_string());
                    }
                }
                Err(error) => {
                    last_error = Some(format!("微信 CDN 上传请求失败: {error}"));
                }
            }

            if attempt < CDN_UPLOAD_MAX_RETRIES {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| "微信 CDN 上传失败".to_string()))
    }

    async fn upload_media(
        &self,
        to_user_id: &str,
        upload_media_type: i32,
        data: &[u8],
    ) -> Result<UploadedMediaInfo, String> {
        let mut md5 = Md5::new();
        md5.update(data);
        let raw_md5 = format!("{:x}", md5.finalize());

        let aes_key = Uuid::new_v4().as_bytes().to_vec();
        let aeskey_hex = aes_key
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let ciphertext = Self::encrypt_cdn_payload(data, &aes_key)?;
        let filekey = Uuid::new_v4().simple().to_string();

        let upload_url = self
            .get_upload_url(
                &filekey,
                to_user_id,
                upload_media_type,
                data.len(),
                &raw_md5,
                ciphertext.len(),
                &aeskey_hex,
            )
            .await?;

        let encrypt_query_param = self
            .upload_encrypted_media_to_cdn(
                &ciphertext,
                upload_url.upload_full_url.as_deref(),
                upload_url.upload_param.as_deref(),
                &filekey,
            )
            .await?;

        Ok(UploadedMediaInfo {
            encrypt_query_param,
            aeskey_hex,
            file_size: data.len(),
            file_size_ciphertext: ciphertext.len(),
        })
    }

    pub async fn send_binary_media(
        &self,
        to_user_id: &str,
        media_type: i32,
        file_name: &str,
        data: &[u8],
        context_token: Option<&str>,
    ) -> Result<(), String> {
        let (upload_media_type, item_type, item_json) = match media_type {
            MSG_ITEM_TYPE_IMAGE => {
                let uploaded = self
                    .upload_media(to_user_id, UPLOAD_MEDIA_TYPE_IMAGE, data)
                    .await?;
                (
                    UPLOAD_MEDIA_TYPE_IMAGE,
                    MSG_ITEM_TYPE_IMAGE,
                    json!({
                        "image_item": {
                            "media": {
                                "encrypt_query_param": uploaded.encrypt_query_param,
                                "aes_key": Self::aeskey_message_value(&uploaded.aeskey_hex),
                                "encrypt_type": 1
                            },
                            "mid_size": uploaded.file_size_ciphertext
                        }
                    }),
                )
            }
            MSG_ITEM_TYPE_VIDEO => {
                let uploaded = self
                    .upload_media(to_user_id, UPLOAD_MEDIA_TYPE_VIDEO, data)
                    .await?;
                (
                    UPLOAD_MEDIA_TYPE_VIDEO,
                    MSG_ITEM_TYPE_VIDEO,
                    json!({
                        "video_item": {
                            "media": {
                                "encrypt_query_param": uploaded.encrypt_query_param,
                                "aes_key": Self::aeskey_message_value(&uploaded.aeskey_hex),
                                "encrypt_type": 1
                            },
                            "video_size": uploaded.file_size_ciphertext
                        }
                    }),
                )
            }
            MSG_ITEM_TYPE_FILE | MSG_ITEM_TYPE_VOICE => {
                let uploaded = self
                    .upload_media(to_user_id, UPLOAD_MEDIA_TYPE_FILE, data)
                    .await?;
                (
                    UPLOAD_MEDIA_TYPE_FILE,
                    MSG_ITEM_TYPE_FILE,
                    json!({
                        "file_item": {
                            "media": {
                                "encrypt_query_param": uploaded.encrypt_query_param,
                                "aes_key": Self::aeskey_message_value(&uploaded.aeskey_hex),
                                "encrypt_type": 1
                            },
                            "file_name": file_name,
                            "len": uploaded.file_size.to_string()
                        }
                    }),
                )
            }
            other => {
                return Err(format!("不支持的微信媒体类型: {other}"));
            }
        };

        log::info!(
            "微信上传媒体完成: to_user_id={} upload_media_type={} item_type={} file_name={}",
            to_user_id,
            upload_media_type,
            item_type,
            file_name
        );
        self.send_media_message(to_user_id, item_type, item_json, context_token)
            .await
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

        let response = build_http_client()
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

        let response = build_http_client()
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Bytes;
    use axum::extract::State;
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Json, Router};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct TestState {
        base_url: String,
        get_upload_bodies: Arc<Mutex<Vec<serde_json::Value>>>,
        upload_bodies: Arc<Mutex<Vec<Vec<u8>>>>,
        send_message_bodies: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    async fn get_upload_url_handler(
        State(state): State<TestState>,
        Json(body): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        state.get_upload_bodies.lock().unwrap().push(body);
        Json(json!({
            "upload_full_url": format!("{}/upload", state.base_url),
        }))
    }

    async fn upload_handler(State(state): State<TestState>, body: Bytes) -> impl IntoResponse {
        state.upload_bodies.lock().unwrap().push(body.to_vec());
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-encrypted-param",
            HeaderValue::from_static("download-token"),
        );
        (StatusCode::OK, headers)
    }

    async fn send_message_handler(
        State(state): State<TestState>,
        Json(body): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        state.send_message_bodies.lock().unwrap().push(body);
        Json(json!({}))
    }

    #[tokio::test]
    async fn send_binary_media_posts_uploaded_file_message() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("local addr");
        let base_url = format!("http://{addr}");
        let state = TestState {
            base_url: base_url.clone(),
            get_upload_bodies: Arc::new(Mutex::new(Vec::new())),
            upload_bodies: Arc::new(Mutex::new(Vec::new())),
            send_message_bodies: Arc::new(Mutex::new(Vec::new())),
        };

        let router = Router::new()
            .route("/ilink/bot/getuploadurl", post(get_upload_url_handler))
            .route("/upload", post(upload_handler))
            .route("/ilink/bot/sendmessage", post(send_message_handler))
            .with_state(state.clone());

        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("serve test router");
        });

        let api = WeChatApi::new(&base_url, "test-token", None);
        api.send_binary_media(
            "wxid-test-user",
            MSG_ITEM_TYPE_FILE,
            "report.pdf",
            b"hello",
            Some("ctx-token"),
        )
        .await
        .expect("send file media");

        let get_upload_bodies = state.get_upload_bodies.lock().unwrap().clone();
        assert_eq!(get_upload_bodies.len(), 1);
        assert_eq!(get_upload_bodies[0]["to_user_id"], "wxid-test-user");
        assert_eq!(get_upload_bodies[0]["media_type"], UPLOAD_MEDIA_TYPE_FILE);
        assert_eq!(get_upload_bodies[0]["rawsize"], 5);
        assert_eq!(get_upload_bodies[0]["filesize"], 16);

        let upload_bodies = state.upload_bodies.lock().unwrap().clone();
        assert_eq!(upload_bodies.len(), 1);
        assert_eq!(upload_bodies[0].len(), 16);

        let send_message_bodies = state.send_message_bodies.lock().unwrap().clone();
        assert_eq!(send_message_bodies.len(), 1);
        let body = &send_message_bodies[0];
        assert_eq!(body["msg"]["to_user_id"], "wxid-test-user");
        assert_eq!(body["msg"]["context_token"], "ctx-token");
        assert_eq!(body["msg"]["item_list"][0]["type"], MSG_ITEM_TYPE_FILE);
        assert_eq!(
            body["msg"]["item_list"][0]["file_item"]["media"]["encrypt_query_param"],
            "download-token"
        );
        assert_eq!(
            body["msg"]["item_list"][0]["file_item"]["file_name"],
            "report.pdf"
        );
        assert_eq!(body["msg"]["item_list"][0]["file_item"]["len"], "5");
        assert_eq!(
            body["msg"]["item_list"][0]["file_item"]["media"]["encrypt_type"],
            1
        );
        assert!(body["msg"]["item_list"][0]["file_item"]["media"]["aes_key"]
            .as_str()
            .map(|value| !value.is_empty())
            .unwrap_or(false));

        server.abort();
    }
}
