use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub manifest_path: String,
    pub scope: String,
    pub install_type: String,
    pub updated_at: i64,
    pub source: Option<String>,
    pub source_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSkillCatalog {
    pub available: bool,
    pub updated_at: Option<i64>,
    pub message: String,
    pub skills: Vec<SystemSkillItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSkillItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub install_url: Option<String>,
    pub installed: bool,
}

#[derive(Debug, Clone)]
struct SkillRoot {
    path: PathBuf,
    scope: &'static str,
    lock_path: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
struct SkillLockFile {
    #[serde(default)]
    skills: HashMap<String, SkillLockEntry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SkillLockEntry {
    source: Option<String>,
    #[serde(rename = "sourceType")]
    source_type: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SkillManifest {
    name: Option<String>,
    description: Option<String>,
}

pub fn list_installed_skills() -> Result<Vec<InstalledSkill>, String> {
    scan_skill_roots(&candidate_skill_roots())
}

pub fn resolve_skill_directories(skill_ids: &[String]) -> Result<Vec<PathBuf>, String> {
    if skill_ids.is_empty() {
        return Ok(Vec::new());
    }

    let installed = list_installed_skills()?;
    let mut available_by_id = installed
        .into_iter()
        .map(|skill| (skill.id, PathBuf::from(skill.path)))
        .collect::<HashMap<_, _>>();
    for (skill_id, path) in list_runtime_system_skill_directories()? {
        available_by_id.entry(skill_id).or_insert(path);
    }

    let mut resolved = Vec::new();
    let mut seen = HashSet::new();

    for skill_id in skill_ids {
        let Some(path) = available_by_id.get(skill_id) else {
            continue;
        };

        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if seen.insert(canonical) {
            resolved.push(path.clone());
        }
    }

    Ok(resolved)
}

fn list_runtime_system_skill_directories() -> Result<HashMap<String, PathBuf>, String> {
    let mut items = HashMap::new();
    for root in discover_runtime_system_skill_roots() {
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let skill_id = entry.file_name().to_string_lossy().trim().to_string();
            if skill_id.is_empty() || skill_id.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.join("SKILL.md").is_file() {
                items.entry(skill_id).or_insert(path);
            }
        }
    }
    Ok(items)
}

pub fn list_system_skill_catalog_for_app(app: &AppHandle) -> Result<SystemSkillCatalog, String> {
    let roots = system_skill_roots(app)?;
    Ok(list_system_skill_catalog_with_roots(&roots))
}

pub fn install_system_skill(app: &AppHandle, skill_id: &str) -> Result<InstalledSkill, String> {
    let trimmed_skill_id = skill_id.trim();
    if trimmed_skill_id.is_empty() {
        return Err("技能 ID 不能为空".to_string());
    }

    let target_root = preferred_global_skill_root()?;
    fs::create_dir_all(&target_root)
        .map_err(|error| format!("创建全局技能目录失败 {}: {error}", target_root.display()))?;

    let source_path = resolve_system_skill_path(app, trimmed_skill_id)?
        .ok_or_else(|| format!("系统技能 {trimmed_skill_id} 不存在"))?;
    let target_path = target_root.join(trimmed_skill_id);

    if target_path.exists() {
        remove_existing_skill_path(&target_path)?;
    }

    copy_dir_recursive(&source_path, &target_path)?;

    let installed = scan_skill_roots(&candidate_skill_roots())?;
    installed
        .into_iter()
        .find(|skill| skill.id == trimmed_skill_id)
        .ok_or_else(|| format!("安装完成，但未在技能目录中找到 {trimmed_skill_id}"))
}

fn list_system_skill_catalog_with_roots(roots: &[PathBuf]) -> SystemSkillCatalog {
    let installed_ids = list_installed_skills()
        .map(|skills| {
            skills
                .into_iter()
                .map(|skill| skill.id)
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();

    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for root in roots {
        if !root.exists() || !root.is_dir() {
            continue;
        }

        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let skill_id = entry.file_name().to_string_lossy().trim().to_string();
            if skill_id.is_empty() || skill_id.starts_with('.') || !seen.insert(skill_id.clone()) {
                continue;
            }

            let skill_path = entry.path();
            let manifest_path = skill_path.join("SKILL.md");
            if !manifest_path.is_file() {
                continue;
            }

            let manifest_text = match fs::read_to_string(&manifest_path) {
                Ok(content) => content,
                Err(_) => continue,
            };
            let manifest = parse_skill_manifest(&manifest_text);

            items.push(SystemSkillItem {
                id: skill_id.clone(),
                name: manifest
                    .name
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(skill_id.clone()),
                description: manifest
                    .description
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "暂无描述".to_string()),
                install_url: Some(format!("system://{skill_id}")),
                installed: installed_ids.contains(&skill_id),
            });
        }
    }

    items.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));

