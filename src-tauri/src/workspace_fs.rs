//! 团队工作空间磁盘目录：`teams/<workspace_id>/...`

use crate::agent_workspace;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

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
    for rel in ["docs", "inbox", "memory/entries", "briefs"] {
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

pub fn read_team_file(workspace_id: &str, rel_path: &str) -> Result<String, String> {
    let root = team_root(workspace_id)?;
    let normalized = rel_path.trim().trim_start_matches('/');
    if normalized.contains("..") {
        return Err("禁止路径穿越".to_string());
    }
    let path = root.join(normalized);
    if !path.starts_with(&root) {
        return Err("路径越界".to_string());
    }
    fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {e}"))
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
