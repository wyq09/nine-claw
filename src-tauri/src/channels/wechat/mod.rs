pub mod api;
pub mod types;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64_ENGINE, Engine as Base64Engine};
use serde_json::json;
use tauri::{AppHandle, Emitter};

use self::api::WeChatApi;
use self::types::*;
use crate::channels::pi_bridge::PiBridge;
use crate::channels::types::{BotMessage, ChannelStatus, MediaPayload, MediaType};
use crate::channels::Channel;

/// Truncate `s` to at most `max_chars` Unicode characters (not bytes).
fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

fn now_timestamp_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const DEFAULT_BOT_TYPE: &str = "3";
const MAX_QR_REFRESH: u32 = 3;
const CHUNK_SIZE: usize = 3900;
/// Interval between consecutive getUpdates polls (seconds).
const POLL_INTERVAL_SECS: u64 = 3;
/// Chunk size for on_chunk frontend streaming (characters).
const STREAM_CHUNK_SIZE: usize = 500;

/// Internal message routed from monitor thread to worker thread.
struct WorkItem {
    user_id: String,
    text: String,
    context_token: String,
}

/// WeChat Channel — fully self-contained.
///
/// Manages its own monitor thread (getUpdates long-poll), worker thread
/// (AI via pi subprocess), and reply delivery (sendMessage with
/// context_token + all required iLink ghost fields).
pub struct WeChatChannel {
    token: String,
    base_url: String,
    route_tag: Option<String>,
    running: Arc<AtomicBool>,
    status: Arc<Mutex<ChannelStatus>>,
    sync_buf: Arc<Mutex<String>>,
    /// Per-user context_token cache (user_id → latest context_token).
    context_tokens: Arc<Mutex<HashMap<String, String>>>,
    /// AI provider config so the channel can run pi independently.
    ai_provider_id: String,
    ai_base_url: String,
    ai_api_key: String,
    ai_model: String,
}

impl WeChatChannel {
    pub fn new(token: &str, base_url: &str, route_tag: Option<&str>) -> Self {
        let url = if base_url.is_empty() {
            "https://ilinkai.weixin.qq.com".to_string()
        } else {
            base_url.to_string()
        };
        Self {
            token: token.to_string(),
            base_url: url,
            route_tag: route_tag.map(|s| s.to_string()),
            running: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(ChannelStatus::Disconnected)),
            sync_buf: Arc::new(Mutex::new(String::new())),
            context_tokens: Arc::new(Mutex::new(HashMap::new())),
            ai_provider_id: String::new(),
            ai_base_url: String::new(),
            ai_api_key: String::new(),
            ai_model: String::new(),
        }
    }

    /// Configure the AI provider so the channel can call pi for replies.
    pub fn set_ai_config(&mut self, provider_id: &str, base_url: &str, api_key: &str, model: &str) {
        self.ai_provider_id = provider_id.to_string();
        self.ai_base_url = base_url.to_string();
        self.ai_api_key = api_key.to_string();
        self.ai_model = model.to_string();
    }

    /// QR code login: fetch QR and poll until confirmed.
    pub fn login_with_qr(&self, app: &AppHandle) -> Result<WechatLoginResult, String> {
        let base_url = self.base_url.clone();

        let qr_resp = block_on_async(WeChatApi::get_bot_qrcode(&base_url, DEFAULT_BOT_TYPE))?;
        let qrcode = qr_resp
            .qrcode
            .clone()
            .ok_or_else(|| "QR 码响应缺少 qrcode 字段".to_string())?;
        let qrcode_content = qr_resp
            .qrcode_img_content
            .clone()
            .ok_or_else(|| "QR 码响应缺少 qrcode_img_content 字段".to_string())?;

        let qr_data_uri = generate_qr_data_uri(&qrcode_content)?;
        let _ = app.emit(
            "bot://qr-code",
            serde_json::json!({
                "channelId": "wechat", "qrcodeUrl": &qr_data_uri, "status": "waiting"
            }),
        );

        let mut refresh_count: u32 = 0;
        let deadline = std::time::Instant::now() + Duration::from_secs(300);

        while std::time::Instant::now() < deadline {
            match block_on_async(WeChatApi::get_qrcode_status(&base_url, &qrcode)) {
                Ok(sr) => match sr.status.as_deref() {
                    Some("wait") => {}
                    Some("scaned") => {
                        let _ = app.emit(
                            "bot://qr-code",
                            serde_json::json!({
                                "channelId": "wechat", "status": "scanned"
                            }),
                        );
                    }
                    Some("expired") => {
                        refresh_count += 1;
                        if refresh_count > MAX_QR_REFRESH {
                            return Ok(WechatLoginResult {
                                connected: false,
                                message: "二维码多次过期，请重新登录".to_string(),
                                ..Default::default()
                            });
                        }
                        let nq =
                            block_on_async(WeChatApi::get_bot_qrcode(&base_url, DEFAULT_BOT_TYPE))?;
                        let nc = nq.qrcode_img_content.as_deref().unwrap_or("");
                        let nd = generate_qr_data_uri(nc)?;
                        let _ = app.emit(
                            "bot://qr-code",
                            serde_json::json!({
                                "channelId": "wechat", "qrcodeUrl": &nd, "status": "refreshed"
                            }),
                        );
                    }
                    Some("confirmed") => {
                        let bot_token = sr
                            .bot_token
                            .clone()
                            .ok_or_else(|| "登录成功但未收到 bot_token".to_string())?;
                        let ilink_bot_id = sr
                            .ilink_bot_id
                            .clone()
                            .ok_or_else(|| "登录成功但未收到 ilink_bot_id".to_string())?;
                        let _ = app.emit(
                            "bot://qr-code",
                            serde_json::json!({
                                "channelId": "wechat", "status": "confirmed"
                            }),
                        );
                        return Ok(WechatLoginResult {
                            connected: true,
                            bot_token: Some(bot_token),
                            account_id: Some(ilink_bot_id),
                            base_url: sr.baseurl.clone(),
                            user_id: sr.ilink_user_id.clone(),
                            message: "微信连接成功".to_string(),
                        });
                    }
                    _ => {}
                },
                Err(e) if e == "timeout" => {}
                Err(e) => return Err(format!("查询扫码状态失败: {e}")),
            }
            thread::sleep(Duration::from_secs(2));
        }

        Ok(WechatLoginResult {
            connected: false,
            message: "登录超时，请重试".to_string(),
            ..Default::default()
        })
    }
}

