use crate::agent_capabilities::{
    dedupe_skill_ids, new_agent_default_capability_policy, normalize_capability_policy,
    static_capability_policy, AgentCapabilityPolicy,
};
use crate::agent_workspace::{self, AgentWorkspaceSeed};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::AppHandle;
use uuid::Uuid;

const DEFAULT_AGENT_STATE_KEY: &str = "default_agent_id";
const DEFAULT_ALLOWED_TOOL_IDS: &[&str] = &[
    "bash",
    "read_file",
    "write_file",
    "edit_file",
    "grep",
    "list_dir",
    "glob",
    "web_search",
    "web_fetch",
    "image_generate",
    "image_task_query",
    "agent_spawn",
    "external_api",
];

/// 用户在「系统指令」中填写的内容会进入 `agent_system_prompt`；不应对其做过短截断，仅保留与模型侧类似的硬上限防误粘贴。
const MAX_USER_SYSTEM_INSTRUCTION_CHARS: usize = 32_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentScenarioLlmSlot {
    #[serde(default)]
    pub provider_id: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentScenarioLlmConfig {
    #[serde(default)]
    pub title_generation: Option<AgentScenarioLlmSlot>,
    #[serde(default)]
    pub memory_extraction: Option<AgentScenarioLlmSlot>,
    /// 定时任务在任务中心 / 推送通知里展示的标题与一句话简介（同一次 LLM 调用生成）
    #[serde(default)]
    pub task_push_notification_copy: Option<AgentScenarioLlmSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCollaborationConfig {
    #[serde(default)]
    pub allowed_delegate_agent_ids: Vec<String>,
    #[serde(default)]
    pub handoff_prompt: String,
    #[serde(default = "default_shared_context_policy")]
    pub shared_context_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    #[serde(default)]
    pub trigger_condition: String,
    #[serde(default)]
    pub manual_trigger_only: bool,
    pub system_prompt: String,
    #[serde(default = "static_capability_policy")]
    pub capability_policy: AgentCapabilityPolicy,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default = "default_allowed_tool_ids")]
    pub allowed_tool_ids: Vec<String>,
    pub default_provider_id: String,
    pub default_model: String,
    pub is_builtin: bool,
    pub is_archived: bool,
    pub execution_mode: String,
    pub collaboration_config: Option<AgentCollaborationConfig>,
    pub accent_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_uri: Option<String>,
    #[serde(default)]
    pub bot_configs: HashMap<String, AgentBotConfig>,
    #[serde(default)]
    pub heartbeat_config: AgentHeartbeatConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario_llm_config: Option<AgentScenarioLlmConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_loop_config: Option<crate::agent_loop_types::AgentLoopConfig>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub summary: String,
    pub description: String,
    #[serde(default)]
    pub trigger_condition: String,
    #[serde(default)]
    pub manual_trigger_only: bool,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub capability_policy: Option<AgentCapabilityPolicy>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default = "default_allowed_tool_ids")]
    pub allowed_tool_ids: Vec<String>,
    pub default_provider_id: String,
    pub default_model: String,
    #[serde(default)]
    pub execution_mode: Option<String>,
    #[serde(default)]
    pub collaboration_config: Option<AgentCollaborationConfig>,
    #[serde(default)]
    pub accent_color: Option<String>,
    #[serde(default)]
    pub avatar_uri: Option<String>,
    #[serde(default)]
    pub bot_configs: HashMap<String, AgentBotConfig>,
    #[serde(default)]
    pub heartbeat_config: AgentHeartbeatConfig,
    #[serde(default)]
    pub scenario_llm_config: Option<AgentScenarioLlmConfig>,
    #[serde(default)]
    pub agent_loop_config: Option<crate::agent_loop_types::AgentLoopConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentBotConfig {
    pub enabled: bool,
    /// 用户点击「断开」后为 true；为 false 时应用启动会尝试自动拉起已绑定凭证的通道。
    #[serde(default)]
    pub im_channel_paused: bool,
    pub client_id: String,
    pub client_secret: String,
    pub status: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub route_tag: Option<String>,
    #[serde(default)]
    pub ai_provider_id: Option<String>,
    #[serde(default)]
    pub ai_api_format: Option<String>,
    #[serde(default)]
    pub ai_base_url: Option<String>,
    #[serde(default)]
    pub ai_api_key: Option<String>,
    #[serde(default)]
    pub ai_model: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    /// 对等入站专用：每个智能体独立密钥；也可用 `clientSecret` 字段填写（界面第二格）。
    #[serde(default)]
    pub peer_shared_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentHeartbeatTask {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_heartbeat_task_type")]
    pub task_type: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub message_template: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub working_directory: String,
    #[serde(default = "default_heartbeat_timeout_sec")]
    pub timeout_sec: i64,
    #[serde(default = "default_true")]
    pub notify_on_success: bool,
    #[serde(default = "default_true")]
    pub notify_on_failure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentHeartbeatSchedule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub task_id: String,
    #[serde(default = "default_heartbeat_schedule_type")]
    pub schedule_type: String,
    #[serde(default)]
    pub times: Vec<String>,
    #[serde(default = "default_heartbeat_channel_id")]
    pub channel_id: String,
    #[serde(default)]
    pub target_user_id: String,
    #[serde(default)]
    pub target_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentHeartbeatConfig {
    #[serde(default = "default_heartbeat_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub tasks: Vec<AgentHeartbeatTask>,
    #[serde(default)]
    pub schedules: Vec<AgentHeartbeatSchedule>,
}

impl Default for AgentHeartbeatConfig {
    fn default() -> Self {
        Self {
            timezone: default_heartbeat_timezone(),
            tasks: Vec::new(),
            schedules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAgentConfig {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    #[serde(default)]
    pub trigger_condition: String,
    #[serde(default)]
    pub manual_trigger_only: bool,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default = "static_capability_policy")]
    pub capability_policy: AgentCapabilityPolicy,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default = "default_allowed_tool_ids")]
    pub allowed_tool_ids: Vec<String>,
    #[serde(default)]
    pub default_provider_id: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default = "default_execution_mode")]
    pub execution_mode: String,
    #[serde(default)]
    pub collaboration_config: Option<AgentCollaborationConfig>,
    #[serde(default)]
    pub accent_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario_llm_config: Option<AgentScenarioLlmConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_loop_config: Option<crate::agent_loop_types::AgentLoopConfig>,
}

#[derive(Debug, Clone)]
struct BuiltinAgentSeed {
    id: &'static str,
    name: &'static str,
    summary: &'static str,
    description: &'static str,
    system_prompt: &'static str,
    default_provider_id: &'static str,
    default_model: &'static str,
    accent_color: &'static str,
}

pub fn default_allowed_tool_ids() -> Vec<String> {
    DEFAULT_ALLOWED_TOOL_IDS
        .iter()
        .map(|item| item.to_string())
        .collect()
}

pub fn normalize_allowed_tool_ids(tool_ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for tool_id in tool_ids {
        let Some(canonical) = canonical_allowed_tool_id(&tool_id) else {
            continue;
        };
        if seen.insert(canonical.clone()) {
            normalized.push(canonical);
        }
    }
    normalized
}

pub fn runtime_tool_names_for_allowed_tool_ids(tool_ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tools = Vec::new();
    for tool_id in tool_ids {
        let runtime_name = match tool_id.trim() {
            "read_file" => "read",
            "write_file" => "write",
            "edit_file" => "edit",
            "list_dir" => "ls",
            "glob" => "find",
            "agent_spawn" => "agent_delegate",
            "external_api" => "nineclaw_external_api",
            value => value,
        };
        let runtime_name = runtime_name.trim();
        if !runtime_name.is_empty() && seen.insert(runtime_name.to_string()) {
            tools.push(runtime_name.to_string());
        }
    }
    tools
}

fn canonical_allowed_tool_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let canonical = match trimmed {
        "read" => "read_file",
        "write" => "write_file",
        "edit" => "edit_file",
        "ls" => "list_dir",
        "find" => "glob",
        "agent_delegate" => "agent_spawn",
        "nineclaw_external_api" => "external_api",
        other => other,
    };
    if !DEFAULT_ALLOWED_TOOL_IDS.contains(&canonical) {
        return None;
    }
    Some(canonical.to_string())
}

pub(crate) fn ensure_agents_ready_conn(connection: &Connection) -> Result<(), String> {
    ensure_agents_ready(connection)
}

pub(crate) fn fetch_active_agent(
    connection: &Connection,
    agent_id: &str,
) -> Result<Option<AgentRecord>, String> {
    get_active_agent_by_id(connection, agent_id)
}

pub(crate) fn sync_agent_workspaces_logged(connection: &Connection, context: &str) {
    try_sync_active_agent_workspaces(connection, context);
}

pub fn list_agents(app: &AppHandle) -> Result<Vec<AgentRecord>, String> {
    let connection = crate::open_history_db(app)?;
    list_agents_with_connection(&connection)
}

pub fn get_default_agent(app: &AppHandle) -> Result<Option<AgentRecord>, String> {
    let connection = crate::open_history_db(app)?;
    get_default_agent_with_connection(&connection)
}

pub fn create_agent(app: &AppHandle, payload: AgentInput) -> Result<AgentRecord, String> {
    let mut connection = crate::open_history_db(app)?;
    create_agent_with_connection(&mut connection, payload)
}

pub fn update_agent(
    app: &AppHandle,
    agent_id: String,
    payload: AgentInput,
) -> Result<AgentRecord, String> {
    let mut connection = crate::open_history_db(app)?;
    update_agent_with_connection(&mut connection, &agent_id, payload)
}

pub fn archive_agent(app: &AppHandle, agent_id: String) -> Result<(), String> {
    let mut connection = crate::open_history_db(app)?;
    archive_agent_with_connection(&mut connection, &agent_id)
}

pub fn delete_agent(app: &AppHandle, agent_id: String) -> Result<(), String> {
    let mut connection = crate::open_history_db(app)?;
    delete_agent_with_connection(&mut connection, &agent_id)
}

pub fn set_default_agent(app: &AppHandle, agent_id: String) -> Result<Option<AgentRecord>, String> {
    let connection = crate::open_history_db(app)?;
    set_default_agent_with_connection(&connection, &agent_id)
}

/// 读取智能体完整记录（含 `bot_configs`），供对等网关等按智能体解析密钥。
pub fn get_agent_record(app: &AppHandle, agent_id: &str) -> Result<Option<AgentRecord>, String> {
    let connection = crate::open_history_db(app)?;
    ensure_agents_ready(&connection)?;
    get_active_agent_by_id(&connection, agent_id)
}

pub fn get_conversation_agent_config(
    app: &AppHandle,
    agent_id: &str,
) -> Result<Option<ConversationAgentConfig>, String> {
    let connection = crate::open_history_db(app)?;
    ensure_agents_ready(&connection)?;
    let Some(record) = get_active_agent_by_id(&connection, agent_id)? else {
        return Ok(None);
    };

    Ok(Some(ConversationAgentConfig {
        id: record.id,
        name: record.name,
        summary: record.summary,
        description: record.description,
        trigger_condition: record.trigger_condition,
        manual_trigger_only: record.manual_trigger_only,
        system_prompt: record.system_prompt,
        capability_policy: record.capability_policy,
        skill_ids: record.skill_ids,
        allowed_tool_ids: record.allowed_tool_ids,
        default_provider_id: record.default_provider_id,
        default_model: record.default_model,
        execution_mode: record.execution_mode,
        collaboration_config: record.collaboration_config,
        accent_color: record.accent_color,
        avatar_uri: record.avatar_uri,
        scenario_llm_config: record.scenario_llm_config.clone(),
        agent_loop_config: record.agent_loop_config.clone(),
    }))
}

