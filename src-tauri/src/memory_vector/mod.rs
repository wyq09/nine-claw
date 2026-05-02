//! memory_vectors 表 CRUD 操作。

pub mod migration;
pub mod vector_search;

use rusqlite::{params, Connection, OptionalExtension};
use std::time::{SystemTime, UNIX_EPOCH};

use vector_search::embedding_to_blob;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// memory_vectors 表完整记录。
#[derive(Debug, Clone)]
pub struct MemoryVectorRecord {
    pub id: String,
    pub memory_id: String,
    pub workspace_id: String,
    pub embedding: Vec<f32>,
    pub embedding_model: String,
    pub dimension: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 插入或更新向量。若 memory_id 已有向量则更新，否则插入。
pub fn upsert_vector(
    conn: &Connection,
    id: &str,
    memory_id: &str,
    workspace_id: &str,
    embedding: &[f32],
    model_id: &str,
) -> Result<(), String> {
    let dim = embedding.len() as i32;
    let blob = embedding_to_blob(embedding);
    let now = now_ms();

    // 检查 memory_id 是否已有向量
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM memory_vectors WHERE memory_id = ?1",
            params![memory_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("查询已有向量失败: {e}"))?;

    if let Some(existing_id) = existing {
        // 更新
        conn.execute(
            "UPDATE memory_vectors
             SET embedding = ?1, embedding_model = ?2, dimension = ?3, updated_at = ?4
             WHERE id = ?5",
            params![blob, model_id, dim, now, existing_id],
        )
        .map_err(|e| format!("更新向量失败: {e}"))?;
    } else {
        // 插入
        conn.execute(
            "INSERT INTO memory_vectors (id, memory_id, workspace_id, embedding, embedding_model, dimension, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, memory_id, workspace_id, blob, model_id, dim, now, now],
        )
        .map_err(|e| format!("插入向量失败: {e}"))?;
    }

    Ok(())
}

/// 按 memory_id 删除向量，返回是否实际删除了记录。
pub fn delete_vector_by_memory_id(conn: &Connection, memory_id: &str) -> Result<bool, String> {
    let n = conn
        .execute(
            "DELETE FROM memory_vectors WHERE memory_id = ?1",
            params![memory_id],
        )
        .map_err(|e| format!("删除向量失败: {e}"))?;
    Ok(n > 0)
}

/// 按 memory_id 获取向量记录。
pub fn get_vector_by_memory_id(
    conn: &Connection,
    memory_id: &str,
) -> Result<Option<MemoryVectorRecord>, String> {
    let result = conn
        .query_row(
            "SELECT id, memory_id, workspace_id, embedding, embedding_model, dimension, created_at, updated_at
             FROM memory_vectors WHERE memory_id = ?1",
            params![memory_id],
            |row| {
                let id: String = row.get(0)?;
                let memory_id: String = row.get(1)?;
                let workspace_id: String = row.get(2)?;
                let blob: Vec<u8> = row.get(3)?;
                let embedding_model: String = row.get(4)?;
                let dim: i32 = row.get(5)?;
                let created_at: i64 = row.get(6)?;
                let updated_at: i64 = row.get(7)?;
                Ok((id, memory_id, workspace_id, blob, embedding_model, dim, created_at, updated_at))
            },
        )
        .optional()
        .map_err(|e| format!("查询向量失败: {e}"))?;

    match result {
        Some((id, mid, ws, blob, model, dim, cat, uat)) => {
            let embedding = vector_search::blob_to_embedding(&blob)?;
            Ok(Some(MemoryVectorRecord {
                id,
                memory_id: mid,
                workspace_id: ws,
                embedding,
                embedding_model: model,
                dimension: dim as usize,
                created_at: cat,
                updated_at: uat,
            }))
        }
        None => Ok(None),
    }
}

/// 在工作空间内执行余弦搜索，委托给 vector_search::cosine_search。
pub fn search_vectors(
    conn: &Connection,
    workspace_id: &str,
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
    tag_filter: Option<&[String]>,
) -> Result<Vec<vector_search::SearchHit>, String> {
    vector_search::cosine_search(conn, workspace_id, query_embedding, limit, threshold, tag_filter)
}

/// 查找还没有向量索引的记忆，返回 (id, title, content) 元组。
pub fn find_memories_without_vectors(
    conn: &Connection,
    workspace_id: &str,
    limit: usize,
) -> Result<Vec<(String, String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT wm.id, wm.title, wm.content
             FROM workspace_memories wm
             LEFT JOIN memory_vectors mv ON mv.memory_id = wm.id
             WHERE wm.workspace_id = ?1 AND mv.id IS NULL
             ORDER BY wm.updated_at DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("准备查询未索引记忆失败: {e}"))?;

