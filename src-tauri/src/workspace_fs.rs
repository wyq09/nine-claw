//! 团队工作空间磁盘目录：`teams/<workspace_id>/...`

use crate::agent_workspace;
use serde::Serialize;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub fn team_root(workspace_id: &str) -> Result<PathBuf, String> {
    let root = agent_workspace::resolve_workspace_root()?;
    let trimmed = workspace_id.trim();
    if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains('\\') {
        return Err("无效的工作空间 id".to_string());
    }
    Ok(root.join("teams").join(trimmed))
}

pub fn ensure_team_layout(workspace_id: &str) -> Result<PathBuf, String> {
    let root = team_root(workspace_id)?;
    for rel in ["docs", "inbox", "memory/entries", "briefs", "artifacts"] {
        fs::create_dir_all(root.join(rel))
            .map_err(|e| format!("创建工作目录失败 {}: {e}", rel))?;
    }
    let index = root.join("memory").join("SHARED_NOTES.md");
    if !index.is_file() {
        fs::write(
            &index,
            "# 团队共享记忆索引\n\n条目见数据库 `workspace_memories` 与 `memory/entries/`。\n",
        )
        .map_err(|e| format!("写入 SHARED_NOTES 失败: {e}"))?;
    }
    Ok(root)
}

pub fn team_docs_dir(workspace_id: &str) -> Result<PathBuf, String> {
    Ok(team_root(workspace_id)?.join("docs"))
}

pub fn persist_team_doc_file(
    workspace_id: &str,
    original_name: &str,
    data: &[u8],
) -> Result<(PathBuf, String), String> {
    let docs = team_docs_dir(workspace_id)?;
    let safe = sanitize_filename(original_name);
    let dest = docs.join(&safe);
    let mut file = fs::File::create(&dest).map_err(|e| format!("写入资料文件失败: {e}"))?;
    file.write_all(data).map_err(|e| format!("写入资料内容失败: {e}"))?;
    let rel = format!("docs/{safe}");
    Ok((dest, rel))
}

fn resolve_team_rel_path(workspace_id: &str, rel_path: &str) -> Result<(PathBuf, PathBuf), String> {
    let root = team_root(workspace_id)?;
    let normalized = rel_path.trim().trim_start_matches('/');
    if normalized.contains("..") {
        return Err("禁止路径穿越".to_string());
    }
    let path = root.join(normalized);
    if !path.starts_with(&root) {
        return Err("路径越界".to_string());
    }
    Ok((root, path))
}

pub fn read_team_file(workspace_id: &str, rel_path: &str) -> Result<String, String> {
    let (_root, path) = resolve_team_rel_path(workspace_id, rel_path)?;
    fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {e}"))
}

/// 团队空间内资料的绝对路径（用于系统打开 / 媒体预览）。路径须落在 `docs/` 下。
pub fn absolute_team_resource_path(workspace_id: &str, rel_path: &str) -> Result<PathBuf, String> {
    let (_root, path) = resolve_team_rel_path(workspace_id, rel_path)?;
    let normalized = rel_path.trim().trim_start_matches('/');
    if !normalized.starts_with("docs/") {
        return Err("仅允许访问 docs 下的资料文件".to_string());
    }
    if !path.is_file() {
        return Err("文件不存在".to_string());
    }
    Ok(path)
}

pub fn remove_team_resource_file(workspace_id: &str, rel_path: &str) -> Result<(), String> {
    let normalized = rel_path.trim().trim_start_matches('/');
    if !normalized.starts_with("docs/") {
        return Err("只能删除 docs 下的资料文件".to_string());
    }
    let (_root, path) = resolve_team_rel_path(workspace_id, rel_path)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("删除资料文件失败: {e}")),
    }
}

pub fn write_team_brief(workspace_id: &str, agent_id: &str, content: &str) -> Result<PathBuf, String> {
    let root = ensure_team_layout(workspace_id)?;
    let path = root.join("briefs").join(format!("{agent_id}.md"));
    fs::write(&path, content).map_err(|e| format!("写入简报失败: {e}"))?;
    Ok(path)
}

pub fn write_memory_entry_md(workspace_id: &str, memory_id: &str, title: &str, body: &str) -> Result<PathBuf, String> {
    let root = ensure_team_layout(workspace_id)?;
    let path = root.join("memory").join("entries").join(format!("{memory_id}.md"));
    let md = format!("# {title}\n\n{body}\n");
    fs::write(&path, md).map_err(|e| format!("写入记忆文件失败: {e}"))?;
    Ok(path)
}

pub fn remove_memory_entry_md(workspace_id: &str, memory_id: &str) -> Result<(), String> {
    let mid = memory_id.trim();
    if mid.is_empty() || mid.contains('/') || mid.contains('\\') {
        return Err("无效的记忆 id".to_string());
    }
    let root = team_root(workspace_id)?;
    let path = root.join("memory").join("entries").join(format!("{mid}.md"));
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("删除记忆文件失败: {e}")),
    }
}

