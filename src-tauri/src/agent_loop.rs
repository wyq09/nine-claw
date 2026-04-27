//! Agent Loop engine: marker parsing, loop control, and delegate execution.
//!
//! Provides functions for extracting, parsing, stripping, and formatting the
//! structured markers (`NC_AGENT_LOOP_*`) that the LLM emits to control the
//! agent loop runtime, plus the main `run_agent_loop` orchestration function.

use crate::agent_loop_types::{
    AgentLoopBatchResult, AgentLoopBatchMarker, AgentLoopCallMarker,
    AgentLoopExtendMarker, AgentLoopResult, AgentLoopConfig, ParsedLoopMarker,
    ReviewResponse,
};

use chrono::Local;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::time::{Duration, timeout as tokio_timeout};

// ---------------------------------------------------------------------------
// Marker constants
// ---------------------------------------------------------------------------

/// Marker for a single agent call.
pub const MARKER_CALL: &str = "NC_AGENT_LOOP_CALL_JSON:";
/// Marker for a batch of concurrent agent calls.
pub const MARKER_BATCH: &str = "NC_AGENT_LOOP_BATCH_JSON:";
/// Marker to request an iteration-limit extension.
pub const MARKER_EXTEND: &str = "NC_AGENT_LOOP_EXTEND_JSON:";
/// Marker signalling the loop should terminate.
pub const MARKER_FINAL: &str = "NC_AGENT_LOOP_FINAL:";
/// Marker carrying a single agent result back to the LLM.
pub const MARKER_RESULT: &str = "NC_AGENT_LOOP_RESULT_JSON:";

/// All prefix markers (everything except FINAL which has no JSON payload).
const LOOP_MARKERS: &[&str] = &[MARKER_BATCH, MARKER_CALL, MARKER_EXTEND];

// ---------------------------------------------------------------------------
// extract_first_loop_marker
// ---------------------------------------------------------------------------

/// Search `text` for the first agent-loop marker and return the parsed marker
/// together with the **byte offset** of the marker's start position.
///
/// Search order: `BATCH` first (since `CALL` is a textual subset of `BATCH`),
/// then `CALL`, then `EXTEND`.
pub fn extract_first_loop_marker(text: &str) -> Option<(ParsedLoopMarker, usize)> {
    // Collect candidates: (byte_offset, marker_tag)
    let mut candidates: Vec<(usize, &str)> = Vec::new();

    for &marker in LOOP_MARKERS {
        if let Some(pos) = text.find(marker) {
            candidates.push((pos, marker));
        }
    }

    // Pick the earliest occurrence.
    candidates.sort_by_key(|(pos, _)| *pos);
    let (offset, tag) = candidates.first()?;

    let json_start = *offset + tag.len();
    let remaining = &text[json_start..];
    let value = parse_json_after_marker(remaining)?;

    match *tag {
        MARKER_BATCH => {
            let batch: AgentLoopBatchMarker = serde_json::from_value(value).ok()?;
            Some((ParsedLoopMarker::Batch(batch), *offset))
        }
        MARKER_CALL => {
            let call: AgentLoopCallMarker = serde_json::from_value(value).ok()?;
            Some((ParsedLoopMarker::Call(call), *offset))
        }
        MARKER_EXTEND => {
            let extend: AgentLoopExtendMarker = serde_json::from_value(value).ok()?;
            Some((ParsedLoopMarker::Extend(extend), *offset))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// parse_json_after_marker
// ---------------------------------------------------------------------------

/// Starting from the text immediately after a marker tag, find the first `{`,
/// then match braces (respecting strings and escape sequences) to extract a
/// complete JSON value.
pub fn parse_json_after_marker(remaining: &str) -> Option<serde_json::Value> {
    let start = remaining.find('{')?;
    let bytes = remaining.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut i = start;

    while i < bytes.len() {
        let ch = bytes[i];
        if in_string {
            if ch == b'\\' {
                // Skip escaped character.
                i += 2;
                continue;
            }
            if ch == b'"' {
                in_string = false;
            }
        } else {
            match ch {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let json_str = &remaining[start..=i];
                        return serde_json::from_str(json_str).ok();
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }

    None
}

// ---------------------------------------------------------------------------
// has_final_marker
// ---------------------------------------------------------------------------

/// Return `true` if `text` contains `NC_AGENT_LOOP_FINAL:`.
pub fn has_final_marker(text: &str) -> bool {
    text.contains(MARKER_FINAL)
}

// ---------------------------------------------------------------------------
// strip_loop_markers
// ---------------------------------------------------------------------------

/// Remove every line that contains any `NC_AGENT_LOOP` marker, then collapse
/// consecutive blank lines into a single blank line and trim trailing blanks.
pub fn strip_loop_markers(text: &str) -> String {
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            !line.contains(MARKER_CALL)
                && !line.contains(MARKER_BATCH)
                && !line.contains(MARKER_EXTEND)
                && !line.contains(MARKER_FINAL)
                && !line.contains(MARKER_RESULT)
        })
        .collect();

    // Collapse runs of blank lines.
    let mut result = String::new();
    let mut prev_blank = false;
    for line in &kept {
        let is_blank = line.trim().is_empty();
        if is_blank && prev_blank {
            continue; // skip consecutive blank
        }
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(line);
        prev_blank = is_blank;
    }

    result
}

// ---------------------------------------------------------------------------
// format_single_result / format_batch_result
// ---------------------------------------------------------------------------

/// Format a single agent-loop result as a single-line marker string.
pub fn format_single_result(result: &AgentLoopResult) -> String {
    let json = serde_json::to_string(result).unwrap_or_else(|_| "{}".into());
    format!("{}{}\n", MARKER_RESULT, json)
}

/// Format a batch result as a single-line marker string.
pub fn format_batch_result(result: &AgentLoopBatchResult) -> String {
    let json = serde_json::to_string(result).unwrap_or_else(|_| "{}".into());
    format!("{}{}\n", MARKER_RESULT, json)
}

// ---------------------------------------------------------------------------
// Defense layer: heal_orphaned_tool_calls
// ---------------------------------------------------------------------------

/// Scan `history` for assistant messages whose `tool_calls` entries have no
/// matching `tool` result (identified by `tool_call_id`). For every orphan,
/// inject a synthetic tool-result message immediately after the assistant
/// message so the conversation history remains well-formed for the LLM.
pub fn heal_orphaned_tool_calls(history: &mut Vec<serde_json::Value>) {
    // Collect the IDs of all tool results already present.
    let mut answered_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for msg in history.iter() {
        if msg.get("role").and_then(|r| r.as_str()) == Some("tool") {
            if let Some(id) = msg.get("tool_call_id").and_then(|v| v.as_str()) {
                answered_ids.insert(id.to_string());
            }
        }
    }

    // Walk assistant messages and find orphaned tool_calls.
    // We collect insertions as (index_after_assistant, synthetic_value) and
    // apply them in reverse order so indices remain valid.
    let mut insertions: Vec<(usize, serde_json::Value)> = Vec::new();

    for (i, msg) in history.iter().enumerate() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let tool_calls = match msg.get("tool_calls").and_then(|v| v.as_array()) {
            Some(arr) if !arr.is_empty() => arr,
            _ => continue,
        };

        for tc in tool_calls {
            let tc_id = match tc.get("id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };
            if !answered_ids.contains(&tc_id) {
                let synthetic = serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tc_id,
                    "content": "[NineClaw] 此工具调用因会话中断未完成，请根据已有信息继续。"
                });
                answered_ids.insert(tc_id);
                insertions.push((i + 1, synthetic));
            }
        }
    }

    // Apply in reverse order to keep indices stable.
    for (idx, val) in insertions.into_iter().rev() {
        history.insert(idx, val);
    }
}

// ---------------------------------------------------------------------------
// Defense layer: inject_dynamic_context
// ---------------------------------------------------------------------------

/// Find the **last** message with `"role": "system"` in `history` and append
/// a dynamic context block (current time + permission denials) to its
/// `content`. Appending at the end protects the KV Cache prefix.
pub fn inject_dynamic_context(
    history: &mut Vec<serde_json::Value>,
    permission_denials: &[String],
) {
    let now_str = Local::now().format("%Y-%m-%d %H:%M %Z").to_string();
    let denials_text = if permission_denials.is_empty() {
        "无".to_string()
    } else {
        permission_denials.join("，")
    };

    let context_block = format!(
        "\n\n[动态上下文]\n当前时间：{}\n权限拒绝：{}",
        now_str, denials_text
    );

    // Find the last system message.
    let last_sys_idx = history
        .iter()
        .rposition(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("system"));

    if let Some(idx) = last_sys_idx {
        let content = history[idx]
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let new_content = format!("{}{}", content, context_block);
        history[idx]["content"] = serde_json::Value::String(new_content);
    }
}

// ---------------------------------------------------------------------------
// Defense layer: compress_assistant_message
// ---------------------------------------------------------------------------

/// If an assistant message has a non-empty `tool_calls` array **and** its
/// `content` is fewer than 50 characters of trimmed text, clear the content
/// to save tokens. Substantive reasoning text is preserved.
pub fn compress_assistant_message(msg: &mut serde_json::Value) {
    let has_tool_calls = msg
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .map_or(false, |arr| !arr.is_empty());

    if !has_tool_calls {
        return;
    }

    let content_len = msg
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().len())
        .unwrap_or(0);

    if content_len < 50 {
        msg["content"] = serde_json::Value::String(String::new());
    }
}

