use serde::Serialize;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};

const DEFAULT_WORKSPACE_RELATIVE_PATH: &str = ".nineclaw/workspace";
const LEGACY_WORKSPACE_RELATIVE_PATH: &str = ".openclaw/workspace";
const PRIMARY_WORKSPACE_ROOT_ENV: &str = "NINECLAW_WORKSPACE_ROOT";
const LEGACY_WORKSPACE_ROOT_ENVS: &[&str] = &["NINECLAW_AGENT_WORKSPACE_ROOT"];
const TEMPLATE_DIR: &str = "agents/_templates";
const ROOT_FILES: &[&str] = &[
    "AGENTS.md",
    "SOUL.md",
    "USER.md",
    "MEMORY.md",
    "TOOLS.md",
    "HEARTBEAT.md",
];
const AGENT_FILES: &[&str] = &[
    "IDENTITY.md",
    "ROLE.md",
    "MEMORY.md",
    "TOOLS.md",
    "HEARTBEAT.md",
    "WORKING.md",
    "DECISIONS.md",
    "PUBLIC_CONTEXT.md",
];
const SHARED_VIEW_FILES: &[&str] = &[
    "AGENTS.md",
    "SOUL.md",
    "USER.md",
    "MEMORY.md",
    "TOOLS.md",
    "HEARTBEAT.md",
    "AGENT_REGISTRY.md",
];
const AGENT_VIEW_FILES: &[&str] = &[
    "IDENTITY.md",
    "ROLE.md",
    "MEMORY.md",
    "TOOLS.md",
    "WORKING.md",
    "DECISIONS.md",
    "PUBLIC_CONTEXT.md",
    "HEARTBEAT.md",
    "BOOTSTRAP.md",
];

#[derive(Clone, Copy)]
pub struct AgentWorkspaceSeed<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub summary: &'a str,
    pub description: &'a str,
    pub accent_color: Option<&'a str>,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkspaceFile {
    pub key: String,
    pub scope: String,
    pub section: String,
    pub name: String,
    pub relative_path: String,
    pub absolute_path: String,
    pub read_only: bool,
    pub exists: bool,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkspaceBundle {
    pub agent_id: String,
    pub workspace_root: String,
    pub agent_home: String,
    pub files: Vec<AgentWorkspaceFile>,
}

pub fn runtime_sync_enabled() -> bool {
    !cfg!(test)
}

pub fn resolve_workspace_root() -> Result<PathBuf, String> {
    if let Some(explicit_root) = read_workspace_root_env(PRIMARY_WORKSPACE_ROOT_ENV) {
        return Ok(PathBuf::from(explicit_root));
    }

    for env_name in LEGACY_WORKSPACE_ROOT_ENVS {
        if let Some(explicit_root) = read_workspace_root_env(env_name) {
            return Ok(PathBuf::from(explicit_root));
        }
    }

    if let Some(home) = home_dir() {
        let root = home.join(DEFAULT_WORKSPACE_RELATIVE_PATH);
        migrate_legacy_workspace_if_needed(&home, &root)?;
        return Ok(root);
    }

    Err(format!(
        "无法推断 NineClaw workspace 根目录，请设置 {PRIMARY_WORKSPACE_ROOT_ENV}"
    ))
}

pub fn ensure_agent_workspace(
    seed: AgentWorkspaceSeed<'_>,
    create_bootstrap: bool,
) -> Result<PathBuf, String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;

    let agent_home = root.join("agents").join(seed.id);
    fs::create_dir_all(agent_home.join("memory"))
        .map_err(|error| format!("创建智能体 memory 目录失败: {error}"))?;

    for file_name in AGENT_FILES {
        let target = agent_home.join(file_name);
        if target.exists() {
            continue;
        }

        let content = render_template(&root, file_name, fallback_template(file_name), seed)?;
        fs::write(&target, content)
            .map_err(|error| format!("写入智能体文件 {} 失败: {error}", target.display()))?;
    }

    if create_bootstrap && !seed.is_builtin {
        let bootstrap_path = agent_home.join("BOOTSTRAP.md");
        if !bootstrap_path.exists() {
            let content = render_template(
                &root,
                "BOOTSTRAP.md",
                fallback_template("BOOTSTRAP.md"),
                seed,
            )?;
            fs::write(&bootstrap_path, content).map_err(|error| {
                format!(
                    "写入智能体引导文件 {} 失败: {error}",
                    bootstrap_path.display()
                )
            })?;
        }
    }

    Ok(agent_home)
}

