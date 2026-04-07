#![allow(dead_code)]

mod memory_wiki;

use serde::Serialize;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};

const DEFAULT_WORKSPACE_RELATIVE_PATH: &str = ".nineclaw/workspace";
const LEGACY_WORKSPACE_RELATIVE_PATH: &str = ".openclaw/workspace";
const PRIMARY_WORKSPACE_ROOT_ENV: &str = "NINECLAW_WORKSPACE_ROOT";
const LEGACY_WORKSPACE_ROOT_ENVS: &[&str] = &["NINECLAW_AGENT_WORKSPACE_ROOT"];
/// 设为 `1` / `true` / `yes` 时，每次 ingest 仍向 `memory/categories/*.md` 追加条目（易成流水账；默认关闭）。
const APPEND_CATEGORY_MEMORY_ENV: &str = "NINECLAW_APPEND_CATEGORY_MEMORY_ON_INGEST";
const TEMPLATE_DIR: &str = "agents/_templates";
const CATEGORY_MEMORY_DIR: &str = "memory/categories";
const WORKSPACE_SYSTEM_PROMPT_CHAR_LIMIT: usize = 1200;
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
struct MemoryCategoryDefinition {
    key: &'static str,
    title: &'static str,
    description: &'static str,
    storage_keywords: &'static [&'static str],
    query_keywords: &'static [&'static str],
}

fn append_category_memory_on_ingest() -> bool {
    std::env::var(APPEND_CATEGORY_MEMORY_ENV)
        .map(|value| {
            let value = value.trim().to_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes")
        })
        .unwrap_or(false)
}

const MEMORY_CATEGORY_DEFINITIONS: &[MemoryCategoryDefinition] = &[
    MemoryCategoryDefinition {
        key: "general",
        title: "GENERAL_MEMORY",
        description: "兜底的通用稳定记忆，放未命中特定分类但值得长期保留的事实。",
        storage_keywords: &[],
        query_keywords: &[],
    },
    MemoryCategoryDefinition {
        key: "user_profile",
        title: "USER_PROFILE",
        description: "关于用户身份、背景、角色、公司、所在地、时区等稳定画像。",
        storage_keywords: &[
            "我是",
            "我的职位",
            "我的角色",
            "我的公司",
            "我负责",
            "我在",
            "我来自",
            "我住在",
            "时区",
            "背景",
            "职位",
            "角色",
            "公司",
            "城市",
            "where i am",
            "my role",
            "my company",
            "background",
        ],
        query_keywords: &[
            "我是谁",
            "我的信息",
            "我的背景",
            "我的职位",
            "我的角色",
            "我的公司",
            "我的时区",
            "我的城市",
            "user profile",
            "background",
            "who am i",
            "my role",
            "my company",
        ],
    },
    MemoryCategoryDefinition {
        key: "preferences",
        title: "PREFERENCES",
        description: "用户偏好、口吻、格式要求、称呼习惯、喜欢或避开的做法。",
        storage_keywords: &[
            "偏好",
            "喜欢",
            "不喜欢",
            "习惯",
            "风格",
            "口吻",
            "称呼",
            "叫我",
            "输出格式",
            "尽量",
            "不要",
            "prefer",
            "like",
            "dislike",
            "format",
            "tone",
        ],
        query_keywords: &[
            "我的偏好",
            "我喜欢什么",
            "我不喜欢什么",
            "按我的习惯",
            "按我的风格",
            "怎么称呼我",
            "我的格式要求",
            "preferences",
            "my preference",
            "my style",
            "call me",
        ],
    },
    MemoryCategoryDefinition {
        key: "projects",
        title: "PROJECTS",
        description: "项目、产品、代码库、需求、架构、发布计划等长期项目上下文。",
        storage_keywords: &[
            "项目",
            "产品",
            "代码库",
            "仓库",
            "repo",
            "需求",
            "功能",
            "版本",
            "发布",
            "架构",
            "roadmap",
            "milestone",
            "feature",
            "project",
        ],
        query_keywords: &[
            "这个项目",
            "我们的项目",
            "项目背景",
            "项目计划",
            "产品规划",
            "代码库",
            "仓库",
            "需求",
            "架构",
            "roadmap",
            "repo",
            "project",
            "feature",
            "architecture",
        ],
    },
    MemoryCategoryDefinition {
        key: "commitments",
        title: "COMMITMENTS",
        description: "待办、承诺、截止时间、下一步、提醒事项、跟进动作。",
        storage_keywords: &[
            "待办",
            "todo",
            "提醒",
            "记得",
            "跟进",
            "截止",
            "ddl",
            "明天",
            "下周",
            "下一步",
            "交付",
            "完成",
            "安排",
            "计划",
            "会去做",
        ],
        query_keywords: &[
            "下一步",
            "待办",
            "我们要做什么",
            "后面怎么做",
            "截止",
            "提醒",
            "跟进",
            "计划",
            "安排",
            "todo",
            "next step",
            "deadline",
            "follow up",
        ],
    },
    MemoryCategoryDefinition {
        key: "decisions",
        title: "DECISIONS",
        description: "已经确认的决定、规范、规则、统一约定和禁用项。",
        storage_keywords: &[
            "决定",
            "确认",
            "定下来",
            "选用",
            "约定",
            "规则",
            "规范",
            "统一",
            "以后都",
            "必须",
            "不要再",
            "confirmed",
            "decision",
            "rule",
            "standard",
        ],
        query_keywords: &[
            "之前的决定",
            "之前确认",
            "已经定下来的",
            "规则",
            "规范",
            "约定",
            "统一方案",
            "禁用项",
            "decision",
            "confirmed",
            "rule",
            "standard",
        ],
    },
    MemoryCategoryDefinition {
        key: "relationships",
        title: "RELATIONSHIPS",
        description: "与用户相关的人、团队、客户、合作方、负责人等关系网络。",
        storage_keywords: &[
            "同事",
            "老板",
            "客户",
            "团队",
            "合作方",
            "朋友",
            "家人",
            "负责人",
            "联系人",
            "stakeholder",
            "owner",
            "manager",
            "client",
        ],
        query_keywords: &[
            "谁负责",
            "联系人",
            "客户是谁",
            "团队里",
            "老板",
            "同事",
            "合作方",
            "负责人",
            "关系人",
            "owner",
            "stakeholder",
            "client",
            "contact",
        ],
    },
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
    content.push_str("- `agents/<agent-id>/` is the private home of that agent\n");
    content.push_str("- Agent memory is isolated per agent home and must not be copied from other agent directories\n");
    content.push_str(
        "- Root files are system policy / operator notes, not cross-agent runtime memory\n",
    );
    content.push_str("- Runtime memory loading should only use the current agent home\n");
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

    fs::remove_dir_all(&agent_home).map_err(|error| format!("删除智能体工作区失败: {error}"))?;
    Ok(())
}

