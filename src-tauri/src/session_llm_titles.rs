use crate::agents;
use crate::channels::pi_bridge::{PiBridge, PiProcessOutcome};
use crate::dev_trace::dev_trace;
use crate::history_app_state::storage_conn;
use crate::llm_trace;
use crate::pi_runtime;
use crate::prompts;
use crate::provider_runtime::{normalized_provider_runtime_base_url, resolve_im_llm_runtime};
use crate::storage;
use tauri::AppHandle;

const TASK_METADATA_LLM_CHUNK: usize = 512;
const SESSION_TITLE_LLM_CHUNK: usize = 512;

fn clamp_display_chars(value: &str, max_chars: usize) -> String {
    let t = value.trim();
    if t.chars().count() <= max_chars {
        return t.to_string();
    }
    t.chars().take(max_chars).collect()
}

const TASK_METADATA_GOAL_MAX_CHARS: usize = 4000;

fn clamp_task_execution_body(value: &str, max_chars: usize) -> String {
    let t = value.trim();
    if t.is_empty() || max_chars == 0 {
        return String::new();
    }
    if t.chars().count() <= max_chars {
        return t.to_string();
    }
    format!(
        "{}…",
        t.chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>()
    )
}

fn parse_agent_task_metadata_llm_output(
    raw: &str,
    fallback_goal: &str,
) -> Option<(String, String, String)> {
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    let slice = &trimmed[start..=end];
    let v: serde_json::Value = serde_json::from_str(slice).ok()?;
    let title = v
        .get("title")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let summary = v
        .get("summary")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let goal_from_llm = v
        .get("goal")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let goal = clamp_task_execution_body(
        goal_from_llm.as_deref().unwrap_or(fallback_goal.trim()),
        TASK_METADATA_GOAL_MAX_CHARS,
    );
    if goal.is_empty() {
        return None;
    }
    Some((
        clamp_display_chars(&title, 28),
        clamp_display_chars(&summary, 120),
        goal,
    ))
}

/// 用智能体绑定的模型提炼列表标题、一句话介绍，以及到点执行用的任务正文（写入 payload，非原始聊天记录）；失败返回 `None`（由调用方回退）。
pub(crate) fn refine_agent_task_metadata(
    app: &AppHandle,
    agent_id: &str,
    goal: &str,
    task_type: &str,
    schedule_hint: &str,
) -> Option<(String, String, String)> {
    if std::env::var("NINECLAW_SKIP_TASK_METADATA_LLM")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
    {
        return None;
    }
    let goal = goal.trim();
    if goal.is_empty() {
        return None;
    }
    let record = agents::get_agent_record(app, agent_id).ok().flatten()?;
    let agent_config = agents::get_conversation_agent_config(app, agent_id)
        .ok()
        .flatten()?;
    let (provider_for_task_meta, model_for_task_meta) =
        if let Some(ref sc) = record.scenario_llm_config {
            if let Some(ref slot) = sc.task_push_notification_copy {
                let p = slot.provider_id.trim();
                let m = slot.model.trim();
                if !p.is_empty() && !m.is_empty() {
                    (p.to_string(), m.to_string())
                } else {
                    (
                        record.default_provider_id.clone(),
                        record.default_model.clone(),
                    )
                }
            } else {
                (
                    record.default_provider_id.clone(),
                    record.default_model.clone(),
                )
            }
        } else {
            (
                record.default_provider_id.clone(),
                record.default_model.clone(),
            )
        };
    let runtime =
        resolve_im_llm_runtime(app, &provider_for_task_meta, &model_for_task_meta).ok()?;
    let base_normalized = normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );
    let pi_rt = pi_runtime::require_pi_runtime_location(app).ok()?;
    let bridge = PiBridge::new(
        pi_rt,
        &runtime.provider_id,
        &runtime.api_format,
        &base_normalized,
        &runtime.api_key,
        &runtime.model,
        Some(agent_config),
    );
    let channel_id = format!("nc:taskmeta:{agent_id}");
    let user_id = format!("refine_{}", uuid::Uuid::new_v4().simple());
    let task_type_label = match task_type.trim() {
        "reminder" => "reminder（到点仅提醒）",
        "agent_prompt" => "agent_prompt（到点由智能体执行）",
        other => other,
    };
    let prompt = prompts::build_task_metadata_prompt(task_type_label, schedule_hint, goal);
    let outcome = bridge
        .process_message_interruptible(
            &channel_id,
            &user_id,
            &prompt,
            TASK_METADATA_LLM_CHUNK,
            |_| {},
            |_| {},
        )
        .ok()?;
    let text = match outcome {
        PiProcessOutcome::Completed(r) => r.full_text,
        PiProcessOutcome::Aborted => return None,
    };
    let parsed = parse_agent_task_metadata_llm_output(&text, goal);
    if parsed.is_none() {
        dev_trace(
            "task.meta",
            format!(
                "LLM 元数据解析失败，输出前 200 字：{}",
                text.chars().take(200).collect::<String>()
            ),
        );
    }
    parsed
}