pub fn sync_agent_registry(entries: &[AgentWorkspaceSeed<'_>]) -> Result<(), String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;

    let mut content = String::from(
        "# AGENT_REGISTRY.md\n\nThis file is the generated index of agent homes known to NineClaw.\nDo not store manual long-form notes here; runtime sync will overwrite them.\n\n",
    );

    content.push_str("## Shared Rules\n\n");
    content.push_str("- Root files are shared across agents\n");
    content.push_str("- `agents/<agent-id>/` is the private home of that agent\n");
    content.push_str("- Shared facts belong in root `MEMORY.md`\n");
    content.push_str("- Role-specific facts belong in `agents/<agent-id>/MEMORY.md`\n");
    content.push_str("- New agents should be scaffolded from `agents/_templates/`\n\n");

    content.push_str("## Current Agents\n\n");
    if entries.is_empty() {
        content.push_str("- No active agents yet\n");
    } else {
        for entry in entries {
            let visibility = if entry.is_builtin {
                "builtin"
            } else {
                "custom"
            };
            content.push_str(&format!(
                "- `{}`\n  - path: `agents/{}/`\n  - type: {}\n  - name: {}\n  - summary: {}\n\n",
                entry.id, entry.id, visibility, entry.name, entry.summary
            ));
        }
    }

    let path = root.join("AGENT_REGISTRY.md");
    fs::write(&path, content).map_err(|error| format!("写入 AGENT_REGISTRY.md 失败: {error}"))?;
    Ok(())
}

pub fn delete_agent_workspace(agent_id: &str) -> Result<(), String> {
    let root = resolve_workspace_root()?;
    let agent_home = root.join("agents").join(agent_id);
    if !agent_home.exists() {
        return Ok(());
    }

    fs::remove_dir_all(&agent_home)
        .map_err(|error| format!("删除智能体工作区失败: {error}"))?;
    Ok(())
}

pub fn build_workspace_system_prompt(agent_id: &str) -> Result<String, String> {
    let root = resolve_workspace_root()?;
    let agent_home = root.join("agents").join(agent_id);
    let relative_agent_home = format!("agents/{agent_id}");
    let bootstrap_exists = agent_home.join("BOOTSTRAP.md").exists();
    let root_display = display_workspace_root(&root);

    let mut sections = Vec::new();
    sections.push(format!(
        "NineClaw agent workspace 根目录：{}。",
        root_display
    ));
    sections.push(format!("当前智能体私有 home：{}/。", relative_agent_home));
    sections.push(
        "进入会话后，先同步本地 markdown 工作区，再开始正常答复；不要假装自己记得，先读文件。"
            .to_string(),
    );
    sections.push(
        "加载顺序：root `AGENTS.md` -> root `SOUL.md` -> root `USER.md` -> root `TOOLS.md` -> 私有会话可读 root `MEMORY.md` -> 当前 agent 的 `IDENTITY.md`、`ROLE.md`、`TOOLS.md`、`MEMORY.md`、`WORKING.md`、`DECISIONS.md` -> 最近两天 daily log。".to_string(),
    );
    sections.push(
        "共享 / 外部上下文默认只读 `PUBLIC_CONTEXT.md`，不要把 root `MEMORY.md` 或 agent 私有记忆向外复述。"
            .to_string(),
    );
    sections.push(
        "写入规则：原始过程写 daily log，短期上下文写 `WORKING.md`，稳定经验写 agent `MEMORY.md`，只有跨 agent 都该知道的事实才提升到 root `MEMORY.md`。".to_string(),
    );
    if bootstrap_exists {
        sections.push(
            "检测到当前 agent home 里存在 `BOOTSTRAP.md`。把它当成首次建档引导，先按引导补齐身份和角色文件，完成后删除它。".to_string(),
        );
    }

    Ok(sections.join("\n\n"))
}

pub fn read_agent_workspace_bundle(agent_id: &str) -> Result<AgentWorkspaceBundle, String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;

    let agent_home = root.join("agents").join(agent_id);
    fs::create_dir_all(agent_home.join("memory"))
        .map_err(|error| format!("创建 agent memory 目录失败: {error}"))?;

    let mut files = Vec::new();

    for file_name in SHARED_VIEW_FILES {
        let path = root.join(file_name);
        files.push(read_workspace_file(
            "shared",
            "shared",
            file_name,
            PathBuf::from(file_name),
            path,
            shared_file_is_read_only(file_name),
        ));
    }

    for file_name in AGENT_VIEW_FILES {
        let relative_path = PathBuf::from("agents").join(agent_id).join(file_name);
        let path = root.join(&relative_path);
        files.push(read_workspace_file(
            "agent",
            "private",
            file_name,
            relative_path,
            path,
            false,
        ));
    }

    let mut daily_logs = read_agent_daily_logs(&root, agent_id)?;
    files.append(&mut daily_logs);

    Ok(AgentWorkspaceBundle {
        agent_id: agent_id.to_string(),
        workspace_root: display_workspace_root(&root),
        agent_home: PathBuf::from("agents").join(agent_id).display().to_string(),
        files,
    })
}

