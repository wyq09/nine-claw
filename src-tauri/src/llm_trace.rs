//! LLM 调用链追踪（调试模式）。
//!
//! 开启后，主 Agent↔Pi 以及委派子会话在开始/结束时生成 `TraceEntry`，
//! 完整记录系统提示词、用户输入、响应文本、工具调用、用量等，最终落盘到
//! `teams/<workspace_id>/.debug/YYYY-MM-DD.jsonl`，供前端调试面板回看。
//!
//! 设计目标：
//! - 不改动 Pi 流式解析主循环，只在「开始」「结束」「工具调用」三个关键点落点；
//! - 文件按日切片，append-only，避免并发写冲突时覆盖历史；
//! - 同时发出 `workspace.llm_trace` Tauri 事件，用于前端面板实时刷新。

use crate::agent_workspace;
use crate::pi_usage::PiTokenUsagePayload;
use crate::workspace_fs;
use chrono::{Local, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle};
use uuid::Uuid;

/// 单个 system prompt 分段（便于前端分节展示）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSystemPromptSection {
    /// 段落标签（如 `agent_system_prompt`、`workspace_team`、`delegate_preface`）。
    pub label: String,
    pub content: String,
}

/// 结构化消息块。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceMessageBlock {
    pub id: String,
    /// `system` / `user`
    pub role: String,
    pub label: String,
    pub content: String,
}

/// 结构化响应块。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceResponseBlock {
    pub id: String,
    /// `thinking` / `output`
    pub kind: String,
    pub label: String,
    pub content: String,
}

/// 工具调用记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceToolCall {
    pub tool_call_id: String,
    pub tool_name: String,
    /// 原始 JSON 参数（完整，不做截断）。失败时为空字符串。
    pub args_json: String,
    /// 工具返回结果（文本）。可能为空。
    pub result_text: String,
    /// `running` / `done` / `error`
    pub status: String,
    pub is_error: Option<bool>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

/// 追踪条目。运行中与归档中字段相同，`status="running"` 的条目仅在内存中可见。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceEntry {
    pub id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// `main_pi`（主 Agent → Pi）或 `delegate`（主 Agent → 子 Agent）。
    pub kind: String,
    /// `agent_llm` / `action_llm` / `agent_agent`
    #[serde(default)]
    pub trace_type: String,
    /// `agent` / `action`
    #[serde(default)]
    pub caller_kind: String,
    /// `model` / `agent`
    #[serde(default)]
    pub target_kind: String,
    /// 发起方智能体 id / 名称。
    pub caller_agent_id: String,
    pub caller_agent_name: String,
    /// 被调用方。`main_pi` 时通常等于发起方（自己调自己的 LLM）；`delegate` 时是子 Agent。
    pub target_agent_id: Option<String>,
    pub target_agent_name: Option<String>,
    pub session_id: Option<String>,
    /// 父 trace id。`delegate` 条目在开启时会自动关联到同会话中最近一次主 Pi trace，
    /// 前端可据此把「主→Pi」与它派出的「主→子」渲染成一棵树。
    #[serde(default)]
    pub parent_trace_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub response_id: Option<String>,
    /// `running` / `done` / `error` / `aborted`
    pub status: String,
    pub error: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<u64>,
    pub usage: Option<PiTokenUsagePayload>,
    pub system_prompts: Vec<TraceSystemPromptSection>,
    pub user_message: String,
    pub response_text: String,
    pub thinking_text: String,
    #[serde(default)]
    pub message_blocks: Vec<TraceMessageBlock>,
    #[serde(default)]
    pub response_blocks: Vec<TraceResponseBlock>,
    pub tool_calls: Vec<TraceToolCall>,
}

impl TraceEntry {
    fn touch_duration(&mut self, now: i64) {
        if self.finished_at.is_some() {
            if let Some(end) = self.finished_at {
                self.duration_ms = Some((end.saturating_sub(self.started_at)).max(0) as u64);
            }
        } else {
            self.duration_ms = Some((now.saturating_sub(self.started_at)).max(0) as u64);
        }
    }

