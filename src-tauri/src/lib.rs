mod agent_tasks;
mod agent_workspace;
mod agents;
mod channels;
mod chat_attachments;
mod dev_trace;
mod heartbeat;
mod media_directives;
mod peer_gateway;
mod pi_runtime;
mod pi_timeouts;
mod prompt_attachments;
mod scheduler;
mod skills;

use base64::{engine::general_purpose::STANDARD as BASE64_ENGINE, Engine as _};
use md5::{Digest, Md5};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Build a reqwest client that auto-detects proxy availability.
/// If proxy env vars are set and the proxy port is reachable, use proxy.
/// Otherwise, skip proxy to avoid connecting to a dead port.
fn build_http_client() -> reqwest::Client {
    let proxy_available = std::env::var("http_proxy")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("all_proxy"))
        .ok()
        .and_then(|proxy_url| {
            // Extract host:port from proxy URL like "http://127.0.0.1:7890" or "socks5://127.0.0.1:7890"
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
use tauri::{AppHandle, Emitter, Manager, PhysicalSize, Size};

use agent_tasks::{
    AgentTaskDeliveryRecord, AgentTaskListItem, AgentTaskPromptResult, AgentTaskUpdateInput,
};
use agent_workspace::AgentWorkspaceBundle;
use agents::{AgentInput, AgentRecord, ConversationAgentConfig};
use channels::factory::ChannelConfig;
use channels::manager::ChannelManager;
use channels::pi_bridge::{PiBridge, PiProcessOutcome};
use channels::types::{MediaPayload, MediaType};
use channels::wechat::WeChatChannel;
use chat_attachments::{ChatAttachmentUpload, PersistedChatAttachment};
use dev_trace::{dev_trace, dev_trace_block};
use pi_runtime::RuntimeDependencyStatus;
use skills::{InstalledSkill, SystemSkillCatalog};

#[derive(Clone)]
struct PiRuntimeHandle {
    abort_requested: Arc<AtomicBool>,
    pid: u32,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

static PI_RUNTIME_HANDLES: OnceLock<Mutex<HashMap<String, PiRuntimeHandle>>> = OnceLock::new();
static DESKTOP_POOLED_PI: OnceLock<Mutex<HashMap<String, DesktopPooledPi>>> = OnceLock::new();
/// 同一桌面 session 串行化 `stream_pi_prompt`，避免并发时池替换/双进程互相 kill 导致 SIGKILL、stdout 空读。
static DESKTOP_STREAM_SESSION_MUTEXES: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    OnceLock::new();

fn desktop_session_stream_mutex(session_id: &str) -> Arc<Mutex<()>> {
    let map = DESKTOP_STREAM_SESSION_MUTEXES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().expect("DESKTOP_STREAM_SESSION_MUTEXES poisoned");
    guard
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}
const HISTORY_DB_FILE: &str = "nineclaw.sqlite3";
const LEGACY_HISTORY_DB_FILES: &[&str] = &["yqagent.sqlite3"];
const PI_SESSION_FILE_PREFIX: &str = "nineclaw-pi-session-";
const PI_SUMMARY_FILE_PREFIX: &str = "nineclaw-pi-summary-";
const LEGACY_PI_SESSION_FILE_PREFIXES: &[&str] = &["yqagent-pi-session-"];
const PI_RUNTIME_DIR_NAME: &str = "nineclaw-pi-runtime";
const HISTORY_STATE_KEY: &str = "history_v1";
const PROVIDER_CONFIGS_STATE_KEY: &str = "provider_configs_v1";
const CUSTOM_PROVIDER_META_STATE_KEY: &str = "custom_provider_meta_v1";

struct DesktopPooledPi {
    child: Child,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stdout_rx: mpsc::Receiver<Result<String, String>>,
    stderr_buffer: Arc<Mutex<String>>,
    fingerprint: String,
}

fn pi_reuse_desktop_enabled() -> bool {
    std::env::var("NINECLAW_PI_REUSE_DESKTOP")
        .map(|value| value.trim() != "0")
        .unwrap_or(true)
}

fn open_path_in_default_app(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = Command::new("open");
        cmd.arg(path);
        cmd
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", ""]);
        cmd.arg(path);
        cmd
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(path);
        cmd
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("打开文件失败: {error}"))
}

fn open_url_in_default_browser(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("链接不能为空".to_string());
    }

    let normalized = trimmed.to_ascii_lowercase();
    if !(normalized.starts_with("http://") || normalized.starts_with("https://")) {
        return Err("仅支持打开 http 或 https 链接".to_string());
    }

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = Command::new("open");
        cmd.arg(trimmed);
        cmd
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", ""]);
        cmd.arg(trimmed);
        cmd
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(trimmed);
        cmd
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("打开链接失败: {error}"))
}

fn resolve_local_file_path(file_path: &str) -> Result<PathBuf, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err("文件路径不能为空".to_string());
    }

    let decoded = if let Some(raw_path) = trimmed.strip_prefix("file://") {
        urlencoding::decode(raw_path)
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| raw_path.to_string())
    } else {
        trimmed.to_string()
    };

    let path = PathBuf::from(&decoded);
    let resolved_path = if path.is_absolute() {
        path
    } else if let Ok(current_dir) = std::env::current_dir() {
        current_dir.join(path)
    } else {
        PathBuf::from(&decoded)
    };

    if !resolved_path.exists() {
        return Err(format!("文件不存在: {}", resolved_path.display()));
    }

    Ok(resolved_path)
}

pub(crate) fn infer_media_mime_type(path: &Path, mime_hint: Option<&str>) -> String {
    if let Some(mime_hint) = mime_hint {
        let trimmed = mime_hint.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if let Some(sniffed) = sniff_media_mime_type(path) {
        return sniffed;
    }

    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png".to_string(),
        Some("jpg") | Some("jpeg") => "image/jpeg".to_string(),
        Some("gif") => "image/gif".to_string(),
        Some("webp") => "image/webp".to_string(),
        Some("bmp") => "image/bmp".to_string(),
        Some("svg") => "image/svg+xml".to_string(),
        Some("mp4") => "video/mp4".to_string(),
        Some("mov") => "video/quicktime".to_string(),
        Some("webm") => "video/webm".to_string(),
        Some("m4v") => "video/x-m4v".to_string(),
        Some("avi") => "video/x-msvideo".to_string(),
        Some("mkv") => "video/x-matroska".to_string(),
        Some("mp3") => "audio/mpeg".to_string(),
        Some("wav") => "audio/wav".to_string(),
        Some("m4a") => "audio/mp4".to_string(),
        Some("aac") => "audio/aac".to_string(),
        Some("ogg") => "audio/ogg".to_string(),
        Some("opus") => "audio/ogg".to_string(),
        Some("amr") => "audio/amr".to_string(),
        Some("silk") => "audio/silk".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

fn sniff_media_mime_type(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut header = [0u8; 64];
    let read_len = file.read(&mut header).ok()?;
    detect_media_mime_from_bytes(&header[..read_len]).map(str::to_string)
}

fn detect_media_mime_from_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }

    if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
        return Some("image/jpeg");
    }

    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }

    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }

    if bytes.len() >= 2 && &bytes[0..2] == b"BM" {
        return Some("image/bmp");
    }

    if bytes.len() >= 6 && bytes.starts_with(b"<?xml ") && bytes.windows(4).any(|w| w == b"<svg") {
        return Some("image/svg+xml");
    }

    if bytes.windows(4).any(|w| w == b"<svg") {
        return Some("image/svg+xml");
    }

    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];
        if matches!(
            brand,
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis"
        ) {
            return Some("image/heic");
        }
        if matches!(brand, b"mif1" | b"msf1" | b"heif") {
            return Some("image/heif");
        }
        if brand == b"qt  " {
            return Some("video/quicktime");
        }
        return Some("video/mp4");
    }

    if bytes.len() >= 4 && bytes[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        if bytes.windows(4).any(|w| w.eq_ignore_ascii_case(b"webm")) {
            return Some("video/webm");
        }
        return Some("video/x-matroska");
    }

    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Some("audio/wav");
    }

    if bytes.starts_with(b"ID3") {
        return Some("audio/mpeg");
    }

    if bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        return Some("audio/mpeg");
    }

    if bytes.starts_with(b"OggS") {
        return Some("audio/ogg");
    }

    if bytes.starts_with(b"#!AMR\n") || bytes.starts_with(b"#!AMR-WB\n") {
        return Some("audio/amr");
    }

    if bytes.starts_with(b"%PDF-") {
        return Some("application/pdf");
    }

    None
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PiTokenUsagePayload {
    #[serde(alias = "inputTokens", alias = "input", default)]
    input_tokens: Option<u64>,
    #[serde(alias = "outputTokens", alias = "output", default)]
    output_tokens: Option<u64>,
    #[serde(
        alias = "cacheReadTokens",
        alias = "cacheRead",
        alias = "cache_read_tokens",
        default
    )]
    cache_read_tokens: Option<u64>,
    #[serde(
        alias = "cacheWriteTokens",
        alias = "cacheWrite",
        alias = "cache_write_tokens",
        default
    )]
    cache_write_tokens: Option<u64>,
    #[serde(alias = "totalTokens", alias = "total_tokens", default)]
    total_tokens: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PiUsageMetadataPayload {
    api: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    #[serde(alias = "responseId", alias = "response_id", default)]
    response_id: Option<String>,
    timestamp: Option<i64>,
}

#[derive(Clone, Serialize)]
struct PiStreamPayload {
    event: String,
    session_id: Option<String>,
    text: Option<String>,
    error: Option<String>,
    aborted_by: Option<String>,
    tool_call_id: Option<String>,
    tool_name: Option<String>,
    args_text: Option<String>,
    result_text: Option<String>,
    is_error: Option<bool>,
    reason: Option<String>,
    #[serde(flatten)]
    usage: Option<PiTokenUsagePayload>,
    #[serde(flatten)]
    usage_meta: Option<PiUsageMetadataPayload>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenUsageRecordRow {
    turn_id: String,
    session_id: String,
    turn_created_at: i64,
    turn_completed_at: Option<i64>,
    agent_id: Option<String>,
    agent_name: Option<String>,
    api: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    response_id: Option<String>,
    usage_timestamp: Option<i64>,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    total_tokens: u64,
    recorded_at: i64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderRuntimeConfig {
    provider_id: String,
    api_format: String,
    base_url: String,
    api_key: String,
    model: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProviderPrefsRow {
    #[serde(default)]
    added: bool,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    api_format: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderPreferencesPayload {
    provider_configs: Option<String>,
    custom_provider_meta: Option<String>,
}

struct ChildExitOutcome {
    status: Option<ExitStatus>,
    timed_out: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MultimodalSummaryEntry {
    timestamp_ms: i64,
    user_prompt: String,
    assistant_response: String,
}

const CHILD_KILL_GRACE_TIMEOUT: Duration = Duration::from_secs(1);
/// stdout 已关闭后等待 pi 进程退出的上限；过短会 `Child::kill`（SIGKILL），易与「上游无输出」叠在一起误判。
const DESKTOP_PI_AFTER_STDOUT_EOF_EXIT_WAIT: Duration = Duration::from_secs(30);

fn spawn_pi_stdout_logger<R>(
    reader: R,
    scope: &'static str,
    tx: mpsc::Sender<Result<String, String>>,
) where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut line_count = 0usize;
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
                    line_count += 1;
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        dev_trace(scope, trimmed);
                    }
                    let _ = tx.send(Ok(line));
                }
                Err(error) => {
                    let message = error.to_string();
                    dev_trace(scope, format!("读取失败: {message}"));
                    let _ = tx.send(Err(message));
                    break;
                }
            }
        }
        dev_trace(scope, format!("stdout_eof: lines={line_count}"));
    });
}

#[derive(Clone, Debug)]
struct DesktopParsedMediaItem {
    media_type: MediaType,
    file_name: String,
    file_path: String,
}

fn desktop_media_reply_prompt(agent_home: Option<&Path>) -> String {
    let mut prompt = String::from(
        "当前回复目标是 NineClaw 桌面用户。如果你需要把本地生成的图片、文件或视频真正回复给用户，请单独输出一行 `::nc-media{type=\"image|file|video\" path=\"/absolute/path/to/file\"}`。该指令行不要附加解释文字；普通文本说明单独写在其他行。",
    );
    if let Some(agent_home) = agent_home {
        let preferred_dir = agent_home.join("outbox");
        prompt.push_str(" 生成给用户的正式产物时，不要只放在临时目录；优先写到 `");
        prompt.push_str(&preferred_dir.display().to_string());
        prompt.push_str("` 或其子目录，再在 `::nc-media` 里引用那个绝对路径。");
    }
    prompt
}

fn is_desktop_image_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg")
    )
}

fn is_desktop_video_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v")
    )
}

fn is_desktop_audio_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp3" | "wav" | "ogg" | "opus" | "m4a" | "aac" | "amr" | "silk")
    )
}

fn parse_desktop_media_directive(line: &str) -> Option<DesktopParsedMediaItem> {
    let directive = media_directives::parse_media_directive_fields(line)?;
    if !Path::new(&directive.path).is_absolute() {
        return None;
    }

    let media_type = match directive
        .media_type
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "image" => MediaType::Image,
        "video" => MediaType::Video,
        "audio" | "voice" => MediaType::Audio,
        "file" => MediaType::File,
        _ => {
            if is_desktop_image_path(&directive.path) {
                MediaType::Image
            } else if is_desktop_video_path(&directive.path) {
                MediaType::Video
            } else if is_desktop_audio_path(&directive.path) {
                MediaType::Audio
            } else {
                MediaType::File
            }
        }
    };

    let file_name = directive.name.unwrap_or_else(|| {
        Path::new(&directive.path)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "attachment".to_string())
    });

    Some(DesktopParsedMediaItem {
        media_type,
        file_name,
        file_path: directive.path,
    })
}

fn parse_desktop_markdown_media(line: &str) -> Option<DesktopParsedMediaItem> {
    let reference = media_directives::parse_markdown_media_reference(line)?;
    if !Path::new(&reference.path).is_absolute() {
        return None;
    }

    let media_type = if line.trim().starts_with("![") || is_desktop_image_path(&reference.path) {
        MediaType::Image
    } else if is_desktop_video_path(&reference.path) {
        MediaType::Video
    } else if is_desktop_audio_path(&reference.path) {
        MediaType::Audio
    } else {
        MediaType::File
    };

    let file_name = Path::new(&reference.path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string());

    Some(DesktopParsedMediaItem {
        media_type,
        file_name,
        file_path: reference.path,
    })
}

fn parse_desktop_plain_path_media(line: &str) -> Option<DesktopParsedMediaItem> {
    let reference = media_directives::parse_plain_media_path_reference(line)?;
    let path = reference.path;

    let media_type = if is_desktop_image_path(&path) {
        MediaType::Image
    } else if is_desktop_video_path(&path) {
        MediaType::Video
    } else if is_desktop_audio_path(&path) {
        MediaType::Audio
    } else {
        MediaType::File
    };

    let file_name = Path::new(&path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string());

    Some(DesktopParsedMediaItem {
        media_type,
        file_name,
        file_path: path,
    })
}

fn split_desktop_text_and_media(content: &str) -> (String, Vec<DesktopParsedMediaItem>) {
    let mut text_lines = Vec::new();
    let mut media_items = Vec::new();

    for line in content.lines() {
        if let Some(item) = parse_desktop_media_directive(line)
            .or_else(|| parse_desktop_markdown_media(line))
            .or_else(|| parse_desktop_plain_path_media(line))
        {
            media_items.push(item);
        } else {
            text_lines.push(line);
        }
    }

    (text_lines.join("\n").trim().to_string(), media_items)
}

fn desktop_media_type_slug(media_type: &MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "image",
        MediaType::Video => "video",
        MediaType::Audio => "audio",
        MediaType::File => "file",
    }
}

fn desktop_media_label(media_type: &MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "图片",
        MediaType::Video => "视频",
        MediaType::Audio => "语音",
        MediaType::File => "文件",
    }
}

fn build_desktop_outbound_display_text(
    text_reply: &str,
    media_items: &[DesktopParsedMediaItem],
) -> String {
    let mut lines: Vec<String> = Vec::new();
    if !text_reply.trim().is_empty() {
        lines.push(text_reply.trim().to_string());
    }
    for item in media_items {
        lines.push(media_directives::build_media_directive_line(
            desktop_media_type_slug(&item.media_type),
            &item.file_path,
            Some(&item.file_name),
            Some(desktop_media_label(&item.media_type)),
        ));
    }
    lines.join("\n")
}

fn persist_desktop_outbound_media_items(
    agent_id: Option<&str>,
    media_items: Vec<DesktopParsedMediaItem>,
) -> Vec<DesktopParsedMediaItem> {
    let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return media_items;
    };

    media_items
        .into_iter()
        .map(|item| {
            let resolved_path =
                agent_workspace::resolve_agent_media_reference(Some(agent_id), &item.file_path)
                    .unwrap_or_else(|| PathBuf::from(&item.file_path));
            let source_path = resolved_path.as_path();
            match agent_workspace::persist_agent_outbound_artifact(
                agent_id,
                "desktop-local",
                &item.file_name,
                source_path,
            ) {
                Ok(stable_path) => {
                    let _ = agent_workspace::register_agent_outbound_artifact_source(
                        agent_id,
                        &item.file_name,
                        &stable_path,
                        None,
                        Some("user=desktop-local channel=desktop"),
                    );
                    DesktopParsedMediaItem {
                        file_path: stable_path.display().to_string(),
                        ..item
                    }
                }
                Err(error) => {
                    log::warn!("归档桌面出站媒体失败 {}: {}", item.file_path, error);
                    item
                }
            }
        })
        .collect()
}

fn finalize_desktop_outbound_reply(content: &str, agent_id: Option<&str>) -> String {
    let (text_reply, media_items) = split_desktop_text_and_media(content);
    if media_items.is_empty() {
        return content.to_string();
    }
    let media_items = persist_desktop_outbound_media_items(agent_id, media_items);
    build_desktop_outbound_display_text(&text_reply, &media_items)
}

#[cfg(test)]
fn desktop_incomplete_reply_error(
    provider_config: Option<&ProviderRuntimeConfig>,
    saw_prompt_response: bool,
    saw_any_output: bool,
    saw_assistant_activity: bool,
) -> Option<String> {
    if provider_config.is_none() {
        return Some(
            "当前会话没有传入 Provider 配置，NineClaw 只能依赖系统 pi 默认模型；这次默认配置没有返回 assistant 回复。请在设置中启用至少一个 Provider，或先在终端确认 `pi --mode rpc` 能独立工作。"
                .to_string(),
        );
    }

    if saw_assistant_activity {
        return Some(
            "pi 已输出部分 assistant 内容，但没有返回完成事件。请重试；若持续出现，请检查当前 pi 版本是否变更了 RPC 输出行为。"
                .to_string(),
        );
    }

    if !saw_prompt_response && !saw_any_output {
        let provider_config = provider_config?;
        if let Err(error) = validate_desktop_provider_config(provider_config) {
            return Some(error);
        }
        return Some(format!(
            "pi 已启动但没有产生任何输出（provider={}，model={}）。请检查该 Provider 的 Base URL、模型可用性，以及当前 pi 版本是否支持该 RPC 运行方式。",
            provider_config.provider_id.trim(),
            provider_config.model.trim()
        ));
    }

    let provider_config = provider_config?;

    if provider_config.provider_id.trim().is_empty()
        || provider_config.base_url.trim().is_empty()
        || provider_config.model.trim().is_empty()
    {
        return Some(
            "pi 已接收消息，但当前桌面会话没有关联完整的 Provider 配置，因此没有返回 assistant 回复。请在设置中补全 Base URL、API Key 和模型后重试。"
                .to_string(),
        );
    }

    let provider_id = provider_config.provider_id.trim();
    let provider_label = if provider_config.api_format.trim().is_empty() {
        provider_id.to_string()
    } else {
        format!(
            "{} ({})",
            provider_id,
            normalize_provider_api_format(&provider_config.api_format, provider_id)
        )
    };

    Some(format!(
        "pi 已接收消息，但没有返回 assistant 回复（provider={}，model={}）。请检查 Provider 配置或确认当前 pi 版本是否变更了 RPC 输出行为。",
        provider_label,
        provider_config.model.trim()
    ))
}

