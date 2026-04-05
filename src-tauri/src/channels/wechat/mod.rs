pub mod api;
pub mod types;

use crate::agent_workspace;
use crate::agents::ConversationAgentConfig;
use crate::dev_trace::dev_trace;
use std::collections::HashMap;
use std::path::Path;
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
use crate::channels::pi_bridge::{PiBridge, PiProcessOutcome, PiRunHandle};
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

fn merge_pending_user_messages(messages: &[String]) -> String {
    if messages.len() <= 1 {
        return messages.first().cloned().unwrap_or_default();
    }

    messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            if index == 0 {
                message.clone()
            } else {
                format!("【用户追加消息 {}】\n{}", index, message)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn cleanup_idle_user_state(
    user_states: &Arc<Mutex<HashMap<String, UserTurnState>>>,
    user_id: &str,
) {
    let mut guard = match user_states.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let should_remove = guard
        .get(user_id)
        .map(|state| !state.running && !state.queued && state.pending_texts.is_empty())
        .unwrap_or(false);

    if should_remove {
        guard.remove(user_id);
    }
}

fn is_image_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg")
    )
}

fn is_video_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v")
    )
}

fn is_audio_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp3" | "wav" | "ogg" | "opus" | "m4a" | "aac" | "amr" | "silk")
    )
}

fn parse_media_directive(line: &str) -> Option<ParsedMediaItem> {
    let trimmed = line.trim();
    if !trimmed.starts_with("::nc-media{") || !trimmed.ends_with('}') {
        return None;
    }

    let body = &trimmed["::nc-media{".len()..trimmed.len() - 1];
    let mut media_type = None;
    let mut path = None;

    for pair in body.split_whitespace() {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let normalized = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key {
            "type" => media_type = Some(normalized),
            "path" => path = Some(normalized),
            _ => {}
        }
    }

    let path = path?;
    if !Path::new(&path).is_absolute() {
        return None;
    }

    let media_type = match media_type.as_deref() {
        Some("image") => MediaType::Image,
        Some("video") => MediaType::Video,
        Some("audio") | Some("voice") => MediaType::Audio,
        Some("file") => MediaType::File,
        _ => {
            if is_image_path(&path) {
                MediaType::Image
            } else if is_video_path(&path) {
                MediaType::Video
            } else if is_audio_path(&path) {
                MediaType::Audio
            } else {
                MediaType::File
            }
        }
    };

    let file_name = Path::new(&path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string());

    Some(ParsedMediaItem {
        media_type,
        file_name,
        file_path: path,
    })
}

fn parse_markdown_media(line: &str) -> Option<ParsedMediaItem> {
    let trimmed = line.trim();
    let start = trimmed.find('(')?;
    let end = trimmed.rfind(')')?;
    if end <= start + 1 {
        return None;
    }

    let path = trimmed[start + 1..end].trim();
    if !Path::new(path).is_absolute() {
        return None;
    }

    let media_type = if trimmed.starts_with("![") || is_image_path(path) {
        MediaType::Image
    } else if is_video_path(path) {
        MediaType::Video
    } else if is_audio_path(path) {
        MediaType::Audio
    } else {
        MediaType::File
    };

    let file_name = Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string());

    Some(ParsedMediaItem {
        media_type,
        file_name,
        file_path: path.to_string(),
    })
}

fn split_text_and_media(content: &str) -> (String, Vec<ParsedMediaItem>) {
    let mut text_lines = Vec::new();
    let mut media_items = Vec::new();

    for line in content.lines() {
        if let Some(item) = parse_media_directive(line).or_else(|| parse_markdown_media(line)) {
            media_items.push(item);
        } else {
            text_lines.push(line);
        }
    }

    (text_lines.join("\n").trim().to_string(), media_items)
}

fn inbound_media_label(media_type: &MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "图片",
        MediaType::Video => "视频",
        MediaType::Audio => "语音",
        MediaType::File => "文件",
    }
}

fn default_extension(media_type: &MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "png",
        MediaType::Video => "mp4",
        MediaType::Audio => "mp3",
        MediaType::File => "bin",
    }
}

