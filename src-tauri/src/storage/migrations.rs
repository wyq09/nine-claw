use rusqlite::{params, Connection, OptionalExtension};

/// Migrate data from the legacy `history_v1` JSON blob in `app_state`
/// into the structured `chat_sessions` and `chat_turns` tables.
///
/// This function is idempotent — it tracks a migration flag in `app_state`
/// and will not re-import if already done.
pub fn migrate_history_v1_to_structured(conn: &mut Connection) -> Result<MigrationResult, String> {
    // Check if migration already done.
    let already_done: bool = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = 'history_v1_migrated'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| format!("检查迁移状态失败: {e}"))?
        .map(|v| v == "true")
        .unwrap_or(false);

    if already_done {
        return Ok(MigrationResult::AlreadyDone);
    }

    // Read the history_v1 blob.
    let payload_opt: Option<String> = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = 'history_v1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| format!("读取 history_v1 失败: {e}"))?;

    let payload = match payload_opt {
        Some(p) => p,
        None => {
            // No legacy data — just mark as migrated.
            mark_migrated(conn)?;
            return Ok(MigrationResult::NoDataToMigrate);
        }
    };

    let parsed: serde_json::Value =
        serde_json::from_str(&payload).map_err(|e| format!("解析 history_v1 JSON 失败: {e}"))?;

    let items = parsed
        .as_array()
        .ok_or_else(|| "history_v1 不是 JSON 数组".to_string())?;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("开启迁移事务失败: {e}"))?;

    let mut sessions_count = 0u32;
    let mut turns_count = 0u32;

    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };

        let id = json_string(obj.get("id")).unwrap_or_default();
        if id.is_empty() {
            continue;
        }

        let title = json_string(obj.get("title")).unwrap_or_else(|| "Untitled".to_string());
        let status = json_string(obj.get("status")).unwrap_or_else(|| "done".to_string());
        let created_at = json_i64(obj.get("createdAt")).unwrap_or_else(now_ms);
        let updated_at = json_i64(obj.get("updatedAt")).unwrap_or_else(now_ms);
        let agent_id = json_string(obj.get("agent"))
            .and_then(|a| {
                // agent is a nested object with its own id
                serde_json::from_str::<serde_json::Value>(&format!("\"{a}\""))
                    .ok()
                    .and_then(|_| None) // agent is an object, not a string
            })
            .or_else(|| {
                obj.get("agent")
                    .and_then(|v| v.as_object())
                    .and_then(|a| json_string(a.get("id")))
            });

        let agent_snapshot_json = obj
            .get("agent")
            .filter(|v| v.is_object())
            .map(|v| v.to_string());
        let bot_target_json = obj
            .get("botTarget")
            .filter(|v| v.is_object())
            .map(|v| v.to_string());
        let session_llm_provider_id = json_string(obj.get("sessionLlmProviderId"));
        let session_llm_model = json_string(obj.get("sessionLlmModel"));

        // Insert session (skip if already exists — idempotent).
        let existing = tx
            .query_row(
                "SELECT 1 FROM chat_sessions WHERE id = ?1",
                params![id],
                |row| row.get::<_, i32>(0),
            )
            .optional()
            .map_err(|e| format!("检查会话存在性失败: {e}"))?;

        if existing.is_none() {
            tx.execute(
                "INSERT INTO chat_sessions (id, title, status, created_at, updated_at, agent_id, agent_snapshot_json, bot_target_json, session_llm_provider_id, session_llm_model)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![id, title, status, created_at, updated_at, agent_id, agent_snapshot_json, bot_target_json, session_llm_provider_id, session_llm_model],
            )
            .map_err(|e| format!("迁移会话 {id} 失败: {e}"))?;
            sessions_count += 1;
        }

        // Insert turns.
        if let Some(turns) = obj.get("turns").and_then(|v| v.as_array()) {
            for (idx, turn_val) in turns.iter().enumerate() {
                let Some(turn_obj) = turn_val.as_object() else {
                    continue;
                };

                let turn_id =
                    json_string(turn_obj.get("id")).unwrap_or_else(|| format!("{id}-t{idx}"));
                let prompt = json_string(turn_obj.get("prompt")).unwrap_or_default();
                let answer = json_string(turn_obj.get("answer")).unwrap_or_default();
                let thinking = json_string(turn_obj.get("thinking")).unwrap_or_default();
                let turn_status =
                    json_string(turn_obj.get("status")).unwrap_or_else(|| "done".to_string());
                let turn_created_at = json_i64(turn_obj.get("createdAt")).unwrap_or_else(now_ms);
                let turn_completed_at = json_i64(turn_obj.get("completedAt"));
                let usage_json = turn_obj
                    .get("usage")
                    .filter(|v| !v.is_null())
                    .map(|v| v.to_string());
                let response_segments_json = turn_obj
                    .get("responseSegments")
                    .filter(|v| !v.is_null())
                    .map(|v| v.to_string());
                let tool_calls_json = turn_obj
                    .get("toolCalls")
                    .filter(|v| !v.is_null())
                    .map(|v| v.to_string());
                let activity_json = turn_obj
                    .get("activity")
                    .filter(|v| !v.is_null())
                    .map(|v| v.to_string());

                let existing_turn = tx
                    .query_row(
                        "SELECT 1 FROM chat_turns WHERE id = ?1",
                        params![turn_id],
                        |row| row.get::<_, i32>(0),
                    )
                    .optional()
                    .map_err(|e| format!("检查轮次存在性失败: {e}"))?;

                if existing_turn.is_none() {
                    tx.execute(
                        "INSERT INTO chat_turns (id, session_id, turn_index, prompt, answer, thinking, status, created_at, completed_at, usage_json, response_segments_json, tool_calls_json, activity_json)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            turn_id, id, idx as i32, prompt, answer, thinking,
                            turn_status, turn_created_at, turn_completed_at,
                            usage_json, response_segments_json, tool_calls_json, activity_json
                        ],
                    )
                    .map_err(|e| format!("迁移轮次 {turn_id} 失败: {e}"))?;
                    turns_count += 1;
                }
            }
        }
    }

    mark_migrated(&tx)?;

    tx.commit().map_err(|e| format!("提交迁移事务失败: {e}"))?;

    Ok(MigrationResult::Migrated {
        sessions: sessions_count,
        turns: turns_count,
    })
}