pub fn write_agent_workspace_file(
    agent_id: &str,
    relative_path: &str,
    content: &str,
) -> Result<AgentWorkspaceBundle, String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;

    let target = resolve_writable_workspace_path(&root, agent_id, relative_path)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建 workspace 目录失败: {error}"))?;
    }

    fs::write(&target, content).map_err(|error| format!("写入 workspace 文件失败: {error}"))?;

    read_agent_workspace_bundle(agent_id)
}

pub fn append_agent_memory_entry(
    agent_id: &str,
    user_id: &str,
    user_message: &str,
    assistant_message: &str,
) -> Result<(), String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;

    let agent_home = root.join("agents").join(agent_id);
    fs::create_dir_all(agent_home.join("memory"))
        .map_err(|error| format!("创建 agent memory 目录失败: {error}"))?;

    let timestamp = current_timestamp_label();
    let summary = summarize_memory_entry(user_message, assistant_message);

    let working_path = agent_home.join("WORKING.md");
    let working_existing = fs::read_to_string(&working_path)
        .unwrap_or_else(|_| fallback_template("WORKING.md").to_string());
    fs::write(
        &working_path,
        upsert_working_memory(&working_existing, user_id, &timestamp, &summary),
    )
    .map_err(|error| format!("写入 WORKING.md 失败: {error}"))?;

    let memory_path = agent_home.join("MEMORY.md");
    let memory_existing = fs::read_to_string(&memory_path)
        .unwrap_or_else(|_| fallback_template("MEMORY.md").to_string());
    fs::write(
        &memory_path,
        append_memory_summary(&memory_existing, user_id, &timestamp, &summary),
    )
    .map_err(|error| format!("写入 MEMORY.md 失败: {error}"))?;

    let daily_log_path = agent_home
        .join("memory")
        .join(format!("{}.md", current_date_label()));
    let daily_log_existing = fs::read_to_string(&daily_log_path)
        .unwrap_or_else(|_| format!("# {}\n\n", current_date_label()));
    fs::write(
        &daily_log_path,
        append_daily_log_entry(
            &daily_log_existing,
            user_id,
            &timestamp,
            user_message,
            assistant_message,
        ),
    )
    .map_err(|error| format!("写入 daily log 失败: {error}"))?;

    Ok(())
}

