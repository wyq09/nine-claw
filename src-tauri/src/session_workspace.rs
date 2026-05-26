//! Session Workspace filesystem helpers.

use crate::{agent_workspace, storage, workspace_fs};
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

const TEXT_PREVIEW_MAX: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWorkspaceState {
    pub session_id: String,
    pub topic_workspace_dir: String,
    pub current_workspace_dir: String,
    pub current_is_topic: bool,
    pub recents: Vec<SessionWorkspaceRecentView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWorkspaceRecentView {
    pub path: String,
    pub last_used_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWorkspaceEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub size: Option<i64>,
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWorkspaceFileInfo {
    pub name: String,
    pub rel_path: String,
    pub absolute_path: String,
    pub is_dir: bool,
    pub size: Option<i64>,
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWorkspaceReadResult {
    pub info: SessionWorkspaceFileInfo,
    pub content: Option<String>,
    pub preview_kind: String,
}

pub fn ensure_state(
    conn: &Connection,
    session_id: &str,
    workspace_id: Option<&str>,
) -> Result<SessionWorkspaceState, String> {
    let sid = normalize_session_id(session_id)?;
    let existing = storage::session_workspace::get_session_workspace_dirs(conn, &sid)?
        .ok_or_else(|| "会话不存在".to_string())?;
    let topic = existing
        .topic_workspace_dir
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let current = existing
        .current_workspace_dir
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);

    let topic = match topic {
        Some(path) => ensure_directory(path)?,
        None => default_topic_workspace_dir(conn, &sid, workspace_id)?,
    };
    let current = match current {
        Some(path) => ensure_directory(path)?,
        None => topic.clone(),
    };

    storage::session_workspace::set_session_workspace_dirs(
        conn,
        &sid,
        &display_path(&topic),
        &display_path(&current),
    )?;
    build_state(conn, &sid, topic, current)
}

pub fn switch_current_dir(
    conn: &Connection,
    session_id: &str,
    dir: &str,
) -> Result<SessionWorkspaceState, String> {
    let sid = normalize_session_id(session_id)?;
    let selected = ensure_directory(PathBuf::from(dir.trim()))?;
    storage::session_workspace::set_current_workspace_dir(conn, &sid, &display_path(&selected))?;
    let dirs = storage::session_workspace::get_session_workspace_dirs(conn, &sid)?
        .ok_or_else(|| "会话不存在".to_string())?;
    let topic = dirs
        .topic_workspace_dir
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| "会话缺少话题工作区".to_string())?;
    build_state(conn, &sid, topic, selected)
}

pub fn reset_current_to_topic(
    conn: &Connection,
    session_id: &str,
) -> Result<SessionWorkspaceState, String> {
    let sid = normalize_session_id(session_id)?;
    let dirs = storage::session_workspace::get_session_workspace_dirs(conn, &sid)?
        .ok_or_else(|| "会话不存在".to_string())?;
    let topic = dirs
        .topic_workspace_dir
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| "会话缺少话题工作区".to_string())?;
    let topic = ensure_directory(topic)?;
    storage::session_workspace::set_current_workspace_dir(conn, &sid, &display_path(&topic))?;
    build_state(conn, &sid, topic.clone(), topic)
}

pub fn current_workspace_dir(
    conn: &Connection,
    session_id: &str,
    workspace_id: Option<&str>,
) -> Result<PathBuf, String> {
    let inferred_workspace_id = if workspace_id.is_none() {
        storage::chat_history::get_chat_session(conn, session_id)?
            .and_then(|session| session.workspace_id)
    } else {
        None
    };
    let state = ensure_state(conn, session_id, workspace_id.or(inferred_workspace_id.as_deref()))?;
    Ok(PathBuf::from(state.current_workspace_dir))
}

pub fn list_entries(
    conn: &Connection,
    session_id: &str,
    sub_path: Option<&str>,
) -> Result<Vec<SessionWorkspaceEntry>, String> {
    let root = current_workspace_dir(conn, session_id, None)?;
    list_entries_at_root(&root, sub_path.unwrap_or(""))
}

