//! 设置面板「重新整理」：从 Markdown + 聊天记录抽取条目写入 K/V 与向量索引。

use crate::agents;
use crate::channels::pi_bridge::{PiBridge, PiProcessOutcome};
use crate::commands_workspace_kv_memory::resolve_kv_workspace_ui;
use crate::managed_runtime::get_embedding_registry;
use crate::pi_runtime;
use crate::provider_runtime::{normalized_provider_runtime_base_url, resolve_im_llm_runtime};
use crate::storage::chat_history::{self, ChatTurn};
use crate::storage::user_memory;
use crate::user_memory_service::{self, UserMemoryOwner};
use crate::workspace_memory_extraction::resolve_memory_extraction_model;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::future::Future;
use tauri::AppHandle;
use uuid::Uuid;

const GLOBAL_USER_KV_WORKSPACE_ID: &str =
    crate::commands_workspace_kv_memory::GLOBAL_USER_KV_WORKSPACE_ID;

const MARKDOWN_CLAMP: usize = 9000;
const CHAT_BUDGET_CHARS: usize = 18000;
const MAX_SESSION_SCAN: usize = 36;
const TURNS_PER_SESSION: usize = 7;
const REORG_PROMPT_CHUNK: usize = 640;
const MAX_TEXT_CHARS: usize = 380;

const VECTOR_DEDUP_THRESHOLD: f32 = 0.91_f32;

fn run_async<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = T>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => Ok(tokio::task::block_in_place(|| handle.block_on(future))),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("创建 Tokio runtime 失败: {e}"))?;
            Ok(rt.block_on(future))
        }
    }
}

fn clamp_chars(value: &str, max: usize) -> String {
    let t = value.trim();
    if t.is_empty() || max == 0 {
        return String::new();
    }
    if t.chars().count() <= max {
        return t.to_string();
    }
    format!(
        "{}…",
        t.chars().take(max.saturating_sub(1)).collect::<String>()
    )
}

