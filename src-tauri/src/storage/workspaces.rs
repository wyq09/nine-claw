//! Team workspace (多智能体工作空间) SQLite 层。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn
        .prepare(&pragma)
        .map_err(|e| format!("读取 {table} 表结构失败: {e}"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("解析 {table} 表结构失败: {e}"))?
        .filter_map(Result::ok)
        .any(|name| name == column);
    if exists {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )
    .map_err(|e| format!("补充 {table}.{column} 失败: {e}"))?;
    Ok(())
}

/// 工作空间表 + 依赖 chat_sessions 上的 workspace_id（由 chat_history 迁移添加）。
pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspaces (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            supervisor_agent_id TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            archived INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_workspaces_updated_at ON workspaces(updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_workspaces_archived ON workspaces(archived);

        CREATE TABLE IF NOT EXISTS workspace_members (
            workspace_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            role TEXT NOT NULL,
            added_at INTEGER NOT NULL,
            PRIMARY KEY (workspace_id, agent_id),
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_members_agent ON workspace_members(agent_id);

        CREATE TABLE IF NOT EXISTS workspace_resources (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            file_name TEXT NOT NULL,
            rel_path TEXT NOT NULL,
            mime TEXT NOT NULL DEFAULT '',
            size INTEGER NOT NULL DEFAULT 0,
            uploader_agent_id TEXT,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_resources_ws ON workspace_resources(workspace_id);

        CREATE TABLE IF NOT EXISTS workspace_memories (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            title TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            author_agent_id TEXT,
            tags_json TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_memories_ws ON workspace_memories(workspace_id);
        ",
    )
    .map_err(|e| format!("初始化工作空间表失败: {e}"))?;

    add_column_if_missing(conn, "chat_sessions", "workspace_id", "TEXT")?;
    add_column_if_missing(conn, "chat_turns", "speaker_agent_id", "TEXT")?;
    add_column_if_missing(conn, "workspaces", "artifacts_root", "TEXT NOT NULL DEFAULT ''")?;
    add_column_if_missing(
        conn,
        "workspaces",
        "supervisor_orchestration_prompt",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "llm_trace_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )?;

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub supervisor_agent_id: String,
    /// 空 = 使用 `teams/<id>/artifacts`；否则为自定义绝对路径
    pub artifacts_root: String,
    /// 空 = 使用应用内置「主智能体角色」默认 Markdown；非空则整段注入团队前言（需含标题行，建议 `## 主智能体角色（MUST）`）
    pub supervisor_orchestration_prompt: String,
    /// 开启后，主 Agent↔Pi 以及委派子会话会写入 `teams/<id>/.debug/YYYY-MM-DD.jsonl` 供调试面板查看。
    pub llm_trace_enabled: i32,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMemberRecord {
    pub workspace_id: String,
    pub agent_id: String,
    pub role: String,
    pub added_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceResourceRecord {
    pub id: String,
    pub workspace_id: String,
    pub file_name: String,
    pub rel_path: String,
    pub mime: String,
    pub size: i64,
    pub uploader_agent_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMemoryRecord {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub content: String,
    pub author_agent_id: Option<String>,
    pub tags_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct CreateWorkspaceInput {
    pub id: String,
    pub name: String,
    pub description: String,
    pub supervisor_agent_id: String,
}

pub fn create_workspace(conn: &Connection, input: &CreateWorkspaceInput) -> Result<WorkspaceRecord, String> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO workspaces (id, name, description, supervisor_agent_id, created_at, updated_at, archived)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        params![
            input.id,
            input.name,
            input.description,
            input.supervisor_agent_id,
            now,
            now,
        ],
    )
    .map_err(|e| format!("创建工作空间失败: {e}"))?;

    conn.execute(
        "INSERT INTO workspace_members (workspace_id, agent_id, role, added_at) VALUES (?1, ?2, 'supervisor', ?3)",
        params![input.id, input.supervisor_agent_id, now],
    )
    .map_err(|e| format!("写入主智能体成员失败: {e}"))?;

    get_workspace(conn, &input.id)?.ok_or_else(|| "刚创建的工作空间查询不到".to_string())
}

pub fn update_workspace(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    artifacts_root: Option<&str>,
    supervisor_orchestration_prompt: Option<&str>,
    llm_trace_enabled: Option<bool>,
) -> Result<WorkspaceRecord, String> {
    let mut rec = get_workspace(conn, id)?
        .ok_or_else(|| format!("工作空间 {id} 不存在"))?;
    let now = now_ms();
    if let Some(n) = name {
        rec.name = n.to_string();
    }
    if let Some(d) = description {
        rec.description = d.to_string();
    }
    if let Some(ar) = artifacts_root {
        let trimmed = ar.trim().to_string();
        if !trimmed.is_empty() {
            crate::workspace_fs::resolve_artifacts_root_path(id, &trimmed)?;
        }
        rec.artifacts_root = trimmed;
    }
    if let Some(p) = supervisor_orchestration_prompt {
        rec.supervisor_orchestration_prompt = p.to_string();
    }
    if let Some(flag) = llm_trace_enabled {
        rec.llm_trace_enabled = if flag { 1 } else { 0 };
    }
    rec.updated_at = now;
    conn.execute(
        "UPDATE workspaces SET name = ?1, description = ?2, artifacts_root = ?3, supervisor_orchestration_prompt = ?4, llm_trace_enabled = ?5, updated_at = ?6 WHERE id = ?7",
        params![
            rec.name,
            rec.description,
            rec.artifacts_root,
            rec.supervisor_orchestration_prompt,
            rec.llm_trace_enabled,
            now,
            id
        ],
    )
    .map_err(|e| format!("更新工作空间失败: {e}"))?;
    get_workspace(conn, id)?.ok_or_else(|| "更新后查询失败".to_string())
}

pub fn set_workspace_archived(conn: &Connection, id: &str, archived: bool) -> Result<(), String> {
    let now = now_ms();
    conn.execute(
        "UPDATE workspaces SET archived = ?1, updated_at = ?2 WHERE id = ?3",
        params![if archived { 1 } else { 0 }, now, id],
    )
    .map_err(|e| format!("归档工作空间失败: {e}"))?;
    Ok(())
}

fn row_workspace(row: &rusqlite::Row) -> rusqlite::Result<WorkspaceRecord> {
    Ok(WorkspaceRecord {
        id: row.get("id")?,
        name: row.get("name")?,
        description: row.get("description")?,
        supervisor_agent_id: row.get("supervisor_agent_id")?,
        artifacts_root: row.get::<_, Option<String>>("artifacts_root")?.unwrap_or_default(),
        supervisor_orchestration_prompt: row
            .get::<_, Option<String>>("supervisor_orchestration_prompt")?
            .unwrap_or_default(),
        llm_trace_enabled: row
            .get::<_, Option<i32>>("llm_trace_enabled")?
            .unwrap_or(0),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        archived: row.get("archived")?,
    })
}

pub fn get_workspace(conn: &Connection, id: &str) -> Result<Option<WorkspaceRecord>, String> {
    conn.query_row(
        "SELECT id, name, description, supervisor_agent_id, artifacts_root, supervisor_orchestration_prompt, llm_trace_enabled, created_at, updated_at, archived FROM workspaces WHERE id = ?1",
        params![id],
        row_workspace,
    )
    .optional()
    .map_err(|e| format!("查询工作空间失败: {e}"))
}

pub fn list_workspaces(conn: &Connection, include_archived: bool) -> Result<Vec<WorkspaceRecord>, String> {
    let sql = if include_archived {
        "SELECT id, name, description, supervisor_agent_id, artifacts_root, supervisor_orchestration_prompt, llm_trace_enabled, created_at, updated_at, archived FROM workspaces ORDER BY updated_at DESC"
    } else {
        "SELECT id, name, description, supervisor_agent_id, artifacts_root, supervisor_orchestration_prompt, llm_trace_enabled, created_at, updated_at, archived FROM workspaces WHERE archived = 0 ORDER BY updated_at DESC"
    };
    let mut stmt = conn.prepare(sql).map_err(|e| format!("准备查询失败: {e}"))?;
    let rows = stmt
        .query_map([], row_workspace)
        .map_err(|e| format!("列出工作空间失败: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("读取行失败: {e}"))?);
    }
    Ok(out)
}

fn row_member(row: &rusqlite::Row) -> rusqlite::Result<WorkspaceMemberRecord> {
    Ok(WorkspaceMemberRecord {
        workspace_id: row.get("workspace_id")?,
        agent_id: row.get("agent_id")?,
        role: row.get("role")?,
        added_at: row.get("added_at")?,
    })
}

pub fn list_workspace_members(conn: &Connection, workspace_id: &str) -> Result<Vec<WorkspaceMemberRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT workspace_id, agent_id, role, added_at FROM workspace_members WHERE workspace_id = ?1 ORDER BY added_at ASC",
        )
        .map_err(|e| format!("准备查询成员失败: {e}"))?;
    let rows = stmt
        .query_map(params![workspace_id], row_member)
        .map_err(|e| format!("查询成员失败: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("读取成员行失败: {e}"))?);
    }
    Ok(out)
}

pub fn add_workspace_member(
    conn: &Connection,
    workspace_id: &str,
    agent_id: &str,
    role: &str,
) -> Result<(), String> {
    let _ = get_workspace(conn, workspace_id)?.ok_or_else(|| "工作空间不存在".to_string())?;
    let now = now_ms();
    conn.execute(
        "INSERT INTO workspace_members (workspace_id, agent_id, role, added_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(workspace_id, agent_id) DO UPDATE SET role = excluded.role",
        params![workspace_id, agent_id, role, now],
    )
    .map_err(|e| format!("添加成员失败: {e}"))?;
    conn.execute(
        "UPDATE workspaces SET updated_at = ?1 WHERE id = ?2",
        params![now, workspace_id],
    )
    .map_err(|e| format!("更新时间戳失败: {e}"))?;
    Ok(())
}

pub fn remove_workspace_member(conn: &Connection, workspace_id: &str, agent_id: &str) -> Result<(), String> {
    let sup: String = conn
        .query_row(
            "SELECT supervisor_agent_id FROM workspaces WHERE id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("查询工作空间失败: {e}"))?
        .ok_or_else(|| "工作空间不存在".to_string())?;
    if agent_id == sup {
        return Err("不能移除主智能体".to_string());
    }
    let now = now_ms();
    conn.execute(
        "DELETE FROM workspace_members WHERE workspace_id = ?1 AND agent_id = ?2",
        params![workspace_id, agent_id],
    )
    .map_err(|e| format!("移除成员失败: {e}"))?;
    conn.execute(
        "UPDATE workspaces SET updated_at = ?1 WHERE id = ?2",
        params![now, workspace_id],
    )
    .map_err(|e| format!("更新时间戳失败: {e}"))?;
    Ok(())
}

fn row_resource(row: &rusqlite::Row) -> rusqlite::Result<WorkspaceResourceRecord> {
    Ok(WorkspaceResourceRecord {
        id: row.get("id")?,
        workspace_id: row.get("workspace_id")?,
        file_name: row.get("file_name")?,
        rel_path: row.get("rel_path")?,
        mime: row.get("mime")?,
        size: row.get("size")?,
        uploader_agent_id: row.get("uploader_agent_id")?,
        created_at: row.get("created_at")?,
    })
}

pub fn insert_workspace_resource(
    conn: &Connection,
    id: &str,
    workspace_id: &str,
    file_name: &str,
    rel_path: &str,
    mime: &str,
    size: i64,
    uploader_agent_id: Option<&str>,
) -> Result<WorkspaceResourceRecord, String> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO workspace_resources (id, workspace_id, file_name, rel_path, mime, size, uploader_agent_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, workspace_id, file_name, rel_path, mime, size, uploader_agent_id, now],
    )
    .map_err(|e| format!("写入资料记录失败: {e}"))?;
    conn.execute(
        "UPDATE workspaces SET updated_at = ?1 WHERE id = ?2",
        params![now, workspace_id],
    )
    .map_err(|e| format!("更新时间戳失败: {e}"))?;
    conn.query_row(
        "SELECT id, workspace_id, file_name, rel_path, mime, size, uploader_agent_id, created_at FROM workspace_resources WHERE id = ?1",
        params![id],
        row_resource,
    )
    .map_err(|e| format!("查询资料失败: {e}"))
}

pub fn list_workspace_resources(conn: &Connection, workspace_id: &str) -> Result<Vec<WorkspaceResourceRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, workspace_id, file_name, rel_path, mime, size, uploader_agent_id, created_at FROM workspace_resources WHERE workspace_id = ?1 ORDER BY created_at DESC",
        )
        .map_err(|e| format!("准备查询资料失败: {e}"))?;
    let rows = stmt
        .query_map(params![workspace_id], row_resource)
        .map_err(|e| format!("查询资料失败: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("读取资料行失败: {e}"))?);
    }
    Ok(out)
}

/// `rel_path` 形如 `docs/...`。先删文件再调 `delete_workspace_resource_row`，避免库记录已无而文件仍在。
pub fn get_workspace_resource_rel_path(
    conn: &Connection,
    workspace_id: &str,
    resource_id: &str,
) -> Result<String, String> {
    let wid = workspace_id.trim();
    let rid = resource_id.trim();
    if wid.is_empty() || rid.is_empty() {
        return Err("缺少工作空间或资料 id".to_string());
    }
    conn.query_row(
        "SELECT rel_path FROM workspace_resources WHERE workspace_id = ?1 AND id = ?2",
        params![wid, rid],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| format!("查询资料失败: {e}"))?
    .ok_or_else(|| "资料不存在或已删除".to_string())
}

pub fn delete_workspace_resource_row(conn: &Connection, workspace_id: &str, resource_id: &str) -> Result<(), String> {
    let wid = workspace_id.trim();
    let rid = resource_id.trim();
    let n = conn
        .execute(
            "DELETE FROM workspace_resources WHERE workspace_id = ?1 AND id = ?2",
            params![wid, rid],
        )
        .map_err(|e| format!("删除资料记录失败: {e}"))?;
    if n == 0 {
        return Err("资料不存在或已删除".to_string());
    }
    let now = now_ms();
    conn.execute(
        "UPDATE workspaces SET updated_at = ?1 WHERE id = ?2",
        params![now, wid],
    )
    .map_err(|e| format!("更新时间戳失败: {e}"))?;
    Ok(())
}

fn row_memory(row: &rusqlite::Row) -> rusqlite::Result<WorkspaceMemoryRecord> {
    Ok(WorkspaceMemoryRecord {
        id: row.get("id")?,
        workspace_id: row.get("workspace_id")?,
        title: row.get("title")?,
        content: row.get("content")?,
        author_agent_id: row.get("author_agent_id")?,
        tags_json: row.get("tags_json")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub fn insert_workspace_memory(
    conn: &Connection,
    id: &str,
    workspace_id: &str,
    title: &str,
    content: &str,
    author_agent_id: Option<&str>,
    tags_json: &str,
) -> Result<WorkspaceMemoryRecord, String> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO workspace_memories (id, workspace_id, title, content, author_agent_id, tags_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, workspace_id, title, content, author_agent_id, tags_json, now, now],
    )
    .map_err(|e| format!("写入共享记忆失败: {e}"))?;
    conn.execute(
        "UPDATE workspaces SET updated_at = ?1 WHERE id = ?2",
        params![now, workspace_id],
    )
    .map_err(|e| format!("更新时间戳失败: {e}"))?;
    conn.query_row(
        "SELECT id, workspace_id, title, content, author_agent_id, tags_json, created_at, updated_at FROM workspace_memories WHERE id = ?1",
        params![id],
        row_memory,
    )
    .map_err(|e| format!("查询记忆失败: {e}"))
}

pub fn list_workspace_memories(conn: &Connection, workspace_id: &str, limit: i64) -> Result<Vec<WorkspaceMemoryRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, workspace_id, title, content, author_agent_id, tags_json, created_at, updated_at FROM workspace_memories WHERE workspace_id = ?1 ORDER BY updated_at DESC LIMIT ?2",
        )
        .map_err(|e| format!("准备查询记忆失败: {e}"))?;
    let rows = stmt
        .query_map(params![workspace_id, limit], row_memory)
        .map_err(|e| format!("查询记忆失败: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("读取记忆行失败: {e}"))?);
    }
    Ok(out)
}

pub fn update_workspace_memory(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    content: Option<&str>,
    tags_json: Option<&str>,
) -> Result<WorkspaceMemoryRecord, String> {
    let mut rec = conn
        .query_row(
            "SELECT id, workspace_id, title, content, author_agent_id, tags_json, created_at, updated_at FROM workspace_memories WHERE id = ?1",
            params![id],
            row_memory,
        )
        .optional()
        .map_err(|e| format!("查询记忆失败: {e}"))?
        .ok_or_else(|| "记忆不存在".to_string())?;
    let now = now_ms();
    if let Some(t) = title {
        rec.title = t.to_string();
    }
    if let Some(c) = content {
        rec.content = c.to_string();
    }
    if let Some(tj) = tags_json {
        rec.tags_json = tj.to_string();
    }
    rec.updated_at = now;
    conn.execute(
        "UPDATE workspace_memories SET title = ?1, content = ?2, tags_json = ?3, updated_at = ?4 WHERE id = ?5",
        params![rec.title, rec.content, rec.tags_json, now, id],
    )
    .map_err(|e| format!("更新记忆失败: {e}"))?;
    conn.execute(
        "UPDATE workspaces SET updated_at = ?1 WHERE id = (SELECT workspace_id FROM workspace_memories WHERE id = ?2)",
        params![now, id],
    )
    .map_err(|e| format!("更新时间戳失败: {e}"))?;
    conn.query_row(
        "SELECT id, workspace_id, title, content, author_agent_id, tags_json, created_at, updated_at FROM workspace_memories WHERE id = ?1",
        params![id],
        row_memory,
    )
    .map_err(|e| format!("查询记忆失败: {e}"))
}

pub fn delete_workspace_memory(conn: &Connection, workspace_id: &str, memory_id: &str) -> Result<(), String> {
    let wid = workspace_id.trim();
    let mid = memory_id.trim();
    if wid.is_empty() || mid.is_empty() {
        return Err("缺少工作空间或记忆 id".to_string());
    }
    let n = conn
        .execute(
            "DELETE FROM workspace_memories WHERE workspace_id = ?1 AND id = ?2",
            params![wid, mid],
        )
        .map_err(|e| format!("删除共享记忆失败: {e}"))?;
    if n == 0 {
        return Err("记忆不存在或已被删除".to_string());
    }
    let now = now_ms();
    conn.execute(
        "UPDATE workspaces SET updated_at = ?1 WHERE id = ?2",
        params![now, wid],
    )
    .map_err(|e| format!("更新时间戳失败: {e}"))?;
    Ok(())
}
