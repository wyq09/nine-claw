use crate::history_app_state::storage_conn;
use crate::storage::workspaces;
use serde::Serialize;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStats {
    pub system_count: i64,
    pub workspace_count: i64,
    pub agent_count: i64,
}

#[tauri::command]
pub(crate) fn memory_list(
    app: AppHandle,
    workspace_id: String,
    scope: Option<String>,
    limit: Option<i64>,
    page: Option<i64>,
) -> Result<Vec<workspaces::WorkspaceMemoryRecord>, String> {
    let conn = storage_conn(&app)?;
    let limit = limit.unwrap_or(30).min(200);
    let page = page.unwrap_or(1).max(1);
    let offset = (page - 1) * limit;

    let memories = if let Some(ref scope_val) = scope {
        match scope_val.as_str() {
            "system" => {
                let all = workspaces::list_system_memories(&conn, limit + offset)?;
                all.into_iter().skip(offset as usize).collect()
            }
            _ => {
                workspaces::list_memories_by_scope(&conn, &workspace_id, scope_val, limit + offset)?
                    .into_iter()
                    .skip(offset as usize)
                    .collect()
            }
        }
    } else {
        workspaces::list_workspace_memories(&conn, &workspace_id, limit + offset, scope.as_deref())?
            .into_iter()
            .skip(offset as usize)
            .collect()
    };
    Ok(memories)
}

#[tauri::command]
pub(crate) fn memory_update_scope(
    app: AppHandle,
    memory_id: String,
    scope: String,
    scope_agent_id: Option<String>,
) -> Result<(), String> {
    let conn = storage_conn(&app)?;
    let valid_scopes = ["system", "workspace", "agent"];
    if !valid_scopes.contains(&scope.as_str()) {
        return Err(format!("无效的 scope 值: {scope}，必须是 system/workspace/agent"));
    }
    workspaces::update_memory_scope(&conn, &memory_id, &scope, scope_agent_id.as_deref())
}

#[tauri::command]
pub(crate) fn memory_stats(
    app: AppHandle,
    workspace_id: String,
) -> Result<MemoryStats, String> {
    let conn = storage_conn(&app)?;
    let (system, workspace, agent) = workspaces::count_memories_by_scope(&conn, &workspace_id)?;
    Ok(MemoryStats {
        system_count: system,
        workspace_count: workspace,
        agent_count: agent,
    })
}

#[tauri::command]
pub(crate) fn memory_search_text(
    app: AppHandle,
    workspace_id: String,
    query: String,
    scope: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<workspaces::WorkspaceMemoryRecord>, String> {
    let conn = storage_conn(&app)?;
    let limit = limit.unwrap_or(20).min(100);
    let query_lower = query.to_lowercase();

    let all = workspaces::list_workspace_memories(&conn, &workspace_id, 500, None)?;
    let system = workspaces::list_system_memories(&conn, 500)?;

    let mut combined: Vec<workspaces::WorkspaceMemoryRecord> = all;
    combined.extend(system);

    let results: Vec<workspaces::WorkspaceMemoryRecord> = combined
        .into_iter()
        .filter(|m| {
            if let Some(ref s) = scope {
                if m.scope != *s {
                    return false;
                }
            }
            let title_match = m.title.to_lowercase().contains(&query_lower);
            let content_match = m.content.to_lowercase().contains(&query_lower);
            let tags_match = m.tags_json.to_lowercase().contains(&query_lower);
            title_match || content_match || tags_match
        })
        .take(limit as usize)
        .collect();

    Ok(results)
}