pub fn read_agent_workspace_bundle(
    app: &AppHandle,
    agent_id: String,
) -> Result<agent_workspace::AgentWorkspaceBundle, String> {
    let connection = crate::open_history_db(app)?;
    ensure_agents_ready(&connection)?;

    let Some(record) = get_active_agent_by_id(&connection, &agent_id)? else {
        return Err("要查看的智能体不存在".to_string());
    };

    try_ensure_workspace_for_record(&record, false, "读取智能体工作区");
    agent_workspace::read_agent_workspace_bundle(&agent_id)
}

pub fn read_agent_workspace_file(
    app: &AppHandle,
    agent_id: String,
    relative_path: String,
) -> Result<agent_workspace::AgentWorkspaceFile, String> {
    let connection = crate::open_history_db(app)?;
    ensure_agents_ready(&connection)?;

    let Some(record) = get_active_agent_by_id(&connection, &agent_id)? else {
        return Err("要查看的智能体不存在".to_string());
    };

    try_ensure_workspace_for_record(&record, false, "读取智能体工作区文件");
    agent_workspace::read_agent_workspace_file(&agent_id, &relative_path)
}

pub fn write_agent_workspace_file(
    app: &AppHandle,
    agent_id: String,
    relative_path: String,
    content: String,
) -> Result<agent_workspace::AgentWorkspaceBundle, String> {
    let connection = crate::open_history_db(app)?;
    ensure_agents_ready(&connection)?;

    let Some(record) = get_active_agent_by_id(&connection, &agent_id)? else {
        return Err("要写入的智能体不存在".to_string());
    };

    try_ensure_workspace_for_record(&record, false, "写入智能体工作区");
    agent_workspace::write_agent_workspace_file(&agent_id, &relative_path, &content)
}

#[allow(dead_code)]
pub fn build_agent_system_prompt(agent: &ConversationAgentConfig) -> Option<String> {
    build_agent_system_prompt_for_prompt(agent, None)
}

fn trim_prompt_snippet(value: &str, limit: usize) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || limit == 0 {
        return String::new();
    }

    let snippet: String = trimmed.chars().take(limit).collect();
    if trimmed.chars().count() > limit {
        format!("{snippet}…")
    } else {
        snippet
    }
}

pub fn build_agent_system_prompt_for_prompt(
    agent: &ConversationAgentConfig,
    current_prompt: Option<&str>,
) -> Option<String> {
    let mut sections = Vec::new();

    let name = agent.name.trim();
    if !name.is_empty() {
        sections.push(format!("身份：{}", trim_prompt_snippet(name, 80)));
    }

    let summary = agent.summary.trim();
    if !summary.is_empty() {
        sections.push(format!("简介：{}", trim_prompt_snippet(summary, 160)));
    } else {
        let description = agent.description.trim();
        if !description.is_empty() {
            sections.push(format!("简介：{}", trim_prompt_snippet(description, 160)));
        }
    }

    let trigger_condition = agent.trigger_condition.trim();
    if !trigger_condition.is_empty() {
        sections.push(format!(
            "触发条件：{}",
            trim_prompt_snippet(trigger_condition, 240)
        ));
    }
    if agent.manual_trigger_only {
        sections.push("触发限制：禁止模型自动调用；仅允许用户手动触发。".to_string());
    }

    sections.push(format!(
        "能力策略：{}",
        agent.capability_policy.strategy.trim()
    ));
    sections.push(
        "工具使用：如果要读取网页的内容，必须使用 web_fetch 工具；web_search 只用于搜索和发现网页。"
            .to_string(),
    );

    if !agent.skill_ids.is_empty() {
        let listed = agent
            .skill_ids
            .iter()
            .take(4)
            .map(|item| item.as_str())
            .collect::<Vec<_>>()
            .join("、");
        let suffix = if agent.skill_ids.len() > 4 {
            format!(" 等{}个", agent.skill_ids.len())
        } else {
            String::new()
        };
            sections.push(format!("偏好技能：{}{}", listed, suffix));
    }

    if !agent.allowed_tool_ids.is_empty() {
        sections.push(format!(
            "允许工具：{}。严禁调用未列入允许工具的工具。",
            agent.allowed_tool_ids.join("、")
        ));
    }

    let system_prompt = agent.system_prompt.trim();
    if !system_prompt.is_empty() {
        let expanded_prompt = current_prompt
            .map(|arg| system_prompt.replace("${ARG}", arg.trim()))
            .unwrap_or_else(|| system_prompt.to_string());
        let body = if expanded_prompt.chars().count() > MAX_USER_SYSTEM_INSTRUCTION_CHARS {
            let head: String = expanded_prompt
                .chars()
                .take(MAX_USER_SYSTEM_INSTRUCTION_CHARS)
                .collect();
            format!("{head}…\n(已截断至约 {MAX_USER_SYSTEM_INSTRUCTION_CHARS} 字，见仓库文档 docs/AGENT_SYSTEM_PROMPT.md)")
        } else {
            expanded_prompt
        };
        sections.push(format!(
            "最高优先级执行要求（MUST）：以下是用户为当前智能体配置的专属系统提示词。除非违反平台安全或系统级限制，否则必须严格遵守；若与下面的通用能力说明冲突，以本段为准。\n{body}"
        ));
    }

    if agent_workspace::runtime_sync_enabled() {
        if let Ok(workspace_prompt) =
            agent_workspace::build_workspace_system_prompt_for_query(&agent.id, current_prompt)
        {
            sections.push(workspace_prompt);
        }
    }

    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n"))
    }
}

fn ensure_agents_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                summary TEXT NOT NULL,
                description TEXT NOT NULL,
                trigger_condition TEXT NOT NULL DEFAULT '',
                manual_trigger_only INTEGER NOT NULL DEFAULT 0,
                system_prompt TEXT NOT NULL DEFAULT '',
                default_provider_id TEXT NOT NULL,
                default_model TEXT NOT NULL,
                is_builtin INTEGER NOT NULL DEFAULT 0,
                is_archived INTEGER NOT NULL DEFAULT 0,
                execution_mode TEXT NOT NULL DEFAULT 'single',
                collaboration_config_json TEXT,
                heartbeat_config_json TEXT,
                capability_policy_json TEXT,
                allowed_tool_ids_json TEXT,
                accent_color TEXT,
                avatar_uri TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_skills (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                skill_id TEXT NOT NULL,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                UNIQUE(agent_id, skill_id),
                FOREIGN KEY(agent_id) REFERENCES agents(id)
            );
            CREATE INDEX IF NOT EXISTS idx_agent_skills_agent_id ON agent_skills(agent_id);
            CREATE TABLE IF NOT EXISTS agent_bot_bindings (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                channel_id TEXT NOT NULL,
                config_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(agent_id, channel_id),
                FOREIGN KEY(agent_id) REFERENCES agents(id)
            );
            CREATE INDEX IF NOT EXISTS idx_agent_bot_bindings_agent_id ON agent_bot_bindings(agent_id);
            CREATE INDEX IF NOT EXISTS idx_agents_active_name ON agents(is_archived, name);",
        )
        .map_err(|error| format!("初始化智能体数据表失败: {error}"))?;

    add_agents_column_if_missing(connection, "heartbeat_config_json", "TEXT")?;
    add_agents_column_if_missing(connection, "scenario_llm_config_json", "TEXT")?;
    add_agents_column_if_missing(connection, "agent_loop_config_json", "TEXT")?;
    add_agents_column_if_missing(connection, "capability_policy_json", "TEXT")?;
    add_agents_column_if_missing(connection, "allowed_tool_ids_json", "TEXT")?;
    add_agents_column_if_missing(connection, "trigger_condition", "TEXT NOT NULL DEFAULT ''")?;
    add_agents_column_if_missing(connection, "manual_trigger_only", "INTEGER NOT NULL DEFAULT 0")?;
    add_agents_column_if_missing(connection, "avatar_uri", "TEXT")?;
    crate::heartbeat::ensure_heartbeat_schema(connection)?;

    Ok(())
}

fn add_agents_column_if_missing(
    connection: &Connection,
    column_name: &str,
    column_definition: &str,
) -> Result<(), String> {
    let pragma = format!("PRAGMA table_info(agents)");
    let mut statement = connection
        .prepare(&pragma)
        .map_err(|error| format!("读取 agents 表结构失败: {error}"))?;
    let existing = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("解析 agents 表结构失败: {error}"))?
        .filter_map(Result::ok)
        .any(|name| name == column_name);

    if existing {
        return Ok(());
    }

    connection
        .execute(
            &format!("ALTER TABLE agents ADD COLUMN {column_name} {column_definition}"),
            [],
        )
        .map_err(|error| format!("补充 agents.{column_name} 失败: {error}"))?;

    Ok(())
}

fn ensure_agents_ready(connection: &Connection) -> Result<(), String> {
    crate::ensure_app_state_schema(connection)?;
    ensure_agents_schema(connection)?;
    seed_builtin_agents(connection)?;
    try_sync_active_agent_workspaces(connection, "初始化智能体工作区");
    Ok(())
}

fn seed_builtin_agents(connection: &Connection) -> Result<(), String> {
    for seed in builtin_agent_seeds() {
        let capability_policy_json =
            serialize_capability_policy(&new_agent_default_capability_policy())?;
        connection
            .execute(
                "INSERT OR IGNORE INTO agents (
                id, name, summary, description, trigger_condition, manual_trigger_only, system_prompt, default_provider_id,
                default_model, is_builtin, is_archived, execution_mode,
                collaboration_config_json, heartbeat_config_json, capability_policy_json, allowed_tool_ids_json, accent_color, scenario_llm_config_json, agent_loop_config_json, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, '', 0, ?5, ?6, ?7, 1, 0, 'single', NULL, NULL, ?8, ?9, ?10, NULL, NULL, ?11, ?11)",
                params![
                    seed.id,
                    seed.name,
                    seed.summary,
                    seed.description,
                    seed.system_prompt,
                    seed.default_provider_id,
                    seed.default_model,
                    capability_policy_json,
                    serialize_allowed_tool_ids(&default_allowed_tool_ids())?,
                    seed.accent_color,
                    1_735_689_600_000_i64,
                ],
            )
            .map_err(|error| format!("写入内置智能体失败: {error}"))?;
    }

    Ok(())
}

