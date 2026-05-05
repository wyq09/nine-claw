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
    upsert_vector_with_meta(
        conn,
        id,
        memory_id,
        workspace_id,
        embedding,
        model_id,
        None,
        None,
        None,
    )
}

/// 插入或更新向量（带元数据）。若 memory_id 已有向量则更新，否则插入。
pub fn upsert_vector_with_meta(
    conn: &Connection,
    id: &str,
    memory_id: &str,
    workspace_id: &str,
    embedding: &[f32],
    model_id: &str,
    metadata_json: Option<&str>,
    content_text: Option<&str>,
    tags_json: Option<&str>,
) -> Result<(), String> {
    let dim = embedding.len() as i32;
    let blob = embedding_to_blob(embedding);
    let now = now_ms();

    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM memory_vectors WHERE memory_id = ?1",
            params![memory_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("查询已有向量失败: {e}"))?;

    if let Some(existing_id) = existing {
        conn.execute(
            "UPDATE memory_vectors
             SET embedding = ?1, embedding_model = ?2, dimension = ?3, updated_at = ?4,
                 metadata_json = COALESCE(?5, metadata_json),
                 content_text = COALESCE(?6, content_text),
                 tags_json = COALESCE(?7, tags_json)
             WHERE id = ?8",
            params![
                blob,
                model_id,
                dim,
                now,
                metadata_json,
                content_text,
                tags_json,
                existing_id
            ],
        )
        .map_err(|e| format!("更新向量失败: {e}"))?;
    } else {
        conn.execute(
            "INSERT INTO memory_vectors (id, memory_id, workspace_id, embedding, embedding_model, dimension, created_at, updated_at, metadata_json, content_text, tags_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id, memory_id, workspace_id, blob, model_id, dim, now, now, metadata_json, content_text, tags_json],
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
    scope_filter: Option<&[String]>,
    scope_agent_id: Option<&str>,
) -> Result<Vec<vector_search::SearchHit>, String> {
    vector_search::cosine_search(
        conn,
        workspace_id,
        query_embedding,
        limit,
        threshold,
        tag_filter,
        scope_filter,
        scope_agent_id,
    )
}

pub fn search_vectors_across_workspaces(
    conn: &Connection,
    workspace_ids: &[String],
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
) -> Result<Vec<vector_search::SearchHit>, String> {
    let mut merged = Vec::new();
    for workspace_id in workspace_ids {
        let mut hits = vector_search::cosine_search(
            conn,
            workspace_id,
            query_embedding,
            limit,
            threshold,
            None,
            None,
            None,
        )?;
        merged.append(&mut hits);
    }

    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut dedup = std::collections::HashSet::new();
    let mut out = Vec::new();
    for hit in merged {
        if dedup.insert(hit.memory_id.clone()) {
            out.push(hit);
        }
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

/// Three-layer memory search: merges results from system, workspace, and agent scopes.
///
/// For agent A in workspace W:
/// - System: scope='system' (all workspaces)
/// - Workspace: scope='workspace', workspace_id=W
/// - Agent: scope='agent', workspace_id=W, scope_agent_id=A (or all agents if is_supervisor)
pub fn three_layer_search(
    conn: &Connection,
    workspace_id: &str,
    agent_id: Option<&str>,
    is_supervisor: bool,
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
) -> Result<Vec<vector_search::SearchHit>, String> {
    // Build scope list
    let mut scopes = vec!["system".to_string(), "workspace".to_string()];
    let effective_agent_id: Option<&str>;

    if is_supervisor {
        // Supervisor sees all scopes including agent memories
        scopes.push("agent".to_string());
        effective_agent_id = None; // no agent filter — sees all agent memories
    } else if let Some(aid) = agent_id {
        scopes.push("agent".to_string());
        effective_agent_id = Some(aid);
    } else {
        // No agent context — only system + workspace
        effective_agent_id = None;
    }

    let hits = vector_search::cosine_search(
        conn,
        workspace_id,
        query_embedding,
        limit * 3, // fetch extra for dedup headroom
        threshold,
        None,
        Some(&scopes),
        effective_agent_id,
    )?;

    // Deduplicate by memory_id, keep highest score
    let mut best: std::collections::HashMap<String, vector_search::SearchHit> =
        std::collections::HashMap::new();
    for hit in hits {
        use std::collections::hash_map::Entry;
        match best.entry(hit.memory_id.clone()) {
            Entry::Vacant(e) => {
                e.insert(hit);
            }
            Entry::Occupied(mut e) => {
                if hit.score > e.get().score {
                    e.insert(hit);
                }
            }
        }
    }

    let mut results: Vec<vector_search::SearchHit> = best.into_values().collect();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);
    Ok(results)
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

    // ── Integration tests ──────────────────────────────────────────────

    /// Helper: insert a workspace row.
    fn ensure_workspace(conn: &Connection, ws_id: &str) {
        let now = now_ms();
        conn.execute(
            "INSERT OR IGNORE INTO workspaces (id, name, description, supervisor_agent_id, created_at, updated_at, archived)
             VALUES (?1, ?2, '', '', ?3, ?3, 0)",
            params![ws_id, ws_id, now],
        ).unwrap();
    }

    /// Helper: insert a workspace_memories row.
    fn insert_test_memory(conn: &Connection, id: &str, ws: &str, title: &str, content: &str) {
        let now = now_ms();
        conn.execute(
            "INSERT OR IGNORE INTO workspace_memories (id, workspace_id, title, content, tags_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, '[]', ?5, ?6)",
            params![id, ws, title, content, now, now],
        ).unwrap();
    }

    /// Helper: insert a vector for the given memory.
    fn insert_test_vector(
        conn: &Connection,
        memory_id: &str,
        workspace_id: &str,
        embedding: &[f32],
    ) {
        let id = uuid::Uuid::new_v4().to_string();
        upsert_vector(conn, &id, memory_id, workspace_id, embedding, "test-model").unwrap();
    }

    // ── 1. Full CRUD lifecycle ─────────────────────────────────────────

    #[test]
    fn test_full_crud_lifecycle() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-crud";
        let mem_id = "mem-crud-1";
        ensure_workspace(&conn, ws);
        insert_test_memory(&conn, mem_id, ws, "Lifecycle", "Content");

        // Create
        let emb = vec![0.5, 0.5, 0.5];
        insert_test_vector(&conn, mem_id, ws, &emb);
        let rec = get_vector_by_memory_id(&conn, mem_id).unwrap().unwrap();
        assert_eq!(rec.memory_id, mem_id);
        assert_eq!(rec.dimension, 3);

        // Update (upsert with same memory_id)
        let emb2 = vec![0.9, 0.1, 0.0];
        insert_test_vector(&conn, mem_id, ws, &emb2);
        let rec2 = get_vector_by_memory_id(&conn, mem_id).unwrap().unwrap();
        assert!((rec2.embedding[0] - 0.9).abs() < 1e-5);

        // Delete
        let deleted = delete_vector_by_memory_id(&conn, mem_id).unwrap();
        assert!(deleted);
        assert!(get_vector_by_memory_id(&conn, mem_id).unwrap().is_none());
    }

    // ── 2. Cosine search returns most similar ──────────────────────────

    #[test]
    fn test_cosine_search_returns_similar() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-search-sim";
        ensure_workspace(&conn, ws);

        // Three memories with different embeddings
        let mems = vec![
            ("m1".to_string(), vec![1.0f32, 0.0, 0.0]),
            ("m2".to_string(), vec![0.0f32, 1.0, 0.0]),
            ("m3".to_string(), vec![0.0f32, 0.0, 1.0]),
        ];
        for (mid, emb) in &mems {
            insert_test_memory(&conn, mid, ws, mid, "content");
            insert_test_vector(&conn, mid, ws, emb);
        }

        // Query near m1
        let query = vec![0.99, 0.01, 0.0f32];
        let hits = search_vectors(&conn, ws, &query, 10, 0.0, None, None, None).unwrap();
        assert!(!hits.is_empty(), "should have results");
        assert_eq!(hits[0].memory_id, "m1", "m1 should be top result");
    }

    // ── 3. Search respects threshold ───────────────────────────────────

    #[test]
    fn test_search_respects_threshold() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-thresh";
        ensure_workspace(&conn, ws);

        insert_test_memory(&conn, "m-orth", ws, "orth", "content");
        insert_test_vector(&conn, "m-orth", ws, &[1.0, 0.0f32]);

        // Query orthogonal [0,1] vs stored [1,0] -> similarity = 0
        let hits = search_vectors(&conn, ws, &[0.0, 1.0f32], 10, 0.5, None, None, None).unwrap();
        assert!(
            hits.is_empty(),
            "orthogonal vectors should not meet threshold 0.5"
        );
    }

    // ── 4. Search is workspace-scoped ─────────────────────────────────

    #[test]
    fn test_search_workspace_scoped() {
        let conn = open_in_memory().unwrap();
        let ws1 = "ws-scope-1";
        let ws2 = "ws-scope-2";
        ensure_workspace(&conn, ws1);
        ensure_workspace(&conn, ws2);

        insert_test_memory(&conn, "mem-a", ws1, "A", "content");
        insert_test_vector(&conn, "mem-a", ws1, &[1.0, 0.0f32]);

        insert_test_memory(&conn, "mem-b", ws2, "B", "content");
        insert_test_vector(&conn, "mem-b", ws2, &[1.0, 0.0f32]);

        // Search ws1 — should only get mem-a
        let hits = search_vectors(&conn, ws1, &[1.0, 0.0f32], 10, 0.0, None, None, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].memory_id, "mem-a");
    }

    // ── 5. Search empty workspace ──────────────────────────────────────

    #[test]
    fn test_search_empty_workspace() {
        let conn = open_in_memory().unwrap();
        let hits = search_vectors(
            &conn,
            "ws-nonexistent",
            &[1.0, 0.0f32],
            10,
            0.0,
            None,
            None,
            None,
        )
        .unwrap();
        assert!(hits.is_empty());
    }

    // ── 6. Find missing vectors (partial index) ────────────────────────

    #[test]
    fn test_find_missing_vectors_partial_index() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-missing";
        ensure_workspace(&conn, ws);

        // 3 memories, only 1 indexed
        for mid in ["m1", "m2", "m3"] {
            insert_test_memory(&conn, mid, ws, mid, "content");
        }
        insert_test_vector(&conn, "m1", ws, &[0.1, 0.2f32]);

        let unindexed = find_memories_without_vectors(&conn, ws, 10).unwrap();
        assert_eq!(unindexed.len(), 2);
        let ids: Vec<&str> = unindexed.iter().map(|(id, _, _)| id.as_str()).collect();
        assert!(ids.contains(&"m2"));
        assert!(ids.contains(&"m3"));
    }

    // ── 7. Different dimensions in same workspace ──────────────────────

    #[test]
    fn test_different_dimensions_in_same_workspace() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-dims";
        ensure_workspace(&conn, ws);

        let emb_512: Vec<f32> = (0..512).map(|i| (i as f32) * 0.001).collect();
        let emb_256: Vec<f32> = (0..256).map(|i| (i as f32) * 0.001).collect();

        insert_test_memory(&conn, "mem-512", ws, "512d", "content");
        insert_test_vector(&conn, "mem-512", ws, &emb_512);

        insert_test_memory(&conn, "mem-256", ws, "256d", "content");
        insert_test_vector(&conn, "mem-256", ws, &emb_256);

        // Search with 512-dim query — only 512-dim results (256-dim vectors will have similarity 0 due to dimension mismatch in cosine)
        let mut query_512 = emb_512.clone();
        query_512[0] = 10.0; // boost to make similarity clear
        let hits = search_vectors(&conn, ws, &query_512, 10, 0.5, None, None, None).unwrap();
        // The 512-dim vector should have high similarity, the 256-dim one should not
        // (cosine_similarity returns 0.0 for different-length vectors)
        assert!(
            hits.iter().any(|h| h.memory_id == "mem-512"),
            "should find 512-dim memory"
        );
        assert!(
            !hits.iter().any(|h| h.memory_id == "mem-256"),
            "should not find 256-dim memory with 512-dim query"
        );
    }

    // ── 8. Embedding blob roundtrip with realistic 512 floats ──────────

    #[test]
    fn test_embedding_blob_roundtrip_realistic() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-roundtrip";
        let mem_id = "mem-roundtrip";
        ensure_workspace(&conn, ws);
        insert_test_memory(&conn, mem_id, ws, "Roundtrip", "content");

        let mut emb: Vec<f32> = (0..512)
            .map(|i| {
                let x = (i as f32) * 0.001 - 0.256;
                (x * 1000.0).round() / 1000.0 // keep precision manageable
            })
            .collect();
        emb[0] = 0.123;
        emb[511] = -0.987;

        insert_test_vector(&conn, mem_id, ws, &emb);

        let rec = get_vector_by_memory_id(&conn, mem_id).unwrap().unwrap();
        assert_eq!(rec.embedding.len(), 512);
        for (i, (a, b)) in emb.iter().zip(rec.embedding.iter()).enumerate() {
            assert!(
                (a - b).abs() < 1e-5,
                "mismatch at index {i}: expected {a}, got {b}"
            );
        }
    }

    // ── 9. Multiple vectors ranked by similarity ───────────────────────

    #[test]
    fn test_multiple_vectors_ranked_by_similarity() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-ranked";
        ensure_workspace(&conn, ws);

        // 5 orthogonal-ish memories
        let embs: Vec<(&str, Vec<f32>)> = vec![
            ("m1", vec![1.0, 0.0, 0.0, 0.0, 0.0]),
            ("m2", vec![0.0, 1.0, 0.0, 0.0, 0.0]),
            ("m3", vec![0.0, 0.0, 1.0, 0.0, 0.0]),
            ("m4", vec![0.0, 0.0, 0.0, 1.0, 0.0]),
            ("m5", vec![0.0, 0.0, 0.0, 0.0, 1.0]),
        ];
        for (mid, emb) in &embs {
            insert_test_memory(&conn, mid, ws, mid, "content");
            insert_test_vector(&conn, mid, ws, emb);
        }

        // Query near m2
        let query = vec![0.01, 0.99, 0.01, 0.0, 0.0f32];
        let hits = search_vectors(&conn, ws, &query, 10, 0.0, None, None, None).unwrap();
        assert_eq!(hits[0].memory_id, "m2", "m2 should be top result");
        // Verify scores are descending
        for window in hits.windows(2) {
            assert!(
                window[0].score >= window[1].score,
                "results should be sorted by score descending"
            );
        }
    }

    // ── 10. Update memory preserves vector ─────────────────────────────

    #[test]
    fn test_update_memory_preserves_vector() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-preserve";
        let mem_id = "mem-preserve";
        ensure_workspace(&conn, ws);
        insert_test_memory(&conn, mem_id, ws, "Original title", "Original content");
        let emb = vec![0.42, 0.58f32];
        insert_test_vector(&conn, mem_id, ws, &emb);

        // Update memory title (direct SQL, simulating workspace memory update)
        let now = now_ms();
        conn.execute(
            "UPDATE workspace_memories SET title = 'Updated title', updated_at = ?1 WHERE id = ?2",
            params![now, mem_id],
        )
        .unwrap();

        // Vector should still be there
        let rec = get_vector_by_memory_id(&conn, mem_id).unwrap().unwrap();
        assert!((rec.embedding[0] - 0.42).abs() < 1e-5);
        assert!((rec.embedding[1] - 0.58).abs() < 1e-5);
    }

    // ── 11 & 12. find_similar above/below threshold ────────────────────

    #[test]
    fn test_find_similar_above_threshold() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-fsa";
        ensure_workspace(&conn, ws);
        insert_test_memory(&conn, "m-fsa", ws, "similar", "content");
        insert_test_vector(&conn, "m-fsa", ws, &[1.0, 0.0f32]);

        // Query very close to [1,0]
        let result = vector_search::find_similar(&conn, ws, &[0.99, 0.01f32], 0.9).unwrap();
        assert!(result.is_some(), "should find similar above threshold");
        assert_eq!(result.unwrap(), "m-fsa");
    }

    #[test]
    fn test_find_similar_below_threshold() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-fsb";
        ensure_workspace(&conn, ws);
        insert_test_memory(&conn, "m-fsb", ws, "not similar", "content");
        insert_test_vector(&conn, "m-fsb", ws, &[1.0, 0.0f32]);

        // Query orthogonal
        let result = vector_search::find_similar(&conn, ws, &[0.0, 1.0f32], 0.9).unwrap();
        assert!(result.is_none(), "should not find similar below threshold");
    }

    // ── 13. find_memories_without_vectors all indexed ──────────────────

    #[test]
    fn test_find_memories_without_vectors_all_indexed_with_data() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-allidx";
        ensure_workspace(&conn, ws);

        // Insert 3 memories and index all 3
        for mid in ["ma1", "ma2", "ma3"] {
            insert_test_memory(&conn, mid, ws, mid, "content");
            insert_test_vector(&conn, mid, ws, &[0.1, 0.2f32]);
        }

        let unindexed = find_memories_without_vectors(&conn, ws, 10).unwrap();
        assert!(
            unindexed.is_empty(),
            "all memories are indexed, should return empty"
        );
    }

    // ── Three-layer memory integration tests ───────────────────────────

    #[test]
    fn test_scope_default_is_workspace() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-default-scope";
        ensure_workspace(&conn, ws);

        // Insert via helper (uses direct SQL, no scope column specified)
        insert_test_memory(&conn, "mem-default", ws, "Default scope", "content");

        // Verify scope defaults to 'workspace'
        let record = crate::storage::workspaces::get_workspace_memory(&conn, "mem-default")
            .unwrap()
            .unwrap();
        assert_eq!(record.scope, "workspace");
        assert!(record.scope_agent_id.is_none());
    }

    #[test]
    fn test_scope_system_searchable_from_any_workspace() {
        let conn = open_in_memory().unwrap();
        let ws1 = "ws-sys-1";
        let ws2 = "ws-sys-2";
        ensure_workspace(&conn, ws1);
        ensure_workspace(&conn, ws2);

        // Create system memory in ws1
        insert_test_memory(&conn, "mem-sys", ws1, "System memory", "global content");
        // Set scope to system
        crate::storage::workspaces::update_memory_scope(&conn, "mem-sys", "system", None).unwrap();
        // Index it
        insert_test_vector(&conn, "mem-sys", ws1, &[1.0, 0.0, 0.0]);

        // Search in ws2 — should find the system memory from ws1
        let hits = search_vectors(&conn, ws2, &[1.0, 0.0, 0.0], 10, 0.5, None, None, None).unwrap();
        assert_eq!(
            hits.len(),
            1,
            "system memory should be searchable from any workspace"
        );
        assert_eq!(hits[0].memory_id, "mem-sys");
    }

    #[test]
    fn test_scope_agent_isolated() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-agent-iso";
        ensure_workspace(&conn, ws);

        // Create agent-scoped memories for two different agents
        insert_test_memory(&conn, "mem-agent-a", ws, "Agent A memory", "private to A");
        insert_test_memory(&conn, "mem-agent-b", ws, "Agent B memory", "private to B");

        crate::storage::workspaces::update_memory_scope(
            &conn,
            "mem-agent-a",
            "agent",
            Some("agent-a"),
        )
        .unwrap();
        crate::storage::workspaces::update_memory_scope(
            &conn,
            "mem-agent-b",
            "agent",
            Some("agent-b"),
        )
        .unwrap();

        insert_test_vector(&conn, "mem-agent-a", ws, &[1.0, 0.0]);
        insert_test_vector(&conn, "mem-agent-b", ws, &[1.0, 0.0]);

        // Agent A searching — should NOT see agent B's memories
        let hits_a =
            three_layer_search(&conn, ws, Some("agent-a"), false, &[1.0, 0.0], 10, 0.5).unwrap();
        let ids_a: Vec<&str> = hits_a.iter().map(|h| h.memory_id.as_str()).collect();
        assert!(
            ids_a.contains(&"mem-agent-a"),
            "agent A should see own memory"
        );
        assert!(
            !ids_a.contains(&"mem-agent-b"),
            "agent A should NOT see agent B's memory"
        );

        // Agent B searching — should NOT see agent A's memories
        let hits_b =
            three_layer_search(&conn, ws, Some("agent-b"), false, &[1.0, 0.0], 10, 0.5).unwrap();
        let ids_b: Vec<&str> = hits_b.iter().map(|h| h.memory_id.as_str()).collect();
        assert!(
            ids_b.contains(&"mem-agent-b"),
            "agent B should see own memory"
        );
        assert!(
            !ids_b.contains(&"mem-agent-a"),
            "agent B should NOT see agent A's memory"
        );
    }

    #[test]
    fn test_scope_agent_visible_to_supervisor() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-supervisor";
        ensure_workspace(&conn, ws);

        // Create agent-scoped memories
        insert_test_memory(&conn, "mem-sup-a", ws, "Agent A private", "content A");
        insert_test_memory(&conn, "mem-sup-b", ws, "Agent B private", "content B");

        crate::storage::workspaces::update_memory_scope(
            &conn,
            "mem-sup-a",
            "agent",
            Some("agent-a"),
        )
        .unwrap();
        crate::storage::workspaces::update_memory_scope(
            &conn,
            "mem-sup-b",
            "agent",
            Some("agent-b"),
        )
        .unwrap();

        insert_test_vector(&conn, "mem-sup-a", ws, &[1.0, 0.0]);
        insert_test_vector(&conn, "mem-sup-b", ws, &[1.0, 0.0]);

        // Supervisor searching — should see ALL agent memories
        let hits = three_layer_search(&conn, ws, Some("supervisor-id"), true, &[1.0, 0.0], 10, 0.5)
            .unwrap();
        let ids: Vec<&str> = hits.iter().map(|h| h.memory_id.as_str()).collect();
        assert!(
            ids.contains(&"mem-sup-a"),
            "supervisor should see agent A's memory"
        );
        assert!(
            ids.contains(&"mem-sup-b"),
            "supervisor should see agent B's memory"
        );
    }

    #[test]
    fn test_three_layer_search_priority() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-layers";
        let ws_other = "ws-layers-other";
        ensure_workspace(&conn, ws);
        ensure_workspace(&conn, ws_other);

        // System memory in ws_other
        insert_test_memory(&conn, "mem-layer-sys", ws_other, "System fact", "global");
        crate::storage::workspaces::update_memory_scope(&conn, "mem-layer-sys", "system", None)
            .unwrap();
        insert_test_vector(&conn, "mem-layer-sys", ws_other, &[1.0, 0.0, 0.0]);

        // Workspace memory in ws
        insert_test_memory(&conn, "mem-layer-ws", ws, "Workspace fact", "project");
        insert_test_vector(&conn, "mem-layer-ws", ws, &[1.0, 0.0, 0.0]);

        // Agent memory in ws
        insert_test_memory(&conn, "mem-layer-agent", ws, "Agent fact", "private");
        crate::storage::workspaces::update_memory_scope(
            &conn,
            "mem-layer-agent",
            "agent",
            Some("agent-x"),
        )
        .unwrap();
        insert_test_vector(&conn, "mem-layer-agent", ws, &[1.0, 0.0, 0.0]);

        // Agent x searching — should see all 3
        let hits = three_layer_search(&conn, ws, Some("agent-x"), false, &[1.0, 0.0, 0.0], 10, 0.5)
            .unwrap();
        let ids: Vec<&str> = hits.iter().map(|h| h.memory_id.as_str()).collect();
        assert_eq!(ids.len(), 3, "should find memories from all 3 layers");
        assert!(ids.contains(&"mem-layer-sys"));
        assert!(ids.contains(&"mem-layer-ws"));
        assert!(ids.contains(&"mem-layer-agent"));
    }

    #[test]
    fn test_scope_update_migration() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-migrate";
        ensure_workspace(&conn, ws);

        // Insert memory (default scope = workspace)
        insert_test_memory(&conn, "mem-migrate", ws, "Migrate test", "content");

        // Verify default
        let rec = crate::storage::workspaces::get_workspace_memory(&conn, "mem-migrate")
            .unwrap()
            .unwrap();
        assert_eq!(rec.scope, "workspace");

        // Update to system
        crate::storage::workspaces::update_memory_scope(&conn, "mem-migrate", "system", None)
            .unwrap();
        let rec = crate::storage::workspaces::get_workspace_memory(&conn, "mem-migrate")
            .unwrap()
            .unwrap();
        assert_eq!(rec.scope, "system");
        assert!(rec.scope_agent_id.is_none());

        // Update to agent
        crate::storage::workspaces::update_memory_scope(
            &conn,
            "mem-migrate",
            "agent",
            Some("agent-1"),
        )
        .unwrap();
        let rec = crate::storage::workspaces::get_workspace_memory(&conn, "mem-migrate")
            .unwrap()
            .unwrap();
        assert_eq!(rec.scope, "agent");
        assert_eq!(rec.scope_agent_id.as_deref(), Some("agent-1"));
    }

    #[test]
    fn test_count_memories_by_scope() {
        let conn = open_in_memory().unwrap();
        let ws = "ws-count";
        ensure_workspace(&conn, ws);

        // 0 initially
        let (sys, ws_count, agent) =
            crate::storage::workspaces::count_memories_by_scope(&conn, ws).unwrap();
        assert_eq!((sys, ws_count, agent), (0, 0, 0));

        // Add 2 workspace memories
        insert_test_memory(&conn, "mem-c1", ws, "W1", "c");
        insert_test_memory(&conn, "mem-c2", ws, "W2", "c");
        let (sys, ws_count, agent) =
            crate::storage::workspaces::count_memories_by_scope(&conn, ws).unwrap();
        assert_eq!((sys, ws_count, agent), (0, 2, 0));

        // Change one to system
        crate::storage::workspaces::update_memory_scope(&conn, "mem-c1", "system", None).unwrap();
        let (sys, ws_count, agent) =
            crate::storage::workspaces::count_memories_by_scope(&conn, ws).unwrap();
        assert_eq!((sys, ws_count, agent), (1, 1, 0));

        // Add agent memory
        insert_test_memory(&conn, "mem-c3", ws, "A1", "c");
        crate::storage::workspaces::update_memory_scope(&conn, "mem-c3", "agent", Some("agent-x"))
            .unwrap();
        let (sys, ws_count, agent) =
            crate::storage::workspaces::count_memories_by_scope(&conn, ws).unwrap();
        assert_eq!((sys, ws_count, agent), (1, 1, 1));
    }
}
