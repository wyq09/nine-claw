use super::{
    contains_memory_recall_signal, normalize_memory_match_text, trim_to_char_limit,
    MemoryCategoryDefinition,
};
use crate::storage::core_memory;
use md5::{Digest, Md5};
use serde_json::json;

fn content_hash(value: &str) -> String {
    format!("{:x}", Md5::digest(value.as_bytes()))
}

fn tags_json(categories: &[MemoryCategoryDefinition]) -> String {
    serde_json::to_string(
        &categories
            .iter()
            .map(|category| category.key.to_string())
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".to_string())
}

fn doc_meta_for_relative_path(relative_path: &str) -> Option<(&'static str, &'static str)> {
    match relative_path {
        p if p.ends_with("/MEMORY.md") => Some(("memory", "MEMORY")),
        p if p.ends_with("/USER_MODEL.md") => Some(("user_model", "USER_MODEL")),
        p if p.ends_with("/RELATIONSHIP_MAP.md") => Some(("relationship_map", "RELATIONSHIP_MAP")),
        p if p.ends_with("/PITFALLS.md") => Some(("pitfalls", "PITFALLS")),
        p if p.ends_with("/DECISIONS.md") => Some(("decisions", "DECISIONS")),
        p if p.ends_with("/PUBLIC_CONTEXT.md") => Some(("public_context", "PUBLIC_CONTEXT")),
        p if p.ends_with("/WORKING.md") => Some(("working", "WORKING")),
        _ => None,
    }
}

fn index_text_async(agent_id: &str, memory_id: String, text: String) {
    let Some(registry) = crate::managed_runtime::get_embedding_registry() else {
        return;
    };
    let Some(app) = crate::managed_runtime::injected_app_handle() else {
        return;
    };
    let namespace = core_memory::agent_vector_namespace(agent_id);
    tauri::async_runtime::spawn(async move {
        let provider = {
            let guard = registry.read().await;
            guard.default_provider()
        };
        let Some(provider) = provider else {
            return;
        };
        match provider.embed(vec![text]).await {
            Ok(embeddings) => {
                if let Some(embedding) = embeddings.into_iter().next() {
                    if let Ok(conn) = crate::storage_conn(&app) {
                        let vector_id = format!("vec_{}", uuid::Uuid::new_v4().simple());
                        let _ = crate::memory_vector::upsert_vector(
                            &conn,
                            &vector_id,
                            &memory_id,
                            &namespace,
                            &embedding,
                            provider.id(),
                        );
                    }
                }
            }
            Err(error) => log::warn!("核心记忆向量写入失败: {error}"),
        }
    });
}

pub(super) fn sync_core_file_to_db(
    agent_id: &str,
    relative_path: &str,
    content: &str,
) -> Result<(), String> {
    let Some((doc_type, title)) = doc_meta_for_relative_path(relative_path) else {
        return Ok(());
    };
    let Some(app) = crate::managed_runtime::injected_app_handle() else {
        return Ok(());
    };
    let conn = crate::storage_conn(&app)?;
    let document = core_memory::upsert_document(
        &conn,
        agent_id,
        doc_type,
        title,
        Some(relative_path),
        content,
        "[]",
        &content_hash(content),
    )?;
    index_text_async(
        agent_id,
        core_memory::document_vector_memory_id(&document.id),
        format!("{title}\n{}", document.content_md),
    );
    Ok(())
}

pub(super) fn record_memory_ingest_to_db(
    agent_id: &str,
    summary: &str,
    source_ref: &str,
    categories: &[MemoryCategoryDefinition],
    user_message: &str,
    assistant_message: &str,
) -> Result<(), String> {
    let Some(app) = crate::managed_runtime::injected_app_handle() else {
        return Ok(());
    };
    let conn = crate::storage_conn(&app)?;
    let tags_json = tags_json(categories);
    let detail_json = json!({
        "sourceRef": source_ref,
        "userMessage": user_message,
        "assistantMessage": assistant_message,
    })
    .to_string();

    for event_type in ["conversation_ingest", "daily_digest_entry"] {
        let event = core_memory::insert_event(
            &conn,
            &format!("cme_{}", uuid::Uuid::new_v4().simple()),
            agent_id,
            event_type,
            summary,
            summary,
            &tags_json,
            Some(&detail_json),
            Some(source_ref),
        )?;
        index_text_async(
            agent_id,
            core_memory::event_vector_memory_id(&event.id),
            format!("{}\n{}", event.title, event.summary),
        );
    }
    Ok(())
}

pub(super) fn build_db_memory_snapshot(
    agent_id: &str,
    current_prompt: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(app) = crate::managed_runtime::injected_app_handle() else {
        return Ok(None);
    };
    let conn = crate::storage_conn(&app)?;
    let prompt_norm = normalize_memory_match_text(current_prompt.unwrap_or_default());
    let mut sections = Vec::new();

    for doc_type in [
        "working",
        "decisions",
        "pitfalls",
        "user_model",
        "relationship_map",
    ] {
        let Some(doc) = core_memory::get_document_by_type(&conn, agent_id, doc_type)? else {
            continue;
        };
        if prompt_norm.is_empty() && doc_type != "working" {
            continue;
        }
        sections.push(format!(
            "{}:\n{}",
            doc.title,
            trim_to_char_limit(&doc.content_md, 260)
        ));
        if sections.len() >= 3 {
            break;
        }
    }

    let events = core_memory::list_recent_events_for_agent(&conn, agent_id, 10)?;
    let mut event_lines = Vec::new();
    for event in events {
        if !prompt_norm.is_empty()
            && !contains_memory_recall_signal(&prompt_norm)
            && !event.summary.to_lowercase().contains(&prompt_norm)
        {
            continue;
        }
        event_lines.push(format!("- {}", trim_to_char_limit(&event.summary, 120)));
        if event_lines.len() >= 6 {
            break;
        }
    }
    if !event_lines.is_empty() {
        sections.push(format!("记忆事件摘录：\n{}", event_lines.join("\n")));
    }

    let user_memories =
        crate::storage::user_memory::list_user_memories_for_agent(&conn, Some(agent_id), 8)?;
    let mut user_memory_lines = Vec::new();
    for memory in user_memories {
        if !prompt_norm.is_empty()
            && !contains_memory_recall_signal(&prompt_norm)
            && !memory.text.to_lowercase().contains(&prompt_norm)
        {
            continue;
        }
        user_memory_lines.push(format!(
            "- [{} / {}] {}",
            memory.origin_kind,
            memory.bucket,
            trim_to_char_limit(&memory.text, 120)
        ));
        if user_memory_lines.len() >= 6 {
            break;
        }
    }
    if !user_memory_lines.is_empty() {
        sections.push(format!("用户记忆摘录：\n{}", user_memory_lines.join("\n")));
    }

    if let Some(vector_block) = build_vector_hint_block(&conn, agent_id, &prompt_norm) {
        sections.push(vector_block);
    }

    if sections.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trim_to_char_limit(
            &format!("SQLite 核心记忆：\n{}", sections.join("\n\n")),
            1200,
        )))
    }
}

