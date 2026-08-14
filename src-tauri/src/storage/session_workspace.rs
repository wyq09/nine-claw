//! Session workspace metadata stored on chat sessions.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWorkspaceDirs {
    pub session_id: String,
    pub topic_workspace_dir: Option<String>,
    pub current_workspace_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWorkspaceRecent {
    pub session_id: String,
    pub path: String,
    pub last_used_at: i64,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn
        .prepare(&pragma)
        .map_err(|e| format!("读取 {table} 表结构失败: {e}"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("解析 {table} 表结构失败: {e}"))?
        .filter_map(Result::ok)
        .any(|name| name == column);
    if exists {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )
    .map_err(|e| format!("补充 {table}.{column} 失败: {e}"))?;
    Ok(())
}

pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    add_column_if_missing(conn, "chat_sessions", "topic_workspace_dir", "TEXT")?;
    add_column_if_missing(conn, "chat_sessions", "current_workspace_dir", "TEXT")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS chat_session_workspace_recents (
            session_id TEXT NOT NULL,
            path TEXT NOT NULL,
            last_used_at INTEGER NOT NULL,
            PRIMARY KEY (session_id, path),
            FOREIGN KEY(session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_chat_session_workspace_recents_session
            ON chat_session_workspace_recents(session_id, last_used_at DESC);",
    )
    .map_err(|e| format!("初始化会话工作区表失败: {e}"))?;
    Ok(())
}

pub fn get_session_workspace_dirs(
    conn: &Connection,
    session_id: &str,
) -> Result<Option<SessionWorkspaceDirs>, String> {
    conn.query_row(
        "SELECT id, topic_workspace_dir, current_workspace_dir
         FROM chat_sessions WHERE id = ?1",
        params![session_id],
        |row| {
            Ok(SessionWorkspaceDirs {
                session_id: row.get("id")?,
                topic_workspace_dir: row.get("topic_workspace_dir")?,
                current_workspace_dir: row.get("current_workspace_dir")?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("查询会话工作区失败: {e}"))
}

pub fn set_session_workspace_dirs(
    conn: &Connection,
    session_id: &str,
    topic_workspace_dir: &str,
    current_workspace_dir: &str,
) -> Result<SessionWorkspaceDirs, String> {
    let changed = conn
        .execute(
            "UPDATE chat_sessions
             SET topic_workspace_dir = ?2,
                 current_workspace_dir = ?3
             WHERE id = ?1",
            params![session_id, topic_workspace_dir, current_workspace_dir],
        )
        .map_err(|e| format!("保存会话工作区失败: {e}"))?;
    if changed == 0 {
        return Err("会话不存在".to_string());
    }
    upsert_recent(conn, session_id, current_workspace_dir)?;
    get_session_workspace_dirs(conn, session_id)?
        .ok_or_else(|| "刚保存的会话工作区查询不到".to_string())
}

pub fn set_current_workspace_dir(
    conn: &Connection,
    session_id: &str,
    current_workspace_dir: &str,
) -> Result<SessionWorkspaceDirs, String> {
    let changed = conn
        .execute(
            "UPDATE chat_sessions
             SET current_workspace_dir = ?2
             WHERE id = ?1",
            params![session_id, current_workspace_dir],
        )
        .map_err(|e| format!("切换会话工作区失败: {e}"))?;
    if changed == 0 {
        return Err("会话不存在".to_string());
    }
    upsert_recent(conn, session_id, current_workspace_dir)?;
    get_session_workspace_dirs(conn, session_id)?
        .ok_or_else(|| "刚切换的会话工作区查询不到".to_string())
}

pub fn upsert_recent(conn: &Connection, session_id: &str, path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO chat_session_workspace_recents (session_id, path, last_used_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(session_id, path)
         DO UPDATE SET last_used_at = excluded.last_used_at",
        params![session_id, trimmed, now_ms()],
    )
    .map_err(|e| format!("记录最近工作区失败: {e}"))?;
    Ok(())
}

pub fn list_recents(
    conn: &Connection,
    session_id: &str,
    limit: usize,
) -> Result<Vec<SessionWorkspaceRecent>, String> {
    let limit = limit.clamp(1, 50) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT session_id, path, last_used_at
             FROM chat_session_workspace_recents
             WHERE session_id = ?1
             ORDER BY last_used_at DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("准备最近工作区查询失败: {e}"))?;
    let rows = stmt
        .query_map(params![session_id, limit], |row| {
            Ok(SessionWorkspaceRecent {
                session_id: row.get("session_id")?,
                path: row.get("path")?,
                last_used_at: row.get("last_used_at")?,
            })
        })
        .map_err(|e| format!("查询最近工作区失败: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("读取最近工作区失败: {e}"))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::chat_history::{create_chat_session, CreateChatSessionInput};
    use crate::storage::db::open_in_memory;

    fn create_session(conn: &Connection, id: &str) {
        create_chat_session(
            conn,
            &CreateChatSessionInput {
                id: id.to_string(),
                title: id.to_string(),
                status: "running".to_string(),
                agent_id: None,
                agent_snapshot_json: None,
                bot_target_json: None,
                session_llm_provider_id: None,
                session_llm_model: None,
                workspace_id: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn stores_workspace_dirs_and_recent_paths() {
        let conn = open_in_memory().unwrap();
        create_session(&conn, "s1");

        let dirs = set_session_workspace_dirs(&conn, "s1", "/tmp/topic", "/tmp/topic").unwrap();
        assert_eq!(dirs.topic_workspace_dir.as_deref(), Some("/tmp/topic"));
        assert_eq!(dirs.current_workspace_dir.as_deref(), Some("/tmp/topic"));

        set_current_workspace_dir(&conn, "s1", "/tmp/other").unwrap();
        let dirs = get_session_workspace_dirs(&conn, "s1").unwrap().unwrap();
        assert_eq!(dirs.current_workspace_dir.as_deref(), Some("/tmp/other"));

        let recents = list_recents(&conn, "s1", 10).unwrap();
        assert!(recents.iter().any(|item| item.path == "/tmp/topic"));
        assert!(recents.iter().any(|item| item.path == "/tmp/other"));
    }
}