    fn sync_structured_blocks(&mut self) {
        let mut message_blocks = Vec::with_capacity(self.system_prompts.len() + 1);
        for (index, prompt) in self.system_prompts.iter().enumerate() {
            message_blocks.push(TraceMessageBlock {
                id: format!("system-{index}"),
                role: "system".to_string(),
                label: prompt.label.clone(),
                content: prompt.content.clone(),
            });
        }
        message_blocks.push(TraceMessageBlock {
            id: "user-0".to_string(),
            role: "user".to_string(),
            label: "user_prompt".to_string(),
            content: self.user_message.clone(),
        });
        self.message_blocks = message_blocks;

        self.response_blocks = vec![
            TraceResponseBlock {
                id: "thinking-0".to_string(),
                kind: "thinking".to_string(),
                label: "thinking".to_string(),
                content: self.thinking_text.clone(),
            },
            TraceResponseBlock {
                id: "output-0".to_string(),
                kind: "output".to_string(),
                label: "assistant_reply".to_string(),
                content: self.response_text.clone(),
            },
        ];
    }

    fn matches_scope(&self, workspace_id: Option<&str>, session_id: Option<&str>) -> bool {
        let entry_workspace = self
            .workspace_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let desired_workspace = workspace_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match desired_workspace {
            Some(wid) if entry_workspace != Some(wid) => return false,
            None if entry_workspace.is_some() => return false,
            _ => {}
        }

        if let Some(sid) = session_id.map(str::trim).filter(|value| !value.is_empty()) {
            let entry_session = self
                .session_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if entry_session != Some(sid) {
                return false;
            }
        }

        true
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn append_response_delta(entry: &mut TraceEntry, delta: &str) -> bool {
    if delta.is_empty() {
        return false;
    }
    entry.response_text.push_str(delta);
    true
}

fn append_thinking_delta(entry: &mut TraceEntry, delta: &str) -> bool {
    if delta.is_empty() {
        return false;
    }
    entry.thinking_text.push_str(delta);
    true
}

fn registry() -> &'static Mutex<HashMap<String, TraceEntry>> {
    static REG: OnceLock<Mutex<HashMap<String, TraceEntry>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TraceStorageScope {
    Workspace(String),
    Standalone,
}

fn trace_type_meta(kind: &str) -> (&'static str, &'static str, &'static str) {
    match kind {
        "delegate" => ("agent_agent", "agent", "agent"),
        "action_llm" => ("action_llm", "action", "model"),
        _ => ("agent_llm", "agent", "model"),
    }
}

fn storage_scope_for(
    workspace_id: Option<&str>,
    session_id: Option<&str>,
) -> Option<TraceStorageScope> {
    if let Some(wid) = workspace_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(TraceStorageScope::Workspace(wid.to_string()));
    }
    if session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        return Some(TraceStorageScope::Standalone);
    }
    None
}

/// 记录每个 `(workspace_id, session_id)` 会话最近一次主 Pi trace 的 id。
/// 即便主 Pi 已经 `finalize` 出栈，这里仍保留引用，供随后在前端
/// 「计划卡下发」时启动的委派 trace 挂到正确的父节点上。
fn last_main_registry() -> &'static Mutex<HashMap<(String, String), String>> {
    static REG: OnceLock<Mutex<HashMap<(String, String), String>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn scope_registry_key(workspace_id: Option<&str>, session_id: &str) -> (String, String) {
    let bucket = workspace_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("workspace:{value}"))
        .unwrap_or_else(|| "standalone".to_string());
    (bucket, session_id.to_string())
}

fn remember_main_trace_for_scope(workspace_id: Option<&str>, session_id: &str, trace_id: &str) {
    if let Ok(mut map) = last_main_registry().lock() {
        map.insert(
            scope_registry_key(workspace_id, session_id),
            trace_id.to_string(),
        );
    }
}

fn lookup_main_trace_for_scope(workspace_id: Option<&str>, session_id: &str) -> Option<String> {
    last_main_registry().lock().ok().and_then(|m| {
        m.get(&scope_registry_key(workspace_id, session_id))
            .cloned()
    })
}

fn debug_dir(scope: &TraceStorageScope) -> Result<PathBuf, String> {
    let dir = match scope {
        TraceStorageScope::Workspace(workspace_id) => {
            workspace_fs::team_root(workspace_id)?.join(".debug")
        }
        TraceStorageScope::Standalone => agent_workspace::resolve_workspace_root()?
            .join(".debug")
            .join("standalone"),
    };
    fs::create_dir_all(&dir).map_err(|e| format!("创建调试目录失败: {e}"))?;
    Ok(dir)
}