fn attachment_display_line(attachment: &InboundAttachment) -> String {
    let mut line = if attachment.saved_path.is_empty() {
        format!("[收到{}]", inbound_media_label(&attachment.media_type))
    } else {
        format!(
            "[收到{}] {}",
            inbound_media_label(&attachment.media_type),
            attachment.saved_path
        )
    };
    if let Some(transcript) = attachment.transcript.as_deref() {
        if !transcript.trim().is_empty() {
            line.push_str(" | 转写: ");
            line.push_str(transcript.trim());
        }
    }
    line
}

fn build_inbound_prompt(text: &str, attachments: &[InboundAttachment]) -> InboundMessagePayload {
    let normalized_text = text.trim();
    let mut display_lines = Vec::new();
    if !normalized_text.is_empty() {
        display_lines.push(normalized_text.to_string());
    }
    for attachment in attachments {
        display_lines.push(attachment_display_line(attachment));
    }
    let display_text = display_lines.join("\n").trim().to_string();

    let mut prompt_parts = Vec::new();
    if !normalized_text.is_empty() {
        prompt_parts.push(normalized_text.to_string());
    }
    if !attachments.is_empty() {
        let mut section = String::from("用户还发送了以下附件，请按当前智能体能力处理：");
        for attachment in attachments {
            section.push_str("\n- ");
            section.push_str(inbound_media_label(&attachment.media_type));
            if attachment.saved_path.is_empty() {
                section.push_str(" 已接收，但当前没有可用文件路径");
            } else {
                section.push_str(" 文件路径: ");
                section.push_str(&attachment.saved_path);
            }
            if let Some(transcript) = attachment.transcript.as_deref() {
                if !transcript.trim().is_empty() {
                    section.push_str("\n  语音转写: ");
                    section.push_str(transcript.trim());
                }
            }
        }
        prompt_parts.push(section);
    }

    InboundMessagePayload {
        prompt_text: prompt_parts.join("\n\n").trim().to_string(),
        display_text,
    }
}

fn download_url_bytes(rt: &tokio::runtime::Runtime, url: &str) -> Result<Vec<u8>, String> {
    rt.block_on(async {
        let response = reqwest::get(url)
            .await
            .map_err(|error| format!("下载附件失败: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("下载附件失败: HTTP {}", response.status()));
        }
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| format!("读取附件失败: {error}"))
    })
}

fn persist_wechat_attachment(
    rt: &tokio::runtime::Runtime,
    agent_id: &str,
    user_id: &str,
    media_type: MediaType,
    file_name_hint: Option<&str>,
    base64_data: Option<&str>,
    url_data: Option<&str>,
    transcript: Option<&str>,
) -> Result<Option<InboundAttachment>, String> {
    let data = if let Some(raw) = base64_data.filter(|value| !value.trim().is_empty()) {
        Some(
            BASE64_ENGINE
                .decode(raw.trim())
                .map_err(|error| format!("解析附件 Base64 失败: {error}"))?,
        )
    } else if let Some(url) = url_data.filter(|value| !value.trim().is_empty()) {
        Some(download_url_bytes(rt, url.trim())?)
    } else {
        None
    };

    if data.is_none() && transcript.unwrap_or_default().trim().is_empty() {
        return Ok(None);
    }

    let file_name = file_name_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "{}.{}",
                match media_type {
                    MediaType::Image => "image",
                    MediaType::Video => "video",
                    MediaType::Audio => "voice",
                    MediaType::File => "file",
                },
                default_extension(&media_type)
            )
        });

    let saved_path = if let Some(bytes) = data {
        agent_workspace::persist_agent_inbound_artifact(agent_id, user_id, &file_name, &bytes)?
            .to_string_lossy()
            .to_string()
    } else {
        String::new()
    };

    Ok(Some(InboundAttachment {
        media_type,
        saved_path,
        transcript: transcript
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    }))
}

