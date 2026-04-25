use crate::agents::ConversationAgentConfig;
use crate::image_generation::{
    dispatch_image_generation, ImageGenerateProxyRequest, ImageGenerationRuntimeConfig,
};
use crate::provider_runtime::{
    normalize_provider_api_format, normalized_provider_runtime_base_url, ProviderRuntimeConfig,
};
use axum::body::{Body, Bytes};
use axum::extract::{OriginalUri, Path as AxumPath, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, Response, StatusCode};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const SESSIONS_DIR: &str = "memory/sessions";
const HARNESS_DIR: &str = "harness";
const HARNESS_DEFAULT_FILE: &str = "harness/default.json";
const HARNESS_CHAT_FILE: &str = "harness/chat.json";
const HARNESS_CODE_FILE: &str = "harness/code.json";
const HARNESS_CREDENTIALS_FILE: &str = "harness/credentials.json";
const SESSION_CONTEXT_CHAR_LIMIT: usize = 520;
const DEFAULT_SESSION_REPLAY_LIMIT: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventKind {
    Prompt,
    ToolCall,
    ToolResult,
    AssistantOutput,
    Decision,
    RuntimeError,
    RuntimeRetry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEventRecord {
    pub id: String,
    pub session_id: String,
    pub kind: SessionEventKind,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessDefinition {
    #[serde(default = "default_harness_name")]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub prompt_append: String,
    #[serde(default)]
    pub active_tools: Vec<String>,
    #[serde(default = "default_session_replay_limit")]
    pub session_replay_limit: usize,
    #[serde(default)]
    pub enable_external_api_proxy: bool,
    #[serde(default = "default_true")]
    pub auto_retry_on_runtime_failure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HarnessCredentialsConfig {
    #[serde(default)]
    pub external_apis: Vec<ExternalApiCredential>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalApiCredential {
    pub alias: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_auth_header_name")]
    pub auth_header: String,
    #[serde(default = "default_auth_scheme")]
    pub auth_scheme: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SelectedHarness {
    pub file_path: PathBuf,
    pub definition: HarnessDefinition,
}

#[derive(Debug, Clone)]
pub struct PreparedManagedRuntime {
    pub extension_path: PathBuf,
    pub harness: SelectedHarness,
    pub llm_proxy: Option<LlmProxyBinding>,
    pub image_proxy: Option<ImageGenerationRuntimeConfig>,
    pub proxy_base_url: Option<String>,
    pub session_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LlmProxyBinding {
    pub provider_id: String,
    pub api_format: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone)]
struct ProxySessionConfig {
    llm: Option<LlmProxyBinding>,
    image: Option<ImageGenerationRuntimeConfig>,
    external_apis: HashMap<String, ExternalApiCredential>,
}

#[derive(Clone)]
struct CredentialProxyState {
    sessions: Arc<Mutex<HashMap<String, ProxySessionConfig>>>,
    client: reqwest::Client,
}

struct CredentialProxyServer {
    base_url: String,
    state: CredentialProxyState,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExternalDispatchRequest {
    alias: String,
    method: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    query: HashMap<String, String>,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExternalDispatchResponse {
    status: u16,
    body_text: String,
    headers: HashMap<String, String>,
}

fn default_true() -> bool {
    true
}

fn default_harness_name() -> String {
    "default".to_string()
}

fn default_session_replay_limit() -> usize {
    DEFAULT_SESSION_REPLAY_LIMIT
}

fn default_auth_header_name() -> String {
    "authorization".to_string()
}

fn default_auth_scheme() -> String {
    "Bearer".to_string()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn stable_session_token(agent_id: &str, session_id: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(agent_id.trim().as_bytes());
    hasher.update([0]);
    hasher.update(session_id.trim().as_bytes());
    format!("nc_{}", format!("{:x}", hasher.finalize()))
}

pub fn ensure_agent_runtime_scaffold(agent_home: &Path) -> Result<(), String> {
    fs::create_dir_all(agent_home.join(SESSIONS_DIR))
        .map_err(|error| format!("创建 session log 目录失败: {error}"))?;
    fs::create_dir_all(agent_home.join(HARNESS_DIR))
        .map_err(|error| format!("创建 harness 目录失败: {error}"))?;

    for (relative_path, fallback) in [
        (
            PathBuf::from(HARNESS_DEFAULT_FILE),
            default_harness_file(
                "default",
                "通用执行 harness，适合没有明显代码修改目标的对话。",
                &[],
            ),
        ),
        (
            PathBuf::from(HARNESS_CHAT_FILE),
            default_harness_file(
                "chat",
                "聊天 / 规划 / 分析型 harness。少用写文件工具，优先结构化推理与检索。",
                &["read", "bash", "web_search", "web_fetch", "write"],
            ),
        ),
        (
            PathBuf::from(HARNESS_CODE_FILE),
            default_harness_file(
                "code",
                "代码执行 harness。优先读代码、改代码、跑命令、验证结果。",
                &["read", "write", "edit", "bash", "web_fetch"],
            ),
        ),
        (
            PathBuf::from(HARNESS_CREDENTIALS_FILE),
            default_credentials_file(),
        ),
    ] {
        let path = agent_home.join(relative_path);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("创建 harness 子目录失败: {error}"))?;
        }
        fs::write(&path, fallback)
            .map_err(|error| format!("写入 harness 脚手架失败 {}: {error}", path.display()))?;
    }

    Ok(())
}

pub fn default_harness_file(name: &str, description: &str, active_tools: &[&str]) -> String {
    serde_json::to_string_pretty(&json!({
        "name": name,
        "description": description,
        "promptAppend": format!(
            "你当前运行在 `{name}` harness 下。优先遵守该 harness 的职责边界；若任务超出边界，应先收缩问题，再决定是否调用工具。"
        ),
        "activeTools": active_tools,
        "sessionReplayLimit": DEFAULT_SESSION_REPLAY_LIMIT,
        "enableExternalApiProxy": false,
        "autoRetryOnRuntimeFailure": true
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

pub fn default_credentials_file() -> String {
    serde_json::to_string_pretty(&json!({
        "externalApis": [
            {
                "alias": "example",
                "baseUrl": "https://api.example.com",
                "apiKey": "",
                "authHeader": "authorization",
                "authScheme": "Bearer",
                "headers": {
                    "accept": "application/json"
                }
            }
        ]
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

fn read_json_file<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let content = fs::read_to_string(path)
        .map_err(|error| format!("读取 JSON 文件失败 {}: {error}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("解析 JSON 文件失败 {}: {error}", path.display()))
}

pub fn select_harness(
    agent_home: &Path,
    execution_mode: &str,
    prompt: Option<&str>,
) -> Result<SelectedHarness, String> {
    ensure_agent_runtime_scaffold(agent_home)?;

    let prompt = prompt.unwrap_or_default().trim().to_lowercase();
    let wants_code = execution_mode.trim() == "worker"
        || contains_any(
            &prompt,
            &[
                "代码",
                "修复",
                "bug",
                "函数",
                "compile",
                "build",
                "test",
                "cargo",
                "npm",
                "typescript",
                "rust",
                "refactor",
            ],
        );
    let wants_chat = contains_any(
        &prompt,
        &[
            "聊", "总结", "规划", "方案", "分析", "总结", "解释", "review",
        ],
    );

    let code_path = agent_home.join(HARNESS_CODE_FILE);
    let chat_path = agent_home.join(HARNESS_CHAT_FILE);
    let default_path = agent_home.join(HARNESS_DEFAULT_FILE);
    let selected_path = if wants_code && code_path.exists() {
        code_path
    } else if wants_chat && chat_path.exists() {
        chat_path
    } else if default_path.exists() {
        default_path
    } else if chat_path.exists() {
        chat_path
    } else {
        code_path
    };

    let definition: HarnessDefinition = read_json_file(&selected_path)?;
    Ok(SelectedHarness {
        file_path: selected_path,
        definition,
    })
}

fn contains_any(content: &str, needles: &[&str]) -> bool {
    needles
        .iter()
        .any(|needle| !needle.trim().is_empty() && content.contains(&needle.trim().to_lowercase()))
}

pub fn append_session_event(
    agent_home: &Path,
    session_id: &str,
    kind: SessionEventKind,
    summary: impl Into<String>,
    detail: Option<Value>,
) -> Result<(), String> {
    ensure_agent_runtime_scaffold(agent_home)?;
    let event = SessionEventRecord {
        id: format!("sess_evt_{}", Uuid::new_v4().simple()),
        session_id: session_id.trim().to_string(),
        kind,
        summary: truncate_summary(&summary.into(), 220),
        detail,
        created_at: now_ms(),
    };
    let path = session_log_path(agent_home, session_id);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("打开 session log 失败 {}: {error}", path.display()))?;
    let line = serde_json::to_string(&event)
        .map_err(|error| format!("序列化 session event 失败: {error}"))?;
    writeln!(file, "{line}")
        .map_err(|error| format!("写入 session event 失败 {}: {error}", path.display()))?;
    Ok(())
}

pub fn append_session_event_quiet(
    agent_home: Option<&Path>,
    session_id: &str,
    kind: SessionEventKind,
    summary: impl Into<String>,
    detail: Option<Value>,
) {
    let Some(agent_home) = agent_home else {
        return;
    };
    if let Err(error) = append_session_event(agent_home, session_id, kind, summary, detail) {
        log::warn!(
            "managed runtime session event append failed agent_home={} session_id={}: {}",
            agent_home.display(),
            session_id,
            error
        );
    }
}

pub fn clear_session_events(agent_home: &Path, session_id: &str) -> Result<(), String> {
    let path = session_log_path(agent_home, session_id);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "删除 managed runtime session log 失败 {}: {error}",
            path.display()
        )),
    }
}

pub fn append_assistant_output_events(agent_home: Option<&Path>, session_id: &str, content: &str) {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return;
    }
    append_session_event_quiet(
        agent_home,
        session_id,
        SessionEventKind::AssistantOutput,
        trimmed,
        None,
    );
    for decision in extract_decision_summaries(trimmed) {
        append_session_event_quiet(
            agent_home,
            session_id,
            SessionEventKind::Decision,
            decision,
            None,
        );
    }
}

pub fn build_session_context_snapshot(
    agent_home: &Path,
    limit: usize,
    char_limit: usize,
) -> Result<Option<String>, String> {
    ensure_agent_runtime_scaffold(agent_home)?;
    let sessions_dir = agent_home.join(SESSIONS_DIR);
    let mut files = fs::read_dir(&sessions_dir)
        .map_err(|error| {
            format!(
                "读取 session log 目录失败 {}: {error}",
                sessions_dir.display()
            )
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|item| item.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();

    files.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });
    files.reverse();

    let mut events = Vec::new();
    for path in files {
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);
        let mut lines = reader.lines().map_while(Result::ok).collect::<Vec<_>>();
        lines.reverse();
        for line in lines {
            if let Ok(event) = serde_json::from_str::<SessionEventRecord>(&line) {
                events.push(event);
                if events.len() >= limit {
                    break;
                }
            }
        }
        if events.len() >= limit {
            break;
        }
    }

    if events.is_empty() {
        return Ok(None);
    }

    let mut lines = vec!["Recent Session Events:".to_string()];
    for event in events {
        let label = match event.kind {
            SessionEventKind::Prompt => "prompt",
            SessionEventKind::ToolCall => "tool_call",
            SessionEventKind::ToolResult => "tool_result",
            SessionEventKind::AssistantOutput => "output",
            SessionEventKind::Decision => "decision",
            SessionEventKind::RuntimeError => "runtime_error",
            SessionEventKind::RuntimeRetry => "runtime_retry",
        };
        lines.push(format!(
            "- [{}] {}",
            label,
            truncate_summary(&event.summary, 120)
        ));
    }

    let rendered = trim_to_char_limit(&lines.join("\n"), char_limit);
    if rendered.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(rendered))
    }
}

fn truncate_summary(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.trim().to_string();
    }

    let mut output = String::new();
    for ch in value.chars().take(limit) {
        output.push(ch);
    }
    output.push('…');
    output.trim().to_string()
}

fn trim_to_char_limit(value: &str, char_limit: usize) -> String {
    if value.chars().count() <= char_limit {
        return value.to_string();
    }
    let mut output = String::new();
    for ch in value.chars().take(char_limit) {
        output.push(ch);
    }
    output.push('…');
    output
}

fn sanitize_session_file_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim_matches('_').is_empty() {
        "default".to_string()
    } else {
        sanitized
    }
}

fn session_log_path(agent_home: &Path, session_id: &str) -> PathBuf {
    agent_home
        .join(SESSIONS_DIR)
        .join(format!("{}.jsonl", sanitize_session_file_name(session_id)))
}

pub fn extract_decision_summaries(content: &str) -> Vec<String> {
    let normalized = content.trim();
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_lowercase();
        if contains_any(&lower, &["不是决定", "not a decision", "并非决定"]) {
            continue;
        }
        if contains_any(
            &lower,
            &[
                "决定",
                "约定",
                "规则",
                "统一",
                "以后都",
                "must",
                "we will",
                "decision",
            ],
        ) {
            hits.push(truncate_summary(trimmed, 140));
        }
    }
    hits.dedup();
    hits
}

pub fn should_auto_retry_runtime(
    harness: Option<&SelectedHarness>,
    attempt: usize,
    error: &str,
) -> bool {
    if attempt > 0 {
        return false;
    }
    let Some(harness) = harness else {
        return false;
    };
    if !harness.definition.auto_retry_on_runtime_failure {
        return false;
    }
    let lower = error.trim().to_lowercase();
    contains_any(
        &lower,
        &[
            "未返回 agent_end",
            "退出码",
            "超时",
            "timed out",
            "signal",
            "信号终止",
            "runtime",
            "rpc 错误",
            "启动 pi 失败",
            "无输出内容",
        ],
    )
}

pub fn build_retry_prompt(original_prompt: &str) -> String {
    let trimmed = original_prompt.trim();
    if trimmed.is_empty() {
        return "上一次运行因为 runtime 异常中断。请基于当前 session 状态继续未完成任务，不要重复已经完成的工作。"
            .to_string();
    }
    format!(
        "{trimmed}\n\n[system note] 上一次运行因 runtime 异常中断。请读取当前 session 状态并继续未完成任务，不要重复已经完成的步骤。"
    )
}

fn resolve_typebox_import_path(pi_executable: &Path) -> Option<PathBuf> {
    let resolved = fs::canonicalize(pi_executable).unwrap_or_else(|_| pi_executable.to_path_buf());
    let mut roots = Vec::new();
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
                    .join("@sinclair")
                    .join("typebox")
                    .join("build")
                    .join("esm")
                    .join("index.mjs"),
                ancestor
                    .join("pi-package")
                    .join("node_modules")
                    .join("@sinclair")
                    .join("typebox")
                    .join("build")
                    .join("esm")
                    .join("type")
                    .join("index.mjs"),
            ] {
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

pub fn prepare_managed_runtime(
    pi_executable: &Path,
    runtime_dir: &Path,
    agent_config: &ConversationAgentConfig,
    prompt: Option<&str>,
    session_id: &str,
    provider_config: Option<&ProviderRuntimeConfig>,
    image_runtime_config: Option<&ImageGenerationRuntimeConfig>,
) -> Result<PreparedManagedRuntime, String> {
    let workspace_root = crate::agent_workspace::resolve_workspace_root()?;
    let agent_home = workspace_root.join("agents").join(&agent_config.id);
    ensure_agent_runtime_scaffold(&agent_home)?;

    let harness = select_harness(&agent_home, &agent_config.execution_mode, prompt)?;
    let credentials_path = agent_home.join(HARNESS_CREDENTIALS_FILE);
    let credentials: HarnessCredentialsConfig =
        read_json_file(&credentials_path).unwrap_or_default();

    let proxy_session_token = stable_session_token(&agent_config.id, session_id);
    let llm_proxy = provider_config.map(|config| LlmProxyBinding {
        provider_id: config.provider_id.clone(),
        api_format: normalize_provider_api_format(&config.api_format, config.provider_id.trim())
            .to_string(),
        base_url: normalized_provider_runtime_base_url(
            &config.base_url,
            &config.api_format,
            config.provider_id.trim(),
        ),
        api_key: config.api_key.clone(),
        model: config.model.clone(),
    });

    let proxy_server = credential_proxy_server()?;
    {
        let mut guard = proxy_server
            .state
            .sessions
            .lock()
            .map_err(|error| format!("锁定 credential proxy 会话失败: {error}"))?;
        let external_apis = credentials
            .external_apis
            .into_iter()
            .filter(|item| !item.alias.trim().is_empty() && !item.base_url.trim().is_empty())
            .map(|item| (item.alias.clone(), item))
            .collect::<HashMap<_, _>>();
        guard.insert(
            proxy_session_token.clone(),
            ProxySessionConfig {
                llm: llm_proxy.clone(),
                image: image_runtime_config.cloned(),
                external_apis,
            },
        );
    }

    let typebox_import_path = resolve_typebox_import_path(pi_executable)
        .ok_or_else(|| "无法定位 typebox 运行库，无法生成 managed runtime 扩展".to_string())?;
    let extension_path = crate::managed_runtime_extension::write_managed_runtime_extension_files(
        runtime_dir,
        &typebox_import_path,
    )?;

    Ok(PreparedManagedRuntime {
        extension_path,
        harness,
        llm_proxy,
        image_proxy: image_runtime_config.cloned(),
        proxy_base_url: Some(proxy_server.base_url.clone()),
        session_token: Some(proxy_session_token),
    })
}

fn credential_proxy_server() -> Result<&'static CredentialProxyServer, String> {
    static SERVER: OnceLock<CredentialProxyServer> = OnceLock::new();
    if let Some(server) = SERVER.get() {
        return Ok(server);
    }

    let sessions = Arc::new(Mutex::new(HashMap::new()));
    let state = CredentialProxyState {
        sessions,
        client: crate::build_http_client(),
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("绑定 credential proxy 端口失败: {error}"))?;
    let addr = listener
        .local_addr()
        .map_err(|error| format!("读取 credential proxy 地址失败: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("设置 credential proxy listener 非阻塞失败: {error}"))?;
    let router = Router::new()
        .route("/llm/:token/*path", any(llm_proxy_handler))
        .route("/external/:token/dispatch", post(external_proxy_handler))
        .route("/image/:token/generate", post(image_proxy_handler))
        .route("/image/:token/task/:task_id", get(image_task_query_handler))
        .with_state(state.clone());
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                log::error!("credential proxy tokio listener 创建失败: {error}");
                return;
            }
        };
        if let Err(error) = axum::serve(listener, router).await {
            log::error!("credential proxy 服务异常退出: {error}");
        }
    });

    let _ = SERVER.set(CredentialProxyServer {
        base_url: format!("http://{}", addr),
        state,
    });
    SERVER
        .get()
        .ok_or_else(|| "credential proxy 服务初始化失败".to_string())
}

async fn llm_proxy_handler(
    State(state): State<CredentialProxyState>,
    AxumPath((token, path)): AxumPath<(String, String)>,
    method: Method,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Response<Body> {
    let session = {
        let guard = match state.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return response_with_status(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "credential proxy session lock poisoned",
                )
            }
        };
        guard.get(&token).cloned()
    };
    let Some(session) = session else {
        return response_with_status(StatusCode::UNAUTHORIZED, "unknown credential proxy session");
    };
    let Some(llm) = session.llm else {
        return response_with_status(StatusCode::BAD_REQUEST, "no llm proxy configured");
    };

    let mut upstream = llm.base_url.trim_end_matches('/').to_string();
    if !path.trim().is_empty() {
        upstream.push('/');
        upstream.push_str(path.trim_start_matches('/'));
    }
    if let Some(query) = uri.query() {
        upstream.push('?');
        upstream.push_str(query);
    }

    let upstream_method =
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::POST);
    let mut request = state.client.request(upstream_method, &upstream);
    for (name, value) in headers.iter() {
        if name.as_str().eq_ignore_ascii_case("host")
            || name.as_str().eq_ignore_ascii_case("content-length")
            || name.as_str().eq_ignore_ascii_case("authorization")
            || name.as_str().eq_ignore_ascii_case("x-api-key")
        {
            continue;
        }
        request = request.header(name, value);
    }
    request = request.header("x-nineclaw-session-token", token.as_str());
    match llm.api_format.as_str() {
        "anthropic" => {
            request = request.header("x-api-key", llm.api_key.as_str());
            if !headers.contains_key("anthropic-version") {
                request = request.header("anthropic-version", "2023-06-01");
            }
        }
        _ => {
            request = request.bearer_auth(llm.api_key.as_str());
        }
    }
    let response = match request.body(body).send().await {
        Ok(response) => response,
        Err(error) => {
            return response_with_status(
                StatusCode::BAD_GATEWAY,
                &format!("upstream llm proxy request failed: {error}"),
            )
        }
    };

    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let response_headers = response.headers().clone();
    let response_body = match response.bytes().await {
        Ok(body) => body,
        Err(error) => {
            return response_with_status(
                StatusCode::BAD_GATEWAY,
                &format!("read upstream llm response failed: {error}"),
            )
        }
    };
    let mut output = Response::new(Body::from(response_body));
    *output.status_mut() = status;
    for (name, value) in response_headers.iter() {
        if name.as_str().eq_ignore_ascii_case("content-length") {
            continue;
        }
        output.headers_mut().insert(name.clone(), value.clone());
    }
    output
}