// ── Channel trait implementation ──

impl Channel for WeChatChannel {
    fn start(&mut self, app: AppHandle) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("微信通道已在运行".to_string());
        }
        if self.token.is_empty() {
            return Err("微信 token 未配置，请先登录".to_string());
        }

        *self.status.lock().unwrap() = ChannelStatus::Connected;
        self.running.store(true, Ordering::SeqCst);

        // Local channel: monitor → worker
        let (work_tx, work_rx) = mpsc::channel::<WorkItem>();

        // ── Thread 1: Monitor (getUpdates long-poll) ──
        {
            let running = self.running.clone();
            let token = self.token.clone();
            let base_url = self.base_url.clone();
            let route_tag = self.route_tag.clone();
            let sync_buf = self.sync_buf.clone();
            let status = self.status.clone();
            let context_tokens = self.context_tokens.clone();
            let app_handle = app.clone();

            thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::error!("微信监控线程: 创建 runtime 失败: {e}");
                        running.store(false, Ordering::SeqCst);
                        return;
                    }
                };
                let api = WeChatApi::new(&base_url, &token, route_tag.as_deref());

                while running.load(Ordering::SeqCst) {
                    let buf = sync_buf.lock().unwrap().clone();
                    match rt.block_on(api.get_updates(&buf)) {
                        Ok(resp) => {
                            if let Some(new_buf) = resp.get_updates_buf {
                                *sync_buf.lock().unwrap() = new_buf;
                            }
                            if let Some(errcode) = resp.errcode {
                                if errcode == -14 {
                                    log::warn!("WeChat session expired (errcode -14)");
                                    running.store(false, Ordering::SeqCst);
                                    *status.lock().unwrap() =
                                        ChannelStatus::Error("会话已过期".to_string());
                                    break;
                                }
                            }
                            if let Some(msgs) = resp.msgs {
                                for msg in msgs {
                                    if msg.message_type != Some(MSG_TYPE_USER) {
                                        continue;
                                    }
                                    let from_user = msg.from_user_id.clone().unwrap_or_default();

                                    // Save context_token — CRITICAL for replies
                                    if let Some(ref ct) = msg.context_token {
                                        if !ct.is_empty() {
                                            context_tokens
                                                .lock()
                                                .unwrap()
                                                .insert(from_user.clone(), ct.clone());
                                        }
                                    }

                                    let text = extract_text_from_items(&msg.item_list);
                                    if text.is_empty() {
                                        continue;
                                    }

                                    log::info!(
                                        "微信收到消息: 用户={from_user} 内容={}",
                                        truncate_chars(&text, 50)
                                    );

                                    // Emit inbound event to frontend (history integration)
                                    emit_bot_message(&app_handle, &from_user, "inbound", &text);

                                    let ct = msg.context_token.clone().unwrap_or_default();
                                    if work_tx
                                        .send(WorkItem {
                                            user_id: from_user,
                                            text,
                                            context_token: ct,
                                        })
                                        .is_err()
                                    {
                                        log::warn!("Worker 已关闭，停止微信监控");
                                        running.store(false, Ordering::SeqCst);
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if !running.load(Ordering::SeqCst) {
                                break;
                            }
                            if e != "timeout" {
                                log::warn!("微信轮询失败: {e}");
                            }
                        }
                    }
                    // 定时轮询间隔 — 确保每个周期都有休眠，避免狂请求
                    if running.load(Ordering::SeqCst) {
                        thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));
                    }
                }
            });
        }

        // ── Thread 2: Worker (AI processing + streaming reply delivery) ──
        {
            let running = self.running.clone();
            let token = self.token.clone();
            let base_url = self.base_url.clone();
            let route_tag = self.route_tag.clone();
            let context_tokens = self.context_tokens.clone();
            let ai_pid = self.ai_provider_id.clone();
            let ai_base = self.ai_base_url.clone();
            let ai_key = self.ai_api_key.clone();
            let ai_mdl = self.ai_model.clone();
            let app_handle = app.clone();

            thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::error!("微信 Worker: 创建 runtime 失败: {e}");
                        running.store(false, Ordering::SeqCst);
                        return;
                    }
                };
                let api = WeChatApi::new(&base_url, &token, route_tag.as_deref());
                let bridge = PiBridge::new(&ai_pid, &ai_base, &ai_key, &ai_mdl);

                while running.load(Ordering::SeqCst) {
                    let item = match work_rx.recv_timeout(Duration::from_secs(1)) {
                        Ok(item) => item,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    };

                    log::info!(
                        "Worker 开始处理: 用户={} 内容={}",
                        item.user_id,
                        truncate_chars(&item.text, 50)
                    );
                    emit_bot_status(
                        &app_handle,
                        &item.user_id,
                        "processing",
                        &format!("正在处理: {}", truncate_chars(&item.text, 30)),
                    );

                    // Use context_token from message, fallback to stored
                    let ct = if item.context_token.is_empty() {
                        context_tokens
                            .lock()
                            .unwrap()
                            .get(&item.user_id)
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        item.context_token
                    };
                    let ct_opt = if ct.is_empty() {
                        None
                    } else {
                        Some(ct.as_str())
                    };

                    // Streaming callback:
                    // - Only emit chunks to frontend for live in-app display.
                    // - Do NOT send partial messages to WeChat here; WeChat gets one
                    //   complete message at the end to avoid protocol issues.
                    let user_id_ref = &item.user_id;
                    let app_for_cb = app_handle.clone();

                    let result = bridge.process_message(
                        "wechat",
                        &item.user_id,
                        &item.text,
                        STREAM_CHUNK_SIZE,
                        |chunk: &str| {
                            // Emit streaming chunk to frontend for live display
                            emit_bot_message(&app_for_cb, user_id_ref, "outbound_chunk", chunk);
                        },
                    );

                    match result {
                        Ok(full_text) => {
                            if full_text.is_empty() {
                                log::warn!("pi 返回空回复");
                                emit_bot_status(
                                    &app_handle,
                                    &item.user_id,
                                    "warn",
                                    "pi 返回了空回复，请检查 provider 配置",
                                );
                                emit_bot_message(&app_handle, &item.user_id, "outbound_done", "");
                                continue;
                            }
                            log::info!(
                                "Worker 完成: 用户={} 回复 {} 字符",
                                item.user_id,
                                full_text.len()
                            );
                            emit_bot_status(
                                &app_handle,
                                &item.user_id,
                                "done",
                                &format!("回复 {} 字符完成", full_text.len()),
                            );
                            // Send complete reply to WeChat (split into ≤3900-char chunks)
                            send_reply_chunks(&rt, &api, &item.user_id, &full_text, ct_opt);
                            // Emit completion to frontend
                            emit_bot_message(
                                &app_handle,
                                &item.user_id,
                                "outbound_done",
                                &full_text,
                            );
                        }
                        Err(e) => {
                            log::error!("pi 处理失败: {e}");
                            emit_bot_status(
                                &app_handle,
                                &item.user_id,
                                "error",
                                &format!("pi 失败: {e}"),
                            );
                            let error_msg = format!("[处理失败: {e}]");
                            send_reply_chunks(&rt, &api, &item.user_id, &error_msg, ct_opt);
                            emit_bot_message(&app_handle, &item.user_id, "error", &error_msg);
                        }
                    }
                }
            });
        }

        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        self.running.store(false, Ordering::SeqCst);
        *self.status.lock().unwrap() = ChannelStatus::Disconnected;
        Ok(())
    }

    fn send_message(&self, user_id: &str, content: &str) -> Result<(), String> {
        let api = WeChatApi::new(&self.base_url, &self.token, self.route_tag.as_deref());
        let ct = self.context_tokens.lock().unwrap().get(user_id).cloned();
        // Iterate by char boundary to avoid splitting multi-byte characters
        let chars: Vec<(usize, char)> = content.char_indices().collect();
        let mut start = 0;
        while start < chars.len() {
            let end_idx = (start + CHUNK_SIZE).min(chars.len());
            let byte_start = chars[start].0;
            let byte_end = if end_idx < chars.len() {
                chars[end_idx].0
            } else {
                content.len()
            };
            let chunk = &content[byte_start..byte_end];
            block_on_async(api.send_message(user_id, chunk, ct.as_deref()))?;
            start = end_idx;
        }
        Ok(())
    }

    fn send_media(&self, user_id: &str, media: &MediaPayload) -> Result<(), String> {
        let api = WeChatApi::new(&self.base_url, &self.token, self.route_tag.as_deref());
        let ct = self.context_tokens.lock().unwrap().get(user_id).cloned();
        let b64 = BASE64_ENGINE.encode(&media.data);

        let (item_type, item_json) = match media.media_type {
            MediaType::Image => (
                MSG_ITEM_TYPE_IMAGE,
                json!({ "image_item": { "image_base64": b64 } }),
            ),
            MediaType::File => (
                MSG_ITEM_TYPE_FILE,
                json!({ "file_item": { "file_base64": b64, "file_name": media.file_name } }),
            ),
            MediaType::Video => (
                MSG_ITEM_TYPE_VIDEO,
                json!({ "video_item": { "video_base64": b64 } }),
            ),
        };

        block_on_async(api.send_media_message(user_id, item_type, item_json, ct.as_deref()))
    }

    fn status(&self) -> ChannelStatus {
        self.status.lock().unwrap().clone()
    }
}

