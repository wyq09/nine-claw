use crate::agent_workspace;
use crate::app_constants::{
    HISTORY_DB_FILE, HISTORY_RECOVERY_MARKER_KEY, HISTORY_STATE_KEY, LEGACY_HISTORY_DB_FILES,
};
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

#[derive(Debug, Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd)]
struct HistoryDbSignal {
    structured_session_count: i64,
    structured_turn_count: i64,
    snapshot_session_count: i64,
}

impl HistoryDbSignal {
    fn has_history(self) -> bool {
        self.structured_session_count > 0
            || self.structured_turn_count > 0
            || self.snapshot_session_count > 0
    }
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

fn count_optional_table_rows(conn: &Connection, table_name: &str) -> Result<i64, String> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table_name],
            |row| row.get(0),
        )
        .map_err(|error| format!("检查表 {table_name} 是否存在失败: {error}"))?;
    if exists == 0 {
        return Ok(0);
    }

    conn.query_row(&format!("SELECT COUNT(*) FROM {table_name}"), [], |row| {
        row.get(0)
    })
    .map_err(|error| format!("统计表 {table_name} 行数失败: {error}"))
}

fn read_history_db_signal(path: &Path) -> Result<Option<HistoryDbSignal>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let db_path = path.to_path_buf();
    let connection = storage::db::open_at(&db_path)?;
    maybe_recover_structured_history_from_backup(&connection)?;

    let structured_session_count = count_optional_table_rows(&connection, "chat_sessions")?;
    let structured_turn_count = count_optional_table_rows(&connection, "chat_turns")?;
    let snapshot_session_count = load_history_v1_snapshot(&connection)?
        .as_deref()
        .map(storage::chat_history_snapshot::session_count_from_snapshot_json)
        .transpose()?
        .unwrap_or(0);

    Ok(Some(HistoryDbSignal {
        structured_session_count,
        structured_turn_count,
        snapshot_session_count,
    }))
}

fn history_db_candidate_paths(candidate_dirs: &[PathBuf], target_path: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for candidate_dir in candidate_dirs {
        for file_name in std::iter::once(HISTORY_DB_FILE).chain(LEGACY_HISTORY_DB_FILES.iter().copied()) {
            let candidate_path = candidate_dir.join(file_name);
            if candidate_path == target_path || !candidate_path.exists() {
                continue;
            }
            if !paths.iter().any(|existing| existing == &candidate_path) {
                paths.push(candidate_path);
            }
        }
    }
    paths
}

fn recover_empty_target_history_db_from_candidates(
    candidate_dirs: &[PathBuf],
    target_path: &Path,
) -> Result<(), String> {
    let Some(target_signal) = read_history_db_signal(target_path)? else {
        return Ok(());
    };
    if target_signal.has_history() {
        return Ok(());
    }

    let mut best_candidate: Option<(PathBuf, HistoryDbSignal)> = None;
    for candidate_path in history_db_candidate_paths(candidate_dirs, target_path) {
        let Some(candidate_signal) = read_history_db_signal(&candidate_path)? else {
            continue;
        };
        if !candidate_signal.has_history() {
            continue;
        }
        let should_replace = best_candidate
            .as_ref()
            .map(|(_, existing_signal)| candidate_signal > *existing_signal)
            .unwrap_or(true);
        if should_replace {
            best_candidate = Some((candidate_path, candidate_signal));
        }
    }

    let Some((candidate_path, candidate_signal)) = best_candidate else {
        return Ok(());
    };

    let candidate_db_path = candidate_path.clone();
    let candidate_conn = storage::db::open_at(&candidate_db_path)?;
    maybe_recover_structured_history_from_backup(&candidate_conn)?;
    let payload = storage::chat_history_snapshot::export_history_snapshot_json(&candidate_conn)?;
    if storage::chat_history_snapshot::session_count_from_snapshot_json(&payload)? == 0 {
        return Ok(());
    }

    let target_db_path = target_path.to_path_buf();
    let target_conn = storage::db::open_at(&target_db_path)?;
    storage::chat_history_snapshot::merge_history_snapshot_json(&target_conn, &payload)?;
    sync_history_v1_backup_from_structured(&target_conn)?;

    log::warn!(
        "Recovered empty history DB {} from candidate {} (sessions={}, turns={}, snapshot_sessions={})",
        target_path.display(),
        candidate_path.display(),
        candidate_signal.structured_session_count,
        candidate_signal.structured_turn_count,
        candidate_signal.snapshot_session_count,
    );

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
    let candidate_dirs = [workspace_root.clone(), app_data_dir];
    migrate_history_db_from_candidates(&candidate_dirs, &target_path)?;
    recover_empty_target_history_db_from_candidates(&candidate_dirs, &target_path)?;

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

fn load_history_v1_snapshot(conn: &Connection) -> Result<Option<String>, String> {
    use rusqlite::OptionalExtension;

    conn.query_row(
        "SELECT value FROM app_state WHERE key = ?1",
        params![HISTORY_STATE_KEY],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| format!("读取 history_v1 失败: {error}"))
}

fn write_history_v1_snapshot(conn: &Connection, payload: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO app_state (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![HISTORY_STATE_KEY, payload, chrono_like_timestamp()],
    )
    .map_err(|error| format!("写入 history_v1 备份失败: {error}"))?;
    Ok(())
}

fn history_recovery_already_applied(conn: &Connection) -> Result<bool, String> {
    use rusqlite::OptionalExtension;

    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![HISTORY_RECOVERY_MARKER_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("检查历史恢复标记失败: {error}"))?;
    Ok(value.as_deref() == Some("true"))
}

fn mark_history_recovery_applied(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "INSERT INTO app_state (key, value, updated_at) VALUES (?1, 'true', ?2)
         ON CONFLICT(key) DO UPDATE SET value = 'true', updated_at = excluded.updated_at",
        params![HISTORY_RECOVERY_MARKER_KEY, chrono_like_timestamp()],
    )
    .map_err(|error| format!("写入历史恢复标记失败: {error}"))?;
    Ok(())
}

