use crate::skill_cache::SkillCache;
use crate::skill_manifest::parse_skill_manifest;
use crate::skill_providers::{candidate_skill_roots, home_dir, SkillRoot};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
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

#[derive(Debug, Clone)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    #[allow(dead_code)]
    pub path: PathBuf,
    pub triggers: Vec<String>,
    pub examples: Vec<String>,
    pub capabilities: Vec<String>,
    pub requires_auth: bool,
    pub side_effect_level: String,
    pub modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRuntimeSourceInfo {
    pub id: String,
    pub source: String,
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

pub fn list_installed_skills() -> Result<Vec<InstalledSkill>, String> {
    scan_skill_roots(&candidate_skill_roots())
}

static RUNTIME_SKILL_CACHE: OnceLock<Arc<SkillCache<SkillDefinition>>> = OnceLock::new();

/// Process-wide per-root cache for runtime skill definitions. Watchers
/// invalidate on filesystem changes; reads fall back to direct scanning.
pub(crate) fn runtime_skill_cache() -> Arc<SkillCache<SkillDefinition>> {
    RUNTIME_SKILL_CACHE
        .get_or_init(|| Arc::new(SkillCache::new()))
        .clone()
}

/// Start filesystem watchers for every candidate skill root, kept alive for
/// the process lifetime. Model-written skills and human edits both surface
/// as filesystem events, so no extra write-path invalidation is needed.
pub(crate) fn start_runtime_skill_watchers(app: &AppHandle) {
    let roots = candidate_skill_roots()
        .into_iter()
        .map(|root| root.path)
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return;
    }
    // Process-lifetime resource: intentionally leaked (never dropped) so the
    // watcher thread outlives every scan. A bounded one-time allocation.
    let handle = crate::skill_cache::start_skill_watchers(roots, runtime_skill_cache(), app);
    std::mem::forget(handle);
}

pub fn list_runtime_available_skills() -> Result<Vec<SkillDefinition>, String> {
    let cache = runtime_skill_cache();
    let mut skills = Vec::new();
    let mut seen_ids = HashSet::new();

    // candidate_skill_roots() is rank-sorted; first-seen dedup across the
    // cached per-root scans therefore keeps the explicit priority table as
    // the name-resolution rule (lowest rank wins).
    for root in candidate_skill_roots() {
        let definitions = cache.get_or_scan(&root.path, || {
            scan_skill_definitions_from_roots(&[root.clone()]).unwrap_or_else(|error| {
                log::warn!("扫描技能根目录失败 {}: {error}", root.path.display());
                Vec::new()
            })
        });
        for definition in definitions {
            if seen_ids.insert(definition.id.clone()) {
                skills.push(definition);
            }
        }
    }

    for (skill_id, path) in list_runtime_system_skill_directories()? {
        if seen_ids.contains(&skill_id) {
            continue;
        }
        if let Some(definition) =
            load_skill_definition("runtime", &skill_id, &path.join("SKILL.md"), &path)?
        {
            seen_ids.insert(skill_id);
            skills.push(definition);
        }
    }

    skills.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(skills)
}

pub fn resolve_skill_directories(skill_ids: &[String]) -> Result<Vec<PathBuf>, String> {
    if skill_ids.is_empty() {
        return Ok(Vec::new());
    }

    let installed = list_installed_skills()?;
    let mut available_by_id = installed
        .into_iter()
        .map(|skill| (skill.id, (PathBuf::from(skill.path), skill.install_type)))
        .collect::<HashMap<_, _>>();
    for (skill_id, path) in list_runtime_system_skill_directories()? {
        available_by_id
            .entry(skill_id)
            .or_insert((path, "directory".to_string()));
    }

    let mut resolved = Vec::new();
    let mut seen = HashSet::new();

    for skill_id in skill_ids {
        let Some((path, install_type)) = available_by_id.get(skill_id) else {
            continue;
        };
        let mount_path = if install_type == "file" {
            materialize_flat_skill(skill_id, path)?
        } else {
            path.clone()
        };
        let canonical = fs::canonicalize(&mount_path).unwrap_or_else(|_| mount_path.clone());
        if seen.insert(canonical) {
            resolved.push(mount_path);
        }
    }

    Ok(resolved)
}