// ── Free helper functions ──

fn block_on_async<F, T>(fut: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("创建 tokio runtime 失败: {e}"))?;
    rt.block_on(fut)
}

fn generate_qr_data_uri(content: &str) -> Result<String, String> {
    use qrcode::QrCode;
    let code = QrCode::new(content.as_bytes()).map_err(|e| format!("生成二维码失败: {e}"))?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(256, 256)
        .quiet_zone(true)
        .build();
    Ok(format!(
        "data:image/svg+xml;utf8,{}",
        urlencoding::encode(&svg)
    ))
}

/// Send a reply to WeChat, splitting into ≤CHUNK_SIZE character chunks.
fn send_reply_chunks(
    rt: &tokio::runtime::Runtime,
    api: &WeChatApi,
    user_id: &str,
    content: &str,
    context_token: Option<&str>,
) {
    let mut start = 0;
    // Iterate over char boundaries to avoid splitting a multi-byte character
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    while start < chars.len() {
        let end_idx = (start + CHUNK_SIZE).min(chars.len());
        let byte_start = chars[start].0;
        let byte_end = if end_idx < chars.len() {
            chars[end_idx].0
        } else {
            content.len()
        };
        let chunk = &content[byte_start..byte_end];
        if let Err(e) = rt.block_on(api.send_message(user_id, chunk, context_token)) {
            log::error!("发送微信回复失败: {e}");
        }
        start = end_idx;
    }
}

