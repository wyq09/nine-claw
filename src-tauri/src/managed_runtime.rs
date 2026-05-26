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
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use uuid::Uuid;

const SESSIONS_DIR: &str = "memory/sessions";
const HARNESS_DIR: &str = "harness";
const HARNESS_DEFAULT_FILE: &str = "harness/default.json";
const HARNESS_CHAT_FILE: &str = "harness/chat.json";
const HARNESS_CODE_FILE: &str = "harness/code.json";
const HARNESS_CREDENTIALS_FILE: &str = "harness/credentials.json";
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
    #[allow(dead_code)]
    pub llm_proxy: Option<LlmProxyBinding>,
    #[allow(dead_code)]
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
    session_id: Option<String>,
    workspace_id: Option<String>,
    caller_agent_id: Option<String>,
    caller_agent_name: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DelegateRoleHint {
    role: String,
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

fn normalize_delegate_lookup_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn kebab_case_delegate_lookup_key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_was_sep = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep && !out.is_empty() {
            out.push('-');
            last_was_sep = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn build_delegate_lookup_keys(
    agent: &crate::agents::AgentRecord,
    role_hint: Option<&DelegateRoleHint>,
) -> HashSet<String> {
    let mut keys = HashSet::new();
    for raw in [
        agent.id.trim(),
        agent.name.trim(),
        role_hint.map(|hint| hint.role.trim()).unwrap_or_default(),
    ] {
        if raw.is_empty() {
            continue;
        }
        let normalized = normalize_delegate_lookup_key(raw);
        if !normalized.is_empty() {
            keys.insert(normalized);
        }
        let kebab = kebab_case_delegate_lookup_key(raw);
        if !kebab.is_empty() {
            keys.insert(kebab.clone());
            keys.insert(format!("agent-{kebab}"));
        }
    }
    keys
}

fn tokenize_delegate_lookup_text(value: &str) -> Vec<String> {
    value
        .split(|ch: char| {
            !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                && !('\u{4e00}' <= ch && ch <= '\u{9fff}')
        })
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn build_delegate_search_haystack(
    agent: &crate::agents::AgentRecord,
    role_hint: Option<&DelegateRoleHint>,
) -> String {
    [
        agent.id.trim(),
        agent.name.trim(),
        role_hint.map(|hint| hint.role.trim()).unwrap_or_default(),
        agent.summary.trim(),
        agent.description.trim(),
    ]
    .into_iter()
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>()
    .join("\n")
    .to_lowercase()
}

fn score_delegate_agent_match(
    agent: &crate::agents::AgentRecord,
    role_hint: Option<&DelegateRoleHint>,
    requested_role: &str,
    task: &str,
) -> usize {
    let haystack = build_delegate_search_haystack(agent, role_hint);
    if haystack.is_empty() {
        return 0;
    }

    let mut score = 0usize;
    for needle in [requested_role, task] {
        let normalized = needle.trim().to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if haystack.contains(&normalized) {
            score += normalized.chars().count().max(1) * 10;
        }
        for token in tokenize_delegate_lookup_text(needle) {
            if token.chars().count() <= 1 {
                continue;
            }
            if haystack.contains(&token) {
                score += token.chars().count();
            }
        }
    }
    score
}

fn resolve_delegate_agent<'a>(
    candidate_agents: &'a [&crate::agents::AgentRecord],
    role_hints: &HashMap<String, DelegateRoleHint>,
    requested_role: &str,
    task: &str,
) -> Option<&'a crate::agents::AgentRecord> {
    let requested_role = requested_role.trim();
    if requested_role.is_empty() {
        return None;
    }
    let normalized = normalize_delegate_lookup_key(requested_role);
    let kebab = kebab_case_delegate_lookup_key(requested_role);
    let mut requested_keys = HashSet::new();
    requested_keys.insert(normalized.clone());
    if !kebab.is_empty() {
        requested_keys.insert(kebab.clone());
        requested_keys.insert(format!("agent-{kebab}"));
    }

    candidate_agents
        .iter()
        .copied()
        .find(|agent| agent.id == requested_role)
        .or_else(|| {
            candidate_agents.iter().copied().find(|agent| {
                let role_hint = role_hints.get(&agent.id);
                build_delegate_lookup_keys(agent, role_hint)
                    .iter()
                    .any(|key| requested_keys.contains(key))
            })
        })
        .or_else(|| {
            candidate_agents.iter().copied().find(|agent| {
                let role_hint = role_hints.get(&agent.id);
                let haystacks = [
                    normalize_delegate_lookup_key(&agent.name),
                    role_hint
                        .map(|hint| normalize_delegate_lookup_key(&hint.role))
                        .unwrap_or_default(),
                ];
                haystacks
                    .iter()
                    .filter(|value| !value.is_empty())
                    .any(|value| value.contains(&normalized))
            })
        })
        .or_else(|| {
            candidate_agents
                .iter()
                .copied()
                .filter_map(|agent| {
                    let role_hint = role_hints.get(&agent.id);
                    let score = score_delegate_agent_match(agent, role_hint, requested_role, task);
                    (score > 0).then_some((score, agent))
                })
                .max_by(|(left_score, left_agent), (right_score, right_agent)| {
                    left_score
                        .cmp(right_score)
                        .then_with(|| right_agent.updated_at.cmp(&left_agent.updated_at))
                })
                .map(|(_, agent)| agent)
        })
}

fn format_delegate_candidates(
    candidate_agents: &[&crate::agents::AgentRecord],
    role_hints: &HashMap<String, DelegateRoleHint>,
) -> String {
    if candidate_agents.is_empty() {
        return "无".to_string();
    }
    candidate_agents
        .iter()
        .map(|agent| {
            let mut label = format!("{}({})", agent.name, agent.id);
            if let Some(role_hint) = role_hints.get(&agent.id) {
                let role = role_hint.role.trim();
                if !role.is_empty() {
                    label.push_str(&format!(" role={role}"));
                }
            }
            label
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn delegate_scope_label(workspace_id: Option<&str>) -> &'static str {
    if workspace_id.is_some() {
        "当前团队成员"
    } else {
        "当前直接聊天可调用的智能体"
    }
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

fn restrict_harness_to_agent_tools(
    harness: SelectedHarness,
    runtime_dir: &Path,
    allowed_tool_ids: &[String],
) -> Result<SelectedHarness, String> {
    let mut active_tools = crate::agents::runtime_tool_names_for_allowed_tool_ids(allowed_tool_ids);
    if active_tools.is_empty() {
        active_tools.push("__nineclaw_no_tools_allowed__".to_string());
    }

    let mut definition = harness.definition.clone();
    definition.active_tools = active_tools;
    let effective_path = runtime_dir.join(format!(
        "harness-effective-{}.json",
        sanitize_file_segment(&definition.name)
    ));
    let content = serde_json::to_vec_pretty(&definition)
        .map_err(|error| format!("序列化有效 harness 失败: {error}"))?;
    fs::write(&effective_path, content).map_err(|error| {
        format!(
            "写入有效 harness 失败 {}: {error}",
            effective_path.display()
        )
    })?;

    Ok(SelectedHarness {
        file_path: effective_path,
        definition,
    })
}

fn sanitize_file_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
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
    let detail_for_db = detail.clone();
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

    if let Some(app) = injected_app_handle() {
        if let Ok(conn) = crate::storage_conn(&app) {
            let agent_id = agent_home
                .file_name()
                .and_then(|name| name.to_str())
                .map(|value| value.to_string());
            let detail_json = detail_for_db
                .as_ref()
                .and_then(|value| serde_json::to_string(value).ok());
            let _ = crate::storage::core_memory::insert_runtime_session_event(
                &conn,
                &event.id,
                agent_id.as_deref(),
                &event.session_id,
                match event.kind {
                    SessionEventKind::Prompt => "prompt",
                    SessionEventKind::ToolCall => "tool_call",
                    SessionEventKind::ToolResult => "tool_result",
                    SessionEventKind::AssistantOutput => "assistant_output",
                    SessionEventKind::Decision => "decision",
                    SessionEventKind::RuntimeError => "runtime_error",
                    SessionEventKind::RuntimeRetry => "runtime_retry",
                },
                &event.summary,
                detail_json.as_deref(),
            );
        }
    }
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
    if let Some(app) = injected_app_handle() {
        if let Ok(conn) = crate::storage_conn(&app) {
            if let Some(agent_id) = agent_home.file_name().and_then(|name| name.to_str()) {
                if let Ok(events) = crate::storage::core_memory::list_recent_runtime_session_events(
                    &conn, agent_id, limit,
                ) {
                    if !events.is_empty() {
                        let mut lines = vec!["Recent Session Events:".to_string()];
                        for event in events {
                            lines.push(format!(
                                "- [{}] {}",
                                event.kind,
                                truncate_summary(&event.summary, 120)
                            ));
                        }
                        let rendered = trim_to_char_limit(&lines.join("\n"), char_limit);
                        if !rendered.trim().is_empty() {
                            return Ok(Some(rendered));
                        }
                    }
                }
            }
        }
    }

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
                    .join("typebox")
                    .join("build")
                    .join("index.mjs"),
                ancestor
                    .join("pi-package")
                    .join("node_modules")
                    .join("typebox")
                    .join("build")
                    .join("type")
                    .join("index.mjs"),
                ancestor
                    .join("pi-package")
                    .join("node_modules")
                    .join("@sinclair")
                    .join("typebox")
                    .join("build")
                    .join("index.mjs"),
                ancestor
                    .join("pi-package")
                    .join("node_modules")
                    .join("@sinclair")
                    .join("typebox")
                    .join("build")
                    .join("type")
                    .join("index.mjs"),
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
    let harness =
        restrict_harness_to_agent_tools(harness, runtime_dir, &agent_config.allowed_tool_ids)?;
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
                session_id: Some(session_id.to_string()),
                workspace_id: None,
                caller_agent_id: Some(agent_config.id.clone()),
                caller_agent_name: Some(agent_config.name.clone()),
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

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

pub fn inject_credential_proxy_app_handle(app: tauri::AppHandle) {
    let _ = APP_HANDLE.set(app);
}

pub fn injected_app_handle() -> Option<tauri::AppHandle> {
    APP_HANDLE.get().cloned()
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
        .route("/delegate/:token/dispatch", post(delegate_proxy_handler))
        .route("/ask-user/:token/dispatch", post(ask_user_proxy_handler))
        .route("/memory/:token/update", post(memory_update_handler))
        .route("/memory/:token/search", post(memory_search_handler))
        .route("/memory/:token/read", post(memory_read_handler))
        .route("/memory/:token/delete", post(memory_delete_handler))
        .route("/memory/:token/store", post(memory_store_handler))
        .route("/memory/:token/save", post(memory_save_handler))
        .route("/memory/:token/get", post(memory_get_handler))
        .route("/memory/:token/forget", post(memory_forget_handler))
        .route("/memory/:token/list", post(memory_list_handler))
        .route("/chat/:token/search", post(chat_search_handler))
        .route("/task/:token/create", post(task_create_handler))
        .route("/task/:token/list", post(task_list_handler))
        .route("/task/:token/detail", post(task_detail_handler))
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

// ---------------------------------------------------------------------------
// delegate proxy handler — agent_delegate tool endpoint
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DelegateRequest {
    role: String,
    task: String,
    #[serde(default)]
    context: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AskUserRequest {
    title: String,
    #[serde(default)]
    description: Option<String>,
    questions: Value,
    #[serde(default)]
    submit_label: Option<String>,
    #[serde(default)]
    cancel_label: Option<String>,
    #[serde(default)]
    allow_skip: bool,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

async fn delegate_proxy_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<DelegateRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    log::info!(
        "delegate_proxy: 收到委派请求 token={} role={} task_len={}",
        token.len(),
        body.role.len(),
        body.task.len(),
    );

    // Resolve AppHandle lazily — may not be available on very first call
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        log::error!("delegate_proxy: AppHandle 尚未注入");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入，委派功能暂不可用".to_string(),
        )
    })?;

    // Look up session config to get provider info + caller context
    let (llm_binding, session_id, workspace_id, caller_agent_id, caller_agent_name) = {
        let guard = state.sessions.lock().map_err(|_| {
            log::error!("delegate_proxy: session lock poisoned");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "session lock poisoned".to_string(),
            )
        })?;
        guard
            .get(&token)
            .map(|s| {
                (
                    s.llm.clone(),
                    s.session_id.clone(),
                    s.workspace_id.clone(),
                    s.caller_agent_id.clone(),
                    s.caller_agent_name.clone(),
                )
            })
            .unwrap_or((None, None, None, None, None))
    };
    let Some(llm) = llm_binding else {
        log::warn!(
            "delegate_proxy: 未找到 session 或 LLM 配置 token={}",
            token.len()
        );
        return Err((
            StatusCode::UNAUTHORIZED,
            "unknown session or no LLM config".to_string(),
        ));
    };

    let (allowed_delegate_ids, delegate_role_hints): (
        Option<HashSet<String>>,
        HashMap<String, DelegateRoleHint>,
    ) = if let Some(workspace_id) = workspace_id.as_deref() {
        crate::team_workspace::list_team_member_views(&app_handle, workspace_id)
            .ok()
            .map(|members| {
                let mut ids = HashSet::new();
                let mut role_hints = HashMap::new();
                for member in members {
                    if Some(member.agent_id.as_str()) == caller_agent_id.as_deref() {
                        continue;
                    }
                    if !member.role.trim().is_empty() {
                        role_hints.insert(
                            member.agent_id.clone(),
                            DelegateRoleHint { role: member.role },
                        );
                    }
                    ids.insert(member.agent_id);
                }
                (Some(ids), role_hints)
            })
            .unwrap_or((None, HashMap::new()))
    } else {
        (None, HashMap::new())
    };

    // Find agent by role: first constrain to the current workspace / explicit allowlist,
    // then try exact ID, exact name, and fuzzy name contains.
    let agents = crate::agents::list_agents(&app_handle)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let candidate_agents: Vec<_> = agents
        .iter()
        .filter(|agent| {
            if Some(agent.id.as_str()) == caller_agent_id.as_deref() {
                return false;
            }
            allowed_delegate_ids
                .as_ref()
                .map(|ids| ids.contains(&agent.id))
                .unwrap_or(true)
        })
        .collect();

    let agent = resolve_delegate_agent(
        &candidate_agents,
        &delegate_role_hints,
        &body.role,
        &body.task,
    );

    let Some(agent) = agent else {
        let available = format_delegate_candidates(&candidate_agents, &delegate_role_hints);
        let scope_label = delegate_scope_label(workspace_id.as_deref());
        return Ok(Json(json!({
            "ok": false,
            "error": format!(
                "未找到匹配 '{}' 的子智能体。请使用{}中的真实 agentId、显示名{}。当前可委派智能体：{}",
                body.role,
                scope_label,
                if workspace_id.is_some() { "或团队 role" } else { "" },
                available,
            )
        })));
    };

    // Rebuild provider config from the session's LLM binding
    let provider_config = ProviderRuntimeConfig {
        provider_id: llm.provider_id,
        api_format: llm.api_format,
        base_url: llm.base_url,
        api_key: llm.api_key,
        model: llm.model,
    };

    // Begin trace: record delegation in LLM trace panel
    let trace_id = crate::llm_trace::begin(
        &app_handle,
        workspace_id.as_deref(),
        "delegate",
        caller_agent_id.as_deref().unwrap_or("unknown"),
        caller_agent_name.as_deref().unwrap_or("Unknown"),
        Some(&agent.id),
        Some(&agent.name),
        session_id.as_deref(),
        Some(&provider_config.provider_id),
        Some(&provider_config.model),
        vec![],
        &format!("[委派任务] {}\n\n{}", body.task, body.context),
    );

    log::info!(
        "delegate_proxy: 已解析目标智能体 id={} name={} provider={}/{} 开始执行委派",
        agent.id,
        agent.name,
        provider_config.provider_id,
        provider_config.model,
    );

    // Generate runId for real-time event tracking
    let run_id = Uuid::new_v4().simple().to_string();

    // Emit delegate.progress event (status: "running")
    let ws_id_for_progress = workspace_id.clone();
    let _ = app_handle.emit(
        "workspace:delegate:progress",
        serde_json::json!({
            "runId": run_id,
            "workspaceId": ws_id_for_progress,
            "agentId": agent.id,
            "agentName": agent.name,
            "status": "running",
        }),
    );

    // Build event callbacks that emit Tauri events
    let app_for_chunk_cb = app_handle.clone();
    let run_id_for_chunk_cb = run_id.clone();
    let ws_id_for_chunk_cb = workspace_id.clone();
    let on_chunk_cb: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |chunk: &str| {
        if !chunk.is_empty() {
            let _ = app_for_chunk_cb.emit(
                "workspace:delegate:chunk",
                serde_json::json!({
                    "runId": run_id_for_chunk_cb,
                    "workspaceId": ws_id_for_chunk_cb,
                    "deltaText": chunk,
                }),
            );
        }
    });

    let app_for_tool_cb = app_handle.clone();
    let run_id_for_tool_cb = run_id.clone();
    let ws_id_for_tool_cb = workspace_id.clone();
    let seen_tool_ids: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let tool_counter: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let on_tool_cb: Arc<dyn Fn(&str, &str, &str) + Send + Sync> =
        Arc::new(move |tool_call_id: &str, tool_name: &str, status: &str| {
            let is_new = if !tool_call_id.is_empty() {
                seen_tool_ids
                    .lock()
                    .map(|mut s| s.insert(tool_call_id.to_string()))
                    .unwrap_or(true)
            } else {
                true
            };
            let index = if is_new {
                tool_counter.fetch_add(1, Ordering::SeqCst)
            } else {
                tool_counter.load(Ordering::SeqCst).saturating_sub(1)
            };
            let _ = app_for_tool_cb.emit(
                "workspace:delegate:tool",
                serde_json::json!({
                    "runId": run_id_for_tool_cb,
                    "workspaceId": ws_id_for_tool_cb,
                    "toolIndex": index,
                    "toolCallId": tool_call_id,
                    "toolName": tool_name,
                    "status": status,
                }),
            );
        });

    let app_for_turn_cb = app_handle.clone();
    let run_id_for_turn_cb = run_id.clone();
    let ws_id_for_turn_cb = workspace_id.clone();
    let on_turn_cb: Arc<dyn Fn(u32) + Send + Sync> = Arc::new(move |turn_index: u32| {
        let _ = app_for_turn_cb.emit(
            "workspace:delegate:turn",
            serde_json::json!({
                "runId": run_id_for_turn_cb,
                "workspaceId": ws_id_for_turn_cb,
                "turnIndex": turn_index,
            }),
        );
    });

    let event_cbs = crate::agent_loop::DelegateEventCallbacks {
        on_chunk: Some(on_chunk_cb),
        on_tool: Some(on_tool_cb),
        on_turn: Some(on_turn_cb),
    };

    // Execute delegation (blocking call with PiBridge)
    let app = app_handle.clone();
    let agent_id = agent.id.clone();
    let agent_name = agent.name.clone();
    let task = body.task.clone();
    let context = if body.context.is_empty() {
        None
    } else {
        Some(body.context.clone())
    };
    let trace_id_for_delegate = trace_id.clone();
    let run_id_for_done = run_id.clone();
    let ws_id_for_done = workspace_id.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        log::info!(
            "delegate_proxy: spawn_blocking 开始 agent_id={} agent_name={}",
            agent_id,
            agent_name,
        );
        let result = crate::agent_loop::delegate_to_agent_with_trace(
            &app,
            &agent_id,
            &task,
            context.as_deref(),
            &provider_config,
            Some(trace_id_for_delegate.as_str()),
            event_cbs,
        );
        log::info!(
            "delegate_proxy: spawn_blocking 完成 agent_id={} status={} output_len={}",
            agent_id,
            result.status,
            result.output.len(),
        );
        result
    })
    .await
    .map_err(|e| {
        log::error!("delegate_proxy: spawn_blocking JoinError: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    // Emit delegate.done event
    let done_status = if result.status == "success" {
        "success"
    } else {
        "error"
    };
    let _ = app_handle.emit(
        "workspace:delegate:done",
        serde_json::json!({
            "runId": run_id_for_done,
            "workspaceId": ws_id_for_done,
            "status": done_status,
            "agentId": result.agent_id,
            "agentName": result.agent_name,
            "durationMs": result.duration_ms,
        }),
    );

    // Finalize trace
    let final_status = if result.status == "success" {
        "done"
    } else {
        "error"
    };
    let trace_error = if result.status == "success" {
        None
    } else {
        Some(result.output.clone())
    };
    crate::llm_trace::finalize(
        &app_handle,
        &trace_id,
        final_status,
        trace_error,
        Some(result.output.clone()),
        None,
        None,
        None,
        None,
    );

    Ok(Json(json!({
        "ok": result.status == "success",
        "output": result.output,
        "agentId": result.agent_id,
        "agentName": result.agent_name,
        "durationMs": result.duration_ms,
        "runId": run_id,
        "error": if result.status == "success" { Value::Null } else { Value::String(result.output.clone()) }
    })))
}

async fn ask_user_proxy_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<AskUserRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入，ask_user 暂不可用".to_string(),
        )
    })?;

    let session_id = {
        let guard = state.sessions.lock().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "session lock poisoned".to_string(),
            )
        })?;
        guard
            .get(&token)
            .and_then(|config| config.session_id.clone())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    "unknown session or missing session id".to_string(),
                )
            })?
    };

    let widget = crate::widget_runtime::build_ask_user_widget(
        body.title,
        body.description,
        body.submit_label,
        body.cancel_label,
        body.allow_skip,
        body.questions,
    )
    .map_err(|error| (StatusCode::BAD_REQUEST, error))?;

    let timeout_ms = body
        .timeout_ms
        .unwrap_or(10 * 60 * 1000)
        .clamp(1_000, 60 * 60 * 1000);
    let result = crate::widget_runtime::create_pending_widget_request(
        &app_handle,
        &session_id,
        widget,
        timeout_ms,
    )
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;

    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// memory proxy handlers — memory tool endpoints
