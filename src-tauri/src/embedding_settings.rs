use crate::embedding;
use crate::embedding::EmbeddingProvider;
use crate::history_app_state::storage_conn;
use crate::memory_vector;
use crate::proxy_settings;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Manager};

const EMBEDDING_SETTINGS_KEY: &str = "embedding_settings_v1";
const LOCAL_MODEL_DIR_NAME: &str = "embedding-models";
const LOCAL_MODEL_VERSION_DIR: &str = "bge-small-zh-v1.5";
const LOCAL_MODEL_ONNX_URL: &str =
    "https://huggingface.co/onnx-community/bge-small-zh-v1.5-ONNX/resolve/main/onnx/model.onnx";
const LOCAL_MODEL_ONNX_DATA_URL: &str =
    "https://huggingface.co/onnx-community/bge-small-zh-v1.5-ONNX/resolve/main/onnx/model.onnx_data";
const LOCAL_MODEL_TOKENIZER_URL: &str =
    "https://huggingface.co/onnx-community/bge-small-zh-v1.5-ONNX/resolve/main/tokenizer.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingSettings {
    #[serde(default = "default_embedding_mode")]
    pub mode: EmbeddingMode,
    #[serde(default)]
    pub remote_endpoint: String,
    #[serde(default)]
    pub remote_model_name: String,
    #[serde(default)]
    pub remote_api_key: String,
    #[serde(default = "default_remote_dimension")]
    pub remote_dimension: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingMode {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingProviderStatus {
    pub active_provider_id: Option<String>,
    pub mode: EmbeddingMode,
    pub local_model_ready: bool,
    pub local_model_path: String,
    pub local_download_state: LocalModelDownloadState,
    pub remote_configured: bool,
    pub vector_count: i64,
    pub provider_count: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalModelDownloadState {
    Idle,
    Downloading,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReindexResult {
    pub indexed: usize,
    pub skipped: usize,
    pub provider_id: Option<String>,
    pub search_mode_ready: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
struct DownloadStateInner {
    state: LocalModelDownloadState,
    message: String,
}

impl Default for DownloadStateInner {
    fn default() -> Self {
        Self {
            state: LocalModelDownloadState::Idle,
            message: String::new(),
        }
    }
}

fn download_state_cell() -> &'static Mutex<DownloadStateInner> {
    static CELL: OnceLock<Mutex<DownloadStateInner>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(DownloadStateInner::default()))
}

fn set_download_state(state: LocalModelDownloadState, message: impl Into<String>) {
    if let Ok(mut guard) = download_state_cell().lock() {
        guard.state = state;
        guard.message = message.into();
    }
}

fn current_download_state() -> DownloadStateInner {
    download_state_cell()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

fn default_embedding_mode() -> EmbeddingMode {
    EmbeddingMode::Local
}

fn default_remote_dimension() -> usize {
    512
}

impl Default for EmbeddingSettings {
    fn default() -> Self {
        Self {
            mode: default_embedding_mode(),
            remote_endpoint: String::new(),
            remote_model_name: String::new(),
            remote_api_key: String::new(),
            remote_dimension: default_remote_dimension(),
        }
    }
}

fn normalize_settings(mut settings: EmbeddingSettings) -> EmbeddingSettings {
    settings.remote_endpoint = settings.remote_endpoint.trim().to_string();
    settings.remote_model_name = settings.remote_model_name.trim().to_string();
    settings.remote_api_key = settings.remote_api_key.trim().to_string();
    if settings.remote_dimension == 0 {
        settings.remote_dimension = default_remote_dimension();
    }
    settings
}

fn load_settings_from_conn(conn: &Connection) -> Result<EmbeddingSettings, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![EMBEDDING_SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取 embedding 设置失败: {error}"))?;
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(EmbeddingSettings::default());
    };
    serde_json::from_str::<EmbeddingSettings>(&raw)
        .map(normalize_settings)
        .map_err(|error| format!("解析 embedding 设置失败: {error}"))
}

fn save_settings_to_conn(conn: &Connection, settings: &EmbeddingSettings) -> Result<(), String> {
    let json = serde_json::to_string(settings)
        .map_err(|error| format!("序列化 embedding 设置失败: {error}"))?;
    let now = crate::chrono_like_timestamp();
    conn.execute(
        "INSERT INTO app_state (key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![EMBEDDING_SETTINGS_KEY, json, now],
    )
    .map_err(|error| format!("保存 embedding 设置失败: {error}"))?;
    Ok(())
}

pub fn load_embedding_settings(app: &AppHandle) -> Result<EmbeddingSettings, String> {
    let conn = crate::open_history_db(app)?;
    load_settings_from_conn(&conn)
}

pub fn save_embedding_settings(
    app: &AppHandle,
    settings: &EmbeddingSettings,
) -> Result<(), String> {
    let normalized = normalize_settings(settings.clone());
    let conn = crate::open_history_db(app)?;
    save_settings_to_conn(&conn, &normalized)?;
    Ok(())
}

fn local_model_cache_root(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("读取应用数据目录失败: {error}"))?;
    Ok(app_data_dir.join(LOCAL_MODEL_DIR_NAME))
}

fn resource_model_dir(app: &AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    let bundled = resource_dir
        .join(LOCAL_MODEL_DIR_NAME)
        .join(LOCAL_MODEL_VERSION_DIR);
    bundled.exists().then_some(bundled)
}

fn cached_local_model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(local_model_cache_root(app)?.join(LOCAL_MODEL_VERSION_DIR))
}