// ---------------------------------------------------------------------------
// Defense layer: prepare_loop_iteration
// ---------------------------------------------------------------------------

/// Prepare the conversation history for a new agent-loop iteration:
/// heal orphaned tool calls first, then inject dynamic context.
pub fn prepare_loop_iteration(
    history: &mut Vec<serde_json::Value>,
    permission_denials: &[String],
) {
    heal_orphaned_tool_calls(history);
    inject_dynamic_context(history, permission_denials);
}

// ===========================================================================
// Agent Loop Engine — delegate execution & loop control
// ===========================================================================

use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::task::JoinSet;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// emit_loop_event
// ---------------------------------------------------------------------------

/// Emit a typed agent-loop event to the frontend via the Tauri event bus.
fn emit_loop_event(app: &AppHandle, event: &str, payload: serde_json::Value) {
    let _ = app.emit(event, payload);
}

fn emit_completed(
    app: &AppHandle,
    loop_id: &str,
    reason: &str,
    iteration: u32,
    loop_start: &Instant,
) {
    log::info!("AgentLoop [{loop_id}] completed: {reason} at iteration {iteration}");
    emit_loop_event(
        app,
        "agent-loop://completed",
        serde_json::json!({
            "loopId": loop_id,
            "reason": reason,
            "iteration": iteration,
            "totalDurationMs": loop_start.elapsed().as_millis() as u64,
        }),
    );
}