async fn external_proxy_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(payload): Json<ExternalDispatchRequest>,
) -> Result<Json<ExternalDispatchResponse>, (StatusCode, String)> {
    let session = {
        let guard = state.sessions.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "credential proxy session lock poisoned".to_string(),
            )
        })?;
        guard.get(&token).cloned()
    };
    let Some(session) = session else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "unknown credential proxy session".to_string(),
        ));
    };
    let alias = payload.alias.trim();
    let credential = session.external_apis.get(alias).cloned().ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("external api alias not found: {}", payload.alias),
        )
    })?;

    let url = build_external_api_url(&credential.base_url, &payload.path, &payload.query);
    let method = payload
        .method
        .trim()
        .parse::<reqwest::Method>()
        .unwrap_or(reqwest::Method::GET);
    let mut request = state.client.request(method, &url);
    for (name, value) in &credential.headers {
        request = request.header(name, value);
    }
    for (name, value) in &payload.headers {
        request = request.header(name, value);
    }
    if !credential.api_key.trim().is_empty() {
        request = inject_external_auth(request, &credential);
    }
    if let Some(body) = payload.body {
        request = request.body(body);
    }

    let response = request.send().await.map_err(|error| {
        (
            StatusCode::BAD_GATEWAY,
            format!("external api request failed: {error}"),
        )
    })?;
    let status = response.status().as_u16();
    let mut headers = HashMap::new();
    for (name, value) in response.headers() {
        if let Ok(text) = value.to_str() {
            headers.insert(name.to_string(), text.to_string());
        }
    }
    let body_text = response.text().await.map_err(|error| {
        (
            StatusCode::BAD_GATEWAY,
            format!("read external api response failed: {error}"),
        )
    })?;

    Ok(Json(ExternalDispatchResponse {
        status,
        body_text,
        headers,
    }))
}

