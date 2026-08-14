use crate::agents;
use crate::channels::pi_bridge::{PiBridge, PiProcessOutcome};
use crate::llm_trace;
use crate::managed_runtime::get_embedding_registry;
use crate::pi_runtime;
use crate::prompts;
use crate::provider_runtime::{normalized_provider_runtime_base_url, resolve_im_llm_runtime};
use crate::storage::chat_history::{self, ChatTurn};
use crate::storage::user_memory;
use crate::user_memory_service::{self, UserMemoryOwner};
use crate::workspace_memory_extraction::resolve_memory_extraction_model;
use serde_json::Value;
use std::collections::HashSet;
use tauri::AppHandle;
use uuid::Uuid;

const MEMORY_EXTRACTION_LLM_CHUNK: usize = 512;
const MAX_RECENT_TURNS: usize = 6;
const MAX_RECENT_MEMORIES: i64 = 12;
const MAX_MEMORY_ITEMS: usize = 3;
const MAX_TEXT_CHARS: usize = 180;
const MAX_TURN_TEXT_CHARS: usize = 1200;
const VECTOR_DEDUP_THRESHOLD: f32 = 0.93_f32;

#[derive(Clone, Debug)]
struct UserMemoryExtractionRequest {
    session_id: String,
    latest_user_prompt: String,
    latest_assistant_reply: String,
    agent_id: String,
    agent_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConversationTurn {
    turn_index: i32,
    user_prompt: String,
    assistant_reply: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExtractedUserMemory {
    bucket: String,
    text: String,
    tags: Vec<String>,
}

fn clamp_chars(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || max_chars == 0 {
        return String::new();
    }
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    format!(
        "{}…",
        trimmed
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>()
    )
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_fingerprint(value: &str) -> String {
    normalize_whitespace(value).to_lowercase()
}

fn render_recent_memories(memories: &[user_memory::UserMemoryRecord]) -> String {
    if memories.is_empty() {
        return "_暂无_".to_string();
    }
    memories
        .iter()
        .take(MAX_RECENT_MEMORIES as usize)
        .map(|memory| {
            format!(
                "- [{} / {}] {}",
                memory.origin_kind,
                memory.bucket,
                clamp_chars(&memory.text, MAX_TEXT_CHARS)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_conversation_turns(
    session_turns: &[ChatTurn],
    latest_user_prompt: &str,
    latest_assistant_reply: &str,
) -> Vec<ConversationTurn> {
    let mut turns: Vec<ConversationTurn> = session_turns
        .iter()
        .rev()
        .filter(|turn| !turn.prompt.trim().is_empty() || !turn.answer.trim().is_empty())
        .take(MAX_RECENT_TURNS)
        .map(|turn| ConversationTurn {
            turn_index: turn.turn_index,
            user_prompt: turn.prompt.trim().to_string(),
            assistant_reply: turn.answer.trim().to_string(),
        })
        .collect();
    turns.reverse();

    let latest_user_prompt = latest_user_prompt.trim();
    let latest_assistant_reply = latest_assistant_reply.trim();
    if latest_user_prompt.is_empty() || latest_assistant_reply.is_empty() {
        return turns;
    }

    let already_has_latest = turns.last().is_some_and(|turn| {
        normalize_fingerprint(&turn.user_prompt) == normalize_fingerprint(latest_user_prompt)
            && normalize_fingerprint(&turn.assistant_reply)
                == normalize_fingerprint(latest_assistant_reply)
    });
    if !already_has_latest {
        let next_index = turns.last().map(|turn| turn.turn_index + 1).unwrap_or(1);
        turns.push(ConversationTurn {
            turn_index: next_index,
            user_prompt: latest_user_prompt.to_string(),
            assistant_reply: latest_assistant_reply.to_string(),
        });
    }

    if turns.len() > MAX_RECENT_TURNS {
        turns = turns.split_off(turns.len() - MAX_RECENT_TURNS);
    }
    turns
}

fn render_conversation_excerpt(turns: &[ConversationTurn]) -> String {
    if turns.is_empty() {
        return "_暂无_".to_string();
    }
    turns
        .iter()
        .map(|turn| {
            format!(
                "### 第 {} 轮\n用户：{}\n助手：{}",
                turn.turn_index,
                clamp_chars(&turn.user_prompt, MAX_TURN_TEXT_CHARS),
                clamp_chars(&turn.assistant_reply, MAX_TURN_TEXT_CHARS)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn normalize_bucket(raw: &str) -> Option<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "identity" | "profile" => Some("identity".to_string()),
        "work" | "workflow" => Some("work".to_string()),
        "writing" | "style" | "voice" => Some("writing".to_string()),
        "directive" | "instruction" => Some("directive".to_string()),
        _ => None,
    }
}

fn parse_output(raw: &str) -> Vec<ExtractedUserMemory> {
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

    for candidate in candidates {
        let Ok(value) = serde_json::from_str::<Value>(&candidate) else {
            continue;
        };
        if value.get("shouldWrite").and_then(|item| item.as_bool()) == Some(false) {
            return Vec::new();
        }
        let Some(memories) = value.get("memories").and_then(|item| item.as_array()) else {
            continue;
        };
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for item in memories.iter().take(MAX_MEMORY_ITEMS) {
            let Some(bucket) = item
                .get("bucket")
                .and_then(|field| field.as_str())
                .and_then(normalize_bucket)
            else {
                continue;
            };
            let Some(text) = item
                .get("text")
                .and_then(|field| field.as_str())
                .map(|field| clamp_chars(field, MAX_TEXT_CHARS))
                .filter(|field| !field.is_empty())
            else {
                continue;
            };
            let mut tags = item
                .get("tags")
                .and_then(|field| field.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(|item| item.trim().to_ascii_lowercase())
                        .filter(|item| !item.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !tags.contains(&bucket) {
                tags.insert(0, bucket.clone());
            }
            let fingerprint = format!("{bucket}::{}", normalize_fingerprint(&text));
            if seen.insert(fingerprint) {
                out.push(ExtractedUserMemory { bucket, text, tags });
            }
        }
        return out;
    }

    Vec::new()
}

fn memory_exists(
    candidate: &ExtractedUserMemory,
    existing: &[user_memory::UserMemoryRecord],
) -> bool {
    let candidate_text = normalize_fingerprint(&candidate.text);
    existing.iter().any(|memory| {
        memory.bucket == candidate.bucket && normalize_fingerprint(&memory.text) == candidate_text
    })
}

fn run_user_memory_auto_extraction(
    app: &AppHandle,
    request: UserMemoryExtractionRequest,
) -> Result<usize, String> {
    let conn = crate::storage_conn(app)?;
    let Some(agent_record) = agents::get_agent_record(app, &request.agent_id)? else {
        return Err("智能体不存在".to_string());
    };
    let (provider_id, model) = resolve_memory_extraction_model(&agent_record);
    let runtime = resolve_im_llm_runtime(app, &provider_id, &model)?;
    let base_normalized = normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );
    let pi_rt = pi_runtime::require_pi_runtime_location(app)?;
    let recent_memories = user_memory::list_user_memories_for_agent(
        &conn,
        Some(&request.agent_id),
        MAX_RECENT_MEMORIES,
    )?;
    let session_turns =
        chat_history::list_chat_turns(&conn, &request.session_id).unwrap_or_default();
    let prompt = prompts::build_user_memory_auto_extraction_prompt(
        &request.agent_name,
        &render_recent_memories(&recent_memories),
        &render_conversation_excerpt(&build_conversation_turns(
            &session_turns,
            &request.latest_user_prompt,
            &request.latest_assistant_reply,
        )),
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
    let channel_id = format!("nc:user-memory:{}", request.agent_id);
    let user_id = format!(
        "user_mem_extract:{}:{}",
        request.session_id,
        Uuid::new_v4().simple()
    );
    let mut trace_guard = Some(llm_trace::TraceGuard::new(
        app,
        llm_trace::begin(
            app,
            None,
            "action_llm",
            "action:user_memory_auto_extraction",
            "用户记忆提取",
            Some(&request.agent_id),
            Some("LLM"),
            Some(&request.session_id),
            Some(&runtime.provider_id),
            Some(&runtime.model),
            vec![llm_trace::TraceSystemPromptSection {
                label: "user_memory_auto_extraction".to_string(),
                content: "从聊天记录异步提取用户长期记忆，写入 SQLite 用户记忆表。".to_string(),
            }],
            &prompt,
        ),
    ));
    let trace_id = trace_guard
        .as_ref()
        .and_then(|guard| guard.id())
        .map(ToOwned::to_owned);
    let trace_id_for_chunk = trace_id.clone();

    let outcome = bridge.process_message_interruptible_with_events(
        &channel_id,
        &user_id,
        &prompt,
        MEMORY_EXTRACTION_LLM_CHUNK,
        |chunk| {
            if let Some(trace_id) = trace_id_for_chunk.as_deref() {
                llm_trace::append_response(app, trace_id, chunk);
            }
        },
        |_| {},
        |_| {},
    )?;

    let text = match outcome {
        PiProcessOutcome::Completed(result) => {
            if let Some(guard) = trace_guard.as_mut() {
                guard.finalize_done(
                    Some(result.full_text.clone()),
                    None,
                    Some(runtime.provider_id.clone()),
                    Some(runtime.model.clone()),
                    None,
                );
            }
            result.full_text
        }
        PiProcessOutcome::Aborted => {
            if let Some(guard) = trace_guard.as_mut() {
                guard.finalize_error("用户记忆提取被中断".to_string());
            }
            return Ok(0);
        }
    };

    let memories = parse_output(&text);
    if memories.is_empty() {
        return Ok(0);
    }

    let owner = UserMemoryOwner::agent(&request.agent_id);
    let mut inserted = 0usize;
    for memory in memories {
        if memory_exists(&memory, &recent_memories) {
            continue;
        }

        let candidate_text = format!("{}\n{}", memory.bucket, memory.text);
        let mut vector_dup = false;
        if let Some(registry) = get_embedding_registry() {
            let app_clone = app.clone();
            let namespaces = user_memory::vector_namespaces_for_agent(Some(&request.agent_id));
            let result = match tokio::runtime::Handle::try_current() {
                Ok(handle) => tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        let provider = {
                            let guard = registry.read().await;
                            guard.default_provider()
                        };
                        let Some(provider) = provider else {
                            return Ok::<bool, String>(false);
                        };
                        let embeddings = provider.embed(vec![candidate_text.clone()]).await?;
                        let Some(embedding) = embeddings.into_iter().next() else {
                            return Ok(false);
                        };
                        let conn = crate::storage_conn(&app_clone)?;
                        let hits = crate::memory_vector::search_vectors_across_workspaces(
                            &conn,
                            &namespaces,
                            &embedding,
                            1,
                            VECTOR_DEDUP_THRESHOLD,
                        )?;
                        Ok(!hits.is_empty())
                    })
                }),
                Err(_) => {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| format!("创建 Tokio runtime 失败: {e}"))?;
                    rt.block_on(async {
                        let provider = {
                            let guard = registry.read().await;
                            guard.default_provider()
                        };
                        let Some(provider) = provider else {
                            return Ok::<bool, String>(false);
                        };
                        let embeddings = provider.embed(vec![candidate_text.clone()]).await?;
                        let Some(embedding) = embeddings.into_iter().next() else {
                            return Ok(false);
                        };
                        let conn = crate::storage_conn(&app_clone)?;
                        let hits = crate::memory_vector::search_vectors_across_workspaces(
                            &conn,
                            &namespaces,
                            &embedding,
                            1,
                            VECTOR_DEDUP_THRESHOLD,
                        )?;
                        Ok(!hits.is_empty())
                    })
                }
            };
            if let Ok(true) = result {
                vector_dup = true;
            }
        }
        if vector_dup {
            continue;
        }

        let key = format!("nc_um.{}/auto_{}", memory.bucket, Uuid::new_v4().simple());
        let detail_json = serde_json::json!({
            "sessionId": request.session_id,
            "latestUserPrompt": request.latest_user_prompt,
            "latestAssistantReply": request.latest_assistant_reply,
            "bucket": memory.bucket,
        })
        .to_string();
        user_memory_service::upsert_entry(
            app,
            &owner,
            &key,
            &memory.text,
            &memory.tags,
            "auto",
            Some(&request.session_id),
            Some(&detail_json),
        )?;
        inserted += 1;
    }

    Ok(inserted)
}

pub(crate) fn spawn_user_memory_auto_extraction(
    app: &AppHandle,
    session_id: &str,
    latest_user_prompt: &str,
    latest_assistant_reply: &str,
    agent_id: &str,
    agent_name: &str,
) {
    if session_id.trim().is_empty()
        || latest_user_prompt.trim().is_empty()
        || latest_assistant_reply.trim().is_empty()
    {
        return;
    }
    let request = UserMemoryExtractionRequest {
        session_id: session_id.trim().to_string(),
        latest_user_prompt: latest_user_prompt.trim().to_string(),
        latest_assistant_reply: latest_assistant_reply.trim().to_string(),
        agent_id: agent_id.trim().to_string(),
        agent_name: agent_name.trim().to_string(),
    };
    let app = app.clone();
    std::mem::drop(tauri::async_runtime::spawn_blocking(move || {
        let _ = run_user_memory_auto_extraction(&app, request);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_output_deduplicates_and_normalizes_bucket() {
        let raw = r#"{"shouldWrite":true,"memories":[
            {"bucket":"style","text":"用户偏好直接给结论。","tags":["style"]},
            {"bucket":"writing","text":"用户偏好直接给结论。","tags":["writing"]}
        ]}"#;
        let parsed = parse_output(raw);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].bucket, "writing");
        assert_eq!(
            parsed[0].tags,
            vec!["writing".to_string(), "style".to_string()]
        );
    }

    #[test]
    fn parse_output_respects_should_write_false() {
        let raw = r#"{"shouldWrite":false,"memories":[{"bucket":"identity","text":"忽略我","tags":["identity"]}]}"#;
        assert!(parse_output(raw).is_empty());
    }

    #[test]
    fn build_conversation_turns_appends_latest() {
        let turns = build_conversation_turns(&[], "用户提问", "助手回答");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].turn_index, 1);
    }

    #[test]
    fn parse_output_accepts_workflow_voice_and_instruction_aliases() {
        let raw = r#"{"shouldWrite":true,"memories":[
            {"bucket":"workflow","text":"用户习惯先规划再执行。","tags":["workflow"]},
            {"bucket":"voice","text":"用户说话偏好短句直给。","tags":["style"]},
            {"bucket":"instruction","text":"用户要求以后叫他老公。","tags":["instruction"]}
        ]}"#;
        let parsed = parse_output(raw);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].bucket, "work");
        assert_eq!(
            parsed[0].tags,
            vec!["work".to_string(), "workflow".to_string()]
        );
        assert_eq!(parsed[1].bucket, "writing");
        assert_eq!(
            parsed[1].tags,
            vec!["writing".to_string(), "style".to_string()]
        );
        assert_eq!(parsed[2].bucket, "directive");
        assert_eq!(
            parsed[2].tags,
            vec!["directive".to_string(), "instruction".to_string()]
        );
    }
}