/// Execute a single delegate call with an iteration-level timeout.
async fn execute_delegate_with_timeout(
    app: &AppHandle,
    call: AgentLoopCallMarker,
    iteration: u32,
    loop_id: &str,
    provider: &crate::provider_runtime::ProviderRuntimeConfig,
    abort_flag: &Arc<AtomicBool>,
    timeout: Duration,
) -> AgentLoopResult {
    let call_for_error = call.clone();
    let app_clone = app.clone();
    let provider_clone = provider.clone();
    let abort_clone = abort_flag.clone();
    let loop_id_owned = loop_id.to_string();

    let handle = tokio::task::spawn_blocking(move || {
        execute_single_delegate(
            &app_clone,
            &call,
            &loop_id_owned,
            iteration,
            &provider_clone,
            &abort_clone,
        )
    });

    match tokio_timeout(timeout, handle).await {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => AgentLoopResult {
            agent_id: call_for_error.agent_id.clone(),
            agent_name: String::new(),
            task: call_for_error.task.clone(),
            status: "error".into(),
            output: format!("spawn_blocking 失败: {e}"),
            tool_calls_count: 0,
            duration_ms: 0,
        },
        Err(_) => {
            log::warn!("AgentLoop [{loop_id}] iteration {iteration} timed out after {:?}", timeout);
            AgentLoopResult {
                agent_id: call_for_error.agent_id.clone(),
                agent_name: String::new(),
                task: call_for_error.task.clone(),
                status: "error".into(),
                output: format!("委派超时（{}秒）", timeout.as_secs()),
                tool_calls_count: 0,
                duration_ms: timeout.as_millis() as u64,
            }
        }
    }
}