fn builtin_agent_seeds() -> Vec<BuiltinAgentSeed> {
    vec![
        BuiltinAgentSeed {
            id: "general-assistant",
            name: "全场景个人助理",
            summary: "默认总入口，适合承接综合任务、信息整理和执行编排。",
            description:
                "负责理解任务目标、拆解执行步骤、调用合适技能，并在必要时提示风险与确认点。",
            system_prompt: "优先把任务讲清楚、拆明白、做扎实；对不确定信息先确认，不要凭空假设。",
            default_provider_id: "openai",
            default_model: "gpt-4.1",
            accent_color: "#7C5CFA",
        }, // BuiltinAgentSeed {
           //     id: "labor-agent",
           //     name: "劳动人事专职律师助理",
           //     summary: "聚焦劳动争议、人事合规、赔偿测算和办案资料准备。",
           //     description: "适合处理劳动争议案件梳理、证据清单补强、仲裁诉讼流程说明以及人事制度核查。",
           //     system_prompt:
           //         "输出要强调证据链、赔偿计算逻辑和程序节点，遇到关键事实缺口时先提醒用户补充。",
           //     default_provider_id: "openai",
           //     default_model: "gpt-4.1",
           //     accent_color: "#F59E0B",
           // },
           // BuiltinAgentSeed {
           //     id: "litigation-agent",
           //     name: "民商事诉讼专职律师助理",
           //     summary: "聚焦民商事诉讼，擅长案情梳理、文书组织和类案检索辅助。",
           //     description: "适合做案情结构化、争点提炼、文书草拟、证据目录整理和诉讼方案预演。",
           //     system_prompt:
           //         "优先按事实、争点、证据、法律依据和诉讼策略的顺序组织回答，避免空泛表述。",
           //     default_provider_id: "openai",
           //     default_model: "gpt-4.1",
           //     accent_color: "#8B5CF6",
           // },
           // BuiltinAgentSeed {
           //     id: "marketing-agent",
           //     name: "品牌营销专员",
           //     summary: "负责内容选题、品牌表达、活动策划和律师 IP 增长支持。",
           //     description: "适合沉淀内容日历、活动传播方案、品牌语气规范和转化导向的营销文案。",
           //     system_prompt:
           //         "输出要兼顾品牌一致性和实际转化目标，避免空洞口号，尽量给到可执行动作。",
           //     default_provider_id: "openai",
           //     default_model: "gpt-4.1",
           //     accent_color: "#EC4899",
           // },
           // BuiltinAgentSeed {
           //     id: "compliance-agent",
           //     name: "企业合规专职律师助理",
           //     summary: "聚焦合同审查、流程合规、制度文档和业务风险识别。",
           //     description: "适合协助梳理业务流程风险点、输出合规检查清单、制度模板和合同审查意见。",
           //     system_prompt:
           //         "回答要强调边界、责任、审批节点和留痕要求，对高风险事项要明确升级建议。",
           //     default_provider_id: "openai",
           //     default_model: "gpt-4.1",
           //     accent_color: "#10B981",
           // },
           // BuiltinAgentSeed {
           //     id: "ip-agent",
           //     name: "知识产权专职律师助理",
           //     summary: "聚焦专利、商标、著作权和侵权比对场景。",
           //     description: "适合做检索策略建议、权利基础梳理、侵权比对框架和知产申报准备。",
           //     system_prompt:
           //         "输出要严格区分权利类型、保护范围和侵权判断要素，不要混淆不同知产路径。",
           //     default_provider_id: "openai",
           //     default_model: "gpt-4.1",
           //     accent_color: "#6366F1",
           // },
           // BuiltinAgentSeed {
           //     id: "capital-agent",
           //     name: "资本市场非诉律师助理",
           //     summary: "聚焦尽调、底稿整理、信息披露和资本市场项目支持。",
           //     description: "适合协助尽职调查、项目资料编排、问题清单整理和底稿结构化沉淀。",
           //     system_prompt:
           //         "输出要强调底稿完整性、披露口径一致性和项目时间线，不确定事项先标红提示。",
           //     default_provider_id: "openai",
           //     default_model: "gpt-4.1",
           //     accent_color: "#F97316",
           // },
    ]
}