fn ensure_root_scaffold(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root.join(TEMPLATE_DIR))
        .map_err(|error| format!("创建 agent 模板目录失败: {error}"))?;
    fs::create_dir_all(root.join("agents"))
        .map_err(|error| format!("创建 agent 根目录失败: {error}"))?;

    for file_name in ROOT_FILES {
        let path = root.join(file_name);
        if path.exists() {
            continue;
        }

        fs::write(&path, root_fallback_template(file_name))
            .map_err(|error| format!("写入 workspace 根文件 {} 失败: {error}", path.display()))?;
    }

    let readme_path = root.join("agents").join("README.md");
    if !readme_path.exists() {
        fs::write(&readme_path, agents_readme_fallback())
            .map_err(|error| format!("写入 agents/README.md 失败: {error}"))?;
    }

    for file_name in AGENT_FILES.iter().copied().chain(["BOOTSTRAP.md"]) {
        let template_path = root.join(TEMPLATE_DIR).join(file_name);
        if template_path.exists() {
            continue;
        }

        fs::write(&template_path, fallback_template(file_name)).map_err(|error| {
            format!("写入 agent 模板 {} 失败: {error}", template_path.display())
        })?;
    }

    Ok(())
}

fn render_template(
    root: &Path,
    file_name: &str,
    fallback: &str,
    seed: AgentWorkspaceSeed<'_>,
) -> Result<String, String> {
    let template_path = root.join(TEMPLATE_DIR).join(file_name);
    let template = if template_path.exists() {
        fs::read_to_string(&template_path)
            .map_err(|error| format!("读取模板 {} 失败: {error}", template_path.display()))?
    } else {
        fallback.to_string()
    };

    Ok(apply_placeholders(template, seed))
}

fn read_workspace_file(
    scope: &str,
    section: &str,
    name: &str,
    relative_path: PathBuf,
    absolute_path: PathBuf,
    read_only: bool,
) -> AgentWorkspaceFile {
    let exists = absolute_path.exists();
    let content = if exists {
        fs::read_to_string(&absolute_path).unwrap_or_else(|error| format!("读取失败：{error}"))
    } else {
        String::new()
    };

    AgentWorkspaceFile {
        key: format!("{scope}:{}", relative_path.display()),
        scope: scope.to_string(),
        section: section.to_string(),
        name: name.to_string(),
        relative_path: relative_path.display().to_string(),
        absolute_path: absolute_path.display().to_string(),
        read_only,
        exists,
        content,
    }
}

fn read_agent_daily_logs(root: &Path, agent_id: &str) -> Result<Vec<AgentWorkspaceFile>, String> {
    let memory_dir = root.join("agents").join(agent_id).join("memory");
    if !memory_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(&memory_dir)
        .map_err(|error| format!("读取 agent daily log 目录失败: {error}"))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let file_name = path.file_name()?.to_str()?.to_string();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                return None;
            }
            Some((file_name, path))
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| right.0.cmp(&left.0));
    entries.truncate(5);

    Ok(entries
        .into_iter()
        .map(|(file_name, path)| {
            let relative_path = PathBuf::from("agents")
                .join(agent_id)
                .join("memory")
                .join(&file_name);
            read_workspace_file("agent", "dailyLog", &file_name, relative_path, path, false)
        })
        .collect())
}

fn shared_file_is_read_only(file_name: &str) -> bool {
    matches!(file_name, "AGENT_REGISTRY.md")
}

fn resolve_writable_workspace_path(
    root: &Path,
    agent_id: &str,
    relative_path: &str,
) -> Result<PathBuf, String> {
    let normalized = normalize_relative_workspace_path(relative_path)?;

    if normalized.len() == 1 {
        let file_name = &normalized[0];
        if SHARED_VIEW_FILES.contains(&file_name.as_str()) {
            if shared_file_is_read_only(file_name) {
                return Err(format!("{} 是系统生成文件，当前不允许直接修改", file_name));
            }
            return Ok(root.join(file_name));
        }
    }

    if normalized.len() == 3
        && normalized[0] == "agents"
        && normalized[1] == agent_id
        && AGENT_VIEW_FILES.contains(&normalized[2].as_str())
    {
        return Ok(root.join("agents").join(agent_id).join(&normalized[2]));
    }

    if normalized.len() == 4
        && normalized[0] == "agents"
        && normalized[1] == agent_id
        && normalized[2] == "memory"
        && normalized[3].ends_with(".md")
    {
        return Ok(root
            .join("agents")
            .join(agent_id)
            .join("memory")
            .join(&normalized[3]));
    }

    Err("只允许修改当前智能体可写的 workspace markdown 文件".to_string())
}