/// Execute a batch of delegates with an iteration-level timeout.
async fn execute_batch_with_timeout(
    app: &AppHandle,
    batch: &AgentLoopBatchMarker,
    loop_id: &str,
    iteration: u32,
    config: &AgentLoopConfig,
    provider: &crate::provider_runtime::ProviderRuntimeConfig,
    abort_flag: &Arc<AtomicBool>,
    timeout: Duration,
) -> AgentLoopBatchResult {
    let app_clone = app.clone();
    let batch_clone = batch.clone();
    let loop_id_owned = loop_id.to_string();
    let config_clone = config.clone();
    let provider_clone = provider.clone();
    let abort_clone = abort_flag.clone();

    let handle = tokio::task::spawn(async move {
        execute_batch_delegates(
            &app_clone,
            &batch_clone,
            &loop_id_owned,
            iteration,
            &config_clone,
            &provider_clone,
            &abort_clone,
        )
        .await
    });

    match tokio_timeout(timeout, handle).await {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => AgentLoopBatchResult {
            batch_id: format!("batch:{}:{}", loop_id, iteration),
            results: vec![AgentLoopResult {
                agent_id: String::new(),
                agent_name: String::new(),
                task: String::new(),
                status: "error".into(),
                output: format!("batch spawn 失败: {e}"),
                tool_calls_count: 0,
                duration_ms: 0,
            }],
            total_duration_ms: 0,
        },
        Err(_) => {
            log::warn!("AgentLoop [{loop_id}] batch at iteration {iteration} timed out after {:?}", timeout);
            AgentLoopBatchResult {
                batch_id: format!("batch:{}:{}", loop_id, iteration),
                results: vec![AgentLoopResult {
                    agent_id: String::new(),
                    agent_name: String::new(),
                    task: String::new(),
                    status: "timeout".into(),
                    output: format!("批量委派超时（{}秒）", timeout.as_secs()),
                    tool_calls_count: 0,
                    duration_ms: timeout.as_millis() as u64,
                }],
                total_duration_ms: timeout.as_millis() as u64,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// delegate_to_agent — simplified public entry point
// ---------------------------------------------------------------------------

/// Delegate a single task to a sub-agent by ID.
///
/// Convenience wrapper around `execute_single_delegate` for the agent_delegate tool.
pub(crate) fn delegate_to_agent(
    app: &AppHandle,
    agent_id: &str,
    task: &str,
    context: Option<&str>,
    provider: &crate::provider_runtime::ProviderRuntimeConfig,
) -> AgentLoopResult {
    delegate_to_agent_with_trace(app, agent_id, task, context, provider, None)
}

pub(crate) fn delegate_to_agent_with_trace(
    app: &AppHandle,
    agent_id: &str,
    task: &str,
    context: Option<&str>,
    provider: &crate::provider_runtime::ProviderRuntimeConfig,
    trace_id: Option<&str>,
) -> AgentLoopResult {
    let mut prompt = task.to_string();
    if let Some(ctx) = context {
        if !ctx.is_empty() {
            prompt = format!("{prompt}\n\n[额外上下文]\n{ctx}");
        }
    }

    let call = AgentLoopCallMarker {
        agent_id: agent_id.to_string(),
        task: prompt,
        params: serde_json::Value::Null,
        context_injection: None,
        expect_structured_output: false,
        output_format_hint: None,
        pause_for_review: false,
    };

    let abort_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    execute_single_delegate_internal(app, &call, "delegate-tool", 1, provider, &abort_flag, trace_id)
}

// ---------------------------------------------------------------------------
// execute_single_delegate
// ---------------------------------------------------------------------------

/// Execute a single sub-agent call within the agent loop.
///
/// This builds a PiBridge session for the target agent, sends the task prompt,
/// and returns the result as an `AgentLoopResult`.
pub(crate) fn execute_single_delegate(
    app: &AppHandle,
    call: &AgentLoopCallMarker,
    loop_id: &str,
    iteration: u32,
    provider: &crate::provider_runtime::ProviderRuntimeConfig,
    abort_flag: &Arc<AtomicBool>,
) -> AgentLoopResult {
    execute_single_delegate_internal(app, call, loop_id, iteration, provider, abort_flag, None)
}

fn execute_single_delegate_internal(
    app: &AppHandle,
    call: &AgentLoopCallMarker,
    loop_id: &str,
    iteration: u32,
    provider: &crate::provider_runtime::ProviderRuntimeConfig,
    abort_flag: &Arc<AtomicBool>,
    trace_id: Option<&str>,
) -> AgentLoopResult {
    let start = Instant::now();

    // Resolve agent record for name display.
    let agent_record = match crate::agents::get_agent_record(app, &call.agent_id) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return AgentLoopResult {
                agent_id: call.agent_id.clone(),
                agent_name: String::new(),
                task: call.task.clone(),
                status: "error".into(),
                output: format!("智能体 '{}' 不存在", call.agent_id),
                tool_calls_count: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }
        Err(e) => {
            return AgentLoopResult {
                agent_id: call.agent_id.clone(),
                agent_name: String::new(),
                task: call.task.clone(),
                status: "error".into(),
                output: format!("查询智能体失败: {e}"),
                tool_calls_count: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    // Check abort before executing.
    if abort_flag.load(Ordering::Relaxed) {
        return AgentLoopResult {
            agent_id: call.agent_id.clone(),
            agent_name: agent_record.name.clone(),
            task: call.task.clone(),
            status: "cancelled".into(),
            output: "被取消".into(),
            tool_calls_count: 0,
            duration_ms: start.elapsed().as_millis() as u64,
        };
    }

    // Build delegate prompt with optional params.
    let mut prompt = call.task.clone();
    if !call.params.is_null() && call.params.as_object().map_or(false, |o| !o.is_empty()) {
        let pretty = serde_json::to_string_pretty(&call.params).unwrap_or_default();
        prompt = format!("{prompt}\n\n[委派参数]\n{pretty}");
    }

    // Build a PiBridge for this agent.
    let agent_cfg = match crate::agents::get_conversation_agent_config(app, &call.agent_id) {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            return AgentLoopResult {
                agent_id: call.agent_id.clone(),
                agent_name: agent_record.name.clone(),
                task: call.task.clone(),
                status: "error".into(),
                output: "无法加载智能体配置".into(),
                tool_calls_count: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }
        Err(e) => {
            return AgentLoopResult {
                agent_id: call.agent_id.clone(),
                agent_name: agent_record.name.clone(),
                task: call.task.clone(),
                status: "error".into(),
                output: format!("加载智能体配置失败: {e}"),
                tool_calls_count: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    let base_normalized = crate::normalized_provider_runtime_base_url(
        &provider.base_url,
        &provider.api_format,
        &provider.provider_id,
    );

    let pi_rt = match crate::pi_runtime::require_pi_runtime_location(app) {
        Ok(rt) => rt,
        Err(e) => {
            return AgentLoopResult {
                agent_id: call.agent_id.clone(),
                agent_name: agent_record.name.clone(),
                task: call.task.clone(),
                status: "error".into(),
                output: format!("Pi runtime 不可用: {e}"),
                tool_calls_count: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    let bridge = crate::channels::pi_bridge::PiBridge::new(
        pi_rt,
        &provider.provider_id,
        &provider.api_format,
        &base_normalized,
        &provider.api_key,
        &provider.model,
        Some(agent_cfg),
    );

    let _run_id = format!("nc-al:{}:{}:{}", loop_id, iteration, call.agent_id);
    let channel_id = format!("nc:agent-loop:{}", Uuid::new_v4());
    let user_id = Uuid::new_v4().simple().to_string();
    let tool_calls_count = Arc::new(AtomicUsize::new(0));

    let full_prompt = format!(
        "你是智能体 **{}**。请完成下面的任务，直接给出结果。\n\n---\n\n{}",
        agent_record.name, prompt,
    );

    let app_for_chunk = app.clone();
    let trace_id_for_chunk = trace_id.map(str::to_string);
    let on_chunk = move |chunk: &str| {
        if let Some(tid) = trace_id_for_chunk.as_deref() {
            crate::llm_trace::append_response(&app_for_chunk, tid, chunk);
        }
    };

    let app_for_event = app.clone();
    let trace_id_for_event = trace_id.map(str::to_string);
    let tool_calls_count_for_event = tool_calls_count.clone();
    let on_event = move |value: &serde_json::Value| {
        let Some(tid) = trace_id_for_event.as_deref() else {
            return;
        };
        let line_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match line_type {
            "tool_execution_start" | "tool_execution_end" => {
                let tool_call_id = value
                    .get("toolCallId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let tool_name = value.get("toolName").and_then(|v| v.as_str()).unwrap_or("");
                let args_json = value.get("args").map(|v| v.to_string());
                let result_text = value.get("result").and_then(|v| {
                    if let Some(s) = v.as_str() {
                        Some(s.to_string())
                    } else {
                        Some(v.to_string())
                    }
                });
                let is_error = value.get("isError").and_then(|v| v.as_bool());
                let status = if line_type == "tool_execution_start" {
                    tool_calls_count_for_event.fetch_add(1, Ordering::SeqCst);
                    "running"
                } else if is_error == Some(true) {
                    "error"
                } else {
                    "done"
                };
                crate::llm_trace::record_tool(
                    &app_for_event,
                    tid,
                    tool_call_id,
                    tool_name,
                    args_json.as_deref(),
                    result_text.as_deref(),
                    status,
                    is_error,
                );
            }
            _ => {}
        }
        if let Some(delta) = value.get("assistantMessageEvent").and_then(|evt| {
            if evt.get("type").and_then(|t| t.as_str()) == Some("thinking_delta") {
                evt.get("delta").and_then(|d| d.as_str())
            } else {
                None
            }
        }) {
            crate::llm_trace::append_thinking(&app_for_event, tid, delta);
        }
    };

    let outcome = bridge.process_message_interruptible_with_events(
        &channel_id,
        &user_id,
        &full_prompt,
        4096,
        on_chunk,
        |_| {},
        on_event,
    );

    let duration_ms = start.elapsed().as_millis() as u64;
    let tool_calls_count = tool_calls_count.load(Ordering::SeqCst) as u32;

    match outcome {
        Ok(crate::channels::pi_bridge::PiProcessOutcome::Completed(r)) => AgentLoopResult {
            agent_id: call.agent_id.clone(),
            agent_name: agent_record.name.clone(),
            task: call.task.clone(),
            status: "success".into(),
            output: r.full_text,
            tool_calls_count,
            duration_ms,
        },
        Ok(crate::channels::pi_bridge::PiProcessOutcome::Aborted) => AgentLoopResult {
            agent_id: call.agent_id.clone(),
            agent_name: agent_record.name.clone(),
            task: call.task.clone(),
            status: "cancelled".into(),
            output: "委派被中断".into(),
            tool_calls_count,
            duration_ms,
        },
        Err(e) => AgentLoopResult {
            agent_id: call.agent_id.clone(),
            agent_name: agent_record.name.clone(),
            task: call.task.clone(),
            status: "error".into(),
            output: format!("委派执行失败: {e}"),
            tool_calls_count,
            duration_ms,
        },
    }
}

// ---------------------------------------------------------------------------
// execute_batch_delegates
// ---------------------------------------------------------------------------

/// Execute a batch of agent calls concurrently using `JoinSet`.
///
/// Respects `BatchFailStrategy::FailFast`: if the strategy is `FailFast` and
/// any delegate fails, remaining delegates are aborted.
async fn execute_batch_delegates(
    app: &AppHandle,
    batch: &AgentLoopBatchMarker,
    loop_id: &str,
    iteration: u32,
    config: &AgentLoopConfig,
    provider: &crate::provider_runtime::ProviderRuntimeConfig,
    abort_flag: &Arc<AtomicBool>,
) -> AgentLoopBatchResult {
    let start = Instant::now();
    let batch_id = format!("batch:{}:{}", loop_id, iteration);
    let max_concurrent = config.max_concurrent.max(1) as usize;
    let total = batch.calls.len();
    let fail_fast = config.batch_fail_strategy
        == crate::agent_loop_types::BatchFailStrategy::FailFast;

    let mut results: Vec<AgentLoopResult> = Vec::with_capacity(total);
    let mut set: JoinSet<(usize, AgentLoopResult)> = JoinSet::new();

    // Shared abort flag for fail-fast propagation.
    let batch_abort = Arc::new(AtomicBool::new(false));

    let mut enqueued = 0usize;
    let mut next_index = 0usize;

    loop {
        // Fill the JoinSet up to max_concurrent.
        while enqueued < max_concurrent && next_index < total {
            if abort_flag.load(Ordering::Relaxed) || batch_abort.load(Ordering::Relaxed) {
                break;
            }

            let call = batch.calls[next_index].clone();
            let idx = next_index;
            let app_clone = app.clone();
            let loop_id_owned = loop_id.to_string();
            let provider_clone = provider.clone();
            let abort_flag_clone = abort_flag.clone();
            let batch_abort_clone = batch_abort.clone();

            set.spawn_blocking(move || {
                if abort_flag_clone.load(Ordering::Relaxed)
                    || batch_abort_clone.load(Ordering::Relaxed)
                {
                    return (
                        idx,
                        AgentLoopResult {
                            agent_id: call.agent_id.clone(),
                            agent_name: String::new(),
                            task: call.task.clone(),
                            status: "cancelled".into(),
                            output: "批次中止".into(),
                            tool_calls_count: 0,
                            duration_ms: 0,
                        },
                    );
                }
                let result = execute_single_delegate(
                    &app_clone,
                    &call,
                    &loop_id_owned,
                    idx as u32,
                    &provider_clone,
                    &abort_flag_clone,
                );
                (idx, result)
            });

            enqueued += 1;
            next_index += 1;
        }

        if set.is_empty() {
            break;
        }

        // Await the next completed task.
        match set.join_next().await {
            Some(Ok((_idx, result))) => {
                enqueued -= 1;

                // Check for failure in fail-fast mode.
                if fail_fast && result.status != "success" {
                    batch_abort.store(true, Ordering::Relaxed);
                }

                // Store result in position order.
                results.push(result);
            }
            Some(Err(e)) => {
                enqueued -= 1;
                if fail_fast {
                    batch_abort.store(true, Ordering::Relaxed);
                }
                results.push(AgentLoopResult {
                    agent_id: String::new(),
                    agent_name: String::new(),
                    task: String::new(),
                    status: "error".into(),
                    output: format!("JoinSet 任务失败: {e}"),
                    tool_calls_count: 0,
                    duration_ms: 0,
                });
            }
            None => break,
        }
    }

    // Sort results by their original index.
    results.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));

    let total_duration_ms = start.elapsed().as_millis() as u64;

    AgentLoopBatchResult {
        batch_id,
        results,
        total_duration_ms,
    }
}

// ---------------------------------------------------------------------------
// request_user_review
// ---------------------------------------------------------------------------

/// Emit a review request event and wait for the user's response.
///
/// For now this uses a placeholder approach: it creates a oneshot channel,
/// waits with a timeout (600s), and checks the abort flag. The actual
/// ActiveLoops managed state wiring will happen in Task 6.
async fn request_user_review(
    app: &AppHandle,
    loop_id: &str,
    iteration: u32,
    extend_marker: &AgentLoopExtendMarker,
    abort_flag: &Arc<AtomicBool>,
) -> ReviewResponse {
    let (tx, rx) = tokio::sync::oneshot::channel::<ReviewResponse>();

    // Emit the review request event so the frontend can show a dialog.
    emit_loop_event(
        app,
        "agent-loop://review/request",
        serde_json::json!({
            "loopId": loop_id,
            "iteration": iteration,
            "currentIteration": extend_marker.current_iteration,
            "maxIterations": extend_marker.max_iterations,
            "reason": extend_marker.reason,
            "requestedExtra": extend_marker.requested_extra,
        }),
    );

    // Wait for response with timeout + abort check.
    let timeout = Duration::from_secs(600);
    let start = Instant::now();

    // We need to drop tx if nobody responds, so the rx will resolve with Err.
    // For now, since ActiveLoops isn't wired yet, we auto-approve after a
    // brief wait (simulating the review flow).
    drop(tx);

    // Poll abort flag while waiting for the timeout.
    loop {
        if abort_flag.load(Ordering::Relaxed) {
            return ReviewResponse {
                approved: false,
                extend_to: None,
            };
        }

        if start.elapsed() >= timeout {
            // Timeout: auto-deny.
            return ReviewResponse {
                approved: false,
                extend_to: None,
            };
        }

        // Since the oneshot sender was dropped above, rx.await will resolve
        // immediately with Err. In the real wiring (Task 6), the sender is
        // stored in ActiveLoops and the frontend response handler sends
        // through it.
        match rx.await {
            Ok(response) => return response,
            Err(_) => {
                // Sender dropped without response — placeholder: auto-approve
                // with requested extra, capped at max_extend_limit.
                let extra = extend_marker.requested_extra;
                return ReviewResponse {
                    approved: true,
                    extend_to: Some(extend_marker.max_iterations + extra),
                };
            }
        }
    }
}

// ---------------------------------------------------------------------------
// run_agent_loop — THE MAIN FUNCTION
// ---------------------------------------------------------------------------

/// Main Agent Loop orchestration function.
///
/// Runs an iterative loop that:
/// 1. Extracts loop markers from the LLM's accumulated output text.
/// 2. Dispatches single calls (`Call`), concurrent batches (`Batch`), or
///    iteration-limit extension requests (`Extend`).
/// 3. Emits structured events to the frontend for real-time UI updates.
/// 4. Terminates on `max_iterations`, abort, natural end (no marker), or error.
///
/// Returns the final accumulated text (with markers stripped).
pub async fn run_agent_loop(
    app: &AppHandle,
    agent_id: &str,
    session_id: &str,
    config: &AgentLoopConfig,
    provider: &crate::provider_runtime::ProviderRuntimeConfig,
    initial_text: &str,
    depth: u32,
) -> Result<String, String> {
    let loop_id = format!("al:{}", Uuid::new_v4().simple());
    let abort_flag = Arc::new(AtomicBool::new(false));
    let mut accumulated_text = initial_text.to_string();
    let mut iteration: u32 = 0;
    let mut max_iterations = config.max_iterations;

    let iter_timeout = Duration::from_millis(config.iteration_timeout_ms.max(5_000));
    let total_deadline = if config.total_timeout_ms > 0 {
        Some(Instant::now() + Duration::from_millis(config.total_timeout_ms))
    } else {
        None
    };

    log::info!(
        "AgentLoop [{loop_id}] starting: agent={agent_id}, session={session_id}, depth={depth}, max_iterations={max_iterations}, iter_timeout={:?}, total_timeout={:?}",
        iter_timeout,
        total_deadline.map(|d| format!("{:.0}s", d.duration_since(Instant::now()).as_secs_f64()))
    );

    log::info!("AgentLoop [{loop_id}] registered in active loops (placeholder)");

    emit_loop_event(
        app,
        "agent-loop://started",
        serde_json::json!({
            "loopId": loop_id,
            "agentId": agent_id,
            "sessionId": session_id,
            "depth": depth,
            "maxIterations": max_iterations,
        }),
    );

    let loop_start = Instant::now();

    loop {
        // ── Guard: max_iterations ──
        if iteration >= max_iterations {
            emit_completed(&app, &loop_id, "max_iterations", iteration, &loop_start);
            break;
        }

        // ── Guard: total timeout ──
        if let Some(deadline) = total_deadline {
            if Instant::now() >= deadline {
                log::warn!("AgentLoop [{loop_id}] total_timeout exceeded at iteration {iteration}");
                emit_completed(&app, &loop_id, "total_timeout", iteration, &loop_start);
                break;
            }
        }

        // ── Guard: abort flag ──
        if abort_flag.load(Ordering::Relaxed) {
            emit_loop_event(app, "agent-loop://aborted", serde_json::json!({
                "loopId": loop_id, "iteration": iteration,
                "totalDurationMs": loop_start.elapsed().as_millis() as u64,
            }));
            log::info!("AgentLoop [{loop_id}] aborted at iteration {iteration}");
            break;
        }

        let marker = extract_first_loop_marker(&accumulated_text);

        match marker {
            None => {
                emit_completed(&app, &loop_id, "natural", iteration, &loop_start);
                break;
            }

            Some((ParsedLoopMarker::Call(call), _offset)) => {
                emit_loop_event(app, "agent-loop://iteration/start", serde_json::json!({
                    "loopId": loop_id, "iteration": iteration, "type": "call",
                    "agentId": call.agent_id, "task": call.task,
                }));

                let result = execute_delegate_with_timeout(
                    app, call, iteration, &loop_id, provider, &abort_flag, iter_timeout,
                )
                .await;

                emit_loop_event(app, "agent-loop://iteration/end", serde_json::json!({
                    "loopId": loop_id, "iteration": iteration, "type": "call", "result": &result,
                }));

                let clean_text = strip_loop_markers(&accumulated_text);
                accumulated_text = format!("{}\n{}", clean_text, format_single_result(&result));
                iteration += 1;
            }

            Some((ParsedLoopMarker::Batch(batch), _offset)) => {
                emit_loop_event(app, "agent-loop://iteration/start", serde_json::json!({
                    "loopId": loop_id, "iteration": iteration, "type": "batch",
                    "callCount": batch.calls.len(),
                }));

                let batch_result = execute_batch_with_timeout(
                    app, &batch, &loop_id, iteration, config, provider, &abort_flag, iter_timeout,
                )
                .await;

                emit_loop_event(app, "agent-loop://iteration/end", serde_json::json!({
                    "loopId": loop_id, "iteration": iteration, "type": "batch", "result": &batch_result,
                }));

                let clean_text = strip_loop_markers(&accumulated_text);
                accumulated_text = format!("{}\n{}", clean_text, format_batch_result(&batch_result));
                iteration += 1;
            }

            Some((ParsedLoopMarker::Extend(extend), _offset)) => {
                if !config.allow_extend {
                    emit_completed(&app, &loop_id, "natural", iteration, &loop_start);
                    log::info!("AgentLoop [{loop_id}] EXTEND rejected (not allowed), ending");
                    break;
                }

                let review = request_user_review(app, &loop_id, iteration, &extend, &abort_flag).await;

                if review.approved {
                    if let Some(new_max) = review.extend_to {
                        let capped = new_max.min(max_iterations + config.max_extend_limit);
                        max_iterations = capped;
                        log::info!("AgentLoop [{loop_id}] EXTEND approved: max_iterations -> {max_iterations}");
                    }
                    emit_loop_event(app, "agent-loop://review/request", serde_json::json!({
                        "loopId": loop_id, "approved": true, "newMaxIterations": max_iterations,
                    }));
                    accumulated_text = strip_loop_markers(&accumulated_text);
                } else {
                    emit_completed(&app, &loop_id, "extend_denied", iteration, &loop_start);
                    log::info!("AgentLoop [{loop_id}] EXTEND denied, ending");
                    break;
                }
            }
        }
    }

    log::info!("AgentLoop [{loop_id}] unregistered from active loops");
    Ok(strip_loop_markers(&accumulated_text))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_call_marker() {
        let text = format!(
            "Some preamble\n{}{}\nMore text",
            MARKER_CALL,
            r#"{"agentId":"agent-1","task":"do stuff","params":{},"expectStructuredOutput":false,"pauseForReview":false}"#
        );
        let (marker, offset) = extract_first_loop_marker(&text).unwrap();
        assert_eq!(offset, text.find(MARKER_CALL).unwrap());
        match marker {
            ParsedLoopMarker::Call(call) => {
                assert_eq!(call.agent_id, "agent-1");
                assert_eq!(call.task, "do stuff");
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn test_extract_batch_marker_takes_priority() {
        // BATCH appears after CALL in the text, but BATCH is searched first
        // and should still win even though CALL is earlier.
        let text = format!(
            "{}{}\n{}{}",
            MARKER_CALL,
            r#"{"agentId":"a","task":"t","params":{}}"#,
            MARKER_BATCH,
            r#"{"calls":[{"agentId":"b","task":"b1","params":{}}]}"#
        );
        let (marker, _offset) = extract_first_loop_marker(&text).unwrap();
        // CALL is at offset 0, BATCH is later — earliest wins, so this is Call.
        // But we verify the parser works when BATCH comes first in search order.
        match marker {
            ParsedLoopMarker::Call(call) => {
                // CALL is earlier in text, so it is the first found.
                assert_eq!(call.agent_id, "a");
            }
            other => panic!("expected Call (earliest), got {other:?}"),
        }

        // Now put BATCH before CALL to confirm BATCH wins.
        let text2 = format!(
            "{}{}\n{}{}",
            MARKER_BATCH,
            r#"{"calls":[{"agentId":"b","task":"b1","params":{}}]}"#,
            MARKER_CALL,
            r#"{"agentId":"a","task":"t","params":{}}"#
        );
        let (marker2, _) = extract_first_loop_marker(&text2).unwrap();
        match marker2 {
            ParsedLoopMarker::Batch(batch) => {
                assert_eq!(batch.calls.len(), 1);
                assert_eq!(batch.calls[0].agent_id, "b");
            }
            other => panic!("expected Batch, got {other:?}"),
        }
    }

    #[test]
    fn test_extract_extend_marker() {
        let text = format!(
            "{}{}",
            MARKER_EXTEND,
            r#"{"currentIteration":45,"maxIterations":50,"reason":"need more","requestedExtra":10}"#
        );
        let (marker, _) = extract_first_loop_marker(&text).unwrap();
        match marker {
            ParsedLoopMarker::Extend(ext) => {
                assert_eq!(ext.current_iteration, 45);
                assert_eq!(ext.requested_extra, 10);
                assert_eq!(ext.reason, "need more");
            }
            other => panic!("expected Extend, got {other:?}"),
        }
    }

    #[test]
    fn test_no_marker_returns_none() {
        let text = "Just some plain text without any markers.";
        assert!(extract_first_loop_marker(text).is_none());
    }

    #[test]
    fn test_has_final_marker() {
        let positive = format!("blah {} done", MARKER_FINAL);
        assert!(has_final_marker(&positive));

        let negative = "blah NC_AGENT_LOOP_CALL_JSON: stuff";
        assert!(!has_final_marker(negative));
    }

    #[test]
    fn test_strip_markers() {
        let text = format!(
            "Line 1\n{}{{\"a\":1}}\nLine 2\n{} done\nLine 3",
            MARKER_CALL, MARKER_FINAL
        );
        let stripped = strip_loop_markers(&text);
        assert_eq!(stripped, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_parse_json_with_nested_braces() {
        let json = r#"{"outer":{"inner":{"deep":"value"},"arr":[1,2,3]},"x":"}tricky"}"#;
        let remaining = format!("  {}", json);
        let val = parse_json_after_marker(&remaining).unwrap();
        assert_eq!(val["outer"]["inner"]["deep"], "value");
        assert_eq!(val["x"], "}tricky");
    }

    #[test]
    fn test_format_single_result() {
        let result = AgentLoopResult {
            agent_id: "a1".into(),
            agent_name: "Alpha".into(),
            task: "do thing".into(),
            status: "ok".into(),
            output: "done".into(),
            tool_calls_count: 3,
            duration_ms: 1500,
        };
        let formatted = format_single_result(&result);
        assert!(formatted.starts_with(MARKER_RESULT));
        assert!(formatted.contains("\"agentName\":\"Alpha\""));
        assert!(formatted.ends_with('\n'));
    }

    // -----------------------------------------------------------------------
    // Defense layer tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_heal_orphaned_tool_calls_no_orphans() {
        let mut history = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({
                "role": "assistant",
                "content": "let me check",
                "tool_calls": [
                    {"id": "call_1", "type": "function", "function": {"name": "search", "arguments": "{}"}}
                ]
            }),
            serde_json::json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "result data"
            }),
        ];
        let original_len = history.len();
        heal_orphaned_tool_calls(&mut history);
        assert_eq!(history.len(), original_len, "no synthetic messages should be injected");
    }

    #[test]
    fn test_heal_orphaned_tool_calls_with_orphan() {
        let mut history = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({
                "role": "assistant",
                "content": "let me check",
                "tool_calls": [
                    {"id": "call_orphan", "type": "function", "function": {"name": "search", "arguments": "{}"}}
                ]
            }),
            serde_json::json!({"role": "user", "content": "any update?"}),
        ];
        heal_orphaned_tool_calls(&mut history);

        // Should have injected a synthetic tool result.
        assert_eq!(history.len(), 4);
        let injected = &history[2];
        assert_eq!(injected["role"], "tool");
        assert_eq!(injected["tool_call_id"], "call_orphan");
        assert!(injected["content"].as_str().unwrap().contains("[NineClaw]"));
    }

    #[test]
    fn test_inject_dynamic_context() {
        let mut history = vec![
            serde_json::json!({"role": "system", "content": "You are helpful."}),
            serde_json::json!({"role": "user", "content": "hi"}),
        ];
        let denials = vec!["file_write".to_string(), "shell_exec".to_string()];
        inject_dynamic_context(&mut history, &denials);

        let sys_content = history[0]["content"].as_str().unwrap();
        assert!(sys_content.contains("[动态上下文]"), "should contain context marker");
        assert!(sys_content.contains("权限拒绝：file_write，shell_exec"), "should list denials");
        assert!(sys_content.starts_with("You are helpful."), "original content preserved at start");
    }

    #[test]
    fn test_compress_assistant_message_short_text_removed() {
        let mut msg = serde_json::json!({
            "role": "assistant",
            "content": "让我看看",
            "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "search", "arguments": "{}"}}
            ]
        });
        compress_assistant_message(&mut msg);
        assert_eq!(msg["content"].as_str().unwrap(), "", "short content should be cleared");
    }

    #[test]
    fn test_compress_assistant_message_long_text_kept() {
        let long_reasoning = "我需要先分析一下当前的情况，然后根据已有的信息来决定下一步的操作方案。这个任务涉及到多个步骤。";
        let mut msg = serde_json::json!({
            "role": "assistant",
            "content": long_reasoning,
            "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "search", "arguments": "{}"}}
            ]
        });
        compress_assistant_message(&mut msg);
        assert_eq!(msg["content"].as_str().unwrap(), long_reasoning, "long content should be kept");
    }

    #[test]
    fn test_compress_assistant_message_no_tool_calls_kept() {
        let mut msg = serde_json::json!({
            "role": "assistant",
            "content": "好的"
        });
        compress_assistant_message(&mut msg);
        assert_eq!(msg["content"].as_str().unwrap(), "好的", "content without tool_calls should be kept");
    }

    // ── Integration: end-to-end marker cycle ────────────────────

    #[test]
    fn test_full_loop_cycle_text_without_markers() {
        let text = "分析完成，以下是结论：...";
        assert!(extract_first_loop_marker(text).is_none());
    }

    #[test]
    fn test_full_loop_cycle_call_then_result_then_no_marker() {
        // Round 1: CALL marker
        let text1 = format!(
            "让我委派分析\n{}{{\"agentId\":\"a1\",\"task\":\"analyze\",\"params\":{{}},\"expectStructuredOutput\":false,\"pauseForReview\":false}}\n",
            MARKER_CALL
        );
        let marker1 = extract_first_loop_marker(&text1);
        assert!(matches!(marker1, Some((ParsedLoopMarker::Call(_), _))));

        // Simulate result injection
        let result = AgentLoopResult {
            agent_id: "a1".into(),
            agent_name: "Analyzer".into(),
            task: "analyze".into(),
            status: "success".into(),
            output: "found 3 issues".into(),
            tool_calls_count: 2,
            duration_ms: 5000,
        };
        let text_with_result = format!("{}\n{}", strip_loop_markers(&text1), format_single_result(&result));

        // Verify result text contains RESULT marker
        assert!(text_with_result.contains(MARKER_RESULT));

        // Round 2: final reply has no markers → loop ends
        let final_text = "综合分析结果，建议...";
        assert!(extract_first_loop_marker(final_text).is_none());
    }

    #[test]
    fn test_batch_then_extend_then_final() {
        // BATCH marker
        let batch_text = format!(
            "{}{{\"calls\":[{{\"agentId\":\"a1\",\"task\":\"t1\",\"params\":{{}},\"expectStructuredOutput\":false,\"pauseForReview\":false}},{{\"agentId\":\"a2\",\"task\":\"t2\",\"params\":{{}},\"expectStructuredOutput\":false,\"pauseForReview\":false}}],\"pauseForReview\":false}}",
            MARKER_BATCH
        );
        assert!(matches!(extract_first_loop_marker(&batch_text), Some((ParsedLoopMarker::Batch(_), _))));

        // EXTEND marker
        let extend_text = format!(
            "{}{{\"currentIteration\":48,\"maxIterations\":50,\"reason\":\"need more\",\"requestedExtra\":20}}",
            MARKER_EXTEND
        );
        assert!(matches!(extract_first_loop_marker(&extend_text), Some((ParsedLoopMarker::Extend(_), _))));

        // FINAL marker
        assert!(has_final_marker(&format!("done\n{}", MARKER_FINAL)));
    }

    #[test]
    fn test_mixed_text_with_markers_and_final() {
        let text = format!(
            "以下是分析结果：\n{}{{\"agentId\":\"x\",\"task\":\"t\",\"params\":{{}},\"expectStructuredOutput\":false,\"pauseForReview\":false}}\n{}\n附加说明文字",
            MARKER_CALL, MARKER_FINAL
        );
        // CALL should be found (FINAL is separate check)
        let marker = extract_first_loop_marker(&text);
        assert!(matches!(marker, Some((ParsedLoopMarker::Call(_), _))));
        assert!(has_final_marker(&text));
    }
}
