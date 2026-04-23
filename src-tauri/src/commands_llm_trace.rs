use crate::history_app_state::storage_conn;
use crate::llm_trace;
use crate::storage;
use tauri::AppHandle;

#[tauri::command]
pub(crate) fn workspace_llm_trace_status(
    app: AppHandle,
    workspace_id: String,
) -> Result<bool, String> {
    let conn = storage_conn(&app)?;
    let Some(ws) = storage::workspaces::get_workspace(&conn, workspace_id.trim())? else {
        return Err("工作空间不存在".to_string());
    };
    Ok(ws.llm_trace_enabled != 0)
}

#[tauri::command]
pub(crate) fn workspace_llm_trace_set_enabled(
    app: AppHandle,
    workspace_id: String,
    enabled: bool,
) -> Result<storage::workspaces::WorkspaceRecord, String> {
    let conn = storage_conn(&app)?;
    storage::workspaces::update_workspace(
        &conn,
        workspace_id.trim(),
        None,
        None,
        None,
        None,
        Some(enabled),
    )
}

#[tauri::command]
pub(crate) fn llm_trace_list(
    workspace_id: Option<String>,
    session_id: Option<String>,
    days: Option<usize>,
    limit: Option<usize>,
) -> Result<Vec<llm_trace::TraceEntry>, String> {
    let days = days.unwrap_or(3).clamp(1, 14);
    let limit = limit.unwrap_or(100).clamp(1, 500);
    Ok(llm_trace::list_recent(
        workspace_id.as_deref(),
        session_id.as_deref(),
        days,
        limit,
    ))
}

#[tauri::command]
pub(crate) fn llm_trace_clear(
    workspace_id: Option<String>,
    session_id: Option<String>,
) -> Result<(), String> {
    llm_trace::clear(workspace_id.as_deref(), session_id.as_deref())
}
