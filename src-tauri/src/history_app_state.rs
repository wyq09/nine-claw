use crate::agent_workspace;
use crate::app_constants::{HISTORY_DB_FILE, LEGACY_HISTORY_DB_FILES};
use crate::pi_usage::{
    extract_usage_metadata_payload, extract_usage_payload, json_i64, json_string,
    usage_row_total_tokens, PiTokenUsagePayload, PiUsageMetadataPayload,
};
use crate::storage;
use crate::time_util::chrono_like_timestamp;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TokenUsageRecordRow {
    pub(crate) turn_id: String,
    pub(crate) session_id: String,
    pub(crate) turn_created_at: i64,
    pub(crate) turn_completed_at: Option<i64>,
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_name: Option<String>,
    pub(crate) api: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) response_id: Option<String>,
    pub(crate) usage_timestamp: Option<i64>,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) cache_read_tokens: u64,
    pub(crate) cache_write_tokens: u64,
    pub(crate) total_tokens: u64,
    pub(crate) recorded_at: i64,
}

fn migrate_legacy_history_db(app_data_dir: &Path, target_path: &Path) -> Result<(), String> {
    if target_path.exists() {
        return Ok(());
    }

    for legacy_name in LEGACY_HISTORY_DB_FILES {
        let legacy_path = app_data_dir.join(legacy_name);
        if !legacy_path.exists() {
            continue;
        }

        fs::rename(&legacy_path, target_path)
            .or_else(|rename_error| {
                fs::copy(&legacy_path, target_path)
                    .map_err(|copy_error| {
                        std::io::Error::new(
                            copy_error.kind(),
                            format!("rename 失败({rename_error})，copy 也失败: {copy_error}"),
                        )
                    })
                    .and_then(|_| fs::remove_file(&legacy_path))
            })
            .map_err(|error| format!("迁移旧历史数据库失败: {error}"))?;

        break;
    }

    Ok(())
}

fn migrate_history_db_from_candidates(
    candidate_dirs: &[PathBuf],
    target_path: &Path,
) -> Result<(), String> {
    if target_path.exists() {
        return Ok(());
    }

    for candidate_dir in candidate_dirs {
        if !candidate_dir.exists() {
            continue;
        }
        migrate_legacy_history_db(candidate_dir, target_path)?;

        let legacy_target = candidate_dir.join(HISTORY_DB_FILE);
        if !legacy_target.exists() || target_path.exists() {
            continue;
        }

        fs::rename(&legacy_target, target_path)
            .or_else(|rename_error| {
                fs::copy(&legacy_target, target_path)
                    .map_err(|copy_error| {
                        std::io::Error::new(
                            copy_error.kind(),
                            format!("rename 失败({rename_error})，copy 也失败: {copy_error}"),
                        )
                    })
                    .and_then(|_| fs::remove_file(&legacy_target))
            })
            .map_err(|error| format!("迁移历史数据库失败: {error}"))?;
    }

    Ok(())
}

pub(crate) fn history_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let workspace_root = agent_workspace::resolve_workspace_root()?;
    fs::create_dir_all(&workspace_root)
        .map_err(|error| format!("创建共享 workspace 根目录失败: {error}"))?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("读取应用数据目录失败: {error}"))?;

    fs::create_dir_all(&app_data_dir).map_err(|error| format!("创建应用数据目录失败: {error}"))?;

    let target_path = workspace_root.join(HISTORY_DB_FILE);
    migrate_history_db_from_candidates(&[workspace_root.clone(), app_data_dir], &target_path)?;

    Ok(target_path)
}

pub(crate) fn ensure_app_state_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS app_state (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at INTEGER NOT NULL
      )",
            [],
        )
        .map_err(|error| format!("初始化历史数据库失败: {error}"))?;

    Ok(())
}

fn ensure_token_usage_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
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
        .map_err(|error| format!("初始化用量数据库失败: {error}"))?;

    Ok(())
}