fn local_model_dir(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    if let Some(path) = resource_model_dir(app) {
        return Ok(Some(path));
    }
    let cached = cached_local_model_dir(app)?;
    if cached.exists() {
        return Ok(Some(cached));
    }
    Ok(None)
}

fn ensure_embedding_provider_row(
    conn: &Connection,
    id: &str,
    name: &str,
    provider_type: &str,
    endpoint: Option<&str>,
    model_name: &str,
    api_key_ref: Option<&str>,
    dimension: usize,
    is_default: bool,
) -> Result<(), String> {
    conn.execute("UPDATE embedding_providers SET is_default = 0", [])
        .map_err(|error| format!("清理默认 embedding provider 失败: {error}"))?;
    conn.execute(
        "INSERT INTO embedding_providers (id, name, provider_type, endpoint, model_name, api_key_ref, dimension, is_default)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name,
           provider_type = excluded.provider_type,
           endpoint = excluded.endpoint,
           model_name = excluded.model_name,
           api_key_ref = excluded.api_key_ref,
           dimension = excluded.dimension,
           is_default = excluded.is_default",
        params![
            id,
            name,
            provider_type,
            endpoint,
            model_name,
            api_key_ref,
            dimension as i64,
            if is_default { 1 } else { 0 }
        ],
    )
    .map_err(|error| format!("保存 embedding provider 失败: {error}"))?;
    Ok(())
}

fn clear_embedding_providers(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM embedding_providers", [])
        .map_err(|error| format!("清空 embedding providers 失败: {error}"))?;
    Ok(())
}

async fn register_local_provider(
    registry: &Arc<tokio::sync::RwLock<embedding::ProviderRegistry>>,
    model_dir: &Path,
) -> Result<String, String> {
    log::info!("开始加载本地 ONNX embedding 模型: {}", model_dir.display());
    // ONNX model loading is CPU/disk intensive — offload to blocking thread
    let dir = model_dir.to_path_buf();
    let provider =
        tokio::task::spawn_blocking(move || embedding::onnx_local::OnnxLocalProvider::new(&dir))
            .await
            .map_err(|e| format!("ONNX 加载任务失败: {e}"))??;
    let provider_id = provider.id().to_string();
    log::info!("ONNX 模型加载成功: {provider_id}");
    let mut guard = registry.write().await;
    guard.register(Arc::new(provider));
    Ok(provider_id)
}

async fn register_remote_provider(
    registry: &Arc<tokio::sync::RwLock<embedding::ProviderRegistry>>,
    settings: &EmbeddingSettings,
) -> Result<String, String> {
    if settings.remote_endpoint.is_empty()
        || settings.remote_model_name.is_empty()
        || settings.remote_api_key.is_empty()
    {
        return Err("远程 embedding 配置不完整".to_string());
    }
    let provider = embedding::remote_api::RemoteApiProvider::new(
        embedding::remote_api::RemoteApiConfig {
            id: "remote-api".to_string(),
            name: "Remote Embedding API".to_string(),
            endpoint: settings.remote_endpoint.clone(),
            model_name: settings.remote_model_name.clone(),
            api_key: settings.remote_api_key.clone(),
            dimension: settings.remote_dimension,
        },
        proxy_settings::build_http_client(),
    );
    let provider_id = provider.id().to_string();
    let mut guard = registry.write().await;
    guard.register(Arc::new(provider));
    Ok(provider_id)
}

