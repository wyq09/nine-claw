use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Ensure chat_sessions and chat_turns tables exist.
pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS chat_sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            agent_id TEXT,
            agent_snapshot_json TEXT,
            bot_target_json TEXT,
            session_llm_provider_id TEXT,
            session_llm_model TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated_at
            ON chat_sessions(updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_chat_sessions_agent_id
            ON chat_sessions(agent_id);

        CREATE TABLE IF NOT EXISTS chat_turns (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            turn_index INTEGER NOT NULL,
            prompt TEXT NOT NULL,
            answer TEXT NOT NULL DEFAULT '',
            thinking TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            completed_at INTEGER,
            usage_json TEXT,
            response_segments_json TEXT,
            tool_calls_json TEXT,
            activity_json TEXT,
            FOREIGN KEY(session_id) REFERENCES chat_sessions(id),
            UNIQUE(session_id, turn_index)
        );
        CREATE INDEX IF NOT EXISTS idx_chat_turns_session_id_turn_index
            ON chat_turns(session_id, turn_index);
        CREATE INDEX IF NOT EXISTS idx_chat_turns_created_at
            ON chat_turns(created_at);",
    )
    .map_err(|e| format!("初始化聊天表失败: {e}"))?;
    Ok(())
}

// ── Data types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub agent_id: Option<String>,
    pub agent_snapshot_json: Option<String>,
    pub bot_target_json: Option<String>,
    pub session_llm_provider_id: Option<String>,
    pub session_llm_model: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurn {
    pub id: String,
    pub session_id: String,
    pub turn_index: i32,
    pub prompt: String,
    pub answer: String,
    pub thinking: String,
    pub status: String,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub usage_json: Option<String>,
    pub response_segments_json: Option<String>,
    pub tool_calls_json: Option<String>,
    pub activity_json: Option<String>,
    #[serde(default)]
    pub speaker_agent_id: Option<String>,
}

/// Input for creating a new chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChatSessionInput {
    pub id: String,
    pub title: String,
    pub status: String,
    pub agent_id: Option<String>,
    pub agent_snapshot_json: Option<String>,
    pub bot_target_json: Option<String>,
    pub session_llm_provider_id: Option<String>,
    pub session_llm_model: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
}

/// Input for appending a new turn to a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendChatTurnInput {
    pub id: String,
    pub session_id: String,
    pub turn_index: i32,
    pub prompt: String,
    pub answer: String,
    pub thinking: String,
    pub status: String,
    pub usage_json: Option<String>,
    pub response_segments_json: Option<String>,
    pub tool_calls_json: Option<String>,
    pub activity_json: Option<String>,
    #[serde(default)]
    pub speaker_agent_id: Option<String>,
}

/// Input for updating an existing turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChatTurnInput {
    pub id: String,
    pub answer: Option<String>,
    pub thinking: Option<String>,
    pub status: Option<String>,
    pub completed_at: Option<i64>,
    pub usage_json: Option<String>,
    pub response_segments_json: Option<String>,
    pub tool_calls_json: Option<String>,
    pub activity_json: Option<String>,
}

// ── CRUD operations ───────────────────────────────────────────────────

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn row_to_chat_session(row: &rusqlite::Row) -> rusqlite::Result<ChatSession> {
    Ok(ChatSession {
        id: row.get("id")?,
        title: row.get("title")?,
        status: row.get("status")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        agent_id: row.get("agent_id")?,
        agent_snapshot_json: row.get("agent_snapshot_json")?,
        bot_target_json: row.get("bot_target_json")?,
        session_llm_provider_id: row.get("session_llm_provider_id")?,
        session_llm_model: row.get("session_llm_model")?,
        workspace_id: row.get::<_, Option<String>>("workspace_id")?,
    })
}

fn row_to_chat_turn(row: &rusqlite::Row) -> rusqlite::Result<ChatTurn> {
    Ok(ChatTurn {
        id: row.get("id")?,
        session_id: row.get("session_id")?,
        turn_index: row.get("turn_index")?,
        prompt: row.get("prompt")?,
        answer: row.get("answer")?,
        thinking: row.get("thinking")?,
        status: row.get("status")?,
        created_at: row.get("created_at")?,
        completed_at: row.get("completed_at")?,
        usage_json: row.get("usage_json")?,
        response_segments_json: row.get("response_segments_json")?,
        tool_calls_json: row.get("tool_calls_json")?,
        activity_json: row.get("activity_json")?,
        speaker_agent_id: row.get::<_, Option<String>>("speaker_agent_id")?,
    })
}