// ---------------------------------------------------------------------------

const AGENT_MEMORY_WORKSPACE_PREFIX: &str = "__agent_memory__:";

fn build_agent_memory_workspace_id(agent_id: &str) -> String {
    format!("{AGENT_MEMORY_WORKSPACE_PREFIX}{agent_id}")
}

fn kv_memory_vector_id(key: &str) -> String {
    format!("kv::{key}")
}

fn extract_kv_key_from_vector_id(memory_id: &str) -> Option<&str> {
    memory_id.strip_prefix("kv::")
}

fn ensure_memory_workspace_namespace(
    conn: &Connection,
    workspace_id: &str,
    fallback_agent_id: Option<&str>,
) -> Result<(), String> {
    if !workspace_id.starts_with(AGENT_MEMORY_WORKSPACE_PREFIX) {
        return Ok(());
    }
    if crate::storage::workspaces::get_workspace(conn, workspace_id)?.is_some() {
        return Ok(());
    }
    let agent_id = fallback_agent_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("memory-agent");
    crate::storage::workspaces::create_workspace(
        conn,
        &crate::storage::workspaces::CreateWorkspaceInput {
            id: workspace_id.to_string(),
            name: format!("Agent Memory {agent_id}"),
            description: "Implicit agent-private memory namespace".to_string(),
            supervisor_agent_id: agent_id.to_string(),
        },
    )?;
    Ok(())
}