/// Emit a `bot://status` event to the frontend for diagnostic display.
fn emit_bot_status(app: &AppHandle, user_id: &str, level: &str, message: &str) {
    let _ = app.emit(
        "bot://status",
        serde_json::json!({
            "channelId": "wechat",
            "userId": user_id,
            "level": level,   // "processing" | "done" | "warn" | "error"
            "message": message,
            "timestamp": now_timestamp_ms(),
        }),
    );
}

/// Emit a `bot://message` event to the frontend for history tracking.
fn emit_bot_message(app: &AppHandle, user_id: &str, direction: &str, content: &str) {
    let payload = BotMessage {
        channel_id: "wechat".to_string(),
        user_id: user_id.to_string(),
        direction: direction.to_string(),
        content: content.to_string(),
        timestamp: now_timestamp_ms(),
    };
    if let Err(e) = app.emit("bot://message", &payload) {
        log::error!("emit bot://message 失败: {e}");
    }
}

fn extract_text_from_items(items: &Option<Vec<MessageItem>>) -> String {
    let Some(items) = items else {
        return String::new();
    };
    let mut result = String::new();
    for item in items {
        if item.item_type == Some(MSG_ITEM_TYPE_TEXT) {
            if let Some(ref ti) = item.text_item {
                if let Some(ref t) = ti.text {
                    result.push_str(t);
                }
            }
        }
    }
    result
}