pub fn read_file(
    conn: &Connection,
    session_id: &str,
    rel_path: &str,
) -> Result<SessionWorkspaceReadResult, String> {
    let root = current_workspace_dir(conn, session_id, None)?;
    let path = resolve_existing(&root, rel_path)?;
    let info = file_info(&root, &path)?;
    let preview_kind = preview_kind(&info.name, info.is_dir);
    let content = if info.is_dir || matches!(preview_kind.as_str(), "image" | "binary") {
        None
    } else {
        let meta = fs::metadata(&path).map_err(|e| format!("读取文件失败: {e}"))?;
        if meta.len() > TEXT_PREVIEW_MAX {
            return Err(format!(
                "文件超过 {} MB，请使用系统中打开",
                TEXT_PREVIEW_MAX / 1024 / 1024
            ));
        }
        Some(fs::read_to_string(&path).map_err(|e| format!("按文本读取失败: {e}"))?)
    };
    Ok(SessionWorkspaceReadResult {
        info,
        content,
        preview_kind,
    })
}

pub fn absolute_path(
    conn: &Connection,
    session_id: &str,
    rel_path: &str,
    allow_dir: bool,
) -> Result<PathBuf, String> {
    let root = current_workspace_dir(conn, session_id, None)?;
    let path = resolve_existing(&root, rel_path)?;
    if !allow_dir && !path.is_file() {
        return Err("不是文件".to_string());
    }
    Ok(path)
}

pub fn create_file(
    conn: &Connection,
    session_id: &str,
    parent_rel: Option<&str>,
    name: &str,
    content: Option<&str>,
) -> Result<SessionWorkspaceFileInfo, String> {
    let root = current_workspace_dir(conn, session_id, None)?;
    let parent = resolve_dir(&root, parent_rel.unwrap_or(""))?;
    let target = parent.join(safe_child_name(name)?);
    if target.exists() {
        return Err("文件已存在".to_string());
    }
    let mut file = fs::File::create(&target).map_err(|e| format!("创建文件失败: {e}"))?;
    file.write_all(content.unwrap_or("").as_bytes())
        .map_err(|e| format!("写入文件失败: {e}"))?;
    file_info(&root, &target)
}

pub fn create_dir(
    conn: &Connection,
    session_id: &str,
    parent_rel: Option<&str>,
    name: &str,
) -> Result<SessionWorkspaceFileInfo, String> {
    let root = current_workspace_dir(conn, session_id, None)?;
    let parent = resolve_dir(&root, parent_rel.unwrap_or(""))?;
    let target = parent.join(safe_child_name(name)?);
    fs::create_dir(&target).map_err(|e| format!("创建文件夹失败: {e}"))?;
    file_info(&root, &target)
}

pub fn rename_path(
    conn: &Connection,
    session_id: &str,
    rel_path: &str,
    new_name: &str,
) -> Result<SessionWorkspaceFileInfo, String> {
    let root = current_workspace_dir(conn, session_id, None)?;
    let path = resolve_existing(&root, rel_path)?;
    if path == root {
        return Err("不能重命名工作区根目录".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "无法解析父目录".to_string())?;
    let target = parent.join(safe_child_name(new_name)?);
    if target.exists() {
        return Err("目标已存在".to_string());
    }
    fs::rename(&path, &target).map_err(|e| format!("重命名失败: {e}"))?;
    file_info(&root, &target)
}

pub fn delete_path(conn: &Connection, session_id: &str, rel_path: &str) -> Result<(), String> {
    let root = current_workspace_dir(conn, session_id, None)?;
    let path = resolve_existing(&root, rel_path)?;
    if path == root {
        return Err("不能删除工作区根目录".to_string());
    }
    let meta = fs::metadata(&path).map_err(|e| format!("读取路径失败: {e}"))?;
    if meta.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| format!("删除文件夹失败: {e}"))
    } else {
        fs::remove_file(&path).map_err(|e| format!("删除文件失败: {e}"))
    }
}

