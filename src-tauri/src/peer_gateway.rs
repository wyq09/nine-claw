//! HTTP 对等入站：多智能体通过 `to_agent_id` 路由；鉴权为**各智能体独立密钥**（`bot_configs.peer`）。
//! 默认在 **设置 → 通用** 中启用并监听 `0.0.0.0:1052`（局域网可连）；环境变量 `NINECLAW_PEER_BIND` 若设置则覆盖应用内配置。

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tower_http::limit::RequestBodyLimitLayer;

use crate::agents::{self, AgentRecord, ConversationAgentConfig};
use crate::channels::pi_bridge::{PiBridge, PiProcessOutcome};
use crate::channels::types::BotMessage;
use crate::pi_runtime;

const PEER_CHANNEL_ID: &str = "peer";
const STREAM_CHUNK_SIZE: usize = 500;

pub const PEER_INBOUND_PATH: &str = "/nineclaw/v1/inbound";
/// 健康检查（GET，无需鉴权）；用于对接方探测服务是否存活。
pub const PEER_HEALTH_PATH: &str = "/health";
const PEER_HEALTH_PATH_V1: &str = "/nineclaw/v1/health";

const PEER_PROTOCOL: &str = "nineclaw-peer";
const PEER_PROTOCOL_VERSION: u32 = 1;

/// 对等 API 统一错误体（便于对方按 `code` 分支）。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerJsonError {
    pub code: String,
    pub message: String,
}

/// 对等 HTTP 统一响应信封（与请求相同使用 camelCase）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerV1Response {
    pub protocol: &'static str,
    pub version: u32,
    pub ok: bool,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// 与 `reply` 相同；异步 webhook 回调时保留，兼容只解析 `text` 的旧对接方。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<PeerJsonError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<&'static str>,
}

fn echo_agent_field(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn correlation_from_body(body: &PeerInboundBody) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    (
        echo_agent_field(&body.from_agent_id),
        echo_agent_field(&body.to_agent_id),
        echo_agent_field(&body.thread_id),
        body.message_id
            .as_ref()
            .and_then(|m| echo_agent_field(m.as_str())),
    )
}

fn classify_peer_process_error(raw: &str) -> PeerJsonError {
    let code = if raw.contains("仅支持 protocol") || raw.contains("version") {
        "PEER_UNSUPPORTED"
    } else if raw.contains("不能为空") {
        "PEER_BAD_REQUEST"
    } else if raw.contains("智能体不存在") {
        "PEER_AGENT_NOT_FOUND"
    } else if raw.contains("智能体不可用") {
        "PEER_AGENT_UNAVAILABLE"
    } else if raw.contains("中断") {
        "PEER_ABORTED"
    } else {
        "PEER_PROCESS_FAILED"
    };
    PeerJsonError {
        code: code.to_string(),
        message: raw.to_string(),
    }
}

fn peer_v1_peer_error(body: Option<&PeerInboundBody>, err: PeerJsonError) -> PeerV1Response {
    let (from_agent_id, to_agent_id, thread_id, in_reply_to) = body
        .map(correlation_from_body)
        .unwrap_or((None, None, None, None));
    PeerV1Response {
        protocol: PEER_PROTOCOL,
        version: PEER_PROTOCOL_VERSION,
        ok: false,
        kind: "peerError",
        from_agent_id,
        to_agent_id,
        thread_id,
        in_reply_to,
        reply: None,
        text: None,
        error: Some(err),
        mode: None,
        message: None,
        service: None,
        channel_id: None,
    }
}

fn peer_v1_inbound_reply_ok(body: &PeerInboundBody, reply: String) -> PeerV1Response {
    let (from_agent_id, to_agent_id, thread_id, in_reply_to) = correlation_from_body(body);
    PeerV1Response {
        protocol: PEER_PROTOCOL,
        version: PEER_PROTOCOL_VERSION,
        ok: true,
        kind: "inboundReply",
        from_agent_id,
        to_agent_id,
        thread_id,
        in_reply_to,
        reply: Some(reply),
        text: None,
        error: None,
        mode: None,
        message: None,
        service: None,
        channel_id: None,
    }
}

