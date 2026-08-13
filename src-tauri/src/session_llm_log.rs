//! Always-on, session-scoped LLM text logs.
//!
//! This is intentionally simpler than `llm_trace`: it writes human-readable
//! Markdown per session so missed structured trace events still leave a durable
//! audit trail.

use crate::agent_workspace;
use crate::pi_usage::PiTokenUsagePayload;
use crate::workspace_fs;
use chrono::{Local, TimeZone};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle};

const PREVIEW_CHARS: usize = 220;
const MAX_APPEND_CHARS: usize = 240_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLlmLogInfo {
    pub workspace_id: Option<String>,
    pub session_id: String,
    pub path: String,
    pub size: u64,
    pub modified_at: i64,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLlmLogDetail {
    pub info: SessionLlmLogInfo,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLlmLogEvent {
    pub workspace_id: Option<String>,
    pub session_id: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct StartLog {
    pub workspace_id: Option<String>,
    pub session_id: String,
    pub source: String,
    pub channel_id: Option<String>,
    pub user_id: Option<String>,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub prompt: String,
    pub system_prompts: Vec<(String, String)>,
    pub attachments_count: usize,
    pub images_count: usize,
    pub reused_process: Option<bool>,
    pub runtime_session_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolLog {
    pub workspace_id: Option<String>,
    pub session_id: String,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub args: Option<String>,
    pub result: Option<String>,
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct FinishLog {
    pub workspace_id: Option<String>,
    pub session_id: String,
    pub status: String,
    pub response: Option<String>,
    pub error: Option<String>,
    pub usage: Option<PiTokenUsagePayload>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub response_id: Option<String>,
}

pub struct SessionLlmLogGuard {
    app: Option<AppHandle>,
    workspace_id: Option<String>,
    session_id: String,
    finished: bool,
}

impl SessionLlmLogGuard {
    pub fn new(app: Option<&AppHandle>, workspace_id: Option<String>, session_id: String) -> Self {
        Self {
            app: app.cloned(),
            workspace_id,
            session_id,
            finished: false,
        }
    }

    pub fn finish_done(
        &mut self,
        response: Option<String>,
        usage: Option<PiTokenUsagePayload>,
        provider: Option<String>,
        model: Option<String>,
        response_id: Option<String>,
    ) {
        if self.finished {
            return;
        }
        self.finished = true;
        let _ = record_finish(
            self.app.as_ref(),
            FinishLog {
                workspace_id: self.workspace_id.clone(),
                session_id: self.session_id.clone(),
                status: "done".to_string(),
                response,
                error: None,
                usage,
                provider,
                model,
                response_id,
            },
        );
    }

    pub fn finish_error(&mut self, status: &str, error: String, response: Option<String>) {
        if self.finished {
            return;
        }
        self.finished = true;
        let _ = record_finish(
            self.app.as_ref(),
            FinishLog {
                workspace_id: self.workspace_id.clone(),
                session_id: self.session_id.clone(),
                status: status.to_string(),
                response,
                error: Some(error),
                usage: None,
                provider: None,
                model: None,
                response_id: None,
            },
        );
    }
}

impl Drop for SessionLlmLogGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let _ = record_finish(
            self.app.as_ref(),
            FinishLog {
                workspace_id: self.workspace_id.clone(),
                session_id: self.session_id.clone(),
                status: "aborted".to_string(),
                response: None,
                error: Some("调用流程提前结束（未显式标记完成）".to_string()),
                usage: None,
                provider: None,
                model: None,
                response_id: None,
            },
        );
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn format_ts(ts_ms: i64) -> String {
    Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .unwrap_or_else(Local::now)
        .format("%Y-%m-%d %H:%M:%S%.3f %:z")
        .to_string()
}

fn normalize_workspace_id(workspace_id: Option<&str>) -> Option<String> {
    workspace_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_session_id(session_id: &str) -> Result<String, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session id 不能为空".to_string());
    }
    Ok(trimmed.to_string())
}

fn safe_file_stem(value: &str) -> String {
    let mut out = String::new();
    for c in value.trim().chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
            out.push(c);
        } else {
            out.push('_');
        }
        if out.len() >= 160 {
            break;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "session".to_string()
    } else {
        trimmed
    }
}

fn log_dir(workspace_id: Option<&str>) -> Result<PathBuf, String> {
    let dir = if let Some(wid) = normalize_workspace_id(workspace_id) {
        workspace_fs::team_root(&wid)?
            .join(".debug")
            .join("session-logs")
    } else {
        agent_workspace::resolve_workspace_root()?
            .join(".debug")
            .join("standalone")
            .join("session-logs")
    };
    fs::create_dir_all(&dir).map_err(|error| format!("创建 session 日志目录失败: {error}"))?;
    Ok(dir)
}

fn log_path(workspace_id: Option<&str>, session_id: &str) -> Result<PathBuf, String> {
    let sid = normalize_session_id(session_id)?;
    Ok(log_dir(workspace_id)?.join(format!("{}.md", safe_file_stem(&sid))))
}

fn emit_updated(
    app: Option<&AppHandle>,
    workspace_id: Option<&str>,
    session_id: &str,
    path: &PathBuf,
) {
    let Some(app) = app else {
        return;
    };
    let payload = SessionLlmLogEvent {
        workspace_id: normalize_workspace_id(workspace_id),
        session_id: session_id.to_string(),
        path: path.to_string_lossy().to_string(),
    };
    crate::emit_safe::emit_safe(app, "session.llm_log.updated", payload);
}

fn append_to_file(
    app: Option<&AppHandle>,
    workspace_id: Option<&str>,
    session_id: &str,
    body: &str,
) -> Result<PathBuf, String> {
    let path = log_path(workspace_id, session_id)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("打开 session 日志失败: {error}"))?;
    file.write_all(body.as_bytes())
        .map_err(|error| format!("写入 session 日志失败: {error}"))?;
    emit_updated(app, workspace_id, session_id, &path);
    Ok(path)
}

fn append_section(out: &mut String, heading: &str, content: &str) {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return;
    }
    out.push_str("\n### ");
    out.push_str(heading);
    out.push_str("\n\n```text\n");
    out.push_str(&truncate_chars(trimmed, MAX_APPEND_CHARS));
    if !trimmed.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("```\n");
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            out.push_str("\n\n[truncated]\n");
            break;
        }
        out.push(ch);
    }
    out
}

