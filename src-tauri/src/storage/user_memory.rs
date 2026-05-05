use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const OWNER_SCOPE_GLOBAL: &str = "global";
pub const OWNER_SCOPE_AGENT: &str = "agent";
pub const GLOBAL_OWNER_ID: &str = "__global__";

const USER_MEMORY_ID_PREFIX: &str = "user-memory::";
const USER_MEMORY_NAMESPACE_GLOBAL: &str = "user-memory::global";
const USER_MEMORY_NAMESPACE_AGENT_PREFIX: &str = "user-memory::agent::";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_memories (
            id TEXT PRIMARY KEY,
            owner_scope TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            memory_key TEXT NOT NULL,
            bucket TEXT NOT NULL,
            text TEXT NOT NULL,
            tags_json TEXT NOT NULL DEFAULT '[]',
            origin_kind TEXT NOT NULL,
            source_ref TEXT,
            detail_json TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            UNIQUE(owner_scope, owner_id, memory_key)
        );
        CREATE INDEX IF NOT EXISTS idx_user_memories_owner
            ON user_memories(owner_scope, owner_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_user_memories_origin
            ON user_memories(origin_kind, updated_at DESC);",
    )
    .map_err(|e| format!("初始化用户记忆表失败: {e}"))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryRecord {
    pub id: String,
    pub owner_scope: String,
    pub owner_id: String,
    pub memory_key: String,
    pub bucket: String,
    pub text: String,
    pub tags_json: String,
    pub origin_kind: String,
    pub source_ref: Option<String>,
    pub detail_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn row_to_user_memory(row: &rusqlite::Row) -> rusqlite::Result<UserMemoryRecord> {
    Ok(UserMemoryRecord {
        id: row.get("id")?,
        owner_scope: row.get("owner_scope")?,
        owner_id: row.get("owner_id")?,
        memory_key: row.get("memory_key")?,
        bucket: row.get("bucket")?,
        text: row.get("text")?,
        tags_json: row.get("tags_json")?,
        origin_kind: row.get("origin_kind")?,
        source_ref: row.get("source_ref")?,
        detail_json: row.get("detail_json")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn validate_owner_scope(owner_scope: &str) -> Result<(), String> {
    if matches!(owner_scope, OWNER_SCOPE_GLOBAL | OWNER_SCOPE_AGENT) {
        Ok(())
    } else {
        Err(format!("不支持的用户记忆 owner_scope: {owner_scope}"))
    }
}

pub fn vector_memory_id(id: &str) -> String {
    format!("{USER_MEMORY_ID_PREFIX}{id}")
}

pub fn extract_user_memory_id(memory_id: &str) -> Option<&str> {
    memory_id.strip_prefix(USER_MEMORY_ID_PREFIX)
}

pub fn global_vector_namespace() -> &'static str {
    USER_MEMORY_NAMESPACE_GLOBAL
}

pub fn agent_vector_namespace(agent_id: &str) -> String {
    format!("{USER_MEMORY_NAMESPACE_AGENT_PREFIX}{agent_id}")
}

pub fn vector_namespaces_for_agent(agent_id: Option<&str>) -> Vec<String> {
    let mut out = vec![global_vector_namespace().to_string()];
    if let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) {
        out.push(agent_vector_namespace(agent_id));
    }
    out
}

pub fn upsert_user_memory(
    conn: &Connection,
    owner_scope: &str,
    owner_id: &str,
    memory_key: &str,
    bucket: &str,
    text: &str,
    tags_json: &str,
    origin_kind: &str,
    source_ref: Option<&str>,
    detail_json: Option<&str>,
) -> Result<UserMemoryRecord, String> {
    validate_owner_scope(owner_scope)?;
    let now = now_ms();
    let existing = get_user_memory(conn, owner_scope, owner_id, memory_key)?;
    match existing {
        Some(existing) => {
            conn.execute(
                "UPDATE user_memories
                 SET bucket = ?1,
                     text = ?2,
                     tags_json = ?3,
                     origin_kind = ?4,
                     source_ref = ?5,
                     detail_json = ?6,
                     updated_at = ?7
                 WHERE id = ?8",
                params![
                    bucket,
                    text,
                    tags_json,
                    origin_kind,
                    source_ref,
                    detail_json,
                    now,
                    existing.id,
                ],
            )
            .map_err(|e| format!("更新用户记忆失败: {e}"))?;
        }
        None => {
            let id = format!("um_{}", uuid::Uuid::new_v4().simple());
            conn.execute(
                "INSERT INTO user_memories
                 (id, owner_scope, owner_id, memory_key, bucket, text, tags_json, origin_kind, source_ref, detail_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    id,
                    owner_scope,
                    owner_id,
                    memory_key,
                    bucket,
                    text,
                    tags_json,
                    origin_kind,
                    source_ref,
                    detail_json,
                    now,
                    now,
                ],
            )
            .map_err(|e| format!("插入用户记忆失败: {e}"))?;
        }
    }

    get_user_memory(conn, owner_scope, owner_id, memory_key)?
        .ok_or_else(|| "刚写入的用户记忆不存在".to_string())
}

pub fn get_user_memory(
    conn: &Connection,
    owner_scope: &str,
    owner_id: &str,
    memory_key: &str,
) -> Result<Option<UserMemoryRecord>, String> {
    validate_owner_scope(owner_scope)?;
    conn.query_row(
        "SELECT id, owner_scope, owner_id, memory_key, bucket, text, tags_json, origin_kind, source_ref, detail_json, created_at, updated_at
         FROM user_memories
         WHERE owner_scope = ?1 AND owner_id = ?2 AND memory_key = ?3",
        params![owner_scope, owner_id, memory_key],
        row_to_user_memory,
    )
    .optional()
    .map_err(|e| format!("查询用户记忆失败: {e}"))
}

pub fn get_user_memory_by_id(
    conn: &Connection,
    id: &str,
) -> Result<Option<UserMemoryRecord>, String> {
    conn.query_row(
        "SELECT id, owner_scope, owner_id, memory_key, bucket, text, tags_json, origin_kind, source_ref, detail_json, created_at, updated_at
         FROM user_memories
         WHERE id = ?1",
        params![id],
        row_to_user_memory,
    )
    .optional()
    .map_err(|e| format!("查询用户记忆失败: {e}"))
}

pub fn delete_user_memory(
    conn: &Connection,
    owner_scope: &str,
    owner_id: &str,
    memory_key: &str,
) -> Result<(), String> {
    validate_owner_scope(owner_scope)?;
    conn.execute(
        "DELETE FROM user_memories WHERE owner_scope = ?1 AND owner_id = ?2 AND memory_key = ?3",
        params![owner_scope, owner_id, memory_key],
    )
    .map_err(|e| format!("删除用户记忆失败: {e}"))?;
    Ok(())
}

pub fn list_user_memories(
    conn: &Connection,
    owner_scope: &str,
    owner_id: &str,
    limit: i64,
) -> Result<Vec<UserMemoryRecord>, String> {
    validate_owner_scope(owner_scope)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, owner_scope, owner_id, memory_key, bucket, text, tags_json, origin_kind, source_ref, detail_json, created_at, updated_at
             FROM user_memories
             WHERE owner_scope = ?1 AND owner_id = ?2
             ORDER BY updated_at DESC
             LIMIT ?3",
        )
        .map_err(|e| format!("准备查询用户记忆失败: {e}"))?;
    let rows = stmt
        .query_map(params![owner_scope, owner_id, limit], row_to_user_memory)
        .map_err(|e| format!("查询用户记忆失败: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("读取用户记忆失败: {e}"))?);
    }
    Ok(out)
}

pub fn list_user_memories_for_agent(
    conn: &Connection,
    agent_id: Option<&str>,
    limit_per_scope: i64,
) -> Result<Vec<UserMemoryRecord>, String> {
    let mut out = list_user_memories(conn, OWNER_SCOPE_GLOBAL, GLOBAL_OWNER_ID, limit_per_scope)?;
    if let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) {
        out.extend(list_user_memories(
            conn,
            OWNER_SCOPE_AGENT,
            agent_id,
            limit_per_scope,
        )?);
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

pub fn fetch_search_text(
    conn: &Connection,
    memory_id: &str,
) -> Result<Option<(String, String, String, String, Vec<String>)>, String> {
    let Some(user_memory_id) = extract_user_memory_id(memory_id) else {
        return Ok(None);
    };
    let Some(record) = get_user_memory_by_id(conn, user_memory_id)? else {
        return Ok(None);
    };
    let tags = serde_json::from_str::<Vec<String>>(&record.tags_json).unwrap_or_default();
    Ok(Some((
        "user_memory".to_string(),
        record.bucket,
        record.text,
        record.origin_kind,
        tags,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_db() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn user_memory_crud_roundtrip() {
        let conn = open_db();
        ensure_schema(&conn).unwrap();

        let created = upsert_user_memory(
            &conn,
            OWNER_SCOPE_AGENT,
            "agent-1",
            "nc_um.identity/test",
            "identity",
            "用户默认希望结论先行。",
            "[\"identity\"]",
            "manual",
            Some("settings"),
            Some("{\"text\":\"用户默认希望结论先行。\"}"),
        )
        .unwrap();
        assert_eq!(created.bucket, "identity");
        assert_eq!(created.origin_kind, "manual");

        let updated = upsert_user_memory(
            &conn,
            OWNER_SCOPE_AGENT,
            "agent-1",
            "nc_um.identity/test",
            "identity",
            "用户默认希望结论先行，少寒暄。",
            "[\"identity\",\"directive\"]",
            "auto",
            Some("session-1"),
            None,
        )
        .unwrap();
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.origin_kind, "auto");
        assert!(updated.updated_at >= created.updated_at);

        let listed = list_user_memories(&conn, OWNER_SCOPE_AGENT, "agent-1", 10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].text, "用户默认希望结论先行，少寒暄。");

        delete_user_memory(&conn, OWNER_SCOPE_AGENT, "agent-1", "nc_um.identity/test").unwrap();
        assert!(
            get_user_memory(&conn, OWNER_SCOPE_AGENT, "agent-1", "nc_um.identity/test")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn fetch_search_text_returns_origin_and_tags() {
        let conn = open_db();
        ensure_schema(&conn).unwrap();
        let stored = upsert_user_memory(
            &conn,
            OWNER_SCOPE_GLOBAL,
            GLOBAL_OWNER_ID,
            "nc_um.work/test",
            "work",
            "用户在代码评审里优先看风险。",
            "[\"work\",\"directive\"]",
            "migration",
            Some("history-migration"),
            None,
        )
        .unwrap();

        let fetched = fetch_search_text(&conn, &vector_memory_id(&stored.id))
            .unwrap()
            .unwrap();
        assert_eq!(fetched.0, "user_memory");
        assert_eq!(fetched.1, "work");
        assert_eq!(fetched.2, "用户在代码评审里优先看风险。");
        assert_eq!(fetched.3, "migration");
        assert_eq!(fetched.4, vec!["work".to_string(), "directive".to_string()]);
    }
}