fn upsert_usage_record_from_snapshot(
    connection: &Connection,
    session_id: &str,
    agent_id: Option<&str>,
    agent_name: Option<&str>,
    session_model: Option<&str>,
    turn: &serde_json::Value,
    recorded_at: i64,
) -> Result<(), String> {
    let Some(turn_obj) = turn.as_object() else {
        return Ok(());
    };

    let turn_id =
        json_string(turn_obj.get("id")).ok_or_else(|| "历史快照中的 turn 缺少 id".to_string())?;
    let turn_created_at = json_i64(turn_obj.get("createdAt")).unwrap_or(recorded_at);
    let turn_completed_at = json_i64(turn_obj.get("completedAt"));
    let usage = turn_obj.get("usage");
    let usage_payload = extract_usage_payload(usage);
    let Some(usage_payload) = usage_payload else {
        return Ok(());
    };

    let usage_meta = extract_usage_metadata_payload(usage);
    let api = usage_meta.as_ref().and_then(|item| item.api.clone());
    let provider = usage_meta.as_ref().and_then(|item| item.provider.clone());
    let model = usage_meta
        .as_ref()
        .and_then(|item| item.model.clone())
        .or_else(|| session_model.map(ToOwned::to_owned));
    let response_id = usage_meta
        .as_ref()
        .and_then(|item| item.response_id.clone());
    let usage_timestamp = usage_meta.as_ref().and_then(|item| item.timestamp);

    connection
        .execute(
            "INSERT INTO token_usage_records (
              turn_id,
              session_id,
              turn_created_at,
              turn_completed_at,
              agent_id,
              agent_name,
              api,
              provider,
              model,
              response_id,
              usage_timestamp,
              input_tokens,
              output_tokens,
              cache_read_tokens,
              cache_write_tokens,
              total_tokens,
              recorded_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            ON CONFLICT(turn_id) DO UPDATE SET
              session_id = excluded.session_id,
              turn_created_at = excluded.turn_created_at,
              turn_completed_at = excluded.turn_completed_at,
              agent_id = excluded.agent_id,
              agent_name = excluded.agent_name,
              api = COALESCE(excluded.api, token_usage_records.api),
              provider = COALESCE(excluded.provider, token_usage_records.provider),
              model = COALESCE(excluded.model, token_usage_records.model),
              response_id = COALESCE(excluded.response_id, token_usage_records.response_id),
              usage_timestamp = COALESCE(excluded.usage_timestamp, token_usage_records.usage_timestamp),
              input_tokens = excluded.input_tokens,
              output_tokens = excluded.output_tokens,
              cache_read_tokens = excluded.cache_read_tokens,
              cache_write_tokens = excluded.cache_write_tokens,
              total_tokens = excluded.total_tokens,
              recorded_at = excluded.recorded_at",
            params![
                turn_id,
                session_id,
                turn_created_at,
                turn_completed_at,
                agent_id,
                agent_name,
                api,
                provider,
                model,
                response_id,
                usage_timestamp,
                usage_payload.input_tokens.unwrap_or(0),
                usage_payload.output_tokens.unwrap_or(0),
                usage_payload.cache_read_tokens.unwrap_or(0),
                usage_payload.cache_write_tokens.unwrap_or(0),
                usage_payload.total_tokens.unwrap_or(0),
                recorded_at,
            ],
        )
        .map_err(|error| format!("写入用量明细失败: {error}"))?;

    Ok(())
}