fn maybe_recover_structured_history_from_backup(conn: &Connection) -> Result<(), String> {
    let session_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM chat_sessions", [], |row| row.get(0))
        .unwrap_or(0);

    if session_count != 0 || history_recovery_already_applied(conn)? {
        return Ok(());
    }

    let Some(v1_json) = load_history_v1_snapshot(conn)? else {
        return Ok(());
    };

    let backup_session_count =
        storage::chat_history_snapshot::session_count_from_snapshot_json(&v1_json)?;
    if backup_session_count > 0 {
        storage::chat_history_snapshot::merge_history_snapshot_json(conn, &v1_json)?;
        mark_history_recovery_applied(conn)?;
    }

    Ok(())
}

pub(crate) fn sync_history_v1_backup_from_structured(conn: &Connection) -> Result<(), String> {
    let payload = storage::chat_history_snapshot::export_history_snapshot_json(conn)?;
    write_history_v1_snapshot(conn, &payload)
}

#[tauri::command]
pub(crate) fn load_history_state(app: AppHandle) -> Result<Option<String>, String> {
    let connection = open_history_db(&app)?;
    maybe_recover_structured_history_from_backup(&connection)?;
    sync_history_v1_backup_from_structured(&connection)?;

    let payload = storage::chat_history_snapshot::export_history_snapshot_json(&connection)?;
    Ok(Some(payload))
}

#[tauri::command]
pub(crate) fn save_history_state(app: AppHandle, payload: String) -> Result<(), String> {
    let mut connection = open_history_db(&app)?;
    // Red line: front-end history snapshots may be incomplete during reload/recovery.
    // Never replace structured history from them; only merge/upsert.
    storage::chat_history_snapshot::merge_history_snapshot_json(&connection, &payload)?;
    sync_usage_records_from_history_payload(&mut connection, &payload)?;
    sync_history_v1_backup_from_structured(&connection)?;
    reindex_chat_turn_vectors_async(app);

    Ok(())
}

#[tauri::command]
pub(crate) fn clear_history_state(app: AppHandle) -> Result<(), String> {
    let connection = open_history_db(&app)?;
    storage::chat_history::clear_all_chat_sessions(&connection)?;
    sync_history_v1_backup_from_structured(&connection)?;
    Ok(())
}

pub(crate) fn storage_conn(app: &AppHandle) -> Result<rusqlite::Connection, String> {
    let db_path = history_db_path(app)?;
    storage::db::open_at(&db_path)
}