#[cfg(test)]
mod lib_tests {
    use super::{
        aggregate_usage_from_agent_messages, build_desktop_outbound_display_text,
        desktop_incomplete_reply_error, desktop_media_reply_prompt, infer_media_mime_type,
        prepend_multimodal_summary_context, record_multimodal_summary,
        render_multimodal_summary_context, summary_file_path, DesktopParsedMediaItem,
        ProviderRuntimeConfig,
    };
    use crate::channels::types::MediaType;
    use std::fs;
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    fn write_temp_file(extension: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "nineclaw-mime-test-{}.{}",
            Uuid::new_v4(),
            extension
        ));
        fs::write(&path, bytes).expect("write temp media file");
        path
    }

    #[test]
    fn infer_media_mime_type_prefers_magic_bytes_over_misleading_extension() {
        let path = write_temp_file("png", &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F']);
        let mime = infer_media_mime_type(&path, None);
        let _ = fs::remove_file(&path);
        assert_eq!(mime, "image/jpeg");
    }

    #[test]
    fn infer_media_mime_type_detects_png_from_header() {
        let path = write_temp_file("bin", &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let mime = infer_media_mime_type(&path, None);
        let _ = fs::remove_file(&path);
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn multimodal_summary_context_roundtrip() {
        let key = format!("test-summary-{}", Uuid::new_v4());
        let path = summary_file_path(&key);
        let _ = fs::remove_file(&path);

        record_multimodal_summary(&key, "第一张图里有什么", "这是封面页，标题是季度复盘")
            .expect("record summary");
        record_multimodal_summary(&key, "第二张图里有什么", "这是目录页，包含三部分")
            .expect("record second summary");

        let context = render_multimodal_summary_context(&key)
            .expect("render summary context")
            .expect("summary context exists");
        assert!(context.contains("第一张图里有什么"));
        assert!(context.contains("这是目录页"));

        let prompt = prepend_multimodal_summary_context("继续看第三张图", &key)
            .expect("prepend summary context");
        assert!(prompt.contains("原始图片/视频已从主会话上下文移除"));
        assert!(prompt.contains("继续看第三张图"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn desktop_media_reply_prompt_mentions_outbox_when_agent_home_exists() {
        let prompt = desktop_media_reply_prompt(Some(Path::new("/tmp/agent-home")));
        assert!(prompt.contains("::nc-media"));
        assert!(prompt.contains("/tmp/agent-home/outbox"));
    }

    #[test]
    fn desktop_outbound_display_text_rebuilds_media_directives() {
        let display_text = build_desktop_outbound_display_text(
            "这是文件",
            &[DesktopParsedMediaItem {
                media_type: MediaType::File,
                file_name: "weekly report.pdf".to_string(),
                file_path: "/tmp/weekly report.pdf".to_string(),
            }],
        );

        assert_eq!(
            display_text,
            "这是文件\n::nc-media{type=\"file\" path=\"/tmp/weekly%20report.pdf\" name=\"weekly%20report.pdf\" label=\"%E6%96%87%E4%BB%B6\"}"
        );
    }

    #[test]
    fn desktop_incomplete_reply_error_prefers_partial_assistant_message() {
        let provider = ProviderRuntimeConfig {
            provider_id: "openai".to_string(),
            api_format: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "gpt-4.1".to_string(),
        };
        let reason =
            desktop_incomplete_reply_error(Some(&provider), true, true, true).expect("partial reply reason");
        assert!(reason.contains("部分 assistant 内容"));
    }

    #[test]
    fn desktop_incomplete_reply_error_reports_missing_provider_fields() {
        let provider = ProviderRuntimeConfig {
            provider_id: "openai".to_string(),
            api_format: "openai".to_string(),
            base_url: String::new(),
            api_key: "secret".to_string(),
            model: "gpt-4.1".to_string(),
        };

        let reason = desktop_incomplete_reply_error(Some(&provider), true, true, false)
            .expect("missing provider reason");
        assert!(reason.contains("没有关联完整的 Provider 配置"));
    }

    #[test]
    fn aggregate_agent_end_sums_each_assistant_step() {
        use serde_json::json;

        let messages = vec![
            json!({"role": "user"}),
            json!({
                "role": "assistant",
                "usage": {"input": 8237, "output": 110, "cacheRead": 1088, "cacheWrite": 0, "totalTokens": 9435}
            }),
            json!({
                "role": "assistant",
                "usage": {"input": 221, "output": 45, "cacheRead": 9280, "cacheWrite": 0, "totalTokens": 9546}
            }),
            json!({
                "role": "assistant",
                "usage": {"input": 85, "output": 140, "cacheRead": 9472, "cacheWrite": 0, "totalTokens": 9697}
            }),
        ];
        let (usage, _) = aggregate_usage_from_agent_messages(&messages);
        let u = usage.expect("aggregated usage");
        assert_eq!(u.input_tokens, Some(8543));
        assert_eq!(u.output_tokens, Some(295));
        assert_eq!(u.cache_read_tokens, Some(19840));
        assert_eq!(u.cache_write_tokens, Some(0));
        assert_eq!(u.total_tokens, Some(28678));
    }
}

fn spawn_pi_stderr_logger<R>(reader: R, scope: &'static str, buffer: Arc<Mutex<String>>)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut line_count = 0usize;
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
                    line_count += 1;
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        dev_trace(scope, trimmed);
                    }
                    if let Ok(mut stderr) = buffer.lock() {
                        stderr.push_str(&line);
                        stderr.push('\n');
                    }
                }
                Err(error) => {
                    let message = error.to_string();
                    dev_trace(scope, format!("读取失败: {message}"));
                    if let Ok(mut stderr) = buffer.lock() {
                        stderr.push_str(&message);
                        stderr.push('\n');
                    }
                    break;
                }
            }
        }
        dev_trace(scope, format!("stderr_eof: lines={line_count}"));
    });
}

fn runtime_handle_store() -> &'static Mutex<HashMap<String, PiRuntimeHandle>> {
    PI_RUNTIME_HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn desktop_pi_pool() -> &'static Mutex<HashMap<String, DesktopPooledPi>> {
    DESKTOP_POOLED_PI.get_or_init(|| Mutex::new(HashMap::new()))
}

fn insert_runtime_handle(session_id: &str, handle: PiRuntimeHandle) -> Result<(), String> {
    let mut guard = runtime_handle_store()
        .lock()
        .map_err(|error| format!("无法锁定运行时句柄: {error}"))?;
    guard.insert(session_id.to_string(), handle);
    Ok(())
}

fn get_runtime_handle(session_id: &str) -> Result<Option<PiRuntimeHandle>, String> {
    let guard = runtime_handle_store()
        .lock()
        .map_err(|error| format!("无法读取运行时句柄: {error}"))?;
    Ok(guard.get(session_id).cloned())
}

fn remove_runtime_handle(session_id: &str) -> Result<(), String> {
    let mut guard = runtime_handle_store()
        .lock()
        .map_err(|error| format!("无法清理运行时句柄: {error}"))?;
    guard.remove(session_id);
    Ok(())
}

fn next_prompt_command_id(session_id: &str) -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("prompt-{session_id}-{nonce}")
}

fn send_pi_prompt_command(
    stdin: &Arc<Mutex<Option<ChildStdin>>>,
    prompt_id: &str,
    prepared_input: &prompt_attachments::PreparedPromptInput,
) -> Result<(), String> {
    let prompt_command = json!({
      "id": prompt_id,
      "type": "prompt",
      "message": &prepared_input.message,
      "images": &prepared_input.images,
    })
    .to_string();

    let mut stdin_guard = stdin
        .lock()
        .map_err(|error| format!("无法锁定 prompt stdin: {error}"))?;
    let stdin = stdin_guard
        .as_mut()
        .ok_or_else(|| "pi stdin 已关闭，无法写入 prompt".to_string())?;
    writeln!(stdin, "{prompt_command}").map_err(|error| format!("写入 prompt 失败: {error}"))?;
    stdin
        .flush()
        .map_err(|error| format!("刷新 stdin 失败: {error}"))?;
    Ok(())
}

fn maybe_repair_pi_runtime_from_command(command: &mut Command) -> bool {
    let runtime_root = command.get_envs().find_map(|(key, value)| {
        if key.to_string_lossy() == "NINECLAW_PI_RUNTIME_ROOT" {
            value.map(PathBuf::from)
        } else {
            None
        }
    });

    let Some(runtime_root) = runtime_root else {
        return false;
    };

    if let Err(error) = pi_runtime::repair_runtime_directory(&runtime_root) {
        dev_trace(
            "desktop.stream",
            format!(
                "pi 启动前自动修复 runtime 失败: root={} error={}",
                runtime_root.display(),
                error
            ),
        );
        return false;
    }

    dev_trace(
        "desktop.stream",
        format!(
            "pi 启动前已自动修复 runtime 目录，将重试一次: {}",
            runtime_root.display()
        ),
    );
    true
}

fn spawn_desktop_pi_child_fresh(
    mut command: Command,
    normalized_session_id: &str,
    prepared_input: &prompt_attachments::PreparedPromptInput,
) -> Result<
    (
        Child,
        Arc<Mutex<Option<ChildStdin>>>,
        mpsc::Receiver<Result<String, String>>,
        Arc<Mutex<String>>,
    ),
    String,
> {
    let mut child = loop {
        let mut child = command
            .spawn()
            .map_err(|error| format!("调用 pi 失败，请确认已安装并在 PATH 中: {error}"))?;

        // Give the kernel a moment to load the binary; detect immediate exit
        // before wiring up I/O.
        thread::sleep(Duration::from_millis(50));
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("检查 pi 进程状态失败: {error}"))?
        {
            let exit_info = describe_exit_status(status);
            let repaired = maybe_repair_pi_runtime_from_command(&mut command);
            if repaired {
                thread::sleep(Duration::from_millis(150));
                continue;
            }

            return Err(format!(
                "pi 进程启动后立即退出 ({exit_info})，pid={}。\
                 NineClaw 已自动检查运行时目录；这通常意味着动态库加载失败，\
                 或系统安全策略仍阻止了执行。",
                child.id(),
            ));
        }

        break child;
    };

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "无法获取 pi stdin".to_string())?;
    let stdin = Arc::new(Mutex::new(Some(stdin)));

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取 pi 输出".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法读取 pi 错误输出".to_string())?;
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let stderr_buffer = Arc::new(Mutex::new(String::new()));
    spawn_pi_stdout_logger(stdout, "desktop.stream.raw", stdout_tx);
    spawn_pi_stderr_logger(stderr, "desktop.stream.stderr", stderr_buffer.clone());

    if let Err(error) = send_pi_prompt_command(
        &stdin,
        &format!("prompt-{}", normalized_session_id),
        prepared_input,
    ) {
        close_pi_stdin(&stdin);
        kill_child_with_trace(
            &mut child,
            Some(normalized_session_id),
            "spawn_desktop_pi_child_fresh:initial_prompt_write_failed",
        );
        let _ = wait_for_child_exit_with_trace(
            &mut child,
            CHILD_KILL_GRACE_TIMEOUT,
            Some(normalized_session_id),
            "spawn_desktop_pi_child_fresh:after_initial_prompt_write_failed_kill",
        );
        return Err(error);
    }

    Ok((child, stdin, stdout_rx, stderr_buffer))
}

fn kill_child_with_trace(child: &mut Child, session_id: Option<&str>, reason: &str) {
    dev_trace(
        "desktop.stream",
        format!(
            "kill pi: session={} pid={} reason={}",
            session_id.unwrap_or("-"),
            child.id(),
            reason
        ),
    );
    let _ = child.kill();
}

fn describe_exit_status(status: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return format!("signal:{sig}");
        }
    }
    if let Some(code) = status.code() {
        format!("code:{code}")
    } else {
        "unknown".to_string()
    }
}

fn wait_for_child_exit_with_trace(
    child: &mut Child,
    timeout: Duration,
    session_id: Option<&str>,
    reason: &str,
) -> Result<ChildExitOutcome, String> {
    let started_at = Instant::now();

    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("等待 pi 进程状态失败: {error}"))?
        {
            dev_trace(
                "desktop.stream",
                format!(
                    "wait_for_child_exit done: session={} pid={} elapsed_ms={} status={} reason={}",
                    session_id.unwrap_or("-"),
                    child.id(),
                    started_at.elapsed().as_millis(),
                    describe_exit_status(status),
                    reason
                ),
            );
            return Ok(ChildExitOutcome {
                status: Some(status),
                timed_out: false,
            });
        }

        if started_at.elapsed() >= timeout {
            dev_trace(
                "desktop.stream",
                format!(
                    "wait_for_child_exit timeout: session={} pid={} timeout_ms={} reason={}",
                    session_id.unwrap_or("-"),
                    child.id(),
                    timeout.as_millis(),
                    reason
                ),
            );
            kill_child_with_trace(child, session_id, "wait_for_child_exit:timeout_force_kill");
            let kill_started_at = Instant::now();
            while kill_started_at.elapsed() < CHILD_KILL_GRACE_TIMEOUT {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|error| format!("等待被终止的 pi 进程失败: {error}"))?
                {
                    dev_trace(
                        "desktop.stream",
                        format!(
                            "wait_for_child_exit done-after-kill: session={} pid={} elapsed_ms={} status={} reason={}",
                            session_id.unwrap_or("-"),
                            child.id(),
                            started_at.elapsed().as_millis(),
                            describe_exit_status(status),
                            reason
                        ),
                    );
                    return Ok(ChildExitOutcome {
                        status: Some(status),
                        timed_out: true,
                    });
                }
                thread::sleep(Duration::from_millis(25));
            }
            dev_trace(
                "desktop.stream",
                format!(
                    "wait_for_child_exit post-kill still no-exit: session={} pid={} grace_ms={} reason={}",
                    session_id.unwrap_or("-"),
                    child.id(),
                    CHILD_KILL_GRACE_TIMEOUT.as_millis(),
                    reason
                ),
            );
            return Ok(ChildExitOutcome {
                status: None,
                timed_out: true,
            });
        }

        thread::sleep(Duration::from_millis(25));
    }
}

fn close_pi_stdin(stdin: &Arc<Mutex<Option<ChildStdin>>>) {
    if let Ok(mut stdin_guard) = stdin.lock() {
        let _ = stdin_guard.take();
    }
}

fn kill_desktop_pooled_pi(mut pooled: DesktopPooledPi, session_id: Option<&str>, reason: &str) {
    close_pi_stdin(&pooled.stdin);
    kill_child_with_trace(
        &mut pooled.child,
        session_id,
        &format!("kill_desktop_pooled_pi:{reason}"),
    );
    let _ = wait_for_child_exit_with_trace(
        &mut pooled.child,
        CHILD_KILL_GRACE_TIMEOUT,
        session_id,
        &format!("kill_desktop_pooled_pi:{reason}:post_kill"),
    );
}

fn desktop_pi_fingerprint(
    provider_config: Option<&ProviderRuntimeConfig>,
    session_path: &str,
    system_prompt_sections: &[(String, String)],
    skill_paths: &[PathBuf],
) -> String {
    let mut blob: Vec<u8> = Vec::new();
    let provider_id = provider_config
        .map(|config| config.provider_id.trim())
        .unwrap_or_default();
    let api_format = provider_config
        .map(|config| config.api_format.trim())
        .unwrap_or_default();
    let base_url = provider_config
        .map(|config| config.base_url.trim())
        .unwrap_or_default();
    let api_key = provider_config
        .map(|config| config.api_key.trim())
        .unwrap_or_default();
    let model = provider_config
        .map(|config| config.model.trim())
        .unwrap_or_default();

    for value in [
        provider_id,
        api_format,
        base_url,
        api_key,
        model,
        session_path,
    ] {
        blob.extend_from_slice(value.as_bytes());
        blob.push(0);
    }

    for (label, content) in system_prompt_sections {
        blob.extend_from_slice(label.as_bytes());
        blob.push(1);
        blob.extend_from_slice(content.as_bytes());
        blob.push(2);
    }

    for path in skill_paths {
        blob.extend_from_slice(path.as_os_str().as_encoded_bytes());
        blob.push(3);
    }

    format!("{:x}", Md5::digest(&blob))
}

fn take_pooled_desktop_pi(session_id: &str) -> Result<Option<DesktopPooledPi>, String> {
    let mut guard = desktop_pi_pool()
        .lock()
        .map_err(|error| format!("无法锁定桌面 pi 进程池: {error}"))?;
    Ok(guard.remove(session_id))
}

fn store_pooled_desktop_pi(session_id: &str, pooled: DesktopPooledPi) -> Result<(), String> {
    let mut guard = desktop_pi_pool()
        .lock()
        .map_err(|error| format!("无法写入桌面 pi 进程池: {error}"))?;
    let replaced = guard.insert(session_id.to_string(), pooled);
    drop(guard);
    if let Some(previous) = replaced {
        kill_desktop_pooled_pi(previous, Some(session_id), "pool_replace_previous");
    }
    Ok(())
}

fn drain_pooled_desktop_pi() -> Result<Vec<DesktopPooledPi>, String> {
    let mut guard = desktop_pi_pool()
        .lock()
        .map_err(|error| format!("无法清理桌面 pi 进程池: {error}"))?;
    Ok(guard.drain().map(|(_, pooled)| pooled).collect())
}