fn peer_v1_inbound_reply_err(body: &PeerInboundBody, err: PeerJsonError) -> PeerV1Response {
    let (from_agent_id, to_agent_id, thread_id, in_reply_to) = correlation_from_body(body);
    PeerV1Response {
        protocol: PEER_PROTOCOL,
        version: PEER_PROTOCOL_VERSION,
        ok: false,
        kind: "inboundReply",
        from_agent_id,
        to_agent_id,
        thread_id,
        in_reply_to,
        reply: None,
        text: None,
        error: Some(err),
        mode: None,
        message: None,
        service: None,
        channel_id: None,
    }
}

fn peer_v1_inbound_accepted(body: &PeerInboundBody) -> PeerV1Response {
    let (from_agent_id, to_agent_id, thread_id, in_reply_to) = correlation_from_body(body);
    PeerV1Response {
        protocol: PEER_PROTOCOL,
        version: PEER_PROTOCOL_VERSION,
        ok: true,
        kind: "inboundAccepted",
        from_agent_id,
        to_agent_id,
        thread_id,
        in_reply_to,
        reply: None,
        text: None,
        error: None,
        mode: Some("async"),
        message: Some("已接受，将 POST 到 replyWebhook".into()),
        service: None,
        channel_id: None,
    }
}

fn peer_v1_webhook(body: &PeerInboundBody, ok: bool, reply: &str, err: Option<PeerJsonError>) -> PeerV1Response {
    let (reply_o, text_o) = if ok && !reply.trim().is_empty() {
        let s = reply.to_string();
        (Some(s.clone()), Some(s))
    } else {
        (None, None)
    };
    PeerV1Response {
        protocol: PEER_PROTOCOL,
        version: PEER_PROTOCOL_VERSION,
        ok,
        kind: "inboundWebhook",
        from_agent_id: echo_agent_field(&body.to_agent_id),
        to_agent_id: echo_agent_field(&body.from_agent_id),
        thread_id: echo_agent_field(&body.thread_id),
        in_reply_to: body
            .message_id
            .as_ref()
            .and_then(|m| echo_agent_field(m.as_str())),
        reply: reply_o,
        text: text_o,
        error: err,
        mode: None,
        message: None,
        service: None,
        channel_id: None,
    }
}

const PEER_GATEWAY_SETTINGS_KEY: &str = "peer_gateway_settings_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerGatewaySettings {
    #[serde(default = "default_peer_enabled")]
    pub enabled: bool,
    #[serde(default = "default_peer_host")]
    pub host: String,
    #[serde(default = "default_peer_port")]
    pub port: u16,
    #[serde(default)]
    pub public_base: String,
}

fn default_peer_enabled() -> bool {
    true
}

fn default_peer_host() -> String {
    "0.0.0.0".to_string()
}

fn default_peer_port() -> u16 {
    1052
}

impl Default for PeerGatewaySettings {
    fn default() -> Self {
        Self {
            enabled: default_peer_enabled(),
            host: default_peer_host(),
            port: default_peer_port(),
            public_base: String::new(),
        }
    }
}

fn normalize_peer_settings(mut s: PeerGatewaySettings) -> PeerGatewaySettings {
    s.host = s.host.trim().to_string();
    s.public_base = s.public_base.trim().trim_end_matches('/').to_string();
    if s.host.is_empty() {
        s.host = default_peer_host();
    }
    s
}

/// 仅解析环境变量 `NINECLAW_PEER_BIND`（若存在则覆盖应用内监听配置）。
pub fn peer_bind_from_env() -> Option<SocketAddr> {
    let raw = std::env::var("NINECLAW_PEER_BIND").ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with(':') {
        format!("0.0.0.0{trimmed}").parse().ok()
    } else if !trimmed.contains(':') {
        format!("0.0.0.0:{trimmed}").parse().ok()
    } else {
        trimmed.parse().ok()
    }
}

fn resolve_listen_socket(host: &str, port: u16) -> Result<SocketAddr, String> {
    let h = host.trim();
    if h.is_empty() {
        return Err("监听主机不能为空".to_string());
    }
    if port == 0 {
        return Err("端口号无效".to_string());
    }
    h.parse::<IpAddr>()
        .map(|ip| SocketAddr::new(ip, port))
        .map_err(|_| format!("无法解析主机「{h}」"))
}

pub fn load_peer_gateway_settings(app: &AppHandle) -> Result<PeerGatewaySettings, String> {
    let connection = crate::open_history_db(app)?;
    let raw: Option<String> = connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            rusqlite::params![PEER_GATEWAY_SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("读取对等网关设置失败: {e}"))?;
    let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
        return Ok(PeerGatewaySettings::default());
    };
    serde_json::from_str::<PeerGatewaySettings>(&raw)
        .map(normalize_peer_settings)
        .map_err(|e| format!("解析对等网关设置失败: {e}"))
}