pub(crate) fn history_db_diagnostics(app: &AppHandle) -> Result<(PathBuf, i64, i64), String> {
    let db_path = history_db_path(app)?;
    let connection = storage::db::open_at(&db_path)?;
    let session_count = count_optional_table_rows(&connection, "chat_sessions")?;
    let turn_count = count_optional_table_rows(&connection, "chat_turns")?;
    Ok((db_path, session_count, turn_count))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_constants::HISTORY_STATE_KEY;
    use crate::storage::chat_history::{
        append_chat_turn, create_chat_session, list_chat_sessions, list_chat_turns,
        AppendChatTurnInput, CreateChatSessionInput,
    };
    use crate::storage::db::open_at;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!("nineclaw-history-tests-{label}-{unique}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn create_structured_session(path: &Path, session_id: &str, prompt: &str) {
        let db_path = path.to_path_buf();
        let conn = open_at(&db_path).unwrap();
        create_chat_session(
            &conn,
            &CreateChatSessionInput {
                id: session_id.to_string(),
                title: format!("Title {session_id}"),
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
                id: format!("{session_id}-turn-1"),
                session_id: session_id.to_string(),
                turn_index: 0,
                prompt: prompt.to_string(),
                answer: "answer".to_string(),
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
    }

    #[test]
    fn recover_empty_target_history_db_from_structured_candidate() {
        let root_dir = temp_dir_path("structured-candidate");
        let target_path = root_dir.join(HISTORY_DB_FILE);
        let candidate_dir = root_dir.join("app-data");
        fs::create_dir_all(&candidate_dir).unwrap();

        open_at(&target_path).unwrap();
        create_structured_session(&candidate_dir.join(HISTORY_DB_FILE), "candidate-session", "hello");

        recover_empty_target_history_db_from_candidates(&[candidate_dir], &target_path).unwrap();

        let target_conn = open_at(&target_path).unwrap();
        let sessions = list_chat_sessions(&target_conn).unwrap();
        let turns = list_chat_turns(&target_conn, "candidate-session").unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "candidate-session");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].prompt, "hello");
    }

    #[test]
    fn recover_empty_target_history_db_from_history_v1_candidate() {
        let root_dir = temp_dir_path("snapshot-candidate");
        let target_path = root_dir.join(HISTORY_DB_FILE);
        let candidate_dir = root_dir.join("app-data");
        fs::create_dir_all(&candidate_dir).unwrap();

        open_at(&target_path).unwrap();

        let candidate_path = candidate_dir.join(HISTORY_DB_FILE);
        let candidate_conn = open_at(&candidate_path).unwrap();
        let payload = serde_json::json!([
            {
                "id": "snapshot-session",
                "title": "Recovered from snapshot",
                "status": "done",
                "createdAt": 100,
                "updatedAt": 200,
                "turns": [
                    {
                        "id": "snapshot-turn",
                        "prompt": "from backup",
                        "answer": "answer",
                        "thinking": "",
                        "status": "done",
                        "createdAt": 101,
                        "completedAt": 102,
                        "activity": [],
                        "toolCalls": [],
                        "responseSegments": []
                    }
                ]
            }
        ])
        .to_string();
        candidate_conn
            .execute(
                "INSERT INTO app_state (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![HISTORY_STATE_KEY, payload, chrono_like_timestamp()],
            )
            .unwrap();

        recover_empty_target_history_db_from_candidates(&[candidate_dir], &target_path).unwrap();

        let target_conn = open_at(&target_path).unwrap();
        let sessions = list_chat_sessions(&target_conn).unwrap();
        let turns = list_chat_turns(&target_conn, "snapshot-session").unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "snapshot-session");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].prompt, "from backup");
    }

    #[test]
    fn recover_empty_target_history_db_does_not_override_non_empty_target() {
        let root_dir = temp_dir_path("preserve-target");
        let target_path = root_dir.join(HISTORY_DB_FILE);
        let candidate_dir = root_dir.join("app-data");
        fs::create_dir_all(&candidate_dir).unwrap();

        create_structured_session(&target_path, "target-session", "keep me");
        create_structured_session(&candidate_dir.join(HISTORY_DB_FILE), "candidate-session", "old data");

        recover_empty_target_history_db_from_candidates(&[candidate_dir], &target_path).unwrap();

        let target_conn = open_at(&target_path).unwrap();
        let sessions = list_chat_sessions(&target_conn).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "target-session");
    }
}