fn day_file(scope: &TraceStorageScope, ts_ms: i64) -> Result<PathBuf, String> {
    let dir = debug_dir(scope)?;
    let dt = Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .unwrap_or_else(Local::now);
    Ok(dir.join(format!("{}.jsonl", dt.format("%Y-%m-%d"))))
}

fn emit_event(app: &AppHandle, phase: &str, entry: &TraceEntry) {
    let payload = serde_json::json!({
        "phase": phase,
        "entry": entry,
    });
    crate::emit_safe::emit_safe(app, "workspace.llm_trace", payload);
}

fn emit_text_delta_event(app: &AppHandle, entry: &TraceEntry, kind: &str, text: &str) {
    let payload = serde_json::json!({
        "phase": "updated",
        "entry": {
            "id": &entry.id,
            "workspaceId": &entry.workspace_id,
            "kind": &entry.kind,
            "traceType": &entry.trace_type,
            "callerKind": &entry.caller_kind,
            "targetKind": &entry.target_kind,
            "callerAgentId": &entry.caller_agent_id,
            "callerAgentName": &entry.caller_agent_name,
            "targetAgentId": &entry.target_agent_id,
            "targetAgentName": &entry.target_agent_name,
            "sessionId": &entry.session_id,
            "parentTraceId": &entry.parent_trace_id,
            "provider": &entry.provider,
            "model": &entry.model,
            "responseId": &entry.response_id,
            "status": &entry.status,
            "error": &entry.error,
            "startedAt": entry.started_at,
            "finishedAt": entry.finished_at,
            "durationMs": entry.duration_ms,
            "usage": &entry.usage,
            "toolCalls": &entry.tool_calls,
        },
        "delta": {
            "kind": kind,
            "text": text,
        },
    });
    crate::emit_safe::emit_safe(app, "workspace.llm_trace", payload);
}

/// 开始一条追踪。返回 `trace_id`，用于后续补充工具调用与结束时落盘。
pub fn begin(
    app: &AppHandle,
    workspace_id: Option<&str>,
    kind: &str,
    caller_agent_id: &str,
    caller_agent_name: &str,
    target_agent_id: Option<&str>,
    target_agent_name: Option<&str>,
    session_id: Option<&str>,
    provider: Option<&str>,
    model: Option<&str>,
    system_prompts: Vec<TraceSystemPromptSection>,
    user_message: &str,
) -> String {
    let id = Uuid::new_v4().simple().to_string();
    let (trace_type, caller_kind, target_kind) = trace_type_meta(kind);
    let workspace_id = workspace_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    // 委派 trace 自动挂载到同会话最近一次主 Pi trace 下，形成父子关系。
    let parent_trace_id = if kind == "main_pi" {
        None
    } else {
        session_id
            .as_deref()
            .and_then(|sid| lookup_main_trace_for_scope(workspace_id.as_deref(), sid))
    };

    let mut entry = TraceEntry {
        id: id.clone(),
        workspace_id: workspace_id.clone(),
        kind: kind.to_string(),
        trace_type: trace_type.to_string(),
        caller_kind: caller_kind.to_string(),
        target_kind: target_kind.to_string(),
        caller_agent_id: caller_agent_id.to_string(),
        caller_agent_name: caller_agent_name.to_string(),
        target_agent_id: target_agent_id.map(ToOwned::to_owned),
        target_agent_name: target_agent_name.map(ToOwned::to_owned),
        session_id: session_id.clone(),
        parent_trace_id,
        provider: provider.map(ToOwned::to_owned),
        model: model.map(ToOwned::to_owned),
        response_id: None,
        status: "running".to_string(),
        error: None,
        started_at: now_ms(),
        finished_at: None,
        duration_ms: None,
        usage: None,
        system_prompts,
        user_message: user_message.to_string(),
        response_text: String::new(),
        thinking_text: String::new(),
        message_blocks: Vec::new(),
        response_blocks: Vec::new(),
        tool_calls: Vec::new(),
    };
    entry.sync_structured_blocks();
    if let Ok(mut map) = registry().lock() {
        map.insert(id.clone(), entry.clone());
    }
    if kind == "main_pi" {
        if let Some(sid) = session_id.as_deref() {
            remember_main_trace_for_scope(workspace_id.as_deref(), sid, &id);
        }
    }
    emit_event(app, "started", &entry);
    id
}

