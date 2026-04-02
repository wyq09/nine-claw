mod agent_workspace;
mod agents;
mod channels;
mod skills;

use md5::{Digest, Md5};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalSize, Size};

use agent_workspace::AgentWorkspaceBundle;
use agents::{AgentInput, AgentRecord, ConversationAgentConfig};
use channels::factory::ChannelConfig;
use channels::manager::ChannelManager;
use channels::types::{MediaPayload, MediaType};
use channels::wechat::WeChatChannel;
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeDependencyStatus {
    platform: String,
    node_available: bool,
    npm_available: bool,
    pi_available: bool,
    auto_install_attempted: bool,
    auto_install_succeeded: bool,
    messages: Vec<String>,
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

struct ChildExitOutcome {
    status: ExitStatus,
    timed_out: bool,
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
                status,
                timed_out: false,
            });
        }

        if started_at.elapsed() >= timeout {
            child
                .kill()
                .map_err(|error| format!("终止未退出的 pi 进程失败: {error}"))?;
            let status = child
                .wait()
                .map_err(|error| format!("等待被终止的 pi 进程失败: {error}"))?;
            return Ok(ChildExitOutcome {
                status,
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

fn default_provider_base_url(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "openai" => Some("https://api.openai.com/v1"),
        "anthropic" => Some("https://api.anthropic.com"),
        "deepseek" => Some("https://api.deepseek.com"),
        "doubao" => Some("https://ark.cn-beijing.volces.com/api/v3"),
        "siliconflow" => Some("https://api.siliconflow.cn/v1"),
        _ => None,
    }
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

fn normalized_provider_runtime_base_url(
    base_url: &str,
    api_format: &str,
    provider_id: &str,
) -> String {
    match normalize_provider_api_format(api_format, provider_id) {
        "anthropic" => normalize_anthropic_base_url(base_url),
        _ => normalize_provider_base_url(base_url).to_string(),
    }
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
    let api_format = normalize_provider_api_format(&provider_config.api_format, provider_id);

    if provider_id.is_empty() || base_url.is_empty() || model.is_empty() {
        return None;
    }

    match (provider_id, api_format) {
        ("openai", "openai") => {
            let Some(default_base_url) = default_provider_base_url(provider_id) else {
                return None;
            };

            if base_url == normalize_provider_base_url(default_base_url) {
                return None;
            }

            let provider = custom_provider_object(provider_config);
            let mut providers = serde_json::Map::new();
            providers.insert(provider_id.to_string(), serde_json::Value::Object(provider));

            Some(json!({ "providers": providers }))
        }
        ("anthropic", "anthropic") => {
            let Some(default_base_url) = default_provider_base_url(provider_id) else {
                return None;
            };

            if base_url == normalize_anthropic_base_url(default_base_url) {
                return None;
            }

            let provider = custom_provider_object(provider_config);

            let mut providers = serde_json::Map::new();
            providers.insert(provider_id.to_string(), serde_json::Value::Object(provider));

            Some(json!({ "providers": providers }))
        }
        _ => {
            let provider = custom_provider_object(provider_config);
            let mut providers = serde_json::Map::new();
            providers.insert(provider_id.to_string(), serde_json::Value::Object(provider));
            Some(json!({ "providers": providers }))
        }
    }
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
        let mut command = Command::new("pi");
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

            if !provider_config.provider_id.trim().is_empty() {
                command.args(["--provider", provider_config.provider_id.trim()]);
            }

            if !provider_config.model.trim().is_empty() {
                command.args(["--model", provider_config.model.trim()]);
            }

            if !provider_config.api_key.trim().is_empty() {
                command.args(["--api-key", provider_config.api_key.trim()]);
            }
        }

        if let Some(agent_config) = agent_config.as_ref() {
            if let Some(system_prompt) = agents::build_agent_system_prompt(agent_config) {
                command.args(["--append-system-prompt", &system_prompt]);
            }

            for skill_path in skills::resolve_skill_directories(&agent_config.skill_ids)? {
                let skill_path = skill_path.to_string_lossy().to_string();
                command.args(["--skill", &skill_path]);
            }
        }

        let mut child = command
            .spawn()
            .map_err(|error| format!("调用 pi 失败，请确认已安装并在 PATH 中: {error}"))?;

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
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| "无法读取 pi 错误输出".to_string())?;

        let mut saw_agent_end = false;
        let mut saw_message_done = false;
        let mut saw_model_abort_event = false;
        let mut final_usage: Option<PiTokenUsagePayload> = None;
        let mut emitted_assistant_text = String::new();

        for line_result in BufReader::new(stdout).lines() {
            let line = match line_result {
                Ok(current_line) => current_line,
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
                    return Err(format!("读取 pi 输出失败: {error}"));
                }
            };

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
                    if let Some(final_text) = final_text {
                        let missing_text = if emitted_assistant_text.is_empty() {
                            final_text
                        } else if let Some(suffix) =
                            final_text.strip_prefix(&emitted_assistant_text)
                        {
                            suffix.to_string()
                        } else if final_text != emitted_assistant_text {
                            final_text
                        } else {
                            String::new()
                        };

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
                saw_agent_end = true;
                break;
            }
        }

        let exit_outcome = wait_for_child_exit(&mut child, Duration::from_secs(5))?;
        let status = exit_outcome.status;

        remove_runtime_handle(&normalized_session_id)?;

        let mut stderr_text = String::new();
        stderr
            .read_to_string(&mut stderr_text)
            .map_err(|error| format!("读取 pi 错误输出失败: {error}"))?;

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

        if !saw_agent_end && !saw_message_done {
            let fallback_error = if !stderr_text.trim().is_empty() {
                stderr_text.trim().to_string()
            } else {
                "pi 未返回 agent_end 事件".to_string()
            };
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

        if !status.success() {
            if exit_outcome.timed_out
                && (saw_agent_end || saw_message_done)
                && stderr_text.trim().is_empty()
            {
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
                return Ok(());
            }

            let fallback_error = if !stderr_text.trim().is_empty() {
                stderr_text.trim().to_string()
            } else if exit_outcome.timed_out && (saw_agent_end || saw_message_done) {
                "pi 在返回完整结果后退出过慢，运行时已强制回收进程。".to_string()
            } else {
                format!("pi 退出码异常: {status}")
            };
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
        Ok(())
    })
    .await
    .map_err(|error| format!("执行任务失败: {error}"))?
}