    let updated_at = roots
        .iter()
        .filter_map(|root| fs::metadata(root).ok())
        .filter_map(|metadata| metadata.modified().ok())
        .map(system_time_to_ms)
        .max();

    if items.is_empty() {
        SystemSkillCatalog {
            available: false,
            updated_at,
            message: "当前构建中未找到可安装的系统技能。".to_string(),
            skills: Vec::new(),
        }
    } else {
        SystemSkillCatalog {
            available: true,
            updated_at,
            message: "系统技能可一键安装到全局技能目录，安装后可直接分配给智能体。".to_string(),
            skills: items,
        }
    }
}

fn system_skill_roots(app: &AppHandle) -> Result<Vec<PathBuf>, String> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();

    if let Some(dev_root) = discover_dev_system_skill_root() {
        let canonical = fs::canonicalize(&dev_root).unwrap_or(dev_root.clone());
        if seen.insert(canonical) {
            roots.push(dev_root);
        }
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        let resource_root = resource_dir.join("system-skills");
        if resource_root.is_dir() {
            let canonical = fs::canonicalize(&resource_root).unwrap_or(resource_root.clone());
            if seen.insert(canonical) {
                roots.push(resource_root);
            }
        }
    }

    Ok(roots)
}

fn discover_dev_system_skill_root() -> Option<PathBuf> {
    let mut start_points = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        start_points.push(current_dir);
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            start_points.push(parent.to_path_buf());
        }
    }

    for start_point in start_points {
        for ancestor in start_point.ancestors() {
            let candidate = ancestor
                .join("src-tauri")
                .join("resources")
                .join("system-skills");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }

    None
}

fn discover_runtime_system_skill_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();

    if let Some(dev_root) = discover_dev_system_skill_root() {
        let canonical = fs::canonicalize(&dev_root).unwrap_or(dev_root.clone());
        if seen.insert(canonical) {
            roots.push(dev_root);
        }
    }

    let mut start_points = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        start_points.push(current_dir);
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            start_points.push(parent.to_path_buf());
        }
    }

    for start_point in start_points {
        for ancestor in start_point.ancestors() {
            for candidate in [
                ancestor.join("system-skills"),
                ancestor.join("resources").join("system-skills"),
                ancestor.join("Resources").join("system-skills"),
            ] {
                if !candidate.is_dir() {
                    continue;
                }
                let canonical = fs::canonicalize(&candidate).unwrap_or(candidate.clone());
                if seen.insert(canonical) {
                    roots.push(candidate);
                }
            }
        }
    }

    roots
}

fn resolve_system_skill_path(app: &AppHandle, skill_id: &str) -> Result<Option<PathBuf>, String> {
    for root in system_skill_roots(app)? {
        let candidate = root.join(skill_id);
        if candidate.join("SKILL.md").is_file() {
            return Ok(Some(candidate));
        }
    }

    Ok(None)
}

fn preferred_global_skill_root() -> Result<PathBuf, String> {
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        return Ok(PathBuf::from(codex_home).join("skills"));
    }

    let home = home_dir().ok_or_else(|| "无法定位用户主目录".to_string())?;
    Ok(home.join(".codex").join("skills"))
}