fn push_meta(out: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        out.push_str("- ");
        out.push_str(key);
        out.push_str(": `");
        out.push_str(&value.replace('`', "\\`"));
        out.push_str("`\n");
    }
}

fn cache_hit_rate_label(usage: Option<&PiTokenUsagePayload>) -> String {
    let Some(usage) = usage else {
        return "unknown (usage unavailable)".to_string();
    };
    let input = usage.input_tokens.unwrap_or(0);
    let cache_read = usage.cache_read_tokens.unwrap_or(0);
    let cache_write = usage.cache_write_tokens.unwrap_or(0);
    let prompt_side_total = input + cache_read + cache_write;
    if prompt_side_total == 0 {
        return "unknown (prompt tokens unavailable)".to_string();
    }
    let rate = cache_read as f64 / prompt_side_total as f64 * 100.0;
    format!("{rate:.1}% (cacheRead {cache_read} / promptSide {prompt_side_total})")
}

fn parse_backtick_meta(content: &str, key: &str) -> Option<String> {
    let prefix = format!("- {key}: `");
    for line in content.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(&prefix) else {
            continue;
        };
        let Some(value) = rest.strip_suffix('`') else {
            continue;
        };
        let normalized = value.replace("\\`", "`").trim().to_string();
        if !normalized.is_empty() {
            return Some(normalized);
        }
    }
    None
}