fn extract_inbound_message(
    rt: &tokio::runtime::Runtime,
    items: &Option<Vec<MessageItem>>,
    agent_id: Option<&str>,
    user_id: &str,
) -> Result<Option<InboundMessagePayload>, String> {
    let Some(items) = items else {
        return Ok(None);
    };

    let mut text = String::new();
    let mut attachments = Vec::new();

    for item in items {
        if item.item_type == Some(MSG_ITEM_TYPE_TEXT) || item.text_item.is_some() {
            if let Some(ref ti) = item.text_item {
                if let Some(ref t) = ti.text {
                    text.push_str(t);
                }
            }
        }

        if let Some(agent_id) = agent_id {
            if let Some(ref image_item) = item.image_item {
                if let Some(attachment) = persist_wechat_attachment(
                    rt,
                    agent_id,
                    user_id,
                    MediaType::Image,
                    Some("image.png"),
                    image_item.image_base64.as_deref(),
                    image_item.image_url.as_deref(),
                    None,
                )? {
                    attachments.push(attachment);
                }
            }

            if let Some(ref file_item) = item.file_item {
                if let Some(attachment) = persist_wechat_attachment(
                    rt,
                    agent_id,
                    user_id,
                    MediaType::File,
                    file_item.file_name.as_deref(),
                    file_item.file_base64.as_deref(),
                    file_item.file_url.as_deref(),
                    None,
                )? {
                    attachments.push(attachment);
                }
            }

            if let Some(ref video_item) = item.video_item {
                if let Some(attachment) = persist_wechat_attachment(
                    rt,
                    agent_id,
                    user_id,
                    MediaType::Video,
                    Some("video.mp4"),
                    video_item.video_base64.as_deref(),
                    video_item.video_url.as_deref(),
                    None,
                )? {
                    attachments.push(attachment);
                }
            }

            if item.item_type == Some(MSG_ITEM_TYPE_VOICE) || item.voice_item.is_some() {
                let Some(ref voice_item) = item.voice_item else {
                    continue;
                };
                if let Some(attachment) = persist_wechat_attachment(
                    rt,
                    agent_id,
                    user_id,
                    MediaType::Audio,
                    voice_item.file_name.as_deref().or(Some("voice.mp3")),
                    voice_item.voice_base64.as_deref(),
                    voice_item.voice_url.as_deref(),
                    voice_item.text.as_deref(),
                )? {
                    attachments.push(attachment);
                }
            }
        }
    }

    let payload = build_inbound_prompt(&text, &attachments);
    if payload.prompt_text.is_empty() {
        return Ok(None);
    }
    Ok(Some(payload))
}

const DEFAULT_BOT_TYPE: &str = "3";
const MAX_QR_REFRESH: u32 = 3;
const CHUNK_SIZE: usize = 3900;
/// Interval between consecutive getUpdates polls (seconds).
const POLL_INTERVAL_SECS: u64 = 3;
/// Chunk size for on_chunk frontend streaming (characters).
const STREAM_CHUNK_SIZE: usize = 500;
const MESSAGE_COLLECT_WINDOW_MS: u64 = 800;

/// Internal message routed from monitor thread to worker thread.
struct WorkItem {
    user_id: String,
}

#[derive(Clone)]
struct UserTurnState {
    pending_texts: Vec<String>,
    latest_context_token: String,
    last_inbound_at: i64,
    queued: bool,
    running: bool,
    active_run: Option<Arc<PiRunHandle>>,
}

#[derive(Clone, Debug)]
struct InboundAttachment {
    media_type: MediaType,
    saved_path: String,
    transcript: Option<String>,
}

#[derive(Clone, Debug)]
struct InboundMessagePayload {
    prompt_text: String,
    display_text: String,
}

#[derive(Clone, Debug)]
struct ParsedMediaItem {
    media_type: MediaType,
    file_name: String,
    file_path: String,
}

/// WeChat Channel — fully self-contained.
///
/// Manages its own monitor thread (getUpdates long-poll), worker thread
/// (AI via pi subprocess), and reply delivery (sendMessage with
/// context_token + all required iLink ghost fields).
pub struct WeChatChannel {
    channel_id: String,
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
    ai_api_format: String,
    ai_base_url: String,
    ai_api_key: String,
    ai_model: String,
    agent_config: Option<ConversationAgentConfig>,
}

