use rusqlite::Connection;
use std::path::PathBuf;

/// Open an in-memory SQLite connection with all required schemas applied.
/// Used for testing.
pub fn open_in_memory() -> Result<Connection, String> {
    let conn = Connection::open_in_memory().map_err(|e| format!("打开内存数据库失败: {e}"))?;
    apply_pragmas(&conn)?;
    ensure_all_schemas(&conn)?;
    Ok(conn)
}

/// Open a file-backed SQLite connection with all required schemas applied.
pub fn open_at(path: &PathBuf) -> Result<Connection, String> {
    let conn =
        Connection::open(path).map_err(|e| format!("打开数据库失败 {}: {e}", path.display()))?;
    apply_pragmas(&conn)?;
    ensure_all_schemas(&conn)?;
    Ok(conn)
}

fn apply_pragmas(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("设置 PRAGMA 失败: {e}"))?;
    Ok(())
}

/// Ensure all storage schemas exist. Safe to call multiple times (uses IF NOT EXISTS).
pub fn ensure_all_schemas(conn: &Connection) -> Result<(), String> {
    // Preserve legacy tables that other code still uses.
    ensure_app_state_schema(conn)?;
    ensure_token_usage_schema(conn)?;
    // New structured tables.
    super::chat_history::ensure_schema(conn)?;
    super::workspaces::ensure_schema(conn)?;
    Ok(())
}

fn ensure_app_state_schema(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_state (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| format!("初始化 app_state 失败: {e}"))?;
    Ok(())
}

fn ensure_token_usage_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS token_usage_records (
            turn_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            turn_created_at INTEGER NOT NULL,
            turn_completed_at INTEGER,
            agent_id TEXT,
            agent_name TEXT,
            api TEXT,
            provider TEXT,
            model TEXT,
            response_id TEXT,
            usage_timestamp INTEGER,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0,
            cache_write_tokens INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0,
            recorded_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_token_usage_records_turn_completed_at
            ON token_usage_records(turn_completed_at DESC);
        CREATE INDEX IF NOT EXISTS idx_token_usage_records_model
            ON token_usage_records(model);
        CREATE INDEX IF NOT EXISTS idx_token_usage_records_agent_name
            ON token_usage_records(agent_name);",
    )
    .map_err(|e| format!("初始化用量数据库失败: {e}"))?;
    Ok(())
}

/// Millisecond timestamp since epoch.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}