pub fn import_files(
    conn: &Connection,
    session_id: &str,
    source_paths: &[String],
    parent_rel: Option<&str>,
) -> Result<Vec<SessionWorkspaceFileInfo>, String> {
    if source_paths.is_empty() {
        return Err("未选择文件".to_string());
    }
    let root = current_workspace_dir(conn, session_id, None)?;
    let parent = resolve_dir(&root, parent_rel.unwrap_or(""))?;
    let mut imported = Vec::with_capacity(source_paths.len());
    for source_path in source_paths {
        let source = PathBuf::from(source_path.trim());
        if !source.is_absolute() {
            return Err(format!("必须使用绝对路径: {}", source.display()));
        }
        let canonical = source
            .canonicalize()
            .map_err(|e| format!("解析源文件失败: {e}"))?;
        if !canonical.is_file() {
            return Err(format!("不是文件: {}", canonical.display()));
        }
        let file_name = canonical
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "无法解析文件名".to_string())?;
        let safe_name = safe_child_name(file_name)?;
        let target = unique_path_in_dir(&parent, &safe_name)?;
        fs::copy(&canonical, &target).map_err(|e| {
            format!(
                "复制文件失败 {} -> {}: {e}",
                canonical.display(),
                target.display()
            )
        })?;
        imported.push(file_info(&root, &target)?);
    }
    Ok(imported)
}

pub fn persist_inbound_file(
    root: &Path,
    scope: &str,
    file_name: &str,
    data: &[u8],
) -> Result<PathBuf, String> {
    let root = ensure_directory(root.to_path_buf())?;
    let inbox_dir = root.join("chat-inbox").join(current_date_label());
    fs::create_dir_all(&inbox_dir).map_err(|e| format!("创建会话收件目录失败: {e}"))?;
    let safe_scope = safe_segment(scope, "session");
    let safe_name = safe_file_name(file_name, "attachment.bin");
    let path = inbox_dir.join(format!(
        "{}-{}-{}",
        current_timestamp_file_label(),
        safe_scope,
        safe_name
    ));
    fs::write(&path, data).map_err(|e| format!("写入会话附件失败: {e}"))?;
    Ok(path)
}

pub fn create_default_for_new_session(
    conn: &Connection,
    session_id: &str,
    workspace_id: Option<&str>,
) -> Result<SessionWorkspaceState, String> {
    let sid = normalize_session_id(session_id)?;
    let topic = default_topic_workspace_dir(conn, &sid, workspace_id)?;
    storage::session_workspace::set_session_workspace_dirs(
        conn,
        &sid,
        &display_path(&topic),
        &display_path(&topic),
    )?;
    build_state(conn, &sid, topic.clone(), topic)
}

fn build_state(
    conn: &Connection,
    session_id: &str,
    topic: PathBuf,
    current: PathBuf,
) -> Result<SessionWorkspaceState, String> {
    let recents = storage::session_workspace::list_recents(conn, session_id, 12)?
        .into_iter()
        .map(|item| SessionWorkspaceRecentView {
            path: item.path,
            last_used_at: item.last_used_at,
        })
        .collect();
    let topic_s = display_path(&topic);
    let current_s = display_path(&current);
    Ok(SessionWorkspaceState {
        session_id: session_id.to_string(),
        current_is_topic: topic_s == current_s,
        topic_workspace_dir: topic_s,
        current_workspace_dir: current_s,
        recents,
    })
}

fn default_topic_workspace_dir(
    conn: &Connection,
    session_id: &str,
    workspace_id: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(wid) = workspace_id.map(str::trim).filter(|v| !v.is_empty()) {
        let artifacts_root = storage::workspaces::get_workspace(conn, wid)?
            .map(|workspace| workspace.artifacts_root)
            .unwrap_or_default();
        let team_artifacts = workspace_fs::resolve_artifacts_root_path(wid, &artifacts_root)?;
        return ensure_directory(
            team_artifacts
                .join("sessions")
                .join(safe_segment(session_id, "session")),
        );
    }
    let root = agent_workspace::resolve_workspace_root()?;
    ensure_directory(root.join("chat-sessions").join(safe_segment(session_id, "session")))
}

