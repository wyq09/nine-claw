use super::chat_history::{
    append_chat_turn, clear_all_chat_sessions, create_chat_session, get_chat_session,
    get_chat_turn, list_chat_sessions, list_chat_turns, update_chat_turn, AppendChatTurnInput,
    CreateChatSessionInput, UpdateChatTurnInput,
};
use rusqlite::Connection;
use serde_json::{json, Value};

fn now_ms() -> i64 {
    crate::storage::db::now_ms()
}

fn value_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn value_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok()))
    })
}

pub fn export_history_snapshot_json(conn: &Connection) -> Result<String, String> {
    let sessions = list_chat_sessions(conn)?;
    let mut out = Vec::with_capacity(sessions.len());

    for session in sessions {
        let turns = list_chat_turns(conn, &session.id)?;
        out.push(json!({
            "id": session.id,
            "title": session.title,
            "status": session.status,
            "createdAt": session.created_at,
            "updatedAt": session.updated_at,
            "agent": session.agent_snapshot_json.as_deref().and_then(|v| serde_json::from_str::<Value>(v).ok()),
            "botTarget": session.bot_target_json.as_deref().and_then(|v| serde_json::from_str::<Value>(v).ok()),
            "sessionLlmProviderId": session.session_llm_provider_id,
            "sessionLlmModel": session.session_llm_model,
            "workspaceId": session.workspace_id,
            "topicWorkspaceDir": session.topic_workspace_dir,
            "currentWorkspaceDir": session.current_workspace_dir,
            "turns": turns.into_iter().map(|turn| {
                json!({
                    "id": turn.id,
                    "prompt": turn.prompt,
                    "answer": turn.answer,
                    "thinking": turn.thinking,
                    "status": turn.status,
                    "createdAt": turn.created_at,
                    "completedAt": turn.completed_at,
                    "usage": turn.usage_json.as_deref().and_then(|v| serde_json::from_str::<Value>(v).ok()),
                    "responseSegments": turn.response_segments_json.as_deref().and_then(|v| serde_json::from_str::<Value>(v).ok()),
                    "toolCalls": turn.tool_calls_json.as_deref().and_then(|v| serde_json::from_str::<Value>(v).ok()).unwrap_or_else(|| Value::Array(Vec::new())),
                    "activity": turn.activity_json.as_deref().and_then(|v| serde_json::from_str::<Value>(v).ok()).unwrap_or_else(|| Value::Array(Vec::new())),
                    "speakerAgentId": turn.speaker_agent_id,
                })
            }).collect::<Vec<_>>(),
        }));
    }

    serde_json::to_string(&out).map_err(|e| format!("序列化聊天快照失败: {e}"))
}

pub fn session_count_from_snapshot_json(payload: &str) -> Result<i64, String> {
    let parsed: Value =
        serde_json::from_str(payload).map_err(|e| format!("解析聊天快照失败: {e}"))?;
    let items = parsed
        .as_array()
        .ok_or_else(|| "聊天快照必须是数组".to_string())?;
    Ok(items.len() as i64)
}