/// Materialize a flat `<name>.md` skill into a bundle directory
/// (`<name>/SKILL.md`) under the private runtime dir, so the PI process can
/// mount it with the same `--skill <dir>` mechanism as bundle skills.
fn materialize_flat_skill(skill_id: &str, source_file: &Path) -> Result<PathBuf, String> {
    let bundle_dir = crate::runtime_paths::pi_runtime_dir()
        .join("skill-materialize")
        .join(skill_id);
    let content = fs::read(source_file)
        .map_err(|error| format!("读取 flat 技能失败 {}: {error}", source_file.display()))?;
    crate::runtime_paths::write_private_file(&bundle_dir.join("SKILL.md"), &content)?;
    Ok(bundle_dir)
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

pub fn resolve_skill_source_info(
    skill_ids: &[String],
) -> Result<HashMap<String, SkillRuntimeSourceInfo>, String> {
    if skill_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let installed = list_installed_skills()?;
    let installed_by_id = installed
        .into_iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect::<HashMap<_, _>>();
    let runtime_system_ids = list_runtime_system_skill_directories()?
        .into_iter()
        .map(|(skill_id, _)| skill_id)
        .collect::<HashSet<_>>();

    let mut out = HashMap::new();
    for skill_id in skill_ids {
        let source = if runtime_system_ids.contains(skill_id) {
            "default".to_string()
        } else {
            installed_by_id
                .get(skill_id)
                .and_then(|skill| {
                    skill
                        .source_type
                        .clone()
                        .or_else(|| skill.source.clone())
                        .or_else(|| Some(skill.scope.clone()))
                })
                .unwrap_or_else(|| "user".to_string())
        };
        out.insert(
            skill_id.clone(),
            SkillRuntimeSourceInfo {
                id: skill_id.clone(),
                source,
                source_type: installed_by_id
                    .get(skill_id)
                    .and_then(|skill| skill.source_type.clone()),
            },
        );
    }
    Ok(out)
}

/// A discovered skill entry: either a bundle directory (`<id>/SKILL.md`)
/// or a flat file (`<id>.md`). Symlinked directories are treated as their
/// target and classify as bundles, preserving the legacy install behavior.
struct SkillEntry {
    id: String,
    manifest_path: PathBuf,
    skill_path: PathBuf,
    is_flat: bool,
}

fn is_valid_skill_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--")
}