fn normalize_relative_workspace_path(relative_path: &str) -> Result<Vec<String>, String> {
    let trimmed = relative_path.trim();
    if trimmed.is_empty() {
        return Err("workspace 文件路径不能为空".to_string());
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err("workspace 文件路径必须是相对路径".to_string());
    }

    let mut segments = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => segments.push(segment.to_string_lossy().to_string()),
            Component::CurDir => continue,
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => {
                return Err("workspace 文件路径非法".to_string())
            }
        }
    }

    if segments.is_empty() {
        return Err("workspace 文件路径不能为空".to_string());
    }

    Ok(segments)
}

fn read_workspace_root_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn migrate_legacy_workspace_if_needed(home: &Path, target_root: &Path) -> Result<(), String> {
    if target_root.exists() {
        return Ok(());
    }

    let legacy_root = home.join(LEGACY_WORKSPACE_RELATIVE_PATH);
    if !legacy_root.exists() {
        return Ok(());
    }

    if let Some(parent) = target_root.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建 NineClaw workspace 父目录失败: {error}"))?;
    }

    copy_dir_all(&legacy_root, target_root).map_err(|error| {
        format!(
            "迁移旧 workspace 到 {} 失败: {error}",
            target_root.display()
        )
    })
}

fn copy_dir_all(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::create_dir_all(target)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_all(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path)?;
        }
    }

    Ok(())
}

fn apply_placeholders(mut template: String, seed: AgentWorkspaceSeed<'_>) -> String {
    let replacements = [
        ("{{AGENT_ID}}", seed.id),
        ("{{AGENT_NAME}}", seed.name),
        ("{{AGENT_SUMMARY}}", seed.summary),
        ("{{AGENT_DESCRIPTION}}", seed.description),
        (
            "{{AGENT_ACCENT_COLOR}}",
            seed.accent_color.unwrap_or("#7C5CFA"),
        ),
    ];

    for (needle, value) in replacements {
        template = template.replace(needle, value);
    }

    template
}

fn current_date_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    format_timestamp(seconds, "%Y-%m-%d")
}

fn current_timestamp_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    format_timestamp(seconds, "%Y-%m-%d %H:%M:%S")
}

fn format_timestamp(timestamp_secs: i64, pattern: &str) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(timestamp_secs, 0)
        .single()
        .map(|value| value.format(pattern).to_string())
        .unwrap_or_else(|| "1970-01-01 00:00:00".to_string())
}

fn summarize_memory_entry(user_message: &str, assistant_message: &str) -> String {
    let user_compact = user_message.replace('\n', " ").trim().to_string();
    let assistant_compact = assistant_message.replace('\n', " ").trim().to_string();
    let user_summary = truncate_for_memory(&user_compact, 80);
    let assistant_summary = truncate_for_memory(&assistant_compact, 100);
    format!("用户提到：{}；智能体回复：{}", user_summary, assistant_summary)
}

fn truncate_for_memory(value: &str, limit: usize) -> String {
    let mut truncated = String::new();
    for char in value.chars().take(limit) {
        truncated.push(char);
    }
    if value.chars().count() > limit {
        truncated.push('…');
    }
    truncated
}

fn upsert_working_memory(existing: &str, user_id: &str, timestamp: &str, summary: &str) -> String {
    let marker = "## IM Latest Context";
    let replacement = format!(
        "{marker}\n\n- Last user: `{}`\n- Updated at: {}\n- Summary: {}\n",
        user_id, timestamp, summary
    );

    if let Some(index) = existing.find(marker) {
        let prefix = existing[..index].trim_end();
        format!("{prefix}\n\n{replacement}\n")
    } else {
        let base = existing.trim_end();
        format!("{base}\n\n{replacement}\n")
    }
}

