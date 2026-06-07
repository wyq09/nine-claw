mod agent_capabilities;
mod agent_turn_context;
mod agent_loop;
mod agent_loop_types;
mod agent_task_schedule;
mod agent_tasks;
mod agent_workspace;
mod agents;
mod app_log;
mod channels;
mod chat_attachments;
mod dev_trace;
mod embedding;
mod embedding_settings;
mod heartbeat;
mod image_generation;
mod llm_log_export;
mod llm_trace;
mod macos_native_dictation_panel;
mod mcp_settings;
mod managed_runtime;
mod managed_runtime_extension;
mod media_directives;
mod memory_gate;
pub mod memory_vector;
mod peer_gateway;
mod pi_runtime;
mod pi_timeouts;
mod prompt_attachments;
mod proxy_settings;
mod runtime_parameters;
mod scheduler;
mod session_workspace;
mod skills;
pub mod storage;
mod team_supervisor;
mod team_workspace;
mod user_memory_auto_extraction;
mod user_memory_service;
mod widget_runtime;
mod workspace_fs;
mod workspace_memory_extraction;

mod agent_package;
mod agent_presets;
mod app_constants;
mod commands_agents_skills;
mod commands_chat_workspace;
mod commands_llm_log_export;
mod commands_llm_trace;
mod commands_memory;
mod commands_session_llm_log;
mod commands_session_workspace;
mod commands_workspace_kv_memory;
mod history_app_state;
mod pi_usage;
mod prompts;
mod provider_runtime;
mod provider_stream_noise;
mod runtime_agent_config;
mod session_compression;
mod session_llm_log;
mod session_llm_titles;
mod skill_broker;
mod time_util;
mod user_kv_memory_reorganize;

pub(crate) use app_constants::*;
pub(crate) use embedding_settings::{
    configure_embedding_runtime, embedding_status_command, load_embedding_settings_command,
    maybe_start_local_model_download, save_embedding_settings_command,
    trigger_embedding_reindex_command,
};
pub(crate) use history_app_state::{
    ensure_app_state_schema, open_history_db, record_token_usage_for_scheduler_pi_completion,
    storage_conn,
};
pub(crate) use image_generation::{
    load_image_generation_preferences, resolve_default_image_generation_runtime,
    save_image_generation_preferences,
};
pub(crate) use pi_usage::{
    accumulate_pi_token_usage, aggregate_usage_from_agent_messages, extract_usage_metadata_payload,
    extract_usage_payload, json_string, usage_row_total_tokens, PiTokenUsagePayload,
    PiUsageMetadataPayload,
};
#[cfg(test)]
pub(crate) use provider_runtime::should_force_pi_thinking_off;
pub(crate) use provider_runtime::{
    anthropic_messages_url, forced_pi_thinking_level, load_provider_preferences,
    normalize_anthropic_base_url, normalize_provider_api_format, normalize_provider_base_url,
    normalized_provider_runtime_base_url, openai_pi_compat_requires_explicit_thinking_disable,
    openai_pi_compat_requires_reasoning_content_replay, openai_pi_compat_supports_reasoning_effort,
    pi_runtime_dir, resolve_im_llm_runtime, save_provider_preferences, ProviderRuntimeConfig,
};
pub(crate) use proxy_settings::build_http_client;
pub(crate) use session_llm_titles::refine_agent_task_metadata;
pub(crate) use time_util::chrono_like_timestamp;

use app_log::{app_log_export_all, app_log_list, app_log_open_dir, app_log_read};
use commands_agents_skills::*;
use commands_chat_workspace::*;
use commands_llm_log_export::*;
use commands_llm_trace::*;
use commands_memory::*;
use commands_session_llm_log::*;
use commands_session_workspace::*;
use commands_workspace_kv_memory::*;
use history_app_state::{
    clear_history_state, list_token_usage_records, load_history_state, save_history_state,
};
use session_llm_titles::generate_session_conversation_title;
use widget_runtime::{widget_cancel_response, widget_submit_response};

use base64::{engine::general_purpose::STANDARD as BASE64_ENGINE, Engine as _};
use md5::{Digest, Md5};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, PhysicalSize, Size};