impl WeChatChannel {
    pub fn new(channel_id: &str, token: &str, base_url: &str, route_tag: Option<&str>) -> Self {
        let url = if base_url.is_empty() {
            "https://ilinkai.weixin.qq.com".to_string()
        } else {
            base_url.to_string()
        };
        Self {
            channel_id: channel_id.to_string(),
            token: token.to_string(),
            base_url: url,
            route_tag: route_tag.map(|s| s.to_string()),
            running: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(ChannelStatus::Disconnected)),
            sync_buf: Arc::new(Mutex::new(String::new())),
            context_tokens: Arc::new(Mutex::new(HashMap::new())),
            ai_provider_id: String::new(),
            ai_api_format: "openai".to_string(),
            ai_base_url: String::new(),
            ai_api_key: String::new(),
            ai_model: String::new(),
            agent_config: None,
        }
    }

    /// Configure the AI provider so the channel can call pi for replies.
    pub fn set_ai_config(
        &mut self,
        provider_id: &str,
        api_format: &str,
        base_url: &str,
        api_key: &str,
        model: &str,
        agent_config: Option<ConversationAgentConfig>,
    ) {
        self.ai_provider_id = provider_id.to_string();
        self.ai_api_format = api_format.to_string();
        self.ai_base_url = base_url.to_string();
        self.ai_api_key = api_key.to_string();
        self.ai_model = model.to_string();
        self.agent_config = agent_config;
    }

    /// QR code login: fetch QR and poll until confirmed.
    pub fn login_with_qr(&self, app: &AppHandle) -> Result<WechatLoginResult, String> {
        let base_url = self.base_url.clone();
        let channel_id = self.channel_id.clone();

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
                "channelId": &channel_id, "qrcodeUrl": &qr_data_uri, "status": "waiting"
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
                                "channelId": &channel_id, "status": "scanned"
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
                                "channelId": &channel_id, "qrcodeUrl": &nd, "status": "refreshed"
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
                                "channelId": &channel_id, "status": "confirmed"
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

    fn bound_agent_debug_label(&self) -> String {
        self.agent_config
            .as_ref()
            .map(|agent| format!("{} ({})", agent.name, agent.id))
            .unwrap_or_else(|| "未绑定智能体".to_string())
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
        let pi_runtime = crate::pi_runtime::require_pi_runtime_location(&app)?;

        *self.status.lock().unwrap() = ChannelStatus::Connected;
        self.running.store(true, Ordering::SeqCst);

        emit_bot_status(
            &app,
            &self.channel_id,
            "",
            "done",
            &format!(
                "微信机器人已启动，当前绑定智能体: {}",
                self.bound_agent_debug_label()
            ),
        );

        // Local channel: monitor → worker
        let (work_tx, work_rx) = mpsc::channel::<WorkItem>();
        let user_states: Arc<Mutex<HashMap<String, UserTurnState>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // ── Thread 1: Monitor (getUpdates long-poll) ──
        {
            let running = self.running.clone();
            let token = self.token.clone();
            let base_url = self.base_url.clone();
            let route_tag = self.route_tag.clone();
            let sync_buf = self.sync_buf.clone();
            let status = self.status.clone();
            let context_tokens = self.context_tokens.clone();
            let agent_config = self.agent_config.clone();
            let app_handle = app.clone();
            let channel_id = self.channel_id.clone();
            let user_states = user_states.clone();

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

                                    let inbound = match extract_inbound_message(
                                        &rt,
                                        &msg.item_list,
                                        agent_config.as_ref().map(|config| config.id.as_str()),
                                        &from_user,
                                    ) {
                                        Ok(Some(payload)) => payload,
                                        Ok(None) => continue,
                                        Err(error) => {
                                            emit_bot_status(
                                                &app_handle,
                                                &channel_id,
                                                &from_user,
                                                "warn",
                                                &format!("解析微信入站附件失败: {error}"),
                                            );
                                            continue;
                                        }
                                    };

                                    log::info!(
                                        "微信收到消息: 用户={from_user} 内容={}",
                                        truncate_chars(&inbound.display_text, 50)
                                    );
                                    dev_trace(
                                        "wechat",
                                        format!(
                                            "收到消息: user={} chars={} text={}",
                                            from_user,
                                            inbound.display_text.chars().count(),
                                            truncate_chars(&inbound.display_text, 80)
                                        ),
                                    );

                                    // Emit inbound event to frontend (history integration)
                                    emit_bot_message(
                                        &app_handle,
                                        &channel_id,
                                        &from_user,
                                        "inbound",
                                        &inbound.display_text,
                                        agent_config.as_ref(),
                                    );

                                    let ct = msg.context_token.clone().unwrap_or_default();
                                    let control_update = {
                                        let mut guard = match user_states.lock() {
                                            Ok(guard) => guard,
                                            Err(_) => {
                                                running.store(false, Ordering::SeqCst);
                                                break;
                                            }
                                        };

                                        let state =
                                            guard.entry(from_user.clone()).or_insert_with(|| {
                                                UserTurnState {
                                                    pending_texts: Vec::new(),
                                                    latest_context_token: ct.clone(),
                                                    last_inbound_at: now_timestamp_ms(),
                                                    queued: false,
                                                    running: false,
                                                    active_run: None,
                                                }
                                            });

                                        state.pending_texts.push(inbound.prompt_text);
                                        state.latest_context_token = ct.clone();
                                        state.last_inbound_at = now_timestamp_ms();

                                        let should_queue = !state.running && !state.queued;
                                        if should_queue {
                                            state.queued = true;
                                        }
                                        let active_run = if state.running {
                                            state.active_run.clone()
                                        } else {
                                            None
                                        };
                                        (should_queue, active_run)
                                    };

                                    if let Some(run_handle) = control_update.1 {
                                        let _ = run_handle.abort();
                                    }

                                    if control_update.0
                                        && work_tx.send(WorkItem { user_id: from_user }).is_err()
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
            let ai_fmt = self.ai_api_format.clone();
            let ai_base = self.ai_base_url.clone();
            let ai_key = self.ai_api_key.clone();
            let ai_mdl = self.ai_model.clone();
            let agent_config = self.agent_config.clone();
            let app_handle = app.clone();
            let channel_id = self.channel_id.clone();
            let user_states = user_states.clone();

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
                let bridge = PiBridge::new(
                    pi_runtime,
                    &ai_pid,
                    &ai_fmt,
                    &ai_base,
                    &ai_key,
                    &ai_mdl,
                    agent_config.clone(),
                );

                while running.load(Ordering::SeqCst) {
                    let item = match work_rx.recv_timeout(Duration::from_secs(1)) {
                        Ok(item) => item,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    };

                    let user_id = item.user_id;

                    while running.load(Ordering::SeqCst) {
                        let wait_more_ms = {
                            let guard = match user_states.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            let Some(state) = guard.get(&user_id) else {
                                break;
                            };
                            if state.pending_texts.is_empty() {
                                None
                            } else {
                                let elapsed = now_timestamp_ms() - state.last_inbound_at;
                                if elapsed >= MESSAGE_COLLECT_WINDOW_MS as i64 {
                                    None
                                } else {
                                    Some((MESSAGE_COLLECT_WINDOW_MS as i64 - elapsed) as u64)
                                }
                            }
                        };

                        if let Some(wait_ms) = wait_more_ms {
                            thread::sleep(Duration::from_millis(wait_ms));
                            continue;
                        }

                        let (prompt_text, context_token) = {
                            let mut guard = match user_states.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            let Some(state) = guard.get_mut(&user_id) else {
                                break;
                            };
                            if state.pending_texts.is_empty() {
                                state.queued = false;
                                state.running = false;
                                state.active_run = None;
                                break;
                            }

                            state.queued = false;
                            state.running = true;
                            state.active_run = None;

                            (
                                merge_pending_user_messages(&std::mem::take(
                                    &mut state.pending_texts,
                                )),
                                state.latest_context_token.clone(),
                            )
                        };

                        log::info!(
                            "Worker 开始处理: 用户={} 内容={}",
                            user_id,
                            truncate_chars(&prompt_text, 50)
                        );
                        dev_trace(
                            "wechat",
                            format!(
                                "开始处理: user={} prompt_chars={} merged_count={}",
                                user_id,
                                prompt_text.chars().count(),
                                prompt_text.matches("【用户追加消息").count() + 1
                            ),
                        );
                        emit_bot_status(
                            &app_handle,
                            &channel_id,
                            &user_id,
                            "processing",
                            &format!(
                                "正在按智能体 {} 处理: {}",
                                agent_config
                                    .as_ref()
                                    .map(|agent| format!("{} ({})", agent.name, agent.id))
                                    .unwrap_or_else(|| "未绑定智能体".to_string()),
                                truncate_chars(&prompt_text, 30)
                            ),
                        );

                        let ct = if context_token.is_empty() {
                            context_tokens
                                .lock()
                                .unwrap()
                                .get(&user_id)
                                .cloned()
                                .unwrap_or_default()
                        } else {
                            context_token
                        };
                        let ct_opt = if ct.is_empty() {
                            None
                        } else {
                            Some(ct.as_str())
                        };

                        let user_id_for_chunk = user_id.clone();
                        let user_id_for_state = user_id.clone();
                        let app_for_cb = app_handle.clone();
                        let state_for_run = user_states.clone();

                        let result = bridge.process_message_interruptible(
                            &channel_id,
                            &user_id,
                            &prompt_text,
                            STREAM_CHUNK_SIZE,
                            |chunk: &str| {
                                emit_bot_message(
                                    &app_for_cb,
                                    &channel_id,
                                    &user_id_for_chunk,
                                    "outbound_chunk",
                                    chunk,
                                    agent_config.as_ref(),
                                );
                            },
                            move |run_handle| {
                                if let Ok(mut guard) = state_for_run.lock() {
                                    if let Some(state) = guard.get_mut(&user_id_for_state) {
                                        state.active_run = Some(run_handle);
                                    }
                                }
                            },
                        );

                        let has_pending_followup = {
                            let mut guard = match user_states.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            let state = match guard.get_mut(&user_id) {
                                Some(state) => state,
                                None => break,
                            };
                            state.running = false;
                            state.active_run = None;
                            !state.pending_texts.is_empty()
                        };

                        match result {
                            Ok(PiProcessOutcome::Completed(full_text)) => {
                                if has_pending_followup {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &user_id,
                                        "processing",
                                        "检测到用户追加消息，正在合并后重新处理",
                                    );
                                    continue;
                                }

                                let (text_reply, media_items) = split_text_and_media(&full_text);

                                if text_reply.is_empty() && media_items.is_empty() {
                                    log::warn!("pi 返回空回复");
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &user_id,
                                        "warn",
                                        "pi 返回了空回复，请检查 provider 配置",
                                    );
                                    emit_bot_message(
                                        &app_handle,
                                        &channel_id,
                                        &user_id,
                                        "outbound_done",
                                        "",
                                        agent_config.as_ref(),
                                    );
                                    cleanup_idle_user_state(&user_states, &user_id);
                                    break;
                                }
                                log::info!(
                                    "Worker 完成: 用户={} 回复 {} 字符",
                                    user_id,
                                    full_text.len()
                                );
                                dev_trace(
                                    "wechat",
                                    format!(
                                        "处理完成: user={} chars={} media_items={}",
                                        user_id,
                                        full_text.chars().count(),
                                        media_items.len()
                                    ),
                                );
                                emit_bot_status(
                                    &app_handle,
                                    &channel_id,
                                    &user_id,
                                    "done",
                                    &format!("回复 {} 字符完成", full_text.len()),
                                );
                                if let Some(agent_id) =
                                    agent_config.as_ref().map(|config| config.id.as_str())
                                {
                                    let _ = agent_workspace::append_agent_memory_entry(
                                        agent_id,
                                        &user_id,
                                        &prompt_text,
                                        if text_reply.is_empty() {
                                            &full_text
                                        } else {
                                            &text_reply
                                        },
                                    );
                                }
                                if !text_reply.is_empty() {
                                    send_reply_chunks(&rt, &api, &user_id, &text_reply, ct_opt);
                                }
                                for media in media_items {
                                    if let Err(error) =
                                        send_media_item(&rt, &api, &user_id, &media, ct_opt)
                                    {
                                        log::error!("发送媒体消息失败: {error}");
                                    }
                                }
                                emit_bot_message(
                                    &app_handle,
                                    &channel_id,
                                    &user_id,
                                    "outbound_done",
                                    &full_text,
                                    agent_config.as_ref(),
                                );
                                cleanup_idle_user_state(&user_states, &user_id);
                                break;
                            }
                            Ok(PiProcessOutcome::Aborted) => {
                                if has_pending_followup {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &user_id,
                                        "processing",
                                        "收到用户新消息，正在中断上一轮并重新处理",
                                    );
                                    continue;
                                }
                                cleanup_idle_user_state(&user_states, &user_id);
                                break;
                            }
                            Err(e) => {
                                if has_pending_followup {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &user_id,
                                        "warn",
                                        &format!("上一轮处理失败，已切换到用户最新消息: {e}"),
                                    );
                                    continue;
                                }

                                log::error!("pi 处理失败: {e}");
                                dev_trace(
                                    "wechat",
                                    format!("处理失败: user={} error={}", user_id, e),
                                );
                                emit_bot_status(
                                    &app_handle,
                                    &channel_id,
                                    &user_id,
                                    "error",
                                    &format!("pi 失败: {e}"),
                                );
                                let error_msg = format!("[处理失败: {e}]");
                                send_reply_chunks(&rt, &api, &user_id, &error_msg, ct_opt);
                                emit_bot_message(
                                    &app_handle,
                                    &channel_id,
                                    &user_id,
                                    "error",
                                    &error_msg,
                                    agent_config.as_ref(),
                                );
                                cleanup_idle_user_state(&user_states, &user_id);
                                break;
                            }
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
            MediaType::Audio => (
                MSG_ITEM_TYPE_FILE,
                json!({ "file_item": { "file_base64": b64, "file_name": media.file_name } }),
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

fn send_media_item(
    rt: &tokio::runtime::Runtime,
    api: &WeChatApi,
    user_id: &str,
    media: &ParsedMediaItem,
    context_token: Option<&str>,
) -> Result<(), String> {
    let data = std::fs::read(&media.file_path)
        .map_err(|error| format!("读取媒体文件失败 {}: {error}", media.file_path))?;
    let b64 = BASE64_ENGINE.encode(&data);

    let (item_type, item_json) = match media.media_type {
        MediaType::Image => (
            MSG_ITEM_TYPE_IMAGE,
            json!({ "image_item": { "image_base64": b64 } }),
        ),
        MediaType::Audio => (
            MSG_ITEM_TYPE_FILE,
            json!({ "file_item": { "file_base64": b64, "file_name": media.file_name } }),
        ),
        MediaType::Video => (
            MSG_ITEM_TYPE_VIDEO,
            json!({ "video_item": { "video_base64": b64 } }),
        ),
        MediaType::File => (
            MSG_ITEM_TYPE_FILE,
            json!({ "file_item": { "file_base64": b64, "file_name": media.file_name } }),
        ),
    };

    rt.block_on(api.send_media_message(user_id, item_type, item_json, context_token))
}

/// Emit a `bot://status` event to the frontend for diagnostic display.
fn emit_bot_status(app: &AppHandle, channel_id: &str, user_id: &str, level: &str, message: &str) {
    let _ = app.emit(
        "bot://status",
        serde_json::json!({
            "channelId": channel_id,
            "userId": user_id,
            "level": level,   // "processing" | "done" | "warn" | "error"
            "message": message,
            "timestamp": now_timestamp_ms(),
        }),
    );
}

/// Emit a `bot://message` event to the frontend for history tracking.
fn emit_bot_message(
    app: &AppHandle,
    channel_id: &str,
    user_id: &str,
    direction: &str,
    content: &str,
    agent: Option<&ConversationAgentConfig>,
) {
    let payload = BotMessage {
        channel_id: channel_id.to_string(),
        user_id: user_id.to_string(),
        direction: direction.to_string(),
        content: content.to_string(),
        timestamp: now_timestamp_ms(),
        agent: agent.cloned(),
    };
    if let Err(e) = app.emit("bot://message", &payload) {
        log::error!("emit bot://message 失败: {e}");
    }
}
