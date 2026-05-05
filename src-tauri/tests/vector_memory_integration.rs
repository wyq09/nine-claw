//! Integration tests for the vector memory system.
//!
//! Verifies the full pipeline: embedding store → cosine search → read → update → delete,
//! including workspace scoping, threshold filtering, three-layer scope search, and backfill.

use app_lib::storage::db::open_in_memory;

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn insert_workspace(conn: &rusqlite::Connection, id: &str, supervisor: &str) {
    let now = now_ms();
    conn.execute(
        "INSERT OR IGNORE INTO workspaces (id, name, description, supervisor_agent_id, created_at, updated_at, archived)
         VALUES (?1, ?2, '', ?3, ?4, ?4, 0)",
        rusqlite::params![id, id, supervisor, now],
    )
    .unwrap();
}

fn insert_memory(
    conn: &rusqlite::Connection,
    id: &str,
    ws: &str,
    title: &str,
    content: &str,
    scope: &str,
    scope_agent_id: Option<&str>,
) {
    let now = now_ms();
    conn.execute(
        "INSERT INTO workspace_memories (id, workspace_id, title, content, author_agent_id, tags_json, scope, scope_agent_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, NULL, '[]', ?5, ?6, ?7, ?7)",
        rusqlite::params![id, ws, title, content, scope, scope_agent_id, now],
    )
    .unwrap();
}

fn insert_vector(
    conn: &rusqlite::Connection,
    memory_id: &str,
    workspace_id: &str,
    embedding: &[f32],
    model: &str,
) {
    let id = format!("vec_{}", uuid::Uuid::new_v4().simple());
    app_lib::memory_vector::upsert_vector(conn, &id, memory_id, workspace_id, embedding, model)
        .unwrap();
}

/// Wrapper around search_vectors with no scope/tag filters.
fn search(
    conn: &rusqlite::Connection,
    ws: &str,
    query: &[f32],
    limit: usize,
    threshold: f32,
) -> Vec<app_lib::memory_vector::vector_search::SearchHit> {
    app_lib::memory_vector::search_vectors(conn, ws, query, limit, threshold, None, None, None)
        .unwrap()
}

fn setup() -> rusqlite::Connection {
    open_in_memory().unwrap()
}

// ── Full CRUD lifecycle ──────────────────────────────────────────────────────

#[test]
fn test_full_crud_lifecycle() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_memory(
        &conn,
        "m1",
        "ws1",
        "项目架构决策",
        "采用微服务架构，使用 Rust 后端",
        "workspace",
        None,
    );

    // Create vector
    let emb = vec![0.1; 8];
    insert_vector(&conn, "m1", "ws1", &emb, "bge-small-zh-local");

    // Read
    let rec = app_lib::memory_vector::get_vector_by_memory_id(&conn, "m1")
        .unwrap()
        .unwrap();
    assert_eq!(rec.memory_id, "m1");
    assert_eq!(rec.dimension, 8);
    assert_eq!(rec.embedding_model, "bge-small-zh-local");

    // Upsert (update)
    let emb2 = vec![0.9; 8];
    insert_vector(&conn, "m1", "ws1", &emb2, "bge-small-zh-local-v2");
    let updated = app_lib::memory_vector::get_vector_by_memory_id(&conn, "m1")
        .unwrap()
        .unwrap();
    assert_eq!(updated.embedding[0], 0.9);
    assert_eq!(updated.embedding_model, "bge-small-zh-local-v2");

    // Delete
    let deleted = app_lib::memory_vector::delete_vector_by_memory_id(&conn, "m1").unwrap();
    assert!(deleted);
    assert!(app_lib::memory_vector::get_vector_by_memory_id(&conn, "m1")
        .unwrap()
        .is_none());
}

// ── Cosine search returns ranked by similarity ───────────────────────────────

#[test]
fn test_cosine_search_returns_similar() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");

    insert_memory(
        &conn,
        "m1",
        "ws1",
        "Rust 后端",
        "使用 Rust 开发后端",
        "workspace",
        None,
    );
    insert_memory(
        &conn,
        "m2",
        "ws1",
        "前端框架",
        "使用 React 开发前端",
        "workspace",
        None,
    );
    insert_memory(
        &conn,
        "m3",
        "ws1",
        "数据库选型",
        "选择 SQLite 作为存储",
        "workspace",
        None,
    );

    let query_emb = vec![0.9, 0.1];
    let m1_emb = vec![1.0, 0.0]; // very similar to query
    let m2_emb = vec![0.0, 1.0]; // orthogonal
    let m3_emb = vec![0.1, 0.9]; // dissimilar but positive

    insert_vector(&conn, "m1", "ws1", &m1_emb, "model");
    insert_vector(&conn, "m2", "ws1", &m2_emb, "model");
    insert_vector(&conn, "m3", "ws1", &m3_emb, "model");

    let results = search(&conn, "ws1", &query_emb, 10, 0.0);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].memory_id, "m1");
    assert!(results[0].score > results[1].score);
}

