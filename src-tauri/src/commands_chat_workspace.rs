use crate::agent_tasks::{self, AgentTaskDeliveryRecord, AgentTaskListItem, AgentTaskUpdateInput};
use crate::history_app_state::storage_conn;
use crate::provider_runtime::ProviderRuntimeConfig;
use crate::scheduler;
use crate::storage;
use crate::team_workspace;
use crate::workspace_fs;
use base64::{engine::general_purpose::STANDARD as BASE64_ENGINE, Engine as _};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Session list item for the frontend — no turns included.
#[derive(Serialize)]
pub(crate) struct ChatSessionListItem {
    id: String,
    title: String,
    status: String,
    created_at: i64,
    updated_at: i64,
    agent_id: Option<String>,
    agent_snapshot_json: Option<String>,
    bot_target_json: Option<String>,
    session_llm_provider_id: Option<String>,
    session_llm_model: Option<String>,
    workspace_id: Option<String>,
    turn_count: i64,
}

impl From<storage::chat_history::ChatSession> for ChatSessionListItem {
    fn from(s: storage::chat_history::ChatSession) -> Self {
        Self {
            id: s.id,
            title: s.title,
            status: s.status,
            created_at: s.created_at,
            updated_at: s.updated_at,
            agent_id: s.agent_id,
            agent_snapshot_json: s.agent_snapshot_json,
            bot_target_json: s.bot_target_json,
            session_llm_provider_id: s.session_llm_provider_id,
            session_llm_model: s.session_llm_model,
            workspace_id: s.workspace_id.clone(),
            turn_count: 0,
        }
    }
}

/// Full session detail for the frontend — includes turns.
#[derive(Serialize)]
pub(crate) struct ChatSessionDetail {
    id: String,
    title: String,
    status: String,
    created_at: i64,
    updated_at: i64,
    agent_id: Option<String>,
    agent_snapshot_json: Option<String>,
    bot_target_json: Option<String>,
    session_llm_provider_id: Option<String>,
    session_llm_model: Option<String>,
    workspace_id: Option<String>,
    turns: Vec<storage::chat_history::ChatTurn>,
}

impl From<storage::chat_history::ChatSession> for ChatSessionDetail {
    fn from(s: storage::chat_history::ChatSession) -> Self {
        Self {
            id: s.id,
            title: s.title,
            status: s.status,
            created_at: s.created_at,
            updated_at: s.updated_at,
            agent_id: s.agent_id,
            agent_snapshot_json: s.agent_snapshot_json,
            bot_target_json: s.bot_target_json,
            session_llm_provider_id: s.session_llm_provider_id,
            session_llm_model: s.session_llm_model,
            workspace_id: s.workspace_id,
            turns: Vec::new(),
        }
    }
}

#[tauri::command]
pub(crate) fn chat_list_sessions(app: AppHandle) -> Result<Vec<ChatSessionListItem>, String> {
    let conn = storage_conn(&app)?;
    let sessions = storage::chat_history::list_chat_sessions(&conn)?;
    let mut items: Vec<ChatSessionListItem> = sessions
        .into_iter()
        .map(ChatSessionListItem::from)
        .collect();
    for item in &mut items {
        item.turn_count = storage::chat_history::count_chat_turns(&conn, &item.id)?;
    }
    Ok(items)
}

#[tauri::command]
pub(crate) fn chat_get_session_detail(
    app: AppHandle,
    session_id: String,
) -> Result<Option<ChatSessionDetail>, String> {
    let conn = storage_conn(&app)?;
    let Some(session) = storage::chat_history::get_chat_session(&conn, &session_id)? else {
        return Ok(None);
    };
    let turns = storage::chat_history::list_chat_turns(&conn, &session_id)?;
    let mut detail = ChatSessionDetail::from(session);
    detail.turns = turns;
    Ok(Some(detail))
}