fn mark_migrated(conn: &Connection) -> Result<(), String> {
    let now = crate::storage::db::now_ms();
    conn.execute(
        "INSERT INTO app_state (key, value, updated_at) VALUES ('history_v1_migrated', 'true', ?1)
         ON CONFLICT(key) DO UPDATE SET value = 'true', updated_at = excluded.updated_at",
        params![now],
    )
    .map_err(|e| format!("标记迁移状态失败: {e}"))?;
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum MigrationResult {
    AlreadyDone,
    NoDataToMigrate,
    Migrated { sessions: u32, turns: u32 },
}

fn now_ms() -> i64 {
    crate::storage::db::now_ms()
}

fn json_i64(value: Option<&serde_json::Value>) -> Option<i64> {
    value.and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok()))
    })
}

fn json_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::chat_history::{
        count_chat_sessions, count_chat_turns, list_chat_sessions, list_chat_turns,
    };
    use crate::storage::db::open_in_memory;

    fn seed_history_v1(conn: &Connection, payload: &str) {
        let now = crate::storage::db::now_ms();
        conn.execute(
            "INSERT INTO app_state (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params!["history_v1", payload, now],
        )
        .unwrap();
    }

    #[test]
    fn test_migrate_empty_history_v1() {
        let mut conn = open_in_memory().unwrap();

        let result = migrate_history_v1_to_structured(&mut conn).unwrap();
        assert_eq!(result, MigrationResult::NoDataToMigrate);

        // Should be idempotent.
        let result2 = migrate_history_v1_to_structured(&mut conn).unwrap();
        assert_eq!(result2, MigrationResult::AlreadyDone);
    }

    #[test]
    fn test_migrate_single_session_with_turns() {
        let mut conn = open_in_memory().unwrap();

        let payload = serde_json::json!([
            {
                "id": "session-1",
                "title": "Test Session",
                "status": "done",
                "createdAt": 1700000000000_i64,
                "updatedAt": 1700000001000_i64,
                "agent": {
                    "id": "agent-1",
                    "name": "TestBot",
                    "summary": "A test bot",
                    "description": "Test",
                    "systemPrompt": "",
                    "skillIds": [],
                    "defaultProviderId": "openai",
                    "defaultModel": "gpt-4",
                    "executionMode": "pooled"
                },
                "sessionLlmProviderId": "openai",
                "sessionLlmModel": "gpt-4",
                "turns": [
                    {
                        "id": "turn-1",
                        "prompt": "Hello",
                        "answer": "Hi there!",
                        "thinking": "Let me think...",
                        "status": "done",
                        "createdAt": 1700000000100_i64,
                        "completedAt": 1700000000500_i64,
                        "usage": {
                            "inputTokens": 10,
                            "outputTokens": 20,
                            "totalTokens": 30
                        },
                        "activity": [],
                        "toolCalls": [],
                        "responseSegments": [{"type": "text", "text": "Hi there!"}]
                    },
                    {
                        "id": "turn-2",
                        "prompt": "How are you?",
                        "answer": "I'm doing well!",
                        "status": "done",
                        "createdAt": 1700000000600_i64,
                        "completedAt": 1700000001000_i64,
                        "usage": {
                            "inputTokens": 15,
                            "outputTokens": 25,
                            "totalTokens": 40
                        },
                        "activity": [],
                        "toolCalls": [],
                        "responseSegments": [{"type": "text", "text": "I'm doing well!"}]
                    }
                ]
            }
        ])
        .to_string();

        seed_history_v1(&conn, &payload);

        let result = migrate_history_v1_to_structured(&mut conn).unwrap();
        match result {
            MigrationResult::Migrated { sessions, turns } => {
                assert_eq!(sessions, 1);
                assert_eq!(turns, 2);
            }
            other => panic!("Expected Migrated, got {other:?}"),
        }

        // Verify session data.
        let sessions = list_chat_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "session-1");
        assert_eq!(sessions[0].title, "Test Session");
        assert_eq!(sessions[0].status, "done");
        assert_eq!(sessions[0].agent_id.as_deref(), Some("agent-1"));
        assert_eq!(sessions[0].session_llm_model.as_deref(), Some("gpt-4"));
        assert!(sessions[0].agent_snapshot_json.is_some());

        // Verify turns.
        let turns = list_chat_turns(&conn, "session-1").unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].id, "turn-1");
        assert_eq!(turns[0].prompt, "Hello");
        assert_eq!(turns[0].answer, "Hi there!");
        assert_eq!(turns[0].thinking, "Let me think...");
        assert_eq!(turns[0].status, "done");
        assert_eq!(turns[0].turn_index, 0);
        assert!(turns[0].completed_at.is_some());
        assert!(turns[0].usage_json.is_some());

        assert_eq!(turns[1].id, "turn-2");
        assert_eq!(turns[1].prompt, "How are you?");
        assert_eq!(turns[1].turn_index, 1);
    }

    #[test]
    fn test_migrate_idempotent() {
        let mut conn = open_in_memory().unwrap();

        let payload = serde_json::json!([
            {
                "id": "s1",
                "title": "S1",
                "status": "done",
                "createdAt": 1700000000000_i64,
                "updatedAt": 1700000001000_i64,
                "turns": [
                    {
                        "id": "t1",
                        "prompt": "Hi",
                        "answer": "Hello",
                        "status": "done",
                        "createdAt": 1700000000100_i64,
                        "completedAt": 1700000000500_i64
                    }
                ]
            }
        ])
        .to_string();

        seed_history_v1(&conn, &payload);

        // First migration.
        let r1 = migrate_history_v1_to_structured(&mut conn).unwrap();
        assert!(matches!(
            r1,
            MigrationResult::Migrated {
                sessions: 1,
                turns: 1
            }
        ));

        // Second migration should be no-op.
        let r2 = migrate_history_v1_to_structured(&mut conn).unwrap();
        assert_eq!(r2, MigrationResult::AlreadyDone);

        // Data unchanged.
        assert_eq!(count_chat_sessions(&conn).unwrap(), 1);
        assert_eq!(count_chat_turns(&conn, "s1").unwrap(), 1);
    }

    #[test]
    fn test_migrate_multiple_sessions() {
        let mut conn = open_in_memory().unwrap();

        let payload = serde_json::json!([
            {
                "id": "s1",
                "title": "First",
                "status": "done",
                "createdAt": 1700000000000_i64,
                "updatedAt": 1700000001000_i64,
                "turns": []
            },
            {
                "id": "s2",
                "title": "Second",
                "status": "running",
                "createdAt": 1700000002000_i64,
                "updatedAt": 1700000003000_i64,
                "turns": [
                    {
                        "id": "t-s2-0",
                        "prompt": "Test",
                        "answer": "Reply",
                        "status": "running",
                        "createdAt": 1700000002500_i64
                    }
                ]
            },
            {
                "id": "s3",
                "title": "Third",
                "status": "error",
                "createdAt": 1700000004000_i64,
                "updatedAt": 1700000005000_i64,
                "turns": [
                    {
                        "id": "t-s3-0",
                        "prompt": "Broken",
                        "answer": "",
                        "status": "error",
                        "createdAt": 1700000004500_i64
                    }
                ]
            }
        ])
        .to_string();

        seed_history_v1(&conn, &payload);

        let result = migrate_history_v1_to_structured(&mut conn).unwrap();
        match result {
            MigrationResult::Migrated { sessions, turns } => {
                assert_eq!(sessions, 3);
                assert_eq!(turns, 2);
            }
            other => panic!("Expected Migrated, got {other:?}"),
        }

        let sessions = list_chat_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 3);
        // Ordered by updated_at DESC: s3(5000), s2(3000), s1(1000)
        assert_eq!(sessions[0].id, "s3");
        assert_eq!(sessions[1].id, "s2");
        assert_eq!(sessions[2].id, "s1");
    }

    #[test]
    fn test_migrate_handles_missing_fields_gracefully() {
        let mut conn = open_in_memory().unwrap();

        // Minimal payload with missing optional fields.
        let payload = serde_json::json!([
            {
                "id": "minimal",
                "title": "Minimal",
                "status": "done",
                "turns": [
                    {
                        "id": "t-min",
                        "prompt": "Hi",
                        "status": "done"
                    }
                ]
            }
        ])
        .to_string();

        seed_history_v1(&conn, &payload);

        let result = migrate_history_v1_to_structured(&mut conn).unwrap();
        assert!(matches!(result, MigrationResult::Migrated { .. }));

        let turns = list_chat_turns(&conn, "minimal").unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].answer, ""); // default
        assert!(turns[0].completed_at.is_none()); // missing
    }

    #[test]
    fn test_migrate_skips_invalid_entries() {
        let mut conn = open_in_memory().unwrap();

        let payload = serde_json::json!([
            "not an object",
            {"no_id": true},
            {"id": "", "title": "Empty ID"},
            {
                "id": "valid",
                "title": "Valid",
                "status": "done",
                "turns": []
            }
        ])
        .to_string();

        seed_history_v1(&conn, &payload);

        let result = migrate_history_v1_to_structured(&mut conn).unwrap();
        match result {
            MigrationResult::Migrated { sessions, .. } => {
                assert_eq!(sessions, 1); // Only "valid" should be imported
            }
            other => panic!("Expected Migrated, got {other:?}"),
        }
    }

    #[test]
    fn test_migrate_with_bot_target() {
        let mut conn = open_in_memory().unwrap();

        let payload = serde_json::json!([
            {
                "id": "bot-session",
                "title": "Bot Chat",
                "status": "done",
                "turns": [],
                "botTarget": {
                    "channelId": "ch-1",
                    "userId": "user-1"
                }
            }
        ])
        .to_string();

        seed_history_v1(&conn, &payload);

        migrate_history_v1_to_structured(&mut conn).unwrap();

        let sessions = list_chat_sessions(&conn).unwrap();
        assert_eq!(
            sessions[0].bot_target_json.as_deref(),
            Some(r#"{"channelId":"ch-1","userId":"user-1"}"#)
        );
    }
}
