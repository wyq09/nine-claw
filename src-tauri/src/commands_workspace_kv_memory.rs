//! 兼容旧 UI 命名，但底层已经切到 SQLite `user_memories` 主表。

use serde_json::{json, Value};
use tauri::AppHandle;

pub(crate) const GLOBAL_USER_KV_WORKSPACE_ID: &str =
    crate::user_memory_service::GLOBAL_USER_MEMORY_WORKSPACE_ID;

pub(crate) fn resolve_kv_workspace_ui(
    app: &AppHandle,
    workspace_id: Option<String>,
    agent_id: Option<String>,
) -> Result<(String, Option<String>), String> {
    if let Some(ref workspace_id) = workspace_id {
        let workspace_id = workspace_id.trim();
        if !workspace_id.is_empty() {
            let resolved_agent = agent_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string());
            return Ok((workspace_id.to_string(), resolved_agent));
        }
    }

    let resolved_agent = if let Some(ref agent_id) = agent_id {
        let agent_id = agent_id.trim();
        if agent_id.is_empty() {
            crate::agents::get_default_agent(app)?
                .ok_or_else(|| "请先设置默认智能体".to_string())?
                .id
        } else {
            agent_id.to_string()
        }
    } else {
        crate::agents::get_default_agent(app)?
            .ok_or_else(|| "请先设置默认智能体".to_string())?
            .id
    };

    Ok((
        format!("__agent_memory__:{resolved_agent}"),
        Some(resolved_agent),
    ))
}

pub(crate) fn workspace_kv_commit_value(
    app: &AppHandle,
    workspace_id: &str,
    supervisor_fallback: Option<&str>,
    key: &str,
    value: Value,
) -> Result<crate::storage::user_memory::UserMemoryRecord, String> {
    let owner = crate::user_memory_service::resolve_owner(Some(workspace_id), supervisor_fallback)?;
    let value_json =
        serde_json::to_string(&value).map_err(|error| format!("序列化用户记忆失败: {error}"))?;
    let text_value = crate::user_memory_service::build_text_value_for_memory(&value);
    crate::user_memory_service::upsert_entry(
        app,
        &owner,
        key,
        &text_value,
        &[],
        "manual",
        Some("settings_manual"),
        Some(&value_json),
    )
}

#[tauri::command]
pub(crate) fn workspace_kv_memory_ui_list(
    app: AppHandle,
    agent_id: Option<String>,
    workspace_id: Option<String>,
    limit: Option<i64>,
) -> Result<Value, String> {
    let (resolved_workspace_id, resolved_agent_id) =
        resolve_kv_workspace_ui(&app, workspace_id, agent_id)?;
    let owner = crate::user_memory_service::resolve_owner(
        Some(&resolved_workspace_id),
        resolved_agent_id.as_deref(),
    )?;
    let cap = limit.unwrap_or(500).clamp(1, 500);
    let entries = crate::user_memory_service::list_entries(&app, &owner, cap)?
        .into_iter()
        .map(|record| {
            json!({
                "key": record.memory_key,
                "value": record.text,
                "updatedAt": record.updated_at,
                "originKind": record.origin_kind,
                "bucket": record.bucket,
                "tags": serde_json::from_str::<Vec<String>>(&record.tags_json).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "ok": true,
        "workspaceId": resolved_workspace_id,
        "entries": entries,
        "total": entries.len(),
    }))
}

#[tauri::command]
pub(crate) fn workspace_kv_memory_ui_store(
    app: AppHandle,
    agent_id: Option<String>,
    workspace_id: Option<String>,
    key: String,
    value: Value,
) -> Result<Value, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("记忆键不能为空".to_string());
    }
    let (resolved_workspace_id, resolved_agent_id) =
        resolve_kv_workspace_ui(&app, workspace_id, agent_id)?;
    let owner = crate::user_memory_service::resolve_owner(
        Some(&resolved_workspace_id),
        resolved_agent_id.as_deref(),
    )?;
    let value_json =
        serde_json::to_string(&value).map_err(|error| format!("序列化用户记忆失败: {error}"))?;
    let text_value = crate::user_memory_service::build_text_value_for_memory(&value);
    let record = crate::user_memory_service::upsert_entry(
        &app,
        &owner,
        key,
        &text_value,
        &[],
        "manual",
        Some("settings_manual"),
        Some(&value_json),
    )?;

    Ok(json!({
        "ok": true,
        "workspaceId": resolved_workspace_id,
        "key": record.memory_key,
        "value": record.text,
        "updatedAt": record.updated_at,
        "originKind": record.origin_kind,
        "bucket": record.bucket,
    }))
}

#[tauri::command]
pub(crate) fn workspace_kv_memory_ui_forget(
    app: AppHandle,
    agent_id: Option<String>,
    workspace_id: Option<String>,
    key: String,
) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("记忆键不能为空".to_string());
    }
    let (resolved_workspace_id, resolved_agent_id) =
        resolve_kv_workspace_ui(&app, workspace_id, agent_id)?;
    let owner = crate::user_memory_service::resolve_owner(
        Some(&resolved_workspace_id),
        resolved_agent_id.as_deref(),
    )?;
    crate::user_memory_service::delete_entry(&app, &owner, key)
}

#[tauri::command]
pub(crate) async fn workspace_kv_memory_ui_reorganize(
    app: AppHandle,
    agent_id: Option<String>,
    workspace_id: Option<String>,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::user_kv_memory_reorganize::run_user_kv_memory_reorganize(app, agent_id, workspace_id)
    })
    .await
    .map_err(|join| format!("记忆整理线程异常：{join}"))?
}