pub fn save_peer_gateway_settings(app: &AppHandle, settings: &PeerGatewaySettings) -> Result<(), String> {
    if settings.port == 0 {
        return Err("端口必须在 1–65535 之间".into());
    }
    let normalized = normalize_peer_settings(settings.clone());
    let json = serde_json::to_string(&normalized).map_err(|e| format!("序列化对等网关设置失败: {e}"))?;
    let connection = crate::open_history_db(app)?;
    let now = crate::chrono_like_timestamp();
    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![PEER_GATEWAY_SETTINGS_KEY, json, now],
        )
        .map_err(|e| format!("保存对等网关设置失败: {e}"))?;
    Ok(())
}

/// 实际监听地址：优先环境变量，否则来自数据库设置（启用 + host:port）。
pub fn resolve_peer_listen_addr(app: &AppHandle) -> Option<SocketAddr> {
    if let Some(addr) = peer_bind_from_env() {
        return Some(addr);
    }
    let settings = load_peer_gateway_settings(app).unwrap_or_default();
    if !settings.enabled {
        return None;
    }
    resolve_listen_socket(&settings.host, settings.port).ok()
}

fn display_http_host(ip: IpAddr) -> String {
    if ip.is_unspecified() {
        return if ip.is_ipv6() {
            "[::1]".to_string()
        } else {
            "127.0.0.1".to_string()
        };
    }
    match ip {
        IpAddr::V6(_) => format!("[{ip}]"),
        IpAddr::V4(v4) => v4.to_string(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerGatewayInfo {
    /// 当前会尝试监听的地址（已解析）。
    pub enabled: bool,
    /// 已由 `NINECLAW_PEER_BIND` 接管，应用内端口设置无效。
    pub env_override_active: bool,
    pub listen_address: Option<String>,
    /// 用于拼接 API 的基址（无末尾 `/`）。
    pub public_base_url: Option<String>,
    pub inbound_url: Option<String>,
    pub health_url: Option<String>,
}

fn resolve_public_base_url(app: &AppHandle, addr: SocketAddr) -> Option<String> {
    if let Ok(s) = std::env::var("NINECLAW_PEER_PUBLIC_BASE") {
        let t = s.trim().trim_end_matches('/').to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    if let Ok(settings) = load_peer_gateway_settings(app) {
        let t = settings.public_base.trim().trim_end_matches('/').to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    let host = display_http_host(addr.ip());
    Some(format!("http://{}:{}", host, addr.port()))
}

pub fn get_peer_gateway_info(app: &AppHandle) -> PeerGatewayInfo {
    let env_override_active = peer_bind_from_env().is_some();
    let addr = resolve_peer_listen_addr(app);
    let enabled = addr.is_some();
    let listen_address = addr.map(|a| a.to_string());
    let public_base = addr.and_then(|a| resolve_public_base_url(app, a));
    let inbound_url = public_base
        .as_ref()
        .map(|b| format!("{}{}", b.trim_end_matches('/'), PEER_INBOUND_PATH));
    let health_url = public_base
        .as_ref()
        .map(|b| format!("{}{}", b.trim_end_matches('/'), PEER_HEALTH_PATH));
    PeerGatewayInfo {
        enabled,
        env_override_active,
        listen_address,
        public_base_url: public_base,
        inbound_url,
        health_url,
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInboundBody {
    pub protocol: String,
    pub version: u32,
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub thread_id: String,
    #[serde(default)]
    pub message_id: Option<String>,
    pub text: String,
    /// 若设置：立即返回 202，处理完成后向此 URL POST JSON（`kind: inboundWebhook`，见 `PeerV1Response`）。
    #[serde(default)]
    pub reply_webhook: Option<String>,
    /// 可选；设置则 outbound POST 时加入 `Authorization: Bearer …`。
    #[serde(default)]
    pub reply_webhook_auth: Option<String>,
    /// 预留；可携带群聊等上下文，后续版本再接入 Pi。
    #[serde(default)]
    #[allow(dead_code)]
    pub metadata: serde_json::Value,
}

#[derive(Clone)]
struct PeerGatewayState {
    app: AppHandle,
}

fn now_timestamp_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let prefix = "Bearer ";
    if value.len() > prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(value[prefix.len()..].trim().to_string())
    } else {
        None
    }
}

fn peer_effective_inbound_secret(config: &crate::agents::AgentBotConfig) -> Option<String> {
    config
        .peer_shared_secret
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let s = config.client_secret.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        })
}

fn authorize_peer(record: &AgentRecord, token: Option<&str>) -> bool {
    let Some(t) = token.filter(|s| !s.is_empty()) else {
        return false;
    };
    let Some(expected) = record
        .bot_configs
        .get("peer")
        .and_then(peer_effective_inbound_secret)
    else {
        return false;
    };
    expected == t
}

fn emit_peer_status(app: &AppHandle, user_id: &str, level: &str, message: &str) {
    let app_emit = app.clone();
    let channel_id = PEER_CHANNEL_ID.to_string();
    let user_id = user_id.to_string();
    let level = level.to_string();
    let message = message.to_string();
    let timestamp = now_timestamp_ms();
    let payload = json!({
        "channelId": channel_id,
        "userId": user_id,
        "level": level,
        "message": message,
        "timestamp": timestamp,
    });
    let _ = app.clone().run_on_main_thread(move || {
        let _ = app_emit.emit("bot://status", payload);
    });
}

fn emit_peer_message(
    app: &AppHandle,
    user_id: &str,
    direction: &str,
    content: &str,
    agent: Option<&ConversationAgentConfig>,
) {
    let payload = BotMessage {
        channel_id: PEER_CHANNEL_ID.to_string(),
        user_id: user_id.to_string(),
        direction: direction.to_string(),
        content: content.to_string(),
        timestamp: now_timestamp_ms(),
        agent: agent.cloned(),
    };
    let app_emit = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let Err(error) = app_emit.emit("bot://message", &payload) {
            log::error!("emit bot://message (peer) 失败: {error}");
        }
    });
}

fn build_session_user_id(from_agent_id: &str, thread_id: &str) -> String {
    format!("peer:{from_agent_id}:{thread_id}")
}

fn augment_prompt_for_peer(body: &PeerInboundBody) -> String {
    let base = body.text.trim();
    if base.is_empty() {
        return String::new();
    }
    format!(
        "[对等智能体 from_agent_id={} thread_id={}]\n{}",
        body.from_agent_id.trim(),
        body.thread_id.trim(),
        base
    )
}

fn post_reply_webhook(url: &str, auth: Option<&str>, payload: &PeerV1Response) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP 客户端: {e}"))?;
    let mut req = client.post(url).json(payload);
    if let Some(a) = auth.filter(|s| !s.trim().is_empty()) {
        req = req.header(
            header::AUTHORIZATION,
            format!("Bearer {}", a.trim()),
        );
    }
    let response = req
        .send()
        .map_err(|e| format!("webhook 请求失败: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let preview: String = response
            .text()
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        return Err(format!("webhook HTTP {} — {}", status, preview));
    }
    Ok(())
}

/// 同步跑 Pi；返回 assistant 全文或错误信息（用于 JSON 或 webhook）。
fn run_peer_turn(app: &AppHandle, body: &PeerInboundBody) -> Result<String, String> {
    if body.protocol.trim() != "nineclaw-peer" || body.version != 1 {
        return Err("仅支持 protocol=nineclaw-peer 且 version=1".to_string());
    }
    let from = body.from_agent_id.trim();
    let to = body.to_agent_id.trim();
    let thread = body.thread_id.trim();
    if from.is_empty() || to.is_empty() || thread.is_empty() {
        return Err("from_agent_id、to_agent_id、thread_id 均不能为空".to_string());
    }
    if body.text.trim().is_empty() {
        return Err("text 不能为空".to_string());
    }

    if agents::get_agent_record(app, to)?.is_none() {
        return Err(format!("智能体不存在: {to}"));
    }

    let agent_config = agents::get_conversation_agent_config(app, to)?
        .ok_or_else(|| format!("智能体不可用: {to}"))?;

    let runtime = crate::resolve_im_llm_runtime(
        app,
        &agent_config.default_provider_id,
        &agent_config.default_model,
    )?;
    let normalized_base = crate::normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );
    let pi_loc = pi_runtime::require_pi_runtime_location(app)?;
    let bridge = PiBridge::new(
        pi_loc,
        &runtime.provider_id,
        &runtime.api_format,
        &normalized_base,
        &runtime.api_key,
        &runtime.model,
        Some(agent_config.clone()),
    );

    let session_user_id = build_session_user_id(from, thread);
    let prompt = augment_prompt_for_peer(body);

    emit_peer_status(
        app,
        &session_user_id,
        "processing",
        &format!("对等入站 → 智能体 {} ({})", agent_config.name, agent_config.id),
    );
    emit_peer_message(
        app,
        &session_user_id,
        "inbound",
        &prompt,
        Some(&agent_config),
    );

    let user_id_for_chunk = session_user_id.clone();
    let app_for_chunk = app.clone();
    let agent_for_chunk = agent_config.clone();

    let outcome = bridge.process_message_interruptible(
        PEER_CHANNEL_ID,
        &session_user_id,
        &prompt,
        STREAM_CHUNK_SIZE,
        |chunk: &str| {
            emit_peer_message(
                &app_for_chunk,
                &user_id_for_chunk,
                "outbound_chunk",
                chunk,
                Some(&agent_for_chunk),
            );
        },
        |_| {},
    );

    match outcome {
        Ok(PiProcessOutcome::Completed(full_text)) => {
            let _ = crate::agent_workspace::append_agent_memory_entry(
                agent_config.id.as_str(),
                &session_user_id,
                &prompt,
                full_text.trim(),
            );
            emit_peer_message(
                app,
                &session_user_id,
                "outbound_done",
                &full_text,
                Some(&agent_config),
            );
            emit_peer_status(app, &session_user_id, "done", "对等回复已完成");
            Ok(full_text)
        }
        Ok(PiProcessOutcome::Aborted) => Err("处理被中断".to_string()),
        Err(e) => {
            emit_peer_message(app, &session_user_id, "error", &e, Some(&agent_config));
            emit_peer_status(app, &session_user_id, "error", &e);
            Err(e)
        }
    }
}

