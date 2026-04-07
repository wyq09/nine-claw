mod agent_workspace;
mod agents;
mod channels;
mod peer_gateway;
mod chat_attachments;
mod dev_trace;
mod heartbeat;
mod pi_runtime;
mod pi_timeouts;
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
use std::time::{Duration, Instant};

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
            TcpStream::connect_timeout(
                &stripped.parse().ok()?,
                Duration::from_millis(500),
            )
            .ok()
        })
        .is_some();

    let mut builder = reqwest::Client::builder();
    if !proxy_available {
        builder = builder.no_proxy();
    }
    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}
use tauri::{AppHandle, Emitter, Manager, PhysicalSize, Size};

use agent_workspace::AgentWorkspaceBundle;
use agents::{AgentInput, AgentRecord, ConversationAgentConfig};
use channels::factory::ChannelConfig;
use channels::manager::ChannelManager;
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
    stdin: Arc<Mutex<ChildStdin>>,
}

static PI_RUNTIME_HANDLES: OnceLock<Mutex<HashMap<String, PiRuntimeHandle>>> = OnceLock::new();
const HISTORY_DB_FILE: &str = "nineclaw.sqlite3";
const LEGACY_HISTORY_DB_FILES: &[&str] = &["yqagent.sqlite3"];
const PI_SESSION_FILE_PREFIX: &str = "nineclaw-pi-session-";
const LEGACY_PI_SESSION_FILE_PREFIXES: &[&str] = &["yqagent-pi-session-"];
const PI_RUNTIME_DIR_NAME: &str = "nineclaw-pi-runtime";
const HISTORY_STATE_KEY: &str = "history_v1";
const PROVIDER_CONFIGS_STATE_KEY: &str = "provider_configs_v1";
const CUSTOM_PROVIDER_META_STATE_KEY: &str = "custom_provider_meta_v1";

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

fn infer_media_mime_type(path: &Path, mime_hint: Option<&str>) -> String {
    if let Some(mime_hint) = mime_hint {
        let trimmed = mime_hint.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
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

#[derive(Clone, Serialize)]
struct PiTokenUsagePayload {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    cache_write_tokens: Option<u64>,
    total_tokens: Option<u64>,
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

const CHILD_KILL_GRACE_TIMEOUT: Duration = Duration::from_secs(1);

fn spawn_pi_stdout_logger<R>(
    reader: R,
    scope: &'static str,
    tx: mpsc::Sender<Result<String, String>>,
) where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
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
    });
}

fn spawn_pi_stderr_logger<R>(reader: R, scope: &'static str, buffer: Arc<Mutex<String>>)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
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
    });
}

fn runtime_handle_store() -> &'static Mutex<HashMap<String, PiRuntimeHandle>> {
    PI_RUNTIME_HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
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

fn wait_for_child_exit(child: &mut Child, timeout: Duration) -> Result<ChildExitOutcome, String> {
    let started_at = Instant::now();

    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("等待 pi 进程状态失败: {error}"))?
        {
            return Ok(ChildExitOutcome {
                status: Some(status),
                timed_out: false,
            });
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let kill_started_at = Instant::now();
            while kill_started_at.elapsed() < CHILD_KILL_GRACE_TIMEOUT {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|error| format!("等待被终止的 pi 进程失败: {error}"))?
                {
                    return Ok(ChildExitOutcome {
                        status: Some(status),
                        timed_out: true,
                    });
                }
                thread::sleep(Duration::from_millis(25));
            }
            return Ok(ChildExitOutcome {
                status: None,
                timed_out: true,
            });
        }

        thread::sleep(Duration::from_millis(25));
    }
}