/// 追加/更新工具调用。`tool_call_id` 相同时合并（开始 + 结束）。
pub fn record_tool(
    app: &AppHandle,
    trace_id: &str,
    tool_call_id: &str,
    tool_name: &str,
    args_json: Option<&str>,
    result_text: Option<&str>,
    status: &str,
    is_error: Option<bool>,
) {
    if trace_id.is_empty() {
        return;
    }
    let snapshot = {
        let Ok(mut map) = registry().lock() else {
            return;
        };
        let Some(entry) = map.get_mut(trace_id) else {
            return;
        };
        let now = now_ms();
        let idx = entry
            .tool_calls
            .iter()
            .position(|t| !tool_call_id.is_empty() && t.tool_call_id == tool_call_id);
        if let Some(i) = idx {
            let t = &mut entry.tool_calls[i];
            t.tool_name = tool_name.to_string();
            if let Some(a) = args_json {
                if !a.is_empty() {
                    t.args_json = a.to_string();
                }
            }
            if let Some(r) = result_text {
                if !r.is_empty() {
                    t.result_text = r.to_string();
                }
            }
            t.status = status.to_string();
            t.is_error = is_error.or(t.is_error);
            if matches!(status, "done" | "error") {
                t.finished_at = Some(now);
            }
        } else {
            entry.tool_calls.push(TraceToolCall {
                tool_call_id: tool_call_id.to_string(),
                tool_name: tool_name.to_string(),
                args_json: args_json.unwrap_or_default().to_string(),
                result_text: result_text.unwrap_or_default().to_string(),
                status: status.to_string(),
                is_error,
                started_at: now,
                finished_at: if matches!(status, "done" | "error") {
                    Some(now)
                } else {
                    None
                },
            });
        }
        entry.sync_structured_blocks();
        entry.clone()
    };
    emit_event(app, "updated", &snapshot);
}

/// 追加响应文本片段（text_delta）。
pub fn append_response(app: &AppHandle, trace_id: &str, delta: &str) {
    if trace_id.is_empty() || delta.is_empty() {
        return;
    }
    let snapshot = {
        let Ok(mut map) = registry().lock() else {
            return;
        };
        let Some(entry) = map.get_mut(trace_id) else {
            return;
        };
        if !append_response_delta(entry, delta) {
            return;
        }
        entry.sync_structured_blocks();
        entry.clone()
    };
    emit_text_delta_event(app, &snapshot, "response", delta);
}

/// 追加思考过程文本片段（thinking_delta）。
pub fn append_thinking(app: &AppHandle, trace_id: &str, delta: &str) {
    if trace_id.is_empty() || delta.is_empty() {
        return;
    }
    let snapshot = {
        let Ok(mut map) = registry().lock() else {
            return;
        };
        let Some(entry) = map.get_mut(trace_id) else {
            return;
        };
        if !append_thinking_delta(entry, delta) {
            return;
        }
        entry.sync_structured_blocks();
        entry.clone()
    };
    emit_text_delta_event(app, &snapshot, "thinking", delta);
}