fn list_agents_with_connection(connection: &Connection) -> Result<Vec<AgentRecord>, String> {
    ensure_agents_ready(connection)?;

    let mut statement = connection
        .prepare(
            "SELECT
                id,
                name,
                summary,
                description,
                trigger_condition,
                manual_trigger_only,
                system_prompt,
                default_provider_id,
                default_model,
                is_builtin,
                is_archived,
                execution_mode,
                collaboration_config_json,
                heartbeat_config_json,
                allowed_tool_ids_json,
                accent_color,
                avatar_uri,
                scenario_llm_config_json,
                agent_loop_config_json,
                created_at,
                updated_at
            FROM agents
            WHERE is_archived = 0
            ORDER BY
                CASE WHEN id = 'general-assistant' THEN 0 ELSE 1 END,
                is_builtin DESC,
                lower(name) ASC,
                updated_at DESC",
        )
        .map_err(|error| format!("读取智能体列表失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(AgentRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                summary: row.get(2)?,
                description: row.get(3)?,
                trigger_condition: row.get(4)?,
                manual_trigger_only: row.get::<_, i64>(5)? != 0,
                system_prompt: row.get(6)?,
                capability_policy: static_capability_policy(),
                skill_ids: Vec::new(),
                default_provider_id: row.get(7)?,
                default_model: row.get(8)?,
                is_builtin: row.get::<_, i64>(9)? != 0,
                is_archived: row.get::<_, i64>(10)? != 0,
                execution_mode: row.get(11)?,
                collaboration_config: deserialize_collaboration_config(row.get(12)?),
                heartbeat_config: deserialize_heartbeat_config(row.get(13)?),
                allowed_tool_ids: deserialize_allowed_tool_ids(row.get(14)?),
                accent_color: row.get(15)?,
                avatar_uri: row.get(16)?,
                scenario_llm_config: deserialize_scenario_llm_config(row.get(17)?),
                agent_loop_config: deserialize_agent_loop_config(row.get(18)?),
                bot_configs: HashMap::new(),
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .map_err(|error| format!("解析智能体列表失败: {error}"))?;

    let mut agents = Vec::new();
    for row in rows {
        let mut agent = row.map_err(|error| format!("读取智能体数据失败: {error}"))?;
        agent.skill_ids = load_skill_ids_for_agent(connection, &agent.id)?;
        agent.capability_policy = load_capability_policy_for_agent(
            connection,
            &agent.id,
            &agent.execution_mode,
            &agent.skill_ids,
            agent.is_builtin,
        )?;
        agent.bot_configs = load_bot_configs_for_agent(connection, &agent.id)?;
        agents.push(agent);
    }

    try_sync_agent_registry_from_records(&agents, "同步智能体注册表");

    Ok(agents)
}

fn get_default_agent_with_connection(
    connection: &Connection,
) -> Result<Option<AgentRecord>, String> {
    ensure_agents_ready(connection)?;

    let stored_default_id = connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![DEFAULT_AGENT_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取默认智能体失败: {error}"))?;

    if let Some(agent_id) = stored_default_id {
        if let Some(agent) = get_active_agent_by_id(connection, &agent_id)? {
            return Ok(Some(agent));
        }
    }

    let fallback = find_fallback_default_agent(connection)?;
    if let Some(agent) = fallback.clone() {
        persist_default_agent_id(connection, &agent.id)?;
    }

    Ok(fallback)
}

fn find_fallback_default_agent(connection: &Connection) -> Result<Option<AgentRecord>, String> {
    if let Some(agent) = get_active_agent_by_id(connection, "general-assistant")? {
        return Ok(Some(agent));
    }

    list_agents_with_connection(connection).map(|agents| agents.into_iter().next())
}

fn get_active_agent_by_id(
    connection: &Connection,
    agent_id: &str,
) -> Result<Option<AgentRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT
                id,
                name,
                summary,
                description,
                trigger_condition,
                manual_trigger_only,
                system_prompt,
                default_provider_id,
                default_model,
                is_builtin,
                is_archived,
                execution_mode,
                collaboration_config_json,
                heartbeat_config_json,
                allowed_tool_ids_json,
                accent_color,
                avatar_uri,
                scenario_llm_config_json,
                agent_loop_config_json,
                created_at,
                updated_at
            FROM agents
            WHERE id = ?1 AND is_archived = 0",
        )
        .map_err(|error| format!("准备读取智能体失败: {error}"))?;

    let mut agent = statement
        .query_row(params![agent_id], |row| {
            Ok(AgentRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                summary: row.get(2)?,
                description: row.get(3)?,
                trigger_condition: row.get(4)?,
                manual_trigger_only: row.get::<_, i64>(5)? != 0,
                system_prompt: row.get(6)?,
                capability_policy: static_capability_policy(),
                skill_ids: Vec::new(),
                default_provider_id: row.get(7)?,
                default_model: row.get(8)?,
                is_builtin: row.get::<_, i64>(9)? != 0,
                is_archived: row.get::<_, i64>(10)? != 0,
                execution_mode: row.get(11)?,
                collaboration_config: deserialize_collaboration_config(row.get(12)?),
                heartbeat_config: deserialize_heartbeat_config(row.get(13)?),
                allowed_tool_ids: deserialize_allowed_tool_ids(row.get(14)?),
                accent_color: row.get(15)?,
                avatar_uri: row.get(16)?,
                scenario_llm_config: deserialize_scenario_llm_config(row.get(17)?),
                agent_loop_config: deserialize_agent_loop_config(row.get(18)?),
                bot_configs: HashMap::new(),
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .optional()
        .map_err(|error| format!("读取智能体失败: {error}"))?;

    if let Some(ref mut record) = agent {
        record.skill_ids = load_skill_ids_for_agent(connection, &record.id)?;
        record.capability_policy = load_capability_policy_for_agent(
            connection,
            &record.id,
            &record.execution_mode,
            &record.skill_ids,
            record.is_builtin,
        )?;
        record.bot_configs = load_bot_configs_for_agent(connection, &record.id)?;
    }

    Ok(agent)
}

fn load_skill_ids_for_agent(
    connection: &Connection,
    agent_id: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT skill_id
            FROM agent_skills
            WHERE agent_id = ?1
            ORDER BY sort_order ASC, created_at ASC",
        )
        .map_err(|error| format!("读取智能体技能失败: {error}"))?;

    let rows = statement
        .query_map(params![agent_id], |row| row.get::<_, String>(0))
        .map_err(|error| format!("解析智能体技能失败: {error}"))?;

    let mut skill_ids = Vec::new();
    for row in rows {
        skill_ids.push(row.map_err(|error| format!("读取技能绑定失败: {error}"))?);
    }
    Ok(skill_ids)
}

fn load_capability_policy_for_agent(
    connection: &Connection,
    agent_id: &str,
    execution_mode: &str,
    skill_ids: &[String],
    is_builtin: bool,
) -> Result<AgentCapabilityPolicy, String> {
    let raw = connection
        .query_row(
            "SELECT capability_policy_json FROM agents WHERE id = ?1",
            params![agent_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| format!("读取智能体能力策略失败: {error}"))?
        .flatten();

    Ok(deserialize_capability_policy(
        raw,
        execution_mode,
        skill_ids,
        is_builtin,
    ))
}

fn load_bot_configs_for_agent(
    connection: &Connection,
    agent_id: &str,
) -> Result<HashMap<String, AgentBotConfig>, String> {
    let mut statement = connection
        .prepare(
            "SELECT channel_id, config_json
            FROM agent_bot_bindings
            WHERE agent_id = ?1
            ORDER BY channel_id ASC",
        )
        .map_err(|error| format!("读取智能体机器人绑定失败: {error}"))?;

    let rows = statement
        .query_map(params![agent_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("解析智能体机器人绑定失败: {error}"))?;

    let mut configs = HashMap::new();
    for row in rows {
        let (channel_id, raw) = row.map_err(|error| format!("读取机器人绑定数据失败: {error}"))?;
        let config = deserialize_bot_config(&raw)?;
        configs.insert(channel_id, config);
    }
    Ok(configs)
}

fn create_agent_with_connection(
    connection: &mut Connection,
    payload: AgentInput,
) -> Result<AgentRecord, String> {
    ensure_agents_ready(connection)?;
    let normalized = normalize_agent_input(payload)?;
    let agent_id = normalized
        .id
        .clone()
        .unwrap_or_else(|| format!("agent_{}", Uuid::new_v4().simple()));
    let workspace_seed = AgentWorkspaceSeed {
        id: agent_id.as_str(),
        name: normalized.name.as_str(),
        summary: normalized.summary.as_str(),
        description: normalized.description.as_str(),
        accent_color: normalized.accent_color.as_deref(),
        is_builtin: false,
    };
    let now = crate::chrono_like_timestamp();
    let collaboration_json =
        serialize_collaboration_config(normalized.collaboration_config.as_ref())?;
    let heartbeat_json = serialize_heartbeat_config(&normalized.heartbeat_config)?;
    let scenario_json = serialize_scenario_llm_config(&normalized.scenario_llm_config)?;
    let agent_loop_json = serialize_agent_loop_config(&normalized.agent_loop_config)?;
    let capability_policy_json = serialize_capability_policy(&normalized.capability_policy)?;
    let allowed_tool_ids_json = serialize_allowed_tool_ids(&normalized.allowed_tool_ids)?;

    let transaction = connection
        .transaction()
        .map_err(|error| format!("创建智能体事务失败: {error}"))?;

    transaction
        .execute(
            "INSERT INTO agents (
                id, name, summary, description, trigger_condition, manual_trigger_only, system_prompt, default_provider_id,
                default_model, is_builtin, is_archived, execution_mode,
                collaboration_config_json, heartbeat_config_json, capability_policy_json, allowed_tool_ids_json, accent_color, avatar_uri, scenario_llm_config_json, agent_loop_config_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 0, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19)",
            params![
                agent_id.as_str(),
                normalized.name.as_str(),
                normalized.summary.as_str(),
                normalized.description.as_str(),
                normalized.trigger_condition.as_str(),
                if normalized.manual_trigger_only { 1_i64 } else { 0_i64 },
                normalized.system_prompt.as_str(),
                normalized.default_provider_id.as_str(),
                normalized.default_model.as_str(),
                normalized.execution_mode.as_str(),
                collaboration_json,
                heartbeat_json,
                capability_policy_json,
                allowed_tool_ids_json,
                normalized.accent_color.as_deref(),
                normalized.avatar_uri.as_deref(),
                scenario_json,
                agent_loop_json,
                now,
            ],
        )
        .map_err(|error| format!("写入智能体失败: {error}"))?;

    replace_agent_skills(&transaction, &agent_id, &normalized.skill_ids, now)?;
    let mut bot_configs = normalized.bot_configs;
    apply_peer_inbound_defaults(&mut bot_configs, None);
    replace_agent_bot_bindings(&transaction, &agent_id, &bot_configs, now)?;
    if agent_workspace::runtime_sync_enabled() {
        agent_workspace::ensure_agent_workspace(workspace_seed, true)
            .map_err(|error| format!("创建智能体工作区失败: {error}"))?;
    }
    transaction
        .commit()
        .map_err(|error| format!("提交智能体事务失败: {error}"))?;

    let record = get_active_agent_by_id(connection, &agent_id)?
        .ok_or_else(|| "创建智能体后读取结果失败".to_string())?;
    try_sync_active_agent_workspaces(connection, "创建后同步智能体工作区");
    Ok(record)
}

fn update_agent_with_connection(
    connection: &mut Connection,
    agent_id: &str,
    payload: AgentInput,
) -> Result<AgentRecord, String> {
    ensure_agents_ready(connection)?;
    let existing = get_active_agent_by_id(connection, agent_id)?
        .ok_or_else(|| "要更新的智能体不存在".to_string())?;

    let normalized = normalize_agent_input(payload)?;
    let next_agent_id = normalized
        .id
        .clone()
        .unwrap_or_else(|| agent_id.to_string());
    if next_agent_id.as_str() != agent_id {
        let conflict = connection
            .query_row(
                "SELECT 1 FROM agents WHERE id = ?1",
                params![next_agent_id.as_str()],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| format!("检查 Agent_ID 是否可用失败: {error}"))?;
        if conflict.is_some() {
            return Err("Agent_ID 已存在，请换一个。".to_string());
        }
    }
    let now = crate::chrono_like_timestamp();
    let collaboration_json =
        serialize_collaboration_config(normalized.collaboration_config.as_ref())?;
    let heartbeat_json = serialize_heartbeat_config(&normalized.heartbeat_config)?;
    let scenario_json = serialize_scenario_llm_config(&normalized.scenario_llm_config)?;
    let agent_loop_json = serialize_agent_loop_config(&normalized.agent_loop_config)?;
    let capability_policy_json = serialize_capability_policy(&normalized.capability_policy)?;
    let allowed_tool_ids_json = serialize_allowed_tool_ids(&normalized.allowed_tool_ids)?;

    let transaction = connection
        .transaction()
        .map_err(|error| format!("更新智能体事务失败: {error}"))?;
    if next_agent_id.as_str() != agent_id {
        transaction
            .execute_batch("PRAGMA defer_foreign_keys = ON;")
            .map_err(|error| format!("启用 Agent_ID 更新事务约束延迟失败: {error}"))?;
    }

    transaction
        .execute(
            "UPDATE agents
            SET
                id = ?2,
                name = ?3,
                summary = ?4,
                description = ?5,
                trigger_condition = ?6,
                manual_trigger_only = ?7,
                system_prompt = ?8,
                default_provider_id = ?9,
                default_model = ?10,
                execution_mode = ?11,
                collaboration_config_json = ?12,
                heartbeat_config_json = ?13,
                capability_policy_json = ?14,
                allowed_tool_ids_json = ?15,
                accent_color = ?16,
                avatar_uri = ?17,
                scenario_llm_config_json = ?18,
                agent_loop_config_json = ?19,
                updated_at = ?20
            WHERE id = ?1 AND is_archived = 0",
            params![
                agent_id,
                next_agent_id.as_str(),
                normalized.name,
                normalized.summary,
                normalized.description,
                normalized.trigger_condition,
                if normalized.manual_trigger_only { 1_i64 } else { 0_i64 },
                normalized.system_prompt,
                normalized.default_provider_id,
                normalized.default_model,
                normalized.execution_mode,
                collaboration_json,
                heartbeat_json,
                capability_policy_json,
                allowed_tool_ids_json,
                normalized.accent_color,
                normalized.avatar_uri,
                scenario_json,
                agent_loop_json,
                now,
            ],
        )
        .map_err(|error| format!("更新智能体失败: {error}"))?;

    if next_agent_id.as_str() != agent_id {
        rename_agent_references(&transaction, agent_id, &next_agent_id)?;
    }
    replace_agent_skills(&transaction, &next_agent_id, &normalized.skill_ids, now)?;
    let mut bot_configs = normalized.bot_configs;
    apply_peer_inbound_defaults(&mut bot_configs, existing.bot_configs.get("peer"));
    replace_agent_bot_bindings(&transaction, &next_agent_id, &bot_configs, now)?;
    transaction
        .commit()
        .map_err(|error| format!("提交智能体更新失败: {error}"))?;

    let record = get_active_agent_by_id(connection, &next_agent_id)?
        .ok_or_else(|| "更新智能体后读取结果失败".to_string())?;
    try_ensure_workspace_for_record(&record, false, "更新智能体工作区");
    try_sync_active_agent_workspaces(connection, "更新后同步智能体工作区");
    Ok(record)
}

fn archive_agent_with_connection(
    connection: &mut Connection,
    agent_id: &str,
) -> Result<(), String> {
    ensure_agents_ready(connection)?;
    let now = crate::chrono_like_timestamp();
    connection
        .execute(
            "UPDATE agents SET is_archived = 1, updated_at = ?2 WHERE id = ?1 AND is_archived = 0",
            params![agent_id, now],
        )
        .map_err(|error| format!("归档智能体失败: {error}"))?;

    let current_default = connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![DEFAULT_AGENT_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取默认智能体失败: {error}"))?;

    if current_default.as_deref() == Some(agent_id) {
        if let Some(next_default) = find_fallback_default_agent(connection)? {
            persist_default_agent_id(connection, &next_default.id)?;
        } else {
            connection
                .execute(
                    "DELETE FROM app_state WHERE key = ?1",
                    params![DEFAULT_AGENT_STATE_KEY],
                )
                .map_err(|error| format!("清理默认智能体失败: {error}"))?;
        }
    }

    try_sync_active_agent_workspaces(connection, "归档后同步智能体工作区");

    Ok(())
}

fn delete_agent_with_connection(connection: &mut Connection, agent_id: &str) -> Result<(), String> {
    ensure_agents_ready(connection)?;
    let Some(agent) = get_active_agent_by_id(connection, agent_id)? else {
        return Err("要删除的智能体不存在".to_string());
    };
    if agent.is_builtin {
        return Err("内置智能体不允许删除".to_string());
    }

    let transaction = connection
        .transaction()
        .map_err(|error| format!("删除智能体事务失败: {error}"))?;

    transaction
        .execute(
            "DELETE FROM agent_bot_bindings WHERE agent_id = ?1",
            params![agent_id],
        )
        .map_err(|error| format!("删除机器人绑定失败: {error}"))?;
    transaction
        .execute(
            "DELETE FROM agent_heartbeat_runs WHERE agent_id = ?1",
            params![agent_id],
        )
        .map_err(|error| format!("删除心跳任务运行记录失败: {error}"))?;
    transaction
        .execute(
            "DELETE FROM agent_skills WHERE agent_id = ?1",
            params![agent_id],
        )
        .map_err(|error| format!("删除智能体技能绑定失败: {error}"))?;
    transaction
        .execute("DELETE FROM agents WHERE id = ?1", params![agent_id])
        .map_err(|error| format!("删除智能体失败: {error}"))?;

    let current_default = transaction
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![DEFAULT_AGENT_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取默认智能体失败: {error}"))?;

    if current_default.as_deref() == Some(agent_id) {
        if let Some(next_default) = find_fallback_default_agent(&transaction)? {
            persist_default_agent_id(&transaction, &next_default.id)?;
        } else {
            transaction
                .execute(
                    "DELETE FROM app_state WHERE key = ?1",
                    params![DEFAULT_AGENT_STATE_KEY],
                )
                .map_err(|error| format!("清理默认智能体失败: {error}"))?;
        }
    }

    transaction
        .commit()
        .map_err(|error| format!("提交删除智能体事务失败: {error}"))?;

    let _ = agent_workspace::delete_agent_workspace(agent_id);
    try_sync_active_agent_workspaces(connection, "删除后同步智能体工作区");
    Ok(())
}

fn set_default_agent_with_connection(
    connection: &Connection,
    agent_id: &str,
) -> Result<Option<AgentRecord>, String> {
    ensure_agents_ready(connection)?;
    let Some(agent) = get_active_agent_by_id(connection, agent_id)? else {
        return Err("要设为默认的智能体不存在".to_string());
    };

    persist_default_agent_id(connection, agent_id)?;
    Ok(Some(agent))
}

fn persist_default_agent_id(connection: &Connection, agent_id: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![DEFAULT_AGENT_STATE_KEY, agent_id, crate::chrono_like_timestamp()],
        )
        .map_err(|error| format!("保存默认智能体失败: {error}"))?;

    Ok(())
}

fn rename_agent_references(
    connection: &Connection,
    old_agent_id: &str,
    new_agent_id: &str,
) -> Result<(), String> {
    let now = crate::chrono_like_timestamp();
    for (table, statement, uses_now) in [
        (
            "agent_skills",
            "UPDATE agent_skills SET agent_id = ?2 WHERE agent_id = ?1",
            false,
        ),
        (
            "agent_bot_bindings",
            "UPDATE agent_bot_bindings SET agent_id = ?2, updated_at = ?3 WHERE agent_id = ?1",
            true,
        ),
        (
            "chat_sessions",
            "UPDATE chat_sessions SET agent_id = ?2 WHERE agent_id = ?1",
            false,
        ),
        (
            "workspace_members",
            "UPDATE workspace_members SET agent_id = ?2 WHERE agent_id = ?1",
            false,
        ),
        (
            "workspaces",
            "UPDATE workspaces SET supervisor_agent_id = ?2, updated_at = ?3 WHERE supervisor_agent_id = ?1",
            true,
        ),
        (
            "workspace_resources",
            "UPDATE workspace_resources SET uploader_agent_id = ?2 WHERE uploader_agent_id = ?1",
            false,
        ),
        (
            "workspace_memories",
            "UPDATE workspace_memories SET author_agent_id = ?2, updated_at = ?3 WHERE author_agent_id = ?1",
            true,
        ),
    ] {
        if !table_exists(connection, table)? {
            continue;
        }
        if uses_now {
            connection
                .execute(statement, params![old_agent_id, new_agent_id, now])
                .map_err(|error| format!("更新 Agent_ID 引用失败: {error}"))?;
        } else {
            connection
                .execute(statement, params![old_agent_id, new_agent_id])
                .map_err(|error| format!("更新 Agent_ID 引用失败: {error}"))?;
        }
    }

    connection
        .execute(
            "UPDATE app_state SET value = ?2, updated_at = ?3 WHERE key = ?4 AND value = ?1",
            params![old_agent_id, new_agent_id, now, DEFAULT_AGENT_STATE_KEY],
        )
        .map_err(|error| format!("更新默认智能体引用失败: {error}"))?;

    Ok(())
}

fn table_exists(connection: &Connection, table_name: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table_name],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(|error| format!("检查数据表是否存在失败: {error}"))
}

fn replace_agent_skills(
    connection: &Connection,
    agent_id: &str,
    skill_ids: &[String],
    created_at: i64,
) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM agent_skills WHERE agent_id = ?1",
            params![agent_id],
        )
        .map_err(|error| format!("清理智能体技能绑定失败: {error}"))?;

    for (index, skill_id) in skill_ids.iter().enumerate() {
        connection
            .execute(
                "INSERT INTO agent_skills (id, agent_id, skill_id, sort_order, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    format!("agent_skill_{}", Uuid::new_v4().simple()),
                    agent_id,
                    skill_id,
                    index as i64,
                    created_at,
                ],
            )
            .map_err(|error| format!("写入智能体技能绑定失败: {error}"))?;
    }

    Ok(())
}

fn random_peer_inbound_secret() -> String {
    format!("ncp_{}", Uuid::new_v4().simple())
}

fn peer_binding_has_secret(config: &AgentBotConfig) -> bool {
    config
        .peer_shared_secret
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        || !config.client_secret.trim().is_empty()
}

fn new_peer_binding_with_secret() -> AgentBotConfig {
    normalize_bot_config(AgentBotConfig {
        enabled: false,
        im_channel_paused: false,
        client_id: String::new(),
        client_secret: String::new(),
        status: "未连接".to_string(),
        token: None,
        base_url: None,
        route_tag: None,
        ai_provider_id: None,
        ai_api_format: None,
        ai_base_url: None,
        ai_api_key: None,
        ai_model: None,
        error_message: None,
        peer_shared_secret: Some(random_peer_inbound_secret()),
    })
}

/// 保证 `peer` 渠道存在且带有入站密钥：新建时生成；更新时若表单清空则保留库中旧密钥。
fn apply_peer_inbound_defaults(
    bot_configs: &mut HashMap<String, AgentBotConfig>,
    previous_peer: Option<&AgentBotConfig>,
) {
    use std::collections::hash_map::Entry;

    match bot_configs.entry("peer".to_string()) {
        Entry::Vacant(slot) => {
            slot.insert(new_peer_binding_with_secret());
        }
        Entry::Occupied(mut entry) => {
            let config = entry.get_mut();
            if peer_binding_has_secret(config) {
                return;
            }
            if let Some(prev) = previous_peer {
                if let Some(secret) = prev
                    .peer_shared_secret
                    .as_deref()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                {
                    config.peer_shared_secret = Some(secret);
                    return;
                }
                let trimmed = prev.client_secret.trim();
                if !trimmed.is_empty() {
                    config.client_secret = prev.client_secret.clone();
                    return;
                }
            }
            config.peer_shared_secret = Some(random_peer_inbound_secret());
        }
    }
}

/// 启动时补全历史智能体的对等密钥（幂等）。
pub fn backfill_peer_inbound_secrets(app: &AppHandle) -> Result<u32, String> {
    let mut connection = crate::open_history_db(app)?;
    ensure_agents_ready(&connection)?;
    let agents = list_agents_with_connection(&connection)?;
    let now = crate::chrono_like_timestamp();
    let transaction = connection
        .transaction()
        .map_err(|error| format!("对等密钥补全事务失败: {error}"))?;
    let mut updated = 0u32;
    for agent in agents {
        let mut configs = agent.bot_configs.clone();
        let previous = agent.bot_configs.get("peer");
        apply_peer_inbound_defaults(&mut configs, previous);
        if configs.get("peer") != agent.bot_configs.get("peer") {
            replace_agent_bot_bindings(&transaction, &agent.id, &configs, now)?;
            updated += 1;
        }
    }
    transaction
        .commit()
        .map_err(|error| format!("对等密钥补全提交失败: {error}"))?;
    Ok(updated)
}

/// 轮换某智能体的对等入站密钥并立即落库。
pub fn rotate_agent_peer_inbound_secret(
    app: &AppHandle,
    agent_id: &str,
) -> Result<AgentRecord, String> {
    let mut connection = crate::open_history_db(app)?;
    ensure_agents_ready(&connection)?;
    let Some(record) = get_active_agent_by_id(&connection, agent_id)? else {
        return Err("智能体不存在".to_string());
    };
    let now = crate::chrono_like_timestamp();
    let mut configs = record.bot_configs.clone();
    let previous = record.bot_configs.get("peer");
    apply_peer_inbound_defaults(&mut configs, previous);
    if let Some(peer) = configs.get_mut("peer") {
        peer.peer_shared_secret = Some(random_peer_inbound_secret());
        peer.client_secret.clear();
    }
    let transaction = connection
        .transaction()
        .map_err(|error| format!("轮换对等密钥事务失败: {error}"))?;
    replace_agent_bot_bindings(&transaction, agent_id, &configs, now)?;
    transaction
        .commit()
        .map_err(|error| format!("轮换对等密钥提交失败: {error}"))?;
    get_active_agent_by_id(&connection, agent_id)?.ok_or_else(|| "轮换后读取智能体失败".to_string())
}

fn replace_agent_bot_bindings(
    connection: &Connection,
    agent_id: &str,
    bot_configs: &HashMap<String, AgentBotConfig>,
    updated_at: i64,
) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM agent_bot_bindings WHERE agent_id = ?1",
            params![agent_id],
        )
        .map_err(|error| format!("清理智能体机器人绑定失败: {error}"))?;

    for (channel_id, config) in bot_configs {
        let config_json = serialize_bot_config(config)?;
        connection
            .execute(
                "INSERT INTO agent_bot_bindings (id, agent_id, channel_id, config_json, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![
                    format!("agent_bot_binding_{}", Uuid::new_v4().simple()),
                    agent_id,
                    channel_id,
                    config_json,
                    updated_at,
                ],
            )
            .map_err(|error| format!("写入智能体机器人绑定失败: {error}"))?;
    }

    Ok(())
}