fn normalize_fingerprint(content: &str) -> String {
    content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn try_load_agent_workspace(app: &AppHandle, agent_id: &str, rel: &str) -> String {
    agents::read_agent_workspace_file(app, agent_id.to_string(), rel.to_string())
        .map(|file| clamp_chars(file.content.trim(), MARKDOWN_CLAMP))
        .unwrap_or_default()
}

fn try_load_workspace_root(rel: &str) -> String {
    let Ok(root) = crate::agent_workspace::resolve_workspace_root() else {
        return String::new();
    };
    fs::read_to_string(root.join(rel))
        .map(|body| clamp_chars(body.trim(), MARKDOWN_CLAMP))
        .unwrap_or_default()
}

fn build_markdown_bundle_for_agent(app: &AppHandle, agent_id: &str) -> String {
    let aid = agent_id.trim();
    let prefix = format!("agents/{}/", aid);
    let chunks = vec![
        (
            "MEMORY.md",
            try_load_agent_workspace(app, aid, &format!("{prefix}MEMORY.md")),
        ),
        (
            "USER_MODEL.md",
            try_load_agent_workspace(app, aid, &format!("{prefix}USER_MODEL.md")),
        ),
        (
            "RELATIONSHIP_MAP.md",
            try_load_agent_workspace(app, aid, &format!("{prefix}RELATIONSHIP_MAP.md")),
        ),
        (
            "WORKING.md (摘录)",
            try_load_agent_workspace(app, aid, &format!("{prefix}WORKING.md")),
        ),
    ];

    chunks
        .into_iter()
        .filter(|(_, body)| !body.trim().is_empty())
        .map(|(title, body)| format!("### {title}\n{body}"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn build_markdown_bundle_global() -> String {
    [
        ("USER.md", try_load_workspace_root("USER.md")),
        ("MEMORY.md", try_load_workspace_root("MEMORY.md")),
        ("SOUL.md", try_load_workspace_root("SOUL.md")),
    ]
    .iter()
    .filter(|(_, body)| !body.trim().is_empty())
    .map(|(title, body)| format!("### {title}\n{body}"))
    .collect::<Vec<_>>()
    .join("\n\n")
}

fn render_turn(turn: &ChatTurn, session_title: &str) -> Option<String> {
    let prompt = clamp_chars(&turn.prompt, 1600);
    let answer = clamp_chars(&turn.answer, 1600);
    if prompt.trim().is_empty() && answer.trim().is_empty() {
        return None;
    }
    Some(format!(
        "### 会话：{} ({})\n用户：{}\n助手：{}\n",
        clamp_chars(session_title, 140),
        turn.turn_index,
        prompt,
        answer
    ))
}

fn build_chat_digest(
    conn: &rusqlite::Connection,
    agent_filter: Option<&str>,
    budget: usize,
) -> Result<String, String> {
    let sessions = chat_history::list_chat_sessions(conn)?;
    let mut out = String::new();

    for session in sessions.into_iter().take(MAX_SESSION_SCAN) {
        if let Some(filter_id) = agent_filter.map(str::trim).filter(|s| !s.is_empty()) {
            if session.agent_id.as_deref() != Some(filter_id) {
                continue;
            }
        }

        let title = session.title.trim();
        let title_fallback = session.id.as_str();

        let turns = chat_history::list_chat_turns(conn, &session.id)?;
        for turn in turns.iter().rev().take(TURNS_PER_SESSION) {
            if out.chars().count() >= budget {
                return Ok(out.trim().to_string());
            }
            if let Some(fragment) = render_turn(
                turn,
                if title.is_empty() {
                    title_fallback
                } else {
                    title
                },
            ) {
                if out.chars().count() + fragment.chars().count() > budget {
                    break;
                }
                out.push_str(&fragment);
            }
        }
    }

    Ok(out.trim().to_string())
}

fn build_existing_kv_outline(
    conn: &rusqlite::Connection,
    owner: &UserMemoryOwner,
) -> Result<String, String> {
    let records = user_memory::list_user_memories(conn, &owner.owner_scope, &owner.owner_id, 80)?;
    if records.is_empty() {
        return Ok(String::new());
    }

    Ok(records
        .into_iter()
        .map(|record| {
            format!(
                "- {} :: {}",
                record.memory_key,
                clamp_chars(&record.text, 260)
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

fn parse_reorganize_llm(raw: &str) -> Vec<(String, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut candidates = vec![trimmed.to_string()];
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end >= start {
            candidates.push(trimmed[start..=end].to_string());
        }
    }
    for cand in candidates {
        if let Ok(value) = serde_json::from_str::<Value>(&cand) {
            if let Some(arr) = value.get("items").and_then(|item| item.as_array()) {
                let mut parsed = Vec::new();
                for entry in arr {
                    let cat_raw = entry
                        .get("category")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_ascii_lowercase();
                    let txt = entry
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty());
                    let Some(txt) = txt else { continue };

                    parsed.push((normalize_bucket(&cat_raw), clamp_chars(txt, MAX_TEXT_CHARS)));
                }
                return parsed;
            }
        }
    }
    Vec::new()
}

fn normalize_bucket(raw: &str) -> String {
    match raw {
        "identity" | "profile" | "用户身份" | "身份" => "identity".into(),
        "work" | "workflow" | "工作" | "工作方式" => "work".into(),
        "writing" | "style" | "写作" | "文风" => "writing".into(),
        "directive" | "instruction" | "指令" | "用户指令" => "directive".into(),
        _ => "identity".into(),
    }
}

pub(crate) fn run_user_kv_memory_reorganize(
    app: AppHandle,
    agent_id: Option<String>,
    workspace_id: Option<String>,
) -> Result<Value, String> {
    let explicit_agent_param = agent_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let (wid, supervisor) = resolve_kv_workspace_ui(&app, workspace_id, agent_id)?;

    let global_scope = wid == GLOBAL_USER_KV_WORKSPACE_ID;
    let owner = user_memory_service::resolve_owner(Some(&wid), supervisor.as_deref())?;

    let agent_subject = explicit_agent_param
        .clone()
        .filter(|_| !global_scope)
        .or_else(|| supervisor.clone());

    let conn = crate::storage_conn(&app)?;

    let markdown_pack = if global_scope {
        build_markdown_bundle_global()
    } else if let Some(aid) = agent_subject.as_ref() {
        build_markdown_bundle_for_agent(&app, aid)
    } else {
        String::new()
    };

    let chat_digest = build_chat_digest(&conn, agent_subject.as_deref(), CHAT_BUDGET_CHARS)?;

    if markdown_pack.trim().is_empty() && chat_digest.trim().is_empty() {
        return Err("未读取到可用的 Markdown 或聊天记录，无法进行整理".to_string());
    }

    let kv_outline = build_existing_kv_outline(&conn, &owner)?;

    let model_agent_record = crate::agents::get_default_agent(&app)?
        .ok_or_else(|| "请先在应用中设置默认智能体，以便记忆整理调用大模型".to_string())?;

    let (provider_id, model) = resolve_memory_extraction_model(&model_agent_record);
    let runtime = resolve_im_llm_runtime(&app, &provider_id, &model)?;
    let base_normalized = normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );
    let pi_rt = pi_runtime::require_pi_runtime_location(&app)?;

    let scope_description = if global_scope {
        "全局用户记忆（共享 KV）".into()
    } else {
        format!(
            "智能体 {}（__agent_memory__ KV）",
            agent_subject.as_deref().unwrap_or("未选定"),
        )
    };

    let prompt = crate::prompts::build_user_kv_memory_reorganize_prompt(
        &scope_description,
        &markdown_pack,
        &chat_digest,
        &kv_outline,
    );

    let bridge = PiBridge::new(
        pi_rt,
        &runtime.provider_id,
        &runtime.api_format,
        &base_normalized,
        &runtime.api_key,
        &runtime.model,
        None,
    );
    let channel_id = format!("nc:user-memory-reorg:{}:{}", wid, Uuid::new_v4().simple());
    let user_tag = format!("mem_reorg:{}", Uuid::new_v4().simple());

    let outcome = bridge.process_message_interruptible_with_events(
        &channel_id,
        &user_tag,
        &prompt,
        REORG_PROMPT_CHUNK,
        |_| {},
        |_| {},
        |_| {},
    )?;

    let text = match outcome {
        PiProcessOutcome::Completed(res) => res.full_text,
        PiProcessOutcome::Aborted => {
            return Err("记忆整理被中断".to_string());
        }
    };

    let items = parse_reorganize_llm(&text);
    if items.is_empty() {
        return Ok(json!({
            "ok": true,
            "inserted": 0usize,
            "skipped": 0usize,
            "message": "模型未抽取到新条目",
        }));
    }

    let fingerprints: HashSet<String> =
        user_memory::list_user_memories(&conn, &owner.owner_scope, &owner.owner_id, 220)?
            .into_iter()
            .map(|record| normalize_fingerprint(record.text.trim()))
            .filter(|fp| fp.len() > 6)
            .collect();

    let mut seen_this_run = HashSet::<String>::new();
    let mut inserted = 0usize;
    let mut skipped = 0usize;

    let registry = get_embedding_registry();

    for (bucket, memo) in items {
        let fp = normalize_fingerprint(&memo);
        if fp.len() < 8 {
            skipped += 1;
            continue;
        }
        if fingerprints.contains(&fp) || seen_this_run.contains(&fp) {
            skipped += 1;
            continue;
        }

        let vector_dup = registry.as_ref().cloned().is_some_and(|reg| {
            match run_async(async {
                let provider = {
                    let guard = reg.read().await;
                    guard.default_provider()
                };
                let Some(provider) = provider else {
                    return Ok::<Option<String>, String>(None);
                };
                let embeddings = provider.embed(vec![memo.clone()]).await?;
                let query_vec = embeddings.into_iter().next();
                let Some(query_vec) = query_vec else {
                    return Ok(None);
                };
                let conn_vec = crate::storage_conn(&app)?;
                let hits = crate::memory_vector::search_vectors_across_workspaces(
                    &conn_vec,
                    &[owner.vector_namespace()],
                    &query_vec,
                    1,
                    VECTOR_DEDUP_THRESHOLD,
                )?;
                let hit = hits.into_iter().next().map(|entry| entry.memory_id);
                Ok(hit)
            }) {
                Ok(Ok(hit)) => hit.is_some(),
                Err(e) => {
                    log::warn!("向量去重跳过：{e}");
                    false
                }
                Ok(Err(e)) => {
                    log::warn!("向量去重跳过：{e}");
                    false
                }
            }
        });

        if vector_dup {
            skipped += 1;
            continue;
        }

        let key = format!("nc_um.{}/rg_{}", bucket, Uuid::new_v4().simple());
        let value = json!({
            "text": memo.clone(),
            "source": "migration",
        });

        user_memory_service::upsert_entry(
            &app,
            &owner,
            &key,
            &memo,
            &[],
            "migration",
            Some("chat_history_reorganize"),
            Some(&value.to_string()),
        )?;
        seen_this_run.insert(fp);
        inserted += 1;
    }

    Ok(json!({
        "ok": true,
        "inserted": inserted,
        "skipped": skipped,
        "message": format!("写入 {inserted} 条；跳过近似或重复的 {skipped} 条"),
    }))
}