async fn health_handler() -> impl IntoResponse {
    Json(PeerV1Response {
        protocol: PEER_PROTOCOL,
        version: PEER_PROTOCOL_VERSION,
        ok: true,
        kind: "health",
        from_agent_id: None,
        to_agent_id: None,
        thread_id: None,
        in_reply_to: None,
        reply: None,
        text: None,
        error: None,
        mode: None,
        message: None,
        service: Some("nineclaw-peer"),
        channel_id: Some(PEER_CHANNEL_ID),
    })
}

async fn inbound_handler(
    State(state): State<Arc<PeerGatewayState>>,
    headers: HeaderMap,
    Json(body): Json<PeerInboundBody>,
) -> Response {
    let token = bearer_token(&headers);
    let to_id = body.to_agent_id.trim().to_string();

    let record_result = tokio::task::spawn_blocking({
        let app = state.app.clone();
        let to_id = to_id.clone();
        move || agents::get_agent_record(&app, &to_id)
    })
    .await;

    let record = match record_result {
        Ok(Ok(Some(r))) => r,
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(peer_v1_peer_error(
                    Some(&body),
                    PeerJsonError {
                        code: "PEER_AGENT_NOT_FOUND".into(),
                        message: "智能体不存在".into(),
                    },
                )),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(peer_v1_peer_error(
                    Some(&body),
                    PeerJsonError {
                        code: "PEER_INTERNAL".into(),
                        message: e,
                    },
                )),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(peer_v1_peer_error(
                    Some(&body),
                    PeerJsonError {
                        code: "PEER_INTERNAL".into(),
                        message: "内部任务失败".into(),
                    },
                )),
            )
                .into_response();
        }
    };

    if !authorize_peer(&record, token.as_deref()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(peer_v1_peer_error(
                Some(&body),
                PeerJsonError {
                    code: "PEER_AUTH_FAILED".into(),
                    message: "鉴权失败：请使用目标智能体在「虾/对等」中配置的独立密钥作为 Bearer，或与保存的密钥完全一致".into(),
                },
            )),
        )
            .into_response();
    }

    let webhook = body
        .reply_webhook
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    if let Some(url) = webhook {
        let auth = body.reply_webhook_auth.clone();
        let app = state.app.clone();
        let body_clone = body.clone();
        std::thread::spawn(move || {
            let result = run_peer_turn(&app, &body_clone);
            let payload = match &result {
                Ok(text) => peer_v1_webhook(&body_clone, true, text.as_str(), None),
                Err(err) => peer_v1_webhook(
                    &body_clone,
                    false,
                    "",
                    Some(classify_peer_process_error(err.as_str())),
                ),
            };
            if let Err(e) = post_reply_webhook(&url, auth.as_deref(), &payload) {
                log::warn!("peer reply_webhook 投递失败: {e}");
            }
        });
        return (StatusCode::ACCEPTED, Json(peer_v1_inbound_accepted(&body))).into_response();
    }

    let body_snapshot = body.clone();
    let app = state.app.clone();
    let body_clone = body;
    let sync_result = tokio::task::spawn_blocking(move || run_peer_turn(&app, &body_clone)).await;

    match sync_result {
        Ok(Ok(reply)) => (
            StatusCode::OK,
            Json(peer_v1_inbound_reply_ok(&body_snapshot, reply)),
        )
            .into_response(),
        Ok(Err(e)) => (
            StatusCode::OK,
            Json(peer_v1_inbound_reply_err(
                &body_snapshot,
                classify_peer_process_error(&e),
            )),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(peer_v1_peer_error(
                Some(&body_snapshot),
                PeerJsonError {
                    code: "PEER_INTERNAL".into(),
                    message: "处理任务失败".into(),
                },
            )),
        )
            .into_response(),
    }
}