pub fn replace_history_snapshot_json(conn: &Connection, payload: &str) -> Result<(), String> {
    let parsed: Value =
        serde_json::from_str(payload).map_err(|e| format!("解析聊天快照失败: {e}"))?;
    let items = parsed
        .as_array()
        .ok_or_else(|| "聊天快照必须是数组".to_string())?;

    clear_all_chat_sessions(conn)?;

    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };

        let Some(session_id) = value_string(obj.get("id")) else {
            continue;
        };
        let title = value_string(obj.get("title")).unwrap_or_else(|| "Untitled".to_string());
        let status = value_string(obj.get("status")).unwrap_or_else(|| "done".to_string());
        let created_at = value_i64(obj.get("createdAt")).unwrap_or_else(now_ms);
        let updated_at = value_i64(obj.get("updatedAt")).unwrap_or(created_at);
        let agent_snapshot_json = obj.get("agent").map(Value::to_string);
        let bot_target_json = obj.get("botTarget").map(Value::to_string);
        let agent_id = obj
            .get("agent")
            .and_then(Value::as_object)
            .and_then(|agent| value_string(agent.get("id")));
        let topic_workspace_dir = value_string(obj.get("topicWorkspaceDir"));
        let current_workspace_dir = value_string(obj.get("currentWorkspaceDir"));

        create_chat_session(
            conn,
            &CreateChatSessionInput {
                id: session_id.clone(),
                title,
                status,
                agent_id,
                agent_snapshot_json,
                bot_target_json,
                session_llm_provider_id: value_string(obj.get("sessionLlmProviderId")),
                session_llm_model: value_string(obj.get("sessionLlmModel")),
                workspace_id: value_string(obj.get("workspaceId")),
            },
        )?;
        conn.execute(
            "UPDATE chat_sessions
             SET created_at = ?1,
                 updated_at = ?2,
                 topic_workspace_dir = ?3,
                 current_workspace_dir = ?4
             WHERE id = ?5",
            rusqlite::params![
                created_at,
                updated_at,
                topic_workspace_dir,
                current_workspace_dir,
                session_id
            ],
        )
        .map_err(|e| format!("回填会话时间失败: {e}"))?;

        if let Some(turns) = obj.get("turns").and_then(Value::as_array) {
            for (index, turn) in turns.iter().enumerate() {
                let Some(turn_obj) = turn.as_object() else {
                    continue;
                };
                let turn_id = value_string(turn_obj.get("id"))
                    .unwrap_or_else(|| format!("{session_id}-turn-{index}"));
                let turn_created_at = value_i64(turn_obj.get("createdAt")).unwrap_or_else(now_ms);
                let completed_at = value_i64(turn_obj.get("completedAt"));
                let answer = value_string(turn_obj.get("answer")).unwrap_or_default();
                let thinking = value_string(turn_obj.get("thinking")).unwrap_or_default();
                let status =
                    value_string(turn_obj.get("status")).unwrap_or_else(|| "done".to_string());

                append_chat_turn(
                    conn,
                    &AppendChatTurnInput {
                        id: turn_id.clone(),
                        session_id: session_id.clone(),
                        turn_index: index as i32,
                        prompt: value_string(turn_obj.get("prompt")).unwrap_or_default(),
                        answer: answer.clone(),
                        thinking: thinking.clone(),
                        status: status.clone(),
                        usage_json: turn_obj.get("usage").map(Value::to_string),
                        response_segments_json: turn_obj
                            .get("responseSegments")
                            .map(Value::to_string),
                        tool_calls_json: turn_obj.get("toolCalls").map(Value::to_string),
                        activity_json: turn_obj.get("activity").map(Value::to_string),
                        speaker_agent_id: value_string(turn_obj.get("speakerAgentId")),
                    },
                )?;
                conn.execute(
                    "UPDATE chat_turns SET created_at = ?1 WHERE id = ?2",
                    rusqlite::params![turn_created_at, turn_id],
                )
                .map_err(|e| format!("回填轮次创建时间失败: {e}"))?;
                let _ = update_chat_turn(
                    conn,
                    &UpdateChatTurnInput {
                        id: turn_id,
                        answer: Some(answer),
                        thinking: Some(thinking),
                        status: Some(status),
                        completed_at,
                        usage_json: turn_obj.get("usage").map(Value::to_string),
                        response_segments_json: turn_obj
                            .get("responseSegments")
                            .map(Value::to_string),
                        tool_calls_json: turn_obj.get("toolCalls").map(Value::to_string),
                        activity_json: turn_obj.get("activity").map(Value::to_string),
                    },
                )?;
            }
        }

        // append_chat_turn 会把 updated_at 覆盖为 now()，此处还原为原始值
        conn.execute(
            "UPDATE chat_sessions SET updated_at = ?1 WHERE id = ?2",
            rusqlite::params![updated_at, session_id],
        )
        .map_err(|e| format!("还原会话 updated_at 失败: {e}"))?;
    }

    Ok(())
}

