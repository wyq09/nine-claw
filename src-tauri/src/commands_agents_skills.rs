use crate::agent_workspace::AgentWorkspaceBundle;
use crate::agents::{self, AgentInput, AgentRecord};
use crate::skills::{InstalledSkill, SystemSkillCatalog};
use tauri::AppHandle;

#[tauri::command]
pub(crate) fn list_installed_skills() -> Result<Vec<InstalledSkill>, String> {
    crate::skills::list_installed_skills()
}

#[tauri::command]
pub(crate) fn list_system_skill_catalog(app: AppHandle) -> Result<SystemSkillCatalog, String> {
    crate::skills::list_system_skill_catalog_for_app(&app)
}

#[tauri::command]
pub(crate) fn install_system_skill(
    app: AppHandle,
    skill_id: String,
) -> Result<InstalledSkill, String> {
    crate::skills::install_system_skill(&app, &skill_id)
}

#[tauri::command]
pub(crate) fn list_agents(app: AppHandle) -> Result<Vec<AgentRecord>, String> {
    agents::list_agents(&app)
}

#[tauri::command]
pub(crate) fn get_default_agent(app: AppHandle) -> Result<Option<AgentRecord>, String> {
    agents::get_default_agent(&app)
}

#[tauri::command]
pub(crate) fn create_agent(app: AppHandle, payload: AgentInput) -> Result<AgentRecord, String> {
    agents::create_agent(&app, payload)
}

#[tauri::command]
pub(crate) fn update_agent(
    app: AppHandle,
    agent_id: String,
    payload: AgentInput,
) -> Result<AgentRecord, String> {
    agents::update_agent(&app, agent_id, payload)
}

#[tauri::command]
pub(crate) fn archive_agent(app: AppHandle, agent_id: String) -> Result<(), String> {
    agents::archive_agent(&app, agent_id)
}

#[tauri::command]
pub(crate) fn delete_agent(app: AppHandle, agent_id: String) -> Result<(), String> {
    agents::delete_agent(&app, agent_id)
}

#[tauri::command]
pub(crate) fn migrate_agent_id(
    app: AppHandle,
    old_agent_id: String,
    new_agent_id: String,
) -> Result<String, String> {
    agents::migrate_agent_id(&app, old_agent_id, new_agent_id)
}

#[tauri::command]
pub(crate) fn set_default_agent(
    app: AppHandle,
    agent_id: String,
) -> Result<Option<AgentRecord>, String> {
    agents::set_default_agent(&app, agent_id)
}

#[tauri::command]
pub(crate) fn list_default_agent_presets(
    app: AppHandle,
) -> Result<Vec<crate::agent_presets::AgentPresetSummary>, String> {
    agents::list_default_agent_presets(&app)
}

#[tauri::command]
pub(crate) fn reset_agent_to_default_preset(
    app: AppHandle,
    agent_id: String,
) -> Result<AgentRecord, String> {
    agents::reset_agent_to_default_preset(&app, agent_id)
}

#[tauri::command]
pub(crate) fn read_agent_workspace_bundle(
    app: AppHandle,
    agent_id: String,
) -> Result<AgentWorkspaceBundle, String> {
    agents::read_agent_workspace_bundle(&app, agent_id)
}

#[tauri::command]
pub(crate) fn read_agent_workspace_file(
    app: AppHandle,
    agent_id: String,
    relative_path: String,
) -> Result<crate::agent_workspace::AgentWorkspaceFile, String> {
    agents::read_agent_workspace_file(&app, agent_id, relative_path)
}

#[tauri::command]
pub(crate) fn write_agent_workspace_file(
    app: AppHandle,
    agent_id: String,
    relative_path: String,
    content: String,
) -> Result<AgentWorkspaceBundle, String> {
    agents::write_agent_workspace_file(&app, agent_id, relative_path, content)
}

#[tauri::command]
pub(crate) fn export_agent_package(
    app: AppHandle,
    agent_id: String,
    dest_path: String,
    include_secrets: Option<bool>,
    include_shared_root: bool,
) -> Result<(), String> {
    crate::agent_package::export_agent_package_to_path(
        &app,
        agent_id,
        dest_path,
        include_secrets.unwrap_or(false),
        include_shared_root,
    )
}

#[tauri::command]
pub(crate) fn import_agent_package(
    app: AppHandle,
    package_path: String,
) -> Result<crate::agent_package::AgentImportResult, String> {
    crate::agent_package::import_agent_package_from_path(&app, package_path)
}