#[allow(dead_code)]
pub fn build_workspace_system_prompt(agent_id: &str) -> Result<String, String> {
    build_workspace_system_prompt_for_query(agent_id, None)
}

pub fn build_workspace_system_prompt_for_query(
    agent_id: &str,
    current_prompt: Option<&str>,
) -> Result<String, String> {
    let root = resolve_workspace_root()?;
    let agent_home = root.join("agents").join(agent_id);
    let bootstrap_exists = agent_home.join("BOOTSTRAP.md").exists();

    let mut sections = Vec::new();
    sections.push(
        "记忆规则：只使用当前 agent 私有记忆；命中不到就直说；禁止引用其他 agents/<id>/ 内容。"
            .to_string(),
    );
    sections.push(
        "写回：短期写 WORKING.md；长期请整理 memory/categories/*.md、DECISIONS.md、PUBLIC_CONTEXT.md。MEMORY.md 仅人设与核心原则，运行时不可自动写入。对话 ingest 默认只增长 raw、daily、SOURCE_INDEX/LOG；分类文件不自动追加（避免流水账）。若需恢复每次对话写分类，设置环境变量 NINECLAW_APPEND_CATEGORY_MEMORY_ON_INGEST=1。"
            .to_string(),
    );
    if bootstrap_exists {
        sections.push("BOOTSTRAP.md 存在：先按引导补齐身份文件，完成后删除。".to_string());
    }

    if let Some(wiki_snapshot) =
        memory_wiki::build_memory_wiki_snapshot(&agent_home, current_prompt)?
    {
        sections.push(wiki_snapshot);
    }
    sections.push(build_legacy_workspace_memory_snapshot(&root, agent_id)?);

    Ok(trim_to_char_limit(
        &sections.join("\n\n"),
        WORKSPACE_SYSTEM_PROMPT_CHAR_LIMIT,
    ))
}