fn build_vector_hint_block(
    conn: &rusqlite::Connection,
    agent_id: &str,
    prompt_norm: &str,
) -> Option<String> {
    if prompt_norm.is_empty() {
        return None;
    }
    let registry = crate::managed_runtime::get_embedding_registry()?;
    let embeddings = match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| {
            handle.block_on(async {
                let guard = registry.read().await;
                let provider = guard
                    .default_provider()
                    .ok_or_else(|| "no provider".to_string())?;
                provider.embed(vec![prompt_norm.to_string()]).await
            })
        }),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()?;
            rt.block_on(async {
                let guard = registry.read().await;
                let provider = guard
                    .default_provider()
                    .ok_or_else(|| "no provider".to_string())?;
                provider.embed(vec![prompt_norm.to_string()]).await
            })
        }
    }
    .ok()?;
    let query = embeddings.first()?;
    let namespaces = {
        let mut namespaces = vec![core_memory::agent_vector_namespace(agent_id)];
        namespaces.extend(crate::storage::user_memory::vector_namespaces_for_agent(
            Some(agent_id),
        ));
        namespaces
    };
    let hits =
        crate::memory_vector::search_vectors_across_workspaces(conn, &namespaces, query, 3, 0.55)
            .ok()?;
    if hits.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    for hit in hits {
        if let Ok(Some((source_kind, title, content))) =
            crate::storage::core_memory::fetch_search_text(conn, &hit.memory_id)
        {
            lines.push(format!(
                "- {} / {}: {}",
                source_kind,
                title,
                trim_to_char_limit(&content, 80)
            ));
            continue;
        }
        if let Ok(Some((source_kind, bucket, text, origin_kind, _tags))) =
            crate::storage::user_memory::fetch_search_text(conn, &hit.memory_id)
        {
            lines.push(format!(
                "- {} / {} / {}: {}",
                source_kind,
                origin_kind,
                bucket,
                trim_to_char_limit(&text, 80)
            ));
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(format!("语义检索命中：\n{}", lines.join("\n")))
    }
}
