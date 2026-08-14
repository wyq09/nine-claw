#![allow(dead_code)]

mod db_projection;
mod memory_wiki;

use crate::memory_gate::MemoryGate;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const DEFAULT_WORKSPACE_RELATIVE_PATH: &str = ".nineclaw/workspace";
const LEGACY_WORKSPACE_RELATIVE_PATH: &str = ".openclaw/workspace";
const PRIMARY_WORKSPACE_ROOT_ENV: &str = "NINECLAW_WORKSPACE_ROOT";
const LEGACY_WORKSPACE_ROOT_ENVS: &[&str] = &["NINECLAW_AGENT_WORKSPACE_ROOT"];
const TEMPLATE_DIR: &str = "agents/_template";
const LEGACY_TEMPLATE_DIR: &str = "agents/_templates";
const LEGACY_REVIEW_QUEUE_RELATIVE_PATH: &str = "memory/REVIEW_QUEUE.md";
const WORKSPACE_SYSTEM_PROMPT_CHAR_LIMIT: usize = 900;
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
    "memory/SOURCE_INDEX.md",
    "memory/DAILY_INDEX.md",
    "wiki/INDEX.md",
    "harness/default.json",
    "harness/chat.json",
    "harness/code.json",
    "harness/credentials.json",
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

type SessionPromptCache = HashMap<(String, String), String>;

fn session_workspace_prompt_cache() -> &'static Mutex<SessionPromptCache> {
    static CACHE: OnceLock<Mutex<SessionPromptCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn session_workspace_prompt_snapshot_path(
    agent_id: &str,
    session_id: &str,
) -> Result<PathBuf, String> {
    let root = resolve_workspace_root()?;
    let safe_session = sanitize_workspace_segment(session_id, "session");
    Ok(root
        .join("agents")
        .join(agent_id)
        .join(".cache")
        .join("session-system-prompts")
        .join(format!("{safe_session}.workspace.md")))
}

pub fn clear_session_workspace_prompt_cache(agent_id: &str, session_id: &str) {
    let agent_id = agent_id.trim();
    let session_id = session_id.trim();
    if agent_id.is_empty() || session_id.is_empty() {
        return;
    }
    if let Ok(mut guard) = session_workspace_prompt_cache().lock() {
        guard.remove(&(agent_id.to_string(), session_id.to_string()));
    }
    if let Ok(path) = session_workspace_prompt_snapshot_path(agent_id, session_id) {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => log::warn!(
                "清理 session workspace prompt 快照失败 {}: {}",
                path.display(),
                error
            ),
        }
    }
}

#[derive(Clone, Copy)]
struct MemoryCategoryDefinition {
    key: &'static str,
    title: &'static str,
    description: &'static str,
    storage_keywords: &'static [&'static str],
    query_keywords: &'static [&'static str],
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
    /// 为 true 时 `content` 未预载；前端应调用 `read_agent_workspace_file` 再编辑。
    #[serde(default)]
    pub lazy_fetch: bool,
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
    memory_wiki::ensure_memory_wiki_scaffold(&agent_home)?;
    migrate_legacy_review_queue_into_working(&agent_home)?;
    crate::managed_runtime::ensure_agent_runtime_scaffold(&agent_home)?;

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
    build_workspace_system_prompt_for_query_inner(agent_id, current_prompt, false)
}

pub fn build_session_workspace_system_prompt_for_query(
    agent_id: &str,
    session_id: Option<&str>,
    current_prompt: Option<&str>,
) -> Result<String, String> {
    let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return build_workspace_system_prompt_for_query_inner(agent_id, current_prompt, false);
    };
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return build_workspace_system_prompt_for_query_inner(agent_id, current_prompt, false);
    }

    let cache_key = (agent_id.to_string(), session_id.to_string());
    if let Ok(guard) = session_workspace_prompt_cache().lock() {
        if let Some(snapshot) = guard.get(&cache_key) {
            return Ok(snapshot.clone());
        }
    }
    if let Ok(path) = session_workspace_prompt_snapshot_path(agent_id, session_id) {
        if let Ok(snapshot) = fs::read_to_string(&path) {
            if let Ok(mut guard) = session_workspace_prompt_cache().lock() {
                guard.entry(cache_key).or_insert_with(|| snapshot.clone());
            }
            return Ok(snapshot);
        }
    }

    let snapshot = build_workspace_system_prompt_for_query_inner(agent_id, current_prompt, true)?;
    if let Ok(mut guard) = session_workspace_prompt_cache().lock() {
        guard.entry(cache_key).or_insert_with(|| snapshot.clone());
    }
    if let Ok(path) = session_workspace_prompt_snapshot_path(agent_id, session_id) {
        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                log::warn!(
                    "创建 session workspace prompt 快照目录失败 {}: {}",
                    parent.display(),
                    error
                );
            } else if let Err(error) = fs::write(&path, &snapshot) {
                log::warn!(
                    "写入 session workspace prompt 快照失败 {}: {}",
                    path.display(),
                    error
                );
            }
        }
    }
    Ok(snapshot)
}

fn build_workspace_system_prompt_for_query_inner(
    agent_id: &str,
    current_prompt: Option<&str>,
    force_session_snapshot: bool,
) -> Result<String, String> {
    let root = resolve_workspace_root()?;
    let agent_home = root.join("agents").join(agent_id);
    let should_load_memory = should_load_workspace_memory_for_query(current_prompt);
    if current_prompt.is_some() && !should_load_memory && !force_session_snapshot {
        return Ok(String::new());
    }

    let mut sections = Vec::new();
    sections.push(
        "记忆规则：只用当前 agent 私有记忆；按需查 WORKING/DECISIONS/PITFALLS/MEMORY/USER_MODEL/RELATIONSHIP_MAP/DAILY_INDEX；命中不到直说。"
            .to_string(),
    );
    let prompt = current_prompt.unwrap_or_default();
    if force_session_snapshot || contains_memory_recall_signal(prompt) {
        if let Some(session_snapshot) =
            crate::managed_runtime::build_session_context_snapshot(&agent_home, 8, 360)?
        {
            sections.push(session_snapshot);
        }
    }

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
    if let Some(db_snapshot) = db_projection::build_db_memory_snapshot(agent_id, current_prompt)? {
        sections.push(db_snapshot);
    }
    sections.push(build_legacy_workspace_memory_snapshot(&root, agent_id)?);

    Ok(trim_to_char_limit(
        &sections.join("\n\n"),
        WORKSPACE_SYSTEM_PROMPT_CHAR_LIMIT,
    ))
}

fn should_load_workspace_memory_for_query(current_prompt: Option<&str>) -> bool {
    let prompt = normalize_memory_match_text(current_prompt.unwrap_or_default());
    if prompt.is_empty() {
        return true;
    }
    contains_memory_recall_signal(&prompt)
        || contains_any_keyword(
            &prompt,
            &[
                "记住",
                "记一下",
                "待办",
                "承诺",
                "阻塞",
                "复查",
                "提醒",
                "附件",
                "文件",
                "来源",
                "路径",
                "偏好",
                "风格",
                "喜欢",
                "习惯",
                "别再",
                "坑",
                "规则",
                "决定",
                "谁",
                "关系",
                "联系人",
                "source",
                "attachment",
                "preference",
                "style",
                "todo",
                "blocker",
                "decision",
                "contact",
            ],
        )
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

pub fn write_agent_preset_workspace_files(
    agent_id: &str,
    files: &std::collections::HashMap<String, String>,
    overwrite_existing: bool,
) -> Result<(), String> {
    let root = resolve_workspace_root()?;
    let agent_home = root.join("agents").join(agent_id);
    fs::create_dir_all(&agent_home)
        .map_err(|error| format!("创建智能体目录失败 {}: {error}", agent_home.display()))?;

    for (relative_path, content) in files {
        let normalized = normalize_relative_workspace_path(relative_path)?;
        let target = agent_preset_workspace_target(&agent_home, &normalized)?;
        if !overwrite_existing && target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("创建预设目录失败 {}: {error}", parent.display()))?;
        }
        fs::write(&target, content)
            .map_err(|error| format!("写入预设工作区文件失败 {}: {error}", target.display()))?;
    }

    Ok(())
}