use agents::ConversationAgentConfig;
use channels::factory::ChannelConfig;
use channels::manager::ChannelManager;
use channels::types::{MediaPayload, MediaType};
use channels::wechat::WeChatChannel;
use chat_attachments::{ChatAttachmentUpload, PersistedChatAttachment};
use chrono::Utc;
use dev_trace::{dev_trace, dev_trace_block};
use pi_runtime::RuntimeDependencyStatus;
#[derive(Clone)]
struct PiRuntimeHandle {
    abort_requested: Arc<AtomicBool>,
    pid: u32,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

static PI_RUNTIME_HANDLES: OnceLock<Mutex<HashMap<String, PiRuntimeHandle>>> = OnceLock::new();
static DESKTOP_POOLED_PI: OnceLock<Mutex<HashMap<String, DesktopPooledPi>>> = OnceLock::new();
static DESKTOP_IDLE_COMPRESSION_EPOCHS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
/// 同一桌面 session 串行化 `stream_pi_prompt`，避免并发时池替换/双进程互相 kill 导致 SIGKILL、stdout 空读。
static DESKTOP_STREAM_SESSION_MUTEXES: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    OnceLock::new();

#[cfg(test)]
pub(crate) fn workspace_env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn desktop_session_stream_mutex(session_id: &str) -> Arc<Mutex<()>> {
    let map = DESKTOP_STREAM_SESSION_MUTEXES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().expect("DESKTOP_STREAM_SESSION_MUTEXES poisoned");
    guard
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}
struct DesktopPooledPi {
    child: Child,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stdout_rx: mpsc::Receiver<Result<String, String>>,
    stderr_buffer: Arc<Mutex<String>>,
    fingerprint: String,
    workspace_id: Option<String>,
    session_path: PathBuf,
    last_usage: Option<PiTokenUsagePayload>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopCompressionCommandResult {
    compressed: bool,
    reason: String,
}

fn pi_reuse_desktop_enabled() -> bool {
    std::env::var("NINECLAW_PI_REUSE_DESKTOP")
        .map(|value| value.trim() != "0")
        .unwrap_or(true)
}

pub(crate) fn open_path_in_default_app(path: &Path) -> Result<(), String> {
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

pub(crate) fn reveal_path_in_file_manager(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = Command::new("open");
        cmd.args(["-R"]);
        cmd.arg(path);
        cmd
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = Command::new("explorer");
        cmd.arg(format!("/select,{}", path.display()));
        cmd
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(path.parent().unwrap_or(path));
        cmd
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("在文件管理器中显示失败: {error}"))
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
    strategy: Option<String>,
    mounted_skill_ids: Option<Vec<String>>,
    reasons: Option<Vec<String>>,
    #[serde(flatten)]
    usage: Option<PiTokenUsagePayload>,
    #[serde(flatten)]
    usage_meta: Option<PiUsageMetadataPayload>,
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
const DESKTOP_ABORT_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

/// stdout 已关闭后等待 pi 进程自行退出的最长时间，超时后会 SIGKILL。
/// 默认 600 秒；可通过环境变量 `NINECLAW_DESKTOP_PI_AFTER_STDOUT_EOF_WAIT_SECS` 覆盖（整数秒，范围 5～600）。
fn desktop_pi_after_stdout_eof_exit_wait() -> Duration {
    const DEFAULT_SECS: u64 = 600;
    const MIN_SECS: u64 = 5;
    const MAX_SECS: u64 = 600;
    let secs = std::env::var("NINECLAW_DESKTOP_PI_AFTER_STDOUT_EOF_WAIT_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_SECS)
        .clamp(MIN_SECS, MAX_SECS);
    Duration::from_secs(secs)
}

fn cleanup_aborted_desktop_child(
    child: &mut Child,
    stdin: &Arc<Mutex<Option<ChildStdin>>>,
    session_id: &str,
    reason: &str,
) {
    close_pi_stdin(stdin);
    kill_child_with_trace(child, Some(session_id), reason);
    let _ =
        wait_for_child_exit_with_trace(child, DESKTOP_ABORT_WAIT_TIMEOUT, Some(session_id), reason);
}

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

fn desktop_media_reply_prompt(
    agent_home: Option<&Path>,
    workspace_output_root: Option<&Path>,
) -> String {
    prompts::desktop_media_reply_prompt(agent_home, workspace_output_root)
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

// ── Context Window Guard ──

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionContextStats {
    session_id: String,
    used_tokens: u64,
    context_window: Option<u64>,
    input_tokens: u64,
    output_tokens: u64,
    model: Option<String>,
    source: String,
}

#[cfg_attr(not(test), allow(dead_code))]
fn parse_context_stats_from_rpc_response(value: &serde_json::Value) -> Option<SessionContextStats> {
    let cu = value.get("contextUsage")?;
    let used_tokens = cu.get("usedTokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let raw_cw = cu
        .get("contextWindow")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let context_window = if raw_cw > 0 { Some(raw_cw) } else { None };
    let input_tokens = cu.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let output_tokens = cu.get("outputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
    Some(SessionContextStats {
        session_id: String::new(),
        used_tokens,
        context_window,
        input_tokens,
        output_tokens,
        model: None,
        source: "auto".to_string(),
    })
}

#[cfg_attr(not(test), allow(dead_code))]
fn resolve_context_window_from_sources(
    auto_detected: Option<u64>,
    manual_config: Option<u64>,
) -> (Option<u64>, &'static str) {
    if let Some(auto) = auto_detected {
        if auto > 0 {
            return (Some(auto), "auto");
        }
    }
    if let Some(manual) = manual_config {
        if manual > 0 {
            return (Some(manual), "manual");
        }
    }
    (None, "unknown")
}

/// 与前端 `aggregateSessionUsageFromTurns` 一致：按 turn 累计 `usage`。
fn aggregate_token_usage_from_history_turns(
    turns: Option<&Vec<serde_json::Value>>,
) -> Option<PiTokenUsagePayload> {
    let turns = turns?;
    let mut total: Option<PiTokenUsagePayload> = None;
    for turn in turns {
        let usage = turn.as_object().and_then(|obj| obj.get("usage"));
        let step = extract_usage_payload(usage);
        accumulate_pi_token_usage(&mut total, step);
    }
    total
}

fn session_context_stats_from_history(
    app: &AppHandle,
    session_id: &str,
) -> Result<SessionContextStats, String> {
    let connection = open_history_db(app)?;
    let payload: Option<String> = connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![HISTORY_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取历史快照失败: {error}"))?;

    let Some(payload) = payload else {
        return Ok(SessionContextStats {
            session_id: session_id.to_string(),
            used_tokens: 0,
            context_window: None,
            input_tokens: 0,
            output_tokens: 0,
            model: None,
            source: "unknown".to_string(),
        });
    };

    let parsed: serde_json::Value =
        serde_json::from_str(&payload).map_err(|error| format!("解析历史快照失败: {error}"))?;
    let Some(items) = parsed.as_array() else {
        return Ok(SessionContextStats {
            session_id: session_id.to_string(),
            used_tokens: 0,
            context_window: None,
            input_tokens: 0,
            output_tokens: 0,
            model: None,
            source: "unknown".to_string(),
        });
    };

    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let Some(id) = json_string(obj.get("id")) else {
            continue;
        };
        if id != session_id {
            continue;
        }

        let merged =
            aggregate_token_usage_from_history_turns(obj.get("turns").and_then(|v| v.as_array()));

        let model = json_string(obj.get("sessionLlmModel")).or_else(|| {
            obj.get("agent")
                .and_then(|value| value.as_object())
                .and_then(|agent| json_string(agent.get("defaultModel")))
        });

        let (used_tokens, input_tokens, output_tokens) = merged
            .map(|payload| {
                let used = usage_row_total_tokens(&payload);
                let input = payload.input_tokens.unwrap_or(0);
                let output = payload.output_tokens.unwrap_or(0);
                (used, input, output)
            })
            .unwrap_or((0, 0, 0));

        return Ok(SessionContextStats {
            session_id: session_id.to_string(),
            used_tokens,
            context_window: None,
            input_tokens,
            output_tokens,
            model,
            source: "history".to_string(),
        });
    }

    Ok(SessionContextStats {
        session_id: session_id.to_string(),
        used_tokens: 0,
        context_window: None,
        input_tokens: 0,
        output_tokens: 0,
        model: None,
        source: "unknown".to_string(),
    })
}

#[tauri::command]
async fn get_session_context_stats(
    app: AppHandle,
    session_id: String,
) -> Result<SessionContextStats, String> {
    let trimmed = session_id.trim().to_string();
    if trimmed.is_empty() {
        return Ok(SessionContextStats {
            session_id: String::new(),
            used_tokens: 0,
            context_window: None,
            input_tokens: 0,
            output_tokens: 0,
            model: None,
            source: "unknown".to_string(),
        });
    }

    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || session_context_stats_from_history(&app, &trimmed))
        .await
        .map_err(|error| error.to_string())?
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
        aggregate_token_usage_from_history_turns, aggregate_usage_from_agent_messages,
        build_desktop_anthropic_compat_extension_source, build_desktop_outbound_display_text,
        build_provider_models_config_with_input, build_turn_prompt_with_multimodal_summary,
        channel_manager, desktop_incomplete_reply_error, desktop_media_reply_prompt,
        desktop_pi_fingerprint, forced_pi_thinking_level, infer_media_mime_type,
        is_provider_image_block_rejection_error, is_provider_reasoning_history_rejection_error,
        lock_channel_manager, parse_context_stats_from_rpc_response,
        prepend_multimodal_summary_context, quarantine_pi_session_file, record_multimodal_summary,
        render_multimodal_summary_context, resolve_context_window_from_sources,
        resolve_pi_ai_import_path, sanitize_pi_session_replay_state, should_force_pi_thinking_off,
        should_retry_text_only_after_image_rejection, summary_file_path, usage_row_total_tokens,
        DesktopParsedMediaItem, ProviderRuntimeConfig,
    };
    use crate::channels::types::MediaType;
    use crate::prompt_attachments;
    use serde_json::json;
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
    fn lock_channel_manager_recovers_from_poisoned_mutex() {
        let _test_guard = crate::workspace_env_test_lock();
        let manager = channel_manager();
        manager.clear_poison();

        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = manager
                .lock()
                .expect("channel manager lock should start clean");
            panic!("poison channel manager for test");
        }));

        assert!(poisoned.is_err());
        assert!(manager.lock().is_err());
        drop(lock_channel_manager());
        assert!(manager.lock().is_ok());
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
    fn desktop_pi_fingerprint_changes_when_proxy_binding_changes() {
        let provider = ProviderRuntimeConfig {
            provider_id: "custom".to_string(),
            api_format: "openai".to_string(),
            base_url: "https://example.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "gpt-5".to_string(),
        };

        let base = desktop_pi_fingerprint(
            Some(&provider),
            "/tmp/session.jsonl",
            "http://127.0.0.1:3000",
            "token-a",
            &[("agent_system_prompt".to_string(), "hello".to_string())],
            &[],
        );
        let changed_proxy_url = desktop_pi_fingerprint(
            Some(&provider),
            "/tmp/session.jsonl",
            "http://127.0.0.1:4000",
            "token-a",
            &[("agent_system_prompt".to_string(), "hello".to_string())],
            &[],
        );
        let changed_proxy_token = desktop_pi_fingerprint(
            Some(&provider),
            "/tmp/session.jsonl",
            "http://127.0.0.1:3000",
            "token-b",
            &[("agent_system_prompt".to_string(), "hello".to_string())],
            &[],
        );

        assert_ne!(base, changed_proxy_url);
        assert_ne!(base, changed_proxy_token);
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
    fn visual_turn_skips_previous_multimodal_summary_context() {
        let key = format!("test-visual-summary-{}", Uuid::new_v4());
        let path = summary_file_path(&key);
        let _ = fs::remove_file(&path);

        record_multimodal_summary(&key, "第一张图是什么", "这是上一张图的摘要")
            .expect("record summary");

        let prompt = build_turn_prompt_with_multimodal_summary(
            "识别这张新图片",
            &key,
            &[prompt_attachments::PromptAttachmentInput {
                file_name: "fresh.png".to_string(),
                file_path: "/tmp/fresh.png".to_string(),
                mime_type: "image/png".to_string(),
                kind: "image".to_string(),
                transcript: None,
            }],
        )
        .expect("build prompt");

        assert_eq!(prompt, "识别这张新图片");
        assert!(!prompt.contains("上一张图的摘要"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn text_only_turn_keeps_previous_multimodal_summary_context() {
        let key = format!("test-text-summary-{}", Uuid::new_v4());
        let path = summary_file_path(&key);
        let _ = fs::remove_file(&path);

        record_multimodal_summary(&key, "第一张图是什么", "这是上一张图的摘要")
            .expect("record summary");

        let prompt = build_turn_prompt_with_multimodal_summary("继续分析刚才那张图", &key, &[])
            .expect("build prompt");

        assert!(prompt.contains("原始图片/视频已从主会话上下文移除"));
        assert!(prompt.contains("这是上一张图的摘要"));
        assert!(prompt.contains("继续分析刚才那张图"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn desktop_media_reply_prompt_mentions_outbox_when_agent_home_exists() {
        let prompt = desktop_media_reply_prompt(Some(Path::new("/tmp/agent-home")), None);
        assert!(prompt.contains("::nc-media"));
        assert!(prompt.contains("/tmp/agent-home/outbox"));
    }

    #[test]
    fn desktop_media_reply_prompt_prefers_session_workspace_root() {
        let prompt = desktop_media_reply_prompt(
            Some(Path::new("/tmp/agent-home")),
            Some(Path::new("/session/workspace")),
        );
        assert!(prompt.contains("::nc-media"));
        assert!(prompt.contains("/session/workspace"));
        assert!(prompt.contains("当前会话工作区"));
        assert!(!prompt.contains("/tmp/agent-home/outbox"));
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
        let reason = desktop_incomplete_reply_error(Some(&provider), true, true, true)
            .expect("partial reply reason");
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
    fn detects_provider_image_url_text_only_rejection() {
        let error = "400 Failed to deserialize the JSON body into the target type: messages[39]: unknown variant `image_url`, expected `text` at line 1 column 1325060";
        assert!(is_provider_image_block_rejection_error(error));
        assert!(!is_provider_image_block_rejection_error(
            "429 rate limit exceeded"
        ));
    }

    #[test]
    fn current_visual_turn_does_not_retry_with_text_only_history() {
        assert!(!should_retry_text_only_after_image_rejection(
            0,
            true,
            "messages[39]: unknown variant `image_url`, expected `text`"
        ));
        assert!(should_retry_text_only_after_image_rejection(
            0,
            false,
            "messages[39]: unknown variant `image_url`, expected `text`"
        ));
        assert!(!should_retry_text_only_after_image_rejection(
            1,
            false,
            "messages[39]: unknown variant `image_url`, expected `text`"
        ));
    }

    #[test]
    fn provider_models_config_can_fallback_to_text_only_input() {
        let provider = ProviderRuntimeConfig {
            provider_id: "custom".to_string(),
            api_format: "openai".to_string(),
            base_url: "https://example.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "custom-model".to_string(),
        };

        let default_config = build_provider_models_config_with_input(&provider, false, false)
            .expect("default config");
        let text_only_config = build_provider_models_config_with_input(&provider, true, false)
            .expect("text-only config");

        let default_input =
            &default_config["providers"]["nineclaw-runtime-8b9035807842"]["models"][0]["input"];
        let text_only_input =
            &text_only_config["providers"]["nineclaw-runtime-8b9035807842"]["models"][0]["input"];
        assert_eq!(default_input, &json!(["text", "image"]));
        assert_eq!(text_only_input, &json!(["text"]));
    }

    #[test]
    fn provider_models_config_can_disable_reasoning_effort_for_history_fallback() {
        let provider = ProviderRuntimeConfig {
            provider_id: "custom".to_string(),
            api_format: "openai".to_string(),
            base_url: "https://example.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "glm-4.6".to_string(),
        };

        let default_config = build_provider_models_config_with_input(&provider, false, false)
            .expect("default config");
        let fallback_config = build_provider_models_config_with_input(&provider, false, true)
            .expect("fallback config");

        assert_eq!(
            default_config["providers"]["nineclaw-runtime-8b9035807842"]["compat"]
                ["supportsReasoningEffort"],
            json!(true)
        );
        assert_eq!(
            fallback_config["providers"]["nineclaw-runtime-8b9035807842"]["compat"]
                ["supportsReasoningEffort"],
            json!(false)
        );
    }

    #[test]
    fn provider_models_config_for_deepseek_v4_pro_sends_qwen_thinking_off_shape() {
        let provider = ProviderRuntimeConfig {
            provider_id: "custom".to_string(),
            api_format: "openai".to_string(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            api_key: "secret".to_string(),
            model: "deepseek-v4-pro".to_string(),
        };

        let config = build_provider_models_config_with_input(&provider, false, false)
            .expect("deepseek config");
        let provider_json = &config["providers"]["nineclaw-runtime-8b9035807842"];
        assert_eq!(
            provider_json["compat"]["supportsReasoningEffort"],
            json!(false)
        );
        assert_eq!(provider_json["compat"]["thinkingFormat"], json!("qwen"));
        assert_eq!(provider_json["models"][0]["reasoning"], json!(true));
        assert!(should_force_pi_thinking_off(&provider, false));
        assert_eq!(forced_pi_thinking_level(&provider, false), Some("off"));
    }

    #[test]
    fn provider_models_config_for_mimo_replays_reasoning_content() {
        let provider = ProviderRuntimeConfig {
            provider_id: "custom".to_string(),
            api_format: "openai".to_string(),
            base_url: "https://api.xiaomimimo.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "MiMo-V2.5-Pro".to_string(),
        };

        let config =
            build_provider_models_config_with_input(&provider, false, false).expect("mimo config");
        let provider_json = &config["providers"]["nineclaw-runtime-8b9035807842"];
        assert_eq!(
            provider_json["compat"]["requiresReasoningContentOnAssistantMessages"],
            json!(true)
        );
        assert_eq!(provider_json["compat"]["thinkingFormat"], json!("deepseek"));
        assert_eq!(provider_json["models"][0]["reasoning"], json!(true));
        assert!(!should_force_pi_thinking_off(&provider, false));
        assert!(!should_force_pi_thinking_off(&provider, true));
        assert_eq!(forced_pi_thinking_level(&provider, false), Some("medium"));
        assert_eq!(forced_pi_thinking_level(&provider, true), Some("medium"));
    }

    #[test]
    fn provider_models_config_does_not_apply_mimo_replay_to_other_models() {
        let provider = ProviderRuntimeConfig {
            provider_id: "custom".to_string(),
            api_format: "openai".to_string(),
            base_url: "https://api.xiaomimimo.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "mimo-v2-tts".to_string(),
        };

        let config =
            build_provider_models_config_with_input(&provider, false, false).expect("tts config");
        let provider_json = &config["providers"]["nineclaw-runtime-8b9035807842"];
        assert!(provider_json["compat"]
            .get("requiresReasoningContentOnAssistantMessages")
            .is_none());
        assert!(provider_json["compat"].get("thinkingFormat").is_none());
    }

    #[test]
    fn provider_models_config_keeps_mimo_replay_compat_when_retry_disables_reasoning_effort() {
        let provider = ProviderRuntimeConfig {
            provider_id: "custom".to_string(),
            api_format: "openai".to_string(),
            base_url: "https://api.xiaomimimo.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "mimo-v2.5".to_string(),
        };

        let config =
            build_provider_models_config_with_input(&provider, false, true).expect("mimo config");
        let provider_json = &config["providers"]["nineclaw-runtime-8b9035807842"];
        assert_eq!(
            provider_json["compat"]["requiresReasoningContentOnAssistantMessages"],
            json!(true)
        );
        assert_eq!(provider_json["compat"]["thinkingFormat"], json!("deepseek"));
        assert_eq!(provider_json["models"][0]["reasoning"], json!(true));
        assert!(!should_force_pi_thinking_off(&provider, true));
        assert_eq!(forced_pi_thinking_level(&provider, true), Some("medium"));
    }

    #[test]
    fn provider_models_config_marks_anthropic_mimo_as_reasoning_capable() {
        let provider = ProviderRuntimeConfig {
            provider_id: "custom".to_string(),
            api_format: "anthropic".to_string(),
            base_url: "https://token-plan-cn.xiaomimimo.com/anthropic".to_string(),
            api_key: "secret".to_string(),
            model: "mimo-v2.5-pro".to_string(),
        };

        let config =
            build_provider_models_config_with_input(&provider, false, false).expect("mimo config");
        let provider_json = &config["providers"]["nineclaw-runtime-8b9035807842"];
        assert_eq!(provider_json["api"], json!("anthropic-messages"));
        assert_eq!(provider_json["models"][0]["reasoning"], json!(true));
        assert_eq!(forced_pi_thinking_level(&provider, false), Some("medium"));
    }

    #[test]
    fn forced_pi_thinking_level_preserves_non_mimo_fallbacks() {
        let provider = ProviderRuntimeConfig {
            provider_id: "custom".to_string(),
            api_format: "openai".to_string(),
            base_url: "https://example.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "glm-4.6".to_string(),
        };

        assert_eq!(forced_pi_thinking_level(&provider, false), None);
        assert_eq!(forced_pi_thinking_level(&provider, true), Some("off"));
    }

    #[test]
    fn detects_provider_reasoning_history_rejection() {
        assert!(is_provider_reasoning_history_rejection_error(
            "400 The reasoning_content in the thinking mode must be passed back to the API."
        ));
        assert!(!is_provider_reasoning_history_rejection_error(
            "400 invalid api key"
        ));
    }

    #[test]
    fn sanitize_pi_session_replay_state_strips_reasoning_metadata() {
        let path = std::env::temp_dir().join(format!(
            "nineclaw-session-sanitize-test-{}.jsonl",
            Uuid::new_v4()
        ));
        fs::write(
            &path,
            r#"{"type":"message","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hidden","thinkingSignature":"reasoning_content"},{"type":"text","text":"visible"}],"reasoning_content":"opaque","reasoning_details":[{"x":1}]}}"#,
        )
        .expect("write contaminated session");

        let changed = sanitize_pi_session_replay_state(&path).expect("sanitize session");
        let sanitized = fs::read_to_string(&path).expect("read sanitized");
        let _ = fs::remove_file(&path);

        assert!(changed);
        assert!(sanitized.contains("visible"));
        assert!(!sanitized.contains("reasoning_content"));
        assert!(!sanitized.contains("thinkingSignature"));
        assert!(!sanitized.contains(r#""type":"thinking""#));
    }

    #[test]
    fn quarantine_pi_session_file_moves_contaminated_session_as_backup() {
        let path = std::env::temp_dir().join(format!(
            "nineclaw-session-quarantine-test-{}.jsonl",
            Uuid::new_v4()
        ));
        fs::write(&path, "{}\n").expect("write session");

        let changed = quarantine_pi_session_file(&path, "reasoning-history").expect("quarantine");
        assert!(changed);
        assert!(!path.exists());

        let parent = path.parent().expect("temp parent");
        let file_name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("file stem");
        let backup = fs::read_dir(parent)
            .expect("read temp")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|candidate| {
                candidate
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(|name| name.starts_with(file_name) && name.contains("quarantine"))
                    .unwrap_or(false)
            })
            .expect("backup exists");
        let _ = fs::remove_file(backup);
    }

    #[test]
    fn anthropic_compat_extension_defends_against_url_only_image_blocks() {
        let source = build_desktop_anthropic_compat_extension_source(
            Path::new("/tmp/pi-ai/index.js"),
            "provider",
            "API_KEY",
            "https://example.com",
            "model",
            true,
            false,
        )
        .expect("extension source");

        assert!(source.contains("imageBlockPlaceholder"));
        assert!(source.contains("image_url"));
        assert!(source.contains("input: ['text']"));
        assert!(source.contains("URL-only image block is not replayable"));
    }

    #[test]
    fn anthropic_compat_extension_uses_serialized_import_path_and_multimodal_input() {
        let source = build_desktop_anthropic_compat_extension_source(
            Path::new("/tmp/pi runtime/pi-ai/index.js"),
            "provider-id",
            "API_KEY",
            "https://example.com/base/",
            "model-name",
            false,
            false,
        )
        .expect("extension source");

        assert!(source.contains("\"/tmp/pi runtime/pi-ai/index.js\""));
        assert!(source.contains("input: ['text', 'image']"));
        assert!(source.contains("provider-id"));
        assert!(source.contains("https://example.com/base/"));
        assert!(source.contains("model-name"));
    }

    #[test]
    fn anthropic_compat_extension_marks_mimo_models_as_reasoning_capable() {
        let source = build_desktop_anthropic_compat_extension_source(
            Path::new("/tmp/pi-ai/index.js"),
            "provider",
            "API_KEY",
            "https://example.com/base/",
            "mimo-v2.5-pro",
            false,
            true,
        )
        .expect("extension source");

        assert!(source.contains("reasoning: true"));
        assert!(source.contains("REQUIRES_REASONING_CONTENT_REPLAY = true"));
        assert!(source.contains("assistantMessage.reasoning_content"));
        assert!(source.contains("type: 'thinking', thinking: sanitizedReasoningContent"));
        assert!(source.contains("payload.thinking"));
        assert!(source.contains("thinkingSignature: 'reasoning_content'"));
    }

    #[test]
    fn anthropic_compat_extension_preserves_error_causes() {
        let source = build_desktop_anthropic_compat_extension_source(
            Path::new("/tmp/pi-ai/index.js"),
            "provider",
            "API_KEY",
            "https://example.com/base/",
            "mimo-v2.5-pro",
            false,
            true,
        )
        .expect("extension source");

        assert!(source.contains("function formatErrorWithCause(error)"));
        assert!(source.contains("error.cause"));
        assert!(source.contains("output.errorMessage = formatErrorWithCause(error);"));
    }

    #[test]
    fn resolve_pi_ai_import_path_prefers_pi_package_node_modules() {
        let root = std::env::temp_dir().join(format!("nineclaw-pi-ai-path-{}", Uuid::new_v4()));
        let pi_dir = root.join("runtime");
        let pi_path = pi_dir.join("pi");
        let candidate = root
            .join("pi-package")
            .join("node_modules")
            .join("@mariozechner")
            .join("pi-ai")
            .join("dist")
            .join("index.js");

        fs::create_dir_all(candidate.parent().expect("candidate parent"))
            .expect("create candidate");
        fs::create_dir_all(&pi_dir).expect("create pi dir");
        fs::write(&pi_path, "#!/bin/sh\n").expect("write pi");
        fs::write(&candidate, "export {};\n").expect("write candidate");

        let resolved = resolve_pi_ai_import_path(&pi_path).expect("resolve path");
        let expected = fs::canonicalize(&candidate).expect("canonical candidate");
        assert_eq!(resolved, expected);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_pi_ai_import_path_falls_back_to_nested_pi_dependency() {
        let root = std::env::temp_dir().join(format!("nineclaw-pi-ai-nested-{}", Uuid::new_v4()));
        let pi_dir = root.join("runtime");
        let pi_path = pi_dir.join("pi");
        let candidate = root
            .join("node_modules")
            .join("@mariozechner")
            .join("pi-coding-agent")
            .join("node_modules")
            .join("@mariozechner")
            .join("pi-ai")
            .join("dist")
            .join("index.js");

        fs::create_dir_all(candidate.parent().expect("candidate parent"))
            .expect("create candidate");
        fs::create_dir_all(&pi_dir).expect("create pi dir");
        fs::write(&pi_path, "#!/bin/sh\n").expect("write pi");
        fs::write(&candidate, "export {};\n").expect("write candidate");

        let resolved = resolve_pi_ai_import_path(&pi_path).expect("resolve path");
        let expected = fs::canonicalize(&candidate).expect("canonical candidate");
        assert_eq!(resolved, expected);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_pi_ai_import_path_returns_none_when_no_candidate_exists() {
        let root = std::env::temp_dir().join(format!("nineclaw-pi-ai-missing-{}", Uuid::new_v4()));
        let pi_dir = root.join("runtime");
        let pi_path = pi_dir.join("pi");

        fs::create_dir_all(&pi_dir).expect("create pi dir");
        fs::write(&pi_path, "#!/bin/sh\n").expect("write pi");

        let resolved = resolve_pi_ai_import_path(&pi_path);
        assert!(resolved.is_none());

        let _ = fs::remove_dir_all(&root);
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

    // ── Context Window Guard tests ──

    #[test]
    fn parse_context_stats_valid() {
        let json = json!({
            "contextUsage": {
                "usedTokens": 5000,
                "contextWindow": 128000,
                "inputTokens": 3000,
                "outputTokens": 2000
            }
        });
        let stats = parse_context_stats_from_rpc_response(&json);
        assert!(stats.is_some());
        let s = stats.unwrap();
        assert_eq!(s.used_tokens, 5000);
        assert_eq!(s.context_window, Some(128000));
        assert_eq!(s.input_tokens, 3000);
        assert_eq!(s.output_tokens, 2000);
    }

    #[test]
    fn parse_context_stats_missing_usage() {
        let json = json!({});
        let stats = parse_context_stats_from_rpc_response(&json);
        assert!(stats.is_none());
    }

    #[test]
    fn parse_context_stats_null_context_window() {
        let json = json!({
            "contextUsage": {
                "usedTokens": 5000,
                "contextWindow": null,
                "inputTokens": 3000,
                "outputTokens": 2000
            }
        });
        let stats = parse_context_stats_from_rpc_response(&json);
        assert!(stats.is_some());
        assert_eq!(stats.unwrap().context_window, None);
    }

    #[test]
    fn parse_context_stats_zero_tokens() {
        let json = json!({
            "contextUsage": {
                "usedTokens": 0,
                "contextWindow": 0,
                "inputTokens": 0,
                "outputTokens": 0
            }
        });
        let stats = parse_context_stats_from_rpc_response(&json);
        assert!(stats.is_some());
        let s = stats.unwrap();
        assert_eq!(s.used_tokens, 0);
        assert_eq!(s.context_window, None); // 0 is treated as None
    }

    #[test]
    fn resolve_context_window_prefers_auto() {
        let (tokens, source) = resolve_context_window_from_sources(Some(128000), Some(64000));
        assert_eq!(tokens, Some(128000));
        assert_eq!(source, "auto");
    }

    #[test]
    fn resolve_context_window_falls_back_to_manual() {
        let (tokens, source) = resolve_context_window_from_sources(None, Some(64000));
        assert_eq!(tokens, Some(64000));
        assert_eq!(source, "manual");
    }

    #[test]
    fn resolve_context_window_unknown_when_both_absent() {
        let (tokens, source) = resolve_context_window_from_sources(None, None);
        assert_eq!(tokens, None);
        assert_eq!(source, "unknown");
    }

    #[test]
    fn resolve_context_window_ignores_zero_auto() {
        let (tokens, source) = resolve_context_window_from_sources(Some(0), Some(64000));
        assert_eq!(tokens, Some(64000));
        assert_eq!(source, "manual");
    }

    #[test]
    fn aggregate_history_turns_sums_turn_usage() {
        let turns = vec![json!({
            "usage": {"inputTokens": 100, "outputTokens": 50, "totalTokens": 160}
        })];
        let merged = aggregate_token_usage_from_history_turns(Some(&turns));
        let p = merged.expect("merged usage");
        assert_eq!(p.input_tokens, Some(100));
        assert_eq!(p.output_tokens, Some(50));
        assert_eq!(usage_row_total_tokens(&p), 160);
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

fn desktop_idle_compression_epochs() -> &'static Mutex<HashMap<String, u64>> {
    DESKTOP_IDLE_COMPRESSION_EPOCHS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn bump_desktop_idle_compression_epoch(session_id: &str) -> u64 {
    let mut guard = desktop_idle_compression_epochs()
        .lock()
        .expect("DESKTOP_IDLE_COMPRESSION_EPOCHS poisoned");
    let entry = guard.entry(session_id.to_string()).or_insert(0);
    *entry = entry.saturating_add(1);
    *entry
}

fn current_desktop_idle_compression_epoch(session_id: &str) -> u64 {
    let guard = desktop_idle_compression_epochs()
        .lock()
        .expect("DESKTOP_IDLE_COMPRESSION_EPOCHS poisoned");
    guard.get(session_id).copied().unwrap_or(0)
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

fn send_pi_rpc_command(
    stdin: &Arc<Mutex<Option<ChildStdin>>>,
    command: serde_json::Value,
) -> Result<(), String> {
    let mut stdin_guard = stdin
        .lock()
        .map_err(|error| format!("无法锁定 rpc stdin: {error}"))?;
    let stdin = stdin_guard
        .as_mut()
        .ok_or_else(|| "pi stdin 已关闭，无法写入 rpc 命令".to_string())?;
    writeln!(stdin, "{command}").map_err(|error| format!("写入 rpc 命令失败: {error}"))?;
    stdin
        .flush()
        .map_err(|error| format!("刷新 rpc stdin 失败: {error}"))?;
    Ok(())
}

fn wait_for_pi_rpc_response(
    stdout_rx: &mpsc::Receiver<Result<String, String>>,
    command_id: &str,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let started = Instant::now();
    loop {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .ok_or_else(|| format!("等待 pi rpc 响应超时: {command_id}"))?;
        let line = match stdout_rx.recv_timeout(remaining.min(Duration::from_secs(5))) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => return Err(format!("读取 pi rpc 响应失败: {error}")),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err("等待 pi rpc 响应时 stdout 已断开".to_string())
            }
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|item| item.as_str()) != Some("response") {
            continue;
        }
        if value.get("id").and_then(|item| item.as_str()) != Some(command_id) {
            continue;
        }
        if value.get("success").and_then(|item| item.as_bool()) == Some(false) {
            let message = value
                .get("error")
                .and_then(|item| item.as_str())
                .unwrap_or("unknown error");
            return Err(format!("pi rpc 命令失败 {command_id}: {message}"));
        }
        return Ok(value);
    }
}

fn desktop_compression_archive_root(workspace_id: Option<&str>) -> Result<PathBuf, String> {
    if let Some(wid) = workspace_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(workspace_fs::team_root(wid)?
            .join(".debug")
            .join("session-compression"));
    }
    Ok(agent_workspace::resolve_workspace_root()?
        .join(".debug")
        .join("standalone")
        .join("session-compression"))
}

fn compression_config_from_env() -> session_compression::CompressionConfig {
    let mut config = session_compression::CompressionConfig::default();
    if let Some(value) = env_u64("NINECLAW_COMPRESSION_TOKEN_THRESHOLD") {
        config.token_threshold = value.max(1);
    }
    if let Some(value) = env_usize("NINECLAW_COMPRESSION_MESSAGE_THRESHOLD") {
        config.message_count_threshold = value.max(1);
    }
    if let Some(value) = env_u64("NINECLAW_COMPRESSION_TARGET_TOKENS") {
        config.target_compressed_tokens = value.max(1);
    }
    if let Some(value) = env_usize("NINECLAW_COMPRESSION_MAX_RECENT_MESSAGES") {
        config.max_recent_messages = value.max(1);
    }
    if let Some(value) = env_bool("NINECLAW_IDLE_COMPRESSION_ENABLED") {
        config.idle_compression_enabled = value;
    }
    if let Some(value) = env_u64("NINECLAW_IDLE_COMPRESSION_DELAY_MS") {
        config.idle_compression_delay_ms = value.max(1);
    }
    if let Some(value) = env_u64("NINECLAW_IDLE_COMPRESSION_TOKEN_THRESHOLD") {
        config.idle_token_threshold = value.max(1);
    }
    config
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.trim().parse::<u64>().ok()
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.trim().parse::<usize>().ok()
}

fn env_bool(name: &str) -> Option<bool> {
    let raw = std::env::var(name).ok()?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn maybe_compact_desktop_session_after_turn(
    app: &tauri::AppHandle,
    workspace_id: Option<&str>,
    session_id: &str,
    session_path: &Path,
    stdin: &Arc<Mutex<Option<ChildStdin>>>,
    stdout_rx: &mpsc::Receiver<Result<String, String>>,
    usage: Option<&PiTokenUsagePayload>,
    force_idle: bool,
) -> Result<bool, String> {
    let Some(usage) = usage else {
        return Ok(false);
    };
    let used_tokens = usage_row_total_tokens(usage);
    let config = compression_config_from_env();
    let entries = session_compression::load_session_entries(session_path)?;
    let Some(plan) =
        session_compression::plan_compression(&entries, used_tokens, &config, force_idle)
    else {
        return Ok(false);
    };

    maybe_compact_desktop_session_with_plan(
        app,
        workspace_id,
        session_id,
        stdin,
        stdout_rx,
        plan,
        &config,
    )
}

fn maybe_compact_desktop_session_with_plan(
    app: &tauri::AppHandle,
    workspace_id: Option<&str>,
    session_id: &str,
    stdin: &Arc<Mutex<Option<ChildStdin>>>,
    stdout_rx: &mpsc::Receiver<Result<String, String>>,
    plan: session_compression::CompressionPlan,
    config: &session_compression::CompressionConfig,
) -> Result<bool, String> {
    let archive_root = desktop_compression_archive_root(workspace_id)?;
    let command_id = format!("compact-{session_id}-{}", Utc::now().timestamp_millis());
    let custom_instructions = format!(
        "{}\n\n{}",
        "NineClaw compression policy: produce a concise structured summary with <topics> and <summary> tags. Do not continue the user task.",
        session_compression::build_compression_instruction(plan.compression_level, config)
            .get("content")
            .and_then(|item| item.as_str())
            .unwrap_or_default()
    );
    let command = json!({
        "id": command_id,
        "type": "compact",
        "customInstructions": custom_instructions,
    });
    send_pi_rpc_command(stdin, command)?;
    let response = wait_for_pi_rpc_response(stdout_rx, &command_id, Duration::from_secs(180))?;
    let summary = response
        .get("data")
        .and_then(|data| data.get("summary"))
        .and_then(|item| item.as_str())
        .unwrap_or_default();
    let topics = session_compression::parse_topics(summary);
    let archive = session_compression::write_chunk_archive(
        &archive_root,
        session_id,
        plan.compression_level,
        &plan,
        topics.as_deref(),
    )?;
    dev_trace(
        "desktop.stream",
        format!(
            "insert_then_compress completed: session={} reason={:?} tokens_before={} messages_before={} archive={}",
            session_id,
            plan.reason,
            plan.original_token_count,
            plan.original_message_count,
            archive.path
        ),
    );
    let _ = app.emit(
        "nineclaw-runtime-notification",
        serde_json::json!({
            "kind": "session_compressed",
            "sessionId": session_id,
            "archivePath": archive.path,
            "topics": topics,
        }),
    );
    Ok(true)
}

fn schedule_desktop_idle_compression(app: &tauri::AppHandle, session_id: &str) {
    let config = compression_config_from_env();
    if !config.idle_compression_enabled {
        return;
    }
    let epoch = bump_desktop_idle_compression_epoch(session_id);
    let app = app.clone();
    let session_id = session_id.to_string();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(config.idle_compression_delay_ms));
        if current_desktop_idle_compression_epoch(&session_id) != epoch {
            dev_trace(
                "desktop.stream",
                format!("idle compression cancelled by newer prompt: session={session_id}"),
            );
            return;
        }
        if get_runtime_handle(&session_id).ok().flatten().is_some() {
            dev_trace(
                "desktop.stream",
                format!("idle compression skipped while stream active: session={session_id}"),
            );
            return;
        }
        let Some(pooled) = take_pooled_desktop_pi(&session_id).ok().flatten() else {
            return;
        };
        let result = maybe_compact_desktop_session_after_turn(
            &app,
            pooled.workspace_id.as_deref(),
            &session_id,
            &pooled.session_path,
            &pooled.stdin,
            &pooled.stdout_rx,
            pooled.last_usage.as_ref(),
            true,
        );
        match result {
            Ok(true) => dev_trace(
                "desktop.stream",
                format!("idle compression applied: session={session_id}"),
            ),
            Ok(false) => dev_trace(
                "desktop.stream",
                format!("idle compression skipped below threshold: session={session_id}"),
            ),
            Err(error) => dev_trace(
                "desktop.stream",
                format!("idle compression failed and rolled back by PI: session={session_id} error={error}"),
            ),
        }
        if let Err(error) = store_pooled_desktop_pi(&session_id, pooled) {
            dev_trace(
                "desktop.stream",
                format!("idle compression pool restore failed: session={session_id} error={error}"),
            );
        }
    });
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
    proxy_base_url: &str,
    proxy_session_token: &str,
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
        proxy_base_url,
        proxy_session_token,
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

fn strip_provider_replay_state(value: &mut serde_json::Value) -> bool {
    let mut changed = false;
    match value {
        serde_json::Value::Object(map) => {
            for key in [
                "reasoning_content",
                "reasoningContent",
                "reasoning_details",
                "reasoningDetails",
                "reasoning",
                "thinkingSignature",
                "thoughtSignature",
            ] {
                if map.remove(key).is_some() {
                    changed = true;
                }
            }

            for child in map.values_mut() {
                if strip_provider_replay_state(child) {
                    changed = true;
                }
            }
        }
        serde_json::Value::Array(items) => {
            let original_len = items.len();
            items.retain(|item| {
                let is_thinking = item
                    .get("type")
                    .and_then(|kind| kind.as_str())
                    .map(|kind| {
                        matches!(
                            kind,
                            "thinking"
                                | "reasoning"
                                | "reasoning_content"
                                | "reasoningContent"
                                | "redacted_thinking"
                        )
                    })
                    .unwrap_or(false);
                !is_thinking
            });
            if items.len() != original_len {
                changed = true;
            }
            for child in items {
                if strip_provider_replay_state(child) {
                    changed = true;
                }
            }
        }
        _ => {}
    }
    changed
}

pub(crate) fn sanitize_pi_session_replay_state(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(path)
        .map_err(|error| format!("读取 pi session 以修复 reasoning 历史失败: {error}"))?;
    let mut changed = false;
    let mut sanitized_lines = Vec::new();

    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(mut value) => {
                if strip_provider_replay_state(&mut value) {
                    changed = true;
                }
                sanitized_lines.push(
                    serde_json::to_string(&value)
                        .map_err(|error| format!("序列化修复后的 pi session 失败: {error}"))?,
                );
            }
            Err(_) => sanitized_lines.push(line.to_string()),
        }
    }

    if !changed {
        return Ok(false);
    }

    let backup_path = path.with_extension(format!(
        "jsonl.reasoning-bak-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    ));
    fs::write(&backup_path, raw).map_err(|error| {
        format!(
            "备份污染的 pi session 失败 {}: {error}",
            backup_path.display()
        )
    })?;

    let mut sanitized = sanitized_lines.join("\n");
    sanitized.push('\n');
    fs::write(path, sanitized)
        .map_err(|error| format!("写回修复后的 pi session 失败 {}: {error}", path.display()))?;
    Ok(true)
}

pub(crate) fn quarantine_pi_session_file(path: &Path, reason: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let backup_path = path.with_extension(format!(
        "jsonl.quarantine-{}-{}",
        reason
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .trim_matches('-'),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    ));
    fs::rename(path, &backup_path).map_err(|error| {
        format!(
            "隔离污染的 pi session 失败 {} -> {}: {error}",
            path.display(),
            backup_path.display()
        )
    })?;
    Ok(true)
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
    if let Some(app) = managed_runtime::injected_app_handle() {
        if let Ok(conn) = storage_conn(&app) {
            if let Ok(rows) = crate::storage::core_memory::list_runtime_multimodal_summaries(
                &conn,
                summary_key,
                8,
            ) {
                if !rows.is_empty() {
                    return Ok(rows
                        .into_iter()
                        .map(|row| MultimodalSummaryEntry {
                            timestamp_ms: row.created_at,
                            user_prompt: row.user_prompt,
                            assistant_response: row.assistant_response,
                        })
                        .collect());
                }
            }
        }
    }

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

pub(crate) fn build_turn_prompt_with_multimodal_summary(
    prompt: &str,
    summary_key: &str,
    attachments: &[prompt_attachments::PromptAttachmentInput],
) -> Result<String, String> {
    if prompt_attachments::attachments_include_visual_context(attachments) {
        return Ok(prompt.to_string());
    }

    prepend_multimodal_summary_context(prompt, summary_key)
}

pub(crate) fn clear_multimodal_summary(summary_key: &str) -> Result<(), String> {
    if let Some(app) = managed_runtime::injected_app_handle() {
        if let Ok(conn) = storage_conn(&app) {
            let _ =
                crate::storage::core_memory::clear_runtime_multimodal_summaries(&conn, summary_key);
        }
    }
    let path = summary_file_path(summary_key);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("删除多模态摘要失败 {}: {error}", path.display())),
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

    if let Some(app) = managed_runtime::injected_app_handle() {
        if let Ok(conn) = storage_conn(&app) {
            let _ =
                crate::storage::core_memory::clear_runtime_multimodal_summaries(&conn, summary_key);
            for entry in &entries {
                let _ = crate::storage::core_memory::insert_runtime_multimodal_summary(
                    &conn,
                    summary_key,
                    &entry.user_prompt,
                    &entry.assistant_response,
                );
            }
        }
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

fn format_js_error_with_cause() -> &'static str {
    r#"function formatErrorWithCause(error) {
  if (!(error instanceof Error)) {
    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }

  let message = error.message || String(error);
  const cause = error.cause;
  if (cause instanceof Error) {
    const causeMessage = cause.message || String(cause);
    if (causeMessage && !message.includes(causeMessage)) {
      message += `; cause: ${causeMessage}`;
    }
  } else if (cause != null) {
    let causeText = '';
    try {
      causeText = typeof cause === 'string' ? cause : JSON.stringify(cause);
    } catch {
      causeText = String(cause);
    }
    if (causeText && !message.includes(causeText)) {
      message += `; cause: ${causeText}`;
    }
  }

  return message;
}
"#
}

fn build_desktop_anthropic_compat_extension_source(
    pi_ai_import_path: &Path,
    provider_id: &str,
    api_key_env: &str,
    base_url: &str,
    model: &str,
    text_only_input: bool,
    requires_reasoning_content_replay: bool,
) -> Result<String, String> {
    let import_path = js_string_literal(&pi_ai_import_path.to_string_lossy())?;
    let provider_id = js_string_literal(provider_id)?;
    let api_key_env = js_string_literal(api_key_env)?;
    let base_url = js_string_literal(base_url)?;
    let model = js_string_literal(model)?;
    let model_input = if text_only_input {
        "['text']"
    } else {
        "['text', 'image']"
    };
    let model_reasoning = if requires_reasoning_content_replay {
        "true"
    } else {
        "false"
    };

    Ok(format!(
        r#"import {{ createAssistantMessageEventStream, calculateCost, parseStreamingJson }} from {import_path};

{format_error_with_cause}

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

const REQUIRES_REASONING_CONTENT_REPLAY = {model_reasoning};

function convertContentBlocks(content) {{
  const items = Array.isArray(content) ? content : [];
  const hasImages = items.some((block) => block?.type === 'image' && block.data && block.mimeType);
  if (!hasImages) {{
    return sanitizeSurrogates(items.map((block) => block?.text || imageBlockPlaceholder(block)).filter(Boolean).join('\n'));
  }}

  const blocks = items.flatMap((block) => {{
    if (block.type === 'text') {{
      const text = sanitizeSurrogates(block.text);
      return text.trim() ? [{{ type: 'text', text }}] : [];
    }}

    if (block.type === 'image' && block.data && block.mimeType) {{
      return [{{
        type: 'image',
        source: {{
          type: 'base64',
          media_type: block.mimeType,
          data: block.data,
        }},
      }}];
    }}

    const placeholder = imageBlockPlaceholder(block);
    return placeholder ? [{{ type: 'text', text: sanitizeSurrogates(placeholder) }}] : [];
  }});

  if (!blocks.some((block) => block.type === 'text')) {{
    blocks.unshift({{ type: 'text', text: '(see attached image)' }});
  }}

  return blocks;
}}

function imageBlockPlaceholder(block) {{
  if (!block || typeof block !== 'object') return '';
  if (block.type === 'image_url' || block.image_url) return '[image omitted: URL-only image block is not replayable in this runtime]';
  if (block.type === 'input_image' || block.image_url) return '[image omitted: URL-only image block is not replayable in this runtime]';
  if (block.type === 'image') return '[image omitted: image block was missing base64 data or mime type]';
  return '';
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
        const blocks = (Array.isArray(message.content) ? message.content : []).flatMap((item) => {{
          if (item.type === 'text') {{
            const text = sanitizeSurrogates(item.text);
            return text.trim() ? [{{ type: 'text', text }}] : [];
          }}
          if (item.type === 'image' && item.data && item.mimeType) {{
            return [{{
                type: 'image',
                source: {{
                  type: 'base64',
                  media_type: item.mimeType,
                  data: item.data,
                }},
              }}];
          }}
          const placeholder = imageBlockPlaceholder(item);
          return placeholder ? [{{ type: 'text', text: sanitizeSurrogates(placeholder) }}] : [];
        }});
        if (blocks.length > 0) {{
          params.push({{ role: 'user', content: blocks }});
        }}
      }}
      continue;
    }}

    if (message.role === 'assistant') {{
      const blocks = [];
      const reasoningBlocks = [];
      for (const block of message.content) {{
        if (block.type === 'text' && block.text.trim()) {{
          blocks.push({{ type: 'text', text: sanitizeSurrogates(block.text) }});
        }} else if (block.type === 'thinking' && block.thinking.trim()) {{
          if (REQUIRES_REASONING_CONTENT_REPLAY) {{
            reasoningBlocks.push(block.thinking);
          }} else {{
            blocks.push({{ type: 'text', text: sanitizeSurrogates(block.thinking) }});
          }}
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
        const assistantMessage = {{ role: 'assistant', content: blocks }};
        const reasoningContent = reasoningBlocks.join('\n').trim();
        if (REQUIRES_REASONING_CONTENT_REPLAY && reasoningContent) {{
          const sanitizedReasoningContent = sanitizeSurrogates(reasoningContent);
          assistantMessage.content = [
            {{ type: 'thinking', thinking: sanitizedReasoningContent }},
            ...blocks,
          ];
          assistantMessage.reasoning_content = sanitizedReasoningContent;
        }}
        params.push(assistantMessage);
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
  if (reason == null || reason === '') {{
    return 'stop';
  }}
  const normalized = String(reason);
  switch (normalized) {{
    case 'end_turn':
    case 'pause_turn':
    case 'stop_sequence':
    case 'stop':
      return 'stop';
    case 'max_tokens':
      return 'length';
    case 'tool_use':
    case 'tool_calls':
      return 'toolUse';
    // 部分网关/兼容层会返回非标准 finish_reason，流已正常结束时不宜当成硬错误。
    case 'unexpected_state':
    case 'paused':
      return 'stop';
    default:
      if (/unexpected/i.test(normalized)) {{
        return 'stop';
      }}
      return 'error';
  }}
}}

function thinkingBudgetTokens(level, maxTokens) {{
  const budgets = {{
    minimal: 1024,
    low: 2048,
    medium: 4096,
    high: 8192,
    xhigh: 12000,
  }};
  const requested = budgets[level] || budgets.medium;
  return Math.max(1024, Math.min(requested, Math.max(1024, maxTokens - 1024)));
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

      if (model.reasoning && options?.reasoning && options.reasoning !== 'off') {{
        payload.thinking = {{
          type: 'enabled',
          budget_tokens: thinkingBudgetTokens(options.reasoning, payload.max_tokens),
        }};
      }}

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
          }} else if (
            event.content_block?.type === 'thinking' ||
            event.content_block?.type === 'reasoning' ||
            event.content_block?.type === 'reasoning_content'
          ) {{
            output.content.push({{ type: 'thinking', thinking: '', thinkingSignature: 'reasoning_content', index: event.index }});
            stream.push({{ type: 'thinking_start', contentIndex: output.content.length - 1, partial: output }});
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
          }} else if (
            block.type === 'thinking' &&
            (
              event.delta?.type === 'thinking_delta' ||
              event.delta?.type === 'reasoning_delta' ||
              event.delta?.type === 'reasoning_content_delta' ||
              event.delta?.reasoning_content != null ||
              event.delta?.thinking != null ||
              event.delta?.text != null
            )
          ) {{
            const thinkingDelta = event.delta?.text ?? event.delta?.reasoning_content ?? event.delta?.thinking ?? '';
            if (thinkingDelta) {{
              block.thinking += thinkingDelta;
              stream.push({{ type: 'thinking_delta', contentIndex, delta: thinkingDelta, partial: output }});
            }}
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
          }} else if (block.type === 'thinking') {{
            stream.push({{ type: 'thinking_end', contentIndex, content: block.thinking, partial: output }});
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
        const hasRenderable = output.content.some((block) =>
          (block.type === 'text' && String(block.text || '').trim().length > 0) ||
          block.type === 'toolCall',
        );
        if (hasRenderable) {{
          output.stopReason = 'stop';
          output.errorMessage = undefined;
        }} else {{
          throw new Error('Anthropic-compatible stream ended without a valid stop reason');
        }}
      }}

      stream.push({{ type: 'done', reason: output.stopReason, message: output }});
      stream.end();
    }} catch (error) {{
      for (const block of output.content) {{
        delete block.index;
      }}
      output.stopReason = options?.signal?.aborted ? 'aborted' : 'error';
      output.errorMessage = formatErrorWithCause(error);
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
      reasoning: {model_reasoning},
      input: {model_input},
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
        model_input = model_input,
        model_reasoning = model_reasoning,
        format_error_with_cause = format_js_error_with_cause(),
    ))
}

fn prepare_desktop_anthropic_compat_extension(
    runtime_dir: &Path,
    pi_executable: &Path,
    provider_config: &ProviderRuntimeConfig,
    text_only_input: bool,
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
        text_only_input,
        openai_pi_compat_requires_reasoning_content_replay(provider_config.model.trim()),
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
    text_only_input: bool,
    disable_reasoning_effort: bool,
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
    let requires_explicit_thinking_disable =
        openai_pi_compat_requires_explicit_thinking_disable(model);
    let requires_reasoning_content_replay =
        openai_pi_compat_requires_reasoning_content_replay(model);
    let effective_disable_reasoning_effort =
        disable_reasoning_effort && !requires_reasoning_content_replay;
    let model_reasoning = !effective_disable_reasoning_effort
        && (openai_pi_compat_supports_reasoning_effort(model)
            || requires_explicit_thinking_disable
            || requires_reasoning_content_replay);
    let mut provider = serde_json::Map::new();
    let model_input = if text_only_input {
        json!(["text"])
    } else {
        json!(["text", "image"])
    };
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
                    "input": model_input,
                    "reasoning": requires_reasoning_content_replay
                  }
                ]),
            );
        }
        _ => {
            provider.insert("api".to_string(), json!("openai-completions"));
            let mut compat = serde_json::Map::new();
            compat.insert("supportsDeveloperRole".to_string(), json!(false));
            compat.insert(
                "supportsReasoningEffort".to_string(),
                json!(model_reasoning && !requires_explicit_thinking_disable),
            );
            if requires_explicit_thinking_disable {
                compat.insert("thinkingFormat".to_string(), json!("qwen"));
            }
            if requires_reasoning_content_replay {
                compat.insert(
                    "requiresReasoningContentOnAssistantMessages".to_string(),
                    json!(true),
                );
                compat.insert("thinkingFormat".to_string(), json!("deepseek"));
            }
            provider.insert("compat".to_string(), serde_json::Value::Object(compat));
            provider.insert(
                "models".to_string(),
                json!([
                  {
                    "id": model,
                    "api": "openai-completions",
                    "input": model_input,
                    "reasoning": model_reasoning
                  }
                ]),
            );
        }
    }
    provider
}

fn build_provider_models_config_with_input(
    provider_config: &ProviderRuntimeConfig,
    text_only_input: bool,
    disable_reasoning_effort: bool,
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
    let provider =
        custom_provider_object(provider_config, text_only_input, disable_reasoning_effort);
    let mut providers = serde_json::Map::new();
    providers.insert(
        runtime_provider_id(provider_id),
        serde_json::Value::Object(provider),
    );
    Some(json!({ "providers": providers }))
}

#[allow(dead_code)]
fn prepare_pi_runtime_dir(provider_config: &ProviderRuntimeConfig) -> Result<PathBuf, String> {
    let runtime_dir = pi_runtime_dir();
    fs::create_dir_all(&runtime_dir).map_err(|error| format!("创建 pi 运行目录失败: {error}"))?;

    let auth_path = runtime_dir.join("auth.json");
    fs::write(&auth_path, "{}").map_err(|error| format!("写入 pi auth 配置失败: {error}"))?;

    let models_path = runtime_dir.join("models.json");
    if let Some(models_config) =
        build_provider_models_config_with_input(provider_config, false, false)
    {
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

fn is_provider_image_block_rejection_error(error: &str) -> bool {
    let lower = error.trim().to_ascii_lowercase();
    (lower.contains("image_url")
        || lower.contains("input_image")
        || lower.contains("unknown variant `image")
        || lower.contains("unknown variant image"))
        && (lower.contains("expected `text`")
            || lower.contains("expected text")
            || lower.contains("messages[")
            || lower.contains("deserialize"))
}

fn is_provider_reasoning_history_rejection_error(error: &str) -> bool {
    let lower = error.trim().to_ascii_lowercase();
    (lower.contains("reasoning_content")
        || lower.contains("reasoning content")
        || lower.contains("thinking mode")
        || lower.contains("reasoning mode"))
        && (lower.contains("must be passed back")
            || lower.contains("pass back")
            || lower.contains("missing")
            || lower.contains("required"))
}

fn should_retry_text_only_after_image_rejection(
    attempt: usize,
    current_turn_has_visual_context: bool,
    error: &str,
) -> bool {
    attempt == 0
        && !current_turn_has_visual_context
        && is_provider_image_block_rejection_error(error)
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
            strategy: None,
            mounted_skill_ids: None,
            reasons: None,
            usage,
            usage_meta,
        },
    )
    .map_err(|emit_error| format!("发送事件失败: {emit_error}"))
}

fn emit_pi_stream_skill_selection_event(
    app: &tauri::AppHandle,
    session_id: Option<String>,
    decision: &crate::skill_broker::SkillBrokerDecision,
) -> Result<(), String> {
    app.emit(
        "pi://stream",
        PiStreamPayload {
            event: "skill_selection".to_string(),
            session_id,
            text: None,
            error: None,
            aborted_by: None,
            tool_call_id: None,
            tool_name: None,
            args_text: None,
            result_text: None,
            is_error: None,
            reason: None,
            strategy: Some(decision.strategy.clone()),
            mounted_skill_ids: Some(decision.mounted_skill_ids.clone()),
            reasons: Some(decision.reasons.clone()),
            usage: None,
            usage_meta: None,
        },
    )
    .map_err(|emit_error| format!("发送能力装配事件失败: {emit_error}"))
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
    apply_main_window_size_and_center(app);

    // macOS can ignore the first centering request if the native window is not
    // fully realized yet, so retry shortly after startup on the main thread.
    let app_handle = app.clone();
    thread::spawn(move || {
        for delay_ms in [150_u64, 450_u64] {
            thread::sleep(Duration::from_millis(delay_ms));
            let app_handle = app_handle.clone();
            let main_thread_handle = app_handle.clone();
            let _ = app_handle.run_on_main_thread(move || {
                apply_main_window_size_and_center(&main_thread_handle);
            });
        }
    });
}

fn apply_main_window_size_and_center(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    match window.current_monitor() {
        Ok(Some(monitor)) => {
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
        }
        Ok(None) => {
            eprintln!("NineClaw: 启动时尚未解析到当前显示器，跳过尺寸自适应，仍将尝试居中");
        }
        Err(error) => {
            eprintln!("NineClaw: 读取显示器信息失败，跳过窗口尺寸自适应: {error}");
        }
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

    // 成功结束时常带空的或警告性 errorMessage，避免当成致命错误。
    if stop_reason != "error" {
        if let Some(ref text) = error_text {
            if provider_stream_noise::is_recoverable_provider_terminal_noise(text) {
                return None;
            }
        }
    }

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
async fn compact_desktop_session_before_model_switch(
    app: tauri::AppHandle,
    session_id: String,
    workspace_id: Option<String>,
    current_model: String,
    next_model: String,
) -> Result<DesktopCompressionCommandResult, String> {
    let session_id = session_id.trim().to_string();
    if session_id.is_empty() {
        return Ok(DesktopCompressionCommandResult {
            compressed: false,
            reason: "empty_session_id".to_string(),
        });
    }
    if current_model.trim() == next_model.trim() {
        return Ok(DesktopCompressionCommandResult {
            compressed: false,
            reason: "same_model".to_string(),
        });
    }
    if get_runtime_handle(&session_id)?.is_some() {
        return Ok(DesktopCompressionCommandResult {
            compressed: false,
            reason: "session_running".to_string(),
        });
    }

    let app_for_task = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(pooled) = take_pooled_desktop_pi(&session_id)? else {
            return Ok(DesktopCompressionCommandResult {
                compressed: false,
                reason: "no_pooled_runtime".to_string(),
            });
        };

        let config = compression_config_from_env();
        let entries = session_compression::load_session_entries(&pooled.session_path)?;
        let used_tokens = pooled
            .last_usage
            .as_ref()
            .map(usage_row_total_tokens)
            .unwrap_or_default();
        let plan = session_compression::plan_model_switch_compression(&entries, used_tokens, &config);
        let result = match plan {
            Some(plan) => maybe_compact_desktop_session_with_plan(
                &app_for_task,
                workspace_id.as_deref().or(pooled.workspace_id.as_deref()),
                &session_id,
                &pooled.stdin,
                &pooled.stdout_rx,
                plan,
                &config,
            )
            .map(|compressed| DesktopCompressionCommandResult {
                compressed,
                reason: if compressed {
                    "model_switch_compressed".to_string()
                } else {
                    "compression_skipped".to_string()
                },
            }),
            None => Ok(DesktopCompressionCommandResult {
                compressed: false,
                reason: "below_model_switch_threshold".to_string(),
            }),
        };

        if let Err(error) = store_pooled_desktop_pi(&session_id, pooled) {
            dev_trace(
                "desktop.stream",
                format!("model-switch compression pool restore failed: session={session_id} error={error}"),
            );
        }

        result
    })
    .await
    .map_err(|error| format!("模型切换前压缩任务失败: {error}"))?
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
    app: tauri::AppHandle,
    agent_id: String,
    session_id: Option<String>,
    workspace_id: Option<String>,
    attachments: Vec<ChatAttachmentUpload>,
) -> Result<Vec<PersistedChatAttachment>, String> {
    let session_workspace_root: Option<std::path::PathBuf> = {
        let sid = session_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(sid) = sid {
            let conn = storage_conn(&app)?;
            Some(crate::session_workspace::current_workspace_dir(
                &conn,
                sid,
                workspace_id.as_deref(),
            )?)
        } else {
            None
        }
    };

    tauri::async_runtime::spawn_blocking(move || {
        chat_attachments::persist_chat_attachments(
            &agent_id,
            session_id.as_deref(),
            session_workspace_root.as_deref(),
            attachments,
        )
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

/// 打开 macOS 系统设置 → 通知面板
#[tauri::command]
async fn open_system_notification_settings() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.Notifications-Settings.extension")
                .spawn()
                .map_err(|e| format!("打开系统通知设置失败: {e}"))?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            // Windows / Linux fallback — 暂不实现
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("打开系统通知设置失败: {error}"))?
}

/// 通过 osascript 发送 macOS 系统通知（绕过 notify-rust，兼容 macOS 15+）
#[tauri::command]
async fn send_native_notification(
    title: String,
    body: String,
    subtitle: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");
        let escaped_body = body.replace('\\', "\\\\").replace('"', "\\\"");
        let escaped_subtitle = subtitle
            .as_deref()
            .map(|s| s.replace('\\', "\\\\").replace('"', "\\\""));

        let script = if let Some(st) = escaped_subtitle {
            format!(
                "display notification \"{}\" with title \"{}\" subtitle \"{}\"",
                escaped_body, escaped_title, st
            )
        } else {
            format!(
                "display notification \"{}\" with title \"{}\"",
                escaped_body, escaped_title
            )
        };

        let output = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("执行 osascript 失败: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("osascript 报错: {stderr}"));
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("发送通知失败: {error}"))?
}

/// 调试用：通过 Rust 端直接发一条系统通知，用于排查前端通知不弹窗问题
#[tauri::command]
async fn debug_send_test_notification() -> Result<String, String> {
    let output = std::process::Command::new("osascript")
        .args(["-e", "display notification \"如果你看到这条通知，说明通知功能正常！\" with title \"NineClaw 测试\""])
        .output()
        .map_err(|e| format!("执行 osascript 失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("osascript 报错: {stderr}"));
    }
    Ok("通知已发送".to_string())
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

fn maybe_expand_team_delegates(
    app: &tauri::AppHandle,
    workspace_id: Option<&String>,
    provider_config: &Option<ProviderRuntimeConfig>,
    text: &mut String,
) {
    // 团队边界护栏：仅在"当前会话绑定了具体 workspace"时才展开委派标记。
    // peer_gateway 的入站会话走 `process_message_interruptible`，不会调到这里；
    // 即使将来有新路径误把 peer 文本喂到本函数，这里的空 workspace_id 早退出
    // 也会让外部智能体无法借道 `NINECLAW_DELEGATE_JSON:` / `_PLAN_JSON:` 触发本地委派。
    let Some(wid) = workspace_id.map(|s| s.as_str()).filter(|s| !s.is_empty()) else {
        return;
    };

    if text.contains("NINECLAW_DELEGATE_PLAN_JSON:") {
        match team_workspace::expand_delegate_plan_markers_in_text(text.as_str()) {
            Ok(next) => *text = next,
            Err(e) => text.push_str(&format!("\n\n[委派计划标记解析失败: {e}]\n")),
        }
    }

    let Some(p) = provider_config.as_ref() else {
        return;
    };
    if !text.contains("NINECLAW_DELEGATE_JSON:") {
        return;
    }
    match team_workspace::expand_delegate_markers_in_text(app, wid, text.as_str(), Some(p)) {
        Ok(next) => *text = next,
        Err(e) => text.push_str(&format!("\n\n[委派标记解析失败: {e}]\n")),
    }
}

#[tauri::command]
async fn stream_pi_prompt(
    app: tauri::AppHandle,
    prompt: String,
    session_id: Option<String>,
    provider_config: Option<ProviderRuntimeConfig>,
    agent_config: Option<ConversationAgentConfig>,
    attachments: Option<Vec<prompt_attachments::PromptAttachmentInput>>,
    workspace_id: Option<String>,
    runtime_parameters: Option<runtime_parameters::RuntimeParametersPayload>,
) -> Result<(), String> {
    let merged_runtime_parameters =
        crate::runtime_parameters::merge_from_payload(runtime_parameters);
    let trimmed_prompt = prompt.trim().to_string();
    let attachments = attachments.unwrap_or_default();
    if trimmed_prompt.is_empty() && attachments.is_empty() {
        return Err("prompt 不能为空".to_string());
    }

    managed_runtime::inject_credential_proxy_app_handle(app.clone());
    let agent_config = runtime_agent_config::refresh_runtime_agent_config(&app, agent_config)?;

    let normalized_session_id = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "default".to_string());
    bump_desktop_idle_compression_epoch(&normalized_session_id);
    let summary_key = session_summary_key(&normalized_session_id);

    let workspace_id_for_stream = workspace_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    // 团队边界硬校验：团队会话里当前 agent 必须是该工作空间的成员（含主智能体）。
    // 这层兜住前端任意入口漏过滤的情况，防止"团队外智能体"被召唤到团队会话中。
    if let (Some(wid), Some(cfg)) = (workspace_id_for_stream.as_deref(), agent_config.as_ref()) {
        let agent_id = cfg.id.trim();
        if !agent_id.is_empty() {
            let conn = storage_conn(&app)?;
            let ws = storage::workspaces::get_workspace(&conn, wid)?;
            if let Some(ws) = ws {
                let is_supervisor = ws.supervisor_agent_id == agent_id;
                let is_member = if is_supervisor {
                    true
                } else {
                    let members = storage::workspaces::list_workspace_members(&conn, wid)?;
                    members.iter().any(|m| m.agent_id == agent_id)
                };
                if !is_member {
                    return Err(format!(
                        "WORKSPACE_MEMBER_ONLY: 智能体 `{}` 不是团队 `{}` 的成员，无法在该团队会话中发言",
                        agent_id, wid
                    ));
                }
            }
        }
    }

    let workspace_root_for_media: Option<std::path::PathBuf> = storage_conn(&app)
        .ok()
        .and_then(|conn| {
            crate::session_workspace::current_workspace_dir(
                &conn,
                &normalized_session_id,
                workspace_id_for_stream.as_deref(),
            )
            .ok()
        })
        .or_else(|| {
            workspace_id_for_stream
                .as_deref()
                .and_then(|wid| team_workspace::resolve_workspace_artifacts_root(&app, wid).ok())
        });
    let desktop_agent_home_for_runtime = agent_config.as_ref().and_then(|config| {
        agent_workspace::resolve_workspace_root()
            .ok()
            .map(|root| root.join("agents").join(&config.id))
    });
    let desktop_harness_for_retry = match (
        desktop_agent_home_for_runtime.as_ref(),
        agent_config.as_ref(),
    ) {
        (Some(agent_home), Some(config)) => managed_runtime::select_harness(
            agent_home,
            &config.execution_mode,
            Some(trimmed_prompt.as_str()),
        )
        .ok(),
        _ => None,
    };
    managed_runtime::append_session_event_quiet(
        desktop_agent_home_for_runtime.as_deref(),
        &normalized_session_id,
        managed_runtime::SessionEventKind::Prompt,
        trimmed_prompt.clone(),
        None,
    );

    let current_turn_has_visual_context =
        prompt_attachments::attachments_include_visual_context(&attachments);
    let mut attempt_prompt = trimmed_prompt.clone();
    let mut text_only_provider_input_retry = false;
    let mut disable_reasoning_provider_retry = false;
    let llm_attempt_cap = merged_runtime_parameters
        .llm_outer_max_attempt_rounds
        .max(1)
        .min(24) as usize;
    for attempt in 0..llm_attempt_cap {
        let app = app.clone();
        let tool_iteration_cap = merged_runtime_parameters.max_agent_tool_rounds_per_dialogue;
        let stall_retry_cap = merged_runtime_parameters.stream_disconnect_max_retries;
        let delegate_iteration_cap = merged_runtime_parameters
            .max_agent_tool_rounds_per_dialogue
            .max(1)
            .min(500);
        let session_id_for_attempt = normalized_session_id.clone();
        let normalized_session_id = session_id_for_attempt.clone();
        let runtime_session_id = session_id_for_attempt.clone();
        let provider_config = provider_config.clone();
        let attempt_model_requires_reasoning_replay = provider_config
            .as_ref()
            .map(|config| openai_pi_compat_requires_reasoning_content_replay(&config.model))
            .unwrap_or(false);
        let text_only_provider_input = text_only_provider_input_retry;
        let disable_reasoning_effort = disable_reasoning_provider_retry;
        let agent_config = agent_config.clone();
        let attachments = attachments.clone();
        let workspace_id_for_stream = workspace_id_for_stream.clone();
        let workspace_root_for_media = workspace_root_for_media.clone();
        let summary_key = summary_key.clone();
        let trimmed_prompt = attempt_prompt.clone();
        let mut turn_prompt =
            build_turn_prompt_with_multimodal_summary(&trimmed_prompt, &summary_key, &attachments)?;
        if let Some(agent_config) = agent_config.as_ref() {
            let mut blocks = agent_turn_context::build_session_context_blocks(
                agent_config,
                Some(normalized_session_id.as_str()),
                trimmed_prompt.as_str(),
            )?;
            if let Some(wid) = workspace_id_for_stream.as_deref() {
                let speaker_id = agent_config.id.trim();
                agent_turn_context::extend_with_block(
                    &mut blocks,
                    "team_supervisor_instructions",
                    team_workspace::supervisor_agent_prompt_appendix_for_pi(
                        &app,
                        wid,
                        speaker_id,
                    )?,
                );
                agent_turn_context::extend_with_block(
                    &mut blocks,
                    "team_workspace_context",
                    team_workspace::build_workspace_preface(&app, wid, speaker_id)?,
                );
            }
            agent_turn_context::extend_with_block(
                &mut blocks,
                "desktop_media_output",
                desktop_media_reply_prompt(
                    desktop_agent_home_for_runtime.as_deref(),
                    workspace_root_for_media.as_deref(),
                ),
            );
            turn_prompt = agent_turn_context::wrap_turn_context_message(&turn_prompt, &blocks);
        } else {
            turn_prompt = agent_turn_context::wrap_turn_context_message(
                &turn_prompt,
                &[agent_turn_context::TurnContextBlock::new(
                    "desktop_media_output",
                    desktop_media_reply_prompt(None, workspace_root_for_media.as_deref()),
                )
                .expect("desktop media context")],
            );
        }
        let prepared_input = prompt_attachments::prepare_prompt_input(&turn_prompt, &attachments)?;

        let attempt_result = tauri::async_runtime::spawn_blocking(move || {
        let session_stream_mutex = desktop_session_stream_mutex(&runtime_session_id);
        let _session_stream_guard = session_stream_mutex.try_lock().map_err(|_| {
            "该会话已有进行中的生成，请等待完成或先中止后再发。".to_string()
        })?;

        emit_stream_event(
            &app,
            "start",
            Some(runtime_session_id.clone()),
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
            .args([
                "--mode",
                "rpc",
                "--session",
                &session_path_string,
                "--no-skills",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut managed_runtime_prepared: Option<managed_runtime::PreparedManagedRuntime> = None;
        if let Some(provider_config) = provider_config.as_ref() {
            validate_desktop_provider_config(provider_config)?;
            let runtime_dir = pi_runtime_dir();
            fs::create_dir_all(&runtime_dir)
                .map_err(|error| format!("创建 pi 运行目录失败: {error}"))?;
            let auth_path = runtime_dir.join("auth.json");
            fs::write(&auth_path, "{}")
                .map_err(|error| format!("写入 pi auth 配置失败: {error}"))?;

            if let Some(agent_config) = agent_config.as_ref() {
                let image_runtime_config = resolve_default_image_generation_runtime(&app)?;
                managed_runtime_prepared = Some(managed_runtime::prepare_managed_runtime(
                    &pi_location.executable,
                    &runtime_dir,
                    agent_config,
                    Some(trimmed_prompt.as_str()),
                    &normalized_session_id,
                    Some(provider_config),
                    image_runtime_config.as_ref(),
                )?);
            }

            let runtime_provider_config = provider_config.clone();

            let models_path = runtime_dir.join("models.json");
            if let Some(models_config) =
                build_provider_models_config_with_input(
                    &runtime_provider_config,
                    text_only_provider_input,
                    disable_reasoning_effort,
                )
            {
                let content = serde_json::to_vec_pretty(&models_config)
                    .map_err(|error| format!("序列化 provider 配置失败: {error}"))?;
                fs::write(&models_path, content)
                    .map_err(|error| format!("写入 provider models 配置失败: {error}"))?;
            } else if models_path.exists() {
                fs::remove_file(&models_path)
                    .map_err(|error| format!("清理 provider models 配置失败: {error}"))?;
            }

            command.env("PI_CODING_AGENT_DIR", &runtime_dir);
            if let Some(runtime_root) = pi_location
                .resource_root
                .as_deref()
                .or_else(|| pi_location.executable.parent())
            {
                command.env("NINECLAW_PI_RUNTIME_ROOT", runtime_root.as_os_str());
            }

            if normalize_provider_api_format(
                &runtime_provider_config.api_format,
                runtime_provider_config.provider_id.trim(),
            ) == "anthropic"
            {
                scrub_anthropic_process_env(&mut command);
            }

            let compat_extension = if should_use_desktop_anthropic_compat_extension(&runtime_provider_config)
            {
                Some(prepare_desktop_anthropic_compat_extension(
                    &runtime_dir,
                    &pi_location.executable,
                    &runtime_provider_config,
                    text_only_provider_input,
                )?)
            } else {
                None
            };

            if let Some(compat_extension) = compat_extension.as_ref() {
                command
                    .env(&compat_extension.api_key_env, runtime_provider_config.api_key.trim())
                    .args([
                        "--extension",
                        compat_extension.path.to_string_lossy().as_ref(),
                    ]);
                dev_trace(
                    "desktop.stream",
                    format!(
                        "启用 Anthropic 兼容扩展: session={} provider={} compat_provider={} extension={}",
                        normalized_session_id,
                        runtime_provider_config.provider_id.trim(),
                        compat_extension.provider_id,
                        compat_extension.path.display(),
                    ),
                );
            }

            if let Some(prepared) = managed_runtime_prepared.as_ref() {
                command
                    .args([
                        "--extension",
                        prepared.extension_path.to_string_lossy().as_ref(),
                    ])
                    .env(
                        "NINECLAW_HARNESS_FILE",
                        prepared.harness.file_path.to_string_lossy().as_ref(),
                    )
                    .env(
                        "NINECLAW_PROXY_BASE_URL",
                        prepared.proxy_base_url.as_deref().unwrap_or_default(),
                    )
                    .env(
                        "NINECLAW_PROXY_SESSION_TOKEN",
                        prepared.session_token.as_deref().unwrap_or_default(),
                    )
                    .env("NINECLAW_SESSION_ID", normalized_session_id.as_str());
                if let Some(path) = prepared.mcp_config_path.as_ref() {
                    command.env("NINECLAW_MCP_CONFIG_FILE", path.as_os_str());
                }
            }

            let runtime_provider_id = compat_extension
                .as_ref()
                .map(|item| item.provider_id.clone())
                .unwrap_or_else(|| runtime_provider_id(runtime_provider_config.provider_id.trim()));
            if !runtime_provider_id.is_empty() {
                command.args(["--provider", &runtime_provider_id]);
            }

            if !runtime_provider_config.model.trim().is_empty() {
                command.args(["--model", runtime_provider_config.model.trim()]);
            }

            if let Some(thinking_level) =
                forced_pi_thinking_level(&runtime_provider_config, disable_reasoning_effort)
            {
                command.args(["--thinking", thinking_level]);
            }

            if !runtime_provider_config.api_key.trim().is_empty() {
                command.args(["--api-key", runtime_provider_config.api_key.trim()]);
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
        let mut selected_skill_ids: Vec<String> = Vec::new();
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
            let base_agent_prompt = agents::build_agent_system_prompt_for_session_prompt(
                agent_config,
                Some(normalized_session_id.as_str()),
                None,
            );
            if let Some(system_prompt) = base_agent_prompt {
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

            if let Some(ref wid) = workspace_id_for_stream {
                command.env("NINECLAW_WORKSPACE_ID", wid);
            }

            let skill_decision =
                crate::skill_broker::select_skills_for_turn(agent_config, trimmed_prompt.as_str(), &[])?;
            selected_skill_ids = skill_decision.mounted_skill_ids.clone();
            if !selected_skill_ids.is_empty() {
                match skills::resolve_skill_source_info(&selected_skill_ids)
                    .and_then(|info| serde_json::to_string(&info).map_err(|error| error.to_string()))
                {
                    Ok(skill_sources_json) => {
                        command.env("NINECLAW_ACTIVE_SKILL_SOURCES_JSON", skill_sources_json);
                    }
                    Err(error) => {
                        dev_trace(
                            "desktop.stream",
                            format!(
                                "runtime skill source snapshot skipped: session={} error={}",
                                normalized_session_id, error
                            ),
                        );
                    }
                }
            }
            let _ = emit_pi_stream_skill_selection_event(
                &app,
                Some(normalized_session_id.clone()),
                &skill_decision,
            );
            if let Some(skill_prompt) = crate::skill_broker::runtime_skill_prompt(&skill_decision) {
                dev_trace(
                    "desktop.stream",
                    format!(
                        "runtime_skill_selection: session={} {}",
                        normalized_session_id, skill_prompt
                    ),
                );
            }

            for skill_path in skills::resolve_skill_directories(&selected_skill_ids)? {
                skill_count += 1;
                skill_paths.push(skill_path.clone());
                let skill_path = skill_path.to_string_lossy().to_string();
                command.args(["--skill", &skill_path]);
            }
        }
        dev_trace(
            "desktop.stream",
            format!(
                "准备启动 pi: session={} prompt_chars={} system_prompt_chars={} skill_count={} provider={} model={} agent={} selected_skills={} pi_path={} images={} fresh_multimodal_session={} runtime_session={}",
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
                if selected_skill_ids.is_empty() {
                    "(none)".to_string()
                } else {
                    selected_skill_ids.join(",")
                },
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

        let mut session_text_log_guard = {
            let _ = session_llm_log::record_start(
                Some(&app),
                session_llm_log::StartLog {
                    workspace_id: workspace_id_for_stream.clone(),
                    session_id: normalized_session_id.clone(),
                    source: "desktop".to_string(),
                    channel_id: None,
                    user_id: None,
                    agent_id: agent_config.as_ref().map(|item| item.id.clone()),
                    agent_name: agent_config.as_ref().map(|item| item.name.clone()),
                    provider: provider_config.as_ref().map(|item| item.provider_id.clone()),
                    model: provider_config.as_ref().map(|item| item.model.clone()),
                    prompt: trimmed_prompt.clone(),
                    system_prompts: system_prompt_sections.clone(),
                    attachments_count: attachments.len(),
                    images_count: prepared_input.images.len(),
                    reused_process: None,
                    runtime_session_path: Some(session_path_string.clone()),
                },
            );
            session_llm_log::SessionLlmLogGuard::new(
                Some(&app),
                workspace_id_for_stream.clone(),
                normalized_session_id.clone(),
            )
        };

        // 调试模式：团队空间遵循 `llm_trace_enabled`；单独 session 则默认记录结构化 trace。
        // 系统提示词、用户输入、响应、用量最终会写入 scope 对应的 `.debug/YYYY-MM-DD.jsonl`。
        let mut pi_trace_guard: Option<llm_trace::TraceGuard> = None;
        if let Some(cfg) = agent_config.as_ref() {
            let trace_enabled = if let Some(wid) = workspace_id_for_stream.as_deref() {
                storage_conn(&app)
                    .and_then(|c| storage::workspaces::get_workspace(&c, wid))
                    .ok()
                    .flatten()
                    .map(|w| w.llm_trace_enabled != 0)
                    .unwrap_or(false)
            } else {
                true
            };
            if trace_enabled {
                let sections: Vec<llm_trace::TraceSystemPromptSection> = system_prompt_sections
                    .iter()
                    .map(|(label, content)| llm_trace::TraceSystemPromptSection {
                        label: label.clone(),
                        content: content.clone(),
                    })
                    .collect();
                let trace_id = llm_trace::begin(
                    &app,
                    workspace_id_for_stream.as_deref(),
                    "main_pi",
                    cfg.id.trim(),
                    cfg.name.trim(),
                    None,
                    None,
                    Some(&normalized_session_id),
                    provider_config.as_ref().map(|p| p.provider_id.as_str()),
                    provider_config.as_ref().map(|p| p.model.as_str()),
                    sections,
                    &trimmed_prompt,
                );
                pi_trace_guard = Some(llm_trace::TraceGuard::new(&app, trace_id));
            }
        }

        let desktop_reuse_enabled = pi_reuse_desktop_enabled() && !fresh_multimodal_session;
        let desktop_fingerprint = desktop_pi_fingerprint(
            provider_config.as_ref(),
            &session_path_string,
            managed_runtime_prepared
                .as_ref()
                .and_then(|prepared| prepared.proxy_base_url.as_deref())
                .unwrap_or_default(),
            managed_runtime_prepared
                .as_ref()
                .and_then(|prepared| prepared.session_token.as_deref())
                .unwrap_or_default(),
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
        let mut completed_pi_turns = 0u32;
        let mut stream_stall_retries = 0u32;
        let mut iteration_limit_warned = false;
        let mut iteration_abort_sent = false;

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
                if let Some(guard) = pi_trace_guard.as_mut() {
                    guard.finalize_error(timeout_error.clone());
                }
                session_text_log_guard.finish_error("error", timeout_error.clone(), None);
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
                    stream_stall_retries = 0;
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
                        cleanup_aborted_desktop_child(
                            &mut child,
                            &stdin,
                            &normalized_session_id,
                            "stream_pi_prompt:stdout_read_error_after_abort",
                        );
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
                        if let Some(guard) = pi_trace_guard.as_mut() {
                            guard.finalize_aborted("用户中止后读取 pi 输出失败".to_string());
                        }
                        session_text_log_guard.finish_error(
                            "aborted",
                            "用户中止后读取 pi 输出失败".to_string(),
                            Some(emitted_assistant_text.clone()),
                        );
                        return Ok(());
                    }
                    if let Some(guard) = pi_trace_guard.as_mut() {
                        guard.finalize_error(format!("读取 pi 输出失败: {error}"));
                    }
                    session_text_log_guard.finish_error(
                        "error",
                        format!("读取 pi 输出失败: {error}"),
                        Some(emitted_assistant_text.clone()),
                    );
                    return Err(format!("读取 pi 输出失败: {error}"));
                }
                Err(RecvTimeoutError::Timeout) => {
                    let timeout_error = if timeout == remaining_total {
                        format!("pi 总运行超时（>{} 秒）", pi_total_runtime_timeout.as_secs())
                    } else if saw_any_output {
                        format!(
                            "等待 pi 后续输出超时（>{} 秒）",
                            pi_idle_output_timeout.as_secs()
                        )
                    } else {
                        format!(
                            "等待 pi 首包输出超时（>{} 秒）",
                            pi_first_output_timeout.as_secs()
                        )
                    };
                    let stalled_waiting_for_more = timeout != remaining_total && saw_any_output;
                    if stalled_waiting_for_more && stream_stall_retries < stall_retry_cap {
                        stream_stall_retries += 1;
                        let backoff_ms =
                            (250u64 * (1u64 << stream_stall_retries.min(6))).min(7000).max(100);
                        thread::sleep(Duration::from_millis(backoff_ms));
                        dev_trace(
                            "desktop.stream",
                            format!(
                                "pi stdout stall retry {}/{} backoff_ms={} session={}",
                                stream_stall_retries,
                                stall_retry_cap,
                                backoff_ms,
                                normalized_session_id
                            ),
                        );
                        continue;
                    }
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
                    if let Some(guard) = pi_trace_guard.as_mut() {
                        guard.finalize_error(timeout_error.clone());
                    }
                    session_text_log_guard.finish_error(
                        "error",
                        timeout_error.clone(),
                        Some(emitted_assistant_text.clone()),
                    );
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
                        cleanup_aborted_desktop_child(
                            &mut child,
                            &stdin,
                            &normalized_session_id,
                            "stream_pi_prompt:json_parse_error_after_abort",
                        );
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
                        if let Some(guard) = pi_trace_guard.as_mut() {
                            guard.finalize_aborted("用户中止后解析 pi 输出失败".to_string());
                        }
                        session_text_log_guard.finish_error(
                            "aborted",
                            "用户中止后解析 pi 输出失败".to_string(),
                            Some(emitted_assistant_text.clone()),
                        );
                        return Ok(());
                    }
                    if let Some(guard) = pi_trace_guard.as_mut() {
                        guard.finalize_error(format!("解析 pi 输出失败: {error}"));
                    }
                    session_text_log_guard.finish_error(
                        "error",
                        format!("解析 pi 输出失败: {error}"),
                        Some(emitted_assistant_text.clone()),
                    );
                    return Err(format!("解析 pi 输出失败: {error}"));
                }
            };

            let line_type = value
                .get("type")
                .and_then(|item| item.as_str())
                .unwrap_or_default();

            if line_type == "turn_end" {
                completed_pi_turns += 1;
                let cap = tool_iteration_cap.max(1).min(500);
                let warn_at = cap.saturating_sub(10).max(1);
                if !iteration_limit_warned && completed_pi_turns >= warn_at && completed_pi_turns < cap {
                    iteration_limit_warned = true;
                    let _ = app.emit(
                        "nineclaw-runtime-notification",
                        serde_json::json!({
                            "kind": "agent_loop_approaching_limit",
                            "sessionId": &normalized_session_id,
                            "current": completed_pi_turns,
                            "max": cap,
                            "message": format!(
                                "工具调用轮数已接近上限（约 {}/{} 轮），请在设置 → 参数中调整。",
                                completed_pi_turns,
                                cap
                            ),
                        }),
                    );
                }
                if completed_pi_turns >= cap && !iteration_abort_sent {
                    iteration_abort_sent = true;
                    abort_requested.store(true, Ordering::SeqCst);
                    dev_trace(
                        "desktop.stream",
                        format!(
                            "agent tool iteration limit: session={} completed_pi_turns={} cap={}",
                            normalized_session_id, completed_pi_turns, cap,
                        ),
                    );
                    if let Err(error) =
                        crate::runtime_parameters::write_abort_json_stdin(&stdin, &normalized_session_id)
                    {
                        dev_trace(
                            "desktop.stream",
                            format!("iteration-limit abort write failed: {error}"),
                        );
                    }
                }
            }

            if line_type == "nineclaw_skill_evolution_usage" {
                let step_usage = extract_usage_payload(value.get("usage"));
                if step_usage.is_some() {
                    accumulate_pi_token_usage(&mut final_usage, step_usage);
                    dev_trace(
                        "desktop.stream",
                        format!(
                            "skill_evolution usage aggregated: session={} route={} cost={}",
                            normalized_session_id,
                            value
                                .get("route")
                                .and_then(|item| item.as_str())
                                .unwrap_or_default(),
                            value
                                .get("cost")
                                .and_then(|item| item.as_f64())
                                .unwrap_or_default()
                        ),
                    );
                }
                continue;
            }

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
                    if let Some(guard) = pi_trace_guard.as_mut() {
                        guard.finalize_error(error_text.clone());
                    }
                    session_text_log_guard.finish_error(
                        "error",
                        error_text.clone(),
                        Some(emitted_assistant_text.clone()),
                    );
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
                        if let Some(tid) = pi_trace_guard.as_ref().and_then(|g| g.id()) {
                            llm_trace::append_response(&app, tid, &delta_text);
                        }
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
                        if let Some(tid) = pi_trace_guard.as_ref().and_then(|g| g.id()) {
                            llm_trace::append_thinking(&app, tid, &thinking_text);
                        }
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
                            if let Some(tid) = pi_trace_guard.as_ref().and_then(|g| g.id()) {
                                llm_trace::append_response(&app, tid, &missing_text);
                            }
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
                            if let Some(tid) = pi_trace_guard.as_ref().and_then(|g| g.id()) {
                                llm_trace::append_response(&app, tid, &missing_text);
                            }
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

                if let Some(tid) = pi_trace_guard.as_ref().and_then(|g| g.id()) {
                    llm_trace::record_tool(
                        &app,
                        tid,
                        tool_call_id.as_deref().unwrap_or(""),
                        tool_name.as_deref().unwrap_or(""),
                        args_text.as_deref(),
                        None,
                        "running",
                        None,
                    );
                }
                let _ = session_llm_log::record_tool_start(
                    Some(&app),
                    session_llm_log::ToolLog {
                        workspace_id: workspace_id_for_stream.clone(),
                        session_id: normalized_session_id.clone(),
                        tool_call_id: tool_call_id.clone(),
                        tool_name: tool_name.clone(),
                        args: args_text.clone(),
                        result: None,
                        is_error: None,
                    },
                );

                managed_runtime::append_session_event_quiet(
                    desktop_agent_home.as_deref(),
                    &normalized_session_id,
                    managed_runtime::SessionEventKind::ToolCall,
                    format!(
                        "{} {}",
                        tool_name.as_deref().unwrap_or("unknown_tool"),
                        args_text.as_deref().unwrap_or("")
                    ),
                    Some(serde_json::json!({
                        "toolCallId": tool_call_id,
                        "toolName": tool_name,
                        "args": args_text,
                    })),
                );

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

                if let Some(tid) = pi_trace_guard.as_ref().and_then(|g| g.id()) {
                    let status = if is_error.unwrap_or(false) { "error" } else { "done" };
                    llm_trace::record_tool(
                        &app,
                        tid,
                        tool_call_id.as_deref().unwrap_or(""),
                        tool_name.as_deref().unwrap_or(""),
                        args_text.as_deref(),
                        result_text.as_deref(),
                        status,
                        is_error,
                    );
                }
                let _ = session_llm_log::record_tool_end(
                    Some(&app),
                    session_llm_log::ToolLog {
                        workspace_id: workspace_id_for_stream.clone(),
                        session_id: normalized_session_id.clone(),
                        tool_call_id: tool_call_id.clone(),
                        tool_name: tool_name.clone(),
                        args: args_text.clone(),
                        result: result_text.clone(),
                        is_error,
                    },
                );

                managed_runtime::append_session_event_quiet(
                    desktop_agent_home.as_deref(),
                    &normalized_session_id,
                    managed_runtime::SessionEventKind::ToolResult,
                    format!(
                        "{} {}",
                        tool_name.as_deref().unwrap_or("unknown_tool"),
                        result_text.as_deref().unwrap_or("")
                    ),
                    Some(serde_json::json!({
                        "toolCallId": tool_call_id,
                        "toolName": tool_name,
                        "args": args_text,
                        "result": result_text,
                        "isError": is_error,
                    })),
                );

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
                                if let Some(tid) = pi_trace_guard.as_ref().and_then(|g| g.id()) {
                                    llm_trace::append_response(&app, tid, &missing_text);
                                }
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
                maybe_expand_team_delegates(
                    &app,
                    workspace_id_for_stream.as_ref(),
                    &provider_config,
                    &mut emitted_assistant_text,
                );
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

                // Agent Loop: if agent has agent_loop_config, enter loop
                if let Some(ref agent_cfg) = agent_config {
                    if let Some(ref loop_config) = agent_cfg.agent_loop_config {
                        if let Some(ref provider) = provider_config {
                            let app_clone = app.clone();
                            let agent_id = agent_cfg.id.clone();
                            let sid = normalized_session_id.clone();
                            let mut lc = loop_config.clone();
                            lc.max_iterations = delegate_iteration_cap;
                            let prov = provider.clone();
                            let initial_text = emitted_assistant_text.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = crate::agent_loop::run_agent_loop(
                                    &app_clone,
                                    &agent_id,
                                    &sid,
                                    &lc,
                                    &prov,
                                    &initial_text,
                                    0,
                                )
                                .await;
                            });
                        }
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
            if let Some(guard) = pi_trace_guard.as_mut() {
                guard.finalize_done(
                    Some(emitted_assistant_text.clone()),
                    final_usage.clone(),
                    final_usage_meta.as_ref().and_then(|m| m.provider.clone()),
                    final_usage_meta.as_ref().and_then(|m| m.model.clone()),
                    final_usage_meta.as_ref().and_then(|m| m.response_id.clone()),
                );
            }
            session_text_log_guard.finish_done(
                Some(emitted_assistant_text.clone()),
                final_usage.clone(),
                final_usage_meta.as_ref().and_then(|m| m.provider.clone()),
                final_usage_meta.as_ref().and_then(|m| m.model.clone()),
                final_usage_meta.as_ref().and_then(|m| m.response_id.clone()),
            );

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
            if let Some(wid) = workspace_id_for_stream.as_deref() {
                workspace_memory_extraction::spawn_workspace_memory_extraction(
                    &app,
                    wid,
                    &normalized_session_id,
                    &trimmed_prompt,
                    &emitted_assistant_text,
                    agent_config.as_ref().map(|config| config.id.as_str()),
                    agent_config.as_ref().map(|config| config.name.as_str()),
                );
            }
            if let Some(agent_config) = agent_config.as_ref() {
                user_memory_auto_extraction::spawn_user_memory_auto_extraction(
                    &app,
                    &normalized_session_id,
                    &trimmed_prompt,
                    &emitted_assistant_text,
                    &agent_config.id,
                    &agent_config.name,
                );
            }
            managed_runtime::append_assistant_output_events(
                desktop_agent_home.as_deref(),
                &normalized_session_id,
                &emitted_assistant_text,
            );

            store_pooled_desktop_pi(
                &normalized_session_id,
                DesktopPooledPi {
                    child,
                    stdin,
                    stdout_rx,
                    stderr_buffer,
                    fingerprint: desktop_fingerprint,
                    workspace_id: workspace_id_for_stream.clone(),
                    session_path: session_path.clone(),
                    last_usage: final_usage.clone(),
                },
            )?;
            // 压缩可能再次调用模型；只在 idle 后台执行，避免主回复 done 后继续占用同会话输入通道。
            schedule_desktop_idle_compression(&app, &normalized_session_id);
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

        if !should_pool_after_turn {
            close_pi_stdin(&stdin);
        }

        let aborting_now = abort_requested.load(Ordering::SeqCst) || saw_model_abort_event;
        let exit_wait_reason = if abort_requested.load(Ordering::SeqCst) {
            close_pi_stdin(&stdin);
            kill_child_with_trace(
                &mut child,
                Some(&normalized_session_id),
                "stream_pi_prompt:user_abort_fast_cleanup",
            );
            "stream_pi_prompt:user_abort_fast_cleanup"
        } else if saw_model_abort_event {
            close_pi_stdin(&stdin);
            kill_child_with_trace(
                &mut child,
                Some(&normalized_session_id),
                "stream_pi_prompt:model_abort_fast_cleanup",
            );
            "stream_pi_prompt:model_abort_fast_cleanup"
        } else {
            "stream_pi_prompt:post_stdout_eof_wait_exit"
        };
        let exit_outcome = wait_for_child_exit_with_trace(
            &mut child,
            if aborting_now {
                DESKTOP_ABORT_WAIT_TIMEOUT
            } else {
                desktop_pi_after_stdout_eof_exit_wait()
            },
            Some(&normalized_session_id),
            exit_wait_reason,
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
            if let Some(guard) = pi_trace_guard.as_mut() {
                guard.finalize_aborted("用户中止".to_string());
            }
            session_text_log_guard.finish_error(
                "aborted",
                "用户中止".to_string(),
                Some(emitted_assistant_text.clone()),
            );
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
            if let Some(guard) = pi_trace_guard.as_mut() {
                guard.finalize_aborted("模型返回 aborted".to_string());
            }
            session_text_log_guard.finish_error(
                "aborted",
                "模型返回 aborted".to_string(),
                Some(emitted_assistant_text.clone()),
            );
            return Ok(());
        }

        let saw_any_normal_finish =
            saw_agent_end || saw_message_done || saw_assistant_terminal_message;
        if let Some(ref err) = assistant_terminal_error {
            if provider_stream_noise::is_recoverable_provider_terminal_noise(err)
                && (saw_any_normal_finish || !emitted_assistant_text.trim().is_empty())
            {
                dev_trace(
                    "desktop.stream",
                    format!(
                        "recoverable provider noise ignored (session={}): {}",
                        normalized_session_id, err
                    ),
                );
                assistant_terminal_error = None;
            }
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
            if let Some(guard) = pi_trace_guard.as_mut() {
                guard.finalize_error(error_text.clone());
            }
            session_text_log_guard.finish_error(
                "error",
                error_text.clone(),
                Some(emitted_assistant_text.clone()),
            );
            return Err(error_text);
        }

        if saw_terminal_completion {
            maybe_expand_team_delegates(
                &app,
                workspace_id_for_stream.as_ref(),
                &provider_config,
                &mut emitted_assistant_text,
            );
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

            // Agent Loop: if agent has agent_loop_config, enter loop
            if let Some(ref agent_cfg) = agent_config {
                if let Some(ref loop_config) = agent_cfg.agent_loop_config {
                    if let Some(ref provider) = provider_config {
                        let app_clone = app.clone();
                        let agent_id = agent_cfg.id.clone();
                        let sid = normalized_session_id.clone();
                        let mut lc = loop_config.clone();
                        lc.max_iterations = delegate_iteration_cap;
                        let prov = provider.clone();
                        let initial_text = emitted_assistant_text.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = crate::agent_loop::run_agent_loop(
                                &app_clone,
                                &agent_id,
                                &sid,
                                &lc,
                                &prov,
                                &initial_text,
                                0,
                            )
                            .await;
                        });
                    }
                }
            }
        }

        if saw_terminal_completion && !done_emitted {
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
            if let Some(guard) = pi_trace_guard.as_mut() {
                guard.finalize_done(
                    Some(emitted_assistant_text.clone()),
                    final_usage.clone(),
                    final_usage_meta.as_ref().and_then(|m| m.provider.clone()),
                    final_usage_meta.as_ref().and_then(|m| m.model.clone()),
                    final_usage_meta.as_ref().and_then(|m| m.response_id.clone()),
                );
            }
            session_text_log_guard.finish_done(
                Some(emitted_assistant_text.clone()),
                final_usage.clone(),
                final_usage_meta.as_ref().and_then(|m| m.provider.clone()),
                final_usage_meta.as_ref().and_then(|m| m.model.clone()),
                final_usage_meta.as_ref().and_then(|m| m.response_id.clone()),
            );
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
        if saw_terminal_completion {
            if let Some(wid) = workspace_id_for_stream.as_deref() {
                workspace_memory_extraction::spawn_workspace_memory_extraction(
                    &app,
                    wid,
                    &normalized_session_id,
                    &trimmed_prompt,
                    &emitted_assistant_text,
                    agent_config.as_ref().map(|config| config.id.as_str()),
                    agent_config.as_ref().map(|config| config.name.as_str()),
                );
            }
            if let Some(agent_config) = agent_config.as_ref() {
                user_memory_auto_extraction::spawn_user_memory_auto_extraction(
                    &app,
                    &normalized_session_id,
                    &trimmed_prompt,
                    &emitted_assistant_text,
                    &agent_config.id,
                    &agent_config.name,
                );
            }
        }
        if saw_terminal_completion {
            managed_runtime::append_assistant_output_events(
                desktop_agent_home.as_deref(),
                &normalized_session_id,
                &emitted_assistant_text,
            );
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
            if let Some(guard) = pi_trace_guard.as_mut() {
                guard.finalize_error(fallback_error.clone());
            }
            session_text_log_guard.finish_error(
                "error",
                fallback_error.clone(),
                Some(emitted_assistant_text.clone()),
            );
            return Err(fallback_error);
        }

        if status.map(|value| !value.success()).unwrap_or(true) {
            if done_emitted && saw_terminal_completion {
                session_text_log_guard.finish_done(
                    Some(emitted_assistant_text.clone()),
                    final_usage.clone(),
                    final_usage_meta.as_ref().and_then(|m| m.provider.clone()),
                    final_usage_meta.as_ref().and_then(|m| m.model.clone()),
                    final_usage_meta.as_ref().and_then(|m| m.response_id.clone()),
                );
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
                        final_usage.clone(),
                        final_usage_meta.clone(),
                    )?;
                }
                session_text_log_guard.finish_done(
                    Some(emitted_assistant_text.clone()),
                    final_usage.clone(),
                    final_usage_meta.as_ref().and_then(|m| m.provider.clone()),
                    final_usage_meta.as_ref().and_then(|m| m.model.clone()),
                    final_usage_meta.as_ref().and_then(|m| m.response_id.clone()),
                );
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
            if let Some(guard) = pi_trace_guard.as_mut() {
                guard.finalize_error(fallback_error.clone());
            }
            session_text_log_guard.finish_error(
                "error",
                fallback_error.clone(),
                Some(emitted_assistant_text.clone()),
            );
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
                final_usage.clone(),
                final_usage_meta.clone(),
            )?;
        }
        if let Some(guard) = pi_trace_guard.as_mut() {
            guard.finalize_done(
                Some(emitted_assistant_text.clone()),
                final_usage.clone(),
                final_usage_meta.as_ref().and_then(|m| m.provider.clone()),
                final_usage_meta.as_ref().and_then(|m| m.model.clone()),
                final_usage_meta.as_ref().and_then(|m| m.response_id.clone()),
            );
        }
        session_text_log_guard.finish_done(
            Some(emitted_assistant_text.clone()),
            final_usage.clone(),
            final_usage_meta.as_ref().and_then(|m| m.provider.clone()),
            final_usage_meta.as_ref().and_then(|m| m.model.clone()),
            final_usage_meta.as_ref().and_then(|m| m.response_id.clone()),
        );
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
        .map_err(|error| format!("执行任务失败: {error}"))?;

        match attempt_result {
            Ok(()) => return Ok(()),
            Err(error) => {
                managed_runtime::append_session_event_quiet(
                    desktop_agent_home_for_runtime.as_deref(),
                    &session_id_for_attempt,
                    managed_runtime::SessionEventKind::RuntimeError,
                    error.clone(),
                    Some(serde_json::json!({ "attempt": attempt + 1 })),
                );
                if should_retry_text_only_after_image_rejection(
                    attempt,
                    current_turn_has_visual_context,
                    &error,
                ) {
                    managed_runtime::append_session_event_quiet(
                        desktop_agent_home_for_runtime.as_deref(),
                        &session_id_for_attempt,
                        managed_runtime::SessionEventKind::RuntimeRetry,
                        "provider rejected image blocks; retrying once with text-only message history",
                        Some(serde_json::json!({ "attempt": attempt + 1, "error": error })),
                    );
                    text_only_provider_input_retry = true;
                    continue;
                }
                if current_turn_has_visual_context
                    && attempt == 0
                    && is_provider_image_block_rejection_error(&error)
                {
                    let clarified = format!(
                        "当前模型或 Provider 拒绝了图片输入，这次回答并没有真正看到你上传的图片。\n原始错误：{}",
                        error
                    );
                    managed_runtime::append_session_event_quiet(
                        desktop_agent_home_for_runtime.as_deref(),
                        &session_id_for_attempt,
                        managed_runtime::SessionEventKind::RuntimeError,
                        clarified.clone(),
                        Some(serde_json::json!({ "attempt": attempt + 1, "error": error })),
                    );
                    return Err(clarified);
                }
                if attempt < 2 && is_provider_reasoning_history_rejection_error(&error) {
                    let current_model_requires_reasoning_replay =
                        attempt_model_requires_reasoning_replay;
                    let session_path = session_file_path(Some(session_id_for_attempt.as_str()));
                    if !current_model_requires_reasoning_replay {
                        match sanitize_pi_session_replay_state(&session_path) {
                            Ok(true) => dev_trace(
                                "desktop.stream",
                                format!(
                                    "sanitized reasoning replay state before retry: session={} path={}",
                                    session_id_for_attempt,
                                    session_path.display()
                                ),
                            ),
                            Ok(false) => {
                                if session_path.exists() {
                                    match quarantine_pi_session_file(
                                        &session_path,
                                        "reasoning-history",
                                    ) {
                                        Ok(true) => dev_trace(
                                            "desktop.stream",
                                            format!(
                                                "quarantined pi session for fresh reasoning retry: session={} path={}",
                                                session_id_for_attempt,
                                                session_path.display()
                                            ),
                                        ),
                                        Ok(false) => {}
                                        Err(quarantine_error) => dev_trace(
                                            "desktop.stream",
                                            format!(
                                                "failed to quarantine pi session for reasoning retry: session={} path={} error={}",
                                                session_id_for_attempt,
                                                session_path.display(),
                                                quarantine_error
                                            ),
                                        ),
                                    }
                                    dev_trace(
                                        "desktop.stream",
                                        format!(
                                            "reasoning replay retry requested but session needed no sanitize: session={} path={}",
                                            session_id_for_attempt,
                                            session_path.display()
                                        ),
                                    );
                                }
                            }
                            Err(sanitize_error) => dev_trace(
                                "desktop.stream",
                                format!(
                                    "failed to sanitize reasoning replay state: session={} path={} error={}",
                                    session_id_for_attempt,
                                    session_path.display(),
                                    sanitize_error
                                ),
                            ),
                        }
                    } else {
                        dev_trace(
                            "desktop.stream",
                            format!(
                                "mimo reasoning replay retry keeps session state intact: session={} path={}",
                                session_id_for_attempt,
                                session_path.display()
                            ),
                        );
                    }
                    let retry_note = if current_model_requires_reasoning_replay {
                        "provider rejected MiMo reasoning replay; retrying with MiMo replay compatibility preserved"
                    } else {
                        "provider rejected replayed reasoning history; retrying once with reasoning effort disabled"
                    };
                    managed_runtime::append_session_event_quiet(
                        desktop_agent_home_for_runtime.as_deref(),
                        &session_id_for_attempt,
                        managed_runtime::SessionEventKind::RuntimeRetry,
                        retry_note,
                        Some(serde_json::json!({ "attempt": attempt + 1, "error": error })),
                    );
                    disable_reasoning_provider_retry = !current_model_requires_reasoning_replay;
                    attempt_prompt = if current_model_requires_reasoning_replay {
                        prompt.trim().to_string()
                    } else {
                        format!(
                            "{}\n\n[system note] 上一次请求因为旧 PI session 中的 reasoning/thinking 历史无法被当前 Provider 回放而失败。NineClaw 已修复或隔离污染的 session 历史。请基于当前可见上下文和持久记忆继续处理用户请求；如果关键上下文缺失，只问一个最必要的问题。",
                            prompt.trim()
                        )
                    };
                    continue;
                }
                if managed_runtime::should_auto_retry_runtime(
                    desktop_harness_for_retry.as_ref(),
                    attempt,
                    &error,
                ) {
                    managed_runtime::append_session_event_quiet(
                        desktop_agent_home_for_runtime.as_deref(),
                        &session_id_for_attempt,
                        managed_runtime::SessionEventKind::RuntimeRetry,
                        format!("runtime failure on attempt {}, retrying once", attempt + 1),
                        Some(serde_json::json!({ "attempt": attempt + 1, "error": error })),
                    );
                    attempt_prompt = managed_runtime::build_retry_prompt(&prompt);
                    continue;
                }
                return Err(error);
            }
        }
    }

    Err("pi 自动重试后仍未完成".to_string())
}

// ── Bot Channel Commands ──

static CHANNEL_MANAGER: OnceLock<Mutex<ChannelManager>> = OnceLock::new();

pub(crate) fn channel_manager() -> &'static Mutex<ChannelManager> {
    CHANNEL_MANAGER.get_or_init(|| Mutex::new(ChannelManager::new()))
}

pub(crate) fn lock_channel_manager() -> MutexGuard<'static, ChannelManager> {
    let manager = channel_manager();
    match manager.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("channel manager lock was poisoned; recovering shared IM channel state");
            let guard = poisoned.into_inner();
            manager.clear_poison();
            guard
        }
    }
}

#[tauri::command]
async fn sync_runtime_parameters(
    payload: runtime_parameters::RuntimeParametersPayload,
) -> Result<runtime_parameters::RuntimeParametersPayload, String> {
    Ok(runtime_parameters::merge_from_payload(Some(payload)))
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

    let mut mgr = lock_channel_manager();

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
    let mut mgr = lock_channel_manager();
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

    let mut mgr = lock_channel_manager();

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
    let mut mgr = lock_channel_manager();
    mgr.start_channel(&channel_id, app)?;

    Ok(())
}

#[tauri::command]
async fn bot_stop_wechat(channel_id: String) -> Result<(), String> {
    let mut mgr = lock_channel_manager();
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
fn load_network_proxy_settings(
    app: AppHandle,
) -> Result<proxy_settings::NetworkProxySettings, String> {
    proxy_settings::load_proxy_settings(&app)
}

#[tauri::command]
fn load_mcp_settings(app: AppHandle) -> Result<mcp_settings::McpSettings, String> {
    mcp_settings::load_mcp_settings(&app)
}

#[tauri::command]
fn save_network_proxy_settings(
    app: AppHandle,
    settings: proxy_settings::NetworkProxySettings,
) -> Result<proxy_settings::NetworkProxySettings, String> {
    proxy_settings::save_proxy_settings(&app, &settings)
}

#[tauri::command]
fn save_mcp_settings(
    app: AppHandle,
    settings: mcp_settings::McpSettings,
) -> Result<mcp_settings::McpSettings, String> {
    mcp_settings::save_mcp_settings_and_snapshot(&app, &settings)
}

#[tauri::command]
async fn test_network_proxy_connection(
    settings: proxy_settings::NetworkProxySettings,
) -> Result<String, String> {
    proxy_settings::test_proxy_connection(&settings).await
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
    let mut mgr = lock_channel_manager();
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
    let mgr = lock_channel_manager();
    let status = mgr.get_status(&channel_id)?;
    serde_json::to_string(&status).map_err(|e| format!("序列化状态失败: {e}"))
}

#[tauri::command]
async fn bot_send_message(
    channel_id: String,
    user_id: String,
    content: String,
) -> Result<(), String> {
    let mgr = lock_channel_manager();
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

    let mgr = lock_channel_manager();
    mgr.send_media(&channel_id, &user_id, &payload)
}

#[tauri::command]
async fn agent_loop_respond_review(
    loop_id: String,
    approved: bool,
    extend_to: Option<u32>,
) -> Result<(), String> {
    // TODO: Wire to ActiveLoops managed state in a follow-up
    // For now, log and return Ok
    log::info!(
        "Agent Loop 审核响应: loop_id={}, approved={}, extend_to={:?}",
        loop_id,
        approved,
        extend_to
    );
    Ok(())
}

#[tauri::command]
async fn agent_loop_abort(loop_id: String) -> Result<(), String> {
    // TODO: Wire to ActiveLoops managed state in a follow-up
    log::info!("Agent Loop 取消请求: loop_id={}", loop_id);
    Ok(())
}

#[tauri::command]
async fn agent_loop_respond_approval(
    app: tauri::AppHandle,
    loop_id: String,
    approved: bool,
) -> Result<(), String> {
    use agent_loop_types::ActiveLoops;
    let active_loops = app.state::<ActiveLoops>();
    let sender = active_loops
        .approval_pending
        .lock()
        .map_err(|e| format!("锁 poisoned: {e}"))?
        .remove(&loop_id);
    match sender {
        Some(tx) => {
            let _ = tx.send(approved);
            log::info!(
                "Agent Loop 审批响应已发送: loop_id={}, approved={}",
                loop_id,
                approved
            );
            Ok(())
        }
        None => {
            log::warn!("Agent Loop 审批响应无待处理请求: loop_id={}", loop_id);
            Err(format!("没有待处理的审批请求: loop_id={loop_id}"))
        }
    }
}

#[tauri::command]
fn macos_open_native_dictation_panel(app: tauri::AppHandle) -> Result<(), String> {
    macos_native_dictation_panel::open(&app)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let log_dir = match app.path().app_data_dir() {
                Ok(root) => root.join("logs"),
                Err(error) => {
                    eprintln!("解析应用数据目录失败，跳过文件日志: {error}");
                    PathBuf::new()
                }
            };
            if !log_dir.as_os_str().is_empty() {
                if let Err(error) = std::fs::create_dir_all(&log_dir) {
                    eprintln!("创建日志目录失败: {error}");
                } else {
                    let date_logger =
                        fern::DateBased::new(log_dir.join("nineclaw-"), "%Y-%m-%d.log");
                    let file_dispatch = fern::Dispatch::new().chain(date_logger);
                    let mut log_builder = tauri_plugin_log::Builder::new()
                        .clear_targets()
                        .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                        .max_file_size(u128::MAX)
                        .level(log::LevelFilter::Info)
                        .target(tauri_plugin_log::Target::new(
                            tauri_plugin_log::TargetKind::Dispatch(file_dispatch),
                        ))
                        .format(|out, message, record| {
                            let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                            let lvl = format!("{:<5}", record.level());
                            out.finish(format_args!(
                                "[{}] [{}] [{}] {}",
                                ts,
                                lvl,
                                record.target(),
                                message
                            ))
                        });
                    #[cfg(debug_assertions)]
                    {
                        log_builder = log_builder.target(tauri_plugin_log::Target::new(
                            tauri_plugin_log::TargetKind::Stdout,
                        ));
                    }
                    if let Err(error) = app.handle().plugin(log_builder.build()) {
                        eprintln!("初始化文件日志失败: {error}");
                    }
                }
            }

            dev_trace("app", "NineClaw 启动");
            let process_id = std::process::id();
            log::info!("NineClaw 进程启动 pid={process_id}");
            match history_app_state::history_db_diagnostics(&app.handle()) {
                Ok((db_path, session_count, turn_count)) => {
                    log::info!(
                        "历史数据库诊断: pid={} path={} sessions={} turns={}",
                        process_id,
                        db_path.display(),
                        session_count,
                        turn_count
                    );
                }
                Err(error) => {
                    log::warn!("历史数据库诊断失败: pid={} error={}", process_id, error);
                }
            }
            std::env::set_var("ORT_LOG", "warning");
            let _ = ort::init()
                .with_logger(std::sync::Arc::new(
                    |level, _category, _id, _location, message| match level {
                        ort::logging::LogLevel::Warning => log::warn!("[onnxruntime] {message}"),
                        ort::logging::LogLevel::Error | ort::logging::LogLevel::Fatal => {
                            log::error!("[onnxruntime] {message}")
                        }
                        _ => {}
                    },
                ))
                .commit();
            managed_runtime::inject_credential_proxy_app_handle(app.handle().clone());
            resize_main_window_to_screen(&app.handle());
            if let Err(error) = proxy_settings::apply_saved_proxy_settings(&app.handle()) {
                log::warn!("应用启动时载入代理设置失败: {error}");
            }
            llm_log_export::init(&app.handle());

            // Initialize embedding provider registry
            let embedding_registry = embedding::new_registry();
            managed_runtime::inject_embedding_registry(embedding_registry.clone());

            // Defer heavy runtime initialization to a background thread so the
            // window renders immediately.  The frontend listens for the
            // "pi://runtime-ready" event to know when PI features are available.
            let app_handle = app.handle().clone();
            let setup_embedding_registry = embedding_registry.clone();
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
                let post_scheduler_handle = app_handle.clone();
                scheduler::start_embedded_scheduler(app_handle);

                // --- Embedding provider init + background index rebuild ---
                let init_result = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(configure_embedding_runtime(
                        &post_scheduler_handle,
                        &setup_embedding_registry,
                    )),
                    Err(error) => Err(format!("创建 embedding runtime 失败: {error}")),
                };
                match init_result {
                    Ok(status) => {
                        if let Some(provider_id) = status.active_provider_id {
                            log::info!("embedding provider initialized: {}", provider_id);
                        } else {
                            log::warn!("embedding provider unavailable: {}", status.message);
                        }
                        maybe_start_local_model_download(
                            post_scheduler_handle.clone(),
                            setup_embedding_registry.clone(),
                        );
                    }
                    Err(error) => {
                        log::warn!("embedding runtime init failed: {error}");
                    }
                }

                // Background: rebuild vector index for memories without embeddings
                let rebuild_registry = setup_embedding_registry.clone();
                let rebuild_app = post_scheduler_handle.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let Ok(conn) = storage_conn(&rebuild_app) else {
                        return;
                    };
                    let Ok(workspaces) = storage::workspaces::list_workspaces(&conn, false) else {
                        return;
                    };

                    for ws in &workspaces {
                        let Ok(missing) =
                            memory_vector::find_memories_without_vectors(&conn, &ws.id, 50)
                        else {
                            continue;
                        };
                        if missing.is_empty() {
                            continue;
                        }
                        log::info!(
                            "Rebuilding index for workspace {}: {} memories without vectors",
                            ws.id,
                            missing.len()
                        );

                        let texts: Vec<String> = missing
                            .iter()
                            .map(|(_, title, content)| format!("{title}\n{content}"))
                            .collect();

                        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        else {
                            continue;
                        };
                        let Ok(embeddings) = rt.block_on(async {
                            let guard = rebuild_registry.read().await;
                            if let Some(provider) = guard.default_provider() {
                                provider.embed(texts).await
                            } else {
                                Err("no provider".into())
                            }
                        }) else {
                            continue;
                        };

                        for ((mid, _, _), emb) in missing.iter().zip(embeddings.iter()) {
                            let vid = uuid::Uuid::new_v4().to_string();
                            let _ = memory_vector::upsert_vector(
                                &conn,
                                &vid,
                                mid,
                                &ws.id,
                                emb,
                                "bge-small-zh-local",
                            );
                        }
                        log::info!("Indexed {} memories for workspace {}", missing.len(), ws.id);
                    }
                });
            });

            Ok(())
        })
        .manage(agent_loop_types::ActiveLoops {
            loops: std::sync::Mutex::new(std::collections::HashMap::new()),
            approval_pending: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            load_history_state,
            save_history_state,
            clear_history_state,
            chat_list_sessions,
            chat_get_session_detail,
            chat_create_session,
            chat_update_session_title,
            chat_append_turn,
            chat_update_turn,
            chat_delete_session,
            chat_clear_all_sessions,
            chat_migrate_history_v1,
            sync_history_backup_from_structured,
            list_token_usage_records,
            load_provider_preferences,
            save_provider_preferences,
            load_embedding_settings_command,
            save_embedding_settings_command,
            embedding_status_command,
            trigger_embedding_reindex_command,
            load_image_generation_preferences,
            save_image_generation_preferences,
            load_mcp_settings,
            list_installed_skills,
            list_system_skill_catalog,
            install_system_skill,
            list_agents,
            get_default_agent,
            create_agent,
            update_agent,
            rotate_agent_peer_inbound_secret,
            get_peer_gateway_info,
            save_mcp_settings,
            load_network_proxy_settings,
            load_peer_gateway_settings,
            save_network_proxy_settings,
            save_peer_gateway_settings,
            test_network_proxy_connection,
            archive_agent,
            delete_agent,
            migrate_agent_id,
            set_default_agent,
            list_default_agent_presets,
            reset_agent_to_default_preset,
            read_agent_workspace_bundle,
            read_agent_workspace_file,
            write_agent_workspace_file,
            export_agent_package,
            import_agent_package,
            list_scheduled_jobs,
            list_scheduled_job_runs,
            sync_scheduler_jobs,
            trigger_scheduler_job_now,
            get_scheduler_status,
            install_scheduler_service,
            uninstall_scheduler_service,
            list_agent_task_deliveries,
            list_agent_tasks,
            pause_agent_task,
            resume_agent_task,
            delete_agent_task,
            update_agent_task,
            run_agent_task_now,
            stream_pi_prompt,
            widget_submit_response,
            widget_cancel_response,
            workspace_list,
            workspace_create,
            workspace_update,
            workspace_default_supervisor_orchestration_prompt,
            workspace_llm_trace_status,
            workspace_llm_trace_set_enabled,
            llm_trace_list,
            llm_trace_clear,
            session_llm_log_get,
            session_llm_log_list,
            session_llm_log_clear,
            session_workspace_get,
            session_workspace_switch,
            session_workspace_reset_to_topic,
            session_workspace_list_entries,
            session_workspace_read_file,
            session_workspace_absolute_path,
            session_workspace_create_file,
            session_workspace_create_dir,
            session_workspace_rename,
            session_workspace_delete,
            session_workspace_open_path,
            session_workspace_reveal_path,
            session_workspace_import_files,
            llm_log_export_get,
            llm_log_export_set,
            llm_log_export_preview,
            app_log_list,
            app_log_read,
            app_log_open_dir,
            app_log_export_all,
            macos_open_native_dictation_panel,
            workspace_resolve_artifacts_root,
            workspace_list_artifacts_entries,
            workspace_read_artifact_text,
            workspace_artifact_absolute_path,
            workspace_set_archived,
            workspace_add_member,
            workspace_remove_member,
            workspace_list_members,
            workspace_list_resources,
            workspace_upload_resource,
            workspace_read_resource_text,
            workspace_resource_absolute_path,
            workspace_delete_resource,
            workspace_list_memories,
            workspace_kv_memory_ui_list,
            workspace_kv_memory_ui_store,
            workspace_kv_memory_ui_forget,
            workspace_kv_memory_ui_reorganize,
            workspace_write_memory,
            workspace_delete_memory,
            memory_list,
            memory_update_scope,
            memory_stats,
            memory_search_text,
            workspace_delegate,
            workspace_run_delegate_task,
            workspace_abort_delegate,
            workspace_augment_delegate,
            abort_pi_stream,
            compact_desktop_session_before_model_switch,
            persist_chat_attachments,
            open_local_file,
            open_external_url,
            open_system_notification_settings,
            send_native_notification,
            debug_send_test_notification,
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
            sync_runtime_parameters,
            test_llm_provider_connection,
            generate_session_conversation_title,
            get_session_context_stats,
            agent_loop_respond_review,
            agent_loop_abort,
            agent_loop_respond_approval
        ])
        .run(app_context())
        .expect("error while running tauri application");
}

pub fn run_scheduler_daemon() -> Result<(), String> {
    let app = tauri::Builder::default()
        .build(app_context())
        .map_err(|error| format!("初始化 scheduler daemon 失败: {error}"))?;
    let app_handle = app.handle().clone();
    let process_id = std::process::id();
    log::info!("NineClaw scheduler daemon 启动 pid={process_id}");
    match history_app_state::history_db_diagnostics(&app_handle) {
        Ok((db_path, session_count, turn_count)) => {
            log::info!(
                "scheduler daemon 历史数据库诊断: pid={} path={} sessions={} turns={}",
                process_id,
                db_path.display(),
                session_count,
                turn_count
            );
        }
        Err(error) => {
            log::warn!(
                "scheduler daemon 历史数据库诊断失败: pid={} error={}",
                process_id,
                error
            );
        }
    }

    let status = pi_runtime::ensure_runtime_dependencies_impl(&app_handle);
    if !status.pi_available {
        log::warn!(
            "scheduler daemon runtime dependency check: {}",
            status.messages.join(" | ")
        );
    }

    scheduler::run_daemon(app_handle)
}

fn app_context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}