    let rows = stmt
        .query_map(params![workspace_id, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("查询未索引记忆失败: {e}"))?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("读取未索引记忆行失败: {e}"))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_in_memory;

    /// 创建一个测试用工作空间 + 记忆，返回 (workspace_id, memory_id)。
    fn seed_workspace_and_memory(conn: &Connection) -> (String, String) {
        let ws_id = "ws-test-1";
        let mem_id = "mem-test-1";
        let now = now_ms();
        conn.execute(
            "INSERT OR IGNORE INTO workspaces (id, name, description, supervisor_agent_id, created_at, updated_at, archived)
             VALUES (?1, 'test', '', 'agent-1', ?2, ?2, 0)",
            params![ws_id, now],
        )
        .unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO workspace_memories (id, workspace_id, title, content, author_agent_id, tags_json, created_at, updated_at)
             VALUES (?1, ?2, '测试记忆', '内容', NULL, '[]', ?3, ?3)",
            params![mem_id, ws_id, now],
        )
        .unwrap();
        (ws_id.to_string(), mem_id.to_string())
    }

    #[test]
    fn test_upsert_inserts_new() {
        let conn = open_in_memory().unwrap();
        let (ws_id, mem_id) = seed_workspace_and_memory(&conn);

        let embedding = vec![0.1, 0.2, 0.3];
        upsert_vector(&conn, "vec-1", &mem_id, &ws_id, &embedding, "model-a").unwrap();

        let rec = get_vector_by_memory_id(&conn, &mem_id).unwrap().unwrap();
        assert_eq!(rec.memory_id, mem_id);
        assert_eq!(rec.workspace_id, ws_id);
        assert_eq!(rec.dimension, 3);
        assert_eq!(rec.embedding_model, "model-a");
        assert_eq!(rec.embedding.len(), 3);
        assert!((rec.embedding[0] - 0.1).abs() < 1e-5);
    }

    #[test]
    fn test_upsert_updates_existing() {
        let conn = open_in_memory().unwrap();
        let (ws_id, mem_id) = seed_workspace_and_memory(&conn);

        let emb1 = vec![0.1, 0.2];
        upsert_vector(&conn, "vec-1", &mem_id, &ws_id, &emb1, "model-a").unwrap();

        // upsert with same memory_id should update, not fail
        let emb2 = vec![0.9, 0.8];
        upsert_vector(&conn, "vec-2", &mem_id, &ws_id, &emb2, "model-b").unwrap();

        let rec = get_vector_by_memory_id(&conn, &mem_id).unwrap().unwrap();
        assert_eq!(rec.embedding_model, "model-b");
        assert!((rec.embedding[0] - 0.9).abs() < 1e-5);
        // id should remain the original (we update in place)
        assert_eq!(rec.id, "vec-1");
    }

    #[test]
    fn test_delete_vector() {
        let conn = open_in_memory().unwrap();
        let (ws_id, mem_id) = seed_workspace_and_memory(&conn);

        let embedding = vec![0.1];
        upsert_vector(&conn, "vec-1", &mem_id, &ws_id, &embedding, "model-a").unwrap();

        let deleted = delete_vector_by_memory_id(&conn, &mem_id).unwrap();
        assert!(deleted);

        let rec = get_vector_by_memory_id(&conn, &mem_id).unwrap();
        assert!(rec.is_none());
    }

    #[test]
    fn test_delete_nonexistent_returns_false() {
        let conn = open_in_memory().unwrap();
        let deleted = delete_vector_by_memory_id(&conn, "no-such-mem").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let conn = open_in_memory().unwrap();
        let rec = get_vector_by_memory_id(&conn, "no-such-mem").unwrap();
        assert!(rec.is_none());
    }

    #[test]
    fn test_find_memories_without_vectors() {
        let conn = open_in_memory().unwrap();
        let (ws_id, mem_id) = seed_workspace_and_memory(&conn);

        let unindexed = find_memories_without_vectors(&conn, &ws_id, 10).unwrap();
        assert_eq!(unindexed.len(), 1);
        assert_eq!(unindexed[0].0, mem_id);
        assert_eq!(unindexed[0].1, "测试记忆");

        // Index the memory
        let embedding = vec![0.1, 0.2];
        upsert_vector(&conn, "vec-1", &mem_id, &ws_id, &embedding, "model-a").unwrap();

        let unindexed2 = find_memories_without_vectors(&conn, &ws_id, 10).unwrap();
        assert!(unindexed2.is_empty());
    }

    #[test]
    fn test_find_memories_without_vectors_all_indexed() {
        let conn = open_in_memory().unwrap();
        // No memories exist at all
        let unindexed = find_memories_without_vectors(&conn, "no-such-ws", 10).unwrap();
        assert!(unindexed.is_empty());
    }
}