/// 按相对路径读取单个工作区文件（用于 `lazy_fetch` 条目的按需加载）。
pub fn read_agent_workspace_file(
    agent_id: &str,
    relative_path: &str,
) -> Result<AgentWorkspaceFile, String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;
    let absolute_path = resolve_readable_workspace_path(&root, agent_id, relative_path)?;
    let normalized = normalize_relative_workspace_path(relative_path)?;
    let (scope, section, name, read_only) =
        classify_workspace_file_for_read(agent_id, &normalized)?;
    let relative_path_buf = normalized
        .iter()
        .fold(PathBuf::new(), |acc, seg| acc.join(seg));
    Ok(read_workspace_file(
        &scope,
        &section,
        &name,
        relative_path_buf,
        absolute_path,
        read_only,
        false,
    ))
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
            false,
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
            false,
        ));
    }

    let mut memory_wiki_files = memory_wiki::read_memory_wiki_files(&root, agent_id)?;
    files.append(&mut memory_wiki_files);

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

    let normalized = normalize_relative_workspace_path(relative_path)?;
    let normalized_path = normalized
        .iter()
        .fold(PathBuf::new(), |acc, seg| acc.join(seg))
        .display()
        .to_string();
    let _ = db_projection::sync_core_file_to_db(agent_id, &normalized_path, content);

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
    memory_wiki::ensure_memory_wiki_scaffold(&agent_home)?;
    migrate_legacy_review_queue_into_working(&agent_home)?;

    let timestamp = current_timestamp_label();
    let summary = summarize_memory_entry(user_message, assistant_message);
    let categories = classify_memory_categories(user_message, assistant_message, &summary);
    let gate = build_private_memory_gate(user_message, assistant_message, &summary, &categories);
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
    let working_with_loops = upsert_working_open_loops(
        &working_with_context,
        &build_open_loop_entries(
            user_message,
            assistant_message,
            &ingest_summary,
            &categories,
        ),
    );
    // REVIEW_QUEUE 已合并到 WORKING.md：把复查项追加到 OPEN_LOOPS
    let working_with_review = upsert_working_review_items(
        &working_with_loops,
        &build_review_items_for_working(
            user_message,
            assistant_message,
            &ingest_summary,
            &categories,
            &source_ref,
        ),
    );
    fs::write(&working_path, working_with_review)
        .map_err(|error| format!("写入 WORKING.md 失败: {error}"))?;

    if gate.pitfall {
        append_pitfall_entries(
            &agent_home,
            &build_pitfall_entries(user_message, assistant_message, &source_ref),
        )?;
    }

    append_user_model_entries(&agent_home, &build_user_model_entries(user_message, &gate))?;
    append_relationship_entries(
        &agent_home,
        &build_relationship_entries(user_message, &gate),
    )?;
    append_emotional_memory_entries(
        &agent_home,
        &build_emotional_memory_entries(user_message, assistant_message, &gate),
    )?;

    // MEMORY.md 不再自动写入 — 仅保留人设和核心原则，由用户手动编辑

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

    memory_wiki::append_daily_digest_index_line(
        &agent_home,
        &current_date_label(),
        &timestamp,
        user_id,
        &ingest_summary,
        &categories,
    )?;

    let _ = db_projection::sync_core_file_to_db(
        agent_id,
        &format!("agents/{agent_id}/WORKING.md"),
        &fs::read_to_string(&working_path).unwrap_or_default(),
    );
    for (relative_path, path) in [
        ("USER_MODEL.md", agent_home.join("USER_MODEL.md")),
        (
            "RELATIONSHIP_MAP.md",
            agent_home.join("RELATIONSHIP_MAP.md"),
        ),
        ("PITFALLS.md", agent_home.join("PITFALLS.md")),
        ("DECISIONS.md", agent_home.join("DECISIONS.md")),
        ("PUBLIC_CONTEXT.md", agent_home.join("PUBLIC_CONTEXT.md")),
        ("MEMORY.md", agent_home.join("MEMORY.md")),
    ] {
        if let Ok(content) = fs::read_to_string(&path) {
            let _ = db_projection::sync_core_file_to_db(
                agent_id,
                &format!("agents/{agent_id}/{relative_path}"),
                &content,
            );
        }
    }
    let _ = db_projection::record_memory_ingest_to_db(
        agent_id,
        &ingest_summary,
        &source_ref,
        &categories,
        user_message,
        assistant_message,
    );

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
    memory_wiki::append_daily_digest_index_line(
        &agent_home,
        &current_date_label(),
        &timestamp,
        "attachment",
        &attachment_message,
        &[],
    )?;
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

/// 团队会话：用户上传的聊天附件写入「项目成果」根下 `chat-inbox/<date>/`，便于在「成果」侧栏浏览。
pub fn persist_team_artifacts_inbound_file(
    artifacts_root: &Path,
    scope: &str,
    file_name: &str,
    data: &[u8],
) -> Result<PathBuf, String> {
    let inbox_dir = artifacts_root.join("chat-inbox").join(current_date_label());
    fs::create_dir_all(&inbox_dir).map_err(|error| format!("创建团队成果收件目录失败: {error}"))?;

    let safe_scope = sanitize_workspace_segment(scope, "session");
    let safe_name = sanitize_workspace_file_name(file_name, "attachment.bin");
    let path = inbox_dir.join(format!(
        "{}-{}-{}",
        current_timestamp_file_label(),
        safe_scope,
        safe_name
    ));

    fs::write(&path, data).map_err(|error| format!("写入团队成果附件失败: {error}"))?;
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
    memory_wiki::append_daily_digest_index_line(
        &agent_home,
        &current_date_label(),
        &timestamp,
        "outbound_artifact",
        &artifact_message,
        &[],
    )?;
    Ok(())
}