fn resolve_proxy_session(
    state: &CredentialProxyState,
    token: &str,
) -> Result<ProxySessionConfig, (StatusCode, String)> {
    let guard = state.sessions.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "session lock poisoned".to_string(),
        )
    })?;
    guard.get(token).cloned().ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "unknown credential proxy session".to_string(),
        )
    })
}

/// Resolve workspace_id for memory operations: prefer body value, then session workspace,
/// then fall back to an agent-private namespace so non-team agents can still use memory tools.
fn resolve_memory_workspace(
    state: &CredentialProxyState,
    token: &str,
    body_workspace_id: Option<&str>,
) -> Result<String, (StatusCode, String)> {
    if let Some(ws) = body_workspace_id {
        if !ws.trim().is_empty() {
            return Ok(ws.to_string());
        }
    }
    let session = resolve_proxy_session(state, token)?;
    if let Some(workspace_id) = session.workspace_id.filter(|ws| !ws.trim().is_empty()) {
        return Ok(workspace_id);
    }
    if let Some(agent_id) = session.caller_agent_id.filter(|id| !id.trim().is_empty()) {
        return Ok(build_agent_memory_workspace_id(&agent_id));
    }
    Err((
        StatusCode::BAD_REQUEST,
        "workspace_id is required for memory operations".to_string(),
    ))
}

