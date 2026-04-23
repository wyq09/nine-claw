use crate::llm_log_export;
use tauri::AppHandle;

#[tauri::command]
pub(crate) fn llm_log_export_get() -> Option<String> {
    llm_log_export::get_dir()
}

#[tauri::command]
pub(crate) fn llm_log_export_set(app: AppHandle, path: Option<String>) -> Result<(), String> {
    llm_log_export::set_dir(&app, path)
}

#[tauri::command]
pub(crate) fn llm_log_export_preview() -> Result<llm_log_export::LlmLogPreview, String> {
    llm_log_export::preview_latest()
}