fn normalize_agent_input(payload: AgentInput) -> Result<NormalizedAgentInput, String> {
    let id = normalize_agent_id(payload.id)?;
    let name = trim_required(payload.name, "智能体名称")?;
    let explicit_summary = payload.summary.trim().to_string();
    let explicit_description = payload.description.trim().to_string();
    let description = if !explicit_description.is_empty() {
        explicit_description
    } else if !explicit_summary.is_empty() {
        explicit_summary.clone()
    } else {
        return Err("智能体角色说明不能为空".to_string());
    };
    let summary = if !explicit_summary.is_empty() {
        explicit_summary
    } else {
        build_auto_agent_summary(&description, &name)
    };
    let default_provider_id = trim_required(payload.default_provider_id, "默认模型供应商")?;
    let default_model = trim_required(payload.default_model, "默认模型")?;

    Ok(NormalizedAgentInput {
        id,
        name,
        summary,
        description,
        trigger_condition: payload.trigger_condition.trim().to_string(),
        manual_trigger_only: payload.manual_trigger_only,
        system_prompt: payload.system_prompt.trim().to_string(),
        capability_policy: normalize_capability_policy(
            payload.capability_policy,
            static_capability_policy(),
        ),
        skill_ids: dedupe_skill_ids(payload.skill_ids),
        allowed_tool_ids: normalize_allowed_tool_ids(payload.allowed_tool_ids),
        default_provider_id,
        default_model,
        execution_mode: normalize_execution_mode(payload.execution_mode.as_deref()),
        collaboration_config: payload
            .collaboration_config
            .map(normalize_collaboration_config),
        accent_color: payload
            .accent_color
            .map(|color| color.trim().to_string())
            .filter(|color| !color.is_empty()),
        avatar_uri: payload
            .avatar_uri
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        bot_configs: normalize_bot_configs(payload.bot_configs)?,
        heartbeat_config: normalize_heartbeat_config(payload.heartbeat_config)?,
        scenario_llm_config: normalize_scenario_llm_config(payload.scenario_llm_config),
        agent_loop_config: payload.agent_loop_config,
    })
}

fn trim_required(value: String, field_name: &str) -> Result<String, String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(format!("{field_name}不能为空"));
    }
    Ok(trimmed)
}

fn normalize_agent_id(value: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err("Agent_ID 只能包含英文字母、数字、下划线和连字符".to_string());
    }
    Ok(Some(trimmed))
}

fn normalize_inline_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn build_auto_agent_summary(description: &str, fallback_name: &str) -> String {
    const AUTO_AGENT_SUMMARY_MAX_CHARS: usize = 36;

    let normalized_description = normalize_inline_text(description);
    let source = if normalized_description.is_empty() {
        normalize_inline_text(fallback_name)
    } else {
        normalized_description
    };
    if source.is_empty() {
        return String::new();
    }
    let total_chars = source.chars().count();
    if total_chars <= AUTO_AGENT_SUMMARY_MAX_CHARS {
        return source;
    }
    let truncated: String = source
        .chars()
        .take(AUTO_AGENT_SUMMARY_MAX_CHARS.saturating_sub(1))
        .collect();
    format!("{truncated}…")
}

fn default_execution_mode() -> String {
    "single".to_string()
}

fn default_true() -> bool {
    true
}

fn default_heartbeat_task_type() -> String {
    "notify".to_string()
}