pub fn merge_history_snapshot_json(conn: &Connection, payload: &str) -> Result<(), String> {
    let parsed: Value =
        serde_json::from_str(payload).map_err(|e| format!("解析聊天快照失败: {e}"))?;
    let items = parsed
        .as_array()
        .ok_or_else(|| "聊天快照必须是数组".to_string())?;

    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };

        let Some(session_id) = value_string(obj.get("id")) else {
            continue;
        };
        let title = value_string(obj.get("title")).unwrap_or_else(|| "Untitled".to_string());
        let status = value_string(obj.get("status")).unwrap_or_else(|| "done".to_string());
        let created_at = value_i64(obj.get("createdAt")).unwrap_or_else(now_ms);
        let updated_at = value_i64(obj.get("updatedAt")).unwrap_or(created_at);
        let agent_snapshot_json = obj.get("agent").map(Value::to_string);
        let bot_target_json = obj.get("botTarget").map(Value::to_string);
        let agent_id = obj
            .get("agent")
            .and_then(Value::as_object)
            .and_then(|agent| value_string(agent.get("id")));
        let session_llm_provider_id = value_string(obj.get("sessionLlmProviderId"));
        let session_llm_model = value_string(obj.get("sessionLlmModel"));
        let workspace_id = value_string(obj.get("workspaceId"));
        let topic_workspace_dir = value_string(obj.get("topicWorkspaceDir"));
        let current_workspace_dir = value_string(obj.get("currentWorkspaceDir"));

        if get_chat_session(conn, &session_id)?.is_some() {
            conn.execute(
                "UPDATE chat_sessions
                 SET title = ?1,
                     status = ?2,
                     created_at = ?3,
                     updated_at = ?4,
                     agent_id = ?5,
                     agent_snapshot_json = ?6,
                     bot_target_json = ?7,
                     session_llm_provider_id = ?8,
                     session_llm_model = ?9,
                     workspace_id = ?10,
                     topic_workspace_dir = ?11,
                     current_workspace_dir = ?12
                 WHERE id = ?13",
                rusqlite::params![
                    title,
                    status,
                    created_at,
                    updated_at,
                    agent_id,
                    agent_snapshot_json,
                    bot_target_json,
                    session_llm_provider_id,
                    session_llm_model,
                    workspace_id,
                    topic_workspace_dir,
                    current_workspace_dir,
                    session_id,
                ],
            )
            .map_err(|e| format!("更新聊天会话失败: {e}"))?;
        } else {
            create_chat_session(
                conn,
                &CreateChatSessionInput {
                    id: session_id.clone(),
                    title,
                    status,
                    agent_id,
                    agent_snapshot_json,
                    bot_target_json,
                    session_llm_provider_id,
                    session_llm_model,
                    workspace_id,
                },
            )?;
            conn.execute(
                "UPDATE chat_sessions
                 SET created_at = ?1,
                     updated_at = ?2,
                     topic_workspace_dir = ?3,
                     current_workspace_dir = ?4
                 WHERE id = ?5",
                rusqlite::params![
                    created_at,
                    updated_at,
                    topic_workspace_dir,
                    current_workspace_dir,
                    session_id
                ],
            )
            .map_err(|e| format!("回填会话时间失败: {e}"))?;
        }

        if let Some(turns) = obj.get("turns").and_then(Value::as_array) {
            for (index, turn) in turns.iter().enumerate() {
                let Some(turn_obj) = turn.as_object() else {
                    continue;
                };
                let turn_id = value_string(turn_obj.get("id"))
                    .unwrap_or_else(|| format!("{session_id}-turn-{index}"));
                let turn_created_at = value_i64(turn_obj.get("createdAt")).unwrap_or_else(now_ms);
                let completed_at = value_i64(turn_obj.get("completedAt"));
                let prompt = value_string(turn_obj.get("prompt")).unwrap_or_default();
                let answer = value_string(turn_obj.get("answer")).unwrap_or_default();
                let thinking = value_string(turn_obj.get("thinking")).unwrap_or_default();
                let turn_status =
                    value_string(turn_obj.get("status")).unwrap_or_else(|| "done".to_string());
                let usage_json = turn_obj.get("usage").map(Value::to_string);
                let response_segments_json = turn_obj.get("responseSegments").map(Value::to_string);
                let tool_calls_json = turn_obj.get("toolCalls").map(Value::to_string);
                let activity_json = turn_obj.get("activity").map(Value::to_string);
                let speaker_agent_id = value_string(turn_obj.get("speakerAgentId"));

                if get_chat_turn(conn, &turn_id)?.is_some() {
                    conn.execute(
                        "UPDATE chat_turns
                         SET session_id = ?1,
                             turn_index = ?2,
                             prompt = ?3,
                             answer = ?4,
                             thinking = ?5,
                             status = ?6,
                             created_at = ?7,
                             completed_at = ?8,
                             usage_json = ?9,
                             response_segments_json = ?10,
                             tool_calls_json = ?11,
                             activity_json = ?12,
                             speaker_agent_id = ?13
                         WHERE id = ?14",
                        rusqlite::params![
                            session_id,
                            index as i32,
                            prompt,
                            answer,
                            thinking,
                            turn_status,
                            turn_created_at,
                            completed_at,
                            usage_json,
                            response_segments_json,
                            tool_calls_json,
                            activity_json,
                            speaker_agent_id,
                            turn_id,
                        ],
                    )
                    .map_err(|e| format!("更新聊天轮次失败: {e}"))?;
                } else {
                    append_chat_turn(
                        conn,
                        &AppendChatTurnInput {
                            id: turn_id.clone(),
                            session_id: session_id.clone(),
                            turn_index: index as i32,
                            prompt,
                            answer: answer.clone(),
                            thinking: thinking.clone(),
                            status: turn_status.clone(),
                            usage_json: usage_json.clone(),
                            response_segments_json: response_segments_json.clone(),
                            tool_calls_json: tool_calls_json.clone(),
                            activity_json: activity_json.clone(),
                            speaker_agent_id,
                        },
                    )?;
                    conn.execute(
                        "UPDATE chat_turns SET created_at = ?1 WHERE id = ?2",
                        rusqlite::params![turn_created_at, turn_id.clone()],
                    )
                    .map_err(|e| format!("回填轮次创建时间失败: {e}"))?;
                    let _ = update_chat_turn(
                        conn,
                        &UpdateChatTurnInput {
                            id: turn_id,
                            answer: Some(answer),
                            thinking: Some(thinking),
                            status: Some(turn_status),
                            completed_at,
                            usage_json,
                            response_segments_json,
                            tool_calls_json,
                            activity_json,
                        },
                    )?;
                }
            }
        }

        conn.execute(
            "UPDATE chat_sessions SET updated_at = ?1, created_at = ?2 WHERE id = ?3",
            rusqlite::params![updated_at, created_at, session_id],
        )
        .map_err(|e| format!("还原会话时间戳失败: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_in_memory;

    #[test]
    fn history_snapshot_roundtrip_uses_structured_tables() {
        let conn = open_in_memory().unwrap();
        let payload = serde_json::json!([
            {
                "id": "session-1",
                "title": "Session 1",
                "status": "done",
                "createdAt": 100,
                "updatedAt": 200,
                "agent": { "id": "agent-a", "name": "Agent A" },
                "workspaceId": "ws-1",
                "turns": [
                    {
                        "id": "turn-1",
                        "prompt": "hello",
                        "answer": "world",
                        "thinking": "plan",
                        "status": "done",
                        "createdAt": 110,
                        "completedAt": 120,
                        "activity": [],
                        "toolCalls": [],
                        "responseSegments": [{ "type": "text", "text": "world" }]
                    }
                ]
            }
        ]);

        replace_history_snapshot_json(&conn, &payload.to_string()).unwrap();
        let exported = export_history_snapshot_json(&conn).unwrap();
        let rows: Vec<Value> = serde_json::from_str(&exported).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "session-1");
        assert_eq!(rows[0]["turns"][0]["answer"], "world");
    }

    #[test]
    fn merge_history_snapshot_preserves_unmentioned_sessions() {
        let conn = open_in_memory().unwrap();

        create_chat_session(
            &conn,
            &CreateChatSessionInput {
                id: "existing-session".to_string(),
                title: "Existing".to_string(),
                status: "done".to_string(),
                agent_id: None,
                agent_snapshot_json: None,
                bot_target_json: None,
                session_llm_provider_id: None,
                session_llm_model: None,
                workspace_id: None,
            },
        )
        .unwrap();
        append_chat_turn(
            &conn,
            &AppendChatTurnInput {
                id: "existing-turn".to_string(),
                session_id: "existing-session".to_string(),
                turn_index: 0,
                prompt: "persist me".to_string(),
                answer: "still here".to_string(),
                thinking: String::new(),
                status: "done".to_string(),
                usage_json: None,
                response_segments_json: None,
                tool_calls_json: None,
                activity_json: None,
                speaker_agent_id: None,
            },
        )
        .unwrap();

        let payload = serde_json::json!([
            {
                "id": "new-session",
                "title": "New Session",
                "status": "running",
                "createdAt": 100,
                "updatedAt": 200,
                "turns": [
                    {
                        "id": "new-turn",
                        "prompt": "hello",
                        "answer": "world",
                        "thinking": "",
                        "status": "done",
                        "createdAt": 110,
                        "completedAt": 120,
                        "activity": [],
                        "toolCalls": [],
                        "responseSegments": [{ "type": "text", "text": "world" }]
                    }
                ]
            }
        ]);

        merge_history_snapshot_json(&conn, &payload.to_string()).unwrap();

        let exported = export_history_snapshot_json(&conn).unwrap();
        let rows: Vec<Value> = serde_json::from_str(&exported).unwrap();
        let ids = rows
            .iter()
            .map(|row| row["id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();

        assert!(ids.contains(&"existing-session"));
        assert!(ids.contains(&"new-session"));
    }

    #[test]
    fn session_count_from_snapshot_json_reads_array_length() {
        let payload = serde_json::json!([
            { "id": "s1", "turns": [] },
            { "id": "s2", "turns": [] },
            { "id": "s3", "turns": [] }
        ]);

        let count = session_count_from_snapshot_json(&payload.to_string()).unwrap();
        assert_eq!(count, 3);
    }
}