/// Create a new chat session.
pub fn create_chat_session(
    conn: &Connection,
    input: &CreateChatSessionInput,
) -> Result<ChatSession, String> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO chat_sessions (id, title, status, created_at, updated_at, agent_id, agent_snapshot_json, bot_target_json, session_llm_provider_id, session_llm_model, workspace_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            input.id,
            input.title,
            input.status,
            now,
            now,
            input.agent_id,
            input.agent_snapshot_json,
            input.bot_target_json,
            input.session_llm_provider_id,
            input.session_llm_model,
            input.workspace_id,
        ],
    )
    .map_err(|e| format!("创建聊天会话失败: {e}"))?;

    get_chat_session(conn, &input.id)?.ok_or_else(|| "刚创建的会话查询不到".to_string())
}

/// List all chat sessions ordered by updated_at DESC.
pub fn list_chat_sessions(conn: &Connection) -> Result<Vec<ChatSession>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, status, created_at, updated_at, agent_id, agent_snapshot_json, bot_target_json, session_llm_provider_id, session_llm_model, workspace_id
             FROM chat_sessions ORDER BY updated_at DESC",
        )
        .map_err(|e| format!("准备查询失败: {e}"))?;

    let rows = stmt
        .query_map([], row_to_chat_session)
        .map_err(|e| format!("查询会话列表失败: {e}"))?;

    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row.map_err(|e| format!("读取会话行失败: {e}"))?);
    }
    Ok(sessions)
}

/// Get a single chat session by ID.
pub fn get_chat_session(conn: &Connection, id: &str) -> Result<Option<ChatSession>, String> {
    conn.query_row(
        "SELECT id, title, status, created_at, updated_at, agent_id, agent_snapshot_json, bot_target_json, session_llm_provider_id, session_llm_model, workspace_id
         FROM chat_sessions WHERE id = ?1",
        params![id],
        row_to_chat_session,
    )
    .optional()
    .map_err(|e| format!("查询会话失败: {e}"))
}

/// Append a new turn to a session. Also bumps the session's updated_at.
pub fn append_chat_turn(
    conn: &Connection,
    input: &AppendChatTurnInput,
) -> Result<ChatTurn, String> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO chat_turns (id, session_id, turn_index, prompt, answer, thinking, status, created_at, completed_at, usage_json, response_segments_json, tool_calls_json, activity_json, speaker_agent_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            input.id,
            input.session_id,
            input.turn_index,
            input.prompt,
            input.answer,
            input.thinking,
            input.status,
            now,
            // completed_at: initially null for running turns
            Option::<i64>::None,
            input.usage_json,
            input.response_segments_json,
            input.tool_calls_json,
            input.activity_json,
            input.speaker_agent_id,
        ],
    )
    .map_err(|e| format!("追加聊天轮次失败: {e}"))?;

    // Bump session updated_at.
    conn.execute(
        "UPDATE chat_sessions SET updated_at = ?1 WHERE id = ?2",
        params![now, input.session_id],
    )
    .map_err(|e| format!("更新会话时间戳失败: {e}"))?;

    get_chat_turn(conn, &input.id)?.ok_or_else(|| "刚创建的轮次查询不到".to_string())
}

/// Get a single chat turn by ID.
pub fn get_chat_turn(conn: &Connection, id: &str) -> Result<Option<ChatTurn>, String> {
    conn.query_row(
        "SELECT id, session_id, turn_index, prompt, answer, thinking, status, created_at, completed_at, usage_json, response_segments_json, tool_calls_json, activity_json, speaker_agent_id
         FROM chat_turns WHERE id = ?1",
        params![id],
        row_to_chat_turn,
    )
    .optional()
    .map_err(|e| format!("查询轮次失败: {e}"))
}

/// List all turns for a session, ordered by turn_index ASC.
pub fn list_chat_turns(conn: &Connection, session_id: &str) -> Result<Vec<ChatTurn>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, turn_index, prompt, answer, thinking, status, created_at, completed_at, usage_json, response_segments_json, tool_calls_json, activity_json, speaker_agent_id
             FROM chat_turns WHERE session_id = ?1 ORDER BY turn_index ASC",
        )
        .map_err(|e| format!("准备查询失败: {e}"))?;

    let rows = stmt
        .query_map(params![session_id], row_to_chat_turn)
        .map_err(|e| format!("查询轮次列表失败: {e}"))?;

    let mut turns = Vec::new();
    for row in rows {
        turns.push(row.map_err(|e| format!("读取轮次行失败: {e}"))?);
    }
    Ok(turns)
}