pub async fn configure_embedding_runtime(
    app: &AppHandle,
    registry: &Arc<tokio::sync::RwLock<embedding::ProviderRegistry>>,
) -> Result<EmbeddingProviderStatus, String> {
    log::info!("configure_embedding_runtime 开始");
    let settings = load_embedding_settings(app)?;
    let conn = crate::open_history_db(app)?;
    clear_embedding_providers(&conn)?;

    let mut active_provider_id = None;
    let message: String;

    match settings.mode {
        EmbeddingMode::Local => {
            log::info!("embedding 模式: local");
            if let Some(model_dir) = local_model_dir(app)? {
                log::info!("找到本地模型目录: {}", model_dir.display());
                match register_local_provider(registry, &model_dir).await {
                    Ok(provider_id) => {
                        log::info!("本地 provider 注册成功: {provider_id}");
                        ensure_embedding_provider_row(
                            &conn,
                            &provider_id,
                            "Local bge-small-zh-v1.5",
                            "onnx_local",
                            None,
                            LOCAL_MODEL_VERSION_DIR,
                            None,
                            512,
                            true,
                        )?;
                        active_provider_id = Some(provider_id);
                        message = format!("本地 embedding 模型已就绪: {}", model_dir.display());
                        set_download_state(LocalModelDownloadState::Ready, "本地模型已就绪");
                    }
                    Err(error) => {
                        log::error!("本地 provider 注册失败: {error}");
                        message = format!("本地 embedding 初始化失败: {error}");
                        set_download_state(LocalModelDownloadState::Failed, message.clone());
                    }
                }
            } else {
                log::warn!("本地模型目录未找到");
                message = "本地 embedding 模型未就绪，将在后台自动下载".to_string();
            }
        }
        EmbeddingMode::Remote => {
            log::info!("embedding 模式: remote → {}", settings.remote_endpoint);
            match register_remote_provider(registry, &settings).await {
                Ok(provider_id) => {
                    ensure_embedding_provider_row(
                        &conn,
                        &provider_id,
                        "Remote Embedding API",
                        "remote_api",
                        Some(&settings.remote_endpoint),
                        &settings.remote_model_name,
                        Some("stored_in_app_state"),
                        settings.remote_dimension,
                        true,
                    )?;
                    active_provider_id = Some(provider_id);
                    message = "远程 embedding provider 已启用".to_string();
                }
                Err(error) => {
                    message = format!("远程 embedding 初始化失败: {error}");
                }
            }
        }
    }

    log::info!(
        "configure_embedding_runtime 完成: provider={:?} msg={}",
        active_provider_id,
        message
    );
    build_provider_status(app, &conn, &settings, active_provider_id, message)
}

fn build_provider_status(
    app: &AppHandle,
    conn: &Connection,
    settings: &EmbeddingSettings,
    active_provider_id: Option<String>,
    fallback_message: String,
) -> Result<EmbeddingProviderStatus, String> {
    let local_path = cached_local_model_dir(app)
        .unwrap_or_else(|_| PathBuf::from(""))
        .display()
        .to_string();
    let local_ready = local_model_dir(app)?.is_some();
    let download_state = current_download_state();
    let vector_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_vectors", [], |row| row.get(0))
        .unwrap_or(0);
    let provider_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM embedding_providers", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);
    let remote_configured = !settings.remote_endpoint.is_empty()
        && !settings.remote_model_name.is_empty()
        && !settings.remote_api_key.is_empty();
    let message = if !download_state.message.trim().is_empty()
        && matches!(settings.mode, EmbeddingMode::Local)
        && !local_ready
    {
        download_state.message.clone()
    } else {
        fallback_message
    };

    Ok(EmbeddingProviderStatus {
        active_provider_id,
        mode: settings.mode.clone(),
        local_model_ready: local_ready,
        local_model_path: local_path,
        local_download_state: if local_ready {
            LocalModelDownloadState::Ready
        } else {
            download_state.state
        },
        remote_configured,
        vector_count,
        provider_count,
        message,
    })
}