// ── Search respects threshold ────────────────────────────────────────────────

#[test]
fn test_search_threshold_filters_low_similarity() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_memory(&conn, "m1", "ws1", "test", "content", "workspace", None);
    insert_vector(&conn, "m1", "ws1", &vec![1.0, 0.0], "model");

    let results = search(&conn, "ws1", &vec![0.0, 1.0], 10, 0.5);
    assert!(results.is_empty());
}

// ── Search is workspace-scoped ───────────────────────────────────────────────

#[test]
fn test_search_workspace_scoped() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_workspace(&conn, "ws2", "sup-2");

    insert_memory(&conn, "m1", "ws1", "ws1 记忆", "内容", "workspace", None);
    insert_memory(&conn, "m2", "ws2", "ws2 记忆", "内容", "workspace", None);
    insert_vector(&conn, "m1", "ws1", &vec![1.0], "model");
    insert_vector(&conn, "m2", "ws2", &vec![1.0], "model");

    let results = search(&conn, "ws1", &vec![1.0], 10, 0.0);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory_id, "m1");
}

// ── Empty workspace returns no results ───────────────────────────────────────

#[test]
fn test_search_empty_workspace() {
    let conn = setup();
    let results = search(&conn, "ws-nonexistent", &vec![0.1; 8], 10, 0.0);
    assert!(results.is_empty());
}

// ── find_memories_without_vectors ────────────────────────────────────────────

#[test]
fn test_find_missing_vectors() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_memory(&conn, "m1", "ws1", "indexed", "content", "workspace", None);
    insert_memory(
        &conn,
        "m2",
        "ws1",
        "not indexed",
        "content",
        "workspace",
        None,
    );
    insert_memory(
        &conn,
        "m3",
        "ws1",
        "also not indexed",
        "content",
        "workspace",
        None,
    );
    insert_vector(&conn, "m1", "ws1", &vec![0.1; 4], "model");

    let missing = app_lib::memory_vector::find_memories_without_vectors(&conn, "ws1", 10).unwrap();
    assert_eq!(missing.len(), 2);
    let ids: Vec<&str> = missing.iter().map(|(id, _, _)| id.as_str()).collect();
    assert!(ids.contains(&"m2"));
    assert!(ids.contains(&"m3"));
}

// ── Dimension mismatch handling ──────────────────────────────────────────────

#[test]
fn test_different_dimensions_in_same_workspace() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_memory(&conn, "m1", "ws1", "8d", "content", "workspace", None);
    insert_memory(&conn, "m2", "ws1", "4d", "content", "workspace", None);
    insert_vector(&conn, "m1", "ws1", &vec![1.0, 0.0], "model-2d-a");
    insert_vector(&conn, "m2", "ws1", &vec![0.0, 1.0], "model-2d-b");

    let results = search(&conn, "ws1", &vec![1.0, 0.0], 10, 0.5);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory_id, "m1");
}

// ── Embedding blob roundtrip with realistic data ─────────────────────────────

#[test]
fn test_embedding_blob_roundtrip_realistic() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_memory(&conn, "m1", "ws1", "test", "content", "workspace", None);
    let original: Vec<f32> = (0..32).map(|i| (i as f32) * 0.001).collect();
    insert_vector(&conn, "m1", "ws1", &original, "model");

    let rec = app_lib::memory_vector::get_vector_by_memory_id(&conn, "m1")
        .unwrap()
        .unwrap();
    assert_eq!(rec.embedding.len(), 32);
    for (i, (a, b)) in original.iter().zip(rec.embedding.iter()).enumerate() {
        assert!(
            (a - b).abs() < 1e-6,
            "Mismatch at index {}: {} vs {}",
            i,
            a,
            b
        );
    }
}

// ── Multiple vectors ranked by similarity ────────────────────────────────────