#[tauri::command]
pub(crate) fn chat_create_session(
    app: AppHandle,
    id: String,
    title: String,
    status: String,
    agent_id: Option<String>,
    agent_snapshot_json: Option<String>,
    bot_target_json: Option<String>,
    session_llm_provider_id: Option<String>,
    session_llm_model: Option<String>,
    workspace_id: Option<String>,
) -> Result<ChatSessionDetail, String> {
    let conn = storage_conn(&app)?;
    let session = storage::chat_history::create_chat_session(
        &conn,
        &storage::chat_history::CreateChatSessionInput {
            id,
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
    Ok(ChatSessionDetail::from(session))
}

#[tauri::command]
pub(crate) fn chat_append_turn(
    app: AppHandle,
    id: String,
    session_id: String,
    turn_index: i32,
    prompt: String,
    answer: String,
    thinking: String,
    status: String,
    usage_json: Option<String>,
    response_segments_json: Option<String>,
    tool_calls_json: Option<String>,
    activity_json: Option<String>,
    speaker_agent_id: Option<String>,
) -> Result<storage::chat_history::ChatTurn, String> {
    let conn = storage_conn(&app)?;
    storage::chat_history::append_chat_turn(
        &conn,
        &storage::chat_history::AppendChatTurnInput {
            id,
            session_id,
            turn_index,
            prompt,
            answer,
            thinking,
            status,
            usage_json,
            response_segments_json,
            tool_calls_json,
            activity_json,
            speaker_agent_id,
        },
    )
}

#[tauri::command]
pub(crate) fn chat_update_turn(
    app: AppHandle,
    id: String,
    answer: Option<String>,
    thinking: Option<String>,
    status: Option<String>,
    completed_at: Option<i64>,
    usage_json: Option<String>,
    response_segments_json: Option<String>,
    tool_calls_json: Option<String>,
    activity_json: Option<String>,
) -> Result<storage::chat_history::ChatTurn, String> {
    let conn = storage_conn(&app)?;
    storage::chat_history::update_chat_turn(
        &conn,
        &storage::chat_history::UpdateChatTurnInput {
            id,
            answer,
            thinking,
            status,
            completed_at,
            usage_json,
            response_segments_json,
            tool_calls_json,
            activity_json,
        },
    )
}

#[tauri::command]
pub(crate) fn chat_delete_session(app: AppHandle, session_id: String) -> Result<(), String> {
    let conn = storage_conn(&app)?;
    storage::chat_history::delete_chat_session(&conn, &session_id)
}

#[tauri::command]
pub(crate) fn chat_clear_all_sessions(app: AppHandle) -> Result<(), String> {
    let conn = storage_conn(&app)?;
    storage::chat_history::clear_all_chat_sessions(&conn)
}

#[tauri::command]
pub(crate) fn chat_migrate_history_v1(app: AppHandle) -> Result<String, String> {
    let mut conn = storage_conn(&app)?;
    let result = storage::migrations::migrate_history_v1_to_structured(&mut conn)?;
    Ok(format!("{result:?}"))
}

#[tauri::command]
pub(crate) fn workspace_list(
    app: AppHandle,
    include_archived: Option<bool>,
) -> Result<Vec<storage::workspaces::WorkspaceRecord>, String> {
    let conn = storage_conn(&app)?;
    storage::workspaces::list_workspaces(&conn, include_archived.unwrap_or(false))
}

#[tauri::command]
pub(crate) fn workspace_create(
    app: AppHandle,
    name: String,
    description: String,
    supervisor_agent_id: String,
) -> Result<storage::workspaces::WorkspaceRecord, String> {
    team_workspace::create_workspace_with_fs(&app, name, description, supervisor_agent_id)
}

#[tauri::command]
pub(crate) fn workspace_update(
    app: AppHandle,
    workspace_id: String,
    name: Option<String>,
    description: Option<String>,
    artifacts_root: Option<String>,
    supervisor_orchestration_prompt: Option<String>,
    llm_trace_enabled: Option<bool>,
) -> Result<storage::workspaces::WorkspaceRecord, String> {
    let conn = storage_conn(&app)?;
    let out = storage::workspaces::update_workspace(
        &conn,
        workspace_id.trim(),
        name.as_deref(),
        description.as_deref(),
        artifacts_root.as_deref(),
        supervisor_orchestration_prompt.as_deref(),
        llm_trace_enabled,
    )?;
    let _ = team_workspace::sync_team_manifest_json(&app, workspace_id.trim());
    Ok(out)
}

#[tauri::command]
pub(crate) fn workspace_default_supervisor_orchestration_prompt(
    app: AppHandle,
    workspace_id: String,
) -> Result<String, String> {
    team_workspace::workspace_default_supervisor_orchestration_prompt(&app, workspace_id.trim())
}

#[tauri::command]
pub(crate) fn workspace_resolve_artifacts_root(
    app: AppHandle,
    workspace_id: String,
) -> Result<String, String> {
    let conn = storage_conn(&app)?;
    let Some(ws) = storage::workspaces::get_workspace(&conn, workspace_id.trim())? else {
        return Err("工作空间不存在".to_string());
    };
    let p = workspace_fs::resolve_artifacts_root_path(workspace_id.trim(), &ws.artifacts_root)?;
    Ok(p.to_string_lossy().to_string())
}

#[tauri::command]
pub(crate) fn workspace_list_artifacts_entries(
    app: AppHandle,
    workspace_id: String,
    sub_path: Option<String>,
) -> Result<Vec<workspace_fs::ArtifactsTreeEntry>, String> {
    let conn = storage_conn(&app)?;
    let Some(ws) = storage::workspaces::get_workspace(&conn, workspace_id.trim())? else {
        return Err("工作空间不存在".to_string());
    };
    workspace_fs::list_artifacts_dir_entries(
        workspace_id.trim(),
        &ws.artifacts_root,
        sub_path.as_deref().unwrap_or(""),
    )
}

#[tauri::command]
pub(crate) fn workspace_read_artifact_text(
    app: AppHandle,
    workspace_id: String,
    rel_path: String,
) -> Result<String, String> {
    let conn = storage_conn(&app)?;
    let Some(ws) = storage::workspaces::get_workspace(&conn, workspace_id.trim())? else {
        return Err("工作空间不存在".to_string());
    };
    workspace_fs::read_artifact_text_preview(workspace_id.trim(), &ws.artifacts_root, &rel_path)
}

#[tauri::command]
pub(crate) fn workspace_artifact_absolute_path(
    app: AppHandle,
    workspace_id: String,
    rel_path: String,
) -> Result<String, String> {
    let conn = storage_conn(&app)?;
    let Some(ws) = storage::workspaces::get_workspace(&conn, workspace_id.trim())? else {
        return Err("工作空间不存在".to_string());
    };
    let p = workspace_fs::artifact_file_absolute_path(
        workspace_id.trim(),
        &ws.artifacts_root,
        &rel_path,
    )?;
    Ok(p.to_string_lossy().to_string())
}

#[tauri::command]
pub(crate) fn workspace_set_archived(
    app: AppHandle,
    workspace_id: String,
    archived: bool,
) -> Result<(), String> {
    let conn = storage_conn(&app)?;
    storage::workspaces::set_workspace_archived(&conn, workspace_id.trim(), archived)
}

#[tauri::command]
pub(crate) fn workspace_add_member(
    app: AppHandle,
    workspace_id: String,
    agent_id: String,
    role: Option<String>,
) -> Result<(), String> {
    let role = role
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("member");
    team_workspace::add_member_and_sync(&app, workspace_id.trim(), agent_id.trim(), role)
}

#[tauri::command]
pub(crate) fn workspace_remove_member(
    app: AppHandle,
    workspace_id: String,
    agent_id: String,
) -> Result<(), String> {
    team_workspace::remove_member_and_sync(&app, workspace_id.trim(), agent_id.trim())
}

#[tauri::command]
pub(crate) fn workspace_list_members(
    app: AppHandle,
    workspace_id: String,
) -> Result<Vec<team_workspace::WorkspaceMemberView>, String> {
    team_workspace::list_team_member_views(&app, workspace_id.trim())
}

#[tauri::command]
pub(crate) fn workspace_list_resources(
    app: AppHandle,
    workspace_id: String,
) -> Result<Vec<storage::workspaces::WorkspaceResourceRecord>, String> {
    let conn = storage_conn(&app)?;
    storage::workspaces::list_workspace_resources(&conn, workspace_id.trim())
}

#[tauri::command]
pub(crate) fn workspace_upload_resource(
    app: AppHandle,
    workspace_id: String,
    file_name: String,
    data_base64: String,
    mime: Option<String>,
    uploader_agent_id: Option<String>,
) -> Result<storage::workspaces::WorkspaceResourceRecord, String> {
    let conn = storage_conn(&app)?;
    let wid = workspace_id.trim();
    let _ = storage::workspaces::get_workspace(&conn, wid)?
        .ok_or_else(|| "工作空间不存在".to_string())?;
    let bytes = BASE64_ENGINE
        .decode(data_base64.trim())
        .map_err(|e| format!("Base64 解码失败: {e}"))?;
    let (_abs, rel) = workspace_fs::persist_team_doc_file(wid, &file_name, &bytes)?;
    let mime_s = mime
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("application/octet-stream")
        .to_string();
    let id = uuid::Uuid::new_v4().to_string();
    let size = bytes.len() as i64;
    storage::workspaces::insert_workspace_resource(
        &conn,
        &id,
        wid,
        file_name.trim(),
        &rel,
        &mime_s,
        size,
        uploader_agent_id.as_deref(),
    )
}

#[tauri::command]
pub(crate) fn workspace_read_resource_text(
    app: AppHandle,
    workspace_id: String,
    rel_path: String,
) -> Result<String, String> {
    let _ = storage_conn(&app)?;
    workspace_fs::read_team_file(workspace_id.trim(), &rel_path)
}

#[tauri::command]
pub(crate) fn workspace_resource_absolute_path(
    app: AppHandle,
    workspace_id: String,
    rel_path: String,
) -> Result<String, String> {
    let _ = storage_conn(&app)?;
    let p = workspace_fs::absolute_team_resource_path(workspace_id.trim(), rel_path.trim())?;
    Ok(p.to_string_lossy().to_string())
}

#[tauri::command]
pub(crate) fn workspace_delete_resource(
    app: AppHandle,
    workspace_id: String,
    resource_id: String,
) -> Result<(), String> {
    team_workspace::delete_team_resource(&app, workspace_id.trim(), resource_id.trim())
}

#[tauri::command]
pub(crate) fn workspace_list_memories(
    app: AppHandle,
    workspace_id: String,
    limit: Option<i64>,
) -> Result<Vec<storage::workspaces::WorkspaceMemoryRecord>, String> {
    let conn = storage_conn(&app)?;
    storage::workspaces::list_workspace_memories(&conn, workspace_id.trim(), limit.unwrap_or(50))
}

#[tauri::command]
pub(crate) fn workspace_write_memory(
    app: AppHandle,
    workspace_id: String,
    title: String,
    content: String,
    author_agent_id: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<storage::workspaces::WorkspaceMemoryRecord, String> {
    team_workspace::write_team_memory_entry(
        &app,
        workspace_id.trim(),
        title,
        content,
        author_agent_id,
        tags.unwrap_or_default(),
    )
}

#[tauri::command]
pub(crate) fn workspace_delete_memory(
    app: AppHandle,
    workspace_id: String,
    memory_id: String,
) -> Result<(), String> {
    team_workspace::delete_team_memory_entry(&app, workspace_id.trim(), memory_id.trim())
}

#[tauri::command]
pub(crate) fn workspace_delegate(
    app: AppHandle,
    workspace_id: String,
    target_agent_id: String,
    task: String,
    provider_config: ProviderRuntimeConfig,
) -> Result<String, String> {
    team_workspace::run_delegate_with_provider(
        &app,
        workspace_id.trim(),
        target_agent_id.trim(),
        task.trim(),
        &provider_config,
    )
}

/// 用户请求中止某次委派。当前实现为"软中止"：仅广播状态事件，
/// 让前端把 DelegationCard 置为 aborted；底层 `spawn_blocking` 任务
/// 会自然结束。后续阶段可替换为真正 `AbortHandle`。
#[tauri::command]
pub(crate) async fn workspace_abort_delegate(app: AppHandle, run_id: String) -> Result<(), String> {
    let abort_message = match team_workspace::abort_delegate_run(run_id.trim()) {
        Ok(true) => None,
        Ok(false) => Some("未找到仍在运行的子智能体，已回退为前端中止状态。".to_string()),
        Err(error) => Some(format!("尝试中止子智能体失败：{error}")),
    };
    let _ = app.emit(
        "workspace.delegate.done",
        serde_json::json!({
            "runId": run_id,
            "status": "aborted",
            "error": abort_message,
        }),
    );
    Ok(())
}

/// 用户向正在进行的委派追加补充说明。当前实现为"软追加"：只广播
/// 事件，补充内容会在旁白机制（阶段 E）通过 `[USER_NOTES]` 注入下一轮。
#[tauri::command]
pub(crate) async fn workspace_augment_delegate(
    app: AppHandle,
    workspace_id: String,
    run_id: String,
    note: String,
) -> Result<(), String> {
    let _ = app.emit(
        "workspace.delegate.progress",
        serde_json::json!({
            "runId": run_id,
            "workspaceId": workspace_id,
            "augmentNote": note,
        }),
    );
    let _ = team_workspace::write_team_memory_entry(
        &app,
        workspace_id.trim(),
        format!("委派补充@{}", run_id),
        note,
        None,
        vec!["delegate-note".to_string()],
    );
    Ok(())
}

/// 从委派计划卡片触发的"下发一项"。发出 `workspace.delegate.*` 事件
/// 供前端 DelegationCard 渲染 running/done/error。
///
/// 返回: (run_id, output)。同步版本（阶段 D 再加 abort/augment）。
#[tauri::command]
pub(crate) async fn workspace_run_delegate_task(
    app: AppHandle,
    workspace_id: String,
    session_id: Option<String>,
    assignee: String,
    task: String,
    provider_config: ProviderRuntimeConfig,
) -> Result<serde_json::Value, String> {
    let run_id = uuid::Uuid::new_v4().to_string();
    let ws_id = workspace_id.trim().to_string();
    let target = assignee.trim().to_string();
    let task_text = task.trim().to_string();
    let sess = session_id.unwrap_or_default();

    let started_at = chrono::Utc::now().timestamp_millis();
    let _ = app.emit(
        "workspace.delegate.progress",
        serde_json::json!({
            "runId": run_id,
            "workspaceId": ws_id,
            "sessionId": sess,
            "assignee": target,
            "task": task_text,
            "status": "running",
            "startedAt": started_at,
        }),
    );

    let run_id_clone = run_id.clone();
    let ws_id_clone = ws_id.clone();
    let target_clone = target.clone();
    let task_clone = task_text.clone();
    let app_clone = app.clone();
    let sess_clone = sess.clone();

    let run_id_for_events = run_id.clone();
    let sess_for_events = sess.clone();
    let out = tauri::async_runtime::spawn_blocking(move || {
        team_workspace::run_delegate_with_provider_events(
            &app_clone,
            &ws_id_clone,
            &target_clone,
            &task_clone,
            &provider_config,
            Some(&run_id_for_events),
            if sess_for_events.is_empty() {
                None
            } else {
                Some(&sess_for_events)
            },
        )
    })
    .await
    .map_err(|e| format!("委派任务中断: {e}"))?;

    let elapsed = chrono::Utc::now().timestamp_millis() - started_at;
    match &out {
        Ok(body) => {
            let _ = app.emit(
                "workspace.delegate.done",
                serde_json::json!({
                    "runId": run_id_clone,
                    "workspaceId": ws_id,
                    "sessionId": sess_clone,
                    "assignee": target,
                    "output": body,
                    "elapsedMs": elapsed,
                    "status": "done",
                }),
            );
            Ok(serde_json::json!({
                "runId": run_id_clone,
                "output": body,
                "elapsedMs": elapsed,
                "status": "done",
            }))
        }
        Err(e) => {
            let _ = app.emit(
                "workspace.delegate.error",
                serde_json::json!({
                    "runId": run_id_clone,
                    "workspaceId": ws_id,
                    "sessionId": sess_clone,
                    "assignee": target,
                    "error": e,
                    "elapsedMs": elapsed,
                    "status": "error",
                }),
            );
            Err(e.clone())
        }
    }
}

#[tauri::command]
pub(crate) fn list_scheduled_jobs(
    app: AppHandle,
) -> Result<Vec<scheduler::ScheduledJobRecord>, String> {
    scheduler::list_jobs(&app)
}

#[tauri::command]
pub(crate) fn list_scheduled_job_runs(
    app: AppHandle,
    limit: Option<u32>,
) -> Result<Vec<scheduler::ScheduledJobRunRecord>, String> {
    scheduler::list_job_runs(&app, limit)
}

#[tauri::command]
pub(crate) fn sync_scheduler_jobs(
    app: AppHandle,
) -> Result<scheduler::SchedulerSyncResult, String> {
    scheduler::sync_materialized_jobs(&app)
}

#[tauri::command]
pub(crate) fn trigger_scheduler_job_now(app: AppHandle, job_id: String) -> Result<(), String> {
    scheduler::trigger_job_now(&app, &job_id)
}

#[tauri::command]
pub(crate) fn get_scheduler_status(
    app: AppHandle,
) -> Result<scheduler::SchedulerRuntimeStatus, String> {
    scheduler::runtime_status(&app)
}

#[tauri::command]
pub(crate) fn install_scheduler_service() -> Result<scheduler::SchedulerServiceStatus, String> {
    scheduler::install_service()
}

#[tauri::command]
pub(crate) fn uninstall_scheduler_service() -> Result<scheduler::SchedulerServiceStatus, String> {
    scheduler::uninstall_service()
}

#[tauri::command]
pub(crate) fn list_agent_task_deliveries(
    app: AppHandle,
    session_ids: Vec<String>,
) -> Result<Vec<AgentTaskDeliveryRecord>, String> {
    agent_tasks::list_delivery_records(&app, &session_ids)
}

#[tauri::command]
pub(crate) fn list_agent_tasks(
    app: AppHandle,
    agent_id: Option<String>,
) -> Result<Vec<AgentTaskListItem>, String> {
    agent_tasks::list_tasks(&app, agent_id.as_deref())
}

#[tauri::command]
pub(crate) fn pause_agent_task(app: AppHandle, task_id: String) -> Result<(), String> {
    agent_tasks::pause_task(&app, &task_id)
}

#[tauri::command]
pub(crate) fn resume_agent_task(app: AppHandle, task_id: String) -> Result<(), String> {
    agent_tasks::resume_task(&app, &task_id)
}

#[tauri::command]
pub(crate) fn delete_agent_task(app: AppHandle, task_id: String) -> Result<(), String> {
    agent_tasks::delete_task(&app, &task_id)
}

#[tauri::command]
pub(crate) fn update_agent_task(
    app: AppHandle,
    task_id: String,
    payload: AgentTaskUpdateInput,
) -> Result<(), String> {
    agent_tasks::update_task(&app, &task_id, &payload)
}

#[tauri::command]
pub(crate) fn run_agent_task_now(app: AppHandle, task_id: String) -> Result<(), String> {
    scheduler::trigger_agent_task_now(&app, &task_id)
}