fn download_file(
    client: &reqwest::blocking::Client,
    url: &str,
    target: &Path,
) -> Result<(), String> {
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("下载模型失败: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("下载模型失败，HTTP {}", response.status()));
    }
    let bytes = response
        .bytes()
        .map_err(|error| format!("读取模型响应失败: {error}"))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建模型缓存目录失败 {}: {error}", parent.display()))?;
    }
    fs::write(target, &bytes)
        .map_err(|error| format!("写入模型归档失败 {}: {error}", target.display()))?;
    Ok(())
}

fn ensure_local_model_downloaded(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(existing) = local_model_dir(app)? {
        return Ok(existing);
    }
    set_download_state(
        LocalModelDownloadState::Downloading,
        "正在下载本地 embedding 模型",
    );
    let cache_root = local_model_cache_root(app)?;
    let extract_root = cache_root.join(LOCAL_MODEL_VERSION_DIR);
    let client = proxy_settings::build_blocking_http_client();
    fs::create_dir_all(&extract_root)
        .map_err(|error| format!("创建本地模型目录失败 {}: {error}", extract_root.display()))?;
    download_file(
        &client,
        LOCAL_MODEL_ONNX_URL,
        &extract_root.join("model.onnx"),
    )?;
    download_file(
        &client,
        LOCAL_MODEL_ONNX_DATA_URL,
        &extract_root.join("model.onnx_data"),
    )?;
    download_file(
        &client,
        LOCAL_MODEL_TOKENIZER_URL,
        &extract_root.join("tokenizer.json"),
    )?;
    set_download_state(LocalModelDownloadState::Ready, "本地模型下载完成");
    Ok(extract_root)
}

pub fn maybe_start_local_model_download(
    app: AppHandle,
    registry: Arc<tokio::sync::RwLock<embedding::ProviderRegistry>>,
) {
    let settings = match load_embedding_settings(&app) {
        Ok(settings) => settings,
        Err(_) => return,
    };
    if settings.mode != EmbeddingMode::Local {
        return;
    }
    if let Ok(Some(_)) = local_model_dir(&app) {
        return;
    }
    let state = current_download_state();
    if state.state == LocalModelDownloadState::Downloading {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || match ensure_local_model_downloaded(&app) {
        Ok(_) => {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            let _ = rt.block_on(configure_embedding_runtime(&app, &registry));
        }
        Err(error) => {
            set_download_state(LocalModelDownloadState::Failed, error);
        }
    });
}

pub async fn reindex_all_memories(
    app: &AppHandle,
    registry: &Arc<tokio::sync::RwLock<embedding::ProviderRegistry>>,
) -> Result<ReindexResult, String> {
    let provider = {
        let guard = registry.read().await;
        guard.default_provider()
    };
    let Some(provider) = provider else {
        return Ok(ReindexResult {
            indexed: 0,
            skipped: 0,
            provider_id: None,
            search_mode_ready: false,
            message: "没有可用的 embedding provider，请先配置并等待模型加载完成".to_string(),
        });
    };

    let conn = storage_conn(app)?;
    let workspaces = crate::storage::workspaces::list_workspaces(&conn, false)?;
    let mut indexed = 0usize;
    let mut skipped = 0usize;

    for ws in &workspaces {
        let missing = memory_vector::find_memories_without_vectors(&conn, &ws.id, 500)?;
        if missing.is_empty() {
            continue;
        }
        let texts: Vec<String> = missing
            .iter()
            .map(|(_, title, content)| format!("{title}\n{content}"))
            .collect();
        let embeddings = provider
            .embed(texts)
            .await
            .map_err(|error| format!("批量重建向量失败: {error}"))?;
        for ((memory_id, _, _), embedding_vec) in missing.iter().zip(embeddings.iter()) {
            let vector_id = format!("vec_{}", uuid::Uuid::new_v4().simple());
            memory_vector::upsert_vector(
                &conn,
                &vector_id,
                memory_id,
                &ws.id,
                embedding_vec,
                provider.id(),
            )?;
            indexed += 1;
        }
    }

    let provider_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM embedding_providers", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);
    if provider_count == 0 {
        skipped += 1;
    }

    Ok(ReindexResult {
        indexed,
        skipped,
        provider_id: Some(provider.id().to_string()),
        search_mode_ready: true,
        message: format!("已重建 {indexed} 条向量索引"),
    })
}