fn remove_existing_skill_path(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("读取已安装技能失败 {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)
            .map_err(|error| format!("删除已安装技能失败 {}: {error}", path.display()))?;
    } else {
        fs::remove_dir_all(path)
            .map_err(|error| format!("删除已安装技能目录失败 {}: {error}", path.display()))?;
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target)
        .map_err(|error| format!("创建目标技能目录失败 {}: {error}", target.display()))?;

    let entries = fs::read_dir(source)
        .map_err(|error| format!("读取系统技能目录失败 {}: {error}", source.display()))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("读取系统技能目录条目失败 {}: {error}", source.display()))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = fs::metadata(&source_path)
            .map_err(|error| format!("读取系统技能文件失败 {}: {error}", source_path.display()))?;
        if metadata.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            fs::copy(&source_path, &target_path).map_err(|error| {
                format!(
                    "复制系统技能文件失败 {} -> {}: {error}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        }
    }

    Ok(())
}

fn scan_skill_roots(roots: &[SkillRoot]) -> Result<Vec<InstalledSkill>, String> {
    let mut skills = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_paths = HashSet::new();

    for root in roots {
        if !root.path.exists() || !root.path.is_dir() {
            continue;
        }

        let lock_entries = load_skill_lock_entries(root.lock_path.as_deref());
        let entries = fs::read_dir(&root.path)
            .map_err(|error| format!("读取技能目录失败 {}: {error}", root.path.display()))?;

        for entry in entries {
            let entry = entry.map_err(|error| {
                format!("读取技能目录条目失败 {}: {error}", root.path.display())
            })?;
            let skill_id = entry.file_name().to_string_lossy().trim().to_string();
            if skill_id.is_empty() || skill_id.starts_with('.') {
                continue;
            }

            let skill_path = entry.path();
            let manifest_path = skill_path.join("SKILL.md");
            if !manifest_path.is_file() {
                continue;
            }

            if !seen_ids.insert(skill_id.clone()) {
                continue;
            }

            let canonical_skill_path =
                fs::canonicalize(&skill_path).unwrap_or_else(|_| skill_path.clone());
            if !seen_paths.insert(canonical_skill_path) {
                continue;
            }

            let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
                format!("读取技能说明失败 {}: {error}", manifest_path.display())
            })?;
            let manifest = parse_skill_manifest(&manifest_text);
            let metadata = fs::symlink_metadata(&skill_path)
                .map_err(|error| format!("读取技能元数据失败 {}: {error}", skill_path.display()))?;

            let updated_at = fs::metadata(&manifest_path)
                .and_then(|item| item.modified())
                .map(system_time_to_ms)
                .unwrap_or_default();
            let install_type = if metadata.file_type().is_symlink() {
                "symlink"
            } else {
                "directory"
            };
            let name = manifest
                .name
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| skill_id.clone());
            let description = manifest
                .description
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "暂无描述".to_string());
            let lock_entry = lock_entries.get(&skill_id).cloned().unwrap_or_default();

            skills.push(InstalledSkill {
                id: skill_id,
                name,
                description,
                path: skill_path.display().to_string(),
                manifest_path: manifest_path.display().to_string(),
                scope: root.scope.to_string(),
                install_type: install_type.to_string(),
                updated_at,
                source: lock_entry.source,
                source_type: lock_entry.source_type,
            });
        }
    }

    skills.sort_by(|left, right| {
        skill_scope_rank(&left.scope)
            .cmp(&skill_scope_rank(&right.scope))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    Ok(skills)
}

fn candidate_skill_roots() -> Vec<SkillRoot> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();

    for workspace_root in discover_workspace_roots() {
        push_skill_root(
            &mut roots,
            &mut seen,
            workspace_root.join("skills"),
            "workspace",
            Some(workspace_root.join("skills-lock.json")),
        );
        push_skill_root(
            &mut roots,
            &mut seen,
            workspace_root.join(".agents").join("skills"),
            "workspace",
            Some(workspace_root.join("skills-lock.json")),
        );
    }

    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        let base = PathBuf::from(codex_home);
        push_skill_root(&mut roots, &mut seen, base.join("skills"), "global", None);
    }

    if let Some(home_dir) = home_dir() {
        push_skill_root(
            &mut roots,
            &mut seen,
            home_dir.join(".codex").join("skills"),
            "global",
            None,
        );
        push_skill_root(
            &mut roots,
            &mut seen,
            home_dir.join(".agents").join("skills"),
            "global",
            None,
        );
    }

    roots
}