fn classify_skill_entry(entry: &fs::DirEntry) -> Option<SkillEntry> {
    let name = entry.file_name().to_string_lossy().trim().to_string();
    if name.is_empty() || name.starts_with('.') {
        return None;
    }

    let path = entry.path();
    if path.is_dir() {
        if !is_valid_skill_id(&name) {
            log::warn!("跳过非法技能 id（需 kebab-case）: {}", path.display());
            return None;
        }
        let manifest_path = path.join("SKILL.md");
        if !manifest_path.is_file() {
            return None;
        }
        Some(SkillEntry {
            id: name,
            manifest_path,
            skill_path: path,
            is_flat: false,
        })
    } else if path.is_file() {
        let Some(id) = name.strip_suffix(".md").map(ToOwned::to_owned) else {
            return None;
        };
        if !is_valid_skill_id(&id) {
            log::warn!("跳过非法技能 id（需 kebab-case）: {}", path.display());
            return None;
        }
        Some(SkillEntry {
            id,
            manifest_path: path.clone(),
            skill_path: path,
            is_flat: true,
        })
    } else {
        None
    }
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
            let Some(skill_entry) = classify_skill_entry(&entry) else {
                continue;
            };

            if !seen_ids.insert(skill_entry.id.clone()) {
                continue;
            }

            let canonical_skill_path = fs::canonicalize(&skill_entry.skill_path)
                .unwrap_or_else(|_| skill_entry.skill_path.clone());
            if !seen_paths.insert(canonical_skill_path) {
                continue;
            }

            let manifest_text = fs::read_to_string(&skill_entry.manifest_path).map_err(|error| {
                format!(
                    "读取技能说明失败 {}: {error}",
                    skill_entry.manifest_path.display()
                )
            })?;
            let manifest = parse_skill_manifest(&manifest_text);
            let metadata = fs::symlink_metadata(&skill_entry.skill_path).map_err(|error| {
                format!(
                    "读取技能元数据失败 {}: {error}",
                    skill_entry.skill_path.display()
                )
            })?;

            let updated_at = fs::metadata(&skill_entry.manifest_path)
                .and_then(|item| item.modified())
                .map(system_time_to_ms)
                .unwrap_or_default();
            let install_type = if skill_entry.is_flat {
                "file"
            } else if metadata.file_type().is_symlink() {
                "symlink"
            } else {
                "directory"
            };
            let name = manifest
                .name
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| skill_entry.id.clone());
            let description = manifest
                .description
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "暂无描述".to_string());
            let lock_entry = lock_entries.get(&skill_entry.id).cloned().unwrap_or_default();

            skills.push(InstalledSkill {
                id: skill_entry.id,
                name,
                description,
                path: skill_entry.skill_path.display().to_string(),
                manifest_path: skill_entry.manifest_path.display().to_string(),
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

fn scan_skill_definitions_from_roots(roots: &[SkillRoot]) -> Result<Vec<SkillDefinition>, String> {
    let mut skills = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_paths = HashSet::new();

    for root in roots {
        if !root.path.exists() || !root.path.is_dir() {
            continue;
        }

        let entries = fs::read_dir(&root.path)
            .map_err(|error| format!("读取技能目录失败 {}: {error}", root.path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!("读取技能目录条目失败 {}: {error}", root.path.display())
            })?;
            let Some(skill_entry) = classify_skill_entry(&entry) else {
                continue;
            };
            if !seen_ids.insert(skill_entry.id.clone()) {
                continue;
            }

            let canonical_skill_path = fs::canonicalize(&skill_entry.skill_path)
                .unwrap_or_else(|_| skill_entry.skill_path.clone());
            if !seen_paths.insert(canonical_skill_path) {
                continue;
            }

            if let Some(definition) = load_skill_definition(
                root.scope,
                &skill_entry.id,
                &skill_entry.manifest_path,
                &skill_entry.skill_path,
            )? {
                skills.push(definition);
            }
        }
    }

    Ok(skills)
}

fn load_skill_definition(
    _scope: &str,
    skill_id: &str,
    manifest_path: &Path,
    skill_path: &Path,
) -> Result<Option<SkillDefinition>, String> {
    let manifest_text = fs::read_to_string(manifest_path)
        .map_err(|error| format!("读取技能说明失败 {}: {error}", manifest_path.display()))?;
    let manifest = parse_skill_manifest(&manifest_text);
    let name = manifest
        .name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| skill_id.to_string());
    let description = manifest
        .description
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "暂无描述".to_string());

    Ok(Some(SkillDefinition {
        id: skill_id.to_string(),
        name,
        description,
        path: skill_path.to_path_buf(),
        triggers: manifest.triggers,
        examples: manifest.examples,
        capabilities: manifest.capabilities,
        requires_auth: manifest.requires_auth,
        side_effect_level: manifest
            .side_effect_level
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "low".to_string()),
        modes: manifest
            .modes
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
    }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, remove_dir_all, write};

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
                rank: 100,
            },
            SkillRoot {
                path: global_skills.clone(),
                scope: "global",
                lock_path: None,
                rank: 400,
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

    #[test]
    fn scan_discovers_flat_skill_files_alongside_bundles() {
        let temp_root = std::env::temp_dir().join(format!(
            "nineclaw-flat-skill-test-{}",
            system_time_to_ms(SystemTime::now())
        ));
        let skills_root = temp_root.join("skills");
        create_dir_all(skills_root.join("bundle-skill")).expect("create bundle skill");
        write(
            skills_root.join("bundle-skill").join("SKILL.md"),
            "---\nname: bundle-skill\ndescription: bundled\n---\n",
        )
        .expect("write bundle manifest");
        write(
            skills_root.join("flat-skill.md"),
            "---\nname: flat-skill\ndescription: flat file body\n---\n\n# Flat body\n",
        )
        .expect("write flat skill");

        let roots = [SkillRoot {
            path: skills_root,
            scope: "workspace",
            lock_path: None,
            rank: 100,
        }];
        let definitions = scan_skill_definitions_from_roots(&roots).expect("scan");

        remove_dir_all(temp_root).expect("cleanup temp root");

        assert_eq!(definitions.len(), 2);
        let flat = definitions
            .iter()
            .find(|skill| skill.id == "flat-skill")
            .expect("flat skill discovered");
        assert_eq!(flat.description, "flat file body");
        assert_eq!(flat.name, "flat-skill");
        assert!(flat.path.ends_with("flat-skill.md"));
        assert!(definitions.iter().any(|skill| skill.id == "bundle-skill"));
    }

    #[test]
    fn scan_skips_non_kebab_case_skill_ids() {
        let temp_root = std::env::temp_dir().join(format!(
            "nineclaw-kebab-skill-test-{}",
            system_time_to_ms(SystemTime::now())
        ));
        let skills_root = temp_root.join("skills");
        create_dir_all(skills_root.join("Bad_Name")).expect("create invalid bundle");
        write(
            skills_root.join("Bad_Name").join("SKILL.md"),
            "---\nname: bad\ndescription: nope\n---\n",
        )
        .expect("write invalid bundle manifest");
        write(
            skills_root.join("Prompt Engineer.md"),
            "---\nname: invalid flat\ndescription: nope\n---\n",
        )
        .expect("write invalid flat skill");

        let roots = [SkillRoot {
            path: skills_root,
            scope: "workspace",
            lock_path: None,
            rank: 100,
        }];
        let definitions = scan_skill_definitions_from_roots(&roots).expect("scan");

        remove_dir_all(temp_root).expect("cleanup temp root");

        assert!(definitions.is_empty(), "invalid ids must be skipped: {definitions:?}");
    }

    #[test]
    fn lower_rank_root_wins_same_skill_id() {
        let temp_root = std::env::temp_dir().join(format!(
            "nineclaw-rank-skill-test-{}",
            system_time_to_ms(SystemTime::now())
        ));
        let workspace_skills = temp_root.join("workspace-skills");
        let user_skills = temp_root.join("user-skills");
        create_dir_all(workspace_skills.join("shared-skill")).expect("create workspace skill");
        create_dir_all(user_skills.join("shared-skill")).expect("create user skill");
        write(
            workspace_skills.join("shared-skill").join("SKILL.md"),
            "---\nname: shared-skill\ndescription: workspace wins\n---\n",
        )
        .expect("write workspace manifest");
        write(
            user_skills.join("shared-skill").join("SKILL.md"),
            "---\nname: shared-skill\ndescription: user loses\n---\n",
        )
        .expect("write user manifest");

        // candidate_skill_roots() returns roots sorted by rank ascending; the
        // scan deduplicates first-seen, so the lower rank must win.
        let roots = [
            SkillRoot {
                path: workspace_skills,
                scope: "workspace",
                lock_path: None,
                rank: 100,
            },
            SkillRoot {
                path: user_skills,
                scope: "global",
                lock_path: None,
                rank: 400,
            },
        ];
        let definitions = scan_skill_definitions_from_roots(&roots).expect("scan");

        remove_dir_all(temp_root).expect("cleanup temp root");

        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].id, "shared-skill");
        assert_eq!(definitions[0].description, "workspace wins");
    }

    #[test]
    fn resolve_skill_directories_materializes_flat_skills() {
        let temp_root = std::env::temp_dir().join(format!(
            "nineclaw-flat-mount-test-{}",
            system_time_to_ms(SystemTime::now())
        ));
        let skills_root = temp_root.join("skills");
        create_dir_all(&skills_root).expect("create skills root");
        write(
            skills_root.join("mount-me.md"),
            "---\nname: mount-me\ndescription: mountable\n---\n\n# Body\n",
        )
        .expect("write flat skill");

        // The `skills/` directory alone makes this temp dir look like a
        // workspace root, so list_installed_skills discovers the flat file.
        let previous_dir = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(&temp_root).expect("set current dir");

        let resolved =
            resolve_skill_directories(&["mount-me".to_string()]).expect("resolve flat skill");

        std::env::set_current_dir(previous_dir).expect("restore current dir");
        let mounted_skill = fs::read_to_string(resolved[0].join("SKILL.md"))
            .expect("read materialized skill");
        let materialized = crate::runtime_paths::pi_runtime_dir()
            .join("skill-materialize")
            .join("mount-me");
        remove_dir_all(temp_root).expect("cleanup temp root");
        let _ = remove_dir_all(materialized);

        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].ends_with("mount-me"), "{resolved:?}");
        assert!(mounted_skill.contains("# Body"));
    }
}