fn hash_session_id(session_id: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(session_id.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn session_summary_key(session_id: &str) -> String {
    hash_session_id(session_id)
}

fn session_file_path(session_id: Option<&str>) -> PathBuf {
    let key = session_id
        .filter(|value| !value.trim().is_empty())
        .map(hash_session_id)
        .unwrap_or_else(|| "default".to_string());
    std::env::temp_dir().join(format!("{PI_SESSION_FILE_PREFIX}{key}.jsonl"))
}

fn ephemeral_session_file_path(session_id: Option<&str>) -> PathBuf {
    let key = session_id
        .filter(|value| !value.trim().is_empty())
        .map(hash_session_id)
        .unwrap_or_else(|| "default".to_string());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{PI_SESSION_FILE_PREFIX}{key}-media-{nonce}.jsonl"))
}

fn summary_file_path(summary_key: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{PI_SUMMARY_FILE_PREFIX}{summary_key}.json"))
}

fn normalize_summary_text(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = compact.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let truncated = trimmed
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| &trimmed[..index])
        .unwrap_or(trimmed);
    format!("{truncated}…")
}

fn load_multimodal_summary_entries(
    summary_key: &str,
) -> Result<Vec<MultimodalSummaryEntry>, String> {
    let path = summary_file_path(summary_key);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("读取多模态摘要失败 {}: {error}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("解析多模态摘要失败 {}: {error}", path.display()))
}

pub(crate) fn render_multimodal_summary_context(
    summary_key: &str,
) -> Result<Option<String>, String> {
    let entries = load_multimodal_summary_entries(summary_key)?;
    if entries.is_empty() {
        return Ok(None);
    }

    let mut lines =
        vec!["以下是此前多模态附件轮次的文字摘要，原始图片/视频已从主会话上下文移除：".to_string()];
    for entry in entries
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        lines.push(format!("- 用户: {}", entry.user_prompt));
        lines.push(format!("  结果: {}", entry.assistant_response));
    }

    Ok(Some(lines.join("\n")))
}

pub(crate) fn prepend_multimodal_summary_context(
    prompt: &str,
    summary_key: &str,
) -> Result<String, String> {
    let Some(summary_context) = render_multimodal_summary_context(summary_key)? else {
        return Ok(prompt.to_string());
    };

    let trimmed_prompt = prompt.trim();
    if trimmed_prompt.is_empty() {
        Ok(summary_context)
    } else {
        Ok(format!("{summary_context}\n\n{trimmed_prompt}"))
    }
}

pub(crate) fn record_multimodal_summary(
    summary_key: &str,
    user_prompt: &str,
    assistant_response: &str,
) -> Result<(), String> {
    let user_prompt = normalize_summary_text(user_prompt, 240);
    let assistant_response = normalize_summary_text(assistant_response, 1200);
    if user_prompt.is_empty() || assistant_response.is_empty() {
        return Ok(());
    }

    let mut entries = load_multimodal_summary_entries(summary_key)?;
    entries.push(MultimodalSummaryEntry {
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        user_prompt,
        assistant_response,
    });

    if entries.len() > 8 {
        let keep_from = entries.len() - 8;
        entries.drain(0..keep_from);
    }

    let path = summary_file_path(summary_key);
    let content = serde_json::to_vec_pretty(&entries)
        .map_err(|error| format!("序列化多模态摘要失败: {error}"))?;
    fs::write(&path, content)
        .map_err(|error| format!("写入多模态摘要失败 {}: {error}", path.display()))
}

fn session_cleanup_paths(session_id: Option<&str>) -> Vec<PathBuf> {
    let key = session_id
        .filter(|value| !value.trim().is_empty())
        .map(hash_session_id)
        .unwrap_or_else(|| "default".to_string());

    let mut paths = vec![std::env::temp_dir().join(format!("{PI_SESSION_FILE_PREFIX}{key}.jsonl"))];
    paths.push(summary_file_path(&key));
    for prefix in LEGACY_PI_SESSION_FILE_PREFIXES {
        paths.push(std::env::temp_dir().join(format!("{prefix}{key}.jsonl")));
    }
    paths
}

fn migrate_legacy_history_db(app_data_dir: &Path, target_path: &Path) -> Result<(), String> {
    if target_path.exists() {
        return Ok(());
    }

    for legacy_name in LEGACY_HISTORY_DB_FILES {
        let legacy_path = app_data_dir.join(legacy_name);
        if !legacy_path.exists() {
            continue;
        }

        fs::rename(&legacy_path, target_path)
            .or_else(|rename_error| {
                fs::copy(&legacy_path, target_path)
                    .map_err(|copy_error| {
                        std::io::Error::new(
                            copy_error.kind(),
                            format!("rename 失败({rename_error})，copy 也失败: {copy_error}"),
                        )
                    })
                    .and_then(|_| fs::remove_file(&legacy_path))
            })
            .map_err(|error| format!("迁移旧历史数据库失败: {error}"))?;

        break;
    }

    Ok(())
}

fn migrate_history_db_from_candidates(
    candidate_dirs: &[PathBuf],
    target_path: &Path,
) -> Result<(), String> {
    if target_path.exists() {
        return Ok(());
    }

    for candidate_dir in candidate_dirs {
        if !candidate_dir.exists() {
            continue;
        }
        migrate_legacy_history_db(candidate_dir, target_path)?;

        let legacy_target = candidate_dir.join(HISTORY_DB_FILE);
        if !legacy_target.exists() || target_path.exists() {
            continue;
        }

        fs::rename(&legacy_target, target_path)
            .or_else(|rename_error| {
                fs::copy(&legacy_target, target_path)
                    .map_err(|copy_error| {
                        std::io::Error::new(
                            copy_error.kind(),
                            format!("rename 失败({rename_error})，copy 也失败: {copy_error}"),
                        )
                    })
                    .and_then(|_| fs::remove_file(&legacy_target))
            })
            .map_err(|error| format!("迁移历史数据库失败: {error}"))?;
    }

    Ok(())
}

pub(crate) fn history_db_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let workspace_root = agent_workspace::resolve_workspace_root()?;
    fs::create_dir_all(&workspace_root)
        .map_err(|error| format!("创建共享 workspace 根目录失败: {error}"))?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("读取应用数据目录失败: {error}"))?;

    fs::create_dir_all(&app_data_dir).map_err(|error| format!("创建应用数据目录失败: {error}"))?;

    let target_path = workspace_root.join(HISTORY_DB_FILE);
    migrate_history_db_from_candidates(&[workspace_root.clone(), app_data_dir], &target_path)?;

    Ok(target_path)
}

pub(crate) fn ensure_app_state_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS app_state (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at INTEGER NOT NULL
      )",
            [],
        )
        .map_err(|error| format!("初始化历史数据库失败: {error}"))?;

    Ok(())
}

fn ensure_token_usage_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS token_usage_records (
              turn_id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL,
              turn_created_at INTEGER NOT NULL,
              turn_completed_at INTEGER,
              agent_id TEXT,
              agent_name TEXT,
              api TEXT,
              provider TEXT,
              model TEXT,
              response_id TEXT,
              usage_timestamp INTEGER,
              input_tokens INTEGER NOT NULL DEFAULT 0,
              output_tokens INTEGER NOT NULL DEFAULT 0,
              cache_read_tokens INTEGER NOT NULL DEFAULT 0,
              cache_write_tokens INTEGER NOT NULL DEFAULT 0,
              total_tokens INTEGER NOT NULL DEFAULT 0,
              recorded_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_token_usage_records_turn_completed_at
              ON token_usage_records(turn_completed_at DESC);
            CREATE INDEX IF NOT EXISTS idx_token_usage_records_model
              ON token_usage_records(model);
            CREATE INDEX IF NOT EXISTS idx_token_usage_records_agent_name
              ON token_usage_records(agent_name);",
        )
        .map_err(|error| format!("初始化用量数据库失败: {error}"))?;

    Ok(())
}

fn json_i64(value: Option<&serde_json::Value>) -> Option<i64> {
    value.and_then(|item| {
        item.as_i64()
            .or_else(|| item.as_u64().and_then(|number| i64::try_from(number).ok()))
    })
}

fn json_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn upsert_usage_record_from_snapshot(
    connection: &Connection,
    session_id: &str,
    agent_id: Option<&str>,
    agent_name: Option<&str>,
    session_model: Option<&str>,
    turn: &serde_json::Value,
    recorded_at: i64,
) -> Result<(), String> {
    let Some(turn_obj) = turn.as_object() else {
        return Ok(());
    };

    let turn_id =
        json_string(turn_obj.get("id")).ok_or_else(|| "历史快照中的 turn 缺少 id".to_string())?;
    let turn_created_at = json_i64(turn_obj.get("createdAt")).unwrap_or(recorded_at);
    let turn_completed_at = json_i64(turn_obj.get("completedAt"));
    let usage = turn_obj.get("usage");
    let usage_payload = extract_usage_payload(usage);
    let Some(usage_payload) = usage_payload else {
        return Ok(());
    };

    let usage_meta = extract_usage_metadata_payload(usage);
    let api = usage_meta.as_ref().and_then(|item| item.api.clone());
    let provider = usage_meta.as_ref().and_then(|item| item.provider.clone());
    let model = usage_meta
        .as_ref()
        .and_then(|item| item.model.clone())
        .or_else(|| session_model.map(ToOwned::to_owned));
    let response_id = usage_meta
        .as_ref()
        .and_then(|item| item.response_id.clone());
    let usage_timestamp = usage_meta.as_ref().and_then(|item| item.timestamp);

    connection
        .execute(
            "INSERT INTO token_usage_records (
              turn_id,
              session_id,
              turn_created_at,
              turn_completed_at,
              agent_id,
              agent_name,
              api,
              provider,
              model,
              response_id,
              usage_timestamp,
              input_tokens,
              output_tokens,
              cache_read_tokens,
              cache_write_tokens,
              total_tokens,
              recorded_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            ON CONFLICT(turn_id) DO UPDATE SET
              session_id = excluded.session_id,
              turn_created_at = excluded.turn_created_at,
              turn_completed_at = excluded.turn_completed_at,
              agent_id = excluded.agent_id,
              agent_name = excluded.agent_name,
              api = COALESCE(excluded.api, token_usage_records.api),
              provider = COALESCE(excluded.provider, token_usage_records.provider),
              model = COALESCE(excluded.model, token_usage_records.model),
              response_id = COALESCE(excluded.response_id, token_usage_records.response_id),
              usage_timestamp = COALESCE(excluded.usage_timestamp, token_usage_records.usage_timestamp),
              input_tokens = excluded.input_tokens,
              output_tokens = excluded.output_tokens,
              cache_read_tokens = excluded.cache_read_tokens,
              cache_write_tokens = excluded.cache_write_tokens,
              total_tokens = excluded.total_tokens,
              recorded_at = excluded.recorded_at",
            params![
                turn_id,
                session_id,
                turn_created_at,
                turn_completed_at,
                agent_id,
                agent_name,
                api,
                provider,
                model,
                response_id,
                usage_timestamp,
                usage_payload.input_tokens.unwrap_or(0),
                usage_payload.output_tokens.unwrap_or(0),
                usage_payload.cache_read_tokens.unwrap_or(0),
                usage_payload.cache_write_tokens.unwrap_or(0),
                usage_payload.total_tokens.unwrap_or(0),
                recorded_at,
            ],
        )
        .map_err(|error| format!("写入用量明细失败: {error}"))?;

    Ok(())
}

/// Persists PI/LLM token usage from scheduler-driven runs (任务中心 / 心跳定时) into `token_usage_records`.
pub(crate) fn record_token_usage_for_scheduler_pi_completion(
    app: &tauri::AppHandle,
    turn_id: String,
    session_label_id: &str,
    agent: &crate::agents::AgentRecord,
    session_model: &str,
    usage: Option<PiTokenUsagePayload>,
    usage_meta: Option<PiUsageMetadataPayload>,
) -> Result<(), String> {
    let Some(usage_payload) = usage else {
        return Ok(());
    };
    if usage_row_total_tokens(&usage_payload) == 0 {
        return Ok(());
    }

    let recorded_at = chrono_like_timestamp();
    let mut usage_value =
        serde_json::to_value(&usage_payload).map_err(|e| format!("序列化 scheduler usage 失败: {e}"))?;
    if let Some(meta) = usage_meta {
        if let serde_json::Value::Object(ref mut map) = usage_value {
            if let Some(ref v) = meta.api {
                map.insert("api".into(), serde_json::Value::String(v.clone()));
            }
            if let Some(ref v) = meta.provider {
                map.insert("provider".into(), serde_json::Value::String(v.clone()));
            }
            if let Some(ref v) = meta.model {
                map.insert("model".into(), serde_json::Value::String(v.clone()));
            }
            if let Some(ref v) = meta.response_id {
                map.insert("responseId".into(), serde_json::Value::String(v.clone()));
            }
            if let Some(v) = meta.timestamp {
                map.insert("timestamp".into(), serde_json::json!(v));
            }
        }
    }

    let turn = serde_json::json!({
        "id": turn_id,
        "createdAt": recorded_at,
        "completedAt": recorded_at,
        "usage": usage_value,
    });

    let connection = open_history_db(app)?;
    ensure_token_usage_schema(&connection)?;
    upsert_usage_record_from_snapshot(
        &connection,
        session_label_id,
        Some(agent.id.as_str()),
        Some(agent.name.as_str()),
        Some(session_model),
        &turn,
        recorded_at,
    )
}

fn sync_usage_records_from_history_payload(
    connection: &mut Connection,
    payload: &str,
) -> Result<(), String> {
    let parsed: serde_json::Value =
        serde_json::from_str(payload).map_err(|error| format!("解析历史快照失败: {error}"))?;
    let Some(history_items) = parsed.as_array() else {
        return Ok(());
    };

    let recorded_at = chrono_like_timestamp();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| format!("开启用量事务失败: {error}"))?;

    for item in history_items {
        let Some(item_obj) = item.as_object() else {
            continue;
        };

        let Some(session_id) = json_string(item_obj.get("id")) else {
            continue;
        };
        let agent = item_obj.get("agent").and_then(|value| value.as_object());
        let agent_id = agent.and_then(|value| json_string(value.get("id")));
        let agent_name = agent.and_then(|value| json_string(value.get("name")));
        let session_model = json_string(item_obj.get("sessionLlmModel"))
            .or_else(|| agent.and_then(|value| json_string(value.get("defaultModel"))));

        let Some(turns) = item_obj.get("turns").and_then(|value| value.as_array()) else {
            continue;
        };

        for turn in turns {
            upsert_usage_record_from_snapshot(
                &transaction,
                &session_id,
                agent_id.as_deref(),
                agent_name.as_deref(),
                session_model.as_deref(),
                turn,
                recorded_at,
            )?;
        }
    }

    transaction
        .commit()
        .map_err(|error| format!("提交用量事务失败: {error}"))?;

    Ok(())
}

pub(crate) fn open_history_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    let db_path = history_db_path(app)?;
    let connection =
        Connection::open(db_path).map_err(|error| format!("打开历史数据库失败: {error}"))?;

    ensure_app_state_schema(&connection)?;
    ensure_token_usage_schema(&connection)?;

    Ok(connection)
}

#[tauri::command]
fn load_history_state(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let connection = open_history_db(&app)?;
    connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![HISTORY_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取历史任务失败: {error}"))
}

#[tauri::command]
fn save_history_state(app: tauri::AppHandle, payload: String) -> Result<(), String> {
    let mut connection = open_history_db(&app)?;
    let updated_at = chrono_like_timestamp();

    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![HISTORY_STATE_KEY, payload, updated_at],
        )
        .map_err(|error| format!("保存历史任务失败: {error}"))?;

    sync_usage_records_from_history_payload(&mut connection, &payload)?;

    Ok(())
}

#[tauri::command]
fn clear_history_state(app: tauri::AppHandle) -> Result<(), String> {
    let connection = open_history_db(&app)?;
    connection
        .execute(
            "DELETE FROM app_state WHERE key = ?1",
            params![HISTORY_STATE_KEY],
        )
        .map_err(|error| format!("清空历史任务失败: {error}"))?;
    Ok(())
}

#[tauri::command]
fn list_token_usage_records(app: tauri::AppHandle) -> Result<Vec<TokenUsageRecordRow>, String> {
    let connection = open_history_db(&app)?;
    let mut statement = connection
        .prepare(
            "SELECT
              turn_id,
              session_id,
              turn_created_at,
              turn_completed_at,
              agent_id,
              agent_name,
              api,
              provider,
              model,
              response_id,
              usage_timestamp,
              input_tokens,
              output_tokens,
              cache_read_tokens,
              cache_write_tokens,
              total_tokens,
              recorded_at
            FROM token_usage_records
            ORDER BY COALESCE(turn_completed_at, turn_created_at) DESC, recorded_at DESC",
        )
        .map_err(|error| format!("查询用量明细失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(TokenUsageRecordRow {
                turn_id: row.get(0)?,
                session_id: row.get(1)?,
                turn_created_at: row.get(2)?,
                turn_completed_at: row.get(3)?,
                agent_id: row.get(4)?,
                agent_name: row.get(5)?,
                api: row.get(6)?,
                provider: row.get(7)?,
                model: row.get(8)?,
                response_id: row.get(9)?,
                usage_timestamp: row.get(10)?,
                input_tokens: row.get(11)?,
                output_tokens: row.get(12)?,
                cache_read_tokens: row.get(13)?,
                cache_write_tokens: row.get(14)?,
                total_tokens: row.get(15)?,
                recorded_at: row.get(16)?,
            })
        })
        .map_err(|error| format!("遍历用量明细失败: {error}"))?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|error| format!("读取用量明细失败: {error}"))?);
    }

    Ok(records)
}

#[tauri::command]
fn load_provider_preferences(app: tauri::AppHandle) -> Result<ProviderPreferencesPayload, String> {
    let connection = open_history_db(&app)?;

    let provider_configs = connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![PROVIDER_CONFIGS_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取 Provider 配置失败: {error}"))?;

    let custom_provider_meta = connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![CUSTOM_PROVIDER_META_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取自定义 Provider 元数据失败: {error}"))?;

    Ok(ProviderPreferencesPayload {
        provider_configs,
        custom_provider_meta,
    })
}

#[tauri::command]
fn save_provider_preferences(
    app: tauri::AppHandle,
    provider_configs_payload: String,
    custom_provider_meta_payload: String,
) -> Result<(), String> {
    let connection = open_history_db(&app)?;
    let updated_at = chrono_like_timestamp();

    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![
                PROVIDER_CONFIGS_STATE_KEY,
                provider_configs_payload,
                updated_at
            ],
        )
        .map_err(|error| format!("保存 Provider 配置失败: {error}"))?;

    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![
                CUSTOM_PROVIDER_META_STATE_KEY,
                custom_provider_meta_payload,
                updated_at
            ],
        )
        .map_err(|error| format!("保存自定义 Provider 元数据失败: {error}"))?;

    Ok(())
}

#[tauri::command]
fn list_installed_skills() -> Result<Vec<InstalledSkill>, String> {
    skills::list_installed_skills()
}

#[tauri::command]
fn list_system_skill_catalog(app: tauri::AppHandle) -> Result<SystemSkillCatalog, String> {
    skills::list_system_skill_catalog_for_app(&app)
}

#[tauri::command]
fn install_system_skill(app: tauri::AppHandle, skill_id: String) -> Result<InstalledSkill, String> {
    skills::install_system_skill(&app, &skill_id)
}

#[tauri::command]
fn list_agents(app: tauri::AppHandle) -> Result<Vec<AgentRecord>, String> {
    agents::list_agents(&app)
}

#[tauri::command]
fn get_default_agent(app: tauri::AppHandle) -> Result<Option<AgentRecord>, String> {
    agents::get_default_agent(&app)
}

#[tauri::command]
fn create_agent(app: tauri::AppHandle, payload: AgentInput) -> Result<AgentRecord, String> {
    agents::create_agent(&app, payload)
}

#[tauri::command]
fn update_agent(
    app: tauri::AppHandle,
    agent_id: String,
    payload: AgentInput,
) -> Result<AgentRecord, String> {
    agents::update_agent(&app, agent_id, payload)
}

#[tauri::command]
fn archive_agent(app: tauri::AppHandle, agent_id: String) -> Result<(), String> {
    agents::archive_agent(&app, agent_id)
}

#[tauri::command]
fn delete_agent(app: tauri::AppHandle, agent_id: String) -> Result<(), String> {
    agents::delete_agent(&app, agent_id)
}

#[tauri::command]
fn set_default_agent(
    app: tauri::AppHandle,
    agent_id: String,
) -> Result<Option<AgentRecord>, String> {
    agents::set_default_agent(&app, agent_id)
}