fn default_heartbeat_schedule_type() -> String {
    "daily".to_string()
}

fn default_heartbeat_channel_id() -> String {
    "wechat".to_string()
}

fn default_heartbeat_timeout_sec() -> i64 {
    180
}

fn default_heartbeat_timezone() -> String {
    "Asia/Shanghai".to_string()
}

fn normalize_execution_mode(value: Option<&str>) -> String {
    match value.unwrap_or_default().trim() {
        "supervisor" => "supervisor".to_string(),
        "worker" => "worker".to_string(),
        _ => "single".to_string(),
    }
}

#[allow(dead_code)]
fn execution_mode_label(value: &str) -> &'static str {
    match value {
        "supervisor" => "多智能体协调者",
        "worker" => "协作执行者",
        _ => "单智能体",
    }
}

fn default_shared_context_policy() -> String {
    "session".to_string()
}

fn normalize_collaboration_config(config: AgentCollaborationConfig) -> AgentCollaborationConfig {
    AgentCollaborationConfig {
        allowed_delegate_agent_ids: dedupe_skill_ids(config.allowed_delegate_agent_ids),
        handoff_prompt: config.handoff_prompt.trim().to_string(),
        shared_context_policy: normalize_shared_context_policy(&config.shared_context_policy),
    }
}

fn normalize_shared_context_policy(value: &str) -> String {
    match value.trim() {
        "summary" => "summary".to_string(),
        "none" => "none".to_string(),
        _ => "session".to_string(),
    }
}

#[allow(dead_code)]
fn shared_context_policy_label(value: &str) -> &'static str {
    match value {
        "summary" => "摘要共享",
        "none" => "不共享",
        _ => "完整会话共享",
    }
}

fn serialize_collaboration_config(
    config: Option<&AgentCollaborationConfig>,
) -> Result<Option<String>, String> {
    config
        .map(|item| {
            serde_json::to_string(item)
                .map_err(|error| format!("序列化智能体协作配置失败: {error}"))
        })
        .transpose()
}

fn deserialize_collaboration_config(raw: Option<String>) -> Option<AgentCollaborationConfig> {
    raw.and_then(|value| serde_json::from_str::<AgentCollaborationConfig>(&value).ok())
}

fn serialize_capability_policy(config: &AgentCapabilityPolicy) -> Result<Option<String>, String> {
    let normalized = normalize_capability_policy(Some(config.clone()), static_capability_policy());
    serde_json::to_string(&normalized)
        .map(Some)
        .map_err(|error| format!("序列化能力策略失败: {error}"))
}

fn serialize_allowed_tool_ids(tool_ids: &[String]) -> Result<Option<String>, String> {
    let normalized = normalize_allowed_tool_ids(tool_ids.to_vec());
    serde_json::to_string(&normalized)
        .map(Some)
        .map_err(|error| format!("序列化允许工具失败: {error}"))
}

fn deserialize_allowed_tool_ids(raw: Option<String>) -> Vec<String> {
    raw.and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
        .map(normalize_allowed_tool_ids)
        .unwrap_or_else(default_allowed_tool_ids)
}

fn deserialize_capability_policy(
    raw: Option<String>,
    _execution_mode: &str,
    _skill_ids: &[String],
    is_builtin: bool,
) -> AgentCapabilityPolicy {
    let fallback = if is_builtin {
        new_agent_default_capability_policy()
    } else {
        static_capability_policy()
    };
    raw.and_then(|value| serde_json::from_str::<AgentCapabilityPolicy>(&value).ok())
        .map(|value| normalize_capability_policy(Some(value), fallback.clone()))
        .unwrap_or(fallback)
}

fn serialize_heartbeat_config(config: &AgentHeartbeatConfig) -> Result<Option<String>, String> {
    let normalized = normalize_heartbeat_config(config.clone())?;
    if normalized.timezone == default_heartbeat_timezone()
        && normalized.tasks.is_empty()
        && normalized.schedules.is_empty()
    {
        return Ok(None);
    }

    serde_json::to_string(&normalized)
        .map(Some)
        .map_err(|error| format!("序列化心跳任务配置失败: {error}"))
}

fn deserialize_heartbeat_config(raw: Option<String>) -> AgentHeartbeatConfig {
    raw.and_then(|value| serde_json::from_str::<AgentHeartbeatConfig>(&value).ok())
        .and_then(|config| normalize_heartbeat_config(config).ok())
        .unwrap_or_default()
}

fn normalize_scenario_llm_slot(slot: Option<AgentScenarioLlmSlot>) -> Option<AgentScenarioLlmSlot> {
    let slot = slot?;
    let provider_id = slot.provider_id.trim().to_string();
    let model = slot.model.trim().to_string();
    if provider_id.is_empty() || model.is_empty() {
        None
    } else {
        Some(AgentScenarioLlmSlot { provider_id, model })
    }
}

fn normalize_scenario_llm_config(
    config: Option<AgentScenarioLlmConfig>,
) -> Option<AgentScenarioLlmConfig> {
    let config = config.unwrap_or_default();
    let title_generation = normalize_scenario_llm_slot(config.title_generation);
    let memory_extraction = normalize_scenario_llm_slot(config.memory_extraction);
    let task_push_notification_copy =
        normalize_scenario_llm_slot(config.task_push_notification_copy);
    if title_generation.is_none()
        && memory_extraction.is_none()
        && task_push_notification_copy.is_none()
    {
        None
    } else {
        Some(AgentScenarioLlmConfig {
            title_generation,
            memory_extraction,
            task_push_notification_copy,
        })
    }
}

fn serialize_scenario_llm_config(
    config: &Option<AgentScenarioLlmConfig>,
) -> Result<Option<String>, String> {
    let Some(config) = config else {
        return Ok(None);
    };
    serde_json::to_string(config)
        .map(Some)
        .map_err(|error| format!("序列化场景模型配置失败: {error}"))
}

fn deserialize_scenario_llm_config(raw: Option<String>) -> Option<AgentScenarioLlmConfig> {
    let raw = raw?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed: AgentScenarioLlmConfig = serde_json::from_str(trimmed).ok()?;
    normalize_scenario_llm_config(Some(parsed))
}

fn serialize_agent_loop_config(
    config: &Option<crate::agent_loop_types::AgentLoopConfig>,
) -> Result<Option<String>, String> {
    let Some(config) = config else {
        return Ok(None);
    };
    serde_json::to_string(config)
        .map(Some)
        .map_err(|error| format!("序列化 agent_loop_config 失败: {error}"))
}

fn deserialize_agent_loop_config(
    raw: Option<String>,
) -> Option<crate::agent_loop_types::AgentLoopConfig> {
    let raw = raw?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

fn normalize_heartbeat_config(
    config: AgentHeartbeatConfig,
) -> Result<AgentHeartbeatConfig, String> {
    let timezone = normalize_heartbeat_timezone(&config.timezone);
    let tasks = normalize_heartbeat_tasks(config.tasks)?;
    let task_ids: HashSet<_> = tasks.iter().map(|task| task.id.as_str()).collect();
    let schedules = normalize_heartbeat_schedules(config.schedules, &task_ids)?;

    Ok(AgentHeartbeatConfig {
        timezone,
        tasks,
        schedules,
    })
}

fn normalize_heartbeat_timezone(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default_heartbeat_timezone()
    } else {
        trimmed.to_string()
    }
}

fn normalize_heartbeat_tasks(
    tasks: Vec<AgentHeartbeatTask>,
) -> Result<Vec<AgentHeartbeatTask>, String> {
    let mut normalized = Vec::new();
    let mut seen_ids = HashSet::new();

    for (index, task) in tasks.into_iter().enumerate() {
        let id = normalize_stable_item_id(&task.id, "task");
        if !seen_ids.insert(id.clone()) {
            continue;
        }

        let task_type = match task.task_type.trim() {
            "shell" => "shell".to_string(),
            _ => "notify".to_string(),
        };
        let name = fallback_name(task.name, "任务", index + 1);
        let description = task.description.trim().to_string();
        let message_template = task.message_template.trim().to_string();
        let command = task.command.trim().to_string();
        let working_directory = task.working_directory.trim().to_string();
        let timeout_sec = task.timeout_sec.clamp(10, 7200);

        if task.enabled && task_type == "shell" && command.is_empty() {
            return Err(format!("心跳任务“{name}”缺少执行命令"));
        }

        normalized.push(AgentHeartbeatTask {
            id,
            name,
            description,
            task_type,
            enabled: task.enabled,
            message_template,
            command,
            working_directory,
            timeout_sec,
            notify_on_success: task.notify_on_success,
            notify_on_failure: task.notify_on_failure,
        });
    }

    Ok(normalized)
}

fn normalize_heartbeat_schedules(
    schedules: Vec<AgentHeartbeatSchedule>,
    task_ids: &HashSet<&str>,
) -> Result<Vec<AgentHeartbeatSchedule>, String> {
    let mut normalized = Vec::new();
    let mut seen_ids = HashSet::new();

    for (index, schedule) in schedules.into_iter().enumerate() {
        let id = normalize_stable_item_id(&schedule.id, "schedule");
        if !seen_ids.insert(id.clone()) {
            continue;
        }

        let name = fallback_name(schedule.name, "规则", index + 1);
        let task_id = schedule.task_id.trim().to_string();
        let times = normalize_schedule_times(schedule.times)?;
        let channel_id = schedule.channel_id.trim();
        let channel_id = if channel_id.is_empty() {
            default_heartbeat_channel_id()
        } else {
            channel_id.to_string()
        };
        let target_user_id = schedule.target_user_id.trim().to_string();
        let target_label = schedule.target_label.trim().to_string();

        if !task_id.is_empty() && !task_ids.contains(task_id.as_str()) {
            return Err(format!("心跳规则“{name}”引用了不存在的任务"));
        }
        if schedule.enabled && task_id.is_empty() {
            return Err(format!("启用中的心跳规则“{name}”必须绑定一个任务"));
        }
        if schedule.enabled && times.is_empty() {
            return Err(format!("启用中的心跳规则“{name}”至少需要一个触发时间"));
        }
        if schedule.enabled && target_user_id.is_empty() {
            return Err(format!("启用中的心跳规则“{name}”缺少接收用户 ID"));
        }

        normalized.push(AgentHeartbeatSchedule {
            id,
            name,
            enabled: schedule.enabled,
            task_id,
            schedule_type: default_heartbeat_schedule_type(),
            times,
            channel_id,
            target_user_id,
            target_label,
        });
    }

    Ok(normalized)
}

fn normalize_schedule_times(times: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();

    for raw in times {
        for part in raw.split(',') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            let normalized_time = normalize_schedule_time(trimmed)?;
            if seen.insert(normalized_time.clone()) {
                normalized.push(normalized_time);
            }
        }
    }

    normalized.sort();
    Ok(normalized)
}

fn normalize_schedule_time(value: &str) -> Result<String, String> {
    let Some((hour_raw, minute_raw)) = value.split_once(':') else {
        return Err(format!("无效的时间格式：{value}，请使用 HH:MM"));
    };

    let hour: u32 = hour_raw
        .trim()
        .parse()
        .map_err(|_| format!("无效的小时：{value}"))?;
    let minute: u32 = minute_raw
        .trim()
        .parse()
        .map_err(|_| format!("无效的分钟：{value}"))?;

    if hour > 23 || minute > 59 {
        return Err(format!("无效的时间：{value}"));
    }

    Ok(format!("{hour:02}:{minute:02}"))
}