fn hash_session_id(session_id: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(session_id.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn session_file_path(session_id: Option<&str>) -> PathBuf {
    let key = session_id
        .filter(|value| !value.trim().is_empty())
        .map(hash_session_id)
        .unwrap_or_else(|| "default".to_string());
    std::env::temp_dir().join(format!("{PI_SESSION_FILE_PREFIX}{key}.jsonl"))
}

fn session_cleanup_paths(session_id: Option<&str>) -> Vec<PathBuf> {
    let key = session_id
        .filter(|value| !value.trim().is_empty())
        .map(hash_session_id)
        .unwrap_or_else(|| "default".to_string());

    let mut paths = vec![std::env::temp_dir().join(format!("{PI_SESSION_FILE_PREFIX}{key}.jsonl"))];
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

pub(crate) fn open_history_db(app: &tauri::AppHandle) -> Result<Connection, String> {
    let db_path = history_db_path(app)?;
    let connection =
        Connection::open(db_path).map_err(|error| format!("打开历史数据库失败: {error}"))?;

    ensure_app_state_schema(&connection)?;

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
    let connection = open_history_db(&app)?;
    let updated_at = chrono_like_timestamp();

    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![HISTORY_STATE_KEY, payload, updated_at],
        )
        .map_err(|error| format!("保存历史任务失败: {error}"))?;

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
                    "api": "anthropic-messages"
                  }
                ]),
            );
        }
        _ => {
            provider.insert("api".to_string(), json!("openai-completions"));
            provider.insert(
                "compat".to_string(),
                json!({
                  "supportsDeveloperRole": false,
                  "supportsReasoningEffort": false
                }),
            );
            provider.insert(
                "models".to_string(),
                json!([
                  {
                    "id": model,
                    "api": "openai-completions"
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

fn extract_usage_payload(value: Option<&serde_json::Value>) -> Option<PiTokenUsagePayload> {
    let usage = value?;
    let input_tokens = extract_json_u64(usage.get("input").or_else(|| usage.get("input_tokens")));
    let output_tokens =
        extract_json_u64(usage.get("output").or_else(|| usage.get("output_tokens")));
    let cache_read_tokens = extract_json_u64(
        usage
            .get("cacheRead")
            .or_else(|| usage.get("cache_read_tokens")),
    );
    let cache_write_tokens = extract_json_u64(
        usage
            .get("cacheWrite")
            .or_else(|| usage.get("cache_write_tokens")),
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
        return Ok(());
    };

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
        writeln!(stdin, "{abort_command}")
            .map_err(|error| format!("发送 abort 指令失败: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("刷新 abort 指令失败: {error}"))?;
        true
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
) -> Result<(), String> {
    let trimmed_prompt = prompt.trim().to_string();
    if trimmed_prompt.is_empty() {
        return Err("prompt 不能为空".to_string());
    }

    let normalized_session_id = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "default".to_string());

    tauri::async_runtime::spawn_blocking(move || {
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

        let session_path = session_file_path(Some(normalized_session_id.as_str()));
        let session_path_string = session_path.to_string_lossy().to_string();
        let resolved_pi_path = pi_runtime::resolve_pi_executable(&app)
            .map(|location| location.executable.display().to_string())
            .unwrap_or_else(|| "(unresolved)".to_string());
        let mut command = pi_runtime::create_pi_command(&app)?;
        command
            .args(["--mode", "rpc", "--session", &session_path_string])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(provider_config) = provider_config.as_ref() {
            let runtime_dir = prepare_pi_runtime_dir(provider_config)?;
            command.env("PI_CODING_AGENT_DIR", runtime_dir);

            if normalize_provider_api_format(
                &provider_config.api_format,
                provider_config.provider_id.trim(),
            ) == "anthropic"
            {
                scrub_anthropic_process_env(&mut command);
            }

            let runtime_provider_id = runtime_provider_id(provider_config.provider_id.trim());
            if !runtime_provider_id.is_empty() {
                command.args(["--provider", &runtime_provider_id]);
            }

            if !provider_config.model.trim().is_empty() {
                command.args(["--model", provider_config.model.trim()]);
            }

            if !provider_config.api_key.trim().is_empty() {
                command.args(["--api-key", provider_config.api_key.trim()]);
            }
        }

        let mut system_prompt_chars = 0usize;
        let mut system_prompt_sections: Vec<(String, String)> = Vec::new();
        let mut skill_count = 0usize;
        if let Some(agent_config) = agent_config.as_ref() {
            if let Ok(workspace_root) = agent_workspace::resolve_workspace_root() {
                let agent_home = workspace_root.join("agents").join(&agent_config.id);
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
                let skill_path = skill_path.to_string_lossy().to_string();
                command.args(["--skill", &skill_path]);
            }
        }

        dev_trace(
            "desktop.stream",
            format!(
                "准备启动 pi: session={} prompt_chars={} system_prompt_chars={} skill_count={} provider={} model={} agent={} pi_path={}",
                normalized_session_id,
                trimmed_prompt.chars().count(),
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

        let mut child = command
            .spawn()
            .map_err(|error| format!("调用 pi 失败，请确认已安装并在 PATH 中: {error}"))?;
        dev_trace(
            "desktop.stream",
            format!("pi 已启动: session={} pid={}", normalized_session_id, child.id()),
        );

        let abort_requested = Arc::new(AtomicBool::new(false));

        {
            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| "无法获取 pi stdin".to_string())?;
            let stdin = Arc::new(Mutex::new(stdin));

            insert_runtime_handle(
                &normalized_session_id,
                PiRuntimeHandle {
                    abort_requested: abort_requested.clone(),
                    pid: child.id(),
                    stdin: stdin.clone(),
                },
            )?;

            let prompt_command = json!({
              "id": format!("prompt-{}", normalized_session_id),
              "type": "prompt",
              "message": trimmed_prompt,
            })
            .to_string();

            {
                let mut stdin_guard = stdin
                    .lock()
                    .map_err(|error| format!("无法锁定 prompt stdin: {error}"))?;
                writeln!(stdin_guard, "{prompt_command}")
                    .map_err(|error| format!("写入 prompt 失败: {error}"))?;
                stdin_guard
                    .flush()
                    .map_err(|error| format!("刷新 stdin 失败: {error}"))?;
            }
        }

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

        let mut saw_agent_end = false;
        let mut saw_message_done = false;
        let mut saw_model_abort_event = false;
        let mut done_emitted = false;
        let mut final_usage: Option<PiTokenUsagePayload> = None;
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
                let _ = child.kill();
                let _ = wait_for_child_exit(&mut child, CHILD_KILL_GRACE_TIMEOUT);
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
                    let _ = child.kill();
                    let _ = wait_for_child_exit(&mut child, CHILD_KILL_GRACE_TIMEOUT);
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
                    let final_text =
                        extract_text_content(assistant_event.and_then(|item| item.get("message")));
                    if let Some(snapshot) = final_text {
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
                    if let Some(snapshot) = extract_text_content(message) {
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

                    final_usage = final_usage.or_else(|| {
                        extract_usage_payload(message.and_then(|item| item.get("usage")))
                    });
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

                        final_usage = final_usage.or_else(|| {
                            extract_usage_payload(last_assistant.get("usage"))
                        });
                    }
                }

                saw_agent_end = true;
                if assistant_terminal_error.is_none() {
                    emit_stream_event(
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
                    )?;
                    done_emitted = true;
                }
                break;
            }
        }

        let exit_outcome = wait_for_child_exit(&mut child, Duration::from_secs(5))?;
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

        if let Some(agent_config) = agent_config.as_ref() {
            if (saw_agent_end || saw_message_done) && !emitted_assistant_text.trim().is_empty() {
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

        if !saw_agent_end && !saw_message_done {
            let fallback_error = if !stderr_text.trim().is_empty() {
                stderr_text.trim().to_string()
            } else {
                "pi 未返回 agent_end 事件".to_string()
            };
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
            if done_emitted && (saw_agent_end || saw_message_done) {
                return Ok(());
            }

            if exit_outcome.timed_out
                && (saw_agent_end || saw_message_done)
                && stderr_text.trim().is_empty()
            {
                if !done_emitted {
                    emit_stream_event(
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
                    )?;
                }
                return Ok(());
            }

            let fallback_error = if !stderr_text.trim().is_empty() {
                stderr_text.trim().to_string()
            } else if exit_outcome.timed_out && (saw_agent_end || saw_message_done) {
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
            emit_stream_event(
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
    Ok(pi_runtime::ensure_runtime_dependencies_impl(&app))
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

fn auto_start_bound_im_services(app: &AppHandle) -> Result<(), String> {
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

            if let Err(error) = resolve_im_llm_runtime(
                app,
                &agent.default_provider_id,
                &agent.default_model,
            ) {
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
        .setup(|app| {
            dev_trace("app", "NineClaw 启动");
            resize_main_window_to_screen(&app.handle());
            let status = pi_runtime::ensure_runtime_dependencies_impl(&app.handle());
            if !status.pi_available {
                log::warn!("runtime dependency check: {}", status.messages.join(" | "));
            }
            if let Err(error) = auto_start_bound_im_services(&app.handle()) {
                log::warn!("应用启动时自动检测 IM 机器人绑定失败: {}", error);
            }
            match agents::backfill_peer_inbound_secrets(&app.handle()) {
                Ok(count) if count > 0 => {
                    log::info!("已为 {count} 个智能体补全对等入站独立密钥");
                }
                Ok(_) => {}
                Err(error) => {
                    log::warn!("对等入站密钥补全未执行: {error}");
                }
            }
            if let Err(error) = peer_gateway::restart_peer_gateway(&app.handle()) {
                log::warn!("对等网关启动: {error}");
            }
            heartbeat::start_heartbeat_scheduler(app.handle().clone());

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
            stream_pi_prompt,
            abort_pi_stream,
            persist_chat_attachments,
            open_local_file,
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