/// Update an existing turn's mutable fields.
pub fn update_chat_turn(
    conn: &Connection,
    input: &UpdateChatTurnInput,
) -> Result<ChatTurn, String> {
    let existing =
        get_chat_turn(conn, &input.id)?.ok_or_else(|| format!("轮次 {} 不存在", input.id))?;

    let answer = input.answer.as_ref().unwrap_or(&existing.answer);
    let thinking = input.thinking.as_ref().unwrap_or(&existing.thinking);
    let status = input.status.as_ref().unwrap_or(&existing.status);
    let completed_at = input.completed_at.or(existing.completed_at);
    let usage_json = input
        .usage_json
        .as_ref()
        .or(existing.usage_json.as_ref())
        .cloned();
    let response_segments_json = input
        .response_segments_json
        .as_ref()
        .or(existing.response_segments_json.as_ref())
        .cloned();
    let tool_calls_json = input
        .tool_calls_json
        .as_ref()
        .or(existing.tool_calls_json.as_ref())
        .cloned();
    let activity_json = input
        .activity_json
        .as_ref()
        .or(existing.activity_json.as_ref())
        .cloned();

    conn.execute(
        "UPDATE chat_turns SET answer = ?1, thinking = ?2, status = ?3, completed_at = ?4, usage_json = ?5, response_segments_json = ?6, tool_calls_json = ?7, activity_json = ?8
         WHERE id = ?9",
        params![answer, thinking, status, completed_at, usage_json, response_segments_json, tool_calls_json, activity_json, input.id],
    )
    .map_err(|e| format!("更新轮次失败: {e}"))?;

    // Also bump session updated_at.
    conn.execute(
        "UPDATE chat_sessions SET updated_at = ?1 WHERE id = (SELECT session_id FROM chat_turns WHERE id = ?2)",
        params![now_ms(), input.id],
    )
    .map_err(|e| format!("更新会话时间戳失败: {e}"))?;

    get_chat_turn(conn, &input.id)?.ok_or_else(|| "刚更新的轮次查询不到".to_string())
}

/// Delete a chat session and all its turns (cascading).
pub fn delete_chat_session(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM chat_turns WHERE session_id = ?1", params![id])
        .map_err(|e| format!("删除会话轮次失败: {e}"))?;
    conn.execute("DELETE FROM chat_sessions WHERE id = ?1", params![id])
        .map_err(|e| format!("删除会话失败: {e}"))?;
    Ok(())
}

/// Clear all chat sessions and turns.
pub fn clear_all_chat_sessions(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM chat_turns", [])
        .map_err(|e| format!("清空轮次失败: {e}"))?;
    conn.execute("DELETE FROM chat_sessions", [])
        .map_err(|e| format!("清空会话失败: {e}"))?;
    Ok(())
}

/// Count chat sessions.
pub fn count_chat_sessions(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM chat_sessions", [], |row| row.get(0))
        .map_err(|e| format!("计数会话失败: {e}"))
}

