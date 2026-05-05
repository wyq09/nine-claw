use crate::storage::user_memory::{
    self, UserMemoryRecord, GLOBAL_OWNER_ID, OWNER_SCOPE_AGENT, OWNER_SCOPE_GLOBAL,
};
use serde_json::Value;
use tauri::AppHandle;

pub const USER_MEMORY_KEY_PREFIX: &str = "nc_um.";
pub const GLOBAL_USER_MEMORY_WORKSPACE_ID: &str = "__nc_user_kv_global__";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserMemoryOwner {
    pub owner_scope: String,
    pub owner_id: String,
}

impl UserMemoryOwner {
    pub fn global() -> Self {
        Self {
            owner_scope: OWNER_SCOPE_GLOBAL.to_string(),
            owner_id: GLOBAL_OWNER_ID.to_string(),
        }
    }

    pub fn agent(agent_id: &str) -> Self {
        Self {
            owner_scope: OWNER_SCOPE_AGENT.to_string(),
            owner_id: agent_id.trim().to_string(),
        }
    }

    pub fn vector_namespace(&self) -> String {
        match self.owner_scope.as_str() {
            OWNER_SCOPE_GLOBAL => user_memory::global_vector_namespace().to_string(),
            OWNER_SCOPE_AGENT => user_memory::agent_vector_namespace(&self.owner_id),
            _ => user_memory::global_vector_namespace().to_string(),
        }
    }
}

pub fn resolve_owner(
    workspace_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<UserMemoryOwner, String> {
    let workspace_id = workspace_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if workspace_id == Some(GLOBAL_USER_MEMORY_WORKSPACE_ID) {
        return Ok(UserMemoryOwner::global());
    }
    let agent_id = agent_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "请先设置默认智能体".to_string())?;
    Ok(UserMemoryOwner::agent(agent_id))
}

pub fn bucket_from_key(key: &str) -> String {
    let key = key.trim();
    let Some(rest) = key.strip_prefix(USER_MEMORY_KEY_PREFIX) else {
        return "other".to_string();
    };
    rest.split('/').next().unwrap_or("other").trim().to_string()
}

pub fn normalize_tags(bucket: &str, tags: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let bucket = bucket.trim().to_ascii_lowercase();
    if !bucket.is_empty() {
        out.push(bucket.clone());
    }
    for tag in tags {
        let tag = tag.trim().to_ascii_lowercase();
        if tag.is_empty() || out.contains(&tag) {
            continue;
        }
        out.push(tag);
    }
    out
}

pub fn build_text_value_for_memory(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::String(text) => text.trim().to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn index_user_memory_async(
    app: AppHandle,
    owner: UserMemoryOwner,
    memory_id: String,
    text: String,
) {
    let Some(registry) = crate::managed_runtime::get_embedding_registry() else {
        return;
    };
    let namespace = owner.vector_namespace();
    tauri::async_runtime::spawn(async move {
        let provider = {
            let guard = registry.read().await;
            guard.default_provider()
        };
        let Some(provider) = provider else {
            return;
        };
        match provider.embed(vec![text]).await {
            Ok(embeddings) => {
                if let Some(embedding) = embeddings.into_iter().next() {
                    if let Ok(conn) = crate::storage_conn(&app) {
                        let vector_id = format!("vec_{}", uuid::Uuid::new_v4().simple());
                        let _ = crate::memory_vector::upsert_vector(
                            &conn,
                            &vector_id,
                            &user_memory::vector_memory_id(&memory_id),
                            &namespace,
                            &embedding,
                            provider.id(),
                        );
                    }
                }
            }
            Err(error) => log::warn!("用户记忆向量写入失败: {error}"),
        }
    });
}

pub fn list_entries(
    app: &AppHandle,
    owner: &UserMemoryOwner,
    limit: i64,
) -> Result<Vec<UserMemoryRecord>, String> {
    let conn = crate::storage_conn(app)?;
    user_memory::list_user_memories(&conn, &owner.owner_scope, &owner.owner_id, limit)
}

pub fn upsert_entry(
    app: &AppHandle,
    owner: &UserMemoryOwner,
    memory_key: &str,
    text: &str,
    tags: &[String],
    origin_kind: &str,
    source_ref: Option<&str>,
    detail_json: Option<&str>,
) -> Result<UserMemoryRecord, String> {
    let bucket = bucket_from_key(memory_key);
    let tags = normalize_tags(&bucket, tags);
    let tags_json =
        serde_json::to_string(&tags).map_err(|error| format!("序列化用户记忆标签失败: {error}"))?;
    let conn = crate::storage_conn(app)?;
    let record = user_memory::upsert_user_memory(
        &conn,
        &owner.owner_scope,
        &owner.owner_id,
        memory_key,
        &bucket,
        text.trim(),
        &tags_json,
        origin_kind,
        source_ref,
        detail_json,
    )?;
    index_user_memory_async(
        app.clone(),
        owner.clone(),
        record.id.clone(),
        format!("{}\n{}", record.bucket, record.text),
    );
    Ok(record)
}

pub fn delete_entry(
    app: &AppHandle,
    owner: &UserMemoryOwner,
    memory_key: &str,
) -> Result<(), String> {
    let conn = crate::storage_conn(app)?;
    if let Some(record) =
        user_memory::get_user_memory(&conn, &owner.owner_scope, &owner.owner_id, memory_key)?
    {
        let _ = crate::memory_vector::delete_vector_by_memory_id(
            &conn,
            &user_memory::vector_memory_id(&record.id),
        )?;
    }
    user_memory::delete_user_memory(&conn, &owner.owner_scope, &owner.owner_id, memory_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_from_key_uses_prefix_segment() {
        assert_eq!(bucket_from_key("nc_um.identity/123"), "identity");
        assert_eq!(bucket_from_key("nc_um.directive/abc"), "directive");
        assert_eq!(bucket_from_key("legacy-key"), "other");
    }

    #[test]
    fn normalize_tags_keeps_bucket_first() {
        let tags = normalize_tags(
            "writing",
            &[
                "Writing".to_string(),
                "tone".to_string(),
                "tone".to_string(),
            ],
        );
        assert_eq!(tags, vec!["writing".to_string(), "tone".to_string()]);
    }
}