fn list_entries_at_root(root: &Path, sub_path: &str) -> Result<Vec<SessionWorkspaceEntry>, String> {
    let dir = resolve_dir(root, sub_path)?;
    let sub = normalize_rel(sub_path)?;
    let mut entries = Vec::new();
    for item in fs::read_dir(&dir).map_err(|e| format!("读取目录失败: {e}"))? {
        let item = item.map_err(|e| format!("读取目录项失败: {e}"))?;
        let meta = item
            .metadata()
            .map_err(|e| format!("读取元数据失败: {e}"))?;
        let name = item.file_name().to_string_lossy().to_string();
        let rel = if sub.is_empty() {
            name.clone()
        } else {
            format!("{sub}/{name}")
        };
        entries.push(SessionWorkspaceEntry {
            name,
            rel_path: rel.replace('\\', "/"),
            is_dir: meta.is_dir(),
            size: if meta.is_file() {
                Some(meta.len() as i64)
            } else {
                None
            },
            modified_ms: meta
                .modified()
                .ok()
                .and_then(|s| s.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64),
        });
    }
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(entries)
}

fn file_info(root: &Path, path: &Path) -> Result<SessionWorkspaceFileInfo, String> {
    let meta = fs::metadata(path).map_err(|e| format!("读取路径失败: {e}"))?;
    let rel = path
        .strip_prefix(root)
        .map_err(|_| "路径越界".to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(SessionWorkspaceFileInfo {
        name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string(),
        rel_path: rel,
        absolute_path: display_path(path),
        is_dir: meta.is_dir(),
        size: if meta.is_file() {
            Some(meta.len() as i64)
        } else {
            None
        },
        modified_ms: meta
            .modified()
            .ok()
            .and_then(|s| s.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64),
    })
}

fn resolve_existing(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let normalized = normalize_rel(rel)?;
    let path = if normalized.is_empty() {
        root.to_path_buf()
    } else {
        root.join(normalized)
    };
    assert_inside(root, &path)?;
    if !path.exists() {
        return Err("路径不存在".to_string());
    }
    Ok(path)
}

fn resolve_dir(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let path = resolve_existing(root, rel)?;
    if !path.is_dir() {
        return Err("不是文件夹".to_string());
    }
    Ok(path)
}

fn assert_inside(root: &Path, path: &Path) -> Result<(), String> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err("路径越界".to_string())
    }
}

fn normalize_rel(rel: &str) -> Result<String, String> {
    let raw = rel.trim().replace('\\', "/");
    if raw.starts_with('/') {
        return Err("必须使用相对路径".to_string());
    }
    let mut parts = Vec::new();
    for component in Path::new(&raw).components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("禁止路径穿越".to_string());
            }
        }
    }
    Ok(parts.join("/"))
}

fn ensure_directory(path: PathBuf) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err("工作区目录必须是绝对路径".to_string());
    }
    fs::create_dir_all(&path).map_err(|e| format!("创建工作区目录失败: {e}"))?;
    path.canonicalize()
        .map_err(|e| format!("解析工作区目录失败: {e}"))
}

fn safe_child_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("名称不能为空".to_string());
    }
    if trimmed == "." || trimmed == ".." || trimmed.contains('/') || trimmed.contains('\\') {
        return Err("名称不能包含路径分隔符".to_string());
    }
    Ok(safe_file_name(trimmed, "untitled"))
}

