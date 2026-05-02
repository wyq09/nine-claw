//! memory_vectors 与 embedding_providers 表迁移。

use rusqlite::Connection;

/// 创建 memory_vectors 和 embedding_providers 表（IF NOT EXISTS）。
pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_vectors (
            id TEXT PRIMARY KEY,
            memory_id TEXT NOT NULL REFERENCES workspace_memories(id) ON DELETE CASCADE,
            workspace_id TEXT NOT NULL,
            embedding BLOB NOT NULL,
            embedding_model TEXT NOT NULL,
            dimension INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_mv_workspace ON memory_vectors(workspace_id);
        CREATE INDEX IF NOT EXISTS idx_mv_memory ON memory_vectors(memory_id);

        CREATE TABLE IF NOT EXISTS embedding_providers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            provider_type TEXT NOT NULL CHECK(provider_type IN ('onnx_local', 'remote_api')),
            endpoint TEXT,
            model_name TEXT NOT NULL,
            api_key_ref TEXT,
            dimension INTEGER NOT NULL,
            is_default INTEGER NOT NULL DEFAULT 0
        );",
    )
    .map_err(|e| format!("初始化向量表失败: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_schema_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        ensure_schema(&conn).unwrap();
        // 表应存在且可写入
        conn.execute(
            "INSERT INTO embedding_providers (id, name, provider_type, model_name, dimension)
             VALUES ('p1', 'test', 'onnx_local', 'bge-small', 512)",
            [],
        )
        .unwrap();
    }
}
