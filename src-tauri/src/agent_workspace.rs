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
const TEMPLATE_DIR: &str = "agents/_template";
const LEGACY_TEMPLATE_DIR: &str = "agents/_templates";
const CATEGORY_MEMORY_DIR: &str = "memory/categories";
const WORKSPACE_SYSTEM_PROMPT_CHAR_LIMIT: usize = 1600;
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
    "USER_MODEL.md",
    "RELATIONSHIP_MAP.md",
    "PITFALLS.md",
    "TOOLS.md",
    "HEARTBEAT.md",
    "WORKING.md",
    "DECISIONS.md",
    "PUBLIC_CONTEXT.md",
];
const AGENT_TEMPLATE_FILES: &[&str] = &[
    "IDENTITY.md",
    "ROLE.md",
    "MEMORY.md",
    "USER_MODEL.md",
    "RELATIONSHIP_MAP.md",
    "PITFALLS.md",
    "TOOLS.md",
    "HEARTBEAT.md",
    "WORKING.md",
    "DECISIONS.md",
    "PUBLIC_CONTEXT.md",
    "memory/INDEX.md",
    "memory/REVIEW_QUEUE.md",
    "memory/LINT.md",
    "memory/SOURCE_INDEX.md",
    "memory/LOG.md",
    "memory/categories/INDEX.md",
    "memory/categories/general.md",
    "memory/categories/user_profile.md",
    "memory/categories/preferences.md",
    "memory/categories/projects.md",
    "memory/categories/commitments.md",
    "memory/categories/decisions.md",
    "memory/categories/relationships.md",
    "memory/categories/pitfalls.md",
    "memory/categories/inferences.md",
    "wiki/INDEX.md",
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
    "USER_MODEL.md",
    "RELATIONSHIP_MAP.md",
    "PITFALLS.md",
    "TOOLS.md",
    "WORKING.md",
    "DECISIONS.md",
    "PUBLIC_CONTEXT.md",
    "HEARTBEAT.md",
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
        description: "兜底稳定记忆，只收没有更好归属但值得长期保留的内容，禁止把它当垃圾桶。",
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
    MemoryCategoryDefinition {
        key: "pitfalls",
        title: "PITFALLS",
        description: "容易翻车的判断、用户明确纠正过的点、以及必须避免的话术或处理方式。",
        storage_keywords: &[
            "别",
            "不要再",
            "记错",
            "误判",
            "翻车",
            "坑",
            "时间线",
            "别问",
            "别说",
            "不要废话",
            "pitfall",
            "mistake",
            "error",
            "wrong",
        ],
        query_keywords: &[
            "容易犯错",
            "坑点",
            "别再犯",
            "注意点",
            "高风险",
            "pitfall",
            "mistake",
            "error",
        ],
    },
    MemoryCategoryDefinition {
        key: "inferences",
        title: "INFERENCES",
        description: "尚未被用户明确确认的推断层。可辅助判断，但不能冒充确定事实。",
        storage_keywords: &[
            "我猜",
            "推测",
            "大概",
            "看起来像",
            "倾向于",
            "可能更喜欢",
            "inference",
            "guess",
            "seems like",
            "likely",
        ],
        query_keywords: &[
            "有哪些推断",
            "你推断过什么",
            "哪些还没确认",
            "tentative",
            "inference",
            "assumption",
            "guess",
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
    _create_bootstrap: bool,
) -> Result<PathBuf, String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;

    let agent_home = root.join("agents").join(seed.id);
    fs::create_dir_all(agent_home.join("memory"))
        .map_err(|error| format!("创建智能体 memory 目录失败: {error}"))?;

    for relative_path in AGENT_TEMPLATE_FILES {
        let target = agent_home.join(relative_path);
        if target.exists() {
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("创建智能体模板目录失败 {}: {error}", parent.display()))?;
        }
        let content =
            render_template(&root, relative_path, fallback_template(relative_path), seed)?;
        fs::write(&target, content)
            .map_err(|error| format!("写入智能体文件 {} 失败: {error}", target.display()))?;
    }

    cleanup_generated_bootstrap(&agent_home, seed)?;
    ensure_category_memory_scaffold(&agent_home)?;
    memory_wiki::refresh_memory_wiki(&agent_home)?;

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
    content.push_str("- New agents should be scaffolded from `agents/_template/`\n\n");

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

    let mut sections = Vec::new();
    sections.push(
        "记忆规则：只使用当前 agent 私有记忆；命中不到就直说；禁止引用其他 agents/<id>/ 内容。"
            .to_string(),
    );
    sections.push(
        "写回：短期写 WORKING.md；长期请整理 memory/categories/*.md、DECISIONS.md、PUBLIC_CONTEXT.md、wiki/*.md。MEMORY.md 只保留启动最小记忆；对话 ingest 默认写 raw、daily、SOURCE_INDEX/LOG、REVIEW_QUEUE 与 WORKING，不自动写 MEMORY.md。若需恢复每次对话追加分类，设置环境变量 NINECLAW_APPEND_CATEGORY_MEMORY_ON_INGEST=1。"
            .to_string(),
    );
    sections.push(
        "检索顺序：先判断是否真的需要查记忆；任务与未闭环优先看 WORKING.md 的 `Current Focus` / `OPEN_LOOPS`；规则优先看 DECISIONS.md 与 PITFALLS.md；用户风格优先看 MEMORY.md 与 USER_MODEL.md；人物关系优先看 RELATIONSHIP_MAP.md 与 relationships；没有证据就不要装记得。"
            .to_string(),
    );

    if let Some(wiki_snapshot) =
        memory_wiki::build_memory_wiki_snapshot(&agent_home, current_prompt)?
    {
        sections.push(wiki_snapshot);
    }
    if let Some(specialized_snapshot) =
        build_specialized_memory_snapshot(&root, agent_id, current_prompt)?
    {
        sections.push(specialized_snapshot);
    }
    if let Some(category_snapshot) =
        build_categorized_memory_snapshot(&root, agent_id, current_prompt)?
    {
        sections.push(category_snapshot);
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
    let agent_heartbeat_placeholder = fallback_template("HEARTBEAT.md");
    let agent_specific = read_meaningful_heartbeat_file(
        root.join("agents").join(agent_id).join("HEARTBEAT.md"),
        &agent_heartbeat_placeholder,
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

    let mut memory_wiki_files = memory_wiki::read_memory_wiki_files(&root, agent_id)?;
    files.append(&mut memory_wiki_files);

    let mut category_files = read_agent_category_memory_files(&root, agent_id)?;
    files.append(&mut category_files);

    let mut wiki_files = read_agent_wiki_files(&root, agent_id)?;
    files.append(&mut wiki_files);

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
    ensure_category_memory_scaffold(&agent_home)?;
    memory_wiki::ensure_memory_wiki_scaffold(&agent_home)?;

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
    let working_existing =
        fs::read_to_string(&working_path).unwrap_or_else(|_| fallback_template("WORKING.md"));
    let working_with_context = upsert_working_memory(
        &working_existing,
        user_id,
        &ingest_summary,
        &source_ref,
        &categories,
    );
    fs::write(
        &working_path,
        upsert_working_open_loops(
            &working_with_context,
            &build_open_loop_entries(
                user_message,
                assistant_message,
                &ingest_summary,
                &categories,
            ),
        ),
    )
    .map_err(|error| format!("写入 WORKING.md 失败: {error}"))?;

    append_pitfall_entries(
        &agent_home,
        &build_pitfall_entries(user_message, assistant_message, &source_ref),
    )?;

    // MEMORY.md 不再自动写入 — 仅保留人设和核心原则，由用户手动编辑
    if append_category_memory_on_ingest() {
        append_category_memory_entries(&agent_home, agent_id, &category_notes, &source_ref)?;
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

    append_review_queue_entries(
        &agent_home,
        &build_review_queue_items(
            user_message,
            assistant_message,
            &ingest_summary,
            &categories,
            &source_ref,
        ),
    )?;

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
    memory_wiki::ensure_memory_wiki_scaffold(&agent_home)?;
    memory_wiki::record_attachment_source(
        &agent_home,
        &timestamp,
        title,
        file_path,
        mime_type,
        note,
    )?;
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

pub fn persist_agent_outbound_artifact(
    agent_id: &str,
    user_id: &str,
    file_name: &str,
    source_path: &Path,
) -> Result<PathBuf, String> {
    if !source_path.exists() {
        return Err(format!("待发送文件不存在: {}", source_path.display()));
    }

    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;

    let outbox_dir = root
        .join("agents")
        .join(agent_id)
        .join("outbox")
        .join(current_date_label());
    fs::create_dir_all(&outbox_dir).map_err(|error| format!("创建智能体发件目录失败: {error}"))?;

    let safe_user = sanitize_workspace_segment(user_id, "user");
    let fallback_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("artifact.bin");
    let safe_name = sanitize_workspace_file_name(file_name, fallback_name);
    let target_path = outbox_dir.join(format!(
        "{}-{}-{}",
        current_timestamp_file_label(),
        safe_user,
        safe_name
    ));

    fs::copy(source_path, &target_path).map_err(|error| {
        format!(
            "归档待发送文件失败 {} -> {}: {error}",
            source_path.display(),
            target_path.display()
        )
    })?;

    Ok(target_path)
}

fn normalize_media_reference(reference: &str) -> String {
    let trimmed = reference.trim();
    if let Some(raw_path) = trimmed.strip_prefix("file://") {
        urlencoding::decode(raw_path)
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| raw_path.to_string())
    } else {
        trimmed.to_string()
    }
}

fn looks_like_file_name_reference(reference: &str) -> bool {
    let candidate = reference.trim();
    let Some(extension) = Path::new(candidate)
        .extension()
        .and_then(|value| value.to_str())
    else {
        return false;
    };
    if extension.is_empty() || extension.len() > 10 {
        return false;
    }
    candidate.chars().all(|char| {
        char.is_ascii_alphanumeric()
            || matches!(char, '.' | '_' | '-' | ' ' | '(' | ')' | '[' | ']')
    })
}

fn media_search_roots(agent_id: Option<&str>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(root) = resolve_workspace_root() {
        if let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) {
            let agent_home = root.join("agents").join(agent_id);
            roots.push(agent_home.join("outbox"));
            roots.push(agent_home.join("inbox"));
            roots.push(agent_home);
        }
    }
    if let Ok(current_dir) = std::env::current_dir() {
        roots.push(current_dir);
    }
    if let Some(home) = home_dir() {
        roots.push(home.join("Desktop"));
        roots.push(home.join("Downloads"));
        roots.push(home.join("Documents"));
    }
    roots
}

fn maybe_push_media_match(
    matches: &mut Vec<PathBuf>,
    candidate: PathBuf,
    target_name: &str,
    raw_reference: &str,
) {
    if !candidate.is_file() {
        return;
    }
    let file_name = candidate
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let normalized_path = candidate.to_string_lossy();
    if file_name.eq_ignore_ascii_case(target_name)
        || file_name
            .to_ascii_lowercase()
            .ends_with(&format!("-{}", target_name.to_ascii_lowercase()))
        || normalized_path.ends_with(raw_reference)
    {
        matches.push(candidate);
    }
}

fn collect_media_matches(
    root: &Path,
    target_name: &str,
    raw_reference: &str,
    matches: &mut Vec<PathBuf>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_media_matches(&path, target_name, raw_reference, matches);
        } else {
            maybe_push_media_match(matches, path, target_name, raw_reference);
        }
    }
}