fn append_memory_summary(existing: &str, user_id: &str, timestamp: &str, summary: &str) -> String {
    let mut next = existing.trim_end().to_string();
    if !next.contains("## IM Persistent Memory") {
        next.push_str("\n\n## IM Persistent Memory\n");
    }
    let _ = writeln!(next, "- {} | `{}` | {}", timestamp, user_id, summary);
    next.push('\n');
    next
}

fn append_daily_log_entry(
    existing: &str,
    user_id: &str,
    timestamp: &str,
    user_message: &str,
    assistant_message: &str,
) -> String {
    let mut next = existing.trim_end().to_string();
    if !next.ends_with('\n') {
        next.push('\n');
    }
    let _ = writeln!(next, "\n## {} · {}", timestamp, user_id);
    let _ = writeln!(next, "\n### User\n\n{}", user_message.trim());
    let _ = writeln!(next, "\n### Agent\n\n{}", assistant_message.trim());
    next.push('\n');
    next
}

fn fallback_template(file_name: &str) -> &'static str {
    match file_name {
        "BOOTSTRAP.md" => {
            "# BOOTSTRAP.md - New Agent First Run\n\n- Clarify the agent name, mission, and boundaries\n- Update IDENTITY.md, ROLE.md, MEMORY.md, TOOLS.md, and PUBLIC_CONTEXT.md\n- Write one note into today's daily log\n- Delete this file when done\n"
        }
        "IDENTITY.md" => {
            "# IDENTITY.md\n\n- **Agent ID:** {{AGENT_ID}}\n- **Name:** {{AGENT_NAME}}\n- **Creature:** 智能体\n- **Vibe:** 高效、直接、少废话\n- **Emoji:** 🤖\n- **Accent Color:** {{AGENT_ACCENT_COLOR}}\n\n## Identity Notes\n\n- Summary: {{AGENT_SUMMARY}}\n"
        }
        "ROLE.md" => {
            "# ROLE.md\n\n## Mission\n\n{{AGENT_DESCRIPTION}}\n\n## Ownership\n\n- Define ownership here\n\n## Do Not\n\n- Leak private memory\n- Act externally without confirmation\n"
        }
        "MEMORY.md" => {
            "# MEMORY.md\n\nThis is the curated private memory for `{{AGENT_NAME}}`.\n"
        }
        "TOOLS.md" => {
            "# TOOLS.md\n\n## Tool Bias\n\n- Preferred tools:\n- Avoid when possible:\n"
        }
        "HEARTBEAT.md" => {
            "# HEARTBEAT.md\n\n# Keep empty if this agent owns no periodic checks.\n"
        }
        "WORKING.md" => {
            "# WORKING.md\n\n## Current Focus\n\n- No active task yet\n"
        }
        "DECISIONS.md" => {
            "# DECISIONS.md\n\n## Decision Log\n\n- No decisions logged yet\n"
        }
        "PUBLIC_CONTEXT.md" => {
            "# PUBLIC_CONTEXT.md\n\n## Safe Identity\n\n- Agent: {{AGENT_NAME}}\n- Summary: {{AGENT_SUMMARY}}\n"
        }
        _ => "",
    }
}

fn root_fallback_template(file_name: &str) -> &'static str {
    match file_name {
        "AGENTS.md" => {
            "# AGENTS.md - NineClaw Agent Workspace Protocol\n\nRoot files are shared memory. `agents/<agent-id>/` is private memory.\n"
        }
        "SOUL.md" => {
            "# SOUL.md\n\n- Be helpful, direct, and competent\n- Do not leak private data\n- Ask before external actions\n"
        }
        "USER.md" => {
            "# USER.md\n\n- **Name:**\n- **What to call them:**\n- **Timezone:**\n- **Notes:**\n"
        }
        "MEMORY.md" => {
            "# MEMORY.md - System Memory\n\nPut only stable, cross-agent facts here.\n"
        }
        "TOOLS.md" => {
            "# TOOLS.md - System Tools\n\nPut shared environment notes here. Do not store live secrets in markdown.\n"
        }
        "HEARTBEAT.md" => {
            "# HEARTBEAT.md - System Heartbeat\n\n# Keep empty to skip system-level heartbeat work.\n"
        }
        _ => "",
    }
}