/// Persists PI/LLM token usage from scheduler-driven runs (任务中心 / 心跳定时) into `token_usage_records`.
pub(crate) fn record_token_usage_for_scheduler_pi_completion(
    app: &AppHandle,
    turn_id: String,
    session_label_id: &str,
    agent: &crate::agents::AgentRecord,
    session_model: &str,
    usage: Option<PiTokenUsagePayload>,
    usage_meta: Option<PiUsageMetadataPayload>,
) -> Result<(), String> {
    let Some(usage_payload) = usage else {
        return Ok(());
    };
    if usage_row_total_tokens(&usage_payload) == 0 {
        return Ok(());
    }

    let recorded_at = chrono_like_timestamp();
    let mut usage_value = serde_json::to_value(&usage_payload)
        .map_err(|e| format!("序列化 scheduler usage 失败: {e}"))?;
    if let Some(meta) = usage_meta {
        if let serde_json::Value::Object(ref mut map) = usage_value {
            if let Some(ref v) = meta.api {
                map.insert("api".into(), serde_json::Value::String(v.clone()));
            }
            if let Some(ref v) = meta.provider {
                map.insert("provider".into(), serde_json::Value::String(v.clone()));
            }
            if let Some(ref v) = meta.model {
                map.insert("model".into(), serde_json::Value::String(v.clone()));
            }
            if let Some(ref v) = meta.response_id {
                map.insert("responseId".into(), serde_json::Value::String(v.clone()));
            }
            if let Some(v) = meta.timestamp {
                map.insert("timestamp".into(), serde_json::json!(v));
            }
        }
    }

    let turn = serde_json::json!({
        "id": turn_id,
        "createdAt": recorded_at,
        "completedAt": recorded_at,
        "usage": usage_value,
    });

    let connection = open_history_db(app)?;
    ensure_token_usage_schema(&connection)?;
    upsert_usage_record_from_snapshot(
        &connection,
        session_label_id,
        Some(agent.id.as_str()),
        Some(agent.name.as_str()),
        Some(session_model),
        &turn,
        recorded_at,
    )
}

fn sync_usage_records_from_history_payload(
    connection: &mut Connection,
    payload: &str,
) -> Result<(), String> {
    let parsed: serde_json::Value =
        serde_json::from_str(payload).map_err(|error| format!("解析历史快照失败: {error}"))?;
    let Some(history_items) = parsed.as_array() else {
        return Ok(());
    };

    let recorded_at = chrono_like_timestamp();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| format!("开启用量事务失败: {error}"))?;

    for item in history_items {
        let Some(item_obj) = item.as_object() else {
            continue;
        };

        let Some(session_id) = json_string(item_obj.get("id")) else {
            continue;
        };
        let agent = item_obj.get("agent").and_then(|value| value.as_object());
        let agent_id = agent.and_then(|value| json_string(value.get("id")));
        let agent_name = agent.and_then(|value| json_string(value.get("name")));
        let session_model = json_string(item_obj.get("sessionLlmModel"))
            .or_else(|| agent.and_then(|value| json_string(value.get("defaultModel"))));

        let Some(turns) = item_obj.get("turns").and_then(|value| value.as_array()) else {
            continue;
        };

        for turn in turns {
            upsert_usage_record_from_snapshot(
                &transaction,
                &session_id,
                agent_id.as_deref(),
                agent_name.as_deref(),
                session_model.as_deref(),
                turn,
                recorded_at,
            )?;
        }
    }

    transaction
        .commit()
        .map_err(|error| format!("提交用量事务失败: {error}"))?;

    Ok(())
}

pub(crate) fn open_history_db(app: &AppHandle) -> Result<Connection, String> {
    let db_path = history_db_path(app)?;
    let connection =
        Connection::open(db_path).map_err(|error| format!("打开历史数据库失败: {error}"))?;

    storage::db::ensure_all_schemas(&connection)?;

    Ok(connection)
}

#[tauri::command]
pub(crate) fn load_history_state(app: AppHandle) -> Result<Option<String>, String> {
    let connection = open_history_db(&app)?;
    let payload = storage::chat_history_snapshot::export_history_snapshot_json(&connection)?;
    Ok(Some(payload))
}

#[tauri::command]
pub(crate) fn save_history_state(app: AppHandle, payload: String) -> Result<(), String> {
    let mut connection = open_history_db(&app)?;
    storage::chat_history_snapshot::replace_history_snapshot_json(&connection, &payload)?;

    sync_usage_records_from_history_payload(&mut connection, &payload)?;
    reindex_chat_turn_vectors_async(app);

    Ok(())
}

#[tauri::command]
pub(crate) fn clear_history_state(app: AppHandle) -> Result<(), String> {
    let connection = open_history_db(&app)?;
    storage::chat_history::clear_all_chat_sessions(&connection)?;
    Ok(())
}

pub(crate) fn storage_conn(app: &AppHandle) -> Result<rusqlite::Connection, String> {
    let db_path = history_db_path(app)?;
    storage::db::open_at(&db_path)
}