#[tauri::command]
fn read_agent_workspace_bundle(
    app: tauri::AppHandle,
    agent_id: String,
) -> Result<AgentWorkspaceBundle, String> {
    agents::read_agent_workspace_bundle(&app, agent_id)
}

#[tauri::command]
fn write_agent_workspace_file(
    app: tauri::AppHandle,
    agent_id: String,
    relative_path: String,
    content: String,
) -> Result<AgentWorkspaceBundle, String> {
    agents::write_agent_workspace_file(&app, agent_id, relative_path, content)
}

#[tauri::command]
fn list_scheduled_jobs(
    app: tauri::AppHandle,
) -> Result<Vec<scheduler::ScheduledJobRecord>, String> {
    scheduler::list_jobs(&app)
}

#[tauri::command]
fn list_scheduled_job_runs(
    app: tauri::AppHandle,
    limit: Option<u32>,
) -> Result<Vec<scheduler::ScheduledJobRunRecord>, String> {
    scheduler::list_job_runs(&app, limit)
}

#[tauri::command]
fn sync_scheduler_jobs(app: tauri::AppHandle) -> Result<scheduler::SchedulerSyncResult, String> {
    scheduler::sync_materialized_jobs(&app)
}

#[tauri::command]
fn trigger_scheduler_job_now(app: tauri::AppHandle, job_id: String) -> Result<(), String> {
    scheduler::trigger_job_now(&app, &job_id)
}

#[tauri::command]
fn get_scheduler_status(
    app: tauri::AppHandle,
) -> Result<scheduler::SchedulerRuntimeStatus, String> {
    scheduler::runtime_status(&app)
}

#[tauri::command]
fn install_scheduler_service() -> Result<scheduler::SchedulerServiceStatus, String> {
    scheduler::install_service()
}

#[tauri::command]
fn uninstall_scheduler_service() -> Result<scheduler::SchedulerServiceStatus, String> {
    scheduler::uninstall_service()
}

#[tauri::command]
fn handle_agent_task_prompt(
    app: tauri::AppHandle,
    prompt: String,
    session_id: String,
    agent_id: String,
) -> Result<AgentTaskPromptResult, String> {
    agent_tasks::handle_prompt(&app, &prompt, &session_id, &agent_id)
}

#[tauri::command]
fn list_agent_task_deliveries(
    app: tauri::AppHandle,
    session_ids: Vec<String>,
) -> Result<Vec<AgentTaskDeliveryRecord>, String> {
    agent_tasks::list_delivery_records(&app, &session_ids)
}

#[tauri::command]
fn list_agent_tasks(
    app: tauri::AppHandle,
    agent_id: Option<String>,
) -> Result<Vec<AgentTaskListItem>, String> {
    agent_tasks::list_tasks(&app, agent_id.as_deref())
}

#[tauri::command]
fn pause_agent_task(app: tauri::AppHandle, task_id: String) -> Result<(), String> {
    agent_tasks::pause_task(&app, &task_id)
}

#[tauri::command]
fn resume_agent_task(app: tauri::AppHandle, task_id: String) -> Result<(), String> {
    agent_tasks::resume_task(&app, &task_id)
}

#[tauri::command]
fn delete_agent_task(app: tauri::AppHandle, task_id: String) -> Result<(), String> {
    agent_tasks::delete_task(&app, &task_id)
}

#[tauri::command]
fn update_agent_task(
    app: tauri::AppHandle,
    task_id: String,
    payload: AgentTaskUpdateInput,
) -> Result<(), String> {
    agent_tasks::update_task(&app, &task_id, &payload)
}

#[tauri::command]
fn run_agent_task_now(app: tauri::AppHandle, task_id: String) -> Result<(), String> {
    scheduler::trigger_agent_task_now(&app, &task_id)
}

const TASK_METADATA_LLM_CHUNK: usize = 512;

fn clamp_display_chars(value: &str, max_chars: usize) -> String {
    let t = value.trim();
    if t.chars().count() <= max_chars {
        return t.to_string();
    }
    t.chars().take(max_chars).collect()
}

fn parse_agent_task_metadata_llm_output(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    let slice = &trimmed[start..=end];
    let v: serde_json::Value = serde_json::from_str(slice).ok()?;
    let title = v
        .get("title")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let summary = v
        .get("summary")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    Some((
        clamp_display_chars(&title, 28),
        clamp_display_chars(&summary, 120),
    ))
}

/// 用智能体绑定的模型把任务正文改写成列表标题 + 一句话介绍；失败返回 `None`（由调用方回退）。
pub(crate) fn refine_agent_task_metadata(
    app: &AppHandle,
    agent_id: &str,
    goal: &str,
    task_type: &str,
    schedule_hint: &str,
) -> Option<(String, String)> {
    if std::env::var("NINECLAW_SKIP_TASK_METADATA_LLM")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
    {
        return None;
    }
    let goal = goal.trim();
    if goal.is_empty() {
        return None;
    }
    let record = agents::get_agent_record(app, agent_id).ok().flatten()?;
    let agent_config = agents::get_conversation_agent_config(app, agent_id).ok().flatten()?;
    let runtime = resolve_im_llm_runtime(
        app,
        &record.default_provider_id,
        &record.default_model,
    )
    .ok()?;
    let base_normalized = normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );
    let pi_rt = pi_runtime::require_pi_runtime_location(app).ok()?;
    let bridge = PiBridge::new(
        pi_rt,
        &runtime.provider_id,
        &runtime.api_format,
        &base_normalized,
        &runtime.api_key,
        &runtime.model,
        Some(agent_config),
    );
    let channel_id = format!("nc:taskmeta:{agent_id}");
    let user_id = format!("refine_{}", uuid::Uuid::new_v4().simple());
    let task_type_label = match task_type.trim() {
        "reminder" => "reminder（到点仅提醒）",
        "agent_prompt" => "agent_prompt（到点由智能体执行）",
        other => other,
    };
    let prompt = format!(
        "你是 NineClaw 定时任务在列表里的展示文案编辑。根据「任务正文」生成 **title**（列表标题）和 **summary**（一句话介绍）。\n\
不要执行任何任务、不要编造正文中没有的需求、不要输出思考过程。\n\
规则：\n\
- title：4～20 个字左右的短名，不用书名号，不要用「定时任务」开头\n\
- summary：20～100 字，概括要做什么或提醒什么；不要逐字复制正文开头；具体触发时间已在其它列展示，summary 里不必重复钟点\n\
\n\
任务类型：{task_type_label}\n\
调度（帮助理解语境）：{schedule_hint}\n\
\n\
任务正文：\n\
{goal}\n\
\n\
只输出一行合法 JSON，不要 markdown 代码块，格式：{{\"title\":\"...\",\"summary\":\"...\"}}"
    );
    let outcome = bridge
        .process_message_interruptible(
            &channel_id,
            &user_id,
            &prompt,
            TASK_METADATA_LLM_CHUNK,
            |_| {},
            |_| {},
        )
        .ok()?;
    let text = match outcome {
        PiProcessOutcome::Completed(r) => r.full_text,
        PiProcessOutcome::Aborted => return None,
    };
    let parsed = parse_agent_task_metadata_llm_output(&text);
    if parsed.is_none() {
        dev_trace(
            "task.meta",
            format!(
                "LLM 元数据解析失败，输出前 200 字：{}",
                text.chars().take(200).collect::<String>()
            ),
        );
    }
    parsed
}

pub(crate) fn chrono_like_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn pi_runtime_dir() -> PathBuf {
    std::env::temp_dir().join(PI_RUNTIME_DIR_NAME)
}

fn default_provider_api_format(provider_id: &str) -> &'static str {
    match provider_id {
        "anthropic" => "anthropic",
        _ => "openai",
    }
}

fn normalize_provider_base_url(value: &str) -> &str {
    value.trim().trim_end_matches('/')
}

fn normalize_anthropic_base_url(value: &str) -> String {
    normalize_provider_base_url(value)
        .trim_end_matches("/v1/messages")
        .trim_end_matches("/messages")
        .trim_end_matches("/v1")
        .trim_end_matches('/')
        .to_string()
}

fn anthropic_messages_url(base_url: &str) -> String {
    if base_url.ends_with("/v1") {
        format!("{base_url}/messages")
    } else {
        format!("{base_url}/v1/messages")
    }
}

fn normalize_provider_api_format(value: &str, provider_id: &str) -> &'static str {
    match value.trim() {
        "anthropic" => "anthropic",
        "openai" => "openai",
        _ => default_provider_api_format(provider_id),
    }
}

/// `pi-ai` OpenAI-compat：推理类模型需开启 `supportsReasoningEffort`，否则部分网关/模型组合下 RPC 可能无 stdout 事件。
pub(crate) fn openai_pi_compat_supports_reasoning_effort(model: &str) -> bool {
    let m = model.trim().to_ascii_lowercase();
    m.contains("gpt-5")
        || m.contains("reasoning")
        || m.contains("thinking")
        || m.contains("-think")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
        // 智谱 GLM-4/5 等：未开启时 OpenAI 兼容路径下常见「pi 已启动但无任何 stdout」
        || m.contains("glm")
        // DeepSeek 推理链
        || m.contains("deepseek-r1")
        || m.contains("deepseek-reasoner")
        // Moonshot / Kimi（含 SiliconFlow 等聚合上的 Pro/moonshotai/...）：部分 OpenAI 兼容路径需 reasoning 标志才有 stdout 事件
        || m.contains("kimi")
        || m.contains("moonshot")
}

/// 微信/飞书 IM 必须使用绑定智能体的默认模型；Base URL / API Key 从应用全局 Provider 配置读取。
pub(crate) fn resolve_im_llm_runtime(
    app: &AppHandle,
    default_provider_id: &str,
    default_model: &str,
) -> Result<ProviderRuntimeConfig, String> {
    let provider_id = default_provider_id.trim().to_string();
    let model = default_model.trim().to_string();
    if provider_id.is_empty() || model.is_empty() {
        return Err("智能体未配置默认 Provider 或模型".to_string());
    }

    let prefs = load_provider_preferences(app.clone())?;
    let raw = prefs
        .provider_configs
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    if raw.is_empty() {
        return Err("未找到全局 Provider 配置".to_string());
    }

    let map: HashMap<String, StoredProviderPrefsRow> =
        serde_json::from_str(&raw).map_err(|e| format!("解析 Provider 配置失败: {e}"))?;

    let row = map
        .get(provider_id.as_str())
        .ok_or_else(|| format!("全局设置中未找到 Provider「{provider_id}」"))?;

    if !row.added {
        return Err(format!("请先在设置中添加 Provider「{provider_id}」"));
    }

    let base_url = row.base_url.trim().to_string();
    let api_key = row.api_key.trim().to_string();
    if base_url.is_empty() || api_key.is_empty() {
        return Err(format!(
            "Provider「{provider_id}」的 Base URL 或 API Key 未填写完整"
        ));
    }

    let api_format_raw = row.api_format.trim();
    let api_format = if api_format_raw.is_empty() {
        default_provider_api_format(&provider_id).to_string()
    } else {
        normalize_provider_api_format(api_format_raw, &provider_id).to_string()
    };

    Ok(ProviderRuntimeConfig {
        provider_id,
        api_format,
        base_url,
        api_key,
        model,
    })
}

pub(crate) fn normalized_provider_runtime_base_url(
    base_url: &str,
    api_format: &str,
    provider_id: &str,
) -> String {
    match normalize_provider_api_format(api_format, provider_id) {
        "anthropic" => normalize_anthropic_base_url(base_url),
        _ => normalize_provider_base_url(base_url).to_string(),
    }
}

fn runtime_provider_id(provider_id: &str) -> String {
    let trimmed = provider_id.trim();
    if trimmed.is_empty() {
        return "nineclaw-runtime-provider".to_string();
    }

    let digest = format!("{:x}", Md5::digest(trimmed.as_bytes()));
    format!("nineclaw-runtime-{}", &digest[..12])
}

#[derive(Clone, Debug)]
struct PiAnthropicCompatExtension {
    path: PathBuf,
    provider_id: String,
    api_key_env: String,
}

fn runtime_provider_suffix(provider_id: &str) -> String {
    let trimmed = provider_id.trim();
    if trimmed.is_empty() {
        return "provider".to_string();
    }

    let digest = format!("{:x}", Md5::digest(trimmed.as_bytes()));
    digest[..12].to_string()
}

fn anthropic_compat_provider_id(provider_id: &str) -> String {
    format!("nineclaw-compat-{}", runtime_provider_suffix(provider_id))
}

fn anthropic_compat_api_key_env(provider_id: &str) -> String {
    format!(
        "NINECLAW_PI_COMPAT_API_KEY_{}",
        runtime_provider_suffix(provider_id).to_ascii_uppercase()
    )
}

fn should_use_desktop_anthropic_compat_extension(provider_config: &ProviderRuntimeConfig) -> bool {
    let provider_id = provider_config.provider_id.trim();
    normalize_provider_api_format(&provider_config.api_format, provider_id) == "anthropic"
        && provider_id != "anthropic"
}