fn clamp_chars_head(value: &str, max_chars: usize) -> String {
    let t = value.trim();
    if t.is_empty() || max_chars == 0 {
        return String::new();
    }
    if t.chars().count() <= max_chars {
        return t.to_string();
    }
    format!(
        "{}…",
        t.chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>()
    )
}

fn parse_session_title_llm_output(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end >= start {
                let slice = &trimmed[start..=end];
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(slice) {
                    if let Some(t) = v
                        .get("title")
                        .and_then(|x| x.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        return Some(clamp_display_chars(t, 28));
                    }
                }
            }
        }
    }
    let line = trimmed.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line
        .trim_matches(|c| {
            matches!(
                c,
                '"' | '`' | '\'' | '「' | '『' | '【' | '」' | '』' | '】'
            )
        })
        .trim();
    if line.is_empty() {
        return None;
    }
    Some(clamp_display_chars(line, 28))
}

/// 用智能体「标题生成」场景模型（未配置则用默认对话模型）只根据首轮用户提问生成会话列表短标题。
fn generate_conversation_session_title_llm(
    app: &AppHandle,
    agent_id: &str,
    session_id: Option<&str>,
    user_message: &str,
    _assistant_message: &str,
) -> Result<String, String> {
    if std::env::var("NINECLAW_SKIP_SESSION_TITLE_LLM")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
    {
        return Ok(String::new());
    }

    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Ok(String::new());
    }

    let um = user_message.trim();
    if um.is_empty() {
        return Ok(String::new());
    }

    let Some(record) = agents::get_agent_record(app, agent_id)? else {
        return Ok(String::new());
    };
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let (provider_for_title, model_for_title) = if let Some(ref sc) = record.scenario_llm_config {
        if let Some(ref slot) = sc.title_generation {
            let p = slot.provider_id.trim();
            let m = slot.model.trim();
            if !p.is_empty() && !m.is_empty() {
                (p.to_string(), m.to_string())
            } else {
                (
                    record.default_provider_id.clone(),
                    record.default_model.clone(),
                )
            }
        } else {
            (
                record.default_provider_id.clone(),
                record.default_model.clone(),
            )
        }
    } else {
        (
            record.default_provider_id.clone(),
            record.default_model.clone(),
        )
    };

    let runtime = resolve_im_llm_runtime(app, &provider_for_title, &model_for_title)?;
    let base_normalized = normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );
    let pi_rt = pi_runtime::require_pi_runtime_location(app)?;
    let bridge = PiBridge::new(
        pi_rt,
        &runtime.provider_id,
        &runtime.api_format,
        &base_normalized,
        &runtime.api_key,
        &runtime.model,
        None,
    );
    let channel_id = format!("nc:sessiontitle:{agent_id}");
    let user_id = format!("title_{}", uuid::Uuid::new_v4().simple());
    let um_snip = clamp_chars_head(um, 6000);
    let prompt = prompts::build_session_title_prompt(&um_snip);
    let workspace_id = session_id.as_deref().and_then(|sid| {
        storage_conn(app)
            .ok()
            .and_then(|conn| {
                storage::chat_history::get_chat_session(&conn, sid)
                    .ok()
                    .flatten()
            })
            .and_then(|session| session.workspace_id)
    });
    let trace_enabled = if let Some(wid) = workspace_id.as_deref() {
        storage_conn(app)
            .ok()
            .and_then(|conn| {
                storage::workspaces::get_workspace(&conn, wid)
                    .ok()
                    .flatten()
            })
            .map(|workspace| workspace.llm_trace_enabled != 0)
            .unwrap_or(false)
    } else {
        session_id.is_some()
    };
    let mut trace_guard = if trace_enabled {
        let system_prompts = vec![llm_trace::TraceSystemPromptSection {
            label: "title_generation_action".to_string(),
            content: "为当前 session 生成简短标题；只参考首条用户消息，不参考助手回复或 agent 系统提示。只输出最终标题或 JSON 包裹的标题字段。"
                .to_string(),
        }];
        Some(llm_trace::TraceGuard::new(
            app,
            llm_trace::begin(
                app,
                workspace_id.as_deref(),
                "action_llm",
                "action:session_title",
                "标题生成",
                None,
                Some("LLM"),
                session_id.as_deref(),
                Some(&runtime.provider_id),
                Some(&runtime.model),
                system_prompts,
                &prompt,
            ),
        ))
    } else {
        None
    };
    let trace_id = trace_guard
        .as_ref()
        .and_then(|guard| guard.id())
        .map(ToOwned::to_owned);
    let trace_id_for_chunk = trace_id.clone();
    let trace_id_for_event = trace_id.clone();
    let outcome = bridge.process_message_interruptible_with_events(
        &channel_id,
        &user_id,
        &prompt,
        SESSION_TITLE_LLM_CHUNK,
        |chunk| {
            if let Some(tid) = trace_id_for_chunk.as_deref() {
                llm_trace::append_response(app, tid, chunk);
            }
        },
        |_| {},
        |event| {
            let Some(tid) = trace_id_for_event.as_deref() else {
                return;
            };
            if let Some(delta) = event.get("assistantMessageEvent").and_then(|evt| {
                if evt.get("type").and_then(|value| value.as_str()) == Some("thinking_delta") {
                    evt.get("delta").and_then(|value| value.as_str())
                } else {
                    None
                }
            }) {
                llm_trace::append_thinking(app, tid, delta);
            }
        },
    )?;
    let text = match outcome {
        PiProcessOutcome::Completed(r) => {
            if let Some(guard) = trace_guard.as_mut() {
                guard.finalize_done(
                    Some(r.full_text.clone()),
                    None,
                    Some(runtime.provider_id.clone()),
                    Some(runtime.model.clone()),
                    None,
                );
            }
            r.full_text
        }
        PiProcessOutcome::Aborted => {
            if let Some(guard) = trace_guard.as_mut() {
                guard.finalize_error("标题生成被中断".to_string());
            }
            return Err("生成会话标题时 pi 被中断".to_string());
        }
    };
    if let Some(title) = parse_session_title_llm_output(&text) {
        return Ok(title);
    }
    dev_trace(
        "session.title",
        format!(
            "会话标题 LLM 解析失败，输出前 200 字：{}",
            text.chars().take(200).collect::<String>()
        ),
    );
    Ok(String::new())
}

#[tauri::command]
pub(crate) async fn generate_session_conversation_title(
    app: AppHandle,
    agent_id: String,
    session_id: Option<String>,
    user_message: String,
    assistant_message: Option<String>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        generate_conversation_session_title_llm(
            &app,
            &agent_id,
            session_id.as_deref(),
            &user_message,
            assistant_message.as_deref().unwrap_or_default(),
        )
    })
    .await
    .map_err(|error| format!("生成会话标题任务失败: {error}"))?
}