pub fn infer_workspace_id_from_session(session_id: &str) -> Option<String> {
    let app = crate::managed_runtime::injected_app_handle()?;
    let conn = crate::history_app_state::storage_conn(&app).ok()?;
    let session = crate::storage::chat_history::get_chat_session(&conn, session_id).ok()??;
    session
        .workspace_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn record_start(app: Option<&AppHandle>, log: StartLog) -> Result<PathBuf, String> {
    let session_id = normalize_session_id(&log.session_id)?;
    let now = now_ms();
    let mut body = String::new();
    body.push_str("\n---\n\n");
    body.push_str("## LLM Turn Start - ");
    body.push_str(&format_ts(now));
    body.push_str("\n\n");
    push_meta(&mut body, "source", Some(&log.source));
    push_meta(&mut body, "workspace", log.workspace_id.as_deref());
    push_meta(&mut body, "session", Some(&session_id));
    push_meta(&mut body, "channel", log.channel_id.as_deref());
    push_meta(&mut body, "user", log.user_id.as_deref());
    push_meta(
        &mut body,
        "agent",
        log.agent_name.as_deref().or(log.agent_id.as_deref()),
    );
    push_meta(&mut body, "agentId", log.agent_id.as_deref());
    push_meta(&mut body, "provider", log.provider.as_deref());
    push_meta(&mut body, "model", log.model.as_deref());
    push_meta(
        &mut body,
        "runtimeSession",
        log.runtime_session_path.as_deref(),
    );
    if let Some(reused) = log.reused_process {
        push_meta(
            &mut body,
            "reusedProcess",
            Some(if reused { "true" } else { "false" }),
        );
    }
    if log.attachments_count > 0 || log.images_count > 0 {
        body.push_str("- attachments: `");
        body.push_str(&log.attachments_count.to_string());
        body.push_str("`, images: `");
        body.push_str(&log.images_count.to_string());
        body.push_str("`\n");
    }
    append_section(&mut body, "User Prompt", &log.prompt);
    for (label, content) in log.system_prompts {
        append_section(&mut body, &format!("System Prompt - {label}"), &content);
    }
    append_to_file(app, log.workspace_id.as_deref(), &session_id, &body)
}

pub fn record_tool_start(app: Option<&AppHandle>, log: ToolLog) -> Result<PathBuf, String> {
    let session_id = normalize_session_id(&log.session_id)?;
    let mut body = String::new();
    body.push_str("\n### Tool Start - ");
    body.push_str(&format_ts(now_ms()));
    body.push_str("\n\n");
    push_meta(&mut body, "session", Some(&session_id));
    push_meta(&mut body, "tool", log.tool_name.as_deref());
    push_meta(&mut body, "toolCallId", log.tool_call_id.as_deref());
    if let Some(args) = log.args.as_deref() {
        append_section(&mut body, "Tool Args", args);
    }
    append_to_file(app, log.workspace_id.as_deref(), &session_id, &body)
}

pub fn record_tool_end(app: Option<&AppHandle>, log: ToolLog) -> Result<PathBuf, String> {
    let session_id = normalize_session_id(&log.session_id)?;
    let mut body = String::new();
    body.push_str("\n### Tool End - ");
    body.push_str(&format_ts(now_ms()));
    body.push_str("\n\n");
    push_meta(&mut body, "session", Some(&session_id));
    push_meta(&mut body, "tool", log.tool_name.as_deref());
    push_meta(&mut body, "toolCallId", log.tool_call_id.as_deref());
    if let Some(is_error) = log.is_error {
        push_meta(
            &mut body,
            "isError",
            Some(if is_error { "true" } else { "false" }),
        );
    }
    if let Some(result) = log.result.as_deref() {
        append_section(&mut body, "Tool Result", result);
    }
    append_to_file(app, log.workspace_id.as_deref(), &session_id, &body)
}

pub fn record_finish(app: Option<&AppHandle>, log: FinishLog) -> Result<PathBuf, String> {
    let session_id = normalize_session_id(&log.session_id)?;
    let cache_hit_rate = cache_hit_rate_label(log.usage.as_ref());
    let mut body = String::new();
    body.push_str("\n## LLM Turn Finish - ");
    body.push_str(&format_ts(now_ms()));
    body.push_str("\n\n");
    push_meta(&mut body, "session", Some(&session_id));
    push_meta(&mut body, "status", Some(&log.status));
    push_meta(&mut body, "provider", log.provider.as_deref());
    push_meta(&mut body, "model", log.model.as_deref());
    push_meta(&mut body, "responseId", log.response_id.as_deref());
    if let Some(usage) = log.usage {
        body.push_str("- usage: input `");
        body.push_str(&usage.input_tokens.unwrap_or_default().to_string());
        body.push_str("`, output `");
        body.push_str(&usage.output_tokens.unwrap_or_default().to_string());
        body.push_str("`, cacheRead `");
        body.push_str(&usage.cache_read_tokens.unwrap_or_default().to_string());
        body.push_str("`, cacheWrite `");
        body.push_str(&usage.cache_write_tokens.unwrap_or_default().to_string());
        body.push_str("`, total `");
        body.push_str(&usage.total_tokens.unwrap_or_default().to_string());
        body.push_str("`\n");
    }
    push_meta(&mut body, "cacheHitRateApprox", Some(&cache_hit_rate));
    push_meta(
        &mut body,
        "cacheHitFormula",
        Some("cacheRead / (input + cacheRead + cacheWrite)"),
    );
    if let Some(error) = log.error.as_deref() {
        append_section(&mut body, "Error", error);
    }
    if let Some(response) = log.response.as_deref() {
        append_section(&mut body, "Assistant Response", response);
    }
    append_to_file(app, log.workspace_id.as_deref(), &session_id, &body)
}

fn file_info(
    workspace_id: Option<&str>,
    session_id: &str,
    path: PathBuf,
) -> Result<SessionLlmLogInfo, String> {
    let meta =
        fs::metadata(&path).map_err(|error| format!("读取 session 日志信息失败: {error}"))?;
    let modified_at = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default();
    let content = fs::read_to_string(&path).unwrap_or_default();
    let display_session_id =
        parse_backtick_meta(&content, "session").unwrap_or_else(|| session_id.to_string());
    let preview = content
        .chars()
        .rev()
        .take(PREVIEW_CHARS)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .trim()
        .to_string();
    Ok(SessionLlmLogInfo {
        workspace_id: normalize_workspace_id(workspace_id),
        session_id: display_session_id,
        path: path.to_string_lossy().to_string(),
        size: meta.len(),
        modified_at,
        preview,
    })
}

pub fn get(workspace_id: Option<&str>, session_id: &str) -> Result<SessionLlmLogDetail, String> {
    let session_id = normalize_session_id(session_id)?;
    let path = log_path(workspace_id, &session_id)?;
    if !path.is_file() {
        return Err("当前 session 还没有 LLM 文本日志".to_string());
    }
    let info = file_info(workspace_id, &session_id, path.clone())?;
    let content =
        fs::read_to_string(&path).map_err(|error| format!("读取 session 日志失败: {error}"))?;
    Ok(SessionLlmLogDetail { info, content })
}

pub fn list(workspace_id: Option<&str>, limit: usize) -> Result<Vec<SessionLlmLogInfo>, String> {
    let dir = log_dir(workspace_id)?;
    let mut items = Vec::new();
    for entry in
        fs::read_dir(&dir).map_err(|error| format!("读取 session 日志目录失败: {error}"))?
    {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        let is_md = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("md"))
            .unwrap_or(false);
        if !is_md {
            continue;
        }
        let Some(stem) = path
            .file_stem()
            .and_then(|value| value.to_str())
            .map(ToOwned::to_owned)
        else {
            continue;
        };
        if let Ok(info) = file_info(workspace_id, &stem, path) {
            items.push(info);
        }
    }
    items.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    items.truncate(limit.clamp(1, 500));
    Ok(items)
}