fn resolve_pi_ai_import_path(pi_executable: &Path) -> Option<PathBuf> {
    let resolved = fs::canonicalize(pi_executable).unwrap_or_else(|_| pi_executable.to_path_buf());
    let mut roots: Vec<PathBuf> = Vec::new();

    if let Some(parent) = resolved.parent() {
        roots.push(parent.to_path_buf());
    }
    if let Some(parent) = pi_executable.parent() {
        if !roots.iter().any(|entry| entry == parent) {
            roots.push(parent.to_path_buf());
        }
    }

    for root in roots {
        for ancestor in root.ancestors() {
            for candidate in [
                ancestor
                    .join("pi-package")
                    .join("node_modules")
                    .join("@mariozechner")
                    .join("pi-ai")
                    .join("dist")
                    .join("index.js"),
                ancestor
                    .join("node_modules")
                    .join("@mariozechner")
                    .join("pi-ai")
                    .join("dist")
                    .join("index.js"),
                ancestor
                    .join("node_modules")
                    .join("@mariozechner")
                    .join("pi-coding-agent")
                    .join("node_modules")
                    .join("@mariozechner")
                    .join("pi-ai")
                    .join("dist")
                    .join("index.js"),
            ] {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn js_string_literal(value: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("序列化 JS 字面量失败: {error}"))
}

fn build_desktop_anthropic_compat_extension_source(
    pi_ai_import_path: &Path,
    provider_id: &str,
    api_key_env: &str,
    base_url: &str,
    model: &str,
) -> Result<String, String> {
    let import_path = js_string_literal(&pi_ai_import_path.to_string_lossy())?;
    let provider_id = js_string_literal(provider_id)?;
    let api_key_env = js_string_literal(api_key_env)?;
    let base_url = js_string_literal(base_url)?;
    let model = js_string_literal(model)?;

    Ok(format!(
        r#"import {{ createAssistantMessageEventStream, calculateCost, parseStreamingJson }} from {import_path};

function sanitizeSurrogates(text) {{
  // 仅替换未成对的 UTF-16 代理项。旧实现会误伤所有 BMP 外字符（emoji 等），全部变成 U+FFFD。
  return String(text ?? '').replace(
    /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/g,
    '\uFFFD',
  );
}}

function anthropicMessagesUrl(baseUrl) {{
  const trimmed = String(baseUrl ?? '').trim().replace(/\/+$/, '');
  return trimmed.endsWith('/v1') ? trimmed + '/messages' : trimmed + '/v1/messages';
}}

function convertContentBlocks(content) {{
  const hasImages = content.some((block) => block.type === 'image');
  if (!hasImages) {{
    return sanitizeSurrogates(content.map((block) => block.text).join('\n'));
  }}

  const blocks = content.map((block) => {{
    if (block.type === 'text') {{
      return {{ type: 'text', text: sanitizeSurrogates(block.text) }};
    }}

    return {{
      type: 'image',
      source: {{
        type: 'base64',
        media_type: block.mimeType,
        data: block.data,
      }},
    }};
  }});

  if (!blocks.some((block) => block.type === 'text')) {{
    blocks.unshift({{ type: 'text', text: '(see attached image)' }});
  }}

  return blocks;
}}

function convertMessages(messages) {{
  const params = [];

  for (let i = 0; i < messages.length; i += 1) {{
    const message = messages[i];

    if (message.role === 'user') {{
      if (typeof message.content === 'string') {{
        if (message.content.trim()) {{
          params.push({{ role: 'user', content: sanitizeSurrogates(message.content) }});
        }}
      }} else {{
        const blocks = message.content.map((item) =>
          item.type === 'text'
            ? {{ type: 'text', text: sanitizeSurrogates(item.text) }}
            : {{
                type: 'image',
                source: {{
                  type: 'base64',
                  media_type: item.mimeType,
                  data: item.data,
                }},
              }},
        );
        if (blocks.length > 0) {{
          params.push({{ role: 'user', content: blocks }});
        }}
      }}
      continue;
    }}

    if (message.role === 'assistant') {{
      const blocks = [];
      for (const block of message.content) {{
        if (block.type === 'text' && block.text.trim()) {{
          blocks.push({{ type: 'text', text: sanitizeSurrogates(block.text) }});
        }} else if (block.type === 'thinking' && block.thinking.trim()) {{
          blocks.push({{ type: 'text', text: sanitizeSurrogates(block.thinking) }});
        }} else if (block.type === 'toolCall') {{
          blocks.push({{
            type: 'tool_use',
            id: block.id,
            name: block.name,
            input: block.arguments,
          }});
        }}
      }}
      if (blocks.length > 0) {{
        params.push({{ role: 'assistant', content: blocks }});
      }}
      continue;
    }}

    if (message.role === 'toolResult') {{
      const toolResults = [{{
        type: 'tool_result',
        tool_use_id: message.toolCallId,
        content: convertContentBlocks(message.content),
        is_error: message.isError,
      }}];

      let nextIndex = i + 1;
      while (nextIndex < messages.length && messages[nextIndex].role === 'toolResult') {{
        const nextMessage = messages[nextIndex];
        toolResults.push({{
          type: 'tool_result',
          tool_use_id: nextMessage.toolCallId,
          content: convertContentBlocks(nextMessage.content),
          is_error: nextMessage.isError,
        }});
        nextIndex += 1;
      }}

      i = nextIndex - 1;
      params.push({{ role: 'user', content: toolResults }});
    }}
  }}

  if (params.length > 0) {{
    const last = params[params.length - 1];
    if (last.role === 'user' && Array.isArray(last.content)) {{
      const lastBlock = last.content[last.content.length - 1];
      if (lastBlock) {{
        lastBlock.cache_control = {{ type: 'ephemeral' }};
      }}
    }}
  }}

  return params;
}}

function convertTools(tools) {{
  return tools.map((tool) => ({{
    name: tool.name,
    description: tool.description,
    input_schema: {{
      type: 'object',
      properties: tool.parameters?.properties || {{}},
      required: tool.parameters?.required || [],
    }},
  }}));
}}

function mapStopReason(reason) {{
  switch (reason) {{
    case 'end_turn':
    case 'pause_turn':
    case 'stop_sequence':
      return 'stop';
    case 'max_tokens':
      return 'length';
    case 'tool_use':
      return 'toolUse';
    default:
      return 'error';
  }}
}}

async function* parseSSE(response) {{
  if (!response.body) {{
    return;
  }}

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  try {{
    while (true) {{
      const {{ done, value }} = await reader.read();
      if (done) {{
        break;
      }}

      buffer += decoder.decode(value, {{ stream: true }}).replace(/\r\n/g, '\n');
      let splitIndex = buffer.indexOf('\n\n');
      while (splitIndex !== -1) {{
        const chunk = buffer.slice(0, splitIndex);
        buffer = buffer.slice(splitIndex + 2);
        const dataLines = chunk
          .split('\n')
          .filter((line) => line.startsWith('data:'))
          .map((line) => line.slice(5).trim());
        if (dataLines.length > 0) {{
          const data = dataLines.join('\n').trim();
          if (data && data !== '[DONE]') {{
            try {{
              yield JSON.parse(data);
            }} catch {{}}
          }}
        }}
        splitIndex = buffer.indexOf('\n\n');
      }}
    }}

    buffer += decoder.decode().replace(/\r\n/g, '\n');
    if (buffer.trim()) {{
      const dataLines = buffer
        .split('\n')
        .filter((line) => line.startsWith('data:'))
        .map((line) => line.slice(5).trim());
      if (dataLines.length > 0) {{
        const data = dataLines.join('\n').trim();
        if (data && data !== '[DONE]') {{
          try {{
            yield JSON.parse(data);
          }} catch {{}}
        }}
      }}
    }}
  }} finally {{
    try {{
      await reader.cancel();
    }} catch {{}}
    try {{
      reader.releaseLock();
    }} catch {{}}
  }}
}}

function streamNineclawAnthropicCompat(model, context, options) {{
  const stream = createAssistantMessageEventStream();

  (async () => {{
    const output = {{
      role: 'assistant',
      content: [],
      api: model.api,
      provider: model.provider,
      model: model.id,
      usage: {{
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        totalTokens: 0,
        cost: {{ input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 }},
      }},
      stopReason: 'stop',
      timestamp: Date.now(),
    }};

    try {{
      const apiKey = options?.apiKey ?? '';
      const headers = Object.assign(
        {{
          'content-type': 'application/json',
          accept: 'text/event-stream',
          'anthropic-version': '2023-06-01',
          'x-api-key': apiKey,
          Authorization: 'Bearer ' + apiKey,
        }},
        model.headers || {{}},
        options?.headers || {{}},
      );

      const payload = {{
        model: model.id,
        messages: convertMessages(context.messages),
        max_tokens: options?.maxTokens || Math.floor(model.maxTokens / 3),
        stream: true,
      }};

      if (context.systemPrompt) {{
        payload.system = [{{
          type: 'text',
          text: sanitizeSurrogates(context.systemPrompt),
          cache_control: {{ type: 'ephemeral' }},
        }}];
      }}

      if (context.tools?.length) {{
        payload.tools = convertTools(context.tools);
      }}

      const response = await fetch(anthropicMessagesUrl(model.baseUrl), {{
        method: 'POST',
        headers,
        body: JSON.stringify(payload),
        signal: options?.signal,
      }});

      if (!response.ok) {{
        throw new Error(await response.text());
      }}

      stream.push({{ type: 'start', partial: output }});
      const blocks = output.content;
      let sawMessageStop = false;

      for await (const event of parseSSE(response)) {{
        if (event.type === 'message_start') {{
          output.responseId = event.message?.id;
          output.usage.input = event.message?.usage?.input_tokens || 0;
          output.usage.output = event.message?.usage?.output_tokens || 0;
          output.usage.cacheRead = event.message?.usage?.cache_read_input_tokens || 0;
          output.usage.cacheWrite = event.message?.usage?.cache_creation_input_tokens || 0;
          output.usage.totalTokens =
            output.usage.input + output.usage.output + output.usage.cacheRead + output.usage.cacheWrite;
          calculateCost(model, output.usage);
        }} else if (event.type === 'content_block_start') {{
          if (event.content_block?.type === 'text') {{
            output.content.push({{ type: 'text', text: '', index: event.index }});
            stream.push({{ type: 'text_start', contentIndex: output.content.length - 1, partial: output }});
          }} else if (event.content_block?.type === 'tool_use') {{
            output.content.push({{
              type: 'toolCall',
              id: event.content_block.id,
              name: event.content_block.name,
              arguments: event.content_block.input || {{}},
              partialJson: '',
              index: event.index,
            }});
            stream.push({{ type: 'toolcall_start', contentIndex: output.content.length - 1, partial: output }});
          }}
        }} else if (event.type === 'content_block_delta') {{
          const contentIndex = blocks.findIndex((block) => block.index === event.index);
          const block = blocks[contentIndex];
          if (!block) {{
            continue;
          }}

          if (event.delta?.type === 'text_delta' && block.type === 'text') {{
            block.text += event.delta.text;
            stream.push({{ type: 'text_delta', contentIndex, delta: event.delta.text, partial: output }});
          }} else if (event.delta?.type === 'input_json_delta' && block.type === 'toolCall') {{
            block.partialJson += event.delta.partial_json;
            block.arguments = parseStreamingJson(block.partialJson);
            stream.push({{
              type: 'toolcall_delta',
              contentIndex,
              delta: event.delta.partial_json,
              partial: output,
            }});
          }}
        }} else if (event.type === 'content_block_stop') {{
          const contentIndex = blocks.findIndex((block) => block.index === event.index);
          const block = blocks[contentIndex];
          if (!block) {{
            continue;
          }}

          delete block.index;
          if (block.type === 'text') {{
            stream.push({{ type: 'text_end', contentIndex, content: block.text, partial: output }});
          }} else if (block.type === 'toolCall') {{
            block.arguments = parseStreamingJson(block.partialJson);
            delete block.partialJson;
            stream.push({{ type: 'toolcall_end', contentIndex, toolCall: block, partial: output }});
          }}
        }} else if (event.type === 'message_delta') {{
          if (event.delta?.stop_reason) {{
            output.stopReason = mapStopReason(event.delta.stop_reason);
          }}
          if (event.usage?.input_tokens != null) {{
            output.usage.input = event.usage.input_tokens;
          }}
          if (event.usage?.output_tokens != null) {{
            output.usage.output = event.usage.output_tokens;
          }}
          if (event.usage?.cache_read_input_tokens != null) {{
            output.usage.cacheRead = event.usage.cache_read_input_tokens;
          }}
          if (event.usage?.cache_creation_input_tokens != null) {{
            output.usage.cacheWrite = event.usage.cache_creation_input_tokens;
          }}
          output.usage.totalTokens =
            output.usage.input + output.usage.output + output.usage.cacheRead + output.usage.cacheWrite;
          calculateCost(model, output.usage);
        }} else if (event.type === 'message_stop') {{
          sawMessageStop = true;
          break;
        }}
      }}

      if (options?.signal?.aborted) {{
        throw new Error('Request was aborted');
      }}
      if (!sawMessageStop) {{
        throw new Error('Anthropic-compatible stream ended before message_stop');
      }}
      if (output.stopReason === 'aborted' || output.stopReason === 'error') {{
        throw new Error('Anthropic-compatible stream ended without a valid stop reason');
      }}

      stream.push({{ type: 'done', reason: output.stopReason, message: output }});
      stream.end();
    }} catch (error) {{
      for (const block of output.content) {{
        delete block.index;
      }}
      output.stopReason = options?.signal?.aborted ? 'aborted' : 'error';
      output.errorMessage = error instanceof Error ? error.message : JSON.stringify(error);
      stream.push({{ type: 'error', reason: output.stopReason, error: output }});
      stream.end();
    }}
  }})();

  return stream;
}}

export default function(pi) {{
  pi.registerProvider({provider_id}, {{
    baseUrl: {base_url},
    apiKey: {api_key_env},
    api: 'nineclaw-anthropic-compat',
    models: [{{
      id: {model},
      name: {model},
      reasoning: false,
      input: ['text', 'image'],
      cost: {{ input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }},
      contextWindow: 200000,
      maxTokens: 16384,
    }}],
    streamSimple: streamNineclawAnthropicCompat,
  }});
}}
"#,
        import_path = import_path,
        provider_id = provider_id,
        api_key_env = api_key_env,
        base_url = base_url,
        model = model,
    ))
}

fn prepare_desktop_anthropic_compat_extension(
    runtime_dir: &Path,
    pi_executable: &Path,
    provider_config: &ProviderRuntimeConfig,
) -> Result<PiAnthropicCompatExtension, String> {
    let pi_ai_import_path = resolve_pi_ai_import_path(pi_executable).ok_or_else(|| {
        format!(
            "无法定位 pi-ai 运行库，无法为 Provider「{}」生成 Anthropic 兼容扩展。",
            provider_config.provider_id.trim()
        )
    })?;
    let compat_provider_id = anthropic_compat_provider_id(provider_config.provider_id.trim());
    let compat_api_key_env = anthropic_compat_api_key_env(provider_config.provider_id.trim());
    let compat_base_url = normalized_provider_runtime_base_url(
        &provider_config.base_url,
        &provider_config.api_format,
        provider_config.provider_id.trim(),
    );
    let extension_source = build_desktop_anthropic_compat_extension_source(
        &pi_ai_import_path,
        &compat_provider_id,
        &compat_api_key_env,
        &compat_base_url,
        provider_config.model.trim(),
    )?;
    let extension_path = runtime_dir.join(format!("{}.mjs", compat_provider_id));
    fs::write(&extension_path, extension_source)
        .map_err(|error| format!("写入 Anthropic 兼容扩展失败: {error}"))?;

    Ok(PiAnthropicCompatExtension {
        path: extension_path,
        provider_id: compat_provider_id,
        api_key_env: compat_api_key_env,
    })
}

fn scrub_anthropic_process_env(command: &mut Command) {
    for key in [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_OAUTH_TOKEN",
    ] {
        command.env_remove(key);
    }
}

fn custom_provider_object(
    provider_config: &ProviderRuntimeConfig,
) -> serde_json::Map<String, serde_json::Value> {
    let base_url = normalized_provider_runtime_base_url(
        &provider_config.base_url,
        &provider_config.api_format,
        provider_config.provider_id.trim(),
    );
    let model = provider_config.model.trim();
    let api_format = normalize_provider_api_format(
        &provider_config.api_format,
        provider_config.provider_id.trim(),
    );
    let mut provider = serde_json::Map::new();
    provider.insert("baseUrl".to_string(), json!(base_url));
    provider.insert(
        "apiKey".to_string(),
        json!(if provider_config.api_key.trim().is_empty() {
            "DUMMY_KEY"
        } else {
            provider_config.api_key.trim()
        }),
    );
    match api_format {
        "anthropic" => {
            provider.insert("api".to_string(), json!("anthropic-messages"));
            // Some Anthropic-compatible gateways require both the standard
            // Anthropic headers and `Authorization: Bearer <key>`.
            provider.insert("authHeader".to_string(), json!(true));
            provider.insert(
                "models".to_string(),
                json!([
                  {
                    "id": model,
                    "api": "anthropic-messages",
                    "input": ["text", "image"]
                  }
                ]),
            );
        }
        _ => {
            provider.insert("api".to_string(), json!("openai-completions"));
            let reasoning = openai_pi_compat_supports_reasoning_effort(model);
            provider.insert(
                "compat".to_string(),
                json!({
                  "supportsDeveloperRole": false,
                  "supportsReasoningEffort": reasoning
                }),
            );
            provider.insert(
                "models".to_string(),
                json!([
                  {
                    "id": model,
                    "api": "openai-completions",
                    "input": ["text", "image"]
                  }
                ]),
            );
        }
    }
    provider
}

fn build_provider_models_config(
    provider_config: &ProviderRuntimeConfig,
) -> Option<serde_json::Value> {
    let provider_id = provider_config.provider_id.trim();
    let base_url = normalized_provider_runtime_base_url(
        &provider_config.base_url,
        &provider_config.api_format,
        provider_id,
    );
    let model = provider_config.model.trim();

    if provider_id.is_empty() || base_url.is_empty() || model.is_empty() {
        return None;
    }

    // Always materialize the selected provider into `models.json` so runtime
    // behavior matches the user's explicit UI configuration, even for built-in
    // providers on their default base URL.
    let provider = custom_provider_object(provider_config);
    let mut providers = serde_json::Map::new();
    providers.insert(
        runtime_provider_id(provider_id),
        serde_json::Value::Object(provider),
    );
    Some(json!({ "providers": providers }))
}

fn prepare_pi_runtime_dir(provider_config: &ProviderRuntimeConfig) -> Result<PathBuf, String> {
    let runtime_dir = pi_runtime_dir();
    fs::create_dir_all(&runtime_dir).map_err(|error| format!("创建 pi 运行目录失败: {error}"))?;

    let auth_path = runtime_dir.join("auth.json");
    fs::write(&auth_path, "{}").map_err(|error| format!("写入 pi auth 配置失败: {error}"))?;

    let models_path = runtime_dir.join("models.json");
    if let Some(models_config) = build_provider_models_config(provider_config) {
        let content = serde_json::to_vec_pretty(&models_config)
            .map_err(|error| format!("序列化 provider 配置失败: {error}"))?;
        fs::write(&models_path, content)
            .map_err(|error| format!("写入 provider models 配置失败: {error}"))?;
    } else if models_path.exists() {
        fs::remove_file(&models_path)
            .map_err(|error| format!("清理 provider models 配置失败: {error}"))?;
    }

    Ok(runtime_dir)
}

fn validate_desktop_provider_config(provider_config: &ProviderRuntimeConfig) -> Result<(), String> {
    if provider_config.provider_id.trim().is_empty() {
        return Err("当前会话的 Provider ID 为空，请重新选择一个已配置模型。".to_string());
    }

    if provider_config.base_url.trim().is_empty() {
        return Err(format!(
            "当前会话使用的 Provider「{}」未填写 Base URL，无法启动 pi。请在设置中补全后重试。",
            provider_config.provider_id.trim()
        ));
    }

    if provider_config.model.trim().is_empty() {
        return Err(format!(
            "当前会话使用的 Provider「{}」未填写模型名，无法启动 pi。请在设置中补全后重试。",
            provider_config.provider_id.trim()
        ));
    }

    Ok(())
}

fn emit_stream_event(
    app: &tauri::AppHandle,
    event: &str,
    session_id: Option<String>,
    text: Option<String>,
    error: Option<String>,
    aborted_by: Option<String>,
    tool_call_id: Option<String>,
    tool_name: Option<String>,
    args_text: Option<String>,
    result_text: Option<String>,
    is_error: Option<bool>,
    reason: Option<String>,
    usage: Option<PiTokenUsagePayload>,
) -> Result<(), String> {
    emit_stream_event_with_meta(
        app,
        event,
        session_id,
        text,
        error,
        aborted_by,
        tool_call_id,
        tool_name,
        args_text,
        result_text,
        is_error,
        reason,
        usage,
        None,
    )
}

fn emit_stream_event_with_meta(
    app: &tauri::AppHandle,
    event: &str,
    session_id: Option<String>,
    text: Option<String>,
    error: Option<String>,
    aborted_by: Option<String>,
    tool_call_id: Option<String>,
    tool_name: Option<String>,
    args_text: Option<String>,
    result_text: Option<String>,
    is_error: Option<bool>,
    reason: Option<String>,
    usage: Option<PiTokenUsagePayload>,
    usage_meta: Option<PiUsageMetadataPayload>,
) -> Result<(), String> {
    app.emit(
        "pi://stream",
        PiStreamPayload {
            event: event.to_string(),
            session_id,
            text,
            error,
            aborted_by,
            tool_call_id,
            tool_name,
            args_text,
            result_text,
            is_error,
            reason,
            usage,
            usage_meta,
        },
    )
    .map_err(|emit_error| format!("发送事件失败: {emit_error}"))
}

fn extract_json_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    value.and_then(|item| {
        item.as_u64()
            .or_else(|| item.as_i64().and_then(|number| u64::try_from(number).ok()))
    })
}

pub(crate) fn extract_usage_payload(
    value: Option<&serde_json::Value>,
) -> Option<PiTokenUsagePayload> {
    let usage = value?;
    let input_tokens = extract_json_u64(
        usage
            .get("input")
            .or_else(|| usage.get("input_tokens"))
            .or_else(|| usage.get("inputTokens")),
    );
    let output_tokens = extract_json_u64(
        usage
            .get("output")
            .or_else(|| usage.get("output_tokens"))
            .or_else(|| usage.get("outputTokens")),
    );
    let cache_read_tokens = extract_json_u64(
        usage
            .get("cacheRead")
            .or_else(|| usage.get("cache_read_tokens"))
            .or_else(|| usage.get("cacheReadTokens")),
    );
    let cache_write_tokens = extract_json_u64(
        usage
            .get("cacheWrite")
            .or_else(|| usage.get("cache_write_tokens"))
            .or_else(|| usage.get("cacheWriteTokens")),
    );
    let total_tokens = extract_json_u64(
        usage
            .get("totalTokens")
            .or_else(|| usage.get("total_tokens")),
    )
    .or_else(|| {
        Some(
            input_tokens.unwrap_or(0)
                + output_tokens.unwrap_or(0)
                + cache_read_tokens.unwrap_or(0)
                + cache_write_tokens.unwrap_or(0),
        )
    });

    if input_tokens.is_none()
        && output_tokens.is_none()
        && cache_read_tokens.is_none()
        && cache_write_tokens.is_none()
        && total_tokens.unwrap_or(0) == 0
    {
        return None;
    }

    Some(PiTokenUsagePayload {
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        total_tokens,
    })
}

fn usage_row_total_tokens(payload: &PiTokenUsagePayload) -> u64 {
    payload.total_tokens.unwrap_or_else(|| {
        payload.input_tokens.unwrap_or(0)
            + payload.output_tokens.unwrap_or(0)
            + payload.cache_read_tokens.unwrap_or(0)
            + payload.cache_write_tokens.unwrap_or(0)
    })
}

fn merge_pi_token_usage_payloads(left: &PiTokenUsagePayload, right: &PiTokenUsagePayload) -> PiTokenUsagePayload {
    let input = left.input_tokens.unwrap_or(0) + right.input_tokens.unwrap_or(0);
    let output = left.output_tokens.unwrap_or(0) + right.output_tokens.unwrap_or(0);
    let cache_read = left.cache_read_tokens.unwrap_or(0) + right.cache_read_tokens.unwrap_or(0);
    let cache_write = left.cache_write_tokens.unwrap_or(0) + right.cache_write_tokens.unwrap_or(0);
    let total = usage_row_total_tokens(left) + usage_row_total_tokens(right);
    PiTokenUsagePayload {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cache_read_tokens: Some(cache_read),
        cache_write_tokens: Some(cache_write),
        total_tokens: Some(total),
    }
}

/// 将多轮 assistant 调用的用量相加（一次用户任务内可能有多条带 `usage` 的 assistant 消息）。
pub(crate) fn accumulate_pi_token_usage(
    into: &mut Option<PiTokenUsagePayload>,
    step: Option<PiTokenUsagePayload>,
) {
    let Some(step) = step else {
        return;
    };
    *into = Some(match into.take() {
        None => step,
        Some(prev) => merge_pi_token_usage_payloads(&prev, &step),
    });
}

/// 从 `agent_end` 的 `messages` 数组汇总所有 assistant 的 `usage`，并取最后一条 assistant 的元数据。
pub(crate) fn aggregate_usage_from_agent_messages(
    messages: &[serde_json::Value],
) -> (Option<PiTokenUsagePayload>, Option<PiUsageMetadataPayload>) {
    let mut total: Option<PiTokenUsagePayload> = None;
    let mut last_meta: Option<PiUsageMetadataPayload> = None;
    for message in messages {
        if message.get("role").and_then(|item| item.as_str()) != Some("assistant") {
            continue;
        }
        let step = extract_usage_payload(message.get("usage"));
        accumulate_pi_token_usage(&mut total, step);
        if let Some(meta) = extract_usage_metadata_payload(Some(message)) {
            last_meta = Some(meta);
        }
    }
    (total, last_meta)
}

pub(crate) fn extract_usage_metadata_payload(
    value: Option<&serde_json::Value>,
) -> Option<PiUsageMetadataPayload> {
    let payload = value?;
    let api = json_string(payload.get("api"));
    let provider = json_string(payload.get("provider"));
    let model = json_string(payload.get("model"));
    let response_id = json_string(
        payload
            .get("responseId")
            .or_else(|| payload.get("response_id")),
    );
    let timestamp = json_i64(payload.get("timestamp"));

    if api.is_none()
        && provider.is_none()
        && model.is_none()
        && response_id.is_none()
        && timestamp.is_none()
    {
        return None;
    }

    Some(PiUsageMetadataPayload {
        api,
        provider,
        model,
        response_id,
        timestamp,
    })
}

fn extract_text_content(value: Option<&serde_json::Value>) -> Option<String> {
    let Some(content) = value
        .and_then(|item| item.get("content"))
        .and_then(|item| item.as_array())
    else {
        return None;
    };

    let joined = content
        .iter()
        .filter_map(|entry| entry.get("text").and_then(|text| text.as_str()))
        .collect::<Vec<_>>()
        .join("");

    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// 流式 `text_delta` 与 `message_update.done` / `message_end` / `agent_end` 中的正文快照在空白或拼接上
/// 可能略有差异；若 `strip_prefix` 失败就整段重发，会把同一段回复追加多遍（用户看到 2～3 条重复内容）。
fn assistant_text_fragment_to_append(emitted: &str, snapshot: &str) -> String {
    if snapshot.is_empty() {
        return String::new();
    }
    if emitted.is_empty() {
        return snapshot.to_string();
    }
    if emitted == snapshot {
        return String::new();
    }
    if let Some(rest) = snapshot.strip_prefix(emitted) {
        return rest.to_string();
    }
    let emitted_trim = emitted.trim_end();
    let snapshot_trim = snapshot.trim_end();
    if emitted_trim == snapshot_trim {
        return String::new();
    }
    if let Some(rest) = snapshot_trim.strip_prefix(emitted_trim) {
        return rest.to_string();
    }
    if let Some(rest) = snapshot.strip_prefix(emitted_trim) {
        return rest.to_string();
    }
    String::new()
}

fn resize_main_window_to_screen(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let monitor = match window.current_monitor() {
        Ok(monitor) => monitor,
        Err(error) => {
            eprintln!("NineClaw: 读取显示器信息失败，跳过窗口自适应: {error}");
            return;
        }
    };

    let Some(monitor) = monitor else {
        return;
    };

    let screen_size = monitor.size();
    let max_width = screen_size.width.saturating_sub(48);
    let max_height = screen_size.height.saturating_sub(64);
    let desired_width = ((screen_size.width as f64) * 0.84).round() as u32;
    let desired_height = ((screen_size.height as f64) * 0.82).round() as u32;
    let width = if max_width >= 1260 {
        desired_width.max(1260).min(max_width)
    } else {
        max_width
    };
    let height = if max_height >= 780 {
        desired_height.max(780).min(max_height)
    } else {
        max_height
    };

    if let Err(error) = window.set_size(Size::Physical(PhysicalSize::new(width, height))) {
        eprintln!("NineClaw: 设置窗口尺寸失败，保留默认尺寸: {error}");
    }

    if let Err(error) = window.center() {
        eprintln!("NineClaw: 窗口居中失败，保留当前窗口位置: {error}");
    }
}

fn extract_message_terminal_error(value: Option<&serde_json::Value>) -> Option<String> {
    let message = value?;
    let stop_reason = message
        .get("stopReason")
        .and_then(|item| item.as_str())
        .unwrap_or_default();

    let error_text = message
        .get("errorMessage")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);

    if stop_reason == "error" {
        return Some(error_text.unwrap_or_else(|| "pi 返回了空错误响应".to_string()));
    }

    error_text
}

#[tauri::command]
async fn abort_pi_stream(session_id: Option<String>) -> Result<(), String> {
    let Some(session_id) = session_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Err("缺少要停止的会话 ID".to_string());
    };

    let Some(handle) = get_runtime_handle(&session_id)? else {
        dev_trace(
            "desktop.stream",
            format!(
                "abort_pi_stream ignored: session={} no_runtime_handle",
                session_id
            ),
        );
        return Ok(());
    };
    dev_trace(
        "desktop.stream",
        format!("abort_pi_stream: session={} pid={}", session_id, handle.pid),
    );

    handle.abort_requested.store(true, Ordering::SeqCst);

    let abort_command = json!({
      "id": format!("abort-{session_id}"),
      "type": "abort",
    })
    .to_string();

    let sent_abort = {
        let mut stdin = handle
            .stdin
            .lock()
            .map_err(|error| format!("无法锁定 abort stdin: {error}"))?;
        if let Some(stdin) = stdin.as_mut() {
            writeln!(stdin, "{abort_command}")
                .map_err(|error| format!("发送 abort 指令失败: {error}"))?;
            stdin
                .flush()
                .map_err(|error| format!("刷新 abort 指令失败: {error}"))?;
            true
        } else {
            false
        }
    };

    if !sent_abort {
        let status = Command::new("kill")
            .args(["-TERM", &handle.pid.to_string()])
            .status()
            .map_err(|error| format!("中止 pi 进程失败: {error}"))?;

        if !status.success() {
            return Err(format!("中止 pi 进程失败，退出码: {status}"));
        }
    }

    Ok(())
}

#[tauri::command]
async fn clear_pi_session() -> Result<(), String> {
    dev_trace("desktop.stream", "clear_pi_session invoked".to_string());
    for pooled in drain_pooled_desktop_pi()? {
        kill_desktop_pooled_pi(pooled, None, "clear_pi_session");
    }

    for entry in
        fs::read_dir(std::env::temp_dir()).map_err(|error| format!("读取临时目录失败: {error}"))?
    {
        let entry = entry.map_err(|error| format!("读取 session 条目失败: {error}"))?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let is_known_session_file = file_name.starts_with(PI_SESSION_FILE_PREFIX)
            || LEGACY_PI_SESSION_FILE_PREFIXES
                .iter()
                .any(|prefix| file_name.starts_with(prefix));
        if !is_known_session_file || !file_name.ends_with(".jsonl") {
            continue;
        }
        fs::remove_file(path).map_err(|error| format!("清理 session 失败: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
async fn clear_pi_session_for_id(session_id: String) -> Result<(), String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    dev_trace(
        "desktop.stream",
        format!("clear_pi_session_for_id invoked: session={}", trimmed),
    );

    if let Some(pooled) = take_pooled_desktop_pi(trimmed)? {
        kill_desktop_pooled_pi(pooled, Some(trimmed), "clear_pi_session_for_id");
    }

    for path in session_cleanup_paths(Some(trimmed)) {
        if !path.exists() {
            continue;
        }

        fs::remove_file(&path).map_err(|error| format!("清理指定 session 失败: {error}"))?;
    }

    Ok(())
}

#[tauri::command]
async fn persist_chat_attachments(
    agent_id: String,
    session_id: Option<String>,
    attachments: Vec<ChatAttachmentUpload>,
) -> Result<Vec<PersistedChatAttachment>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        chat_attachments::persist_chat_attachments(&agent_id, session_id.as_deref(), attachments)
    })
    .await
    .map_err(|error| format!("持久化聊天附件失败: {error}"))?
}

#[tauri::command]
async fn open_local_file(file_path: String) -> Result<(), String> {
    let resolved_path = resolve_local_file_path(&file_path)?;

    tauri::async_runtime::spawn_blocking(move || open_path_in_default_app(&resolved_path))
        .await
        .map_err(|error| format!("打开本地文件失败: {error}"))?
}

#[tauri::command]
async fn open_external_url(url: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_url_in_default_browser(&url))
        .await
        .map_err(|error| format!("打开外部链接失败: {error}"))?
}