pub fn read_agent_heartbeat_instructions(agent_id: &str) -> Result<Option<String>, String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;

    let shared = read_meaningful_heartbeat_file(
        root.join("HEARTBEAT.md"),
        root_fallback_template("HEARTBEAT.md"),
        3_200,
    )?;
    let agent_specific = read_meaningful_heartbeat_file(
        root.join("agents").join(agent_id).join("HEARTBEAT.md"),
        fallback_template("HEARTBEAT.md"),
        3_200,
    )?;

    let mut sections = Vec::new();
    if !shared.is_empty() {
        sections.push(format!(
            "以下是系统级 HEARTBEAT.md，请把它当作全局心跳规则：\n```md\n{}\n```",
            shared
        ));
    }
    if !agent_specific.is_empty() {
        sections.push(format!(
            "以下是当前智能体私有 HEARTBEAT.md，请优先遵守：\n```md\n{}\n```",
            agent_specific
        ));
    }

    if sections.is_empty() {
        Ok(None)
    } else {
        Ok(Some(sections.join("\n\n")))
    }
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
    let categories = classify_memory_categories(user_message, assistant_message, &summary);
    let category_notes =
        build_category_memory_notes(user_message, assistant_message, &summary, &categories);
    let ingest_summary = build_ingest_summary(&category_notes, &summary);
    let source_ref = memory_wiki::record_conversation_ingest(
        &agent_home,
        user_id,
        &timestamp,
        user_message,
        assistant_message,
        &ingest_summary,
        &categories,
    )?;

    let working_path = agent_home.join("WORKING.md");
    let working_existing = fs::read_to_string(&working_path)
        .unwrap_or_else(|_| fallback_template("WORKING.md").to_string());
    fs::write(
        &working_path,
        upsert_working_memory(
            &working_existing,
            user_id,
            &ingest_summary,
            &source_ref,
            &categories,
        ),
    )
    .map_err(|error| format!("写入 WORKING.md 失败: {error}"))?;

    // MEMORY.md 不再自动写入 — 仅保留人设和核心原则，由用户手动编辑
    if append_category_memory_on_ingest() {
        append_category_memory_entries(&agent_home, &category_notes, &source_ref)?;
    }

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
            &ingest_summary,
            &source_ref,
            &categories,
        ),
    )
    .map_err(|error| format!("写入 daily log 失败: {error}"))?;

    memory_wiki::refresh_memory_wiki(&agent_home)?;

    Ok(())
}

pub fn register_agent_attachment_source(
    agent_id: &str,
    title: &str,
    file_path: &Path,
    mime_type: Option<&str>,
    note: Option<&str>,
) -> Result<(), String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;

    let agent_home = root.join("agents").join(agent_id);
    fs::create_dir_all(agent_home.join("memory"))
        .map_err(|error| format!("创建 agent memory 目录失败: {error}"))?;
    let timestamp = current_timestamp_label();
    let daily_log_path = agent_home
        .join("memory")
        .join(format!("{}.md", current_date_label()));
    let existing = fs::read_to_string(&daily_log_path)
        .unwrap_or_else(|_| format!("# {}\n\n", current_date_label()));
    let attachment_message = match mime_type.filter(|value| !value.trim().is_empty()) {
        Some(mime) => format!(
            "收到附件：{}（mime={}，path=`{}`）",
            title,
            mime,
            file_path.display()
        ),
        None => format!("收到附件：{}（path=`{}`）", title, file_path.display()),
    };
    memory_wiki::record_attachment_source(&agent_home, &timestamp, title, file_path, mime_type, note)?;
    fs::write(
        &daily_log_path,
        append_daily_log_entry(
            &existing,
            "attachment",
            &timestamp,
            &attachment_message,
            &file_path.display().to_string(),
            &[],
        ),
    )
    .map_err(|error| format!("写入附件 daily log 失败: {error}"))?;
    memory_wiki::refresh_memory_wiki(&agent_home)?;
    Ok(())
}

pub fn persist_agent_inbound_artifact(
    agent_id: &str,
    user_id: &str,
    file_name: &str,
    data: &[u8],
) -> Result<PathBuf, String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;

    let inbox_dir = root
        .join("agents")
        .join(agent_id)
        .join("inbox")
        .join(current_date_label());
    fs::create_dir_all(&inbox_dir).map_err(|error| format!("创建智能体收件目录失败: {error}"))?;

    let safe_user = sanitize_workspace_segment(user_id, "user");
    let safe_name = sanitize_workspace_file_name(file_name, "attachment.bin");
    let path = inbox_dir.join(format!(
        "{}-{}-{}",
        current_timestamp_file_label(),
        safe_user,
        safe_name
    ));

    fs::write(&path, data).map_err(|error| format!("写入智能体收件文件失败: {error}"))?;
    Ok(path)
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