#[tauri::command]
pub async fn load_embedding_settings_command(app: AppHandle) -> Result<EmbeddingSettings, String> {
    load_embedding_settings(&app)
}

#[tauri::command]
pub async fn save_embedding_settings_command(
    app: AppHandle,
    settings: EmbeddingSettings,
) -> Result<EmbeddingProviderStatus, String> {
    log::info!(
        "save_embedding_settings_command 开始: mode={:?}",
        settings.mode
    );
    save_embedding_settings(&app, &settings)?;
    let Some(registry) = crate::managed_runtime::get_embedding_registry() else {
        return Err("embedding registry 尚未初始化".to_string());
    };
    let status = configure_embedding_runtime(&app, &registry).await?;
    if settings.mode == EmbeddingMode::Local && !status.local_model_ready {
        maybe_start_local_model_download(app.clone(), registry.clone());
    }
    log::info!(
        "save_embedding_settings_command 完成: provider={:?}",
        status.active_provider_id
    );
    embedding_status_command_inner(&app).await
}

async fn embedding_status_command_inner(
    app: &AppHandle,
) -> Result<EmbeddingProviderStatus, String> {
    let conn = crate::open_history_db(app)?;
    let settings = load_settings_from_conn(&conn)?;
    let active_provider_id =
        if let Some(registry) = crate::managed_runtime::get_embedding_registry() {
            let guard = registry.read().await;
            guard
                .default_provider()
                .map(|provider| provider.id().to_string())
        } else {
            None
        };
    build_provider_status(app, &conn, &settings, active_provider_id, String::new())
}

#[tauri::command]
pub async fn embedding_status_command(app: AppHandle) -> Result<EmbeddingProviderStatus, String> {
    embedding_status_command_inner(&app).await
}

#[tauri::command]
pub async fn trigger_embedding_reindex_command(app: AppHandle) -> Result<ReindexResult, String> {
    let Some(registry) = crate::managed_runtime::get_embedding_registry() else {
        return Err("embedding registry 尚未初始化".to_string());
    };
    reindex_all_memories(&app, &registry).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_in_memory;

    #[test]
    fn normalizes_remote_settings() {
        let settings = normalize_settings(EmbeddingSettings {
            mode: EmbeddingMode::Remote,
            remote_endpoint: " https://example.com/v1/embeddings ".to_string(),
            remote_model_name: " text-embedding-3-small ".to_string(),
            remote_api_key: " key ".to_string(),
            remote_dimension: 0,
        });
        assert_eq!(
            settings.remote_endpoint,
            "https://example.com/v1/embeddings"
        );
        assert_eq!(settings.remote_model_name, "text-embedding-3-small");
        assert_eq!(settings.remote_api_key, "key");
        assert_eq!(settings.remote_dimension, 512);
    }

    #[test]
    fn persists_settings_in_app_state() {
        let conn = open_in_memory().expect("open memory db");
        let settings = EmbeddingSettings {
            mode: EmbeddingMode::Remote,
            remote_endpoint: "https://example.com/v1/embeddings".to_string(),
            remote_model_name: "text-embedding-3-small".to_string(),
            remote_api_key: "sk-test".to_string(),
            remote_dimension: 1536,
        };
        save_settings_to_conn(&conn, &settings).expect("save embedding settings");
        let loaded = load_settings_from_conn(&conn).expect("load embedding settings");
        assert_eq!(loaded, settings);
    }

    #[test]
    fn updates_embedding_provider_row() {
        let conn = open_in_memory().expect("open memory db");
        ensure_embedding_provider_row(
            &conn,
            "remote-api",
            "Remote Embedding API",
            "remote_api",
            Some("https://example.com/v1/embeddings"),
            "text-embedding-3-small",
            Some("stored"),
            1536,
            true,
        )
        .expect("insert provider row");

        let row: (String, String, i64) = conn
            .query_row(
                "SELECT provider_type, model_name, is_default FROM embedding_providers WHERE id = 'remote-api'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("query provider row");
        assert_eq!(row.0, "remote_api");
        assert_eq!(row.1, "text-embedding-3-small");
        assert_eq!(row.2, 1);
    }
}