struct GatewayRunner {
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    join: thread::JoinHandle<()>,
}

static PEER_GATEWAY_RUNNER: OnceLock<Mutex<Option<GatewayRunner>>> = OnceLock::new();

fn peer_gateway_runner_lock() -> &'static Mutex<Option<GatewayRunner>> {
    PEER_GATEWAY_RUNNER.get_or_init(|| Mutex::new(None))
}

pub fn stop_peer_gateway() {
    let Ok(mut guard) = peer_gateway_runner_lock().lock() else {
        return;
    };
    if let Some(runner) = guard.take() {
        if let Ok(mut inner) = runner.shutdown_tx.lock() {
            if let Some(tx) = inner.take() {
                let _ = tx.send(());
            }
        }
        let _ = runner.join.join();
    }
}

fn build_peer_router(app: AppHandle) -> Router {
    let state = Arc::new(PeerGatewayState { app });
    Router::new()
        .route(PEER_HEALTH_PATH, get(health_handler))
        .route(PEER_HEALTH_PATH_V1, get(health_handler))
        .route("/nineclaw/v1/inbound", post(inbound_handler))
        .layer(RequestBodyLimitLayer::new(512 * 1024))
        .with_state(state)
}

pub fn start_peer_gateway(app: &AppHandle, addr: SocketAddr) -> Result<(), String> {
    stop_peer_gateway();
    thread::sleep(Duration::from_millis(120));
    let router = build_peer_router(app.clone());
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let holder: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>> =
        Arc::new(Mutex::new(Some(shutdown_tx)));
    let app_clone = app.clone();
    let join = thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                log::error!("peer gateway tokio runtime 创建失败: {e}");
                return;
            }
        };
        runtime.block_on(async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    log::error!("peer gateway 绑定 {addr} 失败: {e}");
                    let app_emit = app_clone.clone();
                    let _ = app_clone.run_on_main_thread(move || {
                        let _ = app_emit.emit(
                            "bot://status",
                            json!({
                                "channelId": PEER_CHANNEL_ID,
                                "userId": "",
                                "level": "error",
                                "message": format!("对等网关绑定失败: {e}"),
                                "timestamp": now_timestamp_ms(),
                            }),
                        );
                    });
                    return;
                }
            };
            log::info!("NineClaw 对等网关已监听 {addr}（多智能体请用 JSON 字段 toAgentId 指定目标）");
            if let Err(e) = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
            {
                log::error!("peer gateway 服务退出: {e}");
            }
        });
    });
    let mut guard = peer_gateway_runner_lock()
        .lock()
        .map_err(|_| "对等网关内部锁失败".to_string())?;
    *guard = Some(GatewayRunner {
        shutdown_tx: holder,
        join,
    });
    Ok(())
}

pub fn restart_peer_gateway(app: &AppHandle) -> Result<(), String> {
    let Some(addr) = resolve_peer_listen_addr(app) else {
        stop_peer_gateway();
        log::info!("对等网关已停止（设置中已关闭或未配置有效监听）");
        return Ok(());
    };
    start_peer_gateway(app, addr)
}