fn build_legacy_workspace_memory_snapshot(root: &Path, agent_id: &str) -> Result<String, String> {
    let agent_home = root.join("agents").join(agent_id);
    let mut sections = Vec::new();

    let memory_content = fs::read_to_string(agent_home.join("MEMORY.md")).unwrap_or_default();
    let mut memory_bullets = extract_section_bullets(&memory_content, "## Core Memory");
    let preference_bullets = extract_section_bullets(&memory_content, "## 用户偏好");
    for item in preference_bullets {
        if memory_bullets
            .iter()
            .any(|existing| existing.trim().eq_ignore_ascii_case(item.trim()))
        {
            continue;
        }
        memory_bullets.push(item);
    }
    memory_bullets.retain(|item| !item.trim().is_empty());
    if !memory_bullets.is_empty() {
        let memory_excerpt = memory_bullets
            .into_iter()
            .take(4)
            .map(|item| format!("- {}", trim_to_char_limit(&item, 80)))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("MEMORY 核心点：\n{}", memory_excerpt));
    }

    let working_content = fs::read_to_string(agent_home.join("WORKING.md")).unwrap_or_default();
    let working_bullets = extract_section_bullets(&working_content, "## IM Latest Context")
        .into_iter()
        .filter(|item| !item.contains("Last user:"))
        .take(3)
        .map(|item| format!("- {}", trim_to_char_limit(&item, 90)))
        .collect::<Vec<_>>();
    if !working_bullets.is_empty() {
        sections.push(format!("WORKING 当前上下文：\n{}", working_bullets.join("\n")));
    }

    let decisions_content = fs::read_to_string(agent_home.join("DECISIONS.md")).unwrap_or_default();
    let decision_bullets = extract_section_bullets(&decisions_content, "## Decision Log")
        .into_iter()
        .filter(|item| !item.contains("No decisions logged yet"))
        .take(3)
        .map(|item| format!("- {}", trim_to_char_limit(&item, 80)))
        .collect::<Vec<_>>();
    if !decision_bullets.is_empty() {
        sections.push(format!("DECISIONS：\n{}", decision_bullets.join("\n")));
    }

    if sections.is_empty() {
        Ok("当前智能体还没有可加载的核心 md 记忆内容。".to_string())
    } else {
        Ok(format!(
            "核心记忆摘录：\n{}",
            sections.join("\n\n")
        ))
    }
}

fn build_categorized_memory_snapshot(
    root: &Path,
    agent_id: &str,
    current_prompt: Option<&str>,
) -> Result<Option<String>, String> {
    let agent_home = root.join("agents").join(agent_id);
    ensure_category_memory_scaffold(&agent_home)?;

    let selected_categories = select_memory_categories_for_query(current_prompt);
    let mut selected_labels = Vec::new();
    let mut sections = Vec::new();

    for category in selected_categories.into_iter().take(2) {
        let path = category_memory_file_path(&agent_home, category.key);
        if !path.exists() {
            continue;
        }

        let content = safe_read_trimmed(
            path,
            if category.key == "general" {
                380
            } else {
                260
            },
        )?;
        if content.is_empty() {
            continue;
        }

        selected_labels.push(category.title);
        sections.push(format!(
            "{}（{}）:\n{}",
            category.title, category.description, content
        ));
    }

    if sections.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!(
            "相关分类记忆（{}）：\n{}",
            selected_labels.join("、"),
            sections.join("\n\n")
        )))
    }
}

fn ensure_category_memory_scaffold(agent_home: &Path) -> Result<(), String> {
    let dir = category_memory_dir(agent_home);
    fs::create_dir_all(&dir).map_err(|error| format!("创建分类记忆目录失败: {error}"))?;

    let index_path = dir.join("INDEX.md");
    if !index_path.exists() {
        fs::write(&index_path, build_category_index_content())
            .map_err(|error| format!("写入分类记忆索引失败: {error}"))?;
    }

    Ok(())
}

fn category_memory_dir(agent_home: &Path) -> PathBuf {
    agent_home.join(CATEGORY_MEMORY_DIR)
}

fn category_memory_file_path(agent_home: &Path, key: &str) -> PathBuf {
    category_memory_dir(agent_home).join(format!("{key}.md"))
}

fn build_category_index_content() -> String {
    let mut content = String::from(
        "# INDEX.md - Categorized Memory\n\nThis directory stores curated memory shards grouped by topic so runtime can load only the most relevant memory for each question.\n\n## Categories\n\n",
    );

    for category in MEMORY_CATEGORY_DEFINITIONS {
        let _ = writeln!(
            content,
            "- `{}` / `{}`: {}",
            category.key, category.title, category.description
        );
    }

    content
}

fn build_category_file_template(category: MemoryCategoryDefinition) -> String {
    format!(
        "# {}.md\n\n## Purpose\n\n{}\n\n## Entries\n\n",
        category.title, category.description
    )
}

fn read_agent_category_memory_files(
    root: &Path,
    agent_id: &str,
) -> Result<Vec<AgentWorkspaceFile>, String> {
    let category_dir = root.join("agents").join(agent_id).join(CATEGORY_MEMORY_DIR);
    if !category_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(&category_dir)
        .map_err(|error| format!("读取分类记忆目录失败: {error}"))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let file_name = path.file_name()?.to_str()?.to_string();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md")
                || !memory_wiki::is_daily_log_file(&path)
            {
                return None;
            }
            Some((file_name, path))
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| left.0.cmp(&right.0));

    Ok(entries
        .into_iter()
        .map(|(file_name, path)| {
            let relative_path = PathBuf::from("agents")
                .join(agent_id)
                .join("memory")
                .join("categories")
                .join(&file_name);
            read_workspace_file("agent", "private", &file_name, relative_path, path, false)
        })
        .collect())
}

