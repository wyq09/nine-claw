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
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

/// 单个 system prompt 分段（便于前端分节展示）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSystemPromptSection {
    /// 段落标签（如 `agent_system_prompt`、`workspace_team`、`delegate_preface`）。
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
    pub workspace_id: String,
    /// `main_pi`（主 Agent → Pi）或 `delegate`（主 Agent → 子 Agent）。
    pub kind: String,
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
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn registry() -> &'static Mutex<HashMap<String, TraceEntry>> {
    static REG: OnceLock<Mutex<HashMap<String, TraceEntry>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 记录每个 `(workspace_id, session_id)` 会话最近一次主 Pi trace 的 id。
/// 即便主 Pi 已经 `finalize` 出栈，这里仍保留引用，供随后在前端
/// 「计划卡下发」时启动的委派 trace 挂到正确的父节点上。
fn last_main_registry() -> &'static Mutex<HashMap<(String, String), String>> {
    static REG: OnceLock<Mutex<HashMap<(String, String), String>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember_main_trace(workspace_id: &str, session_id: &str, trace_id: &str) {
    if let Ok(mut map) = last_main_registry().lock() {
        map.insert(
            (workspace_id.to_string(), session_id.to_string()),
            trace_id.to_string(),
        );
    }
}

fn lookup_main_trace(workspace_id: &str, session_id: &str) -> Option<String> {
    last_main_registry()
        .lock()
        .ok()
        .and_then(|m| m.get(&(workspace_id.to_string(), session_id.to_string())).cloned())
}

fn debug_dir(workspace_id: &str) -> Result<PathBuf, String> {
    let root = workspace_fs::team_root(workspace_id)?;
    let dir = root.join(".debug");
    fs::create_dir_all(&dir).map_err(|e| format!("创建调试目录失败: {e}"))?;
    Ok(dir)
}

fn day_file(workspace_id: &str, ts_ms: i64) -> Result<PathBuf, String> {
    let dir = debug_dir(workspace_id)?;
    let dt = Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .unwrap_or_else(|| Local::now());
    Ok(dir.join(format!("{}.jsonl", dt.format("%Y-%m-%d"))))
}

fn emit_event(app: &AppHandle, phase: &str, entry: &TraceEntry) {
    let payload = serde_json::json!({
        "phase": phase,
        "entry": entry,
    });
    let _ = app.emit("workspace.llm_trace", payload);
}

/// 开始一条追踪。返回 `trace_id`，用于后续补充工具调用与结束时落盘。
pub fn begin(
    app: &AppHandle,
    workspace_id: &str,
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

    // 委派 trace 自动挂载到同会话最近一次主 Pi trace 下，形成父子关系。
    let parent_trace_id = if kind == "delegate" {
        session_id.and_then(|sid| lookup_main_trace(workspace_id, sid))
    } else {
        None
    };

    let entry = TraceEntry {
        id: id.clone(),
        workspace_id: workspace_id.to_string(),
        kind: kind.to_string(),
        caller_agent_id: caller_agent_id.to_string(),
        caller_agent_name: caller_agent_name.to_string(),
        target_agent_id: target_agent_id.map(ToOwned::to_owned),
        target_agent_name: target_agent_name.map(ToOwned::to_owned),
        session_id: session_id.map(ToOwned::to_owned),
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
        tool_calls: Vec::new(),
    };
    if let Ok(mut map) = registry().lock() {
        map.insert(id.clone(), entry.clone());
    }
    if kind == "main_pi" {
        if let Some(sid) = session_id {
            remember_main_trace(workspace_id, sid, &id);
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
        entry.response_text.push_str(delta);
        entry.clone()
    };
    emit_event(app, "updated", &snapshot);
}

/// 追加思考过程文本片段（thinking_delta）。
pub fn append_thinking(_app: &AppHandle, trace_id: &str, delta: &str) {
    if trace_id.is_empty() || delta.is_empty() {
        return;
    }
    let Ok(mut map) = registry().lock() else {
        return;
    };
    let Some(entry) = map.get_mut(trace_id) else {
        return;
    };
    entry.thinking_text.push_str(delta);
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
        entry
    };
    // 写文件
    if let Ok(path) = day_file(&entry.workspace_id, entry.started_at) {
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
            }
        }
    }
    emit_event(app, "finalized", &entry);
}

/// 列出工作空间调试面板上可见的追踪：内存中 running 的 + 最近 `days` 天的归档文件。
pub fn list_recent(workspace_id: &str, days: usize, limit: usize) -> Vec<TraceEntry> {
    let mut out: Vec<TraceEntry> = Vec::new();

    if let Ok(map) = registry().lock() {
        for entry in map.values() {
            if entry.workspace_id == workspace_id {
                out.push(entry.clone());
            }
        }
    }

    let Ok(dir) = debug_dir(workspace_id) else {
        out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
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
                if entry.workspace_id == workspace_id {
                    out.push(entry);
                }
            }
        }
    }

    out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
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

pub fn clear(workspace_id: &str) -> Result<(), String> {
    if let Ok(mut map) = registry().lock() {
        map.retain(|_, v| v.workspace_id != workspace_id);
    }
    let Ok(dir) = debug_dir(workspace_id) else {
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
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}