fn normalize_stable_item_id(raw: &str, prefix: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        format!("{prefix}_{}", Uuid::new_v4().simple())
    } else {
        trimmed.to_string()
    }
}

fn fallback_name(name: String, prefix: &str, index: usize) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        format!("{prefix} {index}")
    } else {
        trimmed.to_string()
    }
}

fn normalize_bot_configs(
    bot_configs: HashMap<String, AgentBotConfig>,
) -> Result<HashMap<String, AgentBotConfig>, String> {
    let mut normalized = HashMap::new();
    for (channel_id, config) in bot_configs {
        let trimmed_channel_id = channel_id.trim().to_string();
        if trimmed_channel_id.is_empty() {
            continue;
        }
        normalized.insert(trimmed_channel_id, normalize_bot_config(config));
    }
    Ok(normalized)
}

fn normalize_bot_config(config: AgentBotConfig) -> AgentBotConfig {
    AgentBotConfig {
        enabled: config.enabled,
        im_channel_paused: config.im_channel_paused,
        client_id: config.client_id.trim().to_string(),
        client_secret: config.client_secret.trim().to_string(),
        status: normalize_bot_status(&config.status),
        token: config
            .token
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        base_url: config
            .base_url
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        route_tag: config
            .route_tag
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        ai_provider_id: config
            .ai_provider_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        ai_api_format: config
            .ai_api_format
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        ai_base_url: config
            .ai_base_url
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        ai_api_key: config
            .ai_api_key
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        ai_model: config
            .ai_model
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        error_message: config
            .error_message
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        peer_shared_secret: config
            .peer_shared_secret
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    }
}

fn normalize_bot_status(status: &str) -> String {
    match status.trim() {
        "待接入" => "待接入".to_string(),
        "已连接" => "已连接".to_string(),
        "登录中" => "登录中".to_string(),
        "错误" => "错误".to_string(),
        _ => "未连接".to_string(),
    }
}

fn serialize_bot_config(config: &AgentBotConfig) -> Result<String, String> {
    serde_json::to_string(config).map_err(|error| format!("序列化机器人绑定配置失败: {error}"))
}

fn deserialize_bot_config(raw: &str) -> Result<AgentBotConfig, String> {
    serde_json::from_str::<AgentBotConfig>(raw)
        .map(normalize_bot_config)
        .map_err(|error| format!("解析机器人绑定配置失败: {error}"))
}

struct NormalizedAgentInput {
    id: Option<String>,
    name: String,
    summary: String,
    description: String,
    trigger_condition: String,
    manual_trigger_only: bool,
    system_prompt: String,
    capability_policy: AgentCapabilityPolicy,
    skill_ids: Vec<String>,
    allowed_tool_ids: Vec<String>,
    default_provider_id: String,
    default_model: String,
    execution_mode: String,
    collaboration_config: Option<AgentCollaborationConfig>,
    accent_color: Option<String>,
    avatar_uri: Option<String>,
    bot_configs: HashMap<String, AgentBotConfig>,
    heartbeat_config: AgentHeartbeatConfig,
    scenario_llm_config: Option<AgentScenarioLlmConfig>,
    agent_loop_config: Option<crate::agent_loop_types::AgentLoopConfig>,
}

fn sync_active_agent_workspaces(connection: &Connection) -> Result<(), String> {
    if !agent_workspace::runtime_sync_enabled() {
        return Ok(());
    }

    let agents = list_active_agents_for_workspace(connection)?;
    for agent in &agents {
        ensure_workspace_for_record(agent, false)?;
    }
    sync_agent_registry_from_records(&agents)?;
    Ok(())
}

fn sync_agent_registry_from_records(records: &[AgentRecord]) -> Result<(), String> {
    if !agent_workspace::runtime_sync_enabled() {
        return Ok(());
    }

    let seeds: Vec<_> = records.iter().map(record_workspace_seed).collect();
    agent_workspace::sync_agent_registry(&seeds)
}

fn ensure_workspace_for_record(record: &AgentRecord, create_bootstrap: bool) -> Result<(), String> {
    if !agent_workspace::runtime_sync_enabled() {
        return Ok(());
    }

    agent_workspace::ensure_agent_workspace(record_workspace_seed(record), create_bootstrap)
        .map(|_| ())
}

fn try_sync_active_agent_workspaces(connection: &Connection, context: &str) {
    if let Err(error) = sync_active_agent_workspaces(connection) {
        log_workspace_warning(context, &error);
    }
}

fn try_sync_agent_registry_from_records(records: &[AgentRecord], context: &str) {
    if let Err(error) = sync_agent_registry_from_records(records) {
        log_workspace_warning(context, &error);
    }
}

fn try_ensure_workspace_for_record(record: &AgentRecord, create_bootstrap: bool, context: &str) {
    if let Err(error) = ensure_workspace_for_record(record, create_bootstrap) {
        log_workspace_warning(context, &error);
    }
}

fn log_workspace_warning(context: &str, error: &str) {
    eprintln!("NineClaw: {context}失败，已降级为仅保留数据库状态: {error}");
}

fn record_workspace_seed<'a>(record: &'a AgentRecord) -> AgentWorkspaceSeed<'a> {
    AgentWorkspaceSeed {
        id: record.id.as_str(),
        name: record.name.as_str(),
        summary: record.summary.as_str(),
        description: record.description.as_str(),
        accent_color: record.accent_color.as_deref(),
        is_builtin: record.is_builtin,
    }
}