fn discover_workspace_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();

    let mut start_points = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        start_points.push(current_dir);
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            start_points.push(parent.to_path_buf());
        }
    }

    for start_point in start_points {
        for ancestor in start_point.ancestors() {
            let candidate = ancestor.to_path_buf();
            if !looks_like_workspace_root(&candidate) {
                continue;
            }
            let canonical = fs::canonicalize(&candidate).unwrap_or(candidate.clone());
            if seen.insert(canonical) {
                roots.push(candidate);
            }
        }
    }

    roots
}

fn looks_like_workspace_root(path: &Path) -> bool {
    path.join("skills").exists()
        || path.join(".agents").join("skills").exists()
        || path.join("skills-lock.json").exists()
        || path.join("package.json").exists()
            && path.join("src-tauri").join("tauri.conf.json").exists()
}

fn push_skill_root(
    roots: &mut Vec<SkillRoot>,
    seen: &mut HashSet<PathBuf>,
    path: PathBuf,
    scope: &'static str,
    lock_path: Option<PathBuf>,
) {
    if !path.exists() || !path.is_dir() {
        return;
    }

    let canonical = fs::canonicalize(&path).unwrap_or(path.clone());
    if seen.insert(canonical) {
        roots.push(SkillRoot {
            path,
            scope,
            lock_path,
        });
    }
}

fn load_skill_lock_entries(path: Option<&Path>) -> HashMap<String, SkillLockEntry> {
    let Some(path) = path else {
        return HashMap::new();
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return HashMap::new(),
    };

    serde_json::from_str::<SkillLockFile>(&content)
        .map(|file| file.skills)
        .unwrap_or_default()
}

fn parse_skill_manifest(content: &str) -> SkillManifest {
    let mut manifest = SkillManifest::default();
    let mut body_lines = Vec::new();
    let mut lines = content.lines().peekable();

    if matches!(lines.peek(), Some(line) if line.trim() == "---") {
        lines.next();
        let mut frontmatter_lines = Vec::new();
        let mut block_scalar_indent: Option<usize> = None;
        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            let indent = line.chars().take_while(|char| char.is_whitespace()).count();

            if let Some(active_indent) = block_scalar_indent {
                if trimmed.is_empty() {
                    frontmatter_lines.push(line.to_string());
                    continue;
                }
                if indent > active_indent {
                    frontmatter_lines.push(line.to_string());
                    continue;
                }
                block_scalar_indent = None;
            }

            if trimmed == "---" {
                break;
            }

            if let Some((_, value)) = trimmed.split_once(':') {
                let value = value.trim();
                if matches!(value, "|" | "|-" | "|+" | ">" | ">-" | ">+") {
                    block_scalar_indent = Some(indent);
                }
            }
            frontmatter_lines.push(line.to_string());
        }

        let mut index = 0usize;
        while index < frontmatter_lines.len() {
            let line = &frontmatter_lines[index];
            let trimmed = line.trim();
            if let Some((key, value)) = trimmed.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                let (normalized, consumed) =
                    parse_frontmatter_value(&frontmatter_lines, index, line, value);
                match key {
                    "name" if manifest.name.is_none() => {
                        manifest.name = Some(normalized);
                    }
                    "description" if manifest.description.is_none() => {
                        manifest.description = Some(normalized);
                    }
                    _ => {}
                }
                index += consumed;
                continue;
            }
            index += 1;
        }
    }

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        body_lines.push(trimmed.to_string());
    }

    if manifest.name.is_none() {
        manifest.name = body_lines
            .iter()
            .find_map(|line| {
                if line.starts_with('#') {
                    Some(line.trim_start_matches('#').trim().to_string())
                } else {
                    None
                }
            })
            .filter(|value| !value.is_empty());
    }

    if manifest.description.is_none() {
        manifest.description = body_lines
            .iter()
            .find(|line| {
                !line.starts_with('#')
                    && !line.starts_with("```")
                    && !line.starts_with('-')
                    && !line.starts_with('*')
            })
            .map(|line| line.trim().to_string())
            .filter(|value| !value.is_empty());
    }

    manifest
}