pub fn clear(workspace_id: Option<&str>, session_id: &str) -> Result<(), String> {
    let session_id = normalize_session_id(session_id)?;
    let path = log_path(workspace_id, &session_id)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("删除 session 日志失败: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_env_test_lock;
    use std::fs;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "nineclaw-session-log-test-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn writes_and_reads_standalone_markdown_log() {
        let _guard = workspace_env_test_lock();
        let root = temp_root("standalone");
        let _ = fs::remove_dir_all(&root);
        std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);

        record_start(
            None,
            StartLog {
                workspace_id: None,
                session_id: "session:one".to_string(),
                source: "desktop".to_string(),
                channel_id: None,
                user_id: None,
                agent_id: Some("agent-a".to_string()),
                agent_name: Some("Agent A".to_string()),
                provider: Some("openai".to_string()),
                model: Some("gpt-test".to_string()),
                prompt: "hello".to_string(),
                system_prompts: vec![("base".to_string(), "system".to_string())],
                attachments_count: 0,
                images_count: 0,
                reused_process: None,
                runtime_session_path: None,
            },
        )
        .expect("write start");
        record_finish(
            None,
            FinishLog {
                workspace_id: None,
                session_id: "session:one".to_string(),
                status: "done".to_string(),
                response: Some("world".to_string()),
                error: None,
                usage: None,
                provider: None,
                model: None,
                response_id: None,
            },
        )
        .expect("write finish");

        let detail = get(None, "session:one").expect("read detail");
        assert!(detail.content.contains("LLM Turn Start"));
        assert!(detail.content.contains("hello"));
        assert!(detail.content.contains("world"));
        assert_eq!(detail.info.session_id, "session:one");
        assert!(detail.info.path.ends_with("session_one.md"));

        let _ = fs::remove_dir_all(&root);
        std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
    }

    #[test]
    fn lists_recent_logs_by_modified_time() {
        let _guard = workspace_env_test_lock();
        let root = temp_root("list");
        let _ = fs::remove_dir_all(&root);
        std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);

        record_finish(
            None,
            FinishLog {
                workspace_id: None,
                session_id: "a".to_string(),
                status: "done".to_string(),
                response: Some("A".to_string()),
                error: None,
                usage: None,
                provider: None,
                model: None,
                response_id: None,
            },
        )
        .expect("write a");
        record_finish(
            None,
            FinishLog {
                workspace_id: None,
                session_id: "b".to_string(),
                status: "error".to_string(),
                response: None,
                error: Some("B".to_string()),
                usage: None,
                provider: None,
                model: None,
                response_id: None,
            },
        )
        .expect("write b");

        let logs = list(None, 10).expect("list logs");
        assert_eq!(logs.len(), 2);
        assert!(logs.iter().any(|item| item.session_id == "a"));
        assert!(logs.iter().any(|item| item.preview.contains("B")));

        let _ = fs::remove_dir_all(&root);
        std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
    }

    #[test]
    fn finish_log_includes_cache_hit_rate_marker() {
        let _guard = workspace_env_test_lock();
        let root = temp_root("cache-hit");
        let _ = fs::remove_dir_all(&root);
        std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);

        record_finish(
            None,
            FinishLog {
                workspace_id: None,
                session_id: "cache-session".to_string(),
                status: "done".to_string(),
                response: Some("done".to_string()),
                error: None,
                usage: Some(PiTokenUsagePayload {
                    input_tokens: Some(100),
                    output_tokens: Some(40),
                    cache_read_tokens: Some(300),
                    cache_write_tokens: Some(100),
                    total_tokens: Some(540),
                }),
                provider: None,
                model: None,
                response_id: None,
            },
        )
        .expect("write cache hit finish");

        let detail = get(None, "cache-session").expect("read cache hit detail");
        assert!(detail.content.contains("cacheRead `300`"));
        assert!(detail.content.contains("cacheWrite `100`"));
        assert!(detail
            .content
            .contains("cacheHitRateApprox: `60.0% (cacheRead 300 / promptSide 500)`"));
        assert!(detail
            .content
            .contains("cacheHitFormula: `cacheRead / (input + cacheRead + cacheWrite)`"));

        let _ = fs::remove_dir_all(&root);
        std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
    }
}