fn resolve_current_agent_id(
    state: &CredentialProxyState,
    token: &str,
) -> Result<String, (StatusCode, String)> {
    resolve_proxy_session(state, token)?
        .caller_agent_id
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "current agent id is unavailable for memory file operations".to_string(),
            )
        })
}

fn memory_markdown_relative_path(agent_id: &str) -> String {
    format!("agents/{agent_id}/MEMORY.md")
}

fn memory_md_record_id(agent_id: &str) -> String {
    format!("memory_md_{agent_id}")
}

fn build_text_value_for_memory(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| String::new()),
    }
}

fn build_chat_search_excerpt(prompt: &str, answer: &str, query: &str) -> String {
    let query_lower = query.trim().to_lowercase();
    let base = if prompt.to_lowercase().contains(&query_lower) {
        prompt
    } else if answer.to_lowercase().contains(&query_lower) {
        answer
    } else {
        prompt
    };
    let compact = base.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= 160 {
        return compact;
    }
    compact.chars().take(160).collect()
}

fn normalize_search_keyword(value: &str) -> String {
    value.trim().to_lowercase()
}

fn matches_search_keyword(haystack: &str, keyword: &str) -> bool {
    if keyword.is_empty() {
        return false;
    }
    haystack.to_lowercase().contains(keyword)
}

fn memory_search_keyword_fallback(
    conn: &Connection,
    workspace_id: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<Value>, String> {
    let keyword = normalize_search_keyword(query);
    let mut results = Vec::new();

    for record in
        crate::storage::workspaces::list_workspace_memories(conn, workspace_id, limit as i64, None)?
    {
        let tags = serde_json::from_str::<Vec<String>>(&record.tags_json).unwrap_or_default();
        if matches_search_keyword(&record.title, &keyword)
            || matches_search_keyword(&record.content, &keyword)
            || tags.iter().any(|tag| matches_search_keyword(tag, &keyword))
        {
            results.push(json!({
                "memoryType": "workspace_memory",
                "searchMode": "keyword_fallback",
                "memoryId": record.id,
                "title": record.title,
                "content": record.content,
                "tags": tags,
                "scope": record.scope,
                "scopeAgentId": record.scope_agent_id,
                "score": Value::Null,
                "updatedAt": record.updated_at,
            }));
            if results.len() >= limit {
                return Ok(results);
            }
        }
    }

    for record in
        crate::storage::workspaces::list_workspace_kv_memories(conn, workspace_id, limit as i64)?
    {
        if matches_search_keyword(&record.memory_key, &keyword)
            || matches_search_keyword(&record.text_value, &keyword)
            || matches_search_keyword(&record.value_json, &keyword)
        {
            let value = serde_json::from_str::<Value>(&record.value_json)
                .unwrap_or_else(|_| Value::String(record.value_json.clone()));
            results.push(json!({
                "memoryType": "kv",
                "searchMode": "keyword_fallback",
                "memoryId": kv_memory_vector_id(&record.memory_key),
                "key": record.memory_key,
                "value": value,
                "score": Value::Null,
                "updatedAt": record.updated_at,
            }));
            if results.len() >= limit {
                break;
            }
        }
    }

    Ok(results)
}

/// Global embedding registry for memory tool handlers. Initialized at startup.
static EMBEDDING_REGISTRY: OnceLock<Arc<tokio::sync::RwLock<crate::embedding::ProviderRegistry>>> =
    OnceLock::new();

pub fn inject_embedding_registry(
    registry: Arc<tokio::sync::RwLock<crate::embedding::ProviderRegistry>>,
) {
    let _ = EMBEDDING_REGISTRY.set(registry);
}

/// Returns a reference-counted handle to the global embedding registry, if initialized.
pub fn get_embedding_registry(
) -> Option<Arc<tokio::sync::RwLock<crate::embedding::ProviderRegistry>>> {
    EMBEDDING_REGISTRY.get().cloned()
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct MemoryUpdateRequest {
    content: String,
    #[serde(default = "default_memory_update_mode")]
    mode: String,
}

fn default_memory_update_mode() -> String {
    "replace".to_string()
}

async fn memory_update_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<MemoryUpdateRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent_id = resolve_current_agent_id(&state, &token)?;
    let relative_path = memory_markdown_relative_path(&agent_id);
    let next_content = if body.mode == "append" {
        let existing = crate::agent_workspace::read_agent_workspace_file(&agent_id, &relative_path)
            .map(|file| file.content)
            .unwrap_or_default();
        if existing.is_empty() {
            body.content.clone()
        } else if existing.ends_with('\n') {
            format!("{existing}{}", body.content)
        } else {
            format!("{existing}\n{}", body.content)
        }
    } else {
        body.content.clone()
    };

    crate::agent_workspace::write_agent_workspace_file(&agent_id, &relative_path, &next_content)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;

    // Also index into workspace_memories + memory_vectors for semantic search.
    // Use the same workspace resolution as memory_search_handler so that
    // memory_search can find this content.
    if let Some(app_handle) = APP_HANDLE.get().cloned() {
        if let Ok(conn) = crate::storage_conn(&app_handle) {
            let ws_id = resolve_memory_workspace(&state, &token, None)
                .unwrap_or_else(|_| build_agent_memory_workspace_id(&agent_id));
            let _ = ensure_memory_workspace_namespace(&conn, &ws_id, Some(&agent_id));

            let record_id = memory_md_record_id(&agent_id);
            let title = "MEMORY.md".to_string();
            let tags_json = r#"["memory_md","agent_private"]"#.to_string();

            if crate::storage::workspaces::get_workspace_memory(&conn, &record_id)
                .ok()
                .flatten()
                .is_some()
            {
                let _ = crate::storage::workspaces::update_workspace_memory(
                    &conn,
                    &record_id,
                    Some(&title),
                    Some(&next_content),
                    Some(&tags_json),
                    Some("agent"),
                    Some(&agent_id),
                );
            } else {
                let _ = crate::storage::workspaces::insert_workspace_memory(
                    &conn,
                    &record_id,
                    &ws_id,
                    &title,
                    &next_content,
                    Some(&agent_id),
                    &tags_json,
                    "agent",
                    Some(&agent_id),
                );
            }

            // Spawn async embedding + vector upsert
            if let Some(registry) = EMBEDDING_REGISTRY.get().cloned() {
                let text_to_embed = next_content.clone();
                let ws_id_clone = ws_id.clone();
                let record_id_clone = record_id.clone();
                let app_h = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let provider = {
                        let guard = registry.read().await;
                        guard.default_provider()
                    };
                    if let Some(provider) = provider {
                        match provider.embed(vec![text_to_embed]).await {
                            Ok(embeddings) => {
                                if let Some(embedding) = embeddings.into_iter().next() {
                                    if let Ok(conn) = crate::storage_conn(&app_h) {
                                        let vector_id = format!("vec_{}", Uuid::new_v4().simple());
                                        let _ = crate::memory_vector::upsert_vector(
                                            &conn,
                                            &vector_id,
                                            &record_id_clone,
                                            &ws_id_clone,
                                            &embedding,
                                            provider.id(),
                                        );
                                    }
                                }
                            }
                            Err(error) => {
                                log::warn!("MEMORY.md 向量嵌入失败: {error}")
                            }
                        }
                    }
                });
            }
        }
    }

    Ok(Json(json!({
        "ok": true,
        "agentId": agent_id,
        "path": relative_path,
        "mode": body.mode,
        "content": next_content,
        "indexed": true,
    })))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