fn classify_memory_categories(
    user_message: &str,
    assistant_message: &str,
    summary: &str,
) -> Vec<MemoryCategoryDefinition> {
    let combined = normalize_memory_match_text(&format!(
        "{}\n{}\n{}",
        user_message, assistant_message, summary
    ));

    let mut categories = vec![memory_category_definition("general").expect("general category")];
    for category in MEMORY_CATEGORY_DEFINITIONS
        .iter()
        .copied()
        .filter(|category| category.key != "general")
    {
        if contains_any_keyword(&combined, category.storage_keywords) {
            categories.push(category);
        }
    }

    categories
}

fn select_memory_categories_for_query(
    current_prompt: Option<&str>,
) -> Vec<MemoryCategoryDefinition> {
    let mut categories = vec![memory_category_definition("general").expect("general category")];
    let prompt = normalize_memory_match_text(current_prompt.unwrap_or_default());

    if prompt.is_empty() {
        for key in ["projects", "commitments", "decisions"] {
            if let Some(category) = memory_category_definition(key) {
                categories.push(category);
            }
        }
        return categories;
    }

    for category in MEMORY_CATEGORY_DEFINITIONS
        .iter()
        .copied()
        .filter(|category| category.key != "general")
    {
        if contains_any_keyword(&prompt, category.query_keywords)
            || contains_any_keyword(&prompt, category.storage_keywords)
        {
            categories.push(category);
        }
    }

    if categories.len() == 1 {
        if contains_memory_recall_signal(&prompt) {
            categories.extend(
                MEMORY_CATEGORY_DEFINITIONS
                    .iter()
                    .copied()
                    .filter(|category| category.key != "general"),
            );
        } else {
            for key in ["projects", "commitments", "decisions"] {
                if let Some(category) = memory_category_definition(key) {
                    categories.push(category);
                }
            }
        }
    }

    categories
}

fn memory_category_definition(key: &str) -> Option<MemoryCategoryDefinition> {
    MEMORY_CATEGORY_DEFINITIONS
        .iter()
        .copied()
        .find(|category| category.key == key)
}

fn normalize_memory_match_text(value: &str) -> String {
    value.trim().to_lowercase()
}

fn contains_any_keyword(content: &str, keywords: &[&str]) -> bool {
    keywords
        .iter()
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| !keyword.is_empty())
        .any(|keyword| content.contains(&keyword))
}

fn contains_memory_recall_signal(content: &str) -> bool {
    contains_any_keyword(
        content,
        &[
            "之前",
            "上次",
            "继续",
            "还记得",
            "按我们之前",
            "以前",
            "历史",
            "记忆",
            "沉淀",
            "last time",
            "previously",
            "remember",
            "continue",
            "history",
        ],
    )
}

fn append_category_memory_entries(
    agent_home: &Path,
    notes: &[(String, String)],
    source_ref: &str,
) -> Result<(), String> {
    ensure_category_memory_scaffold(agent_home)?;

    for (category_key, note) in notes {
        let Some(category) = memory_category_definition(category_key) else {
            continue;
        };
        let path = category_memory_file_path(agent_home, category.key);
        let existing = if path.exists() {
            fs::read_to_string(&path).unwrap_or_default()
        } else {
            build_category_file_template(category)
        };

        fs::write(
            &path,
            append_category_memory_file(&existing, note, source_ref),
        )
        .map_err(|error| format!("写入分类记忆 {} 失败: {error}", path.display()))?;
    }

    Ok(())
}

fn append_category_memory_file(existing: &str, note: &str, source_ref: &str) -> String {
    let mut next = existing.trim_end().to_string();
    if !next.contains("## Entries") {
        next.push_str("\n\n## Entries\n");
    }

    if next.contains(note) {
        return format!("{next}\n");
    }

    let _ = writeln!(next, "\n- {}", note);
    let _ = writeln!(next, "  - Source: `{}`", source_ref);
    next.push('\n');
    next
}

fn safe_read_trimmed(path: PathBuf, limit: usize) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("读取 workspace 记忆文件 {} 失败: {error}", path.display()))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    Ok(trim_to_char_limit(trimmed, limit))
}

fn read_meaningful_heartbeat_file(
    path: PathBuf,
    placeholder: &str,
    limit: usize,
) -> Result<String, String> {
    let content = safe_read_trimmed(path, limit)?;
    if content.is_empty() {
        return Ok(String::new());
    }

    if normalize_heartbeat_instruction_text(&content)
        == normalize_heartbeat_instruction_text(placeholder)
    {
        return Ok(String::new());
    }

    Ok(content)
}