/// 结束一条追踪，落盘 + emit 事件。
pub fn finalize(
    app: &AppHandle,
    trace_id: &str,
    status: &str,
    error: Option<String>,
    response_text_snapshot: Option<String>,
    usage: Option<PiTokenUsagePayload>,
    provider: Option<String>,
    model: Option<String>,
    response_id: Option<String>,
) {
    if trace_id.is_empty() {
        return;
    }
    let entry = {
        let Ok(mut map) = registry().lock() else {
            return;
        };
        let Some(mut entry) = map.remove(trace_id) else {
            return;
        };
        let now = now_ms();
        entry.status = status.to_string();
        entry.error = error;
        if let Some(text) = response_text_snapshot {
            if !text.is_empty() && text.chars().count() > entry.response_text.chars().count() {
                entry.response_text = text;
            }
        }
        if let Some(u) = usage {
            entry.usage = Some(u);
        }
        if let Some(p) = provider {
            if !p.is_empty() {
                entry.provider = Some(p);
            }
        }
        if let Some(m) = model {
            if !m.is_empty() {
                entry.model = Some(m);
            }
        }
        if let Some(r) = response_id {
            if !r.is_empty() {
                entry.response_id = Some(r);
            }
        }
        entry.finished_at = Some(now);
        entry.touch_duration(now);
        entry.sync_structured_blocks();
        entry
    };
    // 写文件
    if let Some(scope) =
        storage_scope_for(entry.workspace_id.as_deref(), entry.session_id.as_deref())
    {
        if let Ok(path) = day_file(&scope, entry.started_at) {
            if let Ok(line) = serde_json::to_string(&entry) {
                let result = (|| -> std::io::Result<()> {
                    let mut f = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)?;
                    f.write_all(line.as_bytes())?;
                    f.write_all(b"\n")?;
                    Ok(())
                })();
                if let Err(e) = result {
                    eprintln!("[llm_trace] 写入 {} 失败: {}", path.display(), e);
                } else {
                    crate::llm_log_export::mirror_line(&line, entry.started_at);
                }
            }
        }
    }
    emit_event(app, "finalized", &entry);
}

/// 列出调试面板上可见的追踪：内存中 running 的 + 最近 `days` 天的归档文件。
pub fn list_recent(
    workspace_id: Option<&str>,
    session_id: Option<&str>,
    days: usize,
    limit: usize,
) -> Vec<TraceEntry> {
    let mut out: Vec<TraceEntry> = Vec::new();

    if let Ok(map) = registry().lock() {
        for entry in map.values() {
            if entry.matches_scope(workspace_id, session_id) {
                out.push(entry.clone());
            }
        }
    }

    let Some(scope) = storage_scope_for(workspace_id, session_id) else {
        out.sort_by_key(|item| std::cmp::Reverse(item.started_at));
        out.truncate(limit);
        return out;
    };
    let Ok(dir) = debug_dir(&scope) else {
        out.sort_by_key(|item| std::cmp::Reverse(item.started_at));
        out.truncate(limit);
        return out;
    };

    let today = Local::now();
    for back in 0..days.max(1) {
        let dt = today - chrono::Duration::days(back as i64);
        let path = dir.join(format!("{}.jsonl", dt.format("%Y-%m-%d")));
        if !path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<TraceEntry>(line) {
                if entry.matches_scope(workspace_id, session_id) {
                    out.push(entry);
                }
            }
        }
    }

    out.sort_by_key(|item| std::cmp::Reverse(item.started_at));
    out.truncate(limit);
    out
}

/// RAII guard：在持有期间绑定一条 `running` trace；
/// - `finalize_done` / `finalize_error` 标记显式结束；
/// - 若 guard 被直接 drop（例如上游提前 `return Err`），自动按 `aborted` 结束，避免泄漏。
pub struct TraceGuard<'a> {
    app: &'a AppHandle,
    trace_id: Option<String>,
}

impl<'a> TraceGuard<'a> {
    pub fn new(app: &'a AppHandle, trace_id: String) -> Self {
        Self {
            app,
            trace_id: Some(trace_id),
        }
    }

    pub fn id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    pub fn finalize_done(
        &mut self,
        response_text: Option<String>,
        usage: Option<PiTokenUsagePayload>,
        provider: Option<String>,
        model: Option<String>,
        response_id: Option<String>,
    ) {
        if let Some(id) = self.trace_id.take() {
            finalize(
                self.app,
                &id,
                "done",
                None,
                response_text,
                usage,
                provider,
                model,
                response_id,
            );
        }
    }

    pub fn finalize_error(&mut self, error: String) {
        if let Some(id) = self.trace_id.take() {
            finalize(
                self.app,
                &id,
                "error",
                Some(error),
                None,
                None,
                None,
                None,
                None,
            );
        }
    }

    pub fn finalize_aborted(&mut self, error: String) {
        if let Some(id) = self.trace_id.take() {
            finalize(
                self.app,
                &id,
                "aborted",
                Some(error),
                None,
                None,
                None,
                None,
                None,
            );
        }
    }
}

