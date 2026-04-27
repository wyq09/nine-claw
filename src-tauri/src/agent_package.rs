//! 智能体包导入 / 导出（ZIP：`manifest.json` + `agent.json` + `workspace/...`）。

use crate::agent_workspace;
use crate::agents::{AgentInput, AgentRecord};
use crate::skills;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use tauri::AppHandle;
use zip::write::FileOptions;
use zip::CompressionMethod;
use zip::ZipArchive;

pub const EXPORT_FORMAT: &str = "nineclaw-agent-export";
pub const EXPORT_VERSION: u32 = 1;

const SHARED_ROOT_ZIP_PREFIX: &str = "shared_workspace_root/";
const WORKSPACE_AGENTS_PREFIX: &str = "workspace/agents/";

/// 与 [`agent_workspace`](crate::agent_workspace) 根目录可读文件一致，但不导出自动生成的注册表。
const OPTIONAL_SHARED_ROOT_FILES: &[&str] = &[
    "AGENTS.md",
    "SOUL.md",
    "USER.md",
    "MEMORY.md",
    "TOOLS.md",
    "HEARTBEAT.md",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPackageManifest {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub source_agent_id: String,
    pub includes_secrets: bool,
    pub includes_shared_root: bool,
    #[serde(default)]
    pub exported_from_builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentImportResult {
    pub agent: AgentRecord,
    pub warnings: Vec<String>,
}

fn redact_agent_record(mut record: AgentRecord, include_secrets: bool) -> AgentRecord {
    if include_secrets {
        return record;
    }
    for (_, cfg) in record.bot_configs.iter_mut() {
        cfg.client_secret.clear();
        cfg.token = None;
        cfg.ai_api_key = None;
        cfg.peer_shared_secret = None;
    }
    record
}

fn collect_files_under(
    dir: &Path,
    base: &Path,
    out: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| format!("读取目录失败 {}: {e}", dir.display()))?
    {
        let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)
            .map_err(|e| format!("读取元数据失败 {}: {e}", path.display()))?;
        if meta.file_type().is_symlink() {
            return Err(format!("不支持导出符号链接: {}", path.display()));
        }
        if path.is_dir() {
            collect_files_under(&path, base, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .map_err(|e| format!("计算相对路径失败: {e}"))?;
            out.push((rel.to_path_buf(), path));
        }
    }
    Ok(())
}

fn zip_sanitize_relative(name: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                return Err("压缩包内路径非法".to_string());
            }
            Component::ParentDir => return Err("压缩包内路径非法".to_string()),
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
        }
    }
    Ok(out)
}

fn unzip_package(package_path: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(package_path).map_err(|e| format!("无法打开包文件: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("无法读取 ZIP: {e}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("读取 ZIP 条目失败: {e}"))?;
        let name = entry.name().to_string();
        let rel = zip_sanitize_relative(&name)?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out_path = dest.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| format!("创建目录失败: {e}"))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建父目录失败: {e}"))?;
        }
        let mut outfile = fs::File::create(&out_path)
            .map_err(|e| format!("创建文件失败 {}: {e}", out_path.display()))?;
        std::io::copy(&mut entry, &mut outfile).map_err(|e| format!("解压写入失败: {e}"))?;
    }
    Ok(())
}