// ── Bot Channel Commands ──

static CHANNEL_MANAGER: OnceLock<Mutex<ChannelManager>> = OnceLock::new();

fn channel_manager() -> &'static Mutex<ChannelManager> {
    CHANNEL_MANAGER.get_or_init(|| Mutex::new(ChannelManager::new()))
}

fn resolve_command_path(candidates: &[&str]) -> Option<PathBuf> {
    let resolver = if cfg!(target_os = "windows") {
        ("where", "/")
    } else {
        ("which", "")
    };

    for candidate in candidates {
        let output = Command::new(resolver.0).arg(candidate).output().ok()?;
        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(first_line) = stdout.lines().map(str::trim).find(|line| !line.is_empty()) {
            return Some(PathBuf::from(first_line));
        }
    }

    None
}

fn prepend_to_path(path: &Path) {
    let Some(path_str) = path.to_str() else {
        return;
    };

    let current = env::var_os("PATH").unwrap_or_default();
    let already_present = env::split_paths(&current).any(|entry| entry == path);
    if already_present {
        return;
    }

    let mut updated = vec![path.to_path_buf()];
    updated.extend(env::split_paths(&current));
    if let Ok(joined) = env::join_paths(updated) {
        env::set_var("PATH", joined);
    } else {
        let mut fallback = path_str.to_string();
        if !current.is_empty() {
            fallback.push(if cfg!(target_os = "windows") {
                ';'
            } else {
                ':'
            });
            fallback.push_str(&current.to_string_lossy());
        }
        env::set_var("PATH", fallback);
    }
}

fn windows_common_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(app_data) = env::var_os("APPDATA") {
        dirs.push(PathBuf::from(app_data).join("npm"));
    }
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        dirs.push(
            PathBuf::from(local_app_data)
                .join("Microsoft")
                .join("WinGet")
                .join("Links"),
        );
    }
    dirs.push(PathBuf::from(r"C:\Program Files\nodejs"));
    dirs.push(PathBuf::from(r"C:\Program Files (x86)\nodejs"));

    dirs
}

fn prime_runtime_path_for_platform() {
    if cfg!(target_os = "windows") {
        for dir in windows_common_bin_dirs() {
            if dir.exists() {
                prepend_to_path(&dir);
            }
        }
    }
}

fn install_nodejs_with_winget(messages: &mut Vec<String>) -> bool {
    let winget = resolve_command_path(&["winget"]);
    let Some(winget_path) = winget else {
        messages.push("未找到 winget，无法自动安装 Node.js。".to_string());
        return false;
    };

    messages.push("检测到缺少 npm，尝试通过 winget 安装 Node.js LTS。".to_string());
    match Command::new(winget_path)
        .args([
            "install",
            "--id",
            "OpenJS.NodeJS.LTS",
            "-e",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ])
        .status()
    {
        Ok(status) if status.success() => {
            messages.push("Node.js LTS 安装完成，正在刷新 PATH。".to_string());
            prime_runtime_path_for_platform();
            true
        }
        Ok(status) => {
            messages.push(format!("winget 安装 Node.js 失败，退出码: {status}"));
            false
        }
        Err(error) => {
            messages.push(format!("执行 winget 安装 Node.js 失败: {error}"));
            false
        }
    }
}

fn install_pi_with_npm(messages: &mut Vec<String>) -> bool {
    let npm = resolve_command_path(&["npm.cmd", "npm"]);
    let Some(npm_path) = npm else {
        messages.push("未找到 npm，无法自动安装 pi。".to_string());
        return false;
    };

    messages.push("尝试通过 npm 全局安装 pi 运行时。".to_string());
    match Command::new(npm_path)
        .args(["install", "-g", "@mariozechner/pi-coding-agent"])
        .status()
    {
        Ok(status) if status.success() => {
            prime_runtime_path_for_platform();
            true
        }
        Ok(status) => {
            messages.push(format!("npm 安装 pi 失败，退出码: {status}"));
            false
        }
        Err(error) => {
            messages.push(format!("执行 npm 安装 pi 失败: {error}"));
            false
        }
    }
}