fn normalize_heartbeat_instruction_text(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn trim_to_char_limit(value: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }

    let mut result = String::new();
    let mut chars = value.chars();
    for _ in 0..limit {
        match chars.next() {
            Some(ch) => result.push(ch),
            None => return result,
        }
    }

    if chars.next().is_some() {
        result.push('…');
    }

    result
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
            if !is_agent_daily_log_name(&file_name) {
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

fn is_agent_daily_log_name(file_name: &str) -> bool {
    if file_name.len() != 13 || !file_name.ends_with(".md") {
        return false;
    }

    let stem = &file_name[..10];
    let bytes = stem.as_bytes();
    bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
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

    if normalized.len() == 5
        && normalized[0] == "agents"
        && normalized[1] == agent_id
        && normalized[2] == "memory"
        && normalized[3] == "categories"
        && normalized[4].ends_with(".md")
    {
        return Ok(root
            .join("agents")
            .join(agent_id)
            .join("memory")
            .join("categories")
            .join(&normalized[4]));
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

fn current_timestamp_file_label() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    millis.to_string()
}

fn summarize_memory_entry(user_message: &str, assistant_message: &str) -> String {
    let user_compact = user_message.replace('\n', " ").trim().to_string();
    let assistant_compact = assistant_message.replace('\n', " ").trim().to_string();
    let user_summary = truncate_for_memory(&user_compact, 80);
    let assistant_summary = truncate_for_memory(&assistant_compact, 100);
    format!(
        "用户提到：{}；智能体回复：{}",
        user_summary, assistant_summary
    )
}

fn compact_memory_text(value: &str, limit: usize) -> String {
    truncate_for_memory(&value.replace('\n', " ").trim().to_string(), limit)
}

fn build_category_memory_notes(
    user_message: &str,
    assistant_message: &str,
    summary: &str,
    categories: &[MemoryCategoryDefinition],
) -> Vec<(String, String)> {
    let mut notes = Vec::new();
    let user_compact = compact_memory_text(user_message, 120);
    let assistant_compact = compact_memory_text(assistant_message, 120);
    let summary_compact = compact_memory_text(summary, 120);

    for category in categories {
        let note = match category.key {
            "user_profile" if !user_compact.is_empty() => format!("用户画像：{}", user_compact),
            "preferences" if !user_compact.is_empty() => format!("用户偏好：{}", user_compact),
            "projects" if !user_compact.is_empty() => format!("项目上下文：{}", user_compact),
            "commitments" if !user_compact.is_empty() => format!("待跟进事项：{}", user_compact),
            "decisions" if !user_compact.is_empty() => format!("已确认约定：{}", user_compact),
            "relationships" if !user_compact.is_empty() => format!("人物关系：{}", user_compact),
            "general" if !summary_compact.is_empty() => format!("对话摘要：{}", summary_compact),
            _ if !summary_compact.is_empty() => format!("记忆摘录：{}", summary_compact),
            _ => continue,
        };
        notes.push((category.key.to_string(), note));
    }

    if notes.is_empty() && !assistant_compact.is_empty() {
        notes.push((
            "general".to_string(),
            format!("对话摘要：{}", assistant_compact),
        ));
    }

    dedupe_memory_notes(notes)
}

fn dedupe_memory_notes(notes: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut seen = Vec::new();
    let mut deduped = Vec::new();
    for (category, note) in notes {
        let normalized = note.trim().to_lowercase();
        if normalized.is_empty() || seen.iter().any(|item| item == &normalized) {
            continue;
        }
        seen.push(normalized);
        deduped.push((category, note));
    }
    deduped
}

fn format_category_titles(categories: &[MemoryCategoryDefinition]) -> Option<String> {
    let titles = categories
        .iter()
        .map(|category| category.title)
        .collect::<Vec<_>>();
    if titles.is_empty() {
        None
    } else {
        Some(titles.join("、"))
    }
}

fn select_core_memory_points(notes: &[(String, String)]) -> Vec<String> {
    notes.iter()
        .filter(|(category, _)| matches!(category.as_str(), "user_profile" | "preferences" | "decisions"))
        .map(|(_, note)| note.clone())
        .take(12)
        .collect()
}

fn build_ingest_summary(notes: &[(String, String)], fallback: &str) -> String {
    let selected = notes
        .iter()
        .filter(|(category, _)| category != "general")
        .take(2)
        .map(|(_, note)| note.as_str())
        .collect::<Vec<_>>();

    if !selected.is_empty() {
        return truncate_for_memory(&selected.join("；"), 180);
    }

    truncate_for_memory(fallback, 180)
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

fn sanitize_workspace_segment(value: &str, fallback: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();

    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

fn sanitize_workspace_file_name(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    let candidate = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    let path = Path::new(candidate);
    let file_name = path
        .file_name()
        .and_then(|item| item.to_str())
        .unwrap_or(fallback);
    sanitize_workspace_segment(file_name, fallback)
}

fn upsert_working_memory(
    existing: &str,
    user_id: &str,
    summary: &str,
    source_ref: &str,
    categories: &[MemoryCategoryDefinition],
) -> String {
    let marker = "## IM Latest Context";
    let category_titles = format_category_titles(categories).unwrap_or_else(|| "GENERAL_MEMORY".to_string());
    let replacement = format!(
        "{marker}\n\n- Last user: `{}`\n- Current note: {}\n- Categories: {}\n- Source: `{}`\n",
        user_id, summary, category_titles, source_ref
    );

    if let Some(index) = existing.find(marker) {
        let prefix = existing[..index].trim_end();
        format!("{prefix}\n\n{replacement}\n")
    } else {
        let base = existing.trim_end();
        format!("{base}\n\n{replacement}\n")
    }
}

fn append_core_memory_points(existing: &str, points: &[String]) -> String {
    let marker = "## Core Memory";
    let mut entries = extract_section_bullets(existing, marker);
    for point in points {
        let normalized = point.trim().to_lowercase();
        if normalized.is_empty()
            || entries
                .iter()
                .any(|item| item.trim().eq_ignore_ascii_case(point.trim()))
        {
            continue;
        }
        entries.push(point.trim().to_string());
    }

    if entries.len() > 12 {
        entries = entries.split_off(entries.len() - 12);
    }

    let cleaned = strip_markdown_section(
        &strip_markdown_section(existing, "## IM Persistent Memory"),
        marker,
    );
    let mut next = cleaned.trim_end().to_string();

    if !entries.is_empty() {
        next.push_str(&format!("\n\n{marker}\n"));
        for entry in entries {
            next.push_str(&format!("\n- {}\n", entry));
        }
    }
    next.push('\n');
    next
}

fn extract_section_bullets(existing: &str, marker: &str) -> Vec<String> {
    let mut bullets = Vec::new();
    let mut in_section = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == marker {
            in_section = true;
            continue;
        }
        if in_section && trimmed.starts_with("## ") {
            break;
        }
        if in_section && trimmed.starts_with("- ") {
            bullets.push(trimmed.trim_start_matches("- ").trim().to_string());
        }
    }

    bullets
}

fn strip_markdown_section(existing: &str, marker: &str) -> String {
    let mut next = String::new();
    let mut in_section = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == marker {
            in_section = true;
            continue;
        }
        if in_section && trimmed.starts_with("## ") {
            in_section = false;
        }
        if !in_section {
            next.push_str(line);
            next.push('\n');
        }
    }

    next.trim_end().to_string()
}

fn append_daily_log_entry(
    existing: &str,
    user_id: &str,
    timestamp: &str,
    summary: &str,
    source_ref: &str,
    categories: &[MemoryCategoryDefinition],
) -> String {
    let mut next = existing.trim_end().to_string();
    if !next.ends_with('\n') {
        next.push('\n');
    }
    let _ = writeln!(next, "\n## {} · {}", timestamp, user_id);
    let _ = writeln!(next, "\n- Summary: {}", summary);
    let _ = writeln!(
        next,
        "- Categories: {}",
        format_category_titles(categories).unwrap_or_else(|| "GENERAL_MEMORY".to_string())
    );
    let _ = writeln!(next, "- Source: `{}`", source_ref);
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
            "# MEMORY.md\n\n这是 `{{AGENT_NAME}}` 的核心记忆文件。\n\n## 人设\n\n- 性格基调：（由用户或 BOOTSTRAP 定义）\n- 说话风格：（由用户或 BOOTSTRAP 定义）\n\n## 核心原则\n\n- （由用户或 BOOTSTRAP 定义，如嫡系理念、活人感等原则）\n\n> 此文件不会在对话过程中被自动修改。如需调整，请手动编辑。\n"
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
            "# AGENTS.md - NineClaw Agent Workspace Protocol\n\nRoot files are system policy notes only. `agents/<agent-id>/` is private memory.\n"
        }
        "SOUL.md" => {
            "# SOUL.md\n\n- Be helpful, direct, and competent\n- Do not leak private data\n- Ask before external actions\n"
        }
        "USER.md" => {
            "# USER.md\n\n- **Name:**\n- **What to call them:**\n- **Timezone:**\n- **Notes:**\n"
        }
        "MEMORY.md" => {
            "# MEMORY.md - System Notes\n\nDeprecated as runtime memory source. Keep only operator-level notes here, not agent memory.\n"
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

    fn lock_workspace_test() -> std::sync::MutexGuard<'static, ()> {
        workspace_test_lock()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
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
        let _guard = lock_workspace_test();
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

        let bundle = read_agent_workspace_bundle("test-agent").expect("workspace bundle");
        assert_eq!(bundle.agent_id, "test-agent");
        assert_eq!(bundle.agent_home, "agents/test-agent");
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
        let _guard = lock_workspace_test();
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
        let _guard = lock_workspace_test();
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

    #[test]
    fn append_agent_memory_entry_writes_raw_log_and_skips_category_shards_by_default() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);
        std::env::remove_var(APPEND_CATEGORY_MEMORY_ENV);

        let seed = AgentWorkspaceSeed {
            id: "memory-agent",
            name: "记忆助理",
            summary: "用于验证原始记忆模式",
            description: "负责把记忆写回核心文件",
            accent_color: Some("#556677"),
            is_builtin: false,
        };

        ensure_agent_workspace(seed, true).expect("scaffold workspace");
        append_agent_memory_entry(
            "memory-agent",
            "user-1",
            "我是产品经理，这个项目下周要上线，之后统一按周报格式同步。",
            "收到，我会继续按周报格式跟进上线计划，并保留这个约定。",
        )
        .expect("append legacy memory");

        let memory = fs::read_to_string(root.join("agents").join("memory-agent").join("MEMORY.md"))
            .expect("read memory");
        // MEMORY.md 不再被自动写入，应保持模板内容
        assert!(memory.contains("核心记忆文件") || memory.contains("核心原则"));
        assert!(!memory.contains("## Core Memory"));
        assert!(!memory.contains("用户画像"));

        let user_profile_path = root
            .join("agents")
            .join("memory-agent")
            .join("memory")
            .join("categories")
            .join("user_profile.md");
        assert!(
            !user_profile_path.exists(),
            "category shards should not be auto-appended by default"
        );

        let working =
            fs::read_to_string(root.join("agents").join("memory-agent").join("WORKING.md"))
                .expect("read working");
        assert!(working.contains("user-1"));
        assert!(working.contains("Current note"));
        assert!(working.contains("memory/raw/"));

        let source_index = fs::read_to_string(
            root.join("agents")
                .join("memory-agent")
                .join("memory")
                .join("SOURCE_INDEX.md"),
        )
        .expect("read source index");
        assert!(source_index.contains("memory/raw/"));
        assert!(source_index.contains("Index: type=conversation"));

        let wiki_index = fs::read_to_string(
            root.join("agents")
                .join("memory-agent")
                .join("memory")
                .join("WIKI_INDEX.md"),
        )
        .expect("read wiki index");
        assert!(wiki_index.contains("Topic map"));

        let raw_source_dir = root
            .join("agents")
            .join("memory-agent")
            .join("memory")
            .join("raw")
            .join(current_date_label());
        assert!(raw_source_dir.exists());

        let daily_log = fs::read_to_string(
            root.join("agents")
                .join("memory-agent")
                .join("memory")
                .join(format!("{}.md", current_date_label())),
        )
        .expect("read daily log");
        assert!(daily_log.contains("Summary:"));
        assert!(daily_log.contains("Source:"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn append_agent_memory_entry_appends_category_shards_when_env_enabled() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);
        std::env::set_var(APPEND_CATEGORY_MEMORY_ENV, "1");

        let seed = AgentWorkspaceSeed {
            id: "memory-agent-env",
            name: "记忆助理",
            summary: "验证环境变量写回分类",
            description: "负责验证分类追加",
            accent_color: Some("#556677"),
            is_builtin: false,
        };

        ensure_agent_workspace(seed, true).expect("scaffold workspace");
        append_agent_memory_entry(
            "memory-agent-env",
            "user-1",
            "我是产品经理，这个项目下周要上线，之后统一按周报格式同步。",
            "收到，我会继续按周报格式跟进上线计划，并保留这个约定。",
        )
        .expect("append memory with categories");

        let user_profile = fs::read_to_string(
            root.join("agents")
                .join("memory-agent-env")
                .join("memory")
                .join("categories")
                .join("user_profile.md"),
        )
        .expect("read user_profile category");
        assert!(user_profile.contains("用户画像"));

        let decisions = fs::read_to_string(
            root.join("agents")
                .join("memory-agent-env")
                .join("memory")
                .join("categories")
                .join("decisions.md"),
        )
        .expect("read decisions category");
        assert!(decisions.contains("已确认约定") || decisions.contains("周报格式"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
        std::env::remove_var(APPEND_CATEGORY_MEMORY_ENV);
    }

    #[test]
    fn workspace_prompt_loads_relevant_categorized_memory_for_query() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "query-agent",
            name: "查询助理",
            summary: "用于验证分类查询",
            description: "负责按问题抽取记忆",
            accent_color: Some("#778899"),
            is_builtin: false,
        };

        ensure_agent_workspace(seed, true).expect("scaffold workspace");
        append_agent_memory_entry(
            "query-agent",
            "user-2",
            "这个项目的发布节奏定成双周一次，我喜欢你用表格给我汇报。",
            "明白，我会记住双周发布节奏，并且后续优先用表格汇报。",
        )
        .expect("append legacy memory");

        let prompt = build_workspace_system_prompt_for_query(
            "query-agent",
            Some("继续这个项目的发布规划，并按我之前喜欢的汇报格式整理。"),
        )
        .expect("workspace prompt");

        assert!(prompt.contains("WIKI_INDEX.md"));
        assert!(prompt.contains("SOURCE_INDEX.md"));
        assert!(prompt.contains("MEMORY.md"));
        assert!(prompt.contains("WORKING.md"));
        assert!(prompt.contains("核心记忆摘录"));
        assert!(!prompt.contains("独立记忆检索工具返回的结果"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }
}
