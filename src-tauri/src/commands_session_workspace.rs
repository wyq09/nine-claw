use crate::history_app_state::storage_conn;
use crate::session_workspace::{
    SessionWorkspaceEntry, SessionWorkspaceFileInfo, SessionWorkspaceReadResult,
    SessionWorkspaceState,
};
use tauri::AppHandle;

#[tauri::command]
pub(crate) fn session_workspace_get(
    app: AppHandle,
    session_id: String,
) -> Result<SessionWorkspaceState, String> {
    let conn = storage_conn(&app)?;
    let session = crate::storage::chat_history::get_chat_session(&conn, session_id.trim())?
        .ok_or_else(|| "会话不存在".to_string())?;
    crate::session_workspace::ensure_state(&conn, session_id.trim(), session.workspace_id.as_deref())
}

#[tauri::command]
pub(crate) fn session_workspace_switch(
    app: AppHandle,
    session_id: String,
    dir: String,
) -> Result<SessionWorkspaceState, String> {
    let conn = storage_conn(&app)?;
    crate::session_workspace::switch_current_dir(&conn, session_id.trim(), dir.trim())
}

#[tauri::command]
pub(crate) fn session_workspace_reset_to_topic(
    app: AppHandle,
    session_id: String,
) -> Result<SessionWorkspaceState, String> {
    let conn = storage_conn(&app)?;
    crate::session_workspace::reset_current_to_topic(&conn, session_id.trim())
}

#[tauri::command]
pub(crate) fn session_workspace_list_entries(
    app: AppHandle,
    session_id: String,
    sub_path: Option<String>,
) -> Result<Vec<SessionWorkspaceEntry>, String> {
    let conn = storage_conn(&app)?;
    crate::session_workspace::list_entries(&conn, session_id.trim(), sub_path.as_deref())
}

#[tauri::command]
pub(crate) fn session_workspace_read_file(
    app: AppHandle,
    session_id: String,
    rel_path: String,
) -> Result<SessionWorkspaceReadResult, String> {
    let conn = storage_conn(&app)?;
    crate::session_workspace::read_file(&conn, session_id.trim(), rel_path.trim())
}

#[tauri::command]
pub(crate) fn session_workspace_absolute_path(
    app: AppHandle,
    session_id: String,
    rel_path: String,
    allow_dir: Option<bool>,
) -> Result<String, String> {
    let conn = storage_conn(&app)?;
    let path = crate::session_workspace::absolute_path(
        &conn,
        session_id.trim(),
        rel_path.trim(),
        allow_dir.unwrap_or(false),
    )?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub(crate) fn session_workspace_create_file(
    app: AppHandle,
    session_id: String,
    parent_rel: Option<String>,
    name: String,
    content: Option<String>,
) -> Result<SessionWorkspaceFileInfo, String> {
    let conn = storage_conn(&app)?;
    crate::session_workspace::create_file(
        &conn,
        session_id.trim(),
        parent_rel.as_deref(),
        name.trim(),
        content.as_deref(),
    )
}

#[tauri::command]
pub(crate) fn session_workspace_create_dir(
    app: AppHandle,
    session_id: String,
    parent_rel: Option<String>,
    name: String,
) -> Result<SessionWorkspaceFileInfo, String> {
    let conn = storage_conn(&app)?;
    crate::session_workspace::create_dir(
        &conn,
        session_id.trim(),
        parent_rel.as_deref(),
        name.trim(),
    )
}

#[tauri::command]
pub(crate) fn session_workspace_rename(
    app: AppHandle,
    session_id: String,
    rel_path: String,
    new_name: String,
) -> Result<SessionWorkspaceFileInfo, String> {
    let conn = storage_conn(&app)?;
    crate::session_workspace::rename_path(
        &conn,
        session_id.trim(),
        rel_path.trim(),
        new_name.trim(),
    )
}

#[tauri::command]
pub(crate) fn session_workspace_delete(
    app: AppHandle,
    session_id: String,
    rel_path: String,
) -> Result<(), String> {
    let conn = storage_conn(&app)?;
    crate::session_workspace::delete_path(&conn, session_id.trim(), rel_path.trim())
}

#[tauri::command]
pub(crate) fn session_workspace_open_path(
    app: AppHandle,
    session_id: String,
    rel_path: String,
) -> Result<(), String> {
    let conn = storage_conn(&app)?;
    let path = crate::session_workspace::absolute_path(&conn, session_id.trim(), rel_path.trim(), true)?;
    crate::open_path_in_default_app(&path)
}

#[tauri::command]
pub(crate) fn session_workspace_reveal_path(
    app: AppHandle,
    session_id: String,
    rel_path: String,
) -> Result<(), String> {
    let conn = storage_conn(&app)?;
    let path = crate::session_workspace::absolute_path(&conn, session_id.trim(), rel_path.trim(), true)?;
    crate::reveal_path_in_file_manager(&path)
}

#[tauri::command]
pub(crate) fn session_workspace_import_files(
    app: AppHandle,
    session_id: String,
    source_paths: Vec<String>,
    parent_rel: Option<String>,
) -> Result<Vec<SessionWorkspaceFileInfo>, String> {
    let conn = storage_conn(&app)?;
    crate::session_workspace::import_files(
        &conn,
        session_id.trim(),
        &source_paths,
        parent_rel.as_deref(),
    )
}