fn parse_frontmatter_value(
    lines: &[String],
    start_index: usize,
    original_line: &str,
    raw_value: &str,
) -> (String, usize) {
    if !matches!(raw_value, "|" | "|-" | "|+" | ">" | ">-" | ">+") {
        return (strip_wrapping_quotes(raw_value).to_string(), 1);
    }

    let key_indent = original_line
        .chars()
        .take_while(|char| char.is_whitespace())
        .count();
    let mut block_lines = Vec::new();
    let mut index = start_index + 1;
    let mut block_indent: Option<usize> = None;

    while index < lines.len() {
        let candidate = &lines[index];
        let trimmed = candidate.trim();
        let indent = candidate
            .chars()
            .take_while(|char| char.is_whitespace())
            .count();

        if trimmed.is_empty() {
            if block_indent.is_some() {
                block_lines.push(String::new());
                index += 1;
                continue;
            }
            index += 1;
            continue;
        }

        let current_block_indent = block_indent.unwrap_or(indent);
        if indent <= key_indent || indent < current_block_indent {
            break;
        }

        block_indent = Some(current_block_indent);
        let content = candidate
            .get(current_block_indent..)
            .unwrap_or(trimmed)
            .trim_end()
            .to_string();
        block_lines.push(content);
        index += 1;
    }

    let value = if raw_value.starts_with('>') {
        fold_frontmatter_block_lines(&block_lines)
    } else {
        trim_frontmatter_block_lines(block_lines).join("\n")
    };

    (value, index.saturating_sub(start_index))
}

fn trim_frontmatter_block_lines(lines: Vec<String>) -> Vec<String> {
    let start = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map(|index| index + 1)
        .unwrap_or(start);

    lines[start..end].to_vec()
}

fn fold_frontmatter_block_lines(lines: &[String]) -> String {
    let trimmed = trim_frontmatter_block_lines(lines.to_vec());
    let mut paragraphs = Vec::new();
    let mut current = String::new();

    for line in trimmed {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.trim().to_string());
                current.clear();
            }
            continue;
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(line.trim());
    }

    if !current.is_empty() {
        paragraphs.push(current.trim().to_string());
    }

    paragraphs.join("\n")
}

fn strip_wrapping_quotes(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        if let Some(stripped) = trimmed
            .strip_prefix('"')
            .and_then(|item| item.strip_suffix('"'))
        {
            return stripped;
        }
        if let Some(stripped) = trimmed
            .strip_prefix('\'')
            .and_then(|item| item.strip_suffix('\''))
        {
            return stripped;
        }
    }
    trimmed
}