pub fn resolve_agent_media_reference(agent_id: Option<&str>, reference: &str) -> Option<PathBuf> {
    let normalized = normalize_media_reference(reference);
    if normalized.is_empty() {
        return None;
    }

    let path = PathBuf::from(&normalized);
    if path.is_absolute() {
        return path.is_file().then_some(path);
    }

    let fileish = normalized.contains('/')
        || normalized.contains('\\')
        || looks_like_file_name_reference(&normalized);
    if !fileish {
        return None;
    }

    let roots = media_search_roots(agent_id);
    for root in &roots {
        let direct = root.join(&normalized);
        if direct.is_file() {
            return Some(direct);
        }
        let target_name = path.file_name().and_then(|value| value.to_str())?;
        if root.exists() {
            let mut matches = Vec::new();
            collect_media_matches(root, target_name, &normalized, &mut matches);
            if let Some(best) = matches.into_iter().max_by_key(|candidate| {
                candidate
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
            }) {
                return Some(best);
            }
        }
    }
    None
}

pub fn register_agent_outbound_artifact_source(
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
    let artifact_message = match mime_type.filter(|value| !value.trim().is_empty()) {
        Some(mime) => format!(
            "生成并发送产物：{}（mime={}，path=`{}`）",
            title,
            mime,
            file_path.display()
        ),
        None => format!(
            "生成并发送产物：{}（path=`{}`）",
            title,
            file_path.display()
        ),
    };
    memory_wiki::ensure_memory_wiki_scaffold(&agent_home)?;
    memory_wiki::record_attachment_source(
        &agent_home,
        &timestamp,
        title,
        file_path,
        mime_type,
        note,
    )?;
    fs::write(
        &daily_log_path,
        append_daily_log_entry(
            &existing,
            "outbound_artifact",
            &timestamp,
            &artifact_message,
            &file_path.display().to_string(),
            &[],
        ),
    )
    .map_err(|error| format!("写入出站产物 daily log 失败: {error}"))?;
    memory_wiki::refresh_memory_wiki(&agent_home)?;
    Ok(())
}

fn ensure_root_scaffold(root: &Path) -> Result<(), String> {
    migrate_legacy_template_dir_if_needed(root)?;
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

    for relative_path in AGENT_TEMPLATE_FILES {
        let template_path = root.join(TEMPLATE_DIR).join(relative_path);
        if template_path.exists() {
            continue;
        }

        if let Some(parent) = template_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("创建 agent 模板子目录失败 {}: {error}", parent.display())
            })?;
        }

        fs::write(&template_path, fallback_template(relative_path)).map_err(|error| {
            format!("写入 agent 模板 {} 失败: {error}", template_path.display())
        })?;
    }

    Ok(())
}

fn build_legacy_workspace_memory_snapshot(root: &Path, agent_id: &str) -> Result<String, String> {
    let agent_home = root.join("agents").join(agent_id);
    let mut sections = Vec::new();

    let memory_content = fs::read_to_string(agent_home.join("MEMORY.md")).unwrap_or_default();
    let mut memory_bullets: Vec<String> = Vec::new();
    for marker in [
        "## Core Memory",
        "## 核心原则",
        "## Core Principles",
        "## Stable Preferences",
        "## 用户偏好",
        "## Relationship Anchors",
        "## Current Theme Anchors",
    ] {
        for item in extract_section_bullets(&memory_content, marker) {
            if memory_bullets
                .iter()
                .any(|existing| existing.trim().eq_ignore_ascii_case(item.trim()))
            {
                continue;
            }
            memory_bullets.push(item);
        }
    }

    for item in extract_section_bullets(&memory_content, "## 用户偏好") {
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
    let focus_bullets = extract_section_bullets(&working_content, "## Current Focus")
        .into_iter()
        .filter(|item| !item.contains("No active task yet"))
        .take(2)
        .map(|item| format!("- {}", trim_to_char_limit(&item, 90)))
        .collect::<Vec<_>>();
    if !focus_bullets.is_empty() {
        sections.push(format!(
            "WORKING 当前 focus：\n{}",
            focus_bullets.join("\n")
        ));
    }
    let open_loop_bullets = extract_section_bullets(&working_content, "## OPEN_LOOPS")
        .into_iter()
        .filter(|item| !item.contains("No open loop yet"))
        .take(3)
        .map(|item| format!("- {}", trim_to_char_limit(&item, 90)))
        .collect::<Vec<_>>();
    if !open_loop_bullets.is_empty() {
        sections.push(format!(
            "WORKING 未闭环事项：\n{}",
            open_loop_bullets.join("\n")
        ));
    }
    let working_bullets = extract_section_bullets(&working_content, "## IM Latest Context")
        .into_iter()
        .filter(|item| !item.contains("Last user:"))
        .take(3)
        .map(|item| format!("- {}", trim_to_char_limit(&item, 90)))
        .collect::<Vec<_>>();
    if !working_bullets.is_empty() {
        sections.push(format!(
            "WORKING 当前上下文：\n{}",
            working_bullets.join("\n")
        ));
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
        Ok(format!("核心记忆摘录：\n{}", sections.join("\n\n")))
    }
}

fn build_specialized_memory_snapshot(
    root: &Path,
    agent_id: &str,
    current_prompt: Option<&str>,
) -> Result<Option<String>, String> {
    let prompt = normalize_memory_match_text(current_prompt.unwrap_or_default());
    if prompt.is_empty() {
        return Ok(None);
    }

    let agent_home = root.join("agents").join(agent_id);
    let mut sections = Vec::new();

    for (file_name, label, keywords) in [
        (
            "USER_MODEL.md",
            "USER_MODEL",
            &[
                "偏好",
                "风格",
                "喜欢",
                "讨厌",
                "习惯",
                "怎么回",
                "称呼",
                "陪伴",
                "call me",
                "style",
                "preference",
            ][..],
        ),
        (
            "PITFALLS.md",
            "PITFALLS",
            &[
                "坑",
                "翻车",
                "别再",
                "时间线",
                "误判",
                "别问",
                "不要废话",
                "pitfall",
                "mistake",
                "error",
            ][..],
        ),
        (
            "RELATIONSHIP_MAP.md",
            "RELATIONSHIP_MAP",
            &[
                "谁",
                "关系",
                "联系人",
                "团队",
                "老板",
                "客户",
                "合作方",
                "负责人",
                "owner",
                "stakeholder",
                "contact",
            ][..],
        ),
    ] {
        if !contains_any_keyword(&prompt, keywords) {
            continue;
        }
        let Some(content) = read_meaningful_agent_note(&agent_home, file_name, 260)? else {
            continue;
        };
        sections.push(format!("{}:\n{}", label, content));
    }

    if sections.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!("专项记忆路由：\n{}", sections.join("\n\n"))))
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

        let content = safe_read_trimmed(path, if category.key == "general" { 380 } else { 260 })?;
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
        "# INDEX.md - Memory Categories\n\nUse these shards for curated long-term memory. Do not dump raw conversation logs here.\n\n## Rules\n\n- Prefer the most specific shard.\n- `general.md` is fallback memory, not a junk drawer.\n- `inferences.md` stores tentative conclusions and must not be treated as confirmed fact.\n- `commitments.md` is a ledger of follow-ups, not a brainstorm list.\n\n## Categories\n\n",
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
        "# {}.md\n\n## Purpose\n\n{}\n\n## Schema\n\n{}\n\n## Entries\n\nNo curated entries yet.\n",
        category.key,
        category.description,
        category_schema_block(category)
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
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
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
            read_workspace_file(
                "agent",
                "categoryMemory",
                &file_name,
                relative_path,
                path,
                false,
            )
        })
        .collect())
}