#[tauri::command]
async fn load_local_media_preview(
    file_path: String,
    mime_type: Option<String>,
) -> Result<String, String> {
    let resolved_path = resolve_local_file_path(&file_path)?;
    let mime_hint = mime_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    tauri::async_runtime::spawn_blocking(move || {
        let bytes = fs::read(&resolved_path)
            .map_err(|error| format!("读取本地媒体失败 {}: {error}", resolved_path.display()))?;
        let mime = infer_media_mime_type(&resolved_path, mime_hint.as_deref());
        Ok(format!(
            "data:{};base64,{}",
            mime,
            BASE64_ENGINE.encode(bytes)
        ))
    })
    .await
    .map_err(|error| format!("加载本地媒体预览失败: {error}"))?
}

#[tauri::command]
async fn stream_pi_prompt(
    app: tauri::AppHandle,
    prompt: String,
    session_id: Option<String>,
    provider_config: Option<ProviderRuntimeConfig>,
    agent_config: Option<ConversationAgentConfig>,
    attachments: Option<Vec<prompt_attachments::PromptAttachmentInput>>,
) -> Result<(), String> {
    let trimmed_prompt = prompt.trim().to_string();
    let attachments = attachments.unwrap_or_default();
    if trimmed_prompt.is_empty() && attachments.is_empty() {
        return Err("prompt 不能为空".to_string());
    }

    let normalized_session_id = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "default".to_string());
    let summary_key = session_summary_key(&normalized_session_id);
    let prompt_with_summary = prepend_multimodal_summary_context(&trimmed_prompt, &summary_key)?;
    let prepared_input =
        prompt_attachments::prepare_prompt_input(&prompt_with_summary, &attachments)?;

    tauri::async_runtime::spawn_blocking(move || {
        let session_stream_mutex = desktop_session_stream_mutex(&normalized_session_id);
        let _session_stream_guard = session_stream_mutex.try_lock().map_err(|_| {
            "该会话已有进行中的生成，请等待完成或先中止后再发。".to_string()
        })?;

        emit_stream_event(
            &app,
            "start",
            Some(normalized_session_id.clone()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )?;

        let fresh_multimodal_session = !prepared_input.images.is_empty();
        let session_path = if fresh_multimodal_session {
            ephemeral_session_file_path(Some(normalized_session_id.as_str()))
        } else {
            session_file_path(Some(normalized_session_id.as_str()))
        };
        let session_path_string = session_path.to_string_lossy().to_string();
        let pi_location = pi_runtime::require_pi_runtime_location(&app)?;
        let resolved_pi_path = pi_location.executable.display().to_string();
        if let Some(runtime_repair_msg) = pi_runtime::repair_runtime_and_report(&pi_location) {
            dev_trace("desktop.stream", runtime_repair_msg);
        }
        let mut command = Command::new(&pi_location.executable);
        pi_runtime::apply_runtime_environment(&mut command, &pi_location);
        command
            .args(["--mode", "rpc", "--session", &session_path_string])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(provider_config) = provider_config.as_ref() {
            validate_desktop_provider_config(provider_config)?;
            let runtime_dir = prepare_pi_runtime_dir(provider_config)?;
            command.env("PI_CODING_AGENT_DIR", &runtime_dir);

            if normalize_provider_api_format(
                &provider_config.api_format,
                provider_config.provider_id.trim(),
            ) == "anthropic"
            {
                scrub_anthropic_process_env(&mut command);
            }

            let compat_extension = if should_use_desktop_anthropic_compat_extension(provider_config)
            {
                Some(prepare_desktop_anthropic_compat_extension(
                    &runtime_dir,
                    &pi_location.executable,
                    provider_config,
                )?)
            } else {
                None
            };

            if let Some(compat_extension) = compat_extension.as_ref() {
                command
                    .env(&compat_extension.api_key_env, provider_config.api_key.trim())
                    .args([
                        "--extension",
                        compat_extension.path.to_string_lossy().as_ref(),
                    ]);
                dev_trace(
                    "desktop.stream",
                    format!(
                        "启用 Anthropic 兼容扩展: session={} provider={} compat_provider={} extension={}",
                        normalized_session_id,
                        provider_config.provider_id.trim(),
                        compat_extension.provider_id,
                        compat_extension.path.display(),
                    ),
                );
            }

            let runtime_provider_id = compat_extension
                .as_ref()
                .map(|item| item.provider_id.clone())
                .unwrap_or_else(|| runtime_provider_id(provider_config.provider_id.trim()));
            if !runtime_provider_id.is_empty() {
                command.args(["--provider", &runtime_provider_id]);
            }

            if !provider_config.model.trim().is_empty() {
                command.args(["--model", provider_config.model.trim()]);
            }

            if !provider_config.api_key.trim().is_empty() {
                command.args(["--api-key", provider_config.api_key.trim()]);
            }
        } else {
            dev_trace(
                "desktop.stream",
                format!(
                    "未传入 provider_config，session={} 将依赖系统 pi 默认配置",
                    normalized_session_id
                ),
            );
        }

        let mut system_prompt_chars = 0usize;
        let mut system_prompt_sections: Vec<(String, String)> = Vec::new();
        let mut skill_count = 0usize;
        let mut skill_paths: Vec<PathBuf> = Vec::new();
        let mut desktop_agent_home: Option<PathBuf> = None;
        if let Some(agent_config) = agent_config.as_ref() {
            if let Ok(workspace_root) = agent_workspace::resolve_workspace_root() {
                let agent_home = workspace_root.join("agents").join(&agent_config.id);
                desktop_agent_home = Some(agent_home.clone());
                command
                    .current_dir(&agent_home)
                    .env("NINECLAW_AGENT_ID", &agent_config.id)
                    .env("NINECLAW_AGENT_NAME", &agent_config.name)
                    .env("NINECLAW_WORKSPACE_ROOT", workspace_root.as_os_str())
                    .env("NINECLAW_AGENT_HOME", agent_home.as_os_str());
            }

            if let Some(system_prompt) = agents::build_agent_system_prompt_for_prompt(
                agent_config,
                Some(trimmed_prompt.as_str()),
            ) {
                system_prompt_chars += system_prompt.chars().count();
                system_prompt_sections.push((
                    "agent_system_prompt".to_string(),
                    system_prompt.clone(),
                ));
                command.args(["--append-system-prompt", &system_prompt]);
            }

            let memory_isolation_prompt = "记忆隔离规则：当前智能体只能使用自己的私有工作区记忆。禁止读取、引用、总结或迁移其他智能体 `agents/<other-agent-id>/` 下的任何 markdown 记忆文件。";
            system_prompt_chars += memory_isolation_prompt.chars().count();
            system_prompt_sections.push((
                "memory_isolation".to_string(),
                memory_isolation_prompt.to_string(),
            ));
            command.args([
                "--append-system-prompt",
                memory_isolation_prompt,
            ]);

            for skill_path in skills::resolve_skill_directories(&agent_config.skill_ids)? {
                skill_count += 1;
                skill_paths.push(skill_path.clone());
                let skill_path = skill_path.to_string_lossy().to_string();
                command.args(["--skill", &skill_path]);
            }
        }

        let desktop_media_prompt = desktop_media_reply_prompt(desktop_agent_home.as_deref());
        system_prompt_chars += desktop_media_prompt.chars().count();
        system_prompt_sections.push((
            "desktop_media".to_string(),
            desktop_media_prompt.clone(),
        ));
        command.args(["--append-system-prompt", &desktop_media_prompt]);

        dev_trace(
            "desktop.stream",
            format!(
                "准备启动 pi: session={} prompt_chars={} system_prompt_chars={} skill_count={} provider={} model={} agent={} pi_path={} images={} fresh_multimodal_session={} runtime_session={}",
                normalized_session_id,
                prepared_input.message.chars().count(),
                system_prompt_chars,
                skill_count,
                provider_config
                    .as_ref()
                    .map(|item| item.provider_id.as_str())
                    .unwrap_or("(default)"),
                provider_config
                    .as_ref()
                    .map(|item| item.model.as_str())
                    .unwrap_or("(default)"),
                agent_config
                    .as_ref()
                    .map(|item| item.id.as_str())
                    .unwrap_or("(none)"),
                resolved_pi_path,
                prepared_input.images.len(),
                fresh_multimodal_session,
                session_path.display(),
            ),
        );
        for (label, content) in &system_prompt_sections {
            dev_trace(
                "desktop.stream",
                format!(
                    "system_prompt_part: session={} label={} chars={}",
                    normalized_session_id,
                    label,
                    content.chars().count()
                ),
            );
            dev_trace_block(
                "desktop.stream",
                format!(
                    "system_prompt_part session={} label={} chars={}",
                    normalized_session_id,
                    label,
                    content.chars().count()
                ),
                content,
            );
        }

        let desktop_reuse_enabled = pi_reuse_desktop_enabled() && !fresh_multimodal_session;
        let desktop_fingerprint = desktop_pi_fingerprint(
            provider_config.as_ref(),
            &session_path_string,
            &system_prompt_sections,
            &skill_paths,
        );
        if fresh_multimodal_session {
            if let Some(pooled) = take_pooled_desktop_pi(&normalized_session_id)? {
                kill_desktop_pooled_pi(
                    pooled,
                    Some(&normalized_session_id),
                    "fresh_multimodal_session_reset_pool",
                );
            }
        }

        let (mut child, stdin, stdout_rx, stderr_buffer, reused_from_pool) =
            if desktop_reuse_enabled {
                if let Some(mut pooled) = take_pooled_desktop_pi(&normalized_session_id)? {
                    let pooled_alive = pooled
                        .child
                        .try_wait()
                        .map_err(|error| format!("检查桌面池化 pi 状态失败: {error}"))?
                        .is_none();
                    if pooled_alive && pooled.fingerprint == desktop_fingerprint {
                        send_pi_prompt_command(
                            &pooled.stdin,
                            &next_prompt_command_id(&normalized_session_id),
                            &prepared_input,
                        )?;
                        (
                            pooled.child,
                            pooled.stdin,
                            pooled.stdout_rx,
                            pooled.stderr_buffer,
                            true,
                        )
                    } else {
                        kill_desktop_pooled_pi(
                            pooled,
                            Some(&normalized_session_id),
                            "pooled_pi_unusable_or_fingerprint_mismatch",
                        );
                        let (child, stdin, stdout_rx, stderr_buffer) =
                            spawn_desktop_pi_child_fresh(
                                command,
                                &normalized_session_id,
                                &prepared_input,
                            )?;
                        (child, stdin, stdout_rx, stderr_buffer, false)
                    }
                } else {
                    let (child, stdin, stdout_rx, stderr_buffer) = spawn_desktop_pi_child_fresh(
                        command,
                        &normalized_session_id,
                        &prepared_input,
                    )?;
                    (child, stdin, stdout_rx, stderr_buffer, false)
                }
            } else {
                let (child, stdin, stdout_rx, stderr_buffer) = spawn_desktop_pi_child_fresh(
                    command,
                    &normalized_session_id,
                    &prepared_input,
                )?;
                (child, stdin, stdout_rx, stderr_buffer, false)
            };
        dev_trace(
            "desktop.stream",
            format!(
                "pi 已启动: session={} pid={} reused_pool={}",
                normalized_session_id,
                child.id(),
                reused_from_pool
            ),
        );

        let abort_requested = Arc::new(AtomicBool::new(false));
        insert_runtime_handle(
            &normalized_session_id,
            PiRuntimeHandle {
                abort_requested: abort_requested.clone(),
                pid: child.id(),
                stdin: stdin.clone(),
            },
        )?;

        let mut saw_agent_end = false;
        let mut saw_message_done = false;
        let mut saw_model_abort_event = false;
        let mut _saw_prompt_response = false;
        let mut _saw_assistant_activity = false;
        let mut saw_assistant_terminal_message = false;
        let mut done_emitted = false;
        let mut final_usage: Option<PiTokenUsagePayload> = None;
        let mut final_usage_meta: Option<PiUsageMetadataPayload> = None;
        let mut emitted_assistant_text = String::new();
        let mut assistant_terminal_error: Option<String> = None;
        let mut saw_any_output = false;
        let started_at = Instant::now();
        let ttft_start = Instant::now();
        let mut logged_ttft = false;
        let pi_total_runtime_timeout = pi_timeouts::pi_total_runtime_timeout();
        let pi_first_output_timeout = pi_timeouts::pi_first_output_timeout();
        let pi_idle_output_timeout = pi_timeouts::pi_idle_output_timeout();

        loop {
            if started_at.elapsed() >= pi_total_runtime_timeout {
                let timeout_error =
                    format!("pi 总运行超时（>{} 秒）", pi_total_runtime_timeout.as_secs());
                dev_trace(
                    "desktop.stream",
                    format!("pi 超时: session={} error={}", normalized_session_id, timeout_error),
                );
                kill_child_with_trace(
                    &mut child,
                    Some(&normalized_session_id),
                    "stream_pi_prompt:total_runtime_timeout",
                );
                let _ = wait_for_child_exit_with_trace(
                    &mut child,
                    CHILD_KILL_GRACE_TIMEOUT,
                    Some(&normalized_session_id),
                    "stream_pi_prompt:after_total_runtime_timeout_kill",
                );
                remove_runtime_handle(&normalized_session_id)?;
                emit_stream_event(
                    &app,
                    "error",
                    Some(normalized_session_id.clone()),
                    None,
                    Some(timeout_error.clone()),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )?;
                return Err(timeout_error);
            }

            let base_timeout = if saw_any_output {
                pi_idle_output_timeout
            } else {
                pi_first_output_timeout
            };
            let remaining_total = pi_total_runtime_timeout
                .checked_sub(started_at.elapsed())
                .unwrap_or(Duration::from_secs(0));
            let timeout = base_timeout.min(remaining_total);
            let line = match stdout_rx.recv_timeout(timeout) {
                Ok(Ok(current_line)) => {
                    if !logged_ttft {
                        logged_ttft = true;
                        dev_trace(
                            "desktop.stream",
                            format!(
                                "stdout_ttft_ms={} session={}",
                                ttft_start.elapsed().as_millis(),
                                normalized_session_id
                            ),
                        );
                    }
                    current_line
                }
                Ok(Err(error)) => {
                    remove_runtime_handle(&normalized_session_id)?;
                    if abort_requested.load(Ordering::SeqCst) {
                        emit_stream_event(
                            &app,
                            "aborted",
                            Some(normalized_session_id.clone()),
                            None,
                            None,
                            Some("user".to_string()),
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                        )?;
                        return Ok(());
                    }
                    return Err(format!("读取 pi 输出失败: {error}"));
                }
                Err(RecvTimeoutError::Timeout) => {
                    let timeout_error = if timeout == remaining_total {
                        format!("pi 总运行超时（>{} 秒）", pi_total_runtime_timeout.as_secs())
                    } else if saw_any_output {
                        format!("等待 pi 后续输出超时（>{} 秒）", pi_idle_output_timeout.as_secs())
                    } else {
                        format!("等待 pi 首包输出超时（>{} 秒）", pi_first_output_timeout.as_secs())
                    };
                    dev_trace(
                        "desktop.stream",
                        format!("pi 超时: session={} error={}", normalized_session_id, timeout_error),
                    );
                    kill_child_with_trace(
                        &mut child,
                        Some(&normalized_session_id),
                        "stream_pi_prompt:stdout_recv_timeout",
                    );
                    let _ = wait_for_child_exit_with_trace(
                        &mut child,
                        CHILD_KILL_GRACE_TIMEOUT,
                        Some(&normalized_session_id),
                        "stream_pi_prompt:after_stdout_recv_timeout_kill",
                    );
                    remove_runtime_handle(&normalized_session_id)?;
                    emit_stream_event(
                        &app,
                        "error",
                        Some(normalized_session_id.clone()),
                        None,
                        Some(timeout_error.clone()),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )?;
                    return Err(timeout_error);
                }
                Err(RecvTimeoutError::Disconnected) => break,
            };
            if !line.trim().is_empty() {
                saw_any_output = true;
            }

            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(parsed) => parsed,
                Err(error) => {
                    remove_runtime_handle(&normalized_session_id)?;
                    if abort_requested.load(Ordering::SeqCst) {
                        emit_stream_event(
                            &app,
                            "aborted",
                            Some(normalized_session_id.clone()),
                            None,
                            None,
                            Some("user".to_string()),
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                        )?;
                        return Ok(());
                    }
                    return Err(format!("解析 pi 输出失败: {error}"));
                }
            };

            let line_type = value
                .get("type")
                .and_then(|item| item.as_str())
                .unwrap_or_default();

            if line_type == "response" {
                let command = value
                    .get("command")
                    .and_then(|item| item.as_str())
                    .unwrap_or_default();
                let success = value
                    .get("success")
                    .and_then(|item| item.as_bool())
                    .unwrap_or(false);

                if command == "prompt" && success {
                    _saw_prompt_response = true;
                }

                if command == "prompt" && !success {
                    let error_text = value
                        .get("error")
                        .and_then(|item| item.as_str())
                        .unwrap_or("prompt 调用失败")
                        .to_string();
                    remove_runtime_handle(&normalized_session_id)?;
                    emit_stream_event(
                        &app,
                        "error",
                        Some(normalized_session_id.clone()),
                        None,
                        Some(error_text.clone()),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )?;
                    return Err(error_text);
                }
            }

            if line_type == "message_update" {
                let assistant_event = value.get("assistantMessageEvent");
                let delta_type = assistant_event
                    .and_then(|item| item.get("type"))
                    .and_then(|item| item.as_str())
                    .unwrap_or_default();

                if delta_type == "text_delta" {
                    let delta_text = assistant_event
                        .and_then(|item| item.get("delta"))
                        .and_then(|item| item.as_str())
                        .unwrap_or_default()
                        .to_string();
                    if !delta_text.is_empty() {
                        _saw_assistant_activity = true;
                        emitted_assistant_text.push_str(&delta_text);
                        emit_stream_event(
                            &app,
                            "delta",
                            Some(normalized_session_id.clone()),
                            Some(delta_text),
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                        )?;
                    }
                }

                if delta_type == "thinking_start" {
                    emit_stream_event(
                        &app,
                        "thinking_start",
                        Some(normalized_session_id.clone()),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )?;
                }

                if delta_type == "thinking_delta" {
                    let thinking_text = assistant_event
                        .and_then(|item| item.get("delta"))
                        .and_then(|item| item.as_str())
                        .unwrap_or_default()
                        .to_string();

                    if !thinking_text.is_empty() {
                        emit_stream_event(
                            &app,
                            "thinking_delta",
                            Some(normalized_session_id.clone()),
                            Some(thinking_text),
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                        )?;
                    }
                }

                if delta_type == "thinking_end" {
                    emit_stream_event(
                        &app,
                        "thinking_end",
                        Some(normalized_session_id.clone()),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )?;
                }

                if delta_type == "done" {
                    let msg_in_assistant = assistant_event.and_then(|item| item.get("message"));
                    let final_text =
                        extract_text_content(msg_in_assistant);
                    if let Some(snapshot) = final_text {
                        _saw_assistant_activity = true;
                        let missing_text =
                            assistant_text_fragment_to_append(&emitted_assistant_text, &snapshot);
                        if !missing_text.is_empty() {
                            emitted_assistant_text.push_str(&missing_text);
                            emit_stream_event(
                                &app,
                                "delta",
                                Some(normalized_session_id.clone()),
                                Some(missing_text),
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                            )?;
                        }
                    }

                    final_usage = extract_usage_payload(
                        assistant_event
                            .and_then(|item| item.get("message"))
                            .and_then(|item| item.get("usage")),
                    );
                    final_usage_meta = extract_usage_metadata_payload(
                        assistant_event.and_then(|item| item.get("message")),
                    );
                    saw_message_done = true;
                    break;
                }

                if delta_type == "error" {
                    let reason = assistant_event
                        .and_then(|item| item.get("reason"))
                        .and_then(|item| item.as_str())
                        .unwrap_or_default();
                    if reason == "aborted" && !abort_requested.load(Ordering::SeqCst) {
                        saw_model_abort_event = true;
                    }
                }
            }

            if matches!(line_type, "message_start" | "message_end" | "turn_end") {
                let message = value.get("message");
                let is_assistant = message
                    .and_then(|item| item.get("role"))
                    .and_then(|item| item.as_str())
                    == Some("assistant");

                if is_assistant {
                    let mut saw_snapshot = false;
                    if let Some(snapshot) = extract_text_content(message) {
                        saw_snapshot = true;
                        _saw_assistant_activity = true;
                        let missing_text =
                            assistant_text_fragment_to_append(&emitted_assistant_text, &snapshot);
                        if !missing_text.is_empty() {
                            emitted_assistant_text.push_str(&missing_text);
                            emit_stream_event(
                                &app,
                                "delta",
                                Some(normalized_session_id.clone()),
                                Some(missing_text),
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                            )?;
                        }
                    }

                    if assistant_terminal_error.is_none() {
                        assistant_terminal_error = extract_message_terminal_error(message);
                    }

                    let step_usage =
                        extract_usage_payload(message.and_then(|item| item.get("usage")));
                    if step_usage.is_some() {
                        accumulate_pi_token_usage(&mut final_usage, step_usage);
                        if let Some(meta) = extract_usage_metadata_payload(message) {
                            final_usage_meta = Some(meta);
                        }
                    }

                    if matches!(line_type, "message_end" | "turn_end")
                        && (saw_snapshot || !emitted_assistant_text.trim().is_empty())
                    {
                        saw_assistant_terminal_message = true;
                    }
                }
            }

            if line_type == "tool_execution_start" {
                let tool_call_id = value
                    .get("toolCallId")
                    .and_then(|item| item.as_str())
                    .map(|item| item.to_string());
                let tool_name = value
                    .get("toolName")
                    .and_then(|item| item.as_str())
                    .map(|item| item.to_string());
                let args_text = value.get("args").map(|item| item.to_string());

                emit_stream_event(
                    &app,
                    "tool_execution_start",
                    Some(normalized_session_id.clone()),
                    None,
                    None,
                    None,
                    tool_call_id,
                    tool_name,
                    args_text,
                    None,
                    None,
                    None,
                    None,
                )?;
            }

            if line_type == "tool_execution_update" {
                let tool_call_id = value
                    .get("toolCallId")
                    .and_then(|item| item.as_str())
                    .map(|item| item.to_string());
                let tool_name = value
                    .get("toolName")
                    .and_then(|item| item.as_str())
                    .map(|item| item.to_string());
                let args_text = value.get("args").map(|item| item.to_string());
                let result_text = extract_text_content(value.get("partialResult"));

                emit_stream_event(
                    &app,
                    "tool_execution_update",
                    Some(normalized_session_id.clone()),
                    None,
                    None,
                    None,
                    tool_call_id,
                    tool_name,
                    args_text,
                    result_text,
                    None,
                    None,
                    None,
                )?;
            }

            if line_type == "tool_execution_end" {
                let tool_call_id = value
                    .get("toolCallId")
                    .and_then(|item| item.as_str())
                    .map(|item| item.to_string());
                let tool_name = value
                    .get("toolName")
                    .and_then(|item| item.as_str())
                    .map(|item| item.to_string());
                let args_text = value.get("args").map(|item| item.to_string());
                let result_text = extract_text_content(value.get("result"));
                let is_error = value.get("isError").and_then(|item| item.as_bool());

                emit_stream_event(
                    &app,
                    "tool_execution_end",
                    Some(normalized_session_id.clone()),
                    None,
                    None,
                    None,
                    tool_call_id,
                    tool_name,
                    args_text,
                    result_text,
                    is_error,
                    None,
                    None,
                )?;
            }

            if line_type == "agent_end" {
                if let Some(messages) = value.get("messages").and_then(|item| item.as_array()) {
                    let (aggregated_usage, aggregated_meta) =
                        aggregate_usage_from_agent_messages(messages.as_slice());

                    if let Some(last_assistant) = messages.iter().rev().find(|message| {
                        message.get("role").and_then(|item| item.as_str()) == Some("assistant")
                    }) {
                        if let Some(snapshot) = extract_text_content(Some(last_assistant)) {
                            let missing_text =
                                assistant_text_fragment_to_append(&emitted_assistant_text, &snapshot);
                            if !missing_text.is_empty() {
                                emitted_assistant_text.push_str(&missing_text);
                                emit_stream_event(
                                    &app,
                                    "delta",
                                    Some(normalized_session_id.clone()),
                                    Some(missing_text),
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                )?;
                            }
                        }

                        if assistant_terminal_error.is_none() {
                            assistant_terminal_error =
                                extract_message_terminal_error(Some(last_assistant));
                        }
                    }

                    if let Some(usage) = aggregated_usage {
                        final_usage = Some(usage);
                    }
                    if let Some(meta) = aggregated_meta {
                        final_usage_meta = Some(meta);
                    }
                }

                saw_agent_end = true;
                if assistant_terminal_error.is_none() {
                    emit_stream_event_with_meta(
                        &app,
                        "done",
                        Some(normalized_session_id.clone()),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        final_usage.clone(),
                        final_usage_meta.clone(),
                    )?;
                    done_emitted = true;
                }
                break;
            }
        }

        let saw_terminal_completion =
            saw_agent_end || saw_message_done || saw_assistant_terminal_message;

        let should_pool_after_turn = desktop_reuse_enabled
            && !abort_requested.load(Ordering::SeqCst)
            && !saw_model_abort_event
            && assistant_terminal_error.is_none()
            && (saw_agent_end || saw_message_done)
            && child
                .try_wait()
                .map_err(|error| format!("检查桌面 pi 退出状态失败: {error}"))?
                .is_none();

        if should_pool_after_turn {
            remove_runtime_handle(&normalized_session_id)?;

            if saw_agent_end || saw_message_done {
                emitted_assistant_text = finalize_desktop_outbound_reply(
                    &emitted_assistant_text,
                    agent_config.as_ref().map(|config| config.id.as_str()),
                );
                emit_stream_event(
                    &app,
                    "final_text",
                    Some(normalized_session_id.clone()),
                    Some(emitted_assistant_text.clone()),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )?;
            }

            if let Some(agent_config) = agent_config.as_ref() {
                if !emitted_assistant_text.trim().is_empty() {
                    if let Err(error) = agent_workspace::append_agent_memory_entry(
                        &agent_config.id,
                        "desktop-local",
                        &trimmed_prompt,
                        &emitted_assistant_text,
                    ) {
                        eprintln!("NineClaw: 写入桌面聊天记忆失败: {error}");
                    }
                }
            }

            if !done_emitted {
                emit_stream_event_with_meta(
                    &app,
                    "done",
                    Some(normalized_session_id.clone()),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    final_usage.clone(),
                    final_usage_meta.clone(),
                )?;
            }

            store_pooled_desktop_pi(
                &normalized_session_id,
                DesktopPooledPi {
                    child,
                    stdin,
                    stdout_rx,
                    stderr_buffer,
                    fingerprint: desktop_fingerprint,
                },
            )?;
            dev_trace(
                "desktop.stream",
                format!(
                    "pi 完成(池化回收): session={} chars={}",
                    normalized_session_id,
                    emitted_assistant_text.chars().count()
                ),
            );
            return Ok(());
        }

        let exit_outcome = wait_for_child_exit_with_trace(
            &mut child,
            DESKTOP_PI_AFTER_STDOUT_EOF_EXIT_WAIT,
            Some(&normalized_session_id),
            "stream_pi_prompt:post_stdout_eof_wait_exit",
        )?;
        let status = exit_outcome.status;

        remove_runtime_handle(&normalized_session_id)?;

        let stderr_text = stderr_buffer
            .lock()
            .map(|stderr| stderr.clone())
            .unwrap_or_else(|_| String::new());

        if abort_requested.load(Ordering::SeqCst) {
            emit_stream_event(
                &app,
                "aborted",
                Some(normalized_session_id.clone()),
                None,
                None,
                Some("user".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )?;
            return Ok(());
        }

        if saw_model_abort_event {
            emit_stream_event(
                &app,
                "aborted",
                Some(normalized_session_id.clone()),
                None,
                None,
                Some("model".to_string()),
                None,
                None,
                None,
                None,
                None,
                Some("aborted".to_string()),
                None,
            )?;
            return Ok(());
        }

        if let Some(error_text) = assistant_terminal_error {
            dev_trace(
                "desktop.stream",
                format!("assistant error: session={} error={}", normalized_session_id, error_text),
            );
            emit_stream_event(
                &app,
                "error",
                Some(normalized_session_id.clone()),
                None,
                Some(error_text.clone()),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                final_usage,
            )?;
            return Err(error_text);
        }

        if saw_terminal_completion {
            emitted_assistant_text = finalize_desktop_outbound_reply(
                &emitted_assistant_text,
                agent_config.as_ref().map(|config| config.id.as_str()),
            );
            emit_stream_event(
                &app,
                "final_text",
                Some(normalized_session_id.clone()),
                Some(emitted_assistant_text.clone()),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )?;
        }

        if let Some(agent_config) = agent_config.as_ref() {
            if saw_terminal_completion && !emitted_assistant_text.trim().is_empty() {
                // 桌面聊天也需要沉淀进统一的 agent wiki，避免记忆只在 Bot 通道生效。
                if let Err(error) = agent_workspace::append_agent_memory_entry(
                    &agent_config.id,
                    "desktop-local",
                    &trimmed_prompt,
                    &emitted_assistant_text,
                ) {
                    eprintln!("NineClaw: 写入桌面聊天记忆失败: {error}");
                }
            }
        }

        if fresh_multimodal_session && saw_terminal_completion {
            if let Err(error) =
                record_multimodal_summary(&summary_key, &trimmed_prompt, &emitted_assistant_text)
            {
                eprintln!("NineClaw: 记录多模态摘要失败: {error}");
            }
        }

        if !saw_terminal_completion {
            let mut fallback_error = if !stderr_text.trim().is_empty() {
                stderr_text.trim().to_string()
            } else {
                "pi 未返回 agent_end 事件".to_string()
            };
            if let Some(st) = status.as_ref() {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    if let Some(sig) = st.signal() {
                        fallback_error =
                            format!("{fallback_error}\npi 进程信号终止: {sig}");
                        if sig == 9 && !saw_any_output && !exit_outcome.timed_out {
                            fallback_error.push_str(
                                "\n\n诊断：pi 进程启动后立即被系统终止且没有任何输出。\
                                 NineClaw 已自动校验并修复 runtime 目录权限、扩展属性，\
                                 并在启动前尝试重新修复；如果你刚刚拒绝过 macOS 的安全放行，\
                                 请到“系统设置 > 隐私与安全性”里允许 NineClaw 或 pi 继续运行，\
                                 然后直接重新发送这条消息。",
                            );
                            fallback_error.push_str(&format!("\n当前 pi 路径: {}", resolved_pi_path));
                        }
                    } else if let Some(code) = st.code() {
                        fallback_error =
                            format!("{fallback_error}\npi 进程退出码: {code}");
                    }
                }
                #[cfg(not(unix))]
                if let Some(code) = st.code() {
                    fallback_error = format!("{fallback_error}\npi 进程退出码: {code}");
                }
            }
            if exit_outcome.timed_out {
                fallback_error.push_str(
                    "\n提示：stdout 已结束后等待 pi 退出超时，NineClaw 已结束子进程（信号 9 常由此产生，不一定是 SiliconFlow/模型映射错误）。请核对 OpenAI 兼容 Base URL（多需以 /v1 结尾）、模型 ID、API Key，并在终端用相同参数试跑 `pi --mode rpc`。",
                );
            }
            dev_trace(
                "desktop.stream",
                format!("pi 未正常结束: session={} error={}", normalized_session_id, fallback_error),
            );
            emit_stream_event(
                &app,
                "error",
                Some(normalized_session_id.clone()),
                None,
                Some(fallback_error.clone()),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )?;
            return Err(fallback_error);
        }

        if status.map(|value| !value.success()).unwrap_or(true) {
            if done_emitted && saw_terminal_completion {
                return Ok(());
            }

            if exit_outcome.timed_out
                && saw_terminal_completion
                && stderr_text.trim().is_empty()
            {
                if !done_emitted {
                    emit_stream_event_with_meta(
                        &app,
                        "done",
                        Some(normalized_session_id.clone()),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        final_usage,
                        final_usage_meta.clone(),
                    )?;
                }
                return Ok(());
            }

            let fallback_error = if !stderr_text.trim().is_empty() {
                stderr_text.trim().to_string()
            } else if exit_outcome.timed_out && saw_terminal_completion {
                "pi 在返回完整结果后退出过慢，运行时已强制回收进程。".to_string()
            } else if exit_outcome.timed_out {
                "pi 已被请求终止，但回收超时，运行时已主动脱离该卡死进程。".to_string()
            } else {
                match status {
                    Some(status) => format!("pi 退出码异常: {status}"),
                    None => "pi 已被请求终止，但进程仍未退出".to_string(),
                }
            };
            dev_trace(
                "desktop.stream",
                format!("pi 异常退出: session={} error={}", normalized_session_id, fallback_error),
            );
            emit_stream_event(
                &app,
                "error",
                Some(normalized_session_id.clone()),
                None,
                Some(fallback_error.clone()),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )?;
            return Err(fallback_error);
        }

        if !done_emitted {
            emit_stream_event_with_meta(
                &app,
                "done",
                Some(normalized_session_id.clone()),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                final_usage,
                final_usage_meta,
            )?;
        }
        dev_trace(
            "desktop.stream",
            format!(
                "pi 完成: session={} chars={}",
                normalized_session_id,
                emitted_assistant_text.chars().count()
            ),
        );
        Ok(())
    })
    .await
    .map_err(|error| format!("执行任务失败: {error}"))?
}