#[test]
fn test_multiple_vectors_ranked_by_similarity() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");

    for i in 0..5 {
        let id = format!("m{i}");
        insert_memory(
            &conn,
            &id,
            "ws1",
            &format!("Memory {i}"),
            "content",
            "workspace",
            None,
        );
        let mut emb = vec![0.0; 8];
        emb[i] = 1.0;
        insert_vector(&conn, &id, "ws1", &emb, "model");
    }

    let mut query = vec![0.0; 8];
    query[2] = 0.99;
    query[3] = 0.01;

    let results = search(&conn, "ws1", &query, 3, 0.0);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].memory_id, "m2");
}

// ── Update memory preserves vector ───────────────────────────────────────────

#[test]
fn test_update_memory_preserves_vector() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_memory(&conn, "m1", "ws1", "original", "content", "workspace", None);
    let emb = vec![0.42; 4];
    insert_vector(&conn, "m1", "ws1", &emb, "model");

    let now = now_ms();
    conn.execute(
        "UPDATE workspace_memories SET title = 'updated', updated_at = ?1 WHERE id = 'm1'",
        rusqlite::params![now],
    )
    .unwrap();

    let rec = app_lib::memory_vector::get_vector_by_memory_id(&conn, "m1")
        .unwrap()
        .unwrap();
    assert_eq!(rec.embedding[0], 0.42);
}

// ── find_similar above threshold ─────────────────────────────────────────────

#[test]
fn test_find_similar_above_threshold() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_memory(&conn, "m1", "ws1", "test", "content", "workspace", None);
    insert_vector(&conn, "m1", "ws1", &vec![1.0, 0.0], "model");

    let similar =
        app_lib::memory_vector::vector_search::find_similar(&conn, "ws1", &vec![0.99, 0.01], 0.9)
            .unwrap();
    assert!(similar.is_some());
    assert_eq!(similar.unwrap(), "m1");
}

// ── find_similar below threshold ─────────────────────────────────────────────

#[test]
fn test_find_similar_below_threshold() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_memory(&conn, "m1", "ws1", "test", "content", "workspace", None);
    insert_vector(&conn, "m1", "ws1", &vec![1.0, 0.0], "model");

    let similar =
        app_lib::memory_vector::vector_search::find_similar(&conn, "ws1", &vec![0.0, 1.0], 0.9)
            .unwrap();
    assert!(similar.is_none());
}

// ── Three-layer scope: system cross-workspace ────────────────────────────────

#[test]
fn test_three_layer_system_cross_workspace() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_workspace(&conn, "ws2", "sup-2");

    insert_memory(
        &conn,
        "sys1",
        "ws2",
        "全局规则",
        "所有 workspace 共享",
        "system",
        None,
    );
    insert_vector(&conn, "sys1", "ws2", &vec![1.0, 0.0], "model");

    let results = app_lib::memory_vector::three_layer_search(
        &conn,
        "ws1",
        None,
        true,
        &vec![0.99, 0.01],
        10,
        0.5,
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory_id, "sys1");
}

// ── Three-layer scope: agent isolation ───────────────────────────────────────

#[test]
fn test_three_layer_agent_isolation() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");

    insert_memory(
        &conn,
        "a1",
        "ws1",
        "Agent A 私有",
        "agent-a 私有记忆",
        "agent",
        Some("agent-a"),
    );
    insert_memory(
        &conn,
        "a2",
        "ws1",
        "Agent B 私有",
        "agent-b 私有记忆",
        "agent",
        Some("agent-b"),
    );
    insert_vector(&conn, "a1", "ws1", &vec![1.0, 0.0], "model");
    insert_vector(&conn, "a2", "ws1", &vec![1.0, 0.0], "model");

    let results = app_lib::memory_vector::three_layer_search(
        &conn,
        "ws1",
        Some("agent-a"),
        false,
        &vec![1.0, 0.0],
        10,
        0.5,
    )
    .unwrap();
    let ids: Vec<&str> = results.iter().map(|h| h.memory_id.as_str()).collect();
    assert!(ids.contains(&"a1"));
    assert!(!ids.contains(&"a2"));
}

// ── Three-layer scope: supervisor sees all ───────────────────────────────────

#[test]
fn test_three_layer_supervisor_sees_all() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");

    insert_memory(
        &conn,
        "a1",
        "ws1",
        "Agent A 私有",
        "private",
        "agent",
        Some("agent-a"),
    );
    insert_vector(&conn, "a1", "ws1", &vec![1.0, 0.0], "model");

    let results = app_lib::memory_vector::three_layer_search(
        &conn,
        "ws1",
        Some("sup-1"),
        true,
        &vec![1.0, 0.0],
        10,
        0.5,
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory_id, "a1");
}