fn read_agent_wiki_files(root: &Path, agent_id: &str) -> Result<Vec<AgentWorkspaceFile>, String> {
    let wiki_dir = root.join("agents").join(agent_id).join("wiki");
    if !wiki_dir.exists() {
        return Ok(Vec::new());
    }

    let mut stack = vec![wiki_dir];
    let mut entries = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|error| format!("读取 wiki 目录失败: {error}"))?
        {
            let entry = entry.map_err(|error| format!("读取 wiki 条目失败: {error}"))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let relative_suffix = path
                .strip_prefix(root.join("agents").join(agent_id))
                .map_err(|error| format!("计算 wiki 相对路径失败: {error}"))?;
            entries.push((relative_suffix.display().to_string(), path));
        }
    }

    entries.sort_by(|left, right| left.0.cmp(&right.0));

    Ok(entries
        .into_iter()
        .map(|(relative_suffix, path)| {
            let relative_path = PathBuf::from("agents")
                .join(agent_id)
                .join(&relative_suffix);
            let file_name = path
                .file_name()
                .and_then(|item| item.to_str())
                .unwrap_or("wiki.md")
                .to_string();
            read_workspace_file("agent", "wiki", &file_name, relative_path, path, false)
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
    agent_id: &str,
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
            append_category_memory_file(&existing, category, agent_id, note, source_ref),
        )
        .map_err(|error| format!("写入分类记忆 {} 失败: {error}", path.display()))?;
    }

    Ok(())
}

fn append_category_memory_file(
    existing: &str,
    category: MemoryCategoryDefinition,
    agent_id: &str,
    note: &str,
    source_ref: &str,
) -> String {
    let mut next = existing.trim_end().to_string();
    if !next.contains("## Entries") {
        next.push_str("\n\n## Entries\n");
    }

    if next.contains(note) {
        return format!("{next}\n");
    }

    if next.contains("No curated entries yet.") {
        next = next.replace("No curated entries yet.", "");
    }

    let structured_entry = build_structured_category_entry(category, agent_id, note, source_ref);
    let _ = writeln!(next, "\n{}", structured_entry.trim_end());
    next.push('\n');
    next
}

fn category_schema_block(category: MemoryCategoryDefinition) -> String {
    match category.key {
        "commitments" => "- item: concrete promise or follow-up\n  status: open|blocked|done|dropped\n  owner: agent or person responsible\n  created: YYYY-MM-DD\n  next_check: YYYY-MM-DD\n  blocker: none or dependency\n  source: file#anchor or memory/raw/... path\n  updated: YYYY-MM-DD".to_string(),
        "inferences" => "- item: inferred tendency or assumption\n  type: inference\n  status: tentative|confirmed|rejected\n  confidence: high|medium|low\n  source: file#anchor or memory/raw/... path\n  updated: YYYY-MM-DD\n  review_at: YYYY-MM-DD".to_string(),
        _ => format!(
            "- item: concise memory statement\n  type: {}\n  status: {}\n  confidence: {}\n  source: file#anchor or memory/raw/... path\n  updated: YYYY-MM-DD",
            category_default_type(category),
            category_default_status(category),
            category_default_confidence(category),
        ),
    }
}

fn category_default_type(category: MemoryCategoryDefinition) -> &'static str {
    match category.key {
        "user_profile" => "profile_fact",
        "preferences" => "preference",
        "projects" => "project_context",
        "commitments" => "commitment",
        "decisions" => "decision",
        "relationships" => "relationship",
        "pitfalls" => "pitfall",
        "inferences" => "inference",
        _ => "general_memory",
    }
}

fn category_default_status(category: MemoryCategoryDefinition) -> &'static str {
    match category.key {
        "commitments" => "open",
        "inferences" => "tentative",
        _ => "active",
    }
}

fn category_default_confidence(category: MemoryCategoryDefinition) -> &'static str {
    match category.key {
        "projects" | "relationships" | "pitfalls" | "general" | "inferences" => "medium",
        _ => "high",
    }
}

fn build_structured_category_entry(
    category: MemoryCategoryDefinition,
    agent_id: &str,
    note: &str,
    source_ref: &str,
) -> String {
    let item = truncate_for_memory(note.trim(), 220);
    let today = current_date_label();
    match category.key {
        "commitments" => format!(
            "- item: {}\n  status: {}\n  owner: {}\n  created: {}\n  next_check: {}\n  blocker: {}\n  source: {}\n  updated: {}\n",
            item,
            category_default_status(category),
            agent_id,
            today,
            days_from_now_label(3),
            derive_commitment_blocker(note),
            source_ref,
            today,
        ),
        "inferences" => format!(
            "- item: {}\n  type: {}\n  status: {}\n  confidence: {}\n  source: {}\n  updated: {}\n  review_at: {}\n",
            item,
            category_default_type(category),
            category_default_status(category),
            category_default_confidence(category),
            source_ref,
            today,
            days_from_now_label(7),
        ),
        _ => format!(
            "- item: {}\n  type: {}\n  status: {}\n  confidence: {}\n  source: {}\n  updated: {}\n",
            item,
            category_default_type(category),
            category_default_status(category),
            category_default_confidence(category),
            source_ref,
            today,
        ),
    }
}

fn derive_commitment_blocker(note: &str) -> &'static str {
    if contains_any_keyword(
        &normalize_memory_match_text(note),
        &[
            "等待",
            "依赖",
            "blocked",
            "blocker",
            "需要",
            "待提供",
            "外部",
        ],
    ) {
        "pending external dependency"
    } else {
        "none"
    }
}