// ── Bot Channel Commands ──

static CHANNEL_MANAGER: OnceLock<Mutex<ChannelManager>> = OnceLock::new();

pub(crate) fn channel_manager() -> &'static Mutex<ChannelManager> {
    CHANNEL_MANAGER.get_or_init(|| Mutex::new(ChannelManager::new()))
}

#[tauri::command]
async fn ensure_runtime_dependencies(
    app: tauri::AppHandle,
) -> Result<RuntimeDependencyStatus, String> {
    Ok(pi_runtime::cached_ensure_runtime_dependencies(&app))
}

#[tauri::command]
async fn bot_login_wechat(
    app: AppHandle,
    channel_id: String,
) -> Result<channels::wechat::types::WechatLoginResult, String> {
    let app_clone = app.clone();
    // login_with_qr uses block_on_async (dedicated runtime) internally,
    // so we must run it on a blocking-capable thread.
    let handle = tauri::async_runtime::spawn_blocking(move || {
        let channel = WeChatChannel::new(&channel_id, "", "", None);
        channel.login_with_qr(&app_clone)
    });
    handle.await.map_err(|e| format!("登录任务执行失败: {e}"))?
}

#[tauri::command]
async fn bot_start_wechat(
    app: AppHandle,
    channel_id: String,
    agent_id: String,
    token: String,
    base_url: Option<String>,
    route_tag: Option<String>,
    provider_id: Option<String>,
    provider_api_format: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    provider_base_url: Option<String>,
) -> Result<(), String> {
    start_wechat_channel(
        app,
        channel_id,
        agent_id,
        token,
        base_url,
        route_tag,
        provider_id,
        provider_api_format,
        model,
        api_key,
        provider_base_url,
    )
}