fn reindex_chat_turn_vectors_async(app: AppHandle) {
    if cfg!(test) {
        return;
    }
    let Some(registry) = crate::managed_runtime::get_embedding_registry() else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let provider = {
            let guard = registry.read().await;
            guard.default_provider()
        };
        let Some(provider) = provider else {
            return;
        };
        let Ok(conn) = storage_conn(&app) else {
            return;
        };
        let Ok(sessions) = storage::chat_history::list_chat_sessions(&conn) else {
            return;
        };

        for session in sessions {
            let Some(agent_id) = session.agent_id.as_deref() else {
                continue;
            };
            let Ok(turns) = storage::chat_history::list_chat_turns(&conn, &session.id) else {
                continue;
            };
            for turn in turns {
                // Only index turns with non-empty answer
                if turn.answer.is_empty() {
                    continue;
                }
                // Skip if already indexed (has metadata)
                let memory_id = storage::core_memory::chat_turn_vector_memory_id(&turn.id);
                let existing_meta: Option<String> = conn
                    .query_row(
                        "SELECT metadata_json FROM memory_vectors WHERE memory_id = ?1",
                        rusqlite::params![memory_id],
                        |row| row.get(0),
                    )
                    .ok()
                    .flatten();
                if existing_meta.is_some() {
                    continue; // Already indexed
                }

                let text = format!("{}\n{}", turn.prompt, turn.answer);
                let metadata = serde_json::json!({
                    "sessionId": session.id,
                    "sessionTitle": session.title,
                    "agentId": agent_id,
                    "turnIndex": turn.turn_index,
                    "turnId": turn.id,
                    "timestamp": turn.created_at,
                    "completedAt": turn.completed_at,
                    "workspaceId": session.workspace_id,
                });
                let metadata_str = serde_json::to_string(&metadata).unwrap_or_default();
                match provider.embed(vec![text.clone()]).await {
                    Ok(embeddings) => {
                        if let Some(embedding) = embeddings.into_iter().next() {
                            let vector_id = format!("vec_{}", uuid::Uuid::new_v4().simple());
                            let _ = crate::memory_vector::upsert_vector_with_meta(
                                &conn,
                                &vector_id,
                                &memory_id,
                                &storage::core_memory::agent_vector_namespace(agent_id),
                                &embedding,
                                provider.id(),
                                Some(&metadata_str),
                                Some(&text),
                                None,
                            );
                        }
                    }
                    Err(error) => log::warn!("聊天轮次向量写入失败: {error}"),
                }
            }
        }
    });
}

#[tauri::command]
pub(crate) fn list_token_usage_records(app: AppHandle) -> Result<Vec<TokenUsageRecordRow>, String> {
    let connection = open_history_db(&app)?;
    let mut statement = connection
        .prepare(
            "SELECT
              turn_id,
              session_id,
              turn_created_at,
              turn_completed_at,
              agent_id,
              agent_name,
              api,
              provider,
              model,
              response_id,
              usage_timestamp,
              input_tokens,
              output_tokens,
              cache_read_tokens,
              cache_write_tokens,
              total_tokens,
              recorded_at
            FROM token_usage_records
            ORDER BY COALESCE(turn_completed_at, turn_created_at) DESC, recorded_at DESC",
        )
        .map_err(|error| format!("查询用量明细失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(TokenUsageRecordRow {
                turn_id: row.get(0)?,
                session_id: row.get(1)?,
                turn_created_at: row.get(2)?,
                turn_completed_at: row.get(3)?,
                agent_id: row.get(4)?,
                agent_name: row.get(5)?,
                api: row.get(6)?,
                provider: row.get(7)?,
                model: row.get(8)?,
                response_id: row.get(9)?,
                usage_timestamp: row.get(10)?,
                input_tokens: row.get(11)?,
                output_tokens: row.get(12)?,
                cache_read_tokens: row.get(13)?,
                cache_write_tokens: row.get(14)?,
                total_tokens: row.get(15)?,
                recorded_at: row.get(16)?,
            })
        })
        .map_err(|error| format!("遍历用量明细失败: {error}"))?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|error| format!("读取用量明细失败: {error}"))?);
    }

    Ok(records)
}
