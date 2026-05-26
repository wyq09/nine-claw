use crate::session_llm_log;

#[tauri::command]
pub(crate) fn session_llm_log_get(
    workspace_id: Option<String>,
    session_id: String,
) -> Result<session_llm_log::SessionLlmLogDetail, String> {
    session_llm_log::get(workspace_id.as_deref(), &session_id)
}

#[tauri::command]
pub(crate) fn session_llm_log_list(
    workspace_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<session_llm_log::SessionLlmLogInfo>, String> {
    session_llm_log::list(workspace_id.as_deref(), limit.unwrap_or(100))
}

#[tauri::command]
pub(crate) fn session_llm_log_clear(
    workspace_id: Option<String>,
    session_id: String,
) -> Result<(), String> {
    session_llm_log::clear(workspace_id.as_deref(), &session_id)
}