fn list_active_agents_for_workspace(connection: &Connection) -> Result<Vec<AgentRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT
                id,
                name,
                summary,
                description,
                trigger_condition,
                manual_trigger_only,
                system_prompt,
                default_provider_id,
                default_model,
                is_builtin,
                is_archived,
                execution_mode,
                collaboration_config_json,
                heartbeat_config_json,
                allowed_tool_ids_json,
                accent_color,
                avatar_uri,
                scenario_llm_config_json,
                agent_loop_config_json,
                created_at,
                updated_at
            FROM agents
            WHERE is_archived = 0
            ORDER BY lower(name) ASC, updated_at DESC",
        )
        .map_err(|error| format!("准备同步 agent workspace 失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(AgentRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                summary: row.get(2)?,
                description: row.get(3)?,
                trigger_condition: row.get(4)?,
                manual_trigger_only: row.get::<_, i64>(5)? != 0,
                system_prompt: row.get(6)?,
                capability_policy: static_capability_policy(),
                skill_ids: Vec::new(),
                default_provider_id: row.get(7)?,
                default_model: row.get(8)?,
                is_builtin: row.get::<_, i64>(9)? != 0,
                is_archived: row.get::<_, i64>(10)? != 0,
                execution_mode: row.get(11)?,
                collaboration_config: deserialize_collaboration_config(row.get(12)?),
                heartbeat_config: deserialize_heartbeat_config(row.get(13)?),
                allowed_tool_ids: deserialize_allowed_tool_ids(row.get(14)?),
                accent_color: row.get(15)?,
                avatar_uri: row.get(16)?,
                scenario_llm_config: deserialize_scenario_llm_config(row.get(17)?),
                agent_loop_config: deserialize_agent_loop_config(row.get(18)?),
                bot_configs: HashMap::new(),
                created_at: row.get(19)?,
                updated_at: row.get(20)?,
            })
        })
        .map_err(|error| format!("读取同步用 agent 数据失败: {error}"))?;

    let mut agents = Vec::new();
    for row in rows {
        let mut agent = row.map_err(|error| format!("解析同步用 agent 数据失败: {error}"))?;
        agent.skill_ids = load_skill_ids_for_agent(connection, &agent.id)?;
        agent.capability_policy = load_capability_policy_for_agent(
            connection,
            &agent.id,
            &agent.execution_mode,
            &agent.skill_ids,
            agent.is_builtin,
        )?;
        agents.push(agent);
    }

    Ok(agents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory db");
        connection
            .execute_batch(
                "CREATE TABLE app_state (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                );",
            )
            .expect("app_state schema");
        connection
    }

    #[test]
    fn create_and_update_agent_roundtrip_skills() {
        let mut connection = connection();

        let created = create_agent_with_connection(
            &mut connection,
            AgentInput {
                id: None,
                name: "自定义助理".to_string(),
                summary: "负责综合处理".to_string(),
                description: "更完整的说明".to_string(),
                trigger_condition: String::new(),
                manual_trigger_only: false,
                system_prompt: "先确认，再执行".to_string(),
                capability_policy: None,
                skill_ids: vec!["alpha".to_string(), "alpha".to_string(), "beta".to_string()],
                allowed_tool_ids: default_allowed_tool_ids(),
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: Some("single".to_string()),
                collaboration_config: None,
                accent_color: Some("#112233".to_string()),
                avatar_uri: None,
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig::default(),
                scenario_llm_config: None,
                agent_loop_config: None,
            },
        )
        .expect("create agent");

        assert_eq!(
            created.skill_ids,
            vec!["alpha".to_string(), "beta".to_string()]
        );
        assert_eq!(created.capability_policy.strategy, "static");
        assert_eq!(created.name, "自定义助理");
        assert_eq!(created.allowed_tool_ids, default_allowed_tool_ids());

        let updated = update_agent_with_connection(
            &mut connection,
            &created.id,
            AgentInput {
                id: None,
                name: "自定义助理 v2".to_string(),
                summary: "负责复杂处理".to_string(),
                description: "更新后的说明".to_string(),
                trigger_condition: String::new(),
                manual_trigger_only: false,
                system_prompt: "".to_string(),
                capability_policy: Some(AgentCapabilityPolicy {
                    strategy: "dynamic".to_string(),
                    required_skill_ids: vec!["required".to_string()],
                    forbidden_skill_ids: vec!["blocked".to_string()],
                    max_dynamic_skills: 3,
                }),
                skill_ids: vec!["gamma".to_string()],
                allowed_tool_ids: vec!["read_file".to_string(), "web_fetch".to_string()],
                default_provider_id: "anthropic".to_string(),
                default_model: "claude-sonnet-4-0".to_string(),
                execution_mode: Some("supervisor".to_string()),
                collaboration_config: Some(AgentCollaborationConfig {
                    allowed_delegate_agent_ids: vec!["worker-1".to_string()],
                    handoff_prompt: "只在必要时委派".to_string(),
                    shared_context_policy: "summary".to_string(),
                }),
                accent_color: None,
                avatar_uri: None,
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig::default(),
                scenario_llm_config: None,
                agent_loop_config: None,
            },
        )
        .expect("update agent");

        assert_eq!(updated.name, "自定义助理 v2");
        assert_eq!(updated.skill_ids, vec!["gamma".to_string()]);
        assert_eq!(updated.execution_mode, "supervisor");
        assert_eq!(updated.capability_policy.strategy, "dynamic");
        assert_eq!(
            updated.capability_policy.required_skill_ids,
            vec!["required".to_string()]
        );
        assert_eq!(
            updated
                .collaboration_config
                .as_ref()
                .map(|item| item.shared_context_policy.as_str()),
            Some("summary")
        );
        assert_eq!(
            updated.allowed_tool_ids,
            vec!["read_file".to_string(), "web_fetch".to_string()]
        );
    }

    #[test]
    fn create_agent_keeps_preferred_skill_list_clean() {
        let mut connection = connection();

        let created = create_agent_with_connection(
            &mut connection,
            AgentInput {
                id: None,
                name: "默认技能测试".to_string(),
                summary: "负责综合处理".to_string(),
                description: "更完整的说明".to_string(),
                trigger_condition: String::new(),
                manual_trigger_only: false,
                system_prompt: "".to_string(),
                capability_policy: Some(new_agent_default_capability_policy()),
                skill_ids: vec![],
                allowed_tool_ids: default_allowed_tool_ids(),
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: Some("single".to_string()),
                collaboration_config: None,
                accent_color: None,
                avatar_uri: None,
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig::default(),
                scenario_llm_config: None,
                agent_loop_config: None,
            },
        )
        .expect("create agent");

        assert!(created.skill_ids.is_empty());
        assert_eq!(created.capability_policy.strategy, "hybrid");
    }

    #[test]
    fn allowed_tool_ids_normalize_aliases_and_map_to_runtime_names() {
        let normalized = normalize_allowed_tool_ids(vec![
            " read ".to_string(),
            "find".to_string(),
            "agent_delegate".to_string(),
            "nineclaw_external_api".to_string(),
            "read_file".to_string(),
            "unknown".to_string(),
        ]);

        assert_eq!(
            normalized,
            vec![
                "read_file".to_string(),
                "glob".to_string(),
                "agent_spawn".to_string(),
                "external_api".to_string(),
            ]
        );
        assert_eq!(
            runtime_tool_names_for_allowed_tool_ids(&normalized),
            vec![
                "read".to_string(),
                "find".to_string(),
                "agent_delegate".to_string(),
                "nineclaw_external_api".to_string(),
            ]
        );
    }

    #[test]
    fn create_agent_generates_summary_from_description_when_missing() {
        let mut connection = connection();

        let created = create_agent_with_connection(
            &mut connection,
            AgentInput {
                id: None,
                name: "项目推进助理".to_string(),
                summary: "".to_string(),
                description: "负责把复杂需求拆解成可执行步骤，并持续推进收尾。".to_string(),
                trigger_condition: String::new(),
                manual_trigger_only: false,
                system_prompt: "".to_string(),
                capability_policy: None,
                skill_ids: vec![],
                allowed_tool_ids: default_allowed_tool_ids(),
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: Some("single".to_string()),
                collaboration_config: None,
                accent_color: None,
                avatar_uri: None,
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig::default(),
                scenario_llm_config: None,
                agent_loop_config: None,
            },
        )
        .expect("create agent");

        assert_eq!(
            created.summary,
            "负责把复杂需求拆解成可执行步骤，并持续推进收尾。"
        );
        assert_eq!(
            created.description,
            "负责把复杂需求拆解成可执行步骤，并持续推进收尾。"
        );
    }

    #[test]
    fn create_and_update_agent_supports_editable_agent_id_and_trigger_fields() {
        let mut connection = connection();

        let created = create_agent_with_connection(
            &mut connection,
            AgentInput {
                id: Some("review_agent".to_string()),
                name: "审查智能体".to_string(),
                summary: "".to_string(),
                description: "负责审查用户提交的内容".to_string(),
                trigger_condition: "用户请求审查时".to_string(),
                manual_trigger_only: true,
                system_prompt: "围绕 ${ARG} 审查".to_string(),
                capability_policy: None,
                skill_ids: vec![],
                allowed_tool_ids: default_allowed_tool_ids(),
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: Some("single".to_string()),
                collaboration_config: None,
                accent_color: None,
                avatar_uri: None,
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig::default(),
                scenario_llm_config: None,
                agent_loop_config: None,
            },
        )
        .expect("create agent");

        assert_eq!(created.id, "review_agent");
        assert_eq!(created.trigger_condition, "用户请求审查时");
        assert!(created.manual_trigger_only);

        let updated = update_agent_with_connection(
            &mut connection,
            &created.id,
            AgentInput {
                id: Some("review_agent_v2".to_string()),
                name: "审查智能体".to_string(),
                summary: "".to_string(),
                description: "负责审查用户提交的内容".to_string(),
                trigger_condition: "用户请求复核时".to_string(),
                manual_trigger_only: false,
                system_prompt: "围绕 ${ARG} 复核".to_string(),
                capability_policy: None,
                skill_ids: vec!["pdf".to_string()],
                allowed_tool_ids: vec!["read".to_string(), "find".to_string()],
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: Some("single".to_string()),
                collaboration_config: None,
                accent_color: None,
                avatar_uri: None,
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig::default(),
                scenario_llm_config: None,
                agent_loop_config: None,
            },
        )
        .expect("update id");

        assert_eq!(updated.id, "review_agent_v2");
        assert_eq!(updated.trigger_condition, "用户请求复核时");
        assert_eq!(
            updated.allowed_tool_ids,
            vec!["read_file".to_string(), "glob".to_string()]
        );
        assert!(!updated.manual_trigger_only);
        assert!(get_active_agent_by_id(&connection, "review_agent")
            .expect("old id lookup")
            .is_none());
    }

    #[test]
    fn default_agent_falls_back_and_reassigns_after_archive() {
        let mut connection = connection();

        let initial_default =
            get_default_agent_with_connection(&connection).expect("initial default");
        assert_eq!(
            initial_default.as_ref().map(|item| item.id.as_str()),
            Some("general-assistant")
        );

        let custom = create_agent_with_connection(
            &mut connection,
            AgentInput {
                id: None,
                name: "项目助理".to_string(),
                summary: "项目推进".to_string(),
                description: "负责项目推进".to_string(),
                trigger_condition: String::new(),
                manual_trigger_only: false,
                system_prompt: "".to_string(),
                capability_policy: None,
                skill_ids: vec![],
                allowed_tool_ids: default_allowed_tool_ids(),
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: Some("single".to_string()),
                collaboration_config: None,
                accent_color: None,
                avatar_uri: None,
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig::default(),
                scenario_llm_config: None,
                agent_loop_config: None,
            },
        )
        .expect("create custom");

        set_default_agent_with_connection(&connection, &custom.id).expect("set default");
        assert_eq!(
            get_default_agent_with_connection(&connection)
                .expect("updated default")
                .as_ref()
                .map(|item| item.id.as_str()),
            Some(custom.id.as_str())
        );

        archive_agent_with_connection(&mut connection, &custom.id).expect("archive custom");
        assert_eq!(
            get_default_agent_with_connection(&connection)
                .expect("fallback default")
                .as_ref()
                .map(|item| item.id.as_str()),
            Some("general-assistant")
        );
    }

    #[test]
    fn build_agent_system_prompt_contains_core_sections() {
        let prompt = build_agent_system_prompt(&ConversationAgentConfig {
            id: "agent".to_string(),
            name: "项目助理".to_string(),
            summary: "负责项目推进".to_string(),
            description: "擅长拆解任务和协调执行".to_string(),
            trigger_condition: String::new(),
            manual_trigger_only: false,
            system_prompt: "避免省略关键确认步骤".to_string(),
            capability_policy: AgentCapabilityPolicy {
                strategy: "hybrid".to_string(),
                required_skill_ids: vec![],
                forbidden_skill_ids: vec![],
                max_dynamic_skills: 4,
            },
            skill_ids: vec!["alpha".to_string(), "beta".to_string()],
            allowed_tool_ids: vec!["read_file".to_string(), "web_fetch".to_string()],
            default_provider_id: "openai".to_string(),
            default_model: "gpt-4.1".to_string(),
            execution_mode: "single".to_string(),
            collaboration_config: None,
            accent_color: None,
            avatar_uri: None,
            scenario_llm_config: None,
            agent_loop_config: None,
        })
        .expect("prompt");

        assert!(prompt.contains("项目助理"));
        assert!(prompt.contains("负责项目推进"));
        assert!(prompt.contains("能力策略"));
        assert!(prompt.contains("必须使用 web_fetch 工具"));
        assert!(prompt.contains("允许工具：read_file、web_fetch"));
        assert!(prompt.contains("偏好技能"));
        assert!(prompt.contains("最高优先级执行要求（MUST）"));
        assert!(prompt.contains("避免省略关键确认步骤"));
    }

    #[test]
    fn build_agent_system_prompt_expands_arg_placeholder() {
        let prompt = build_agent_system_prompt_for_prompt(
            &ConversationAgentConfig {
                id: "agent".to_string(),
                name: "审查智能体".to_string(),
                summary: "".to_string(),
                description: "负责审查".to_string(),
                trigger_condition: "用户要求审查时".to_string(),
                manual_trigger_only: true,
                system_prompt: "请审查：${ARG}".to_string(),
                capability_policy: AgentCapabilityPolicy::default(),
                skill_ids: vec![],
                allowed_tool_ids: default_allowed_tool_ids(),
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: "single".to_string(),
                collaboration_config: None,
                accent_color: None,
                avatar_uri: None,
                scenario_llm_config: None,
                agent_loop_config: None,
            },
            Some("合同条款"),
        )
        .expect("prompt");

        assert!(prompt.contains("触发条件：用户要求审查时"));
        assert!(prompt.contains("禁止模型自动调用"));
        assert!(prompt.contains("请审查：合同条款"));
        assert!(!prompt.contains("${ARG}"));
    }

    #[test]
    fn create_agent_persists_heartbeat_config() {
        let mut connection = connection();

        let created = create_agent_with_connection(
            &mut connection,
            AgentInput {
                id: None,
                name: "提醒助理".to_string(),
                summary: "会定时提醒".to_string(),
                description: "负责晨会提醒和日报抓取".to_string(),
                trigger_condition: String::new(),
                manual_trigger_only: false,
                system_prompt: "".to_string(),
                capability_policy: None,
                skill_ids: vec![],
                allowed_tool_ids: default_allowed_tool_ids(),
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: Some("single".to_string()),
                collaboration_config: None,
                accent_color: None,
                avatar_uri: None,
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig {
                    timezone: "Asia/Shanghai".to_string(),
                    tasks: vec![AgentHeartbeatTask {
                        id: "task_report".to_string(),
                        name: "日报抓取".to_string(),
                        description: "抓取日报后发给用户".to_string(),
                        task_type: "shell".to_string(),
                        enabled: true,
                        message_template: "今日结果：{{stdout}}".to_string(),
                        command: "python3 scripts/daily.py".to_string(),
                        working_directory: "".to_string(),
                        timeout_sec: 90,
                        notify_on_success: true,
                        notify_on_failure: true,
                    }],
                    schedules: vec![AgentHeartbeatSchedule {
                        id: "schedule_morning".to_string(),
                        name: "早间播报".to_string(),
                        enabled: true,
                        task_id: "task_report".to_string(),
                        schedule_type: "daily".to_string(),
                        times: vec!["8:00".to_string(), "17:30".to_string()],
                        channel_id: "wechat".to_string(),
                        target_user_id: "wxid_demo".to_string(),
                        target_label: "自己".to_string(),
                    }],
                },
                scenario_llm_config: None,
                agent_loop_config: None,
            },
        )
        .expect("create agent");

        assert_eq!(created.heartbeat_config.tasks.len(), 1);
        assert_eq!(created.heartbeat_config.schedules.len(), 1);
        assert_eq!(
            created.heartbeat_config.schedules[0].times,
            vec!["08:00".to_string(), "17:30".to_string()]
        );
        assert_eq!(
            created.heartbeat_config.schedules[0].target_user_id,
            "wxid_demo"
        );
    }
}