impl<'a> Drop for TraceGuard<'a> {
    fn drop(&mut self) {
        if let Some(id) = self.trace_id.take() {
            finalize(
                self.app,
                &id,
                "aborted",
                Some("调用流程提前结束（未显式标记完成）".to_string()),
                None,
                None,
                None,
                None,
                None,
            );
        }
    }
}

pub fn clear(workspace_id: Option<&str>, session_id: Option<&str>) -> Result<(), String> {
    if let Ok(mut map) = registry().lock() {
        map.retain(|_, v| !v.matches_scope(workspace_id, session_id));
    }
    let Some(scope) = storage_scope_for(workspace_id, session_id) else {
        return Ok(());
    };
    let Ok(dir) = debug_dir(&scope) else {
        return Ok(());
    };
    let Ok(iter) = fs::read_dir(&dir) else {
        return Ok(());
    };
    for entry in iter.flatten() {
        let path = entry.path();
        let is_jsonl = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("jsonl"))
            .unwrap_or(false);
        if is_jsonl {
            if session_id.is_none() && workspace_id.is_some() {
                let _ = fs::remove_file(path);
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let mut kept = Vec::new();
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(entry) = serde_json::from_str::<TraceEntry>(trimmed) else {
                    kept.push(trimmed.to_string());
                    continue;
                };
                if !entry.matches_scope(workspace_id, session_id) {
                    kept.push(trimmed.to_string());
                }
            }
            if kept.is_empty() {
                let _ = fs::remove_file(path);
            } else {
                let _ = fs::write(path, format!("{}\n", kept.join("\n")));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> TraceEntry {
        let mut entry = TraceEntry {
            id: "trace-1".to_string(),
            workspace_id: Some("ws-1".to_string()),
            kind: "main_pi".to_string(),
            trace_type: "agent_llm".to_string(),
            caller_kind: "agent".to_string(),
            target_kind: "model".to_string(),
            caller_agent_id: "main".to_string(),
            caller_agent_name: "Main".to_string(),
            target_agent_id: None,
            target_agent_name: None,
            session_id: Some("session-1".to_string()),
            parent_trace_id: None,
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4".to_string()),
            response_id: None,
            status: "running".to_string(),
            error: None,
            started_at: 1,
            finished_at: None,
            duration_ms: None,
            usage: None,
            system_prompts: Vec::new(),
            user_message: "hello".to_string(),
            response_text: String::new(),
            thinking_text: String::new(),
            message_blocks: Vec::new(),
            response_blocks: Vec::new(),
            tool_calls: Vec::new(),
        };
        entry.sync_structured_blocks();
        entry
    }

    #[test]
    fn append_response_delta_appends_incrementally() {
        let mut entry = sample_entry();

        assert!(append_response_delta(&mut entry, "foo"));
        assert!(append_response_delta(&mut entry, "bar"));
        assert_eq!(entry.response_text, "foobar");
        assert!(!append_response_delta(&mut entry, ""));
    }

    #[test]
    fn append_thinking_delta_appends_incrementally() {
        let mut entry = sample_entry();

        assert!(append_thinking_delta(&mut entry, "step1"));
        assert!(append_thinking_delta(&mut entry, " -> step2"));
        assert_eq!(entry.thinking_text, "step1 -> step2");
        assert!(!append_thinking_delta(&mut entry, ""));
    }

    #[test]
    fn structured_blocks_fall_back_from_flat_fields() {
        let entry = sample_entry();
        assert_eq!(entry.message_blocks.len(), 1);
        assert_eq!(entry.message_blocks[0].role, "user");
        assert_eq!(entry.response_blocks.len(), 2);
        assert_eq!(entry.response_blocks[0].kind, "thinking");
        assert_eq!(entry.response_blocks[1].kind, "output");
    }

    #[test]
    fn matches_scope_distinguishes_workspace_and_standalone() {
        let workspace_entry = sample_entry();
        let mut standalone_entry = sample_entry();
        standalone_entry.workspace_id = None;

        assert!(workspace_entry.matches_scope(Some("ws-1"), Some("session-1")));
        assert!(!workspace_entry.matches_scope(None, Some("session-1")));
        assert!(standalone_entry.matches_scope(None, Some("session-1")));
        assert!(!standalone_entry.matches_scope(Some("ws-1"), Some("session-1")));
    }

}