/// Count turns in a session.
pub fn count_chat_turns(conn: &Connection, session_id: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM chat_turns WHERE session_id = ?1",
        params![session_id],
        |row| row.get(0),
    )
    .map_err(|e| format!("计数轮次失败: {e}"))
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_in_memory;

    fn make_session_input(id: &str) -> CreateChatSessionInput {
        CreateChatSessionInput {
            id: id.to_string(),
            title: format!("Session {id}"),
            status: "running".to_string(),
            agent_id: None,
            agent_snapshot_json: None,
            bot_target_json: None,
            session_llm_provider_id: None,
            session_llm_model: None,
            workspace_id: None,
        }
    }

    fn make_turn_input(session_id: &str, turn_index: i32, id: &str) -> AppendChatTurnInput {
        AppendChatTurnInput {
            id: id.to_string(),
            session_id: session_id.to_string(),
            turn_index,
            prompt: format!("Prompt {turn_index}"),
            answer: String::new(),
            thinking: String::new(),
            status: "running".to_string(),
            usage_json: None,
            response_segments_json: None,
            tool_calls_json: None,
            activity_json: None,
            speaker_agent_id: None,
        }
    }

    #[test]
    fn test_create_and_get_session() {
        let conn = open_in_memory().unwrap();
        let input = make_session_input("s1");
        let session = create_chat_session(&conn, &input).unwrap();
        assert_eq!(session.id, "s1");
        assert_eq!(session.title, "Session s1");
        assert_eq!(session.status, "running");

        let fetched = get_chat_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(fetched.id, "s1");
        assert_eq!(fetched.title, "Session s1");
    }

    #[test]
    fn test_get_session_not_found() {
        let conn = open_in_memory().unwrap();
        let result = get_chat_session(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_sessions_ordered_by_updated_at() {
        let conn = open_in_memory().unwrap();

        let _s1 = create_chat_session(&conn, &make_session_input("s1")).unwrap();

        // Ensure different timestamps by bumping s1's updated_at explicitly.
        std::thread::sleep(std::time::Duration::from_millis(10));
        conn.execute(
            "UPDATE chat_sessions SET updated_at = ?1 WHERE id = 's1'",
            params![now_ms()],
        )
        .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        let _s2 = create_chat_session(&conn, &make_session_input("s2")).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        let _s3 = create_chat_session(&conn, &make_session_input("s3")).unwrap();

        let sessions = list_chat_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 3);
        // Most recently created should be first.
        assert_eq!(sessions[0].id, "s3");
        assert_eq!(sessions[1].id, "s2");
        assert_eq!(sessions[2].id, "s1");

        // Bump s1 to make it most recent.
        std::thread::sleep(std::time::Duration::from_millis(10));
        conn.execute(
            "UPDATE chat_sessions SET updated_at = ?1 WHERE id = 's1'",
            params![now_ms()],
        )
        .unwrap();

        let sessions = list_chat_sessions(&conn).unwrap();
        assert_eq!(sessions[0].id, "s1"); // now s1 is most recent
    }

    #[test]
    fn test_append_turn_and_list() {
        let conn = open_in_memory().unwrap();
        create_chat_session(&conn, &make_session_input("s1")).unwrap();

        let t1 = append_chat_turn(&conn, &make_turn_input("s1", 0, "t1")).unwrap();
        assert_eq!(t1.session_id, "s1");
        assert_eq!(t1.turn_index, 0);
        assert_eq!(t1.prompt, "Prompt 0");
        assert_eq!(t1.status, "running");
        assert!(t1.completed_at.is_none());

        let t2 = append_chat_turn(&conn, &make_turn_input("s1", 1, "t2")).unwrap();
        assert_eq!(t2.turn_index, 1);

        let turns = list_chat_turns(&conn, "s1").unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].turn_index, 0);
        assert_eq!(turns[1].turn_index, 1);
    }

    #[test]
    fn test_update_turn() {
        let conn = open_in_memory().unwrap();
        create_chat_session(&conn, &make_session_input("s1")).unwrap();
        append_chat_turn(&conn, &make_turn_input("s1", 0, "t1")).unwrap();

        let updated = update_chat_turn(
            &conn,
            &UpdateChatTurnInput {
                id: "t1".to_string(),
                answer: Some("Final answer".to_string()),
                thinking: Some("Thought process".to_string()),
                status: Some("done".to_string()),
                completed_at: Some(now_ms()),
                usage_json: Some(r#"{"inputTokens":10,"outputTokens":20}"#.to_string()),
                response_segments_json: None,
                tool_calls_json: None,
                activity_json: None,
            },
        )
        .unwrap();

        assert_eq!(updated.answer, "Final answer");
        assert_eq!(updated.thinking, "Thought process");
        assert_eq!(updated.status, "done");
        assert!(updated.completed_at.is_some());
        assert!(updated.usage_json.is_some());
    }

    #[test]
    fn test_update_turn_partial() {
        let conn = open_in_memory().unwrap();
        create_chat_session(&conn, &make_session_input("s1")).unwrap();
        append_chat_turn(&conn, &make_turn_input("s1", 0, "t1")).unwrap();

        // Only update answer, leave everything else as-is
        let updated = update_chat_turn(
            &conn,
            &UpdateChatTurnInput {
                id: "t1".to_string(),
                answer: Some("Partial update".to_string()),
                thinking: None,
                status: None,
                completed_at: None,
                usage_json: None,
                response_segments_json: None,
                tool_calls_json: None,
                activity_json: None,
            },
        )
        .unwrap();

        assert_eq!(updated.answer, "Partial update");
        assert_eq!(updated.prompt, "Prompt 0"); // unchanged
        assert_eq!(updated.status, "running"); // unchanged
    }

    #[test]
    fn test_update_turn_not_found() {
        let conn = open_in_memory().unwrap();
        let result = update_chat_turn(
            &conn,
            &UpdateChatTurnInput {
                id: "nonexistent".to_string(),
                answer: Some("x".to_string()),
                thinking: None,
                status: None,
                completed_at: None,
                usage_json: None,
                response_segments_json: None,
                tool_calls_json: None,
                activity_json: None,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_session_cascades_turns() {
        let conn = open_in_memory().unwrap();
        create_chat_session(&conn, &make_session_input("s1")).unwrap();
        append_chat_turn(&conn, &make_turn_input("s1", 0, "t1")).unwrap();
        append_chat_turn(&conn, &make_turn_input("s1", 1, "t2")).unwrap();

        assert_eq!(count_chat_turns(&conn, "s1").unwrap(), 2);

        delete_chat_session(&conn, "s1").unwrap();

        assert!(get_chat_session(&conn, "s1").unwrap().is_none());
        assert_eq!(count_chat_turns(&conn, "s1").unwrap(), 0);
    }

    #[test]
    fn test_clear_all_sessions() {
        let conn = open_in_memory().unwrap();
        create_chat_session(&conn, &make_session_input("s1")).unwrap();
        create_chat_session(&conn, &make_session_input("s2")).unwrap();
        append_chat_turn(&conn, &make_turn_input("s1", 0, "t1")).unwrap();

        assert_eq!(count_chat_sessions(&conn).unwrap(), 2);

        clear_all_chat_sessions(&conn).unwrap();

        assert_eq!(count_chat_sessions(&conn).unwrap(), 0);
        assert_eq!(count_chat_turns(&conn, "s1").unwrap(), 0);
    }

    #[test]
    fn test_append_turn_updates_session_timestamp() {
        let conn = open_in_memory().unwrap();
        let session = create_chat_session(&conn, &make_session_input("s1")).unwrap();
        let original_updated_at = session.updated_at;

        // Small sleep to ensure timestamp differs
        std::thread::sleep(std::time::Duration::from_millis(10));

        append_chat_turn(&conn, &make_turn_input("s1", 0, "t1")).unwrap();

        let updated_session = get_chat_session(&conn, "s1").unwrap().unwrap();
        assert!(updated_session.updated_at > original_updated_at);
    }

    #[test]
    fn test_duplicate_session_id_rejected() {
        let conn = open_in_memory().unwrap();
        create_chat_session(&conn, &make_session_input("s1")).unwrap();
        let result = create_chat_session(&conn, &make_session_input("s1"));
        assert!(result.is_err());
    }

    #[test]
    fn test_duplicate_turn_index_rejected() {
        let conn = open_in_memory().unwrap();
        create_chat_session(&conn, &make_session_input("s1")).unwrap();
        append_chat_turn(&conn, &make_turn_input("s1", 0, "t1")).unwrap();
        // Same session_id + turn_index should fail
        let result = append_chat_turn(&conn, &make_turn_input("s1", 0, "t2"));
        assert!(result.is_err());
    }

    #[test]
    fn test_session_with_agent_and_llm_fields() {
        let conn = open_in_memory().unwrap();
        let input = CreateChatSessionInput {
            id: "s-agent".to_string(),
            title: "Agent session".to_string(),
            status: "running".to_string(),
            agent_id: Some("agent-123".to_string()),
            agent_snapshot_json: Some(r#"{"id":"agent-123","name":"TestBot"}"#.to_string()),
            bot_target_json: None,
            session_llm_provider_id: Some("openai".to_string()),
            session_llm_model: Some("gpt-4".to_string()),
            workspace_id: None,
        };

        let session = create_chat_session(&conn, &input).unwrap();
        assert_eq!(session.agent_id.as_deref(), Some("agent-123"));
        assert_eq!(session.session_llm_model.as_deref(), Some("gpt-4"));
        assert!(session.agent_snapshot_json.is_some());
    }

    #[test]
    fn test_list_sessions_empty() {
        let conn = open_in_memory().unwrap();
        let sessions = list_chat_sessions(&conn).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_list_turns_empty() {
        let conn = open_in_memory().unwrap();
        create_chat_session(&conn, &make_session_input("s1")).unwrap();
        let turns = list_chat_turns(&conn, "s1").unwrap();
        assert!(turns.is_empty());
    }

    #[test]
    fn test_update_turn_bumps_session_timestamp() {
        let conn = open_in_memory().unwrap();
        let session = create_chat_session(&conn, &make_session_input("s1")).unwrap();
        append_chat_turn(&conn, &make_turn_input("s1", 0, "t1")).unwrap();

        let after_append = get_chat_session(&conn, "s1").unwrap().unwrap().updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));

        update_chat_turn(
            &conn,
            &UpdateChatTurnInput {
                id: "t1".to_string(),
                answer: Some("Updated".to_string()),
                thinking: None,
                status: Some("done".to_string()),
                completed_at: Some(now_ms()),
                usage_json: None,
                response_segments_json: None,
                tool_calls_json: None,
                activity_json: None,
            },
        )
        .unwrap();

        let after_update = get_chat_session(&conn, "s1").unwrap().unwrap().updated_at;
        assert!(after_update > after_append);
    }
}