fn unique_path_in_dir(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let initial = dir.join(name);
    if !initial.exists() {
        return Ok(initial);
    }
    let path_obj = Path::new(name);
    let stem = path_obj
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let ext = path_obj
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    for index in 1..1000 {
        let candidate = dir.join(format!("{stem}-{index}{ext}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("无法生成唯一文件名".to_string())
}

fn normalize_session_id(session_id: &str) -> Result<String, String> {
    let sid = session_id.trim();
    if sid.is_empty() {
        return Err("缺少会话 id".to_string());
    }
    Ok(sid.to_string())
}

fn preview_kind(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "folder".to_string();
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "md" | "markdown" => "markdown",
        "html" | "htm" => "html",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "svg" => "image",
        "php" | "py" | "ts" | "tsx" | "js" | "jsx" | "css" | "json" | "rs" | "go" | "java"
        | "kt" | "swift" | "sh" | "zsh" | "toml" | "yaml" | "yml" | "xml" | "sql" => "code",
        "txt" | "log" | "csv" | "env" | "ini" => "text",
        _ => "binary",
    }
    .to_string()
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn current_date_label() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn current_timestamp_file_label() -> String {
    chrono::Local::now().format("%H%M%S%.3f").to_string()
}

fn safe_segment(value: &str, fallback: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(['-', '.', '_']).to_string();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}

fn safe_file_name(value: &str, fallback: &str) -> String {
    let base = Path::new(value)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(fallback);
    let cleaned: String = base
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '\0'))
        .take(200)
        .collect();
    let trimmed = cleaned.trim().to_string();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::chat_history::{create_chat_session, CreateChatSessionInput};

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "nineclaw-session-workspace-{name}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn create_session(conn: &Connection, id: &str, workspace_id: Option<&str>) {
        create_chat_session(
            conn,
            &CreateChatSessionInput {
                id: id.to_string(),
                title: id.to_string(),
                status: "running".to_string(),
                agent_id: None,
                agent_snapshot_json: None,
                bot_target_json: None,
                session_llm_provider_id: None,
                session_llm_model: None,
                workspace_id: workspace_id.map(ToOwned::to_owned),
            },
        )
        .unwrap();
    }

    #[test]
    fn standalone_session_uses_chat_sessions_root() {
        let _guard = crate::workspace_env_test_lock();
        let root = temp_root("standalone");
        std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);
        let conn = storage::db::open_in_memory().unwrap();
        create_session(&conn, "s1", None);

        let state = create_default_for_new_session(&conn, "s1", None).unwrap();
        assert!(state.topic_workspace_dir.ends_with("chat-sessions/s1"));
        assert_eq!(state.topic_workspace_dir, state.current_workspace_dir);
        assert!(Path::new(&state.topic_workspace_dir).is_dir());

        let _ = fs::remove_dir_all(root);
        std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
    }

    #[test]
    fn team_session_uses_artifacts_sessions_root() {
        let _guard = crate::workspace_env_test_lock();
        let root = temp_root("team");
        std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);
        let conn = storage::db::open_in_memory().unwrap();
        storage::workspaces::create_workspace(
            &conn,
            &storage::workspaces::CreateWorkspaceInput {
                id: "ws1".to_string(),
                name: "Team".to_string(),
                description: String::new(),
                supervisor_agent_id: "agent-1".to_string(),
            },
        )
        .unwrap();
        create_session(&conn, "s2", Some("ws1"));

        let state = create_default_for_new_session(&conn, "s2", Some("ws1")).unwrap();
        assert!(state.topic_workspace_dir.ends_with("teams/ws1/artifacts/sessions/s2"));
        assert!(Path::new(&state.topic_workspace_dir).is_dir());

        let _ = fs::remove_dir_all(root);
        std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
    }

    #[test]
    fn team_session_respects_custom_artifacts_root() {
        let _guard = crate::workspace_env_test_lock();
        let root = temp_root("team-custom-root");
        let custom_artifacts = temp_root("team-custom-artifacts");
        std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);
        let conn = storage::db::open_in_memory().unwrap();
        storage::workspaces::create_workspace(
            &conn,
            &storage::workspaces::CreateWorkspaceInput {
                id: "ws-custom".to_string(),
                name: "Team".to_string(),
                description: String::new(),
                supervisor_agent_id: "agent-1".to_string(),
            },
        )
        .unwrap();
        storage::workspaces::update_workspace(
            &conn,
            "ws-custom",
            None,
            None,
            Some(custom_artifacts.to_string_lossy().as_ref()),
            None,
            None,
        )
        .unwrap();
        create_session(&conn, "s-custom", Some("ws-custom"));

        let state = create_default_for_new_session(&conn, "s-custom", Some("ws-custom")).unwrap();
        let canonical_custom = custom_artifacts.canonicalize().unwrap();
        assert!(
            Path::new(&state.topic_workspace_dir).starts_with(&canonical_custom),
            "{} should be under {}",
            state.topic_workspace_dir,
            canonical_custom.display()
        );
        assert!(state.topic_workspace_dir.ends_with("sessions/s-custom"));

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(custom_artifacts);
        std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
    }

    #[test]
    fn lazy_initializes_missing_dirs() {
        let _guard = crate::workspace_env_test_lock();
        let root = temp_root("lazy");
        std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);
        let conn = storage::db::open_in_memory().unwrap();
        create_session(&conn, "old-session", None);

        let state = ensure_state(&conn, "old-session", None).unwrap();
        assert!(state.topic_workspace_dir.ends_with("chat-sessions/old-session"));
        assert!(Path::new(&state.current_workspace_dir).is_dir());

        let _ = fs::remove_dir_all(root);
        std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
    }

    #[test]
    fn file_ops_reject_path_traversal() {
        let root = temp_root("paths");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "hello").unwrap();

        assert!(list_entries_at_root(&root, "../").is_err());
        assert!(resolve_existing(&root, "../a.txt").is_err());
        assert!(safe_child_name("../x").is_err());

        let list = list_entries_at_root(&root, "").unwrap();
        assert_eq!(list[0].rel_path, "a.txt");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn import_files_copies_into_workspace_with_unique_names() {
        let _guard = crate::workspace_env_test_lock();
        let root = temp_root("import");
        std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);
        let conn = storage::db::open_in_memory().unwrap();
        create_session(&conn, "s4", None);
        let state = create_default_for_new_session(&conn, "s4", None).unwrap();

        let external = temp_root("external");
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("demo.txt"), "hello").unwrap();
        fs::write(
            PathBuf::from(&state.current_workspace_dir).join("demo.txt"),
            "existing",
        )
        .unwrap();

        let imported = import_files(
            &conn,
            "s4",
            &[external.join("demo.txt").to_string_lossy().to_string()],
            None,
        )
        .unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "demo-1.txt");
        assert_eq!(
            fs::read_to_string(imported[0].absolute_path.clone()).unwrap(),
            "hello"
        );

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(external);
        std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
    }

    #[test]
    fn create_rename_delete_file_inside_workspace() {
        let _guard = crate::workspace_env_test_lock();
        let root = temp_root("ops");
        std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);
        let conn = storage::db::open_in_memory().unwrap();
        create_session(&conn, "s3", None);
        create_default_for_new_session(&conn, "s3", None).unwrap();

        let created = create_file(&conn, "s3", None, "note.md", Some("# Hi")).unwrap();
        assert_eq!(created.rel_path, "note.md");
        let renamed = rename_path(&conn, "s3", "note.md", "renamed.md").unwrap();
        assert_eq!(renamed.rel_path, "renamed.md");
        delete_path(&conn, "s3", "renamed.md").unwrap();
        assert!(read_file(&conn, "s3", "renamed.md").is_err());

        let _ = fs::remove_dir_all(root);
        std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
    }

    #[test]
    fn inbound_file_persists_under_chat_inbox() {
        let root = temp_root("inbox");
        let path = persist_inbound_file(&root, "s1", "demo.txt", b"demo").unwrap();
        let canonical_root = root.canonicalize().unwrap();
        assert!(path.starts_with(&canonical_root));
        assert!(path.to_string_lossy().contains("chat-inbox"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "demo");
        let _ = fs::remove_dir_all(root);
    }
}