async fn image_proxy_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(payload): Json<ImageGenerateProxyRequest>,
) -> Result<Json<crate::image_generation::ImageGenerateProxyResponse>, (StatusCode, String)> {
    let session = {
        let guard = state.sessions.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "credential proxy session lock poisoned".to_string(),
            )
        })?;
        guard.get(&token).cloned()
    };
    let Some(session) = session else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "unknown credential proxy session".to_string(),
        ));
    };
    let Some(image_runtime) = session.image else {
        return Err((
            StatusCode::BAD_REQUEST,
            "no image generation runtime configured".to_string(),
        ));
    };
    let response = dispatch_image_generation(&state.client, &image_runtime, &payload)
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error))?;
    Ok(Json(response))
}

async fn image_task_query_handler(
    State(state): State<CredentialProxyState>,
    AxumPath((token, task_id)): AxumPath<(String, String)>,
) -> Result<Json<crate::image_generation::ImageTaskQueryResponse>, (StatusCode, String)> {
    let session = {
        let guard = state.sessions.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "credential proxy session lock poisoned".to_string(),
            )
        })?;
        guard.get(&token).cloned()
    };
    let Some(session) = session else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "unknown credential proxy session".to_string(),
        ));
    };
    let Some(image_runtime) = session.image else {
        return Err((
            StatusCode::BAD_REQUEST,
            "no image generation runtime configured".to_string(),
        ));
    };
    let response =
        crate::image_generation::dispatch_image_task_query(&state.client, &image_runtime, &task_id)
            .await
            .map_err(|error| (StatusCode::BAD_GATEWAY, error))?;
    Ok(Json(response))
}