fn agents_readme_fallback() -> &'static str {
    "# agents/README.md\n\nEach subdirectory here is an agent home.\n"
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

fn display_workspace_root(root: &Path) -> String {
    if let Some(home) = home_dir() {
        if let Ok(relative) = root.strip_prefix(&home) {
            if relative.as_os_str().is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", relative.display());
        }
    }

    root.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn workspace_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("nineclaw-agent-workspace-{nonce}"))
    }

    #[test]
    fn scaffold_agent_home_from_templates() {
        let _guard = workspace_test_lock().lock().expect("lock workspace test");
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "test-agent",
            name: "测试代理",
            summary: "用于验证脚手架",
            description: "负责验证 agent workspace 脚手架逻辑",
            accent_color: Some("#112233"),
            is_builtin: false,
        };

        let home = ensure_agent_workspace(seed, true).expect("scaffold workspace");
        assert!(root.join("AGENTS.md").exists());
        assert!(root.join("SOUL.md").exists());
        assert!(root.join("USER.md").exists());
        assert!(root.join("MEMORY.md").exists());
        assert!(root.join("TOOLS.md").exists());
        assert!(root.join("HEARTBEAT.md").exists());
        assert!(home.join("IDENTITY.md").exists());
        assert!(home.join("ROLE.md").exists());
        assert!(home.join("BOOTSTRAP.md").exists());

        let identity = fs::read_to_string(home.join("IDENTITY.md")).expect("read identity");
        assert!(identity.contains("测试代理"));
        assert!(identity.contains("#112233"));

        let prompt = build_workspace_system_prompt("test-agent").expect("workspace prompt");
        assert!(prompt.contains("BOOTSTRAP.md"));
        assert!(prompt.contains("agents/test-agent"));

        let bundle = read_agent_workspace_bundle("test-agent").expect("workspace bundle");
        assert_eq!(bundle.agent_id, "test-agent");
        assert!(bundle
            .files
            .iter()
            .any(|file| file.relative_path == "AGENTS.md"));
        assert!(bundle
            .files
            .iter()
            .any(|file| file.relative_path == "agents/test-agent/IDENTITY.md"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn write_agent_workspace_file_updates_private_memory() {
        let _guard = workspace_test_lock().lock().expect("lock workspace test");
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "writer",
            name: "写手",
            summary: "用于验证保存",
            description: "负责验证 workspace 写入逻辑",
            accent_color: Some("#223344"),
            is_builtin: false,
        };

        ensure_agent_workspace(seed, true).expect("scaffold workspace");

        let bundle = write_agent_workspace_file(
            "writer",
            "agents/writer/MEMORY.md",
            "# MEMORY.md\n\nupdated\n",
        )
        .expect("write workspace file");

        let saved = fs::read_to_string(root.join("agents").join("writer").join("MEMORY.md"))
            .expect("read saved memory");
        assert_eq!(saved, "# MEMORY.md\n\nupdated\n");
        assert!(bundle.files.iter().any(|file| {
            file.relative_path == "agents/writer/MEMORY.md" && file.content == saved
        }));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn reject_generated_or_out_of_scope_workspace_paths() {
        let _guard = workspace_test_lock().lock().expect("lock workspace test");
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "guard",
            name: "守卫",
            summary: "用于验证路径边界",
            description: "负责验证写入白名单",
            accent_color: Some("#445566"),
            is_builtin: false,
        };

        ensure_agent_workspace(seed, true).expect("scaffold workspace");

        let registry_error = write_agent_workspace_file("guard", "AGENT_REGISTRY.md", "nope")
            .expect_err("reject generated registry");
        assert!(registry_error.contains("系统生成文件"));

        let scope_error = write_agent_workspace_file("guard", "agents/other/MEMORY.md", "nope")
            .expect_err("reject foreign agent path");
        assert!(scope_error.contains("只允许修改当前智能体可写的 workspace markdown 文件"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }
}