struct MemoryReadRequest {}

async fn memory_read_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(_body): Json<MemoryReadRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent_id = resolve_current_agent_id(&state, &token)?;
    let relative_path = memory_markdown_relative_path(&agent_id);
    let file = crate::agent_workspace::read_agent_workspace_file(&agent_id, &relative_path)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;

    Ok(Json(json!({
        "ok": true,
        "agentId": agent_id,
        "path": relative_path,
        "content": file.content,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct MemorySearchRequest {
    query: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
    #[serde(default)]
    threshold: Option<f32>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    workspace_id: Option<String>,
}

fn default_search_limit() -> usize {
    10
}

async fn memory_search_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<MemorySearchRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;

    // Resolve workspace_id from session if not provided in body
    let workspace_id = resolve_memory_workspace(&state, &token, body.workspace_id.as_deref())?;

    if workspace_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "workspace_id is required for memory operations".to_string(),
        ));
    }
    let conn =
        crate::storage_conn(&app_handle).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let semantic_results = if let Some(registry) = EMBEDDING_REGISTRY.get().cloned() {
        let provider = {
            let guard = registry.read().await;
            guard.default_provider()
        };
        if let Some(provider) = provider {
            let embeddings = provider
                .embed(vec![body.query.clone()])
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("生成查询嵌入失败: {e}"),
                    )
                })?;

            let query_embedding = embeddings.into_iter().next().ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "嵌入结果为空".to_string(),
                )
            })?;

            let threshold = body.threshold.unwrap_or(0.3);

            // Determine agent context for three-layer scope search
            let session = resolve_proxy_session(&state, &token).ok();
            let caller_agent_id = session
                .as_ref()
                .and_then(|s| s.caller_agent_id.as_deref())
                .filter(|s| !s.trim().is_empty());
            let is_supervisor = if let Some(ref aid) = caller_agent_id {
                // Check if this agent is the workspace supervisor
                crate::storage::workspaces::get_workspace(&conn, &workspace_id)
                    .ok()
                    .flatten()
                    .map(|ws| ws.supervisor_agent_id == *aid)
                    .unwrap_or(false)
            } else {
                true // No agent id = assume supervisor/omniscient
            };

            let mut hits = crate::memory_vector::three_layer_search(
                &conn,
                &workspace_id,
                caller_agent_id,
                is_supervisor,
                &query_embedding,
                body.limit,
                threshold,
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            if let Some(agent_id) = caller_agent_id {
                let mut namespaces = vec![crate::storage::core_memory::agent_vector_namespace(
                    agent_id,
                )];
                namespaces.extend(crate::storage::user_memory::vector_namespaces_for_agent(
                    Some(agent_id),
                ));
                let mut core_hits = crate::memory_vector::search_vectors_across_workspaces(
                    &conn,
                    &namespaces,
                    &query_embedding,
                    body.limit,
                    threshold,
                )
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
                hits.append(&mut core_hits);
                hits.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let mut dedup = std::collections::HashSet::new();
                hits.retain(|hit| dedup.insert(hit.memory_id.clone()));
                hits.truncate(body.limit);
            }

            // Post-filter by tags if specified
            if !body.tags.is_empty() {
                hits.retain(|hit| {
                    if let Ok(Some(record)) =
                        crate::storage::workspaces::get_workspace_memory(&conn, &hit.memory_id)
                    {
                        let tags = serde_json::from_str::<Vec<String>>(&record.tags_json)
                            .unwrap_or_default();
                        body.tags.iter().all(|t| tags.contains(t))
                    } else {
                        crate::storage::user_memory::fetch_search_text(&conn, &hit.memory_id)
                            .ok()
                            .flatten()
                            .map(|(_, _, _, _, tags)| body.tags.iter().all(|t| tags.contains(t)))
                            .unwrap_or(false)
                    }
                });
            }

            let mut results = Vec::new();
            for hit in &hits {
                if let Ok(Some(record)) =
                    crate::storage::workspaces::get_workspace_memory(&conn, &hit.memory_id)
                {
                    results.push(json!({
                        "memoryType": "workspace_memory",
                        "searchMode": "semantic",
                        "memoryId": record.id,
                        "title": record.title,
                        "content": record.content,
                        "tags": serde_json::from_str::<Vec<String>>(&record.tags_json).unwrap_or_default(),
                        "scope": record.scope,
                        "scopeAgentId": record.scope_agent_id,
                        "score": hit.score,
                        "updatedAt": record.updated_at,
                    }));
                    continue;
                }
                if let Some(key) = extract_kv_key_from_vector_id(&hit.memory_id) {
                    if let Ok(Some(record)) = crate::storage::workspaces::get_workspace_kv_memory(
                        &conn,
                        &workspace_id,
                        key,
                    ) {
                        let value = serde_json::from_str::<Value>(&record.value_json)
                            .unwrap_or_else(|_| Value::String(record.value_json.clone()));
                        results.push(json!({
                            "memoryType": "kv",
                            "searchMode": "semantic",
                            "memoryId": hit.memory_id,
                            "key": record.memory_key,
                            "value": value,
                            "score": hit.score,
                            "updatedAt": record.updated_at,
                        }));
                    }
                    continue;
                }
                if let Ok(Some((source_kind, title, content))) =
                    crate::storage::core_memory::fetch_search_text(&conn, &hit.memory_id)
                {
                    results.push(json!({
                        "memoryType": source_kind,
                        "searchMode": "semantic",
                        "memoryId": hit.memory_id,
                        "title": title,
                        "content": content,
                        "score": hit.score,
                    }));
                    continue;
                }
                if let Ok(Some((source_kind, bucket, text, origin_kind, tags))) =
                    crate::storage::user_memory::fetch_search_text(&conn, &hit.memory_id)
                {
                    results.push(json!({
                        "memoryType": source_kind,
                        "searchMode": "semantic",
                        "memoryId": hit.memory_id,
                        "bucket": bucket,
                        "content": text,
                        "originKind": origin_kind,
                        "tags": tags,
                        "score": hit.score,
                    }));
                }
                // Check for saved:: memory entries (from memory_save tool)
                if hit.memory_id.starts_with("saved::") {
                    let meta = hit
                        .metadata_json
                        .as_deref()
                        .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                        .unwrap_or(serde_json::json!({}));
                    let saved_tags: Vec<String> = meta
                        .get("tags")
                        .and_then(|t| {
                            serde_json::from_str(&serde_json::to_string(t).unwrap_or_default()).ok()
                        })
                        .unwrap_or_default();
                    results.push(json!({
                        "memoryType": "saved",
                        "searchMode": "semantic",
                        "memoryId": hit.memory_id,
                        "content": hit.content_text.as_deref().unwrap_or(""),
                        "tags": saved_tags,
                        "metadata": meta,
                        "score": hit.score,
                        "updatedAt": hit.updated_at,
                    }));
                }
            }
            Some(results)
        } else {
            None
        }
    } else {
        None
    };

    let (results, search_mode) = if let Some(results) = semantic_results {
        (results, "semantic")
    } else {
        (
            memory_search_keyword_fallback(&conn, &workspace_id, &body.query, body.limit)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?,
            "keyword_fallback",
        )
    };

    Ok(Json(json!({
        "ok": true,
        "searchMode": search_mode,
        "results": results,
        "total": results.len(),
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct MemoryStoreRequest {
    key: String,
    value: Value,
    #[serde(default)]
    workspace_id: Option<String>,
}

async fn memory_store_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<MemoryStoreRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;
    let session = resolve_proxy_session(&state, &token)?;
    let workspace_id = resolve_memory_workspace(&state, &token, body.workspace_id.as_deref())?;
    let conn =
        crate::storage_conn(&app_handle).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    ensure_memory_workspace_namespace(&conn, &workspace_id, session.caller_agent_id.as_deref())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let value_json = serde_json::to_string(&body.value).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("序列化 K/V 记忆失败: {e}"),
        )
    })?;
    let text_value = build_text_value_for_memory(&body.value);
    let record = crate::storage::workspaces::upsert_workspace_kv_memory(
        &conn,
        &workspace_id,
        &body.key,
        &value_json,
        &text_value,
    )
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if let Some(registry) = EMBEDDING_REGISTRY.get() {
        let registry = registry.clone();
        let workspace_id_clone = workspace_id.clone();
        let key_clone = body.key.clone();
        let text_to_embed = format!("{}\n{}", body.key, text_value);
        tauri::async_runtime::spawn(async move {
            let provider = {
                let guard = registry.read().await;
                guard.default_provider()
            };
            if let Some(provider) = provider {
                match provider.embed(vec![text_to_embed]).await {
                    Ok(embeddings) => {
                        if let Some(embedding) = embeddings.into_iter().next() {
                            if let Some(app_handle) = APP_HANDLE.get() {
                                if let Ok(conn) = crate::storage_conn(app_handle) {
                                    let vector_id = format!("vec_{}", Uuid::new_v4().simple());
                                    let _ = crate::memory_vector::upsert_vector(
                                        &conn,
                                        &vector_id,
                                        &kv_memory_vector_id(&key_clone),
                                        &workspace_id_clone,
                                        &embedding,
                                        provider.id(),
                                    );
                                }
                            }
                        }
                    }
                    Err(error) => log::warn!("生成 K/V 记忆嵌入失败: {error}"),
                }
            }
        });
    }

    Ok(Json(json!({
        "ok": true,
        "workspaceId": record.workspace_id,
        "key": record.memory_key,
        "value": body.value,
        "updatedAt": record.updated_at,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct MemorySaveRequest {
    text: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    metadata: std::collections::HashMap<String, String>,
    #[serde(default)]
    workspace_id: Option<String>,
}

async fn memory_save_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<MemorySaveRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;
    let session = resolve_proxy_session(&state, &token)?;
    let workspace_id = resolve_memory_workspace(&state, &token, body.workspace_id.as_deref())?;
    let _conn =
        crate::storage_conn(&app_handle).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let memory_id = format!("saved::{}", uuid::Uuid::new_v4().simple());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default();

    // Build metadata
    let mut meta = serde_json::Map::new();
    meta.insert(
        "timestamp".to_string(),
        serde_json::Value::Number(now.into()),
    );
    meta.insert(
        "workspaceId".to_string(),
        serde_json::Value::String(workspace_id.clone()),
    );
    if let Some(agent_id) = session.caller_agent_id.as_deref() {
        meta.insert(
            "agentId".to_string(),
            serde_json::Value::String(agent_id.to_string()),
        );
    }
    for (k, v) in &body.metadata {
        meta.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    if !body.tags.is_empty() {
        meta.insert("tags".to_string(), serde_json::json!(body.tags));
    }
    let metadata_json = serde_json::Value::Object(meta).to_string();
    let tags_json = serde_json::json!(body.tags).to_string();

    if let Some(registry) = EMBEDDING_REGISTRY.get() {
        let registry = registry.clone();
        let ws_id = workspace_id.clone();
        let mid = memory_id.clone();
        let text = body.text.clone();
        let meta_str = metadata_json.clone();
        let tags_str = tags_json.clone();
        tauri::async_runtime::spawn(async move {
            let provider = {
                let guard = registry.read().await;
                guard.default_provider()
            };
            if let Some(provider) = provider {
                match provider.embed(vec![text.clone()]).await {
                    Ok(embeddings) => {
                        if let Some(embedding) = embeddings.into_iter().next() {
                            if let Some(app_handle) = APP_HANDLE.get() {
                                if let Ok(conn) = crate::storage_conn(app_handle) {
                                    let vector_id =
                                        format!("vec_{}", uuid::Uuid::new_v4().simple());
                                    let _ = crate::memory_vector::upsert_vector_with_meta(
                                        &conn,
                                        &vector_id,
                                        &mid,
                                        &ws_id,
                                        &embedding,
                                        provider.id(),
                                        Some(&meta_str),
                                        Some(&text),
                                        Some(&tags_str),
                                    );
                                }
                            }
                        }
                    }
                    Err(error) => log::warn!("memory_save 嵌入失败: {error}"),
                }
            }
        });
    }

    Ok(Json(json!({
        "ok": true,
        "memoryId": memory_id,
        "workspaceId": workspace_id,
        "text": body.text,
        "tags": body.tags,
        "timestamp": now,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct MemoryGetRequest {
    key: String,
    #[serde(default)]
    workspace_id: Option<String>,
}

async fn memory_get_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<MemoryGetRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;
    let workspace_id = resolve_memory_workspace(&state, &token, body.workspace_id.as_deref())?;
    let conn =
        crate::storage_conn(&app_handle).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let record =
        crate::storage::workspaces::get_workspace_kv_memory(&conn, &workspace_id, &body.key)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let Some(record) = record else {
        return Ok(Json(json!({
            "ok": false,
            "error": format!("K/V 记忆 {} 不存在", body.key),
        })));
    };
    let value = serde_json::from_str::<Value>(&record.value_json)
        .unwrap_or_else(|_| Value::String(record.value_json.clone()));

    Ok(Json(json!({
        "ok": true,
        "workspaceId": record.workspace_id,
        "key": record.memory_key,
        "value": value,
        "updatedAt": record.updated_at,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct MemoryListRequest {
    #[serde(default = "default_memory_list_limit")]
    limit: i64,
    #[serde(default)]
    workspace_id: Option<String>,
}

fn default_memory_list_limit() -> i64 {
    100
}

async fn memory_list_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<MemoryListRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;
    let workspace_id = resolve_memory_workspace(&state, &token, body.workspace_id.as_deref())?;
    let conn =
        crate::storage_conn(&app_handle).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let records =
        crate::storage::workspaces::list_workspace_kv_memories(&conn, &workspace_id, body.limit)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let entries = records
        .into_iter()
        .map(|record| {
            let value = serde_json::from_str::<Value>(&record.value_json)
                .unwrap_or_else(|_| Value::String(record.value_json));
            json!({
                "key": record.memory_key,
                "value": value,
                "updatedAt": record.updated_at,
            })
        })
        .collect::<Vec<_>>();
    let total = entries.len();

    Ok(Json(json!({
        "ok": true,
        "workspaceId": workspace_id,
        "entries": entries,
        "total": total,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct MemoryForgetRequest {
    key: String,
    #[serde(default)]
    workspace_id: Option<String>,
}

async fn memory_forget_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<MemoryForgetRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;
    let workspace_id = resolve_memory_workspace(&state, &token, body.workspace_id.as_deref())?;
    let conn =
        crate::storage_conn(&app_handle).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let _ =
        crate::memory_vector::delete_vector_by_memory_id(&conn, &kv_memory_vector_id(&body.key));
    crate::storage::workspaces::delete_workspace_kv_memory(&conn, &workspace_id, &body.key)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(json!({
        "ok": true,
        "workspaceId": workspace_id,
        "key": body.key,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct MemoryDeleteRequest {
    memory_id: String,
    #[serde(default)]
    workspace_id: Option<String>,
}

async fn memory_delete_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<MemoryDeleteRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;
    let workspace_id = resolve_memory_workspace(&state, &token, body.workspace_id.as_deref())?;
    let conn =
        crate::storage_conn(&app_handle).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Always delete the vector first (works for all memory types)
    let vector_deleted = crate::memory_vector::delete_vector_by_memory_id(&conn, &body.memory_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // For standalone vector entries (saved::, chat-turn::, kv::), vector deletion is sufficient
    if body.memory_id.starts_with("saved::")
        || body.memory_id.starts_with("chat-turn::")
        || body.memory_id.starts_with("kv::")
    {
        return Ok(Json(json!({
            "ok": true,
            "memoryId": body.memory_id,
            "vectorDeleted": vector_deleted,
        })));
    }

    // For workspace memories, also delete the workspace_memories record
    crate::storage::workspaces::delete_workspace_memory(&conn, &workspace_id, &body.memory_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(json!({
        "ok": true,
        "memoryId": body.memory_id,
        "vectorDeleted": vector_deleted,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct ChatSearchRequest {
    query: String,
    #[serde(default = "default_chat_search_limit")]
    limit: i64,
    #[serde(default)]
    workspace_id: Option<String>,
    #[serde(default)]
    time_range_start: Option<i64>,
    #[serde(default)]
    time_range_end: Option<i64>,
}

fn default_chat_search_limit() -> i64 {
    20
}

async fn chat_search_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<ChatSearchRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;
    let conn =
        crate::storage_conn(&app_handle).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let session = resolve_proxy_session(&state, &token)?;
    let workspace_id = body
        .workspace_id
        .as_deref()
        .or(session.workspace_id.as_deref());

    // Try semantic vector search first
    let mut semantic_results: Vec<Value> = Vec::new();
    let mut search_mode = "keyword";

    if let Some(registry) = get_embedding_registry() {
        let provider = {
            let guard = registry.read().await;
            guard.default_provider()
        };
        if let Some(provider) = provider {
            let embed_result = provider.embed(vec![body.query.clone()]).await;
            match embed_result {
                Ok(embeddings) => {
                    if let Some(query_embedding) = embeddings.into_iter().next() {
                        // Search chat turn vectors across all relevant workspaces
                        let ws_ids: Vec<String> = if let Some(ws) = workspace_id {
                            vec![ws.to_string()]
                        } else {
                            // Search across all workspaces that have chat vectors
                            let mut stmt = conn
                                .prepare("SELECT DISTINCT workspace_id FROM memory_vectors WHERE memory_id LIKE 'chat-turn::%'")
                                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("查询 workspace 列表失败: {e}")))?;
                            let rows =
                                stmt.query_map([], |row| row.get::<_, String>(0))
                                    .map_err(|e| {
                                        (
                                            StatusCode::INTERNAL_SERVER_ERROR,
                                            format!("读取 workspace 列表失败: {e}"),
                                        )
                                    })?;
                            rows.filter_map(|r| r.ok()).collect()
                        };

                        if ws_ids.is_empty() {
                            log::debug!("chat_search: 无 chat-turn 向量，跳过语义搜索");
                        }

                        for ws in &ws_ids {
                            if let Ok(hits) =
                                crate::memory_vector::vector_search::standalone_vector_search(
                                    &conn,
                                    ws,
                                    &query_embedding,
                                    body.limit as usize,
                                    0.3,
                                    "chat-turn::",
                                    body.time_range_start,
                                    body.time_range_end,
                                    None,
                                )
                            {
                                for hit in hits {
                                    let meta = hit
                                        .metadata_json
                                        .as_deref()
                                        .and_then(|m| {
                                            serde_json::from_str::<serde_json::Value>(m).ok()
                                        })
                                        .unwrap_or(serde_json::json!({}));
                                    let content = hit.content_text.as_deref().unwrap_or("");
                                    let parts: Vec<&str> = content.splitn(2, '\n').collect();
                                    let prompt = parts.first().unwrap_or(&"").to_string();
                                    let answer = parts.get(1).unwrap_or(&"").to_string();

                                    semantic_results.push(json!({
                                        "sessionId": meta.get("sessionId").and_then(|v| v.as_str()).unwrap_or(""),
                                        "sessionTitle": meta.get("sessionTitle").and_then(|v| v.as_str()).unwrap_or(""),
                                        "turnId": meta.get("turnId").and_then(|v| v.as_str()).unwrap_or(""),
                                        "turnIndex": meta.get("turnIndex").and_then(|v| v.as_i64()).unwrap_or(0),
                                        "prompt": prompt,
                                        "answer": answer,
                                        "excerpt": build_chat_search_excerpt(&prompt, &answer, &body.query),
                                        "createdAt": meta.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(hit.updated_at),
                                        "completedAt": meta.get("completedAt").and_then(|v| v.as_i64()),
                                        "workspaceId": meta.get("workspaceId").and_then(|v| v.as_str()).unwrap_or(ws),
                                        "speakerAgentId": meta.get("agentId").and_then(|v| v.as_str()).unwrap_or(""),
                                        "score": hit.score,
                                        "searchMode": "semantic",
                                    }));
                                }
                            }
                        }

                        if !semantic_results.is_empty() {
                            search_mode = "semantic";
                            semantic_results.sort_by(|a, b| {
                                b.get("score")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0)
                                    .partial_cmp(
                                        &a.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    )
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            });
                            semantic_results.truncate(body.limit as usize);
                        }
                    }
                }
                Err(error) => {
                    log::warn!("chat_search 嵌入查询失败: {error}，降级到关键词搜索");
                }
            }
        } else {
            log::debug!("chat_search: embedding provider 未初始化，使用关键词搜索");
        }
    }

    // Fallback to keyword search if no semantic results
    let results = if semantic_results.is_empty() {
        let hits = crate::storage::chat_history::search_chat_turns(
            &conn,
            &body.query,
            body.limit,
            workspace_id,
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
        hits.into_iter()
            .map(|hit| {
                json!({
                    "sessionId": hit.session_id,
                    "sessionTitle": hit.session_title,
                    "turnId": hit.turn_id,
                    "turnIndex": hit.turn_index,
                    "excerpt": build_chat_search_excerpt(&hit.prompt, &hit.answer, &body.query),
                    "prompt": hit.prompt,
                    "answer": hit.answer,
                    "createdAt": hit.created_at,
                    "workspaceId": hit.workspace_id,
                    "speakerAgentId": hit.speaker_agent_id,
                    "searchMode": "keyword",
                })
            })
            .collect::<Vec<_>>()
    } else {
        semantic_results
    };

    let total = results.len();
    Ok(Json(json!({
        "ok": true,
        "results": results,
        "total": total,
        "searchMode": search_mode,
    })))
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskCreateRequest {
    goal: String,
    #[serde(default)]
    title: String,
    #[serde(default = "default_task_create_task_type")]
    task_type: String,
    schedule_type: String,
    #[serde(default)]
    timezone: String,
    #[serde(default)]
    interval_minutes: Option<i64>,
    #[serde(default)]
    daily_times: Vec<String>,
    #[serde(default)]
    weekly_days: Vec<u32>,
    #[serde(default)]
    monthly_days: Vec<u32>,
    #[serde(default)]
    run_at_ms: Option<i64>,
    #[serde(default)]
    result_in_new_session: bool,
}

fn default_task_create_task_type() -> String {
    "agent_prompt".to_string()
}

async fn task_create_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<TaskCreateRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;
    let session = resolve_proxy_session(&state, &token)?;
    let agent_id = session
        .caller_agent_id
        .clone()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "缺少 agent_id".to_string()))?;
    let source_session_id = session
        .session_id
        .clone()
        .unwrap_or_else(|| "unknown_session".to_string());

    let input = crate::agent_tasks::AgentTaskCreateInput {
        goal: body.goal,
        title: body.title,
        task_type: body.task_type,
        schedule_type: body.schedule_type,
        timezone: body.timezone,
        interval_minutes: body.interval_minutes,
        daily_times: body.daily_times,
        weekly_days: body.weekly_days,
        monthly_days: body.monthly_days,
        run_at_ms: body.run_at_ms,
        result_in_new_session: body.result_in_new_session,
    };

    let task = tauri::async_runtime::spawn_blocking(move || {
        crate::agent_tasks::create_task(&app_handle, &agent_id, &source_session_id, &input)
    })
    .await
    .map_err(|e| {
        log::error!("task_create: spawn_blocking JoinError: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "data": task,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskListRequest {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    schedule_type: Option<String>,
}

async fn task_list_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<TaskListRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;
    let session = resolve_proxy_session(&state, &token)?;
    let agent_id = session.caller_agent_id.as_deref();

    let tasks = crate::agent_tasks::list_tasks_filtered(
        &app_handle,
        agent_id,
        body.status.as_deref(),
        body.schedule_type.as_deref(),
    )
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "data": tasks,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskDetailRequest {
    task_id: String,
}

async fn task_detail_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    Json(body): Json<TaskDetailRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app_handle = APP_HANDLE.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "AppHandle 尚未注入".to_string(),
        )
    })?;
    let _session = resolve_proxy_session(&state, &token)?;

    let task = crate::agent_tasks::get_task_detail(&app_handle, &body.task_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;

    match task {
        Some(item) => Ok(Json(serde_json::json!({
            "ok": true,
            "data": item,
        }))),
        None => Ok(Json(serde_json::json!({
            "ok": false,
            "error": "未找到对应的定时任务",
        }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_capabilities::static_capability_policy;
    use crate::agents::{AgentHeartbeatConfig, AgentRecord};

    fn test_agent(id: &str, name: &str) -> AgentRecord {
        AgentRecord {
            id: id.to_string(),
            name: name.to_string(),
            summary: String::new(),
            description: String::new(),
            trigger_condition: String::new(),
            manual_trigger_only: false,
            system_prompt: String::new(),
            capability_policy: static_capability_policy(),
            skill_ids: Vec::new(),
            allowed_tool_ids: Vec::new(),
            default_provider_id: String::new(),
            default_model: String::new(),
            is_builtin: false,
            is_archived: false,
            execution_mode: "single".to_string(),
            collaboration_config: None,
            accent_color: None,
            avatar_uri: None,
            bot_configs: HashMap::new(),
            heartbeat_config: AgentHeartbeatConfig::default(),
            scenario_llm_config: None,
            agent_loop_config: None,
            created_at: 0,
            updated_at: 0,
        }
    }

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
    fn restrict_harness_uses_agent_allowed_tools_as_active_tools() {
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

        let restricted = restrict_harness_to_agent_tools(
            harness,
            &root,
            &[
                "read_file".to_string(),
                "grep".to_string(),
                "glob".to_string(),
            ],
        )
        .expect("restrict harness");
        assert_eq!(
            restricted.definition.active_tools,
            vec!["read".to_string(), "grep".to_string(), "find".to_string()]
        );
        assert!(restricted.file_path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restrict_harness_sets_sentinel_when_no_tools_allowed() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("create root");
        ensure_agent_runtime_scaffold(&root).expect("scaffold");
        let harness = select_harness(&root, "single", Some("hello")).expect("select harness");

        let restricted =
            restrict_harness_to_agent_tools(harness, &root, &[]).expect("restrict harness");
        assert_eq!(
            restricted.definition.active_tools,
            vec!["__nineclaw_no_tools_allowed__".to_string()]
        );
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

    #[test]
    fn resolve_delegate_agent_matches_workspace_role_and_slug() {
        let color = test_agent("agent_1", "ColorMaster");
        let psych = test_agent("agent_2", "PsychMarketer");
        let candidates = vec![&color, &psych];
        let role_hints = HashMap::from([(
            color.id.clone(),
            DelegateRoleHint {
                role: "colorist".to_string(),
            },
        )]);

        let matched = resolve_delegate_agent(&candidates, &role_hints, "agent-colorist", "")
            .expect("match role slug");
        assert_eq!(matched.id, color.id);

        let matched = resolve_delegate_agent(&candidates, &role_hints, "ColorMaster", "")
            .expect("match name");
        assert_eq!(matched.id, color.id);
    }

    #[test]
    fn format_delegate_candidates_includes_role_hint() {
        let color = test_agent("agent_1", "ColorMaster");
        let psych = test_agent("agent_2", "PsychMarketer");
        let candidates = vec![&color, &psych];
        let role_hints = HashMap::from([(
            psych.id.clone(),
            DelegateRoleHint {
                role: "marketer".to_string(),
            },
        )]);

        let formatted = format_delegate_candidates(&candidates, &role_hints);
        assert!(formatted.contains("ColorMaster(agent_1)"));
        assert!(formatted.contains("PsychMarketer(agent_2) role=marketer"));
    }

    #[test]
    fn resolve_delegate_agent_can_match_by_description_and_task() {
        let mut color = test_agent("agent_1", "ColorMaster");
        color.description =
            "专注于品牌配色策略、Design Token 体系与 CSS/Tailwind 落地。".to_string();
        let mut psych = test_agent("agent_2", "PsychMarketer");
        psych.description = "专注于消费心理、转化文案和增长策略。".to_string();
        let candidates = vec![&color, &psych];

        let matched = resolve_delegate_agent(
            &candidates,
            &HashMap::new(),
            "帮我找一个适合做配色方案的智能体",
            "需要输出网站品牌配色、design token 和 tailwind 变量",
        )
        .expect("match by description");
        assert_eq!(matched.id, color.id);
    }

    #[test]
    fn direct_chat_delegate_scope_is_global_agents() {
        assert_eq!(delegate_scope_label(None), "当前直接聊天可调用的智能体");
        assert_eq!(delegate_scope_label(Some("workspace-1")), "当前团队成员");
    }

    #[test]
    fn resolve_typebox_import_path_supports_unscoped_typebox_layout() {
        let root = temp_root();
        let pi_dir = root.join("runtime");
        let pi_path = pi_dir.join("pi");
        let candidate = root
            .join("pi-package")
            .join("node_modules")
            .join("typebox")
            .join("build")
            .join("index.mjs");

        fs::create_dir_all(candidate.parent().expect("candidate parent"))
            .expect("create candidate");
        fs::create_dir_all(&pi_dir).expect("create pi dir");
        fs::write(&pi_path, "#!/bin/sh\n").expect("write pi");
        fs::write(&candidate, "export {};\n").expect("write candidate");

        let resolved = resolve_typebox_import_path(&pi_path).expect("resolve path");
        let expected = fs::canonicalize(&candidate).expect("canonical candidate");
        assert_eq!(resolved, expected);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_typebox_import_path_supports_scoped_typebox_layout() {
        let root = temp_root();
        let pi_dir = root.join("runtime");
        let pi_path = pi_dir.join("pi");
        let candidate = root
            .join("pi-package")
            .join("node_modules")
            .join("@sinclair")
            .join("typebox")
            .join("build")
            .join("index.mjs");

        fs::create_dir_all(candidate.parent().expect("candidate parent"))
            .expect("create candidate");
        fs::create_dir_all(&pi_dir).expect("create pi dir");
        fs::write(&pi_path, "#!/bin/sh\n").expect("write pi");
        fs::write(&candidate, "export {};\n").expect("write candidate");

        let resolved = resolve_typebox_import_path(&pi_path).expect("resolve path");
        let expected = fs::canonicalize(&candidate).expect("canonical candidate");
        assert_eq!(resolved, expected);

        let _ = fs::remove_dir_all(root);
    }
}