// ── Backfill: find_memories_without_vectors after insert ─────────────────────

#[test]
fn test_backfill_finds_unindexed() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_memory(&conn, "m1", "ws1", "已索引", "content", "workspace", None);
    insert_memory(&conn, "m2", "ws1", "未索引", "content", "workspace", None);
    insert_vector(&conn, "m1", "ws1", &vec![0.1; 4], "model");

    let missing = app_lib::memory_vector::find_memories_without_vectors(&conn, "ws1", 50).unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].0, "m2");
}

// ── All indexed: find_memories_without_vectors returns empty ──────────────────

#[test]
fn test_backfill_no_missing() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");
    insert_memory(&conn, "m1", "ws1", "t", "c", "workspace", None);
    insert_vector(&conn, "m1", "ws1", &vec![0.1; 4], "model");

    let missing = app_lib::memory_vector::find_memories_without_vectors(&conn, "ws1", 50).unwrap();
    assert!(missing.is_empty());
}

// ── Delete nonexistent vector returns false ───────────────────────────────────

#[test]
fn test_delete_nonexistent_vector() {
    let conn = setup();
    let deleted = app_lib::memory_vector::delete_vector_by_memory_id(&conn, "no-such-id").unwrap();
    assert!(!deleted);
}

// ── Get nonexistent vector returns None ──────────────────────────────────────

#[test]
fn test_get_nonexistent_vector() {
    let conn = setup();
    let result = app_lib::memory_vector::get_vector_by_memory_id(&conn, "no-such-id").unwrap();
    assert!(result.is_none());
}

// ── Workspace memory CRUD via storage layer ──────────────────────────────────

#[test]
fn test_workspace_memory_storage_crud() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");

    let record = app_lib::storage::workspaces::insert_workspace_memory(
        &conn,
        "mem-crud-1",
        "ws1",
        "测试标题",
        "测试内容",
        Some("agent-1"),
        r#"["tag1","tag2"]"#,
        "workspace",
        None,
    )
    .unwrap();
    assert_eq!(record.id, "mem-crud-1");
    assert_eq!(record.title, "测试标题");

    let fetched = app_lib::storage::workspaces::get_workspace_memory(&conn, "mem-crud-1")
        .unwrap()
        .unwrap();
    assert_eq!(fetched.content, "测试内容");

    let updated = app_lib::storage::workspaces::update_workspace_memory(
        &conn,
        "mem-crud-1",
        Some("更新标题"),
        Some("更新内容"),
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(updated.title, "更新标题");

    app_lib::storage::workspaces::delete_workspace_memory(&conn, "ws1", "mem-crud-1").unwrap();
    assert!(
        app_lib::storage::workspaces::get_workspace_memory(&conn, "mem-crud-1")
            .unwrap()
            .is_none()
    );
}

// ── Three-layer scope: three-layer merge combines system + workspace + agent ──

#[test]
fn test_three_layer_merge_all_scopes() {
    let conn = setup();
    insert_workspace(&conn, "ws1", "sup-1");

    insert_memory(&conn, "sys1", "ws1", "全局规则", "全局共享", "system", None);
    insert_memory(
        &conn,
        "ws-mem",
        "ws1",
        "工作空间记忆",
        "团队共享",
        "workspace",
        None,
    );
    insert_memory(
        &conn,
        "ag1",
        "ws1",
        "私有记忆",
        "agent-a 私有",
        "agent",
        Some("agent-a"),
    );

    // All use same direction vector
    let dir = vec![1.0, 0.0];
    insert_vector(&conn, "sys1", "ws1", &dir, "model");
    insert_vector(&conn, "ws-mem", "ws1", &dir, "model");
    insert_vector(&conn, "ag1", "ws1", &dir, "model");

    // agent-a sees system + workspace + its own agent memories
    let results = app_lib::memory_vector::three_layer_search(
        &conn,
        "ws1",
        Some("agent-a"),
        false,
        &vec![0.99, 0.01],
        10,
        0.5,
    )
    .unwrap();
    let ids: Vec<&str> = results.iter().map(|h| h.memory_id.as_str()).collect();
    assert!(ids.contains(&"sys1"));
    assert!(ids.contains(&"ws-mem"));
    assert!(ids.contains(&"ag1"));
}