fn system_time_to_ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn skill_scope_rank(scope: &str) -> u8 {
    match scope {
        "workspace" => 0,
        "global" => 1,
        _ => 2,
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, remove_dir_all, write};

    #[test]
    fn parse_manifest_prefers_frontmatter_fields() {
        let manifest = parse_skill_manifest(
            r#"---
name: all-plan
description: "Collaborative planning using abstract roles."
---

# Ignored title

Fallback description
"#,
        );

        assert_eq!(
            manifest,
            SkillManifest {
                name: Some("all-plan".to_string()),
                description: Some("Collaborative planning using abstract roles.".to_string()),
            }
        );
    }

    #[test]
    fn parse_manifest_falls_back_to_heading_and_body() {
        let manifest = parse_skill_manifest(
            r#"
# Browser Skill

Automate browser interactions for data collection.
"#,
        );

        assert_eq!(
            manifest,
            SkillManifest {
                name: Some("Browser Skill".to_string()),
                description: Some("Automate browser interactions for data collection.".to_string()),
            }
        );
    }

    #[test]
    fn parse_manifest_supports_literal_multiline_description() {
        let manifest = parse_skill_manifest(
            r#"---
name: multiline-skill
description: |
  第一行简介
  ---
  第二行才是补充说明
metadata:
  short-description: ignored
---
"#,
        );

        assert_eq!(
            manifest,
            SkillManifest {
                name: Some("multiline-skill".to_string()),
                description: Some("第一行简介\n---\n第二行才是补充说明".to_string()),
            }
        );
    }

    #[test]
    fn parse_manifest_supports_folded_multiline_description() {
        let manifest = parse_skill_manifest(
            r#"---
name: folded-skill
description: >
  第一行简介
  第二行继续补充

  第二段说明
---
"#,
        );

        assert_eq!(
            manifest,
            SkillManifest {
                name: Some("folded-skill".to_string()),
                description: Some("第一行简介 第二行继续补充\n第二段说明".to_string()),
            }
        );
    }

    #[test]
    fn scan_skill_roots_reads_lock_metadata_and_deduplicates_ids() {
        let temp_root = std::env::temp_dir().join(format!(
            "nineclaw-skill-scan-test-{}",
            system_time_to_ms(SystemTime::now())
        ));
        let workspace_skills = temp_root.join("skills");
        let global_skills = temp_root.join("global-skills");
        create_dir_all(workspace_skills.join("alpha")).expect("create workspace skill");
        create_dir_all(global_skills.join("alpha")).expect("create global skill");
        create_dir_all(global_skills.join("beta")).expect("create global skill");
        write(
            workspace_skills.join("alpha").join("SKILL.md"),
            "---\nname: alpha\ndescription: first\n---\n",
        )
        .expect("write workspace manifest");
        write(
            global_skills.join("alpha").join("SKILL.md"),
            "---\nname: alpha global\ndescription: should be skipped\n---\n",
        )
        .expect("write global manifest");
        write(
            global_skills.join("beta").join("SKILL.md"),
            "# beta\n\nBeta description\n",
        )
        .expect("write beta manifest");
        write(
            temp_root.join("skills-lock.json"),
            r#"{"version":1,"skills":{"alpha":{"source":"example/repo","sourceType":"github"}}}"#,
        )
        .expect("write lock");

        let skills = scan_skill_roots(&[
            SkillRoot {
                path: workspace_skills.clone(),
                scope: "workspace",
                lock_path: Some(temp_root.join("skills-lock.json")),
            },
            SkillRoot {
                path: global_skills.clone(),
                scope: "global",
                lock_path: None,
            },
        ])
        .expect("scan skills");

        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].id, "alpha");
        assert_eq!(skills[0].source.as_deref(), Some("example/repo"));
        assert_eq!(skills[1].id, "beta");
        assert_eq!(skills[1].scope, "global");

        remove_dir_all(temp_root).expect("cleanup temp root");
    }

    #[test]
    fn list_system_skill_catalog_reads_manifest_from_roots() {
        let temp_root = std::env::temp_dir().join(format!(
            "nineclaw-system-skill-test-{}",
            system_time_to_ms(SystemTime::now())
        ));
        let system_root = temp_root.join("system-skills");
        create_dir_all(system_root.join("agent-builder")).expect("create system skill");
        write(
            system_root.join("agent-builder").join("SKILL.md"),
            "---\nname: agent-builder\ndescription: guided builder\n---\n",
        )
        .expect("write system skill manifest");

        let catalog = list_system_skill_catalog_with_roots(&[system_root]);
        assert!(catalog.available);
        assert_eq!(catalog.skills.len(), 1);
        assert_eq!(catalog.skills[0].id, "agent-builder");
        assert_eq!(catalog.skills[0].description, "guided builder");

        remove_dir_all(temp_root).expect("cleanup temp root");
    }

    #[test]
    fn resolve_skill_directories_reads_runtime_system_skills() {
        let temp_root = std::env::temp_dir().join(format!(
            "nineclaw-runtime-system-skill-test-{}",
            system_time_to_ms(SystemTime::now())
        ));
        let app_root = temp_root.join("app");
        let resources_root = app_root.join("resources").join("system-skills");
        create_dir_all(resources_root.join("nineclaw-task-creator")).expect("create system skill");
        write(
            resources_root
                .join("nineclaw-task-creator")
                .join("SKILL.md"),
            "---\nname: nineclaw-task-creator\ndescription: default task creator\n---\n",
        )
        .expect("write system skill manifest");

        let previous_dir = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(&app_root).expect("set current dir");

        let resolved =
            resolve_skill_directories(&["nineclaw-task-creator".to_string()]).expect("resolve");

        std::env::set_current_dir(previous_dir).expect("restore current dir");
        remove_dir_all(temp_root).expect("cleanup temp root");

        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].ends_with("nineclaw-task-creator"));
    }
}