fn inject_external_auth(
    request: reqwest::RequestBuilder,
    credential: &ExternalApiCredential,
) -> reqwest::RequestBuilder {
    let header_name = credential.auth_header.trim();
    if header_name.is_empty() {
        return request;
    }
    let scheme = credential.auth_scheme.trim();
    if scheme.is_empty() {
        request.header(header_name, credential.api_key.trim())
    } else {
        request.header(
            header_name,
            format!("{scheme} {}", credential.api_key.trim()),
        )
    }
}

fn build_external_api_url(base_url: &str, path: &str, query: &HashMap<String, String>) -> String {
    let mut url = base_url.trim_end_matches('/').to_string();
    let path = path.trim();
    if !path.is_empty() {
        url.push('/');
        url.push_str(path.trim_start_matches('/'));
    }
    if !query.is_empty() {
        let mut first = true;
        for (key, value) in query {
            if first {
                url.push('?');
                first = false;
            } else {
                url.push('&');
            }
            let _ = write!(
                url,
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            );
        }
    }
    url
}

fn response_with_status(status: StatusCode, message: &str) -> Response<Body> {
    let mut response = Response::new(Body::from(message.to_string()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        HeaderName::from_static("content-type"),
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "nineclaw-managed-runtime-{}",
            Uuid::new_v4().simple()
        ))
    }

    #[test]
    fn scaffold_creates_sessions_and_harness_files() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("create root");
        ensure_agent_runtime_scaffold(&root).expect("scaffold");
        assert!(root.join(SESSIONS_DIR).exists());
        assert!(root.join(HARNESS_DEFAULT_FILE).exists());
        assert!(root.join(HARNESS_CHAT_FILE).exists());
        assert!(root.join(HARNESS_CODE_FILE).exists());
        assert!(root.join(HARNESS_CREDENTIALS_FILE).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn selects_code_harness_for_code_like_prompt() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("create root");
        ensure_agent_runtime_scaffold(&root).expect("scaffold");
        let harness = select_harness(
            &root,
            "single",
            Some("帮我修复这个 rust bug 并跑 cargo test"),
        )
        .expect("select harness");
        assert_eq!(harness.definition.name, "code");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn appends_events_and_builds_snapshot() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("create root");
        ensure_agent_runtime_scaffold(&root).expect("scaffold");
        append_session_event(
            &root,
            "session-a",
            SessionEventKind::ToolCall,
            "调用 read 工具读取 Cargo.toml",
            None,
        )
        .expect("append event");
        append_session_event(
            &root,
            "session-a",
            SessionEventKind::Decision,
            "决定优先使用本地 proxy 而不是把真实 key 传进 pi",
            None,
        )
        .expect("append decision");
        let snapshot = build_session_context_snapshot(&root, 8, 300)
            .expect("snapshot")
            .expect("has snapshot");
        assert!(snapshot.contains("tool_call"));
        assert!(snapshot.contains("decision"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn extracts_decisions_from_assistant_output() {
        let items = extract_decision_summaries(
            "我们决定统一走本地 proxy。\n这不是决定。\n以后都不要把真实 key 传进 agent。",
        );
        assert_eq!(items.len(), 2);
        assert!(items[0].contains("决定"));
    }

    #[test]
    fn builds_external_api_url_with_query() {
        let mut query = HashMap::new();
        query.insert("q".to_string(), "rust lang".to_string());
        let url = build_external_api_url("https://api.example.com", "/search", &query);
        assert!(url.starts_with("https://api.example.com/search?"));
        assert!(url.contains("q=rust%20lang"));
    }
}