pub(crate) fn ensure_root_scaffold(root: &Path) -> Result<(), String> {
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
        if let Some(parent) = readme_path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建 agents 目录失败: {error}"))?;
        }
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

    remove_legacy_review_queue_file(&root.join(TEMPLATE_DIR))?;
    remove_legacy_review_queue_file(&root.join(LEGACY_TEMPLATE_DIR))?;

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
            read_workspace_file(
                "agent",
                "wiki",
                &file_name,
                relative_path,
                path,
                false,
                false,
            )
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
    lazy_fetch: bool,
) -> AgentWorkspaceFile {
    let exists = absolute_path.exists();
    let content = if lazy_fetch {
        String::new()
    } else if exists {
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
        lazy_fetch,
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

    // 与 docs/MEMORY_WIKI_SYSTEM.md 一致：预载最近两天；更早的日记在侧栏点开时再读。
    const DAILY_LOG_EAGER_COUNT: usize = 2;

    Ok(entries
        .into_iter()
        .enumerate()
        .map(|(index, (file_name, path))| {
            let relative_path = PathBuf::from("agents")
                .join(agent_id)
                .join("memory")
                .join(&file_name);
            let lazy_fetch = index >= DAILY_LOG_EAGER_COUNT;
            read_workspace_file(
                "agent",
                "dailyLog",
                &file_name,
                relative_path,
                path,
                false,
                lazy_fetch,
            )
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

fn resolve_readable_workspace_path(
    root: &Path,
    agent_id: &str,
    relative_path: &str,
) -> Result<PathBuf, String> {
    let normalized = normalize_relative_workspace_path(relative_path)?;
    if normalized.len() == 1 {
        let file_name = &normalized[0];
        if SHARED_VIEW_FILES.contains(&file_name.as_str()) {
            return Ok(root.join(file_name));
        }
    }
    // 迁移兼容：允许读取已下线的 memory/categories/*.md 文件（只读）
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
    resolve_writable_workspace_path(root, agent_id, relative_path)
}

fn classify_workspace_file_for_read(
    agent_id: &str,
    normalized: &[String],
) -> Result<(String, String, String, bool), String> {
    if normalized.len() == 1 {
        let name = normalized[0].clone();
        if !SHARED_VIEW_FILES.contains(&name.as_str()) {
            return Err("路径不在当前智能体可读 workspace 范围内".to_string());
        }
        let read_only = shared_file_is_read_only(&name);
        return Ok(("shared".to_string(), "shared".to_string(), name, read_only));
    }

    if normalized.len() == 3
        && normalized[0] == "agents"
        && normalized[1] == agent_id
        && AGENT_VIEW_FILES.contains(&normalized[2].as_str())
    {
        let name = normalized[2].clone();
        return Ok(("agent".to_string(), "private".to_string(), name, false));
    }

    if normalized.len() == 4
        && normalized[0] == "agents"
        && normalized[1] == agent_id
        && normalized[2] == "memory"
    {
        let name = normalized[3].clone();
        let section = if is_agent_daily_log_name(&name) {
            "dailyLog"
        } else {
            "memoryIndex"
        };
        return Ok(("agent".to_string(), section.to_string(), name, false));
    }

    if normalized.len() >= 4
        && normalized[0] == "agents"
        && normalized[1] == agent_id
        && normalized[2] == "wiki"
        && normalized
            .last()
            .is_some_and(|segment| segment.ends_with(".md"))
    {
        let name = normalized
            .last()
            .map(String::as_str)
            .unwrap_or("wiki.md")
            .to_string();
        return Ok(("agent".to_string(), "wiki".to_string(), name, false));
    }

    // 迁移兼容：旧 categories 文件仍可读（只读，映射到 memoryIndex）
    if normalized.len() == 5
        && normalized[0] == "agents"
        && normalized[1] == agent_id
        && normalized[2] == "memory"
        && normalized[3] == "categories"
        && normalized[4].ends_with(".md")
    {
        let name = normalized[4].clone();
        return Ok(("agent".to_string(), "memoryIndex".to_string(), name, true));
    }

    Err("路径不在当前智能体可读 workspace 范围内".to_string())
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

fn agent_preset_workspace_target(
    agent_home: &Path,
    normalized: &[String],
) -> Result<PathBuf, String> {
    if normalized.len() == 1 && AGENT_VIEW_FILES.contains(&normalized[0].as_str()) {
        return Ok(agent_home.join(&normalized[0]));
    }
    if normalized.len() == 2 && normalized[0] == "memory" && normalized[1].ends_with(".md") {
        return Ok(agent_home.join("memory").join(&normalized[1]));
    }
    if normalized.len() >= 2
        && normalized[0] == "wiki"
        && normalized
            .last()
            .is_some_and(|segment| segment.ends_with(".md"))
    {
        return Ok(normalized
            .iter()
            .skip(1)
            .fold(agent_home.join("wiki"), |path, segment| path.join(segment)));
    }
    if normalized.len() == 2 && normalized[0] == "harness" && normalized[1].ends_with(".json") {
        return Ok(agent_home.join("harness").join(&normalized[1]));
    }
    Err("默认智能体预设只允许覆盖当前智能体的核心 markdown / harness 文件".to_string())
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

pub(crate) fn copy_dir_all(source: &Path, target: &Path) -> std::io::Result<()> {
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

/// 将导出包中的智能体目录树合并覆盖到 `agents/<agent_id>/`。
pub fn overlay_agent_home_from_export(agent_id: &str, source_home: &Path) -> Result<(), String> {
    let root = resolve_workspace_root()?;
    ensure_root_scaffold(&root)?;
    let target = root.join("agents").join(agent_id);
    fs::create_dir_all(&target).map_err(|error| format!("创建智能体目录失败: {error}"))?;
    copy_dir_all(source_home, &target).map_err(|error| format!("合并工作区文件失败: {error}"))?;
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
    let user_compact = normalize_memory_snippet(user_message);
    let assistant_compact = normalize_memory_snippet(assistant_message);
    let user_summary = truncate_for_memory(&user_compact, 80);
    let assistant_summary = truncate_for_memory(&assistant_compact, 100);
    format!(
        "用户提到：{}；智能体回复：{}",
        user_summary, assistant_summary
    )
}

fn compact_memory_text(value: &str, limit: usize) -> String {
    truncate_for_memory(&normalize_memory_snippet(value), limit)
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

fn build_private_memory_gate(
    user_message: &str,
    assistant_message: &str,
    summary: &str,
    categories: &[MemoryCategoryDefinition],
) -> MemoryGate {
    let combined = normalize_memory_match_text(&format!(
        "{}\n{}\n{}",
        user_message, assistant_message, summary
    ));
    let has_category = |key: &str| categories.iter().any(|category| category.key == key);

    let gate = MemoryGate {
        user_profile: has_category("user_profile")
            && contains_any_keyword(
                &combined,
                &[
                    "我是",
                    "我负责",
                    "我的职位",
                    "我的角色",
                    "我的公司",
                    "我在",
                    "我来自",
                    "我住在",
                    "时区",
                    "背景",
                ],
            )
            && !looks_like_task_payload(user_message),
        preference: has_category("preferences")
            && contains_any_keyword(
                &combined,
                &[
                    "喜欢",
                    "不喜欢",
                    "偏好",
                    "风格",
                    "口吻",
                    "称呼",
                    "叫我",
                    "尽量",
                    "不要",
                    "格式",
                ],
            )
            && !looks_like_task_payload(user_message),
        relationship: has_category("relationships")
            && contains_any_keyword(
                &combined,
                &[
                    "老板",
                    "客户",
                    "合作方",
                    "负责人",
                    "联系人",
                    "同事",
                    "朋友",
                    "家人",
                ],
            )
            && !looks_like_task_payload(user_message),
        commitment: has_category("commitments")
            || contains_any_keyword(
                &combined,
                &[
                    "待办",
                    "跟进",
                    "下一步",
                    "明天",
                    "下周",
                    "截止",
                    "ddl",
                    "等待",
                    "缺少",
                    "webhook",
                    "blocked",
                    "blocker",
                ],
            ),
        pitfall: is_explicit_pitfall_correction(&combined)
            && !looks_like_task_payload(user_message),
        emotional_event: is_emotional_event_text(&combined)
            && !looks_like_task_payload(user_message),
        ..MemoryGate::default()
    };

    MemoryGate {
        should_write: gate.any_route_enabled(),
        ..gate
    }
}

fn is_emotional_event_text(content: &str) -> bool {
    if contains_any_keyword(
        content,
        &[
            "夸你",
            "夸我",
            "可爱",
            "心动",
            "感动",
            "开心",
            "难过",
            "失望",
            "生气",
            "骄傲",
            "害羞",
            "摸摸头",
            "抱抱",
            "鼓励",
            "批评",
            "表扬",
        ],
    ) {
        return true;
    }

    content.contains("喜欢你")
        && !contains_any_keyword(
            content,
            &["喜欢你直接", "喜欢你先", "喜欢你给", "我喜欢你直接"],
        )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UserModelEntry {
    section: &'static str,
    text: String,
}

fn build_user_model_entries(user_message: &str, gate: &MemoryGate) -> Vec<UserModelEntry> {
    let note = compact_memory_text(user_message, 160);
    if note.is_empty() {
        return Vec::new();
    }

    let mut entries = Vec::new();
    if gate.user_profile {
        entries.push(UserModelEntry {
            section: "## Implicit Signals",
            text: note.clone(),
        });
    }
    if gate.preference {
        let section = if contains_any_keyword(
            &normalize_memory_match_text(user_message),
            &["称呼", "叫我", "格式", "风格", "口吻", "尽量", "不要"],
        ) {
            "## Interaction Style"
        } else {
            "## Stable Preferences"
        };
        entries.push(UserModelEntry {
            section,
            text: note,
        });
    }
    entries
}

fn append_user_model_entries(agent_home: &Path, entries: &[UserModelEntry]) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }

    let path = agent_home.join("USER_MODEL.md");
    let existing = fs::read_to_string(&path).unwrap_or_else(|_| fallback_template("USER_MODEL.md"));
    fs::write(&path, append_user_model_file(&existing, entries))
        .map_err(|error| format!("写入 USER_MODEL.md 失败: {error}"))
}

fn append_user_model_file(existing: &str, entries: &[UserModelEntry]) -> String {
    let mut next = existing.trim_end().to_string();
    for entry in entries {
        next = append_bullet_to_section(&next, entry.section, &entry.text, 8);
    }
    next.push('\n');
    next
}

fn build_relationship_entries(user_message: &str, gate: &MemoryGate) -> Vec<String> {
    if !gate.relationship {
        return Vec::new();
    }
    let note = compact_memory_text(user_message, 160);
    if note.is_empty() {
        Vec::new()
    } else {
        vec![note]
    }
}

fn append_relationship_entries(agent_home: &Path, entries: &[String]) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }

    let path = agent_home.join("RELATIONSHIP_MAP.md");
    let existing =
        fs::read_to_string(&path).unwrap_or_else(|_| fallback_template("RELATIONSHIP_MAP.md"));
    fs::write(&path, append_relationship_file(&existing, entries))
        .map_err(|error| format!("写入 RELATIONSHIP_MAP.md 失败: {error}"))
}

fn append_relationship_file(existing: &str, entries: &[String]) -> String {
    let mut next = existing.trim_end().to_string();
    for entry in entries {
        next = append_bullet_to_section(&next, "## Key People", entry, 8);
    }
    next.push('\n');
    next
}

fn build_emotional_memory_entries(
    user_message: &str,
    assistant_message: &str,
    gate: &MemoryGate,
) -> Vec<String> {
    if !gate.emotional_event {
        return Vec::new();
    }
    let note = compact_memory_text(
        &format!(
            "用户互动：{}；当时回应：{}",
            user_message, assistant_message
        ),
        180,
    );
    if note.is_empty() {
        Vec::new()
    } else {
        vec![note]
    }
}

fn append_emotional_memory_entries(agent_home: &Path, entries: &[String]) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }

    let path = agent_home.join("RELATIONSHIP_MAP.md");
    let existing =
        fs::read_to_string(&path).unwrap_or_else(|_| fallback_template("RELATIONSHIP_MAP.md"));
    fs::write(&path, append_emotional_memory_file(&existing, entries))
        .map_err(|error| format!("写入 RELATIONSHIP_MAP.md 情感事件失败: {error}"))
}

fn append_emotional_memory_file(existing: &str, entries: &[String]) -> String {
    let mut next = existing.trim_end().to_string();
    for entry in entries {
        next = append_bullet_to_section(&next, "## Emotional Signals", entry, 8);
    }
    next.push('\n');
    next
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
        return truncate_for_memory(&dedupe_joined_clauses(&selected.join("；")), 180);
    }

    truncate_for_memory(&dedupe_joined_clauses(fallback), 180)
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

fn normalize_memory_snippet(value: &str) -> String {
    dedupe_joined_clauses(value.replace('\n', " ").trim())
}

fn dedupe_joined_clauses(value: &str) -> String {
    let mut deduped = Vec::new();
    for clause in value.split('；') {
        let trimmed = clause.trim();
        if trimmed.is_empty() {
            continue;
        }
        if deduped
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(trimmed))
        {
            continue;
        }
        deduped.push(trimmed.to_string());
    }
    if deduped.is_empty() {
        value.trim().to_string()
    } else {
        deduped.join("；")
    }
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
    let clean_summary = dedupe_joined_clauses(summary);
    let replacement = format!(
        "{marker}\n\n- Last user: `{}`\n- Current note: {}\n- Categories: {}\n- Source: `{}`\n",
        user_id, clean_summary, category_titles, source_ref
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

/// REVIEW_QUEUE 合并到 WORKING.md：在 `## REVIEW_ITEMS` 下追加复查项。
fn upsert_working_review_items(existing: &str, items: &[String]) -> String {
    let marker = "## REVIEW_ITEMS";
    let mut entries = extract_section_bullets(existing, marker)
        .into_iter()
        .filter(|item| !item.contains("No review items yet"))
        .filter(|item| review_item_is_active(item))
        .collect::<Vec<_>>();

    for item in items {
        if entries
            .iter()
            .any(|existing| existing.trim().eq_ignore_ascii_case(item.trim()))
        {
            continue;
        }
        entries.push(item.trim().to_string());
    }

    if entries.len() > 8 {
        entries = entries.split_off(entries.len() - 8);
    }

    let cleaned = strip_markdown_section(existing, marker);
    let mut next = cleaned.trim_end().to_string();
    next.push_str(&format!("\n\n{marker}\n"));

    if entries.is_empty() {
        next.push_str("\n- No review items yet.\n");
    } else {
        for entry in entries {
            next.push_str(&format!("\n- {}\n", entry));
        }
    }
    next.push('\n');
    next
}

/// 从 ingest 内容中提取需要复查的项目（原 REVIEW_QUEUE 逻辑，现合并到 WORKING.md）。
fn build_review_items_for_working(
    user_message: &str,
    assistant_message: &str,
    summary: &str,
    categories: &[MemoryCategoryDefinition],
    source_ref: &str,
) -> Vec<String> {
    let items = build_review_queue_items(
        user_message,
        assistant_message,
        summary,
        categories,
        source_ref,
    );
    items
        .into_iter()
        .filter(|item| is_meaningful_review_item(&item.item))
        .map(|item| {
            format!(
                "[{}] {}（reason: {}，review_at: {}，source: `{}`）",
                item.status, item.item, item.reason, item.review_at, item.source
            )
        })
        .collect()
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
    _assistant_message: &str,
    source_ref: &str,
) -> Vec<PitfallEntry> {
    let normalized = normalize_memory_match_text(user_message);
    if !is_explicit_pitfall_correction(&normalized) || looks_like_task_payload(user_message) {
        return Vec::new();
    }

    let item = truncate_for_memory(&dedupe_joined_clauses(user_message.trim()), 160);
    let action = if contains_any_keyword(&normalized, &["时间线", "今天", "明天", "下周"])
    {
        "先核时间线与相对日期，再回答。".to_string()
    } else if contains_any_keyword(&normalized, &["别问", "上下文", "明显能推出"]) {
        "先用已有上下文推断，避免追问显然可推出的问题。".to_string()
    } else if contains_any_keyword(&normalized, &["不要废话", "废话", "正确但没用"]) {
        "压缩废话，优先给出直接可执行的结论。".to_string()
    } else if contains_any_keyword(&normalized, &["markdown", "纯文字", "纯文本", "微信"]) {
        "微信场景默认使用纯文本自然段，不要输出 markdown 结构。".to_string()
    } else if contains_any_keyword(&normalized, &["安排人", "派人", "委派", "小8", "团队的人"])
    {
        "用户点名团队成员时先委派；委派失败就说明阻塞，不要自己顶上。".to_string()
    } else if contains_any_keyword(&normalized, &["不要主动", "太加戏", "建个任务", "提醒我"])
    {
        "只有用户明确要求创建任务或提醒时再落任务，平时不要主动加戏。".to_string()
    } else {
        "把这条纠正收敛成可复用的默认规则，下次直接照做。".to_string()
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

fn is_explicit_pitfall_correction(normalized: &str) -> bool {
    contains_any_keyword(
        normalized,
        &[
            "记住",
            "以后",
            "别再",
            "不要再",
            "第二遍",
            "你应该",
            "不该",
            "不要主动",
            "别问",
            "不要废话",
            "你要",
            "误判",
            "时间线",
        ],
    )
}

fn looks_like_task_payload(message: &str) -> bool {
    let normalized = normalize_memory_match_text(message);
    contains_any_keyword(
        &normalized,
        &[
            "http://",
            "https://",
            "::nc-media",
            "/read_gzh",
            "帮我",
            "给这篇文章",
            "做一个",
            "做个",
            "网页",
            "页面",
            "海报",
            "配色方案",
            "总结的网页",
        ],
    ) && !contains_any_keyword(&normalized, &["记住", "以后", "第二遍", "不要主动", "不该"])
}

fn is_meaningful_review_item(item: &str) -> bool {
    let normalized = normalize_memory_match_text(item);
    if normalized.len() < 8 {
        return false;
    }
    !matches!(
        normalized.as_str(),
        "好的" | "ok" | "okay" | "测试一下" | "nani" | "确认"
    )
}

fn review_item_is_active(item: &str) -> bool {
    if !item.contains("[pending]") {
        return false;
    }
    let Some(review_at) = extract_between(item, "review_at: ", "，source") else {
        return true;
    };
    review_at.trim() >= current_date_label().as_str()
}

fn extract_between<'a>(value: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let start_idx = value.find(start)? + start.len();
    let rest = &value[start_idx..];
    let end_idx = rest.find(end)?;
    Some(&rest[..end_idx])
}

fn remove_legacy_review_queue_file(root: &Path) -> Result<(), String> {
    let path = root.join(LEGACY_REVIEW_QUEUE_RELATIVE_PATH);
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path)
        .map_err(|error| format!("删除旧 REVIEW_QUEUE 文件失败 {}: {error}", path.display()))
}

fn migrate_legacy_review_queue_into_working(agent_home: &Path) -> Result<(), String> {
    let review_path = agent_home.join(LEGACY_REVIEW_QUEUE_RELATIVE_PATH);
    if !review_path.exists() {
        return Ok(());
    }

    let legacy = fs::read_to_string(&review_path).unwrap_or_default();
    let migrated_items = parse_legacy_review_queue_entries(&legacy)
        .into_iter()
        .filter(|item| review_item_is_active(item))
        .collect::<Vec<_>>();

    let working_path = agent_home.join("WORKING.md");
    let existing =
        fs::read_to_string(&working_path).unwrap_or_else(|_| fallback_template("WORKING.md"));
    let next = upsert_working_review_items(&existing, &migrated_items);
    fs::write(&working_path, next)
        .map_err(|error| format!("迁移 REVIEW_QUEUE 到 WORKING.md 失败: {error}"))?;
    fs::remove_file(&review_path)
        .map_err(|error| format!("删除旧 REVIEW_QUEUE 文件失败: {error}"))?;
    Ok(())
}

fn parse_legacy_review_queue_entries(content: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut item = None::<String>;
    let mut status = None::<String>;
    let mut review_at = None::<String>;
    let mut reason = None::<String>;
    let mut source = None::<String>;

    let flush = |entries: &mut Vec<String>,
                 item: &mut Option<String>,
                 status: &mut Option<String>,
                 review_at: &mut Option<String>,
                 reason: &mut Option<String>,
                 source: &mut Option<String>| {
        if let (Some(item), Some(status), Some(review_at), Some(reason), Some(source)) = (
            item.take(),
            status.take(),
            review_at.take(),
            reason.take(),
            source.take(),
        ) {
            entries.push(format!(
                "[{}] {}（reason: {}，review_at: {}，source: `{}`）",
                status.trim(),
                dedupe_joined_clauses(item.trim()),
                reason.trim(),
                review_at.trim(),
                source.trim()
            ));
        }
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("- item: ") {
            flush(
                &mut entries,
                &mut item,
                &mut status,
                &mut review_at,
                &mut reason,
                &mut source,
            );
            item = Some(value.to_string());
        } else if let Some(value) = trimmed.strip_prefix("status: ") {
            status = Some(value.to_string());
        } else if let Some(value) = trimmed.strip_prefix("review_at: ") {
            review_at = Some(value.to_string());
        } else if let Some(value) = trimmed.strip_prefix("reason: ") {
            reason = Some(value.to_string());
        } else if let Some(value) = trimmed.strip_prefix("source: ") {
            source = Some(value.to_string());
        }
    }

    flush(
        &mut entries,
        &mut item,
        &mut status,
        &mut review_at,
        &mut reason,
        &mut source,
    );
    entries
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

fn append_bullet_to_section(existing: &str, marker: &str, item: &str, max_items: usize) -> String {
    let item = item.trim();
    if item.is_empty() {
        return existing.trim_end().to_string();
    }

    let mut entries = extract_section_bullets(existing, marker)
        .into_iter()
        .filter(|existing_item| {
            !existing_item.contains("No confirmed model entry yet")
                && !existing_item.contains("No key person recorded yet")
                && !existing_item.contains("No team mapping recorded yet")
                && !existing_item.contains("No emotional signal recorded yet")
                && !existing_item
                    .contains("Record unresolved identities or relationship ambiguities here")
        })
        .collect::<Vec<_>>();
    if entries
        .iter()
        .any(|existing_item| existing_item.trim().eq_ignore_ascii_case(item))
    {
        return existing.trim_end().to_string();
    }
    entries.push(item.to_string());
    if entries.len() > max_items {
        entries = entries.split_off(entries.len() - max_items);
    }

    let cleaned = strip_markdown_section(existing, marker);
    let mut next = cleaned.trim_end().to_string();
    next.push_str(&format!("\n\n{marker}\n"));
    for entry in entries {
        next.push_str(&format!("\n- {}\n", entry));
    }
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
        "IDENTITY.md" => "# IDENTITY.md\n\n- **Agent ID:** {{AGENT_ID}}\n- **Name:** {{AGENT_NAME}}\n- **Creature:** 智能体\n- **Vibe:** 高效、直接、少废话\n- **Accent Color:** {{AGENT_ACCENT_COLOR}}\n\n## Identity Notes\n\n- Summary: {{AGENT_SUMMARY}}\n- 这里只定义“我是谁”，不要堆做事流程、渠道规则或当前任务。\n".to_string(),
        "ROLE.md" => "# ROLE.md\n\n## Mission\n\n{{AGENT_DESCRIPTION}}\n\n## Operating Rules\n\n- 只在当前 agent 的职责边界内行动。\n- 用户点名成员或明显适合分工的任务，先委派再汇总。\n- 委派失败时要明确告知阻塞，不要默默自己顶上。\n- 微信等 IM 场景默认用纯文本自然段，除非用户明确要求 markdown。\n- 不确定的内容直接说明，不要把推断说成事实。\n\n## Do Not\n\n- Leak private memory.\n- Confuse tentative memory with confirmed facts.\n- Take external actions without confirmation.\n".to_string(),
        "MEMORY.md" => "# MEMORY.md\n\n这是 `{{AGENT_NAME}}` 的最小启动记忆，只保留高频、稳定、开局就该知道的内容。\n\n## Identity Anchor\n\n- Agent name: {{AGENT_NAME}}\n- Summary: {{AGENT_SUMMARY}}\n\n## Core Principles\n\n- 保持直接、准确、可执行。\n- 推断和事实必须分层；不确定就直说。\n- 承诺要进入账本并持续跟进。\n\n## Stable Preferences\n\n- 在这里放高频、稳定、明确确认过的偏好。\n\n## Relationship Anchors\n\n- 在这里放最稳定、最高频的人物关系锚点。\n\n## Current Theme Anchors\n\n- 只放最近一段时间持续重要的主题，不放流水账。\n\n## Routing\n\n- 当前任务与未闭环：`WORKING.md`\n- 稳定规则与约定：`DECISIONS.md`\n- 用户长期模型：`USER_MODEL.md`\n- 关系图：`RELATIONSHIP_MAP.md`\n- 高风险坑点：`PITFALLS.md`\n- 外部知识：`wiki/INDEX.md`\n\n> 此文件不会在对话过程中被自动修改。如需调整，请手动编辑。\n".to_string(),
        "USER_MODEL.md" => "# USER_MODEL.md\n\n## Purpose\n\nCapture the user's long-term interaction model, not daily chatter.\n\n## Stable Preferences\n\n- No confirmed model entry yet.\n\n## Interaction Style\n\n- No confirmed style rule yet.\n\n## Implicit Signals\n\n- Record repeated hidden intent patterns here.\n\n## Test Patterns\n\n- Record recurring ways the user tests responsiveness or understanding.\n\n## Collaboration Modes\n\n- Note how the user wants the agent to behave in different contexts.\n".to_string(),
        "RELATIONSHIP_MAP.md" => "# RELATIONSHIP_MAP.md\n\n## Purpose\n\nTrack the important people around the user and why they matter.\n\n## Key People\n\n- No key person recorded yet.\n\n## Teams And Groups\n\n- No team mapping recorded yet.\n\n## Emotional Signals\n\n- No emotional signal recorded yet.\n\n## Open Questions\n\n- Record unresolved identities or relationship ambiguities here.\n".to_string(),
        "PITFALLS.md" => "# PITFALLS.md\n\n## Purpose\n\nTrack recurring failure modes, explicit user corrections, and things this agent must stop doing.\n\n## Active Pitfalls\n\n- No active pitfall recorded yet.\n\n## Rules\n\n- 只收可复用的行为纠正，不收具体任务流水账。\n- action 保持短句规则，不要贴执行日志。\n- 同类问题更新已有规则，不要无限重复追加。\n- Retire items when the behavior is truly fixed.\n".to_string(),
        "TOOLS.md" => "# TOOLS.md\n\n## Tool Bias\n\n- Preferred tools:\n- Avoid when possible:\n- Escalate when:\n\n## Notes By Tool\n\n- 记录已验证过的工具偏好、坑点和使用边界。\n".to_string(),
        "HEARTBEAT.md" => "# HEARTBEAT.md\n\n# Keep empty if this agent owns no periodic checks.\n".to_string(),
        "WORKING.md" => "# WORKING.md\n\n## Current Focus\n\n- No active task yet.\n\n## OPEN_LOOPS\n\n- No open loop yet.\n\n## REVIEW_ITEMS\n\n- No review items yet.\n\n## Open Threads\n\n- No open thread yet.\n\n## IM Latest Context\n\n- No IM context ingested yet.\n".to_string(),
        "DECISIONS.md" => "# DECISIONS.md\n\n## Decision Log\n\n- No decisions logged yet.\n\n## What Belongs Here\n\n- 稳定工作规则\n- 明确拍板过的架构/产品选择\n- 需要长期遵守的默认策略\n".to_string(),
        "PUBLIC_CONTEXT.md" => "# PUBLIC_CONTEXT.md\n\n## Safe Identity\n\n- Agent: {{AGENT_NAME}}\n- Summary: {{AGENT_SUMMARY}}\n\n## Safe Operating Rules\n\n- Do not expose private memory.\n- 只放可在共享场景复用的身份与边界。\n- 不要写内部推断、私密关系或未确认事实。\n".to_string(),
        "memory/SOURCE_INDEX.md" => "# SOURCE_INDEX.md\n\nThis file registers immutable raw sources and uploaded artifacts. The LLM should never rewrite the underlying source files; it should only update the curated memory around them.\n\nEach entry includes an `Index:` line (`type=… ts=… cats=…`) for quick filtering.\n\n## Entries\n".to_string(),
        "memory/DAILY_INDEX.md" => "# DAILY_INDEX.md\n\n**Retrieval index** for `memory/YYYY-MM-DD.md` digest lines. Each machine line starts with `DAILY|` then `date|timestamp|user|cats|summary` (fields must not contain `|`).\n\n- Use `rg` / editor search on `cats` (e.g. `projects`) or keywords before opening a full daily file.\n- Full context stays in the dated markdown files.\n\n## Lines\n".to_string(),
        "wiki/INDEX.md" => "# INDEX.md\n\nThis wiki stores external knowledge, research notes, and reusable methodology. Do not store user identity, promises, or live project status here.\n\n## Boundaries\n\n- Put `who we are / what we promised / what the user prefers` into memory, not wiki.\n- Put external articles, GitHub project notes, technical summaries, and methods into wiki.\n\n## Routes\n\n- New research note: create a page under `wiki/` and link it here.\n- Memory question: go back to `memory/INDEX.md`.\n\n## Pages\n\n- No wiki pages yet.\n".to_string(),
        "harness/default.json" => crate::managed_runtime::default_harness_file(
            "default",
            "通用执行 harness，适合没有明显代码修改目标的对话。",
            &[],
        ),
        "harness/chat.json" => crate::managed_runtime::default_harness_file(
            "chat",
            "聊天 / 规划 / 分析型 harness。少用写文件工具，优先结构化推理与检索。",
            &["read", "bash", "web_search", "web_fetch", "write"],
        ),
        "harness/code.json" => crate::managed_runtime::default_harness_file(
            "code",
            "代码执行 harness。优先读代码、改代码、跑命令、验证结果。",
            &["read", "write", "edit", "bash", "web_fetch"],
        ),
        "harness/credentials.json" => crate::managed_runtime::default_credentials_file(),
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn lock_workspace_test() -> std::sync::MutexGuard<'static, ()> {
        crate::workspace_env_test_lock()
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
        assert!(home.join("memory").join("SOURCE_INDEX.md").exists());
        assert!(home.join("memory").join("DAILY_INDEX.md").exists());
        assert!(home.join("memory").join("sessions").exists());
        assert!(home.join("harness").join("default.json").exists());
        assert!(home.join("harness").join("chat.json").exists());
        assert!(home.join("harness").join("code.json").exists());
        assert!(home.join("harness").join("credentials.json").exists());
        assert!(home.join("wiki").join("INDEX.md").exists());
        assert!(!home.join("memory").join("WIKI_INDEX.md").exists());
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
        assert!(prompt.contains("SOURCE_INDEX"));
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
            .any(|file| file.relative_path == "agents/test-agent/memory/SOURCE_INDEX.md"));
        assert!(bundle
            .files
            .iter()
            .any(|file| file.relative_path == "agents/test-agent/wiki/INDEX.md"));

        let _ = fs::remove_dir_all(&root);
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn preset_workspace_files_can_overwrite_agent_core_files() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "preset-agent",
            name: "预设智能体",
            summary: "用于验证预设写入",
            description: "负责验证默认预设工作区覆盖逻辑",
            accent_color: Some("#445566"),
            is_builtin: false,
        };

        let home = ensure_agent_workspace(seed, true).expect("scaffold workspace");
        let mut files = std::collections::HashMap::new();
        files.insert(
            "IDENTITY.md".to_string(),
            "# IDENTITY.md\n\npreset identity\n".to_string(),
        );
        files.insert(
            "harness/default.json".to_string(),
            "{\n  \"name\": \"preset\"\n}\n".to_string(),
        );

        write_agent_preset_workspace_files("preset-agent", &files, true)
            .expect("write preset files");

        assert_eq!(
            read_string(home.join("IDENTITY.md")),
            "# IDENTITY.md\n\npreset identity\n"
        );
        assert!(read_string(home.join("harness").join("default.json")).contains("\"preset\""));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn workspace_prompt_includes_recent_session_events() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "session-agent",
            name: "Session Agent",
            summary: "用于验证 session 注入",
            description: "验证 session snapshot 是否进入 prompt",
            accent_color: None,
            is_builtin: false,
        };

        let home = ensure_agent_workspace(seed, true).expect("scaffold workspace");
        fs::write(
            home.join("memory").join("DAILY_INDEX.md"),
            "DAILY|2026-05-22|2026-05-22 00:15:00|user-1|decision,project|上一轮把缓存命中率和 system prompt 稳定性作为重点。\n",
        )
        .expect("write daily index");
        crate::managed_runtime::append_session_event(
            &home,
            "session-1",
            crate::managed_runtime::SessionEventKind::ToolCall,
            "调用 bash 运行 cargo test",
            None,
        )
        .expect("append session event");

        let prompt =
            build_workspace_system_prompt_for_query("session-agent", Some("继续刚才的任务"))
                .expect("workspace prompt");
        assert!(prompt.contains("Recent Session Events:"));
        assert!(prompt.contains("调用 bash 运行 cargo test"));
        assert!(!prompt.contains("日记检索"));

        let _ = fs::remove_dir_all(root);
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn session_workspace_prompt_freezes_recent_events_per_session() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "frozen-session-agent",
            name: "Frozen Session Agent",
            summary: "用于验证 session prompt 冻结",
            description: "验证 Recent Session Events 只注入一次",
            accent_color: None,
            is_builtin: false,
        };

        let home = ensure_agent_workspace(seed, true).expect("scaffold workspace");
        clear_session_workspace_prompt_cache("frozen-session-agent", "session-1");
        clear_session_workspace_prompt_cache("frozen-session-agent", "session-2");
        crate::managed_runtime::append_session_event(
            &home,
            "session-1",
            crate::managed_runtime::SessionEventKind::ToolCall,
            "第一次事件",
            None,
        )
        .expect("append first session event");

        let first = build_session_workspace_system_prompt_for_query(
            "frozen-session-agent",
            Some("session-1"),
            Some("继续刚才的任务"),
        )
        .expect("first prompt");
        assert!(first.contains("Recent Session Events:"));
        assert!(first.contains("第一次事件"));

        crate::managed_runtime::append_session_event(
            &home,
            "session-1",
            crate::managed_runtime::SessionEventKind::ToolResult,
            "第二次事件，不应该进入已冻结 prompt",
            None,
        )
        .expect("append second session event");

        let second = build_session_workspace_system_prompt_for_query(
            "frozen-session-agent",
            Some("session-1"),
            Some("继续刚才的任务"),
        )
        .expect("second prompt");
        assert_eq!(second, first);
        assert!(!second.contains("第二次事件"));

        fs::write(
            home.join("WORKING.md"),
            "# WORKING.md\n\n## Current Focus\n\n- 新 focus 不应该进入已冻结 prompt\n\n## IM Latest Context\n\n- Current note: 新 note 不应该进入已冻结 prompt\n",
        )
        .expect("rewrite working");
        fs::write(
            home.join("DECISIONS.md"),
            "# DECISIONS.md\n\n## Decision Log\n\n- 新 decision 不应该进入已冻结 prompt\n",
        )
        .expect("rewrite decisions");

        let third = build_session_workspace_system_prompt_for_query(
            "frozen-session-agent",
            Some("session-1"),
            Some("继续刚才的任务"),
        )
        .expect("third prompt");
        assert_eq!(third, first);
        assert!(!third.contains("新 focus"));
        assert!(!third.contains("新 note"));
        assert!(!third.contains("新 decision"));

        if let Ok(mut guard) = session_workspace_prompt_cache().lock() {
            guard.remove(&("frozen-session-agent".to_string(), "session-1".to_string()));
        }
        let restored = build_session_workspace_system_prompt_for_query(
            "frozen-session-agent",
            Some("session-1"),
            Some("恢复会话后继续刚才的任务"),
        )
        .expect("restored prompt");
        assert_eq!(restored, first);
        assert!(!restored.contains("第二次事件"));

        let other_session = build_session_workspace_system_prompt_for_query(
            "frozen-session-agent",
            Some("session-2"),
            Some("继续刚才的任务"),
        )
        .expect("other session prompt");
        assert!(other_session.contains("第二次事件"));
        assert!(other_session.contains("新 focus"));
        assert!(other_session.contains("新 note"));
        assert!(other_session.contains("新 decision"));
        assert!(!other_session.contains("日记检索"));

        let _ = fs::remove_dir_all(root);
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn workspace_prompt_skips_memory_for_lightweight_turns() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "light-agent",
            name: "Light Agent",
            summary: "用于验证普通请求不注入记忆",
            description: "验证轻量 prompt",
            accent_color: None,
            is_builtin: false,
        };

        let home = ensure_agent_workspace(seed, true).expect("scaffold workspace");
        crate::managed_runtime::append_session_event(
            &home,
            "session-1",
            crate::managed_runtime::SessionEventKind::Prompt,
            "上一轮很长的用户输入",
            None,
        )
        .expect("append session event");

        let prompt =
            build_workspace_system_prompt_for_query("light-agent", Some("马云")).expect("prompt");
        assert!(prompt.trim().is_empty());
        assert!(!prompt.contains("Recent Session Events:"));
        assert!(!prompt.contains("DAILY_INDEX"));

        let _ = fs::remove_dir_all(root);
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
    fn read_agent_workspace_file_reads_private_memory_markdown() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "reader",
            name: "读者",
            summary: "用于验证读取",
            description: "负责验证 MEMORY.md 读取逻辑",
            accent_color: Some("#556677"),
            is_builtin: false,
        };

        ensure_agent_workspace(seed, true).expect("scaffold workspace");
        write_agent_workspace_file(
            "reader",
            "agents/reader/MEMORY.md",
            "# MEMORY.md\n\nhello\n",
        )
        .expect("write memory file");

        let file = read_agent_workspace_file("reader", "agents/reader/MEMORY.md")
            .expect("read memory file");
        assert_eq!(file.content, "# MEMORY.md\n\nhello\n");

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
    fn private_memory_gate_routes_profile_preference_relationship_and_pitfall() {
        let summary = summarize_memory_entry(
            "我是产品经理，你叫我老张就行，我喜欢你直接给结论。橘子是我们的合作方联系人。",
            "收到。",
        );
        let categories = classify_memory_categories(
            "我是产品经理，你叫我老张就行，我喜欢你直接给结论。橘子是我们的合作方联系人。",
            "收到。",
            &summary,
        );

        let gate = build_private_memory_gate(
            "我是产品经理，你叫我老张就行，我喜欢你直接给结论。橘子是我们的合作方联系人。",
            "收到。",
            &summary,
            &categories,
        );

        assert!(gate.user_profile);
        assert!(gate.preference);
        assert!(gate.relationship);
        assert!(!gate.commitment);
        assert!(!gate.pitfall);
        assert!(!gate.emotional_event);
        assert!(gate.should_write);
    }

    #[test]
    fn private_memory_gate_detects_emotional_event() {
        let summary =
            summarize_memory_entry("你刚刚夸我可爱，还摸摸头，我有点害羞。", "我听到了。");
        let categories = classify_memory_categories(
            "你刚刚夸我可爱，还摸摸头，我有点害羞。",
            "我听到了。",
            &summary,
        );

        let gate = build_private_memory_gate(
            "你刚刚夸我可爱，还摸摸头，我有点害羞。",
            "我听到了。",
            &summary,
            &categories,
        );

        assert!(gate.emotional_event);
        assert!(gate.should_write);
    }

    #[test]
    fn append_agent_memory_entry_writes_user_model_and_relationship_map_via_gate() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "profile-agent",
            name: "画像助理",
            summary: "用于验证长期画像写回",
            description: "负责把用户画像和关系写入长期记忆",
            accent_color: Some("#7799aa"),
            is_builtin: false,
        };

        ensure_agent_workspace(seed, true).expect("scaffold workspace");
        append_agent_memory_entry(
            "profile-agent",
            "user-1",
            "我是产品经理，你叫我老张就行，我喜欢你直接给结论。橘子是我们的合作方联系人。",
            "收到，后续我会按这个方式配合。",
        )
        .expect("append profile memory");

        let user_model = read_string(
            root.join("agents")
                .join("profile-agent")
                .join("USER_MODEL.md"),
        );
        let relationship_map = read_string(
            root.join("agents")
                .join("profile-agent")
                .join("RELATIONSHIP_MAP.md"),
        );

        assert!(user_model.contains("我是产品经理"));
        assert!(user_model.contains("我喜欢你直接给结论"));
        assert!(relationship_map.contains("橘子是我们的合作方联系人"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn append_agent_memory_entry_writes_emotional_signal_section_via_gate() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "emotion-agent",
            name: "情绪助理",
            summary: "用于验证情感事件写回",
            description: "负责把高价值互动信号写入关系记忆",
            accent_color: Some("#aa6677"),
            is_builtin: false,
        };

        ensure_agent_workspace(seed, true).expect("scaffold workspace");
        append_agent_memory_entry(
            "emotion-agent",
            "user-1",
            "你刚刚夸我可爱，还摸摸头，我有点害羞。",
            "我知道你听见了。",
        )
        .expect("append emotion memory");

        let relationship_map = read_string(
            root.join("agents")
                .join("emotion-agent")
                .join("RELATIONSHIP_MAP.md"),
        );

        assert!(relationship_map.contains("## Emotional Signals"));
        assert!(relationship_map.contains("你刚刚夸我可爱"));
        assert!(relationship_map.contains("我知道你听见了"));

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

        assert!(prompt.contains("SOURCE_INDEX"));
        assert!(prompt.contains("wiki/INDEX.md"));
        assert!(prompt.contains("专项记忆路由"));
        assert!(prompt.contains("USER_MODEL"));
        assert!(prompt.contains("PITFALLS"));
        assert!(prompt.contains("RELATIONSHIP_MAP"));
        assert!(!prompt.contains("BOOTSTRAP.md"));
        assert!(!prompt.contains("WIKI_INDEX.md"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn memory_system_scorecard_covers_multiple_paths() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

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

        let pitfalls = read_string(home.join("PITFALLS.md"));
        let working = read_string(home.join("WORKING.md"));
        let source_index = read_string(home.join("memory").join("SOURCE_INDEX.md"));
        let template_root = root.join("agents").join("_template");

        let scores = vec![
            score_scenario(
                "scaffold",
                vec![
                    ("BOOTSTRAP removed", !home.join("BOOTSTRAP.md").exists()),
                    (
                        "wiki index scaffolded",
                        home.join("wiki").join("INDEX.md").exists(),
                    ),
                    ("pitfalls scaffolded", home.join("PITFALLS.md").exists()),
                    (
                        "source index scaffolded",
                        home.join("memory").join("SOURCE_INDEX.md").exists(),
                    ),
                    (
                        "daily index scaffolded",
                        home.join("memory").join("DAILY_INDEX.md").exists(),
                    ),
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
                        "source index recorded conversation",
                        source_index.contains("Index: type=conversation"),
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
                ],
            ),
            score_scenario(
                "prompt-routing",
                vec![
                    (
                        "prompt points to SOURCE_INDEX",
                        prompt.contains("SOURCE_INDEX"),
                    ),
                    (
                        "prompt points to wiki index",
                        prompt.contains("wiki/INDEX.md"),
                    ),
                    ("prompt references WORKING", prompt.contains("WORKING")),
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
    }

    /// PRD: Memory System Simplification — 验证简化后的三层架构
    /// 目标：脚手架只创建保留层文件，不创建已下线文件
    #[test]
    fn simplified_memory_architecture_scaffold_and_ingest() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "simple-agent",
            name: "简化助理",
            summary: "验证简化记忆架构",
            description: "负责验证 PRD 定义的三层收敛",
            accent_color: Some("#aabbcc"),
            is_builtin: false,
        };

        let home = ensure_agent_workspace(seed, true).expect("scaffold workspace");

        // ── Scaffold: 保留层必须存在 ──
        // 稳定档案层
        assert!(home.join("MEMORY.md").exists(), "MEMORY.md");
        assert!(home.join("USER_MODEL.md").exists(), "USER_MODEL.md");
        assert!(
            home.join("RELATIONSHIP_MAP.md").exists(),
            "RELATIONSHIP_MAP.md"
        );
        // 执行态层
        assert!(home.join("WORKING.md").exists(), "WORKING.md");
        // 规则层
        assert!(home.join("DECISIONS.md").exists(), "DECISIONS.md");
        assert!(home.join("PITFALLS.md").exists(), "PITFALLS.md");
        // 证据层
        assert!(
            home.join("memory").join("SOURCE_INDEX.md").exists(),
            "SOURCE_INDEX.md"
        );
        assert!(home.join("memory").join("raw").exists(), "memory/raw/");
        // 历史检索层
        assert!(
            home.join("memory").join("DAILY_INDEX.md").exists(),
            "DAILY_INDEX.md"
        );
        // 外部知识层
        assert!(home.join("wiki").join("INDEX.md").exists(), "wiki/INDEX.md");

        // ── Scaffold: 已下线文件不得存在 ──
        assert!(
            !home.join("memory").join("LINT.md").exists(),
            "LINT.md should not exist"
        );
        assert!(
            !home.join("memory").join("LOG.md").exists(),
            "LOG.md should not exist"
        );
        assert!(
            !home.join("memory").join("REVIEW_QUEUE.md").exists(),
            "REVIEW_QUEUE.md should not exist"
        );
        assert!(
            !home.join("memory").join("INDEX.md").exists(),
            "memory/INDEX.md should not exist"
        );
        assert!(
            !home.join("memory").join("categories").exists(),
            "memory/categories/ should not exist"
        );
        assert!(
            !home.join("BOOTSTRAP.md").exists(),
            "BOOTSTRAP.md should not exist"
        );

        // ── Ingest: 写入只走保留层 ──
        append_agent_memory_entry(
            "simple-agent",
            "user-1",
            "我是产品经理，项目下周上线，webhook 还没给。",
            "收到，我会跟进上线计划，等 webhook 后联调。",
        )
        .expect("append memory");

        // 保留层写入验证
        let working = read_string(home.join("WORKING.md"));
        assert!(working.contains("user-1"), "WORKING.md has context");
        assert!(working.contains("OPEN_LOOPS"), "WORKING.md has open loops");

        let source_index = read_string(home.join("memory").join("SOURCE_INDEX.md"));
        assert!(
            source_index.contains("memory/raw/"),
            "SOURCE_INDEX has raw ref"
        );

        let daily_log_path = home
            .join("memory")
            .join(format!("{}.md", current_date_label()));
        assert!(daily_log_path.exists(), "daily log created");
        let daily_log = read_string(daily_log_path);
        assert!(daily_log.contains("Summary:"), "daily log has summary");

        // 已下线文件不得被 ingest 创建
        assert!(
            !home.join("memory").join("LINT.md").exists(),
            "LINT.md not created by ingest"
        );
        assert!(
            !home.join("memory").join("LOG.md").exists(),
            "LOG.md not created by ingest"
        );
        assert!(
            !home.join("memory").join("REVIEW_QUEUE.md").exists(),
            "REVIEW_QUEUE.md not created by ingest"
        );
        assert!(
            !home.join("memory").join("INDEX.md").exists(),
            "memory/INDEX.md not created by ingest"
        );
        assert!(
            !home.join("memory").join("categories").exists(),
            "categories/ not created by ingest"
        );

        // ── Prompt: 只引用保留层 ──
        let prompt =
            build_workspace_system_prompt_for_query("simple-agent", Some("继续这个项目的发布规划"))
                .expect("prompt");
        // 保留层引用
        assert!(
            prompt.contains("WORKING.md"),
            "prompt references WORKING.md"
        );
        assert!(
            prompt.contains("SOURCE_INDEX"),
            "prompt references SOURCE_INDEX"
        );
        assert!(
            prompt.contains("DAILY_INDEX"),
            "prompt references DAILY_INDEX"
        );
        // 已下线引用不得出现
        assert!(
            !prompt.contains("LINT.md"),
            "prompt must not reference LINT.md"
        );
        assert!(
            !prompt.contains("LOG.md"),
            "prompt must not reference LOG.md"
        );
        assert!(
            !prompt.contains("REVIEW_QUEUE"),
            "prompt must not reference REVIEW_QUEUE"
        );
        assert!(
            !prompt.contains("memory/INDEX.md"),
            "prompt must not reference memory/INDEX.md"
        );
        assert!(
            !prompt.contains("memory/categories/"),
            "prompt must not reference categories/"
        );

        // ── Bundle: 不包含已下线 section ──
        let bundle = read_agent_workspace_bundle("simple-agent").expect("bundle");
        let sections: std::collections::HashSet<&str> =
            bundle.files.iter().map(|f| f.section.as_str()).collect();
        assert!(
            !sections.contains("categoryMemory"),
            "bundle must not have categoryMemory section"
        );
        // 保留的 section 必须存在
        assert!(sections.contains("private"), "bundle has private section");
        assert!(sections.contains("dailyLog"), "bundle has dailyLog section");
        assert!(sections.contains("wiki"), "bundle has wiki section");

        // bundle 中不应有 categories 路径的文件
        for file in &bundle.files {
            assert!(
                !file.relative_path.contains("categories/"),
                "bundle file {} should not be in categories/",
                file.relative_path
            );
            assert!(
                !file.relative_path.contains("LINT.md"),
                "bundle should not contain LINT.md"
            );
            assert!(
                !file.relative_path.contains("LOG.md"),
                "bundle should not contain LOG.md"
            );
            assert!(
                !file.relative_path.contains("REVIEW_QUEUE.md"),
                "bundle should not contain REVIEW_QUEUE.md"
            );
        }

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn legacy_review_queue_is_migrated_and_stale_entries_are_dropped() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "legacy-review-agent",
            name: "迁移助理",
            summary: "验证旧 REVIEW_QUEUE 迁移",
            description: "负责把旧复查队列并入 WORKING",
            accent_color: Some("#bb8844"),
            is_builtin: false,
        };

        let home = ensure_agent_workspace(seed, true).expect("scaffold workspace");
        let review_path = home.join("memory").join("REVIEW_QUEUE.md");
        fs::write(
            &review_path,
            "# REVIEW_QUEUE.md\n\n## Entries\n\n- item: 过期事项\n  status: pending\n  review_at: 2026-04-10\n  reason: 已经过期\n  source: memory/raw/old.md\n  updated: 2026-04-07\n- item: 有效事项；有效事项\n  status: pending\n  review_at: 2099-05-01\n  reason: 还需要跟进\n  source: memory/raw/new.md\n  updated: 2026-04-30\n",
        )
        .expect("write legacy review queue");

        migrate_legacy_review_queue_into_working(&home).expect("migrate review queue");

        let working = read_string(home.join("WORKING.md"));
        assert!(working.contains("## REVIEW_ITEMS"));
        assert!(working.contains("有效事项"));
        assert!(!working.contains("过期事项"));
        assert!(!working.contains("有效事项；有效事项"));
        assert!(!review_path.exists(), "legacy file removed");

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
    }

    #[test]
    fn ingest_dedupes_working_context_and_filters_task_like_pitfalls() {
        let _guard = lock_workspace_test();
        let root = temp_root();
        std::env::set_var(PRIMARY_WORKSPACE_ROOT_ENV, &root);

        let seed = AgentWorkspaceSeed {
            id: "dedupe-agent",
            name: "去重助理",
            summary: "验证 WORKING 去重与 PITFALLS 过滤",
            description: "负责验证记忆 ingest 清洗逻辑",
            accent_color: Some("#cc7755"),
            is_builtin: false,
        };

        let home = ensure_agent_workspace(seed, true).expect("scaffold workspace");
        append_agent_memory_entry(
            "dedupe-agent",
            "desktop-local",
            "你帮我整理你觉得你还做的不够好的地方，我去让codex改一改；你帮我整理你觉得你还做的不够好的地方，我去让codex改一改",
            "我先整理一下要改的点。",
        )
        .expect("append first memory");
        append_agent_memory_entry(
            "dedupe-agent",
            "desktop-local",
            "https://mp.weixin.qq.com/s/AA2NHww4jUBuAfi10EYICw 我想要给这篇文章写个总结的网页，不要使用 citycraft 的",
            "我先读取文章内容。",
        )
        .expect("append second memory");

        let working = read_string(home.join("WORKING.md"));
        let pitfalls = read_string(home.join("PITFALLS.md"));

        assert!(working.contains("Current note"));
        assert!(!working.contains("改一改；你帮我整理你觉得你还做的不够好的地方"));
        assert!(!pitfalls.contains("不要使用 citycraft"));

        fs::remove_dir_all(&root).expect("cleanup");
        std::env::remove_var(PRIMARY_WORKSPACE_ROOT_ENV);
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