// ---- 项目成果 artifacts（默认 teams/<id>/artifacts，可配置绝对路径）----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactsTreeEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub size: Option<i64>,
    pub modified_ms: Option<i64>,
}

fn system_time_ms_ms(t: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    t.ok()
        .and_then(|s| s.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
}

/// `artifacts_root_config` 为空时使用 `teams/<id>/artifacts`；否则为绝对路径（不存在则创建）。
pub fn resolve_artifacts_root_path(workspace_id: &str, artifacts_root_config: &str) -> Result<PathBuf, String> {
    let cfg = artifacts_root_config.trim();
    if !cfg.is_empty() {
        let p = PathBuf::from(cfg);
        if !p.is_absolute() {
            return Err("项目成果目录须为绝对路径".to_string());
        }
        fs::create_dir_all(&p).map_err(|e| format!("无法创建或访问成果目录: {e}"))?;
        p.canonicalize()
            .map_err(|e| format!("成果路径无效（请确认路径存在且可访问）: {e}"))
    } else {
        let root = ensure_team_layout(workspace_id)?;
        let art = root.join("artifacts");
        fs::create_dir_all(&art).map_err(|e| format!("创建 artifacts 目录失败: {e}"))?;
        art.canonicalize().map_err(|e| format!("解析 artifacts 路径失败: {e}"))
    }
}

pub fn normalize_artifact_rel(rel: &str) -> Result<String, String> {
    let s = rel.trim().replace('\\', "/");
    let s = s.trim_start_matches('/').trim_end_matches('/');
    if s.contains("..") {
        return Err("非法相对路径".to_string());
    }
    Ok(s.to_string())
}

pub fn list_artifacts_dir_entries(
    workspace_id: &str,
    artifacts_root_config: &str,
    sub_path: &str,
) -> Result<Vec<ArtifactsTreeEntry>, String> {
    let root = resolve_artifacts_root_path(workspace_id, artifacts_root_config)?;
    let sub = normalize_artifact_rel(sub_path)?;
    let dir = if sub.is_empty() {
        root.clone()
    } else {
        root.join(&sub)
    };
    if !dir.starts_with(&root) {
        return Err("路径越界".to_string());
    }
    let meta = fs::metadata(&dir).map_err(|e| format!("访问目录失败: {e}"))?;
    if !meta.is_dir() {
        return Err("不是文件夹".to_string());
    }
    let mut entries = Vec::new();
    for item in fs::read_dir(&dir).map_err(|e| format!("读取目录失败: {e}"))? {
        let item = item.map_err(|e| format!("读取目录项失败: {e}"))?;
        let meta = item.metadata().map_err(|e| format!("读取元数据失败: {e}"))?;
        let name = item.file_name().to_string_lossy().to_string();
        let rel = if sub.is_empty() {
            name.clone()
        } else {
            format!("{sub}/{name}")
        };
        entries.push(ArtifactsTreeEntry {
            name,
            rel_path: rel.replace('\\', "/"),
            is_dir: meta.is_dir(),
            size: if meta.is_file() { Some(meta.len() as i64) } else { None },
            modified_ms: system_time_ms_ms(meta.modified()),
        });
    }
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(entries)
}

const ARTIFACT_TEXT_PREVIEW_MAX: u64 = 4 * 1024 * 1024;

pub fn read_artifact_text_preview(workspace_id: &str, artifacts_root_config: &str, rel: &str) -> Result<String, String> {
    let root = resolve_artifacts_root_path(workspace_id, artifacts_root_config)?;
    let rel = normalize_artifact_rel(rel)?;
    let path = root.join(&rel);
    if !path.starts_with(&root) {
        return Err("路径越界".to_string());
    }
    let meta = fs::metadata(&path).map_err(|e| format!("读取文件失败: {e}"))?;
    if !meta.is_file() {
        return Err("不是文件".to_string());
    }
    if meta.len() > ARTIFACT_TEXT_PREVIEW_MAX {
        return Err(format!(
            "文件超过 {} MB，请使用系统中打开",
            ARTIFACT_TEXT_PREVIEW_MAX / 1024 / 1024
        ));
    }
    fs::read_to_string(&path).map_err(|e| format!("按文本读取失败: {e}"))
}

pub fn artifact_file_absolute_path(workspace_id: &str, artifacts_root_config: &str, rel: &str) -> Result<PathBuf, String> {
    let root = resolve_artifacts_root_path(workspace_id, artifacts_root_config)?;
    let rel = normalize_artifact_rel(rel)?;
    let path = root.join(&rel);
    if !path.starts_with(&root) {
        return Err("路径越界".to_string());
    }
    if !path.is_file() {
        return Err("不是文件".to_string());
    }
    Ok(path)
}

fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("upload.bin");
    let cleaned: String = base
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '\0'))
        .take(200)
        .collect();
    let trimmed = cleaned.trim().to_string();
    if trimmed.is_empty() {
        "upload.bin".to_string()
    } else {
        trimmed
    }
}