#[tauri::command]
async fn bot_start_lark(
    app: AppHandle,
    channel_id: String,
    agent_id: String,
    app_id: String,
    app_secret: String,
    provider_id: Option<String>,
    provider_api_format: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    provider_base_url: Option<String>,
) -> Result<(), String> {
    start_lark_channel(
        app,
        channel_id,
        agent_id,
        app_id,
        app_secret,
        provider_id,
        provider_api_format,
        model,
        api_key,
        provider_base_url,
    )
}

fn start_wechat_channel(
    app: AppHandle,
    channel_id: String,
    agent_id: String,
    token: String,
    base_url: Option<String>,
    route_tag: Option<String>,
    _provider_id: Option<String>,
    _provider_api_format: Option<String>,
    _model: Option<String>,
    _api_key: Option<String>,
    _provider_base_url: Option<String>,
) -> Result<(), String> {
    let agent_config = agents::get_conversation_agent_config(&app, &agent_id)?
        .ok_or_else(|| "绑定的智能体不存在，无法启动 IM 机器人".to_string())?;

    let runtime = resolve_im_llm_runtime(
        &app,
        &agent_config.default_provider_id,
        &agent_config.default_model,
    )?;
    let normalized_base = normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );

    let mut mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;

    // Register WeChat channel via factory
    let old = mgr.register_channel(ChannelConfig::WeChat {
        channel_id: channel_id.clone(),
        agent_config: Some(agent_config),
        token,
        base_url: base_url.unwrap_or_default(),
        route_tag,
        ai_provider_id: runtime.provider_id.clone(),
        ai_api_format: runtime.api_format.clone(),
        ai_base_url: normalized_base,
        ai_api_key: runtime.api_key.clone(),
        ai_model: runtime.model.clone(),
    })?;
    drop(mgr);
    if let Some(mut channel) = old {
        let _ = channel.stop();
    }
    let mut mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;
    mgr.start_channel(&channel_id, app)?;

    Ok(())
}

fn start_lark_channel(
    app: AppHandle,
    channel_id: String,
    agent_id: String,
    app_id: String,
    app_secret: String,
    _provider_id: Option<String>,
    _provider_api_format: Option<String>,
    _model: Option<String>,
    _api_key: Option<String>,
    _provider_base_url: Option<String>,
) -> Result<(), String> {
    let agent_config = agents::get_conversation_agent_config(&app, &agent_id)?
        .ok_or_else(|| "绑定的智能体不存在，无法启动 IM 机器人".to_string())?;

    let runtime = resolve_im_llm_runtime(
        &app,
        &agent_config.default_provider_id,
        &agent_config.default_model,
    )?;
    let normalized_base = normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );

    let mut mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;

    let old = mgr.register_channel(ChannelConfig::Lark {
        channel_id: channel_id.clone(),
        agent_config: Some(agent_config),
        app_id,
        app_secret,
        ai_provider_id: runtime.provider_id.clone(),
        ai_api_format: runtime.api_format.clone(),
        ai_base_url: normalized_base,
        ai_api_key: runtime.api_key.clone(),
        ai_model: runtime.model.clone(),
    })?;
    drop(mgr);
    if let Some(mut channel) = old {
        let _ = channel.stop();
    }
    let mut mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;
    mgr.start_channel(&channel_id, app)?;

    Ok(())
}

#[tauri::command]
async fn bot_stop_wechat(channel_id: String) -> Result<(), String> {
    let mut mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;
    mgr.stop_channel(&channel_id)
}

#[tauri::command]
fn rotate_agent_peer_inbound_secret(
    app: AppHandle,
    agent_id: String,
) -> Result<agents::AgentRecord, String> {
    agents::rotate_agent_peer_inbound_secret(&app, &agent_id)
}

#[tauri::command]
fn get_peer_gateway_info(app: AppHandle) -> peer_gateway::PeerGatewayInfo {
    peer_gateway::get_peer_gateway_info(&app)
}

#[tauri::command]
fn load_peer_gateway_settings(app: AppHandle) -> Result<peer_gateway::PeerGatewaySettings, String> {
    peer_gateway::load_peer_gateway_settings(&app)
}

#[tauri::command]
fn save_peer_gateway_settings(
    app: AppHandle,
    settings: peer_gateway::PeerGatewaySettings,
) -> Result<peer_gateway::PeerGatewayInfo, String> {
    if peer_gateway::peer_bind_from_env().is_some() {
        return Err(
            "已设置环境变量 NINECLAW_PEER_BIND，监听地址由环境变量决定；请取消该变量后再使用应用内设置。"
                .into(),
        );
    }
    peer_gateway::save_peer_gateway_settings(&app, &settings)?;
    peer_gateway::restart_peer_gateway(&app)?;
    Ok(peer_gateway::get_peer_gateway_info(&app))
}

#[tauri::command]
async fn bot_stop_lark(channel_id: String) -> Result<(), String> {
    let mut mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;
    mgr.stop_channel(&channel_id)
}

pub(crate) fn auto_start_bound_im_services(app: &AppHandle) -> Result<(), String> {
    let agent_records = agents::list_agents(app)?;

    for agent in agent_records {
        for (channel_key, config) in &agent.bot_configs {
            if config.im_channel_paused {
                continue;
            }

            let has_credentials = match channel_key.as_str() {
                "wechat" => config
                    .token
                    .as_ref()
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false),
                "lark" => {
                    !config.client_id.trim().is_empty() && !config.client_secret.trim().is_empty()
                }
                _ => false,
            };

            if !has_credentials {
                continue;
            }

            if let Err(error) =
                resolve_im_llm_runtime(app, &agent.default_provider_id, &agent.default_model)
            {
                log::warn!(
                    "跳过自动启动智能体 {} 的 {} 机器人: {}",
                    agent.id,
                    channel_key,
                    error
                );
                continue;
            }

            match channel_key.as_str() {
                "wechat" => {
                    let Some(token) = config
                        .token
                        .as_ref()
                        .filter(|value| !value.trim().is_empty())
                    else {
                        continue;
                    };

                    let base_url = config.base_url.clone().or_else(|| {
                        Some(config.client_secret.clone()).filter(|value| !value.trim().is_empty())
                    });

                    if let Err(error) = start_wechat_channel(
                        app.clone(),
                        format!("wechat:{}", agent.id),
                        agent.id.clone(),
                        token.clone(),
                        base_url,
                        config.route_tag.clone(),
                        None,
                        None,
                        None,
                        None,
                        None,
                    ) {
                        log::warn!("自动启动智能体 {} 的微信机器人失败: {}", agent.id, error);
                    } else {
                        log::info!("已自动启动智能体 {} 的微信机器人", agent.id);
                    }
                }
                "lark" => {
                    let app_id = config.client_id.trim();
                    let app_secret = config.client_secret.trim();
                    if app_id.is_empty() || app_secret.is_empty() {
                        continue;
                    }

                    if let Err(error) = start_lark_channel(
                        app.clone(),
                        format!("lark:{}", agent.id),
                        agent.id.clone(),
                        app_id.to_string(),
                        app_secret.to_string(),
                        None,
                        None,
                        None,
                        None,
                        None,
                    ) {
                        log::warn!("自动启动智能体 {} 的飞书机器人失败: {}", agent.id, error);
                    } else {
                        log::info!("已自动启动智能体 {} 的飞书机器人", agent.id);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

#[tauri::command]
async fn bot_get_status(channel_id: String) -> Result<String, String> {
    let mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;
    let status = mgr.get_status(&channel_id)?;
    serde_json::to_string(&status).map_err(|e| format!("序列化状态失败: {e}"))
}

#[tauri::command]
async fn bot_send_message(
    channel_id: String,
    user_id: String,
    content: String,
) -> Result<(), String> {
    let mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;
    mgr.send_message(&channel_id, &user_id, &content)
}

#[tauri::command]
async fn test_llm_provider_connection(
    api_format: String,
    base_url: String,
    api_key: String,
    model: String,
) -> Result<String, String> {
    let api_format = normalize_provider_api_format(&api_format, "");
    let base = match api_format {
        "anthropic" => normalize_anthropic_base_url(&base_url),
        _ => normalize_provider_base_url(&base_url).to_string(),
    };
    let model = model.trim();
    if base.is_empty() {
        return Err("Base URL 不能为空".to_string());
    }
    if model.is_empty() {
        return Err("模型名称不能为空".to_string());
    }

    let client = build_http_client();

    let (url, request) = match api_format {
        "anthropic" => {
            let body = json!({
                "model": model,
                "messages": [{ "role": "user", "content": "ping" }],
                "max_tokens": 8,
            });
            let endpoint = anthropic_messages_url(&base);
            let mut request = client
                .post(endpoint.clone())
                .header("Content-Type", "application/json")
                .header("anthropic-version", "2023-06-01")
                .json(&body);
            if !api_key.trim().is_empty() {
                // Some Anthropic-compatible gateways require Bearer auth even when
                // they expose the Messages API surface.
                request = request
                    .header("x-api-key", api_key.trim())
                    .header("Authorization", format!("Bearer {}", api_key.trim()));
            }
            (endpoint, request)
        }
        _ => {
            let body = json!({
                "model": model,
                "messages": [{ "role": "user", "content": "ping" }],
                "max_tokens": 8,
            });
            let mut request = client
                .post(format!("{}/chat/completions", base))
                .header("Content-Type", "application/json")
                .json(&body);
            if !api_key.trim().is_empty() {
                request = request.header("Authorization", format!("Bearer {}", api_key.trim()));
            }
            (format!("{}/chat/completions", base), request)
        }
    };

    let response = request
        .send()
        .await
        .map_err(|error| format!("网络请求失败({url}): {error}"))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取响应失败: {error}"))?;

    if !status.is_success() {
        let preview: String = text.chars().take(280).collect();
        return Err(format!("HTTP {} — {}", status.as_u16(), preview));
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("响应不是合法 JSON: {error}"))?;

    if let Some(err) = parsed.get("error") {
        return Err(format!("API 返回错误: {err}"));
    }

    let success = match api_format {
        "anthropic" => parsed
            .get("content")
            .and_then(|content| content.as_array())
            .map(|content| !content.is_empty())
            .unwrap_or(false),
        _ => parsed
            .get("choices")
            .and_then(|choices| choices.as_array())
            .map(|choices| !choices.is_empty())
            .unwrap_or(false),
    };

    if !success {
        let preview: String = text.chars().take(200).collect();
        return Err(match api_format {
            "anthropic" => format!("响应中无 content: {preview}"),
            _ => format!("响应中无 choices: {preview}"),
        });
    }

    Ok("连通成功：已收到模型回复".to_string())
}

#[tauri::command]
async fn bot_send_media(
    channel_id: String,
    user_id: String,
    media_type: String,
    file_path: String,
) -> Result<(), String> {
    let data = fs::read(&file_path).map_err(|e| format!("读取文件失败: {e}"))?;

    let file_name = std::path::Path::new(&file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let mt = match media_type.as_str() {
        "image" => MediaType::Image,
        "video" => MediaType::Video,
        "audio" | "voice" => MediaType::Audio,
        "file" | _ => MediaType::File,
    };

    let payload = MediaPayload {
        media_type: mt,
        file_name,
        data,
    };

    let mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;
    mgr.send_media(&channel_id, &user_id, &payload)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            dev_trace("app", "NineClaw 启动");
            resize_main_window_to_screen(&app.handle());

            // Defer heavy runtime initialization to a background thread so the
            // window renders immediately.  The frontend listens for the
            // "pi://runtime-ready" event to know when PI features are available.
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                dev_trace("app", "后台 runtime 初始化开始");
                let status = pi_runtime::cached_ensure_runtime_dependencies(&app_handle);
                if !status.pi_available {
                    log::warn!("runtime dependency check: {}", status.messages.join(" | "));
                }
                dev_trace("app", "后台 runtime 初始化完成");

                // Notify frontend that the PI runtime is ready
                let _ = app_handle.emit("pi://runtime-ready", status.pi_available);

                // Start IM services after runtime is ready
                if let Err(error) = auto_start_bound_im_services(&app_handle) {
                    log::warn!("应用启动时自动检测 IM 机器人绑定失败: {}", error);
                }
                match agents::backfill_peer_inbound_secrets(&app_handle) {
                    Ok(count) if count > 0 => {
                        log::info!("已为 {count} 个智能体补全对等入站独立密钥");
                    }
                    Ok(_) => {}
                    Err(error) => {
                        log::warn!("对等入站密钥补全未执行: {error}");
                    }
                }
                if let Err(error) = peer_gateway::restart_peer_gateway(&app_handle) {
                    log::warn!("对等网关启动: {error}");
                }
                scheduler::start_embedded_scheduler(app_handle);
            });

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_history_state,
            save_history_state,
            clear_history_state,
            list_token_usage_records,
            load_provider_preferences,
            save_provider_preferences,
            list_installed_skills,
            list_system_skill_catalog,
            install_system_skill,
            list_agents,
            get_default_agent,
            create_agent,
            update_agent,
            rotate_agent_peer_inbound_secret,
            get_peer_gateway_info,
            load_peer_gateway_settings,
            save_peer_gateway_settings,
            archive_agent,
            delete_agent,
            set_default_agent,
            read_agent_workspace_bundle,
            write_agent_workspace_file,
            list_scheduled_jobs,
            list_scheduled_job_runs,
            sync_scheduler_jobs,
            trigger_scheduler_job_now,
            get_scheduler_status,
            install_scheduler_service,
            uninstall_scheduler_service,
            handle_agent_task_prompt,
            list_agent_task_deliveries,
            list_agent_tasks,
            pause_agent_task,
            resume_agent_task,
            delete_agent_task,
            update_agent_task,
            run_agent_task_now,
            stream_pi_prompt,
            abort_pi_stream,
            persist_chat_attachments,
            open_local_file,
            open_external_url,
            load_local_media_preview,
            clear_pi_session,
            clear_pi_session_for_id,
            bot_login_wechat,
            bot_start_wechat,
            bot_start_lark,
            bot_stop_wechat,
            bot_stop_lark,
            bot_get_status,
            bot_send_message,
            bot_send_media,
            ensure_runtime_dependencies,
            test_llm_provider_connection
        ])
        .run(app_context())
        .expect("error while running tauri application");
}

pub fn run_scheduler_daemon() -> Result<(), String> {
    let app = tauri::Builder::default()
        .build(app_context())
        .map_err(|error| format!("初始化 scheduler daemon 失败: {error}"))?;
    let app_handle = app.handle().clone();

    let status = pi_runtime::ensure_runtime_dependencies_impl(&app_handle);
    if !status.pi_available {
        log::warn!(
            "scheduler daemon runtime dependency check: {}",
            status.messages.join(" | ")
        );
    }
    if let Err(error) = auto_start_bound_im_services(&app_handle) {
        log::warn!("scheduler daemon 自动启动 IM 服务失败: {}", error);
    }

    scheduler::run_daemon(app_handle)
}

fn app_context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}