fn ensure_runtime_dependencies_impl() -> RuntimeDependencyStatus {
    prime_runtime_path_for_platform();

    let platform = env::consts::OS.to_string();
    let mut messages = Vec::new();
    let mut node_available = resolve_command_path(&["node.exe", "node"]).is_some();
    let mut npm_available = resolve_command_path(&["npm.cmd", "npm"]).is_some();
    let mut pi_available = resolve_command_path(&["pi.cmd", "pi.exe", "pi"]).is_some();
    let mut auto_install_attempted = false;
    let mut auto_install_succeeded = false;

    if cfg!(target_os = "windows") && !pi_available {
        auto_install_attempted = true;

        if !npm_available && !install_nodejs_with_winget(&mut messages) {
            messages.push("自动安装中止：Node.js/npm 仍不可用。".to_string());
        }

        node_available = resolve_command_path(&["node.exe", "node"]).is_some();
        npm_available = resolve_command_path(&["npm.cmd", "npm"]).is_some();

        if npm_available {
            let _ = install_pi_with_npm(&mut messages);
        }

        prime_runtime_path_for_platform();
        pi_available = resolve_command_path(&["pi.cmd", "pi.exe", "pi"]).is_some();
        auto_install_succeeded = pi_available;

        if pi_available {
            messages.push("pi 运行时已就绪。".to_string());
        } else {
            messages
                .push("pi 仍不可用。请确认系统允许执行 winget / npm，并重新启动应用。".to_string());
        }
    } else if pi_available {
        messages.push("pi 运行时已就绪。".to_string());
    } else {
        messages.push("当前平台未检测到 pi。".to_string());
    }

    RuntimeDependencyStatus {
        platform,
        node_available,
        npm_available,
        pi_available,
        auto_install_attempted,
        auto_install_succeeded,
        messages,
    }
}

#[tauri::command]
async fn ensure_runtime_dependencies() -> Result<RuntimeDependencyStatus, String> {
    Ok(ensure_runtime_dependencies_impl())
}

#[tauri::command]
async fn bot_login_wechat(
    app: AppHandle,
) -> Result<channels::wechat::types::WechatLoginResult, String> {
    let app_clone = app.clone();
    // login_with_qr uses block_on_async (dedicated runtime) internally,
    // so we must run it on a blocking-capable thread.
    let handle = tauri::async_runtime::spawn_blocking(move || {
        let channel = WeChatChannel::new("", "", None);
        channel.login_with_qr(&app_clone)
    });
    handle.await.map_err(|e| format!("登录任务执行失败: {e}"))?
}

#[tauri::command]
async fn bot_start_wechat(
    app: AppHandle,
    token: String,
    base_url: Option<String>,
    route_tag: Option<String>,
    provider_id: Option<String>,
    provider_api_format: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    provider_base_url: Option<String>,
) -> Result<(), String> {
    let mut mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;

    // Register WeChat channel via factory
    mgr.register_channel(ChannelConfig::WeChat {
        token,
        base_url: base_url.unwrap_or_default(),
        route_tag,
        ai_provider_id: provider_id.unwrap_or_default(),
        ai_api_format: provider_api_format.unwrap_or_else(|| "openai".to_string()),
        ai_base_url: provider_base_url.unwrap_or_default(),
        ai_api_key: api_key.unwrap_or_default(),
        ai_model: model.unwrap_or_default(),
    })?;
    mgr.start_channel("wechat", app)?;

    Ok(())
}

#[tauri::command]
async fn bot_stop_wechat() -> Result<(), String> {
    let mut mgr = channel_manager()
        .lock()
        .map_err(|e| format!("锁失败: {e}"))?;
    mgr.stop_channel("wechat")
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

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| format!("创建 HTTP 客户端失败: {error}"))?;

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
                request = request.header("x-api-key", api_key.trim());
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
            resize_main_window_to_screen(&app.handle());
            let status = ensure_runtime_dependencies_impl();
            if !status.pi_available {
                log::warn!("runtime dependency check: {}", status.messages.join(" | "));
            }

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
            list_installed_skills,
            list_system_skill_catalog,
            install_system_skill,
            list_agents,
            get_default_agent,
            create_agent,
            update_agent,
            archive_agent,
            set_default_agent,
            read_agent_workspace_bundle,
            write_agent_workspace_file,
            stream_pi_prompt,
            abort_pi_stream,
            clear_pi_session,
            clear_pi_session_for_id,
            bot_login_wechat,
            bot_start_wechat,
            bot_stop_wechat,
            bot_get_status,
            bot_send_message,
            bot_send_media,
            ensure_runtime_dependencies,
            test_llm_provider_connection
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