fn days_from_now_label(days: i64) -> String {
    use chrono::{Duration, Local};
    (Local::now().date_naive() + Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

#[derive(Clone)]
struct ReviewQueueItem {
    item: String,
    status: &'static str,
    review_at: String,
    reason: String,
    source: String,
    updated: String,
}

fn build_review_queue_items(
    user_message: &str,
    assistant_message: &str,
    summary: &str,
    categories: &[MemoryCategoryDefinition],
    source_ref: &str,
) -> Vec<ReviewQueueItem> {
    let today = current_date_label();
    let combined = normalize_memory_match_text(&format!(
        "{}\n{}\n{}",
        user_message, assistant_message, summary
    ));
    let item = truncate_for_memory(summary.trim(), 200);
    let mut items = Vec::new();

    if categories
        .iter()
        .any(|category| category.key == "commitments")
    {
        items.push(ReviewQueueItem {
            item: item.clone(),
            status: "pending",
            review_at: days_from_now_label(3),
            reason: "未完成承诺默认需要闭环复查。".to_string(),
            source: source_ref.to_string(),
            updated: today.clone(),
        });
    }

    if categories.iter().any(|category| category.key == "projects")
        && contains_any_keyword(
            &combined,
            &[
                "上线", "发布", "今天", "明天", "本周", "下周", "deadline", "ddl", "current",
            ],
        )
    {
        items.push(ReviewQueueItem {
            item: item.clone(),
            status: "pending",
            review_at: days_from_now_label(7),
            reason: "项目状态带有明显时效性，默认进入复查队列。".to_string(),
            source: source_ref.to_string(),
            updated: today.clone(),
        });
    }

    if contains_any_keyword(
        &combined,
        &[
            "等待",
            "依赖",
            "blocked",
            "blocker",
            "webhook",
            "待提供",
            "需要安装",
            "缺少",
        ],
    ) {
        items.push(ReviewQueueItem {
            item,
            status: "pending",
            review_at: days_from_now_label(2),
            reason: "存在外部依赖或阻塞项，需要短周期复查。".to_string(),
            source: source_ref.to_string(),
            updated: today,
        });
    }

    dedupe_review_queue_items(items)
}

fn dedupe_review_queue_items(items: Vec<ReviewQueueItem>) -> Vec<ReviewQueueItem> {
    let mut seen = Vec::new();
    let mut deduped = Vec::new();
    for item in items {
        let key = format!(
            "{}|{}",
            item.item.to_lowercase(),
            item.reason.to_lowercase()
        );
        if seen.iter().any(|existing| existing == &key) {
            continue;
        }
        seen.push(key);
        deduped.push(item);
    }
    deduped
}

fn append_review_queue_entries(agent_home: &Path, items: &[ReviewQueueItem]) -> Result<(), String> {
    if items.is_empty() {
        return Ok(());
    }

    let path = agent_home.join("memory").join("REVIEW_QUEUE.md");
    let existing =
        fs::read_to_string(&path).unwrap_or_else(|_| fallback_template("memory/REVIEW_QUEUE.md"));
    fs::write(&path, append_review_queue_file(&existing, items))
        .map_err(|error| format!("写入 REVIEW_QUEUE.md 失败: {error}"))
}

fn append_review_queue_file(existing: &str, items: &[ReviewQueueItem]) -> String {
    let mut next = existing.trim_end().to_string();
    if !next.contains("## Entries") {
        next.push_str("\n\n## Entries\n");
    }
    if next.contains("No scheduled review yet.") {
        next = next.replace("No scheduled review yet.", "");
    }

    for item in items {
        if next.contains(&item.item) && next.contains(&item.source) {
            continue;
        }
        let _ = writeln!(next, "\n- item: {}", item.item);
        let _ = writeln!(next, "  status: {}", item.status);
        let _ = writeln!(next, "  review_at: {}", item.review_at);
        let _ = writeln!(next, "  reason: {}", item.reason);
        let _ = writeln!(next, "  source: {}", item.source);
        let _ = writeln!(next, "  updated: {}", item.updated);
    }

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

fn read_meaningful_agent_note(
    agent_home: &Path,
    file_name: &str,
    limit: usize,
) -> Result<Option<String>, String> {
    let path = agent_home.join(file_name);
    let content = safe_read_trimmed(path, limit)?;
    if content.is_empty() {
        return Ok(None);
    }

    let placeholder = fallback_template(file_name);
    if normalize_heartbeat_instruction_text(&content)
        == normalize_heartbeat_instruction_text(&trim_to_char_limit(&placeholder, limit))
    {
        return Ok(None);
    }

    Ok(Some(content))
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
    fallback: String,
    seed: AgentWorkspaceSeed<'_>,
) -> Result<String, String> {
    let template_path = root.join(TEMPLATE_DIR).join(file_name);
    let legacy_template_path = root.join(LEGACY_TEMPLATE_DIR).join(file_name);
    let template = if template_path.exists() {
        fs::read_to_string(&template_path)
            .map_err(|error| format!("读取模板 {} 失败: {error}", template_path.display()))?
    } else if legacy_template_path.exists() {
        fs::read_to_string(&legacy_template_path).map_err(|error| {
            format!(
                "读取旧模板 {} 失败: {error}",
                legacy_template_path.display()
            )
        })?
    } else {
        fallback
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

    if normalized.len() >= 4
        && normalized[0] == "agents"
        && normalized[1] == agent_id
        && normalized[2] == "wiki"
        && normalized
            .last()
            .is_some_and(|segment| segment.ends_with(".md"))
    {
        return Ok(normalized
            .iter()
            .skip(2)
            .fold(root.join("agents").join(agent_id), |path, segment| {
                path.join(segment)
            }));
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

fn migrate_legacy_template_dir_if_needed(root: &Path) -> Result<(), String> {
    let template_dir = root.join(TEMPLATE_DIR);
    if template_dir.exists() {
        return Ok(());
    }

    let legacy_template_dir = root.join(LEGACY_TEMPLATE_DIR);
    if !legacy_template_dir.exists() {
        return Ok(());
    }

    if let Some(parent) = template_dir.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建新模板目录父级失败: {error}"))?;
    }

    fs::rename(&legacy_template_dir, &template_dir)
        .or_else(|_| {
            copy_dir_all(&legacy_template_dir, &template_dir)
                .and_then(|_| fs::remove_dir_all(&legacy_template_dir))
        })
        .map_err(|error| {
            format!(
                "迁移旧模板目录 {} 到 {} 失败: {error}",
                legacy_template_dir.display(),
                template_dir.display()
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

fn cleanup_generated_bootstrap(
    agent_home: &Path,
    seed: AgentWorkspaceSeed<'_>,
) -> Result<(), String> {
    let bootstrap_path = agent_home.join("BOOTSTRAP.md");
    if !bootstrap_path.exists() {
        return Ok(());
    }

    let existing = fs::read_to_string(&bootstrap_path)
        .map_err(|error| format!("读取旧 BOOTSTRAP.md 失败: {error}"))?;
    let legacy = apply_placeholders(legacy_bootstrap_template().to_string(), seed);
    if normalize_heartbeat_instruction_text(&existing)
        == normalize_heartbeat_instruction_text(&legacy)
    {
        fs::remove_file(&bootstrap_path)
            .map_err(|error| format!("删除旧 BOOTSTRAP.md 失败: {error}"))?;
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
            "user_profile" if !user_compact.is_empty() => user_compact.clone(),
            "preferences" if !user_compact.is_empty() => user_compact.clone(),
            "projects" if !user_compact.is_empty() => user_compact.clone(),
            "commitments" if !user_compact.is_empty() => user_compact.clone(),
            "decisions" if !user_compact.is_empty() => user_compact.clone(),
            "relationships" if !user_compact.is_empty() => user_compact.clone(),
            "pitfalls" if !user_compact.is_empty() => user_compact.clone(),
            "inferences" if !summary_compact.is_empty() => summary_compact.clone(),
            "general" if !summary_compact.is_empty() => summary_compact.clone(),
            _ if !summary_compact.is_empty() => summary_compact.clone(),
            _ => continue,
        };
        notes.push((category.key.to_string(), note));
    }

    if notes.is_empty() && !assistant_compact.is_empty() {
        notes.push(("general".to_string(), assistant_compact));
    }

    dedupe_memory_notes(notes)
}

fn dedupe_memory_notes(notes: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut seen = Vec::new();
    let mut deduped = Vec::new();
    for (category, note) in notes {
        let normalized = format!("{}::{}", category, note.trim().to_lowercase());
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
    notes
        .iter()
        .filter(|(category, _)| {
            matches!(
                category.as_str(),
                "user_profile" | "preferences" | "decisions"
            )
        })
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
    let category_titles =
        format_category_titles(categories).unwrap_or_else(|| "GENERAL_MEMORY".to_string());
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

fn build_open_loop_entries(
    user_message: &str,
    assistant_message: &str,
    summary: &str,
    categories: &[MemoryCategoryDefinition],
) -> Vec<String> {
    let combined = normalize_memory_match_text(&format!(
        "{}\n{}\n{}",
        user_message, assistant_message, summary
    ));
    if !categories
        .iter()
        .any(|category| category.key == "commitments")
        && !contains_any_keyword(
            &combined,
            &[
                "等待", "待办", "跟进", "之后", "下周", "明天", "webhook", "材料", "依赖",
                "blocked", "blocker",
            ],
        )
    {
        return Vec::new();
    }

    let prefix = if contains_any_keyword(
        &combined,
        &[
            "等你",
            "等你给",
            "等你提供",
            "待你",
            "你补",
            "你提供",
            "你给我",
        ],
    ) {
        "waiting_user"
    } else if contains_any_keyword(
        &combined,
        &[
            "等待",
            "webhook",
            "依赖",
            "blocked",
            "blocker",
            "待提供",
            "缺少",
        ],
    ) {
        "blocked"
    } else {
        "follow_up"
    };

    vec![format!(
        "{}: {}",
        prefix,
        truncate_for_memory(summary.trim(), 120)
    )]
}

fn upsert_working_open_loops(existing: &str, open_loops: &[String]) -> String {
    let marker = "## OPEN_LOOPS";
    let mut entries = extract_section_bullets(existing, marker)
        .into_iter()
        .filter(|item| !item.contains("No open loop yet"))
        .collect::<Vec<_>>();

    for open_loop in open_loops {
        if entries
            .iter()
            .any(|existing| existing.trim().eq_ignore_ascii_case(open_loop.trim()))
        {
            continue;
        }
        entries.push(open_loop.trim().to_string());
    }

    if entries.len() > 6 {
        entries = entries.split_off(entries.len() - 6);
    }

    let cleaned = strip_markdown_section(existing, marker);
    let mut next = cleaned.trim_end().to_string();
    next.push_str(&format!("\n\n{marker}\n"));

    if entries.is_empty() {
        next.push_str("\n- No open loop yet.\n");
    } else {
        for entry in entries {
            next.push_str(&format!("\n- {}\n", entry));
        }
    }
    next.push('\n');
    next
}

#[derive(Clone)]
struct PitfallEntry {
    item: String,
    risk: &'static str,
    action: String,
    source: String,
    updated: String,
}

fn build_pitfall_entries(
    user_message: &str,
    assistant_message: &str,
    source_ref: &str,
) -> Vec<PitfallEntry> {
    let normalized = normalize_memory_match_text(user_message);
    if !contains_any_keyword(
        &normalized,
        &[
            "别",
            "不要",
            "别再",
            "记错",
            "误判",
            "翻车",
            "时间线",
            "别问",
            "不要废话",
            "不是这个意思",
        ],
    ) {
        return Vec::new();
    }

    let item = truncate_for_memory(user_message.trim(), 160);
    let action = if contains_any_keyword(&normalized, &["时间线", "今天", "明天", "下周"])
    {
        "先核时间线与相对日期，再回答。".to_string()
    } else if contains_any_keyword(&normalized, &["别问", "上下文", "明显能推出"]) {
        "先用已有上下文推断，避免追问显然可推出的问题。".to_string()
    } else if contains_any_keyword(&normalized, &["不要废话", "废话", "正确但没用"]) {
        "压缩废话，优先给出直接可执行的结论。".to_string()
    } else {
        truncate_for_memory(assistant_message.trim(), 120)
    };

    vec![PitfallEntry {
        item,
        risk: "high",
        action,
        source: source_ref.to_string(),
        updated: current_date_label(),
    }]
}

fn append_pitfall_entries(agent_home: &Path, entries: &[PitfallEntry]) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }

    let path = agent_home.join("PITFALLS.md");
    let existing = fs::read_to_string(&path).unwrap_or_else(|_| fallback_template("PITFALLS.md"));
    fs::write(&path, append_pitfall_file(&existing, entries))
        .map_err(|error| format!("写入 PITFALLS.md 失败: {error}"))
}

fn append_pitfall_file(existing: &str, entries: &[PitfallEntry]) -> String {
    let marker = "## Active Pitfalls";
    let mut next = existing.trim_end().to_string();
    if !next.contains(marker) {
        next.push_str(&format!("\n\n{}\n", marker));
    }
    if next.contains("No active pitfall recorded yet.") {
        next = next.replace("No active pitfall recorded yet.", "");
    }

    for entry in entries {
        if next.contains(&entry.item) {
            continue;
        }
        let _ = writeln!(next, "\n- item: {}", entry.item);
        let _ = writeln!(next, "  risk: {}", entry.risk);
        let _ = writeln!(next, "  action: {}", entry.action);
        let _ = writeln!(next, "  source: {}", entry.source);
        let _ = writeln!(next, "  updated: {}", entry.updated);
    }

    next.push('\n');
    next
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

fn fallback_template(file_name: &str) -> String {
    match file_name {
        "IDENTITY.md" => "# IDENTITY.md\n\n- **Agent ID:** {{AGENT_ID}}\n- **Name:** {{AGENT_NAME}}\n- **Creature:** 智能体\n- **Vibe:** 高效、直接、少废话\n- **Accent Color:** {{AGENT_ACCENT_COLOR}}\n\n## Identity Notes\n\n- Summary: {{AGENT_SUMMARY}}\n".to_string(),
        "ROLE.md" => "# ROLE.md\n\n## Mission\n\n{{AGENT_DESCRIPTION}}\n\n## Ownership\n\n- Define ownership here.\n\n## Do Not\n\n- Leak private memory.\n- Confuse tentative memory with confirmed facts.\n- Act externally without confirmation.\n".to_string(),
        "MEMORY.md" => "# MEMORY.md\n\n这是 `{{AGENT_NAME}}` 的最小启动记忆，只保留高频、稳定、开局就该知道的内容。\n\n## Identity Anchor\n\n- Agent name: {{AGENT_NAME}}\n- Summary: {{AGENT_SUMMARY}}\n\n## Core Principles\n\n- 保持直接、准确、可执行。\n- 推断和事实必须分层；不确定就直说。\n- 承诺要进入账本并持续跟进。\n\n## Stable Preferences\n\n- 在这里放高频、稳定、明确确认过的偏好。\n\n## Relationship Anchors\n\n- 在这里放最稳定、最高频的人物关系锚点。\n\n## Current Theme Anchors\n\n- 只放最近一段时间持续重要的主题，不放流水账。\n\n## Routing\n\n- 当前任务与未闭环：`WORKING.md`\n- 稳定规则与约定：`DECISIONS.md`\n- 用户长期模型：`USER_MODEL.md`\n- 关系图：`RELATIONSHIP_MAP.md`\n- 高风险坑点：`PITFALLS.md`\n- 详细长期记忆：`memory/categories/*.md`\n- 承诺与复查：`memory/categories/commitments.md`、`memory/REVIEW_QUEUE.md`\n- 外部知识：`wiki/INDEX.md`\n\n> 此文件不会在对话过程中被自动修改。如需调整，请手动编辑。\n".to_string(),
        "USER_MODEL.md" => "# USER_MODEL.md\n\n## Purpose\n\nCapture the user's long-term interaction model, not daily chatter.\n\n## Stable Preferences\n\n- No confirmed model entry yet.\n\n## Interaction Style\n\n- No confirmed style rule yet.\n\n## Implicit Signals\n\n- Record repeated hidden intent patterns here.\n\n## Test Patterns\n\n- Record recurring ways the user tests responsiveness or understanding.\n\n## Collaboration Modes\n\n- Note how the user wants the agent to behave in different contexts.\n".to_string(),
        "RELATIONSHIP_MAP.md" => "# RELATIONSHIP_MAP.md\n\n## Purpose\n\nTrack the important people around the user and why they matter.\n\n## Key People\n\n- No key person recorded yet.\n\n## Teams And Groups\n\n- No team mapping recorded yet.\n\n## Open Questions\n\n- Record unresolved identities or relationship ambiguities here.\n".to_string(),
        "PITFALLS.md" => "# PITFALLS.md\n\n## Purpose\n\nTrack recurring failure modes, explicit user corrections, and things this agent must stop doing.\n\n## Active Pitfalls\n\n- No active pitfall recorded yet.\n\n## Rules\n\n- Promote only concrete, reusable pitfalls.\n- Prefer actionable phrasing over vague blame.\n- Retire items when the behavior is truly fixed.\n".to_string(),
        "TOOLS.md" => "# TOOLS.md\n\n## Tool Bias\n\n- Preferred tools:\n- Avoid when possible:\n".to_string(),
        "HEARTBEAT.md" => "# HEARTBEAT.md\n\n# Keep empty if this agent owns no periodic checks.\n".to_string(),
        "WORKING.md" => "# WORKING.md\n\n## Current Focus\n\n- No active task yet.\n\n## OPEN_LOOPS\n\n- No open loop yet.\n\n## Open Threads\n\n- No open thread yet.\n\n## IM Latest Context\n\n- No IM context ingested yet.\n".to_string(),
        "DECISIONS.md" => "# DECISIONS.md\n\n## Decision Log\n\n- No decisions logged yet.\n\n## Sync Rules\n\n- Stable rules should be mirrored into `MEMORY.md` or category memory when relevant.\n".to_string(),
        "PUBLIC_CONTEXT.md" => "# PUBLIC_CONTEXT.md\n\n## Safe Identity\n\n- Agent: {{AGENT_NAME}}\n- Summary: {{AGENT_SUMMARY}}\n\n## External Notes\n\n- Put channel-safe context here.\n".to_string(),
        "memory/INDEX.md" => "# INDEX.md\n\nThis file is the entry point for the agent memory system.\n".to_string(),
        "memory/REVIEW_QUEUE.md" => "# REVIEW_QUEUE.md\n\n## Purpose\n\nTrack time-sensitive memory that needs re-checking so stale state does not masquerade as truth.\n\n## Rules\n\n- Add unresolved commitments, blockers, tentative inferences, and medium-confidence changing facts.\n- Mark resolved items as done or dropped instead of silently forgetting them.\n\n## Schema\n\n- item: what needs review\n  status: pending|done|dropped\n  review_at: YYYY-MM-DD\n  reason: why this can go stale\n  source: file#anchor or memory/raw/... path\n  updated: YYYY-MM-DD\n\n## Entries\n\nNo scheduled review yet.\n".to_string(),
        "memory/LINT.md" => "# LINT.md\n\n## Health Checklist\n\n- Keep `memory/INDEX.md` aligned with the actual directory layout.\n- Keep commitments structured and reviewable.\n- Keep inferences separate from confirmed facts.\n- Keep raw evidence, daily summaries, curated memory, and wiki content in their own layers.\n\n## Last Pass\n\n- No lint pass recorded yet.\n".to_string(),
        "memory/SOURCE_INDEX.md" => "# SOURCE_INDEX.md\n\nThis file registers immutable raw sources and uploaded artifacts. The LLM should never rewrite the underlying source files; it should only update the curated memory around them.\n\nEach entry includes an `Index:` line (`type=… ts=… cats=…`) for quick filtering.\n\n## Entries\n".to_string(),
        "memory/LOG.md" => "# LOG.md\n\nAppend-only operational log for ingest, source registration, and memory maintenance.\n\n## Entries\n".to_string(),
        "memory/categories/INDEX.md" => build_category_index_content(),
        "memory/categories/general.md" => build_category_file_template(memory_category_definition("general").expect("general category")),
        "memory/categories/user_profile.md" => build_category_file_template(memory_category_definition("user_profile").expect("user_profile category")),
        "memory/categories/preferences.md" => build_category_file_template(memory_category_definition("preferences").expect("preferences category")),
        "memory/categories/projects.md" => build_category_file_template(memory_category_definition("projects").expect("projects category")),
        "memory/categories/commitments.md" => build_category_file_template(memory_category_definition("commitments").expect("commitments category")),
        "memory/categories/decisions.md" => build_category_file_template(memory_category_definition("decisions").expect("decisions category")),
        "memory/categories/relationships.md" => build_category_file_template(memory_category_definition("relationships").expect("relationships category")),
        "memory/categories/pitfalls.md" => build_category_file_template(memory_category_definition("pitfalls").expect("pitfalls category")),
        "memory/categories/inferences.md" => build_category_file_template(memory_category_definition("inferences").expect("inferences category")),
        "wiki/INDEX.md" => "# INDEX.md\n\nThis wiki stores external knowledge, research notes, and reusable methodology. Do not store user identity, promises, or live project status here.\n\n## Boundaries\n\n- Put `who we are / what we promised / what the user prefers` into memory, not wiki.\n- Put external articles, GitHub project notes, technical summaries, and methods into wiki.\n\n## Routes\n\n- New research note: create a page under `wiki/` and link it here.\n- Memory question: go back to `memory/INDEX.md`.\n\n## Pages\n\n- No wiki pages yet.\n".to_string(),
        _ => String::new(),
    }
}

fn root_fallback_template(file_name: &str) -> &'static str {
    match file_name {
        "AGENTS.md" => {
            "# AGENTS.md - NineClaw Agent Workspace Protocol\n\nRoot files are system policy notes only. `agents/<agent-id>/` is private memory.\n\n## Memory Rules\n\n- New agents scaffold from `agents/_template/`.\n- `MEMORY.md` is startup memory only.\n- `memory/raw/` is evidence, `memory/YYYY-MM-DD.md` is daily digest, `memory/categories/*.md` is curated memory, `wiki/` is external knowledge.\n"
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

fn legacy_bootstrap_template() -> &'static str {
    "# BOOTSTRAP.md - New Agent First Run\n\n- Clarify the agent name, mission, and boundaries\n- Update IDENTITY.md, ROLE.md, MEMORY.md, TOOLS.md, and PUBLIC_CONTEXT.md\n- Write one note into today's daily log\n- Delete this file when done\n"
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

    fn read_string(path: PathBuf) -> String {
        fs::read_to_string(path).expect("read text file")
    }

    #[derive(Debug)]
    struct ScenarioScore {
        name: &'static str,
        score: u32,
        details: Vec<(&'static str, bool)>,
    }

    fn score_scenario(name: &'static str, details: Vec<(&'static str, bool)>) -> ScenarioScore {
        let passed = details.iter().filter(|(_, ok)| *ok).count() as u32;
        let total = details.len().max(1) as u32;
        ScenarioScore {
            name,
            score: passed * 100 / total,
            details,
        }
    }

    fn print_score(score: &ScenarioScore) {
        println!("SCENARIO {} => {}/100", score.name, score.score);
        for (label, ok) in &score.details {
            println!("  [{}] {}", if *ok { "PASS" } else { "FAIL" }, label);
        }
    }

    #[test]
    fn scaffold_agent_home_from_template_memory_system() {
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
        assert!(home.join("USER_MODEL.md").exists());
        assert!(home.join("RELATIONSHIP_MAP.md").exists());
        assert!(home.join("PITFALLS.md").exists());
        assert!(!home.join("BOOTSTRAP.md").exists());
        assert!(home.join("memory").join("INDEX.md").exists());
        assert!(home.join("memory").join("REVIEW_QUEUE.md").exists());
        assert!(home
            .join("memory")
            .join("categories")
            .join("inferences.md")
            .exists());
        assert!(home
            .join("memory")
            .join("categories")
            .join("pitfalls.md")
            .exists());
        assert!(home.join("wiki").join("INDEX.md").exists());
        assert!(!home.join("memory").join("WIKI_INDEX.md").exists());
        assert!(root
            .join("agents")
            .join("_template")
            .join("memory")
            .join("INDEX.md")
            .exists());
        assert!(root
            .join("agents")
            .join("_template")
            .join("wiki")
            .join("INDEX.md")
            .exists());

        let identity = fs::read_to_string(home.join("IDENTITY.md")).expect("read identity");
        assert!(identity.contains("测试代理"));
        assert!(identity.contains("#112233"));

        let prompt = build_workspace_system_prompt("test-agent").expect("workspace prompt");
        assert!(prompt.contains("memory/INDEX.md"));
        assert!(prompt.contains("wiki/INDEX.md"));
        assert!(!prompt.contains("BOOTSTRAP.md"));

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
        assert!(bundle
            .files
            .iter()
            .any(|file| file.relative_path == "agents/test-agent/USER_MODEL.md"));
        assert!(bundle
            .files
            .iter()
            .any(|file| file.relative_path == "agents/test-agent/memory/INDEX.md"));
        assert!(
            bundle
                .files
                .iter()
                .any(|file| file.relative_path
                    == "agents/test-agent/memory/categories/commitments.md")
        );
        assert!(bundle
            .files
            .iter()
            .any(|file| file.relative_path == "agents/test-agent/wiki/INDEX.md"));

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

        write_agent_workspace_file(
            "writer",
            "agents/writer/wiki/how-it-works.md",
            "# How It Works\n\nwiki page\n",
        )
        .expect("write wiki page");
        let wiki_page = fs::read_to_string(
            root.join("agents")
                .join("writer")
                .join("wiki")
                .join("how-it-works.md"),
        )
        .expect("read saved wiki page");
        assert!(wiki_page.contains("wiki page"));

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

        let foreign_wiki_error =
            write_agent_workspace_file("guard", "agents/other/wiki/page.md", "nope")
                .expect_err("reject foreign wiki path");
        assert!(foreign_wiki_error.contains("只允许修改当前智能体可写的 workspace markdown 文件"));

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
            "我是产品经理，这个项目下周要上线，之后统一按周报格式同步，webhook 还没给我。",
            "收到，我会继续按周报格式跟进上线计划，并保留这个约定，等 webhook 后再做联调。",
        )
        .expect("append legacy memory");

        let memory = fs::read_to_string(root.join("agents").join("memory-agent").join("MEMORY.md"))
            .expect("read memory");
        // MEMORY.md 不再被自动写入，应保持模板内容
        assert!(memory.contains("最小启动记忆"));
        assert!(memory.contains("## Core Principles"));
        assert!(!memory.contains("Current note"));

        let user_profile = read_string(
            root.join("agents")
                .join("memory-agent")
                .join("memory")
                .join("categories")
                .join("user_profile.md"),
        );
        assert!(user_profile.contains("## Schema"));
        assert!(!user_profile.contains("- item: 我是产品经理"));

        let working =
            fs::read_to_string(root.join("agents").join("memory-agent").join("WORKING.md"))
                .expect("read working");
        assert!(working.contains("user-1"));
        assert!(working.contains("## OPEN_LOOPS"));
        assert!(working.contains("blocked:"));
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

        let memory_index = fs::read_to_string(
            root.join("agents")
                .join("memory-agent")
                .join("memory")
                .join("INDEX.md"),
        )
        .expect("read memory index");
        assert!(memory_index.contains("raw is evidence"));
        assert!(memory_index.contains("wiki/INDEX.md"));

        let review_queue = read_string(
            root.join("agents")
                .join("memory-agent")
                .join("memory")
                .join("REVIEW_QUEUE.md"),
        );
        assert!(review_queue.contains("review_at:"));
        assert!(review_queue.contains("pending"));
        assert!(review_queue.contains("webhook"));

        let lint = read_string(
            root.join("agents")
                .join("memory-agent")
                .join("memory")
                .join("LINT.md"),
        );
        assert!(lint.contains("## Scorecard"));
        assert!(lint.contains("Total: 100/100"));

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
        assert!(!root
            .join("agents")
            .join("memory-agent")
            .join("memory")
            .join("WIKI_INDEX.md")
            .exists());

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
            "我是产品经理，这个项目下周要上线，待办是之后统一按周报格式同步。",
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
        assert!(user_profile.contains("- item: 我是产品经理"));
        assert!(user_profile.contains("type: profile_fact"));
        assert!(user_profile.contains("confidence: high"));

        let decisions = fs::read_to_string(
            root.join("agents")
                .join("memory-agent-env")
                .join("memory")
                .join("categories")
                .join("decisions.md"),
        )
        .expect("read decisions category");
        assert!(decisions.contains("type: decision"));
        assert!(decisions.contains("status: active"));

        let commitments = fs::read_to_string(
            root.join("agents")
                .join("memory-agent-env")
                .join("memory")
                .join("categories")
                .join("commitments.md"),
        )
        .expect("read commitments category");
        assert!(commitments.contains("status: open"));
        assert!(commitments.contains("owner: memory-agent-env"));
        assert!(commitments.contains("created:"));
        assert!(commitments.contains("next_check:"));
        assert!(commitments.contains("source: memory/raw/"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
        std::env::remove_var(APPEND_CATEGORY_MEMORY_ENV);
    }

    #[test]
    fn append_agent_memory_entry_records_pitfalls_on_user_correction() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "pitfall-agent",
            name: "坑点助理",
            summary: "用于验证坑点沉淀",
            description: "负责记录用户纠正过的高风险错误",
            accent_color: Some("#667788"),
            is_builtin: false,
        };

        ensure_agent_workspace(seed, true).expect("scaffold workspace");
        append_agent_memory_entry(
            "pitfall-agent",
            "user-1",
            "别忽略时间线，也别问上下文明明能推出的问题。",
            "收到，我会先核时间线，再避免追问显然能推出的事情。",
        )
        .expect("append correction");

        let pitfalls = read_string(
            root.join("agents")
                .join("pitfall-agent")
                .join("PITFALLS.md"),
        );
        assert!(pitfalls.contains("别忽略时间线"));
        assert!(pitfalls.contains("risk: high"));
        assert!(pitfalls.contains("action:"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn register_agent_attachment_source_records_indexes() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "attach-agent",
            name: "附件助理",
            summary: "用于验证附件索引",
            description: "负责验证附件导入后的记忆系统",
            accent_color: Some("#8899aa"),
            is_builtin: false,
        };

        ensure_agent_workspace(seed, true).expect("scaffold workspace");
        let attachment = root.join("sample.txt");
        fs::write(&attachment, "hello attachment").expect("write attachment");

        register_agent_attachment_source(
            "attach-agent",
            "sample.txt",
            &attachment,
            Some("text/plain"),
            Some("初版附件"),
        )
        .expect("register attachment");

        let source_index = read_string(
            root.join("agents")
                .join("attach-agent")
                .join("memory")
                .join("SOURCE_INDEX.md"),
        );
        assert!(source_index.contains("attachment"));
        assert!(source_index.contains("sample.txt"));

        let log = read_string(
            root.join("agents")
                .join("attach-agent")
                .join("memory")
                .join("LOG.md"),
        );
        assert!(log.contains("source"));
        assert!(log.contains("sample.txt"));

        let daily_log = read_string(
            root.join("agents")
                .join("attach-agent")
                .join("memory")
                .join(format!("{}.md", current_date_label())),
        );
        assert!(daily_log.contains("收到附件"));
        assert!(daily_log.contains("sample.txt"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn workspace_prompt_routes_memory_review_and_wiki_paths() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);
        std::env::set_var(APPEND_CATEGORY_MEMORY_ENV, "1");

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
        .expect("append memory");
        write_agent_workspace_file(
            "query-agent",
            "agents/query-agent/USER_MODEL.md",
            "# USER_MODEL.md\n\n## Interaction Style\n\n- 用户更偏好主动型、少废话、结果先行的回答。\n",
        )
        .expect("write user model");
        write_agent_workspace_file(
            "query-agent",
            "agents/query-agent/PITFALLS.md",
            "# PITFALLS.md\n\n## Active Pitfalls\n\n- item: 别忽略时间线\n  risk: high\n  action: 先核时间与相对日期。\n  source: test\n  updated: 2026-04-07\n",
        )
        .expect("write pitfalls");
        write_agent_workspace_file(
            "query-agent",
            "agents/query-agent/RELATIONSHIP_MAP.md",
            "# RELATIONSHIP_MAP.md\n\n## Key People\n\n- 橘子：合作方联系人。\n",
        )
        .expect("write relationship map");

        let prompt = build_workspace_system_prompt_for_query(
            "query-agent",
            Some(
                "继续这个项目的发布规划，并按我之前喜欢的汇报风格整理，别再犯时间线错误。橘子是谁？",
            ),
        )
        .expect("workspace prompt");

        assert!(prompt.contains("memory/INDEX.md"));
        assert!(prompt.contains("wiki/INDEX.md"));
        assert!(prompt.contains("memory/REVIEW_QUEUE.md"));
        assert!(prompt.contains("memory/categories/projects.md"));
        assert!(prompt.contains("memory/categories/preferences.md"));
        assert!(prompt.contains("专项记忆路由"));
        assert!(prompt.contains("USER_MODEL"));
        assert!(prompt.contains("PITFALLS"));
        assert!(prompt.contains("RELATIONSHIP_MAP"));
        assert!(prompt.contains("相关分类记忆"));
        assert!(!prompt.contains("BOOTSTRAP.md"));
        assert!(!prompt.contains("WIKI_INDEX.md"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
        std::env::remove_var(APPEND_CATEGORY_MEMORY_ENV);
    }

    #[test]
    fn memory_system_scorecard_covers_multiple_paths() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);
        std::env::set_var(APPEND_CATEGORY_MEMORY_ENV, "1");

        let seed = AgentWorkspaceSeed {
            id: "score-agent",
            name: "评分助理",
            summary: "用于验证完整记忆系统改造",
            description: "负责跑通 scaffold、ingest、attachment、prompt、template 路径",
            accent_color: Some("#334455"),
            is_builtin: false,
        };

        let second_seed = AgentWorkspaceSeed {
            id: "score-agent-2",
            name: "评分助理二号",
            summary: "用于验证模板继承",
            description: "负责验证 _template 作为默认骨架",
            accent_color: Some("#556677"),
            is_builtin: false,
        };

        let home = ensure_agent_workspace(seed, true).expect("scaffold first agent");
        append_agent_memory_entry(
            "score-agent",
            "user-9",
            "我是产品经理，这个项目下周要上线，待办是统一按周报格式同步，webhook 还没提供。",
            "收到，我会按周报格式推进，并把 webhook 阻塞记进复查队列。",
        )
        .expect("append memory");

        let attachment = root.join("score-attachment.txt");
        fs::write(&attachment, "score attachment").expect("write attachment");
        register_agent_attachment_source(
            "score-agent",
            "score-attachment.txt",
            &attachment,
            Some("text/plain"),
            Some("评分路径附件"),
        )
        .expect("register attachment");

        let prompt = build_workspace_system_prompt_for_query(
            "score-agent",
            Some("继续这个项目，并按我之前喜欢的格式整理待办。"),
        )
        .expect("build prompt");

        let second_home = ensure_agent_workspace(second_seed, true).expect("scaffold second agent");

        let memory_index = read_string(home.join("memory").join("INDEX.md"));
        let review_queue = read_string(home.join("memory").join("REVIEW_QUEUE.md"));
        let commitments = read_string(
            home.join("memory")
                .join("categories")
                .join("commitments.md"),
        );
        let pitfalls = read_string(home.join("PITFALLS.md"));
        let working = read_string(home.join("WORKING.md"));
        let lint = read_string(home.join("memory").join("LINT.md"));
        let source_index = read_string(home.join("memory").join("SOURCE_INDEX.md"));
        let log = read_string(home.join("memory").join("LOG.md"));
        let template_root = root.join("agents").join("_template");

        let scores = vec![
            score_scenario(
                "scaffold",
                vec![
                    ("BOOTSTRAP removed", !home.join("BOOTSTRAP.md").exists()),
                    (
                        "memory index scaffolded",
                        home.join("memory").join("INDEX.md").exists(),
                    ),
                    (
                        "review queue scaffolded",
                        home.join("memory").join("REVIEW_QUEUE.md").exists(),
                    ),
                    (
                        "wiki index scaffolded",
                        home.join("wiki").join("INDEX.md").exists(),
                    ),
                    ("pitfalls scaffolded", home.join("PITFALLS.md").exists()),
                    (
                        "legacy WIKI_INDEX removed",
                        !home.join("memory").join("WIKI_INDEX.md").exists(),
                    ),
                ],
            ),
            score_scenario(
                "ingest",
                vec![
                    (
                        "working updated",
                        read_string(home.join("WORKING.md")).contains("Current note"),
                    ),
                    (
                        "open loops updated",
                        working.contains("OPEN_LOOPS") && working.contains("blocked:"),
                    ),
                    (
                        "daily log created",
                        home.join("memory")
                            .join(format!("{}.md", current_date_label()))
                            .exists(),
                    ),
                    (
                        "review queue scheduled",
                        review_queue.contains("review_at:"),
                    ),
                    (
                        "source index recorded conversation",
                        source_index.contains("Index: type=conversation"),
                    ),
                    ("lint scorecard emitted", lint.contains("Total: 100/100")),
                ],
            ),
            score_scenario(
                "category-ledger",
                vec![
                    (
                        "commitments structured",
                        commitments.contains("status: open"),
                    ),
                    (
                        "commitments owner set",
                        commitments.contains("owner: score-agent"),
                    ),
                    (
                        "commitments next_check set",
                        commitments.contains("next_check:"),
                    ),
                    (
                        "inferences shard exists",
                        home.join("memory")
                            .join("categories")
                            .join("inferences.md")
                            .exists(),
                    ),
                    (
                        "pitfalls shard exists",
                        home.join("memory")
                            .join("categories")
                            .join("pitfalls.md")
                            .exists(),
                    ),
                    (
                        "memory index boundary text",
                        memory_index.contains("raw is evidence"),
                    ),
                ],
            ),
            score_scenario(
                "attachment",
                vec![
                    (
                        "attachment indexed",
                        source_index.contains("score-attachment.txt"),
                    ),
                    ("attachment logged", log.contains("score-attachment.txt")),
                    (
                        "attachment daily log recorded",
                        read_string(
                            home.join("memory")
                                .join(format!("{}.md", current_date_label())),
                        )
                        .contains("收到附件"),
                    ),
                    (
                        "attachment kept evidence boundary",
                        source_index.contains("attachment"),
                    ),
                    (
                        "lint survived attachment refresh",
                        lint.contains("Status: healthy"),
                    ),
                ],
            ),
            score_scenario(
                "prompt-routing",
                vec![
                    (
                        "prompt points to memory index",
                        prompt.contains("memory/INDEX.md"),
                    ),
                    (
                        "prompt points to wiki index",
                        prompt.contains("wiki/INDEX.md"),
                    ),
                    (
                        "prompt points to review queue",
                        prompt.contains("memory/REVIEW_QUEUE.md"),
                    ),
                    ("prompt includes query route", prompt.contains("检索顺序")),
                    ("prompt omits BOOTSTRAP", !prompt.contains("BOOTSTRAP.md")),
                    ("prompt omits WIKI_INDEX", !prompt.contains("WIKI_INDEX.md")),
                ],
            ),
            score_scenario(
                "template-rollout",
                vec![
                    ("_template exists", template_root.exists()),
                    (
                        "template memory index exists",
                        template_root.join("memory").join("INDEX.md").exists(),
                    ),
                    (
                        "template review queue exists",
                        template_root
                            .join("memory")
                            .join("REVIEW_QUEUE.md")
                            .exists(),
                    ),
                    (
                        "template inferences shard exists",
                        template_root
                            .join("memory")
                            .join("categories")
                            .join("inferences.md")
                            .exists(),
                    ),
                    (
                        "template pitfalls exists",
                        template_root.join("PITFALLS.md").exists(),
                    ),
                    (
                        "second agent inherits no bootstrap",
                        !second_home.join("BOOTSTRAP.md").exists(),
                    ),
                ],
            ),
            score_scenario(
                "pitfall-playbook",
                vec![
                    (
                        "pitfalls file active",
                        pitfalls.contains("## Active Pitfalls"),
                    ),
                    ("pitfalls file exists", !pitfalls.trim().is_empty()),
                    (
                        "user model template exists",
                        home.join("USER_MODEL.md").exists(),
                    ),
                    (
                        "relationship map template exists",
                        home.join("RELATIONSHIP_MAP.md").exists(),
                    ),
                    (
                        "lint checks human model files",
                        lint.contains("USER_MODEL / PITFALLS / RELATIONSHIP_MAP"),
                    ),
                ],
            ),
        ];

        for score in &scores {
            print_score(score);
            assert!(
                score.score >= 80,
                "{} score too low: {}",
                score.name,
                score.score
            );
        }

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
        std::env::remove_var(APPEND_CATEGORY_MEMORY_ENV);
    }

    #[test]
    fn resolves_media_reference_by_bare_file_name_from_agent_outbox() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let outbox = root
            .join("agents")
            .join("test-agent")
            .join("outbox")
            .join("2026-04-09");
        fs::create_dir_all(&outbox).expect("create outbox");
        let target = outbox.join("123456-user-wechat_login_screenshot.png");
        fs::write(&target, b"png").expect("write file");

        let resolved =
            resolve_agent_media_reference(Some("test-agent"), "wechat_login_screenshot.png")
                .expect("resolve bare file name");
        assert_eq!(resolved, target);

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }
}
