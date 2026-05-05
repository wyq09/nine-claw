use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const DOC_NAMESPACE_PREFIX: &str = "agent-doc::";
const EVENT_NAMESPACE_PREFIX: &str = "agent-event::";
const CHAT_TURN_NAMESPACE_PREFIX: &str = "chat-turn::";
const AGENT_VECTOR_NAMESPACE_PREFIX: &str = "agent::";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS core_memory_documents (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            doc_type TEXT NOT NULL,
            title TEXT NOT NULL,
            relative_path TEXT,
            content_md TEXT NOT NULL,
            tags_json TEXT NOT NULL DEFAULT '[]',
            content_hash TEXT NOT NULL,
            version INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            UNIQUE(agent_id, doc_type)
        );
        CREATE INDEX IF NOT EXISTS idx_core_memory_documents_agent
            ON core_memory_documents(agent_id, updated_at DESC);

        CREATE TABLE IF NOT EXISTS core_memory_events (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            title TEXT NOT NULL,
            summary TEXT NOT NULL,
            tags_json TEXT NOT NULL DEFAULT '[]',
            detail_json TEXT,
            source_ref TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_core_memory_events_agent
            ON core_memory_events(agent_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_core_memory_events_type
            ON core_memory_events(event_type, created_at DESC);

        CREATE TABLE IF NOT EXISTS runtime_session_events (
            id TEXT PRIMARY KEY,
            agent_id TEXT,
            session_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            summary TEXT NOT NULL,
            detail_json TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_runtime_session_events_session
            ON runtime_session_events(session_id, created_at DESC);

        CREATE TABLE IF NOT EXISTS runtime_multimodal_summaries (
            id TEXT PRIMARY KEY,
            summary_key TEXT NOT NULL,
            user_prompt TEXT NOT NULL,
            assistant_response TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_runtime_multimodal_summaries_key
            ON runtime_multimodal_summaries(summary_key, created_at DESC);",
    )
    .map_err(|e| format!("初始化核心记忆表失败: {e}"))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreMemoryDocument {
    pub id: String,
    pub agent_id: String,
    pub doc_type: String,
    pub title: String,
    pub relative_path: Option<String>,
    pub content_md: String,
    pub tags_json: String,
    pub content_hash: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreMemoryEvent {
    pub id: String,
    pub agent_id: String,
    pub event_type: String,
    pub title: String,
    pub summary: String,
    pub tags_json: String,
    pub detail_json: Option<String>,
    pub source_ref: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSessionEvent {
    pub id: String,
    pub agent_id: Option<String>,
    pub session_id: String,
    pub kind: String,
    pub summary: String,
    pub detail_json: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMultimodalSummary {
    pub id: String,
    pub summary_key: String,
    pub user_prompt: String,
    pub assistant_response: String,
    pub created_at: i64,
}

fn row_to_document(row: &rusqlite::Row) -> rusqlite::Result<CoreMemoryDocument> {
    Ok(CoreMemoryDocument {
        id: row.get("id")?,
        agent_id: row.get("agent_id")?,
        doc_type: row.get("doc_type")?,
        title: row.get("title")?,
        relative_path: row.get("relative_path")?,
        content_md: row.get("content_md")?,
        tags_json: row.get("tags_json")?,
        content_hash: row.get("content_hash")?,
        version: row.get("version")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<CoreMemoryEvent> {
    Ok(CoreMemoryEvent {
        id: row.get("id")?,
        agent_id: row.get("agent_id")?,
        event_type: row.get("event_type")?,
        title: row.get("title")?,
        summary: row.get("summary")?,
        tags_json: row.get("tags_json")?,
        detail_json: row.get("detail_json")?,
        source_ref: row.get("source_ref")?,
        created_at: row.get("created_at")?,
    })
}

fn row_to_runtime_event(row: &rusqlite::Row) -> rusqlite::Result<RuntimeSessionEvent> {
    Ok(RuntimeSessionEvent {
        id: row.get("id")?,
        agent_id: row.get("agent_id")?,
        session_id: row.get("session_id")?,
        kind: row.get("kind")?,
        summary: row.get("summary")?,
        detail_json: row.get("detail_json")?,
        created_at: row.get("created_at")?,
    })
}

fn row_to_multimodal_summary(row: &rusqlite::Row) -> rusqlite::Result<RuntimeMultimodalSummary> {
    Ok(RuntimeMultimodalSummary {
        id: row.get("id")?,
        summary_key: row.get("summary_key")?,
        user_prompt: row.get("user_prompt")?,
        assistant_response: row.get("assistant_response")?,
        created_at: row.get("created_at")?,
    })
}

pub fn agent_vector_namespace(agent_id: &str) -> String {
    format!("{AGENT_VECTOR_NAMESPACE_PREFIX}{agent_id}")
}

pub fn document_vector_memory_id(document_id: &str) -> String {
    format!("{DOC_NAMESPACE_PREFIX}{document_id}")
}

pub fn event_vector_memory_id(event_id: &str) -> String {
    format!("{EVENT_NAMESPACE_PREFIX}{event_id}")
}

pub fn chat_turn_vector_memory_id(turn_id: &str) -> String {
    format!("{CHAT_TURN_NAMESPACE_PREFIX}{turn_id}")
}

pub fn extract_document_id(memory_id: &str) -> Option<&str> {
    memory_id.strip_prefix(DOC_NAMESPACE_PREFIX)
}

pub fn extract_event_id(memory_id: &str) -> Option<&str> {
    memory_id.strip_prefix(EVENT_NAMESPACE_PREFIX)
}

pub fn extract_chat_turn_id(memory_id: &str) -> Option<&str> {
    memory_id.strip_prefix(CHAT_TURN_NAMESPACE_PREFIX)
}

pub fn upsert_document(
    conn: &Connection,
    agent_id: &str,
    doc_type: &str,
    title: &str,
    relative_path: Option<&str>,
    content_md: &str,
    tags_json: &str,
    content_hash: &str,
) -> Result<CoreMemoryDocument, String> {
    let existing = conn
        .query_row(
            "SELECT id, agent_id, doc_type, title, relative_path, content_md, tags_json, content_hash, version, created_at, updated_at
             FROM core_memory_documents
             WHERE agent_id = ?1 AND doc_type = ?2",
            params![agent_id, doc_type],
            row_to_document,
        )
        .optional()
        .map_err(|e| format!("查询核心记忆文档失败: {e}"))?;

    let now = now_ms();
    match existing {
        Some(existing) => {
            let next_version = if existing.content_hash == content_hash {
                existing.version
            } else {
                existing.version + 1
            };
            conn.execute(
                "UPDATE core_memory_documents
                 SET title = ?1,
                     relative_path = ?2,
                     content_md = ?3,
                     tags_json = ?4,
                     content_hash = ?5,
                     version = ?6,
                     updated_at = ?7
                 WHERE id = ?8",
                params![
                    title,
                    relative_path,
                    content_md,
                    tags_json,
                    content_hash,
                    next_version,
                    now,
                    existing.id,
                ],
            )
            .map_err(|e| format!("更新核心记忆文档失败: {e}"))?;
        }
        None => {
            let id = format!("cmd_{}", uuid::Uuid::new_v4().simple());
            conn.execute(
                "INSERT INTO core_memory_documents
                 (id, agent_id, doc_type, title, relative_path, content_md, tags_json, content_hash, version, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?10)",
                params![id, agent_id, doc_type, title, relative_path, content_md, tags_json, content_hash, now, now],
            )
            .map_err(|e| format!("插入核心记忆文档失败: {e}"))?;
        }
    }

    conn.query_row(
        "SELECT id, agent_id, doc_type, title, relative_path, content_md, tags_json, content_hash, version, created_at, updated_at
         FROM core_memory_documents
         WHERE agent_id = ?1 AND doc_type = ?2",
        params![agent_id, doc_type],
        row_to_document,
    )
    .map_err(|e| format!("回读核心记忆文档失败: {e}"))
}

pub fn get_document_by_type(
    conn: &Connection,
    agent_id: &str,
    doc_type: &str,
) -> Result<Option<CoreMemoryDocument>, String> {
    conn.query_row(
        "SELECT id, agent_id, doc_type, title, relative_path, content_md, tags_json, content_hash, version, created_at, updated_at
         FROM core_memory_documents
         WHERE agent_id = ?1 AND doc_type = ?2",
        params![agent_id, doc_type],
        row_to_document,
    )
    .optional()
    .map_err(|e| format!("查询核心记忆文档失败: {e}"))
}

pub fn get_document(conn: &Connection, id: &str) -> Result<Option<CoreMemoryDocument>, String> {
    conn.query_row(
        "SELECT id, agent_id, doc_type, title, relative_path, content_md, tags_json, content_hash, version, created_at, updated_at
         FROM core_memory_documents
         WHERE id = ?1",
        params![id],
        row_to_document,
    )
    .optional()
    .map_err(|e| format!("查询核心记忆文档失败: {e}"))
}

pub fn list_documents_for_agent(
    conn: &Connection,
    agent_id: &str,
) -> Result<Vec<CoreMemoryDocument>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, doc_type, title, relative_path, content_md, tags_json, content_hash, version, created_at, updated_at
             FROM core_memory_documents
             WHERE agent_id = ?1
             ORDER BY updated_at DESC",
        )
        .map_err(|e| format!("准备查询核心记忆文档失败: {e}"))?;
    let rows = stmt
        .query_map(params![agent_id], row_to_document)
        .map_err(|e| format!("查询核心记忆文档失败: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("读取核心记忆文档失败: {e}"))?);
    }
    Ok(out)
}

pub fn insert_event(
    conn: &Connection,
    id: &str,
    agent_id: &str,
    event_type: &str,
    title: &str,
    summary: &str,
    tags_json: &str,
    detail_json: Option<&str>,
    source_ref: Option<&str>,
) -> Result<CoreMemoryEvent, String> {
    let created_at = now_ms();
    conn.execute(
        "INSERT INTO core_memory_events
         (id, agent_id, event_type, title, summary, tags_json, detail_json, source_ref, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            agent_id,
            event_type,
            title,
            summary,
            tags_json,
            detail_json,
            source_ref,
            created_at
        ],
    )
    .map_err(|e| format!("插入核心记忆事件失败: {e}"))?;
    get_event(conn, id)?.ok_or_else(|| "刚创建的核心记忆事件不存在".to_string())
}

pub fn get_event(conn: &Connection, id: &str) -> Result<Option<CoreMemoryEvent>, String> {
    conn.query_row(
        "SELECT id, agent_id, event_type, title, summary, tags_json, detail_json, source_ref, created_at
         FROM core_memory_events
         WHERE id = ?1",
        params![id],
        row_to_event,
    )
    .optional()
    .map_err(|e| format!("查询核心记忆事件失败: {e}"))
}

pub fn list_recent_events_for_agent(
    conn: &Connection,
    agent_id: &str,
    limit: usize,
) -> Result<Vec<CoreMemoryEvent>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, event_type, title, summary, tags_json, detail_json, source_ref, created_at
             FROM core_memory_events
             WHERE agent_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("准备查询核心记忆事件失败: {e}"))?;
    let rows = stmt
        .query_map(params![agent_id, limit as i64], row_to_event)
        .map_err(|e| format!("查询核心记忆事件失败: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("读取核心记忆事件失败: {e}"))?);
    }
    Ok(out)
}

pub fn insert_runtime_session_event(
    conn: &Connection,
    id: &str,
    agent_id: Option<&str>,
    session_id: &str,
    kind: &str,
    summary: &str,
    detail_json: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO runtime_session_events
         (id, agent_id, session_id, kind, summary, detail_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            agent_id,
            session_id,
            kind,
            summary,
            detail_json,
            now_ms()
        ],
    )
    .map_err(|e| format!("写入 runtime session 事件失败: {e}"))?;
    Ok(())
}

pub fn list_recent_runtime_session_events(
    conn: &Connection,
    agent_id: &str,
    limit: usize,
) -> Result<Vec<RuntimeSessionEvent>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, kind, summary, detail_json, created_at
             FROM runtime_session_events
             WHERE agent_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("准备查询 runtime session 事件失败: {e}"))?;
    let rows = stmt
        .query_map(params![agent_id, limit as i64], row_to_runtime_event)
        .map_err(|e| format!("查询 runtime session 事件失败: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("读取 runtime session 事件失败: {e}"))?);
    }
    Ok(out)
}

pub fn clear_runtime_multimodal_summaries(
    conn: &Connection,
    summary_key: &str,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM runtime_multimodal_summaries WHERE summary_key = ?1",
        params![summary_key],
    )
    .map_err(|e| format!("删除多模态摘要失败: {e}"))?;
    Ok(())
}

pub fn insert_runtime_multimodal_summary(
    conn: &Connection,
    summary_key: &str,
    user_prompt: &str,
    assistant_response: &str,
) -> Result<(), String> {
    let id = format!("rms_{}", uuid::Uuid::new_v4().simple());
    conn.execute(
        "INSERT INTO runtime_multimodal_summaries
         (id, summary_key, user_prompt, assistant_response, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, summary_key, user_prompt, assistant_response, now_ms()],
    )
    .map_err(|e| format!("插入多模态摘要失败: {e}"))?;
    Ok(())
}

pub fn list_runtime_multimodal_summaries(
    conn: &Connection,
    summary_key: &str,
    limit: usize,
) -> Result<Vec<RuntimeMultimodalSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, summary_key, user_prompt, assistant_response, created_at
             FROM runtime_multimodal_summaries
             WHERE summary_key = ?1
             ORDER BY created_at ASC, rowid ASC
             LIMIT ?2",
        )
        .map_err(|e| format!("准备查询多模态摘要失败: {e}"))?;
    let rows = stmt
        .query_map(
            params![summary_key, limit as i64],
            row_to_multimodal_summary,
        )
        .map_err(|e| format!("查询多模态摘要失败: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("读取多模态摘要失败: {e}"))?);
    }
    Ok(out)
}

pub fn fetch_search_text(
    conn: &Connection,
    memory_id: &str,
) -> Result<Option<(String, String, String)>, String> {
    if let Some(document_id) = extract_document_id(memory_id) {
        let Some(doc) = get_document(conn, document_id)? else {
            return Ok(None);
        };
        return Ok(Some((
            "core_document".to_string(),
            doc.title,
            doc.content_md,
        )));
    }
    if let Some(event_id) = extract_event_id(memory_id) {
        let Some(event) = get_event(conn, event_id)? else {
            return Ok(None);
        };
        return Ok(Some((
            "core_event".to_string(),
            event.event_type,
            event.summary,
        )));
    }
    if let Some(turn_id) = extract_chat_turn_id(memory_id) {
        let Some(turn) = crate::storage::chat_history::get_chat_turn(conn, turn_id)? else {
            return Ok(None);
        };
        return Ok(Some((
            "chat_turn".to_string(),
            format!("Turn {}", turn.turn_index),
            format!("{}\n{}", turn.prompt, turn.answer),
        )));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_in_memory;

    #[test]
    fn core_memory_document_upsert_updates_version() {
        let conn = open_in_memory().unwrap();
        let doc1 = upsert_document(
            &conn,
            "agent-a",
            "working",
            "WORKING",
            Some("agents/agent-a/WORKING.md"),
            "v1",
            "[\"projects\"]",
            "hash-v1",
        )
        .unwrap();
        assert_eq!(doc1.version, 1);

        let doc2 = upsert_document(
            &conn,
            "agent-a",
            "working",
            "WORKING",
            Some("agents/agent-a/WORKING.md"),
            "v2",
            "[\"projects\"]",
            "hash-v2",
        )
        .unwrap();
        assert_eq!(doc2.version, 2);
        assert_eq!(doc2.content_md, "v2");
    }

    #[test]
    fn runtime_multimodal_summary_roundtrip() {
        let conn = open_in_memory().unwrap();
        insert_runtime_multimodal_summary(&conn, "key-1", "图里是什么", "是首页").unwrap();
        insert_runtime_multimodal_summary(&conn, "key-1", "继续", "是目录").unwrap();
        let rows = list_runtime_multimodal_summaries(&conn, "key-1", 8).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].user_prompt, "图里是什么");
        assert_eq!(rows[1].assistant_response, "是目录");
        clear_runtime_multimodal_summaries(&conn, "key-1").unwrap();
        assert!(list_runtime_multimodal_summaries(&conn, "key-1", 8)
            .unwrap()
            .is_empty());
    }
}