fn write_agent_zip(
    dest_path: &Path,
    manifest: &AgentPackageManifest,
    agent: &AgentRecord,
    agent_home: &Path,
    workspace_root: &Path,
    include_shared_root: bool,
) -> Result<(), String> {
    let file = fs::File::create(dest_path).map_err(|e| format!("无法创建导出文件: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default().compression_method(CompressionMethod::Deflated);

    let manifest_json =
        serde_json::to_string_pretty(manifest).map_err(|e| format!("序列化 manifest 失败: {e}"))?;
    zip.start_file("manifest.json", options)
        .map_err(|e| format!("写入 ZIP 失败: {e}"))?;
    zip.write_all(manifest_json.as_bytes())
        .map_err(|e| format!("写入 ZIP 失败: {e}"))?;

    let agent_json =
        serde_json::to_string_pretty(agent).map_err(|e| format!("序列化 agent 失败: {e}"))?;
    zip.start_file("agent.json", options)
        .map_err(|e| format!("写入 ZIP 失败: {e}"))?;
    zip.write_all(agent_json.as_bytes())
        .map_err(|e| format!("写入 ZIP 失败: {e}"))?;

    let mut pairs = Vec::new();
    collect_files_under(agent_home, agent_home, &mut pairs)?;
    let prefix = format!("{}{}/", WORKSPACE_AGENTS_PREFIX, manifest.source_agent_id);
    for (rel, abs) in pairs {
        let zip_path = format!("{}{}", prefix, rel.display());
        zip.start_file(&zip_path, options)
            .map_err(|e| format!("写入 ZIP 失败: {e}"))?;
        let bytes = fs::read(&abs).map_err(|e| format!("读取文件失败: {e}"))?;
        zip.write_all(&bytes)
            .map_err(|e| format!("写入 ZIP 失败: {e}"))?;
    }

    if include_shared_root {
        for file_name in OPTIONAL_SHARED_ROOT_FILES {
            let path = workspace_root.join(file_name);
            if !path.is_file() {
                continue;
            }
            let zip_path = format!("{SHARED_ROOT_ZIP_PREFIX}{file_name}");
            zip.start_file(&zip_path, options)
                .map_err(|e| format!("写入 ZIP 失败: {e}"))?;
            let bytes = fs::read(&path).map_err(|e| format!("读取共享根文件失败: {e}"))?;
            zip.write_all(&bytes)
                .map_err(|e| format!("写入 ZIP 失败: {e}"))?;
        }
    }

    zip.finish().map_err(|e| format!("完成 ZIP 失败: {e}"))?;
    Ok(())
}

/// 导出当前智能体为 ZIP（路径由前端对话框选择）。
pub fn export_agent_package_to_path(
    app: &AppHandle,
    agent_id: String,
    dest_path: String,
    include_secrets: bool,
    include_shared_root: bool,
) -> Result<(), String> {
    let connection = crate::open_history_db(app)?;
    crate::agents::ensure_agents_ready_conn(&connection)?;
    let Some(mut record) = crate::agents::fetch_active_agent(&connection, &agent_id)? else {
        return Err("要导出的智能体不存在".to_string());
    };

    let exported_from_builtin = record.is_builtin;
    record = redact_agent_record(record, include_secrets);

    let workspace_root = agent_workspace::resolve_workspace_root()?;
    agent_workspace::ensure_root_scaffold(&workspace_root)?;
    let agent_home = workspace_root.join("agents").join(&agent_id);
    if !agent_home.is_dir() {
        fs::create_dir_all(&agent_home.join("memory"))
            .map_err(|e| format!("创建工作区目录失败: {e}"))?;
    }

    let manifest = AgentPackageManifest {
        format: EXPORT_FORMAT.to_string(),
        version: EXPORT_VERSION,
        exported_at: chrono::Utc::now().to_rfc3339(),
        source_agent_id: agent_id.clone(),
        includes_secrets: include_secrets,
        includes_shared_root: include_shared_root,
        exported_from_builtin,
    };

    let dest = PathBuf::from(dest_path.trim());
    write_agent_zip(
        &dest,
        &manifest,
        &record,
        &agent_home,
        &workspace_root,
        include_shared_root,
    )?;
    Ok(())
}

fn read_manifest_extracted(root: &Path) -> Result<AgentPackageManifest, String> {
    let path = root.join("manifest.json");
    let raw = fs::read_to_string(&path).map_err(|e| format!("读取 manifest.json 失败: {e}"))?;
    let manifest: AgentPackageManifest =
        serde_json::from_str(&raw).map_err(|e| format!("解析 manifest.json 失败: {e}"))?;
    if manifest.format != EXPORT_FORMAT {
        return Err("不是 NineClaw 智能体导出包".to_string());
    }
    if manifest.version != EXPORT_VERSION {
        return Err(format!(
            "不支持的包版本 {}（当前仅支持 {EXPORT_VERSION}）",
            manifest.version
        ));
    }
    Ok(manifest)
}

fn read_agent_json_extracted(root: &Path) -> Result<AgentRecord, String> {
    let path = root.join("agent.json");
    let raw = fs::read_to_string(&path).map_err(|e| format!("读取 agent.json 失败: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析 agent.json 失败: {e}"))
}

fn resolve_exported_agent_home(
    extract_root: &Path,
    manifest: &AgentPackageManifest,
) -> Result<PathBuf, String> {
    let base = extract_root.join("workspace").join("agents");
    if !base.is_dir() {
        return Err("包内缺少 workspace/agents 目录".to_string());
    }
    let expected = base.join(&manifest.source_agent_id);
    if expected.is_dir() {
        return Ok(expected);
    }
    let mut names: Vec<String> = fs::read_dir(&base)
        .map_err(|e| format!("读取 agents 目录失败: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n != "_template")
        .collect();
    names.sort();
    match names.len() {
        0 => Err("包内 workspace/agents 下没有智能体目录".to_string()),
        1 => Ok(base.join(&names[0])),
        _ => Err(
            "包内存在多个智能体目录且与 manifest.sourceAgentId 不匹配，请使用完整导出包"
                .to_string(),
        ),
    }
}

fn filter_skills_for_import(skill_ids: Vec<String>) -> (Vec<String>, Vec<String>) {
    let installed = match skills::list_installed_skills() {
        Ok(list) => list.into_iter().map(|s| s.id).collect::<HashSet<_>>(),
        Err(_) => HashSet::new(),
    };
    let mut kept = Vec::new();
    let mut warnings = Vec::new();
    for sid in skill_ids {
        if installed.contains(&sid) {
            kept.push(sid);
        } else {
            warnings.push(format!("技能在本机未安装，已跳过：{sid}"));
        }
    }
    (kept, warnings)
}

fn record_to_agent_input(record: AgentRecord, skill_ids: Vec<String>) -> AgentInput {
    AgentInput {
        id: Some(record.id),
        name: record.name,
        summary: record.summary,
        description: record.description,
        trigger_condition: record.trigger_condition,
        manual_trigger_only: record.manual_trigger_only,
        system_prompt: record.system_prompt,
        capability_policy: Some(record.capability_policy),
        skill_ids,
        allowed_tool_ids: record.allowed_tool_ids,
        default_provider_id: record.default_provider_id,
        default_model: record.default_model,
        execution_mode: Some(record.execution_mode),
        collaboration_config: record.collaboration_config,
        accent_color: record.accent_color,
        avatar_uri: record.avatar_uri,
        bot_configs: record.bot_configs,
        heartbeat_config: record.heartbeat_config,
        scenario_llm_config: record.scenario_llm_config,
        agent_loop_config: record.agent_loop_config,
    }
}

fn apply_shared_root_import(extract_root: &Path) -> Result<(), String> {
    let shared = extract_root.join("shared_workspace_root");
    if !shared.is_dir() {
        return Ok(());
    }
    let workspace_root = agent_workspace::resolve_workspace_root()?;
    agent_workspace::ensure_root_scaffold(&workspace_root)?;
    for entry in
        fs::read_dir(&shared).map_err(|e| format!("读取 shared_workspace_root 失败: {e}"))?
    {
        let entry = entry.map_err(|e| format!("读取条目失败: {e}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == "AGENT_REGISTRY.md" {
            continue;
        }
        let dest = workspace_root.join(&name);
        fs::copy(&path, &dest)
            .map_err(|e| format!("写入工作区根文件失败 {}: {e}", dest.display()))?;
    }
    Ok(())
}

/// 从 ZIP 导入为新智能体（新 id），并合并工作区树。
pub fn import_agent_package_from_path(
    app: &AppHandle,
    package_path: String,
) -> Result<AgentImportResult, String> {
    let package = PathBuf::from(package_path.trim());
    if !package.is_file() {
        return Err("包文件不存在".to_string());
    }

    let temp = std::env::temp_dir().join(format!(
        "nineclaw-agent-import-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&temp).map_err(|e| format!("创建临时目录失败: {e}"))?;

    let result = (|| -> Result<AgentImportResult, String> {
        unzip_package(&package, &temp)?;
        let manifest = read_manifest_extracted(&temp)?;
        let record = read_agent_json_extracted(&temp)?;
        if record.id != manifest.source_agent_id {
            // 宽松：以 manifest 为准定位目录
        }
        let exported_home = resolve_exported_agent_home(&temp, &manifest)?;

        let (skill_ids, mut warnings) = filter_skills_for_import(record.skill_ids.clone());
        if manifest.exported_from_builtin {
            warnings.push(
                "该包由内置智能体导出，已作为自定义智能体导入（isBuiltin=false）。".to_string(),
            );
        }
        if !manifest.includes_secrets {
            warnings.push("导出包未包含通道密钥，对等/IM 等需在本机重新配置凭证。".to_string());
        }

        let input = record_to_agent_input(record, skill_ids);
        let created = crate::agents::create_agent(app, input)?;

        if let Err(err) =
            agent_workspace::overlay_agent_home_from_export(&created.id, &exported_home)
        {
            let _ = crate::agents::delete_agent(app, created.id.clone());
            return Err(err);
        }

        if manifest.includes_shared_root {
            apply_shared_root_import(&temp)?;
        }

        let connection = crate::open_history_db(app)?;
        crate::agents::sync_agent_workspaces_logged(&connection, "导入智能体后同步工作区");

        let refreshed = crate::agents::fetch_active_agent(&connection, &created.id)?
            .ok_or_else(|| "导入后读取智能体失败".to_string())?;

        Ok(AgentImportResult {
            agent: refreshed,
            warnings,
        })
    })();

    let _ = fs::remove_dir_all(&temp);
    result
}
