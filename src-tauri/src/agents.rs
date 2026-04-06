use crate::agent_workspace::{self, AgentWorkspaceSeed};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::AppHandle;
use uuid::Uuid;

const DEFAULT_AGENT_STATE_KEY: &str = "default_agent_id";

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
    pub system_prompt: String,
    pub skill_ids: Vec<String>,
    pub default_provider_id: String,
    pub default_model: String,
    pub is_builtin: bool,
    pub is_archived: bool,
    pub execution_mode: String,
    pub collaboration_config: Option<AgentCollaborationConfig>,
    pub accent_color: Option<String>,
    #[serde(default)]
    pub bot_configs: HashMap<String, AgentBotConfig>,
    #[serde(default)]
    pub heartbeat_config: AgentHeartbeatConfig,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInput {
    pub name: String,
    pub summary: String,
    pub description: String,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub default_provider_id: String,
    pub default_model: String,
    #[serde(default)]
    pub execution_mode: Option<String>,
    #[serde(default)]
    pub collaboration_config: Option<AgentCollaborationConfig>,
    #[serde(default)]
    pub accent_color: Option<String>,
    #[serde(default)]
    pub bot_configs: HashMap<String, AgentBotConfig>,
    #[serde(default)]
    pub heartbeat_config: AgentHeartbeatConfig,
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
    pub system_prompt: String,
    #[serde(default)]
    pub skill_ids: Vec<String>,
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
        system_prompt: record.system_prompt,
        skill_ids: record.skill_ids,
        default_provider_id: record.default_provider_id,
        default_model: record.default_model,
        execution_mode: record.execution_mode,
        collaboration_config: record.collaboration_config,
        accent_color: record.accent_color,
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
        sections.push(format!(
            "已挂载技能：{}{}",
            listed, suffix
        ));
    }

    let system_prompt = agent.system_prompt.trim();
    if !system_prompt.is_empty() {
        sections.push(format!(
            "附加执行约束：{}",
            trim_prompt_snippet(system_prompt, 320)
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
                system_prompt TEXT NOT NULL DEFAULT '',
                default_provider_id TEXT NOT NULL,
                default_model TEXT NOT NULL,
                is_builtin INTEGER NOT NULL DEFAULT 0,
                is_archived INTEGER NOT NULL DEFAULT 0,
                execution_mode TEXT NOT NULL DEFAULT 'single',
                collaboration_config_json TEXT,
                heartbeat_config_json TEXT,
                accent_color TEXT,
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
        connection
            .execute(
                "INSERT OR IGNORE INTO agents (
                id, name, summary, description, system_prompt, default_provider_id,
                default_model, is_builtin, is_archived, execution_mode,
                collaboration_config_json, heartbeat_config_json, accent_color, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 0, 'single', NULL, NULL, ?8, ?9, ?9)",
                params![
                    seed.id,
                    seed.name,
                    seed.summary,
                    seed.description,
                    seed.system_prompt,
                    seed.default_provider_id,
                    seed.default_model,
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
                system_prompt,
                default_provider_id,
                default_model,
                is_builtin,
                is_archived,
                execution_mode,
                collaboration_config_json,
                heartbeat_config_json,
                accent_color,
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
                system_prompt: row.get(4)?,
                skill_ids: Vec::new(),
                default_provider_id: row.get(5)?,
                default_model: row.get(6)?,
                is_builtin: row.get::<_, i64>(7)? != 0,
                is_archived: row.get::<_, i64>(8)? != 0,
                execution_mode: row.get(9)?,
                collaboration_config: deserialize_collaboration_config(row.get(10)?),
                heartbeat_config: deserialize_heartbeat_config(row.get(11)?),
                accent_color: row.get(12)?,
                bot_configs: HashMap::new(),
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .map_err(|error| format!("解析智能体列表失败: {error}"))?;

    let mut agents = Vec::new();
    for row in rows {
        let mut agent = row.map_err(|error| format!("读取智能体数据失败: {error}"))?;
        agent.skill_ids = load_skill_ids_for_agent(connection, &agent.id)?;
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
                system_prompt,
                default_provider_id,
                default_model,
                is_builtin,
                is_archived,
                execution_mode,
                collaboration_config_json,
                heartbeat_config_json,
                accent_color,
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
                system_prompt: row.get(4)?,
                skill_ids: Vec::new(),
                default_provider_id: row.get(5)?,
                default_model: row.get(6)?,
                is_builtin: row.get::<_, i64>(7)? != 0,
                is_archived: row.get::<_, i64>(8)? != 0,
                execution_mode: row.get(9)?,
                collaboration_config: deserialize_collaboration_config(row.get(10)?),
                heartbeat_config: deserialize_heartbeat_config(row.get(11)?),
                accent_color: row.get(12)?,
                bot_configs: HashMap::new(),
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .optional()
        .map_err(|error| format!("读取智能体失败: {error}"))?;

    if let Some(ref mut record) = agent {
        record.skill_ids = load_skill_ids_for_agent(connection, &record.id)?;
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
    let agent_id = format!("agent_{}", Uuid::new_v4().simple());
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

    let transaction = connection
        .transaction()
        .map_err(|error| format!("创建智能体事务失败: {error}"))?;

    transaction
        .execute(
            "INSERT INTO agents (
                id, name, summary, description, system_prompt, default_provider_id,
                default_model, is_builtin, is_archived, execution_mode,
                collaboration_config_json, heartbeat_config_json, accent_color, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, ?8, ?9, ?10, ?11, ?12, ?12)",
            params![
                agent_id.as_str(),
                normalized.name.as_str(),
                normalized.summary.as_str(),
                normalized.description.as_str(),
                normalized.system_prompt.as_str(),
                normalized.default_provider_id.as_str(),
                normalized.default_model.as_str(),
                normalized.execution_mode.as_str(),
                collaboration_json,
                heartbeat_json,
                normalized.accent_color.as_deref(),
                now,
            ],
        )
        .map_err(|error| format!("写入智能体失败: {error}"))?;

    replace_agent_skills(&transaction, &agent_id, &normalized.skill_ids, now)?;
    replace_agent_bot_bindings(&transaction, &agent_id, &normalized.bot_configs, now)?;
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
    if get_active_agent_by_id(connection, agent_id)?.is_none() {
        return Err("要更新的智能体不存在".to_string());
    }

    let normalized = normalize_agent_input(payload)?;
    let now = crate::chrono_like_timestamp();
    let collaboration_json =
        serialize_collaboration_config(normalized.collaboration_config.as_ref())?;
    let heartbeat_json = serialize_heartbeat_config(&normalized.heartbeat_config)?;

    let transaction = connection
        .transaction()
        .map_err(|error| format!("更新智能体事务失败: {error}"))?;

    transaction
        .execute(
            "UPDATE agents
            SET
                name = ?2,
                summary = ?3,
                description = ?4,
                system_prompt = ?5,
                default_provider_id = ?6,
                default_model = ?7,
                execution_mode = ?8,
                collaboration_config_json = ?9,
                heartbeat_config_json = ?10,
                accent_color = ?11,
                updated_at = ?12
            WHERE id = ?1 AND is_archived = 0",
            params![
                agent_id,
                normalized.name,
                normalized.summary,
                normalized.description,
                normalized.system_prompt,
                normalized.default_provider_id,
                normalized.default_model,
                normalized.execution_mode,
                collaboration_json,
                heartbeat_json,
                normalized.accent_color,
                now,
            ],
        )
        .map_err(|error| format!("更新智能体失败: {error}"))?;

    replace_agent_skills(&transaction, agent_id, &normalized.skill_ids, now)?;
    replace_agent_bot_bindings(&transaction, agent_id, &normalized.bot_configs, now)?;
    transaction
        .commit()
        .map_err(|error| format!("提交智能体更新失败: {error}"))?;

    let record = get_active_agent_by_id(connection, agent_id)?
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
    let name = trim_required(payload.name, "智能体名称")?;
    let summary = trim_required(payload.summary, "智能体简介")?;
    let description = trim_required(payload.description, "智能体介绍")?;
    let default_provider_id = trim_required(payload.default_provider_id, "默认模型供应商")?;
    let default_model = trim_required(payload.default_model, "默认模型")?;

    Ok(NormalizedAgentInput {
        name,
        summary,
        description,
        system_prompt: payload.system_prompt.trim().to_string(),
        skill_ids: dedupe_skill_ids(payload.skill_ids),
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
        bot_configs: normalize_bot_configs(payload.bot_configs)?,
        heartbeat_config: normalize_heartbeat_config(payload.heartbeat_config)?,
    })
}

fn trim_required(value: String, field_name: &str) -> Result<String, String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(format!("{field_name}不能为空"));
    }
    Ok(trimmed)
}

fn dedupe_skill_ids(skill_ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for skill_id in skill_ids {
        let trimmed = skill_id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
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
    name: String,
    summary: String,
    description: String,
    system_prompt: String,
    skill_ids: Vec<String>,
    default_provider_id: String,
    default_model: String,
    execution_mode: String,
    collaboration_config: Option<AgentCollaborationConfig>,
    accent_color: Option<String>,
    bot_configs: HashMap<String, AgentBotConfig>,
    heartbeat_config: AgentHeartbeatConfig,
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
                system_prompt,
                default_provider_id,
                default_model,
                is_builtin,
                is_archived,
                execution_mode,
                collaboration_config_json,
                heartbeat_config_json,
                accent_color,
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
                system_prompt: row.get(4)?,
                skill_ids: Vec::new(),
                default_provider_id: row.get(5)?,
                default_model: row.get(6)?,
                is_builtin: row.get::<_, i64>(7)? != 0,
                is_archived: row.get::<_, i64>(8)? != 0,
                execution_mode: row.get(9)?,
                collaboration_config: deserialize_collaboration_config(row.get(10)?),
                heartbeat_config: deserialize_heartbeat_config(row.get(11)?),
                accent_color: row.get(12)?,
                bot_configs: HashMap::new(),
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .map_err(|error| format!("读取同步用 agent 数据失败: {error}"))?;

    let mut agents = Vec::new();
    for row in rows {
        agents.push(row.map_err(|error| format!("解析同步用 agent 数据失败: {error}"))?);
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
                name: "自定义助理".to_string(),
                summary: "负责综合处理".to_string(),
                description: "更完整的说明".to_string(),
                system_prompt: "先确认，再执行".to_string(),
                skill_ids: vec!["alpha".to_string(), "alpha".to_string(), "beta".to_string()],
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: Some("single".to_string()),
                collaboration_config: None,
                accent_color: Some("#112233".to_string()),
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig::default(),
            },
        )
        .expect("create agent");

        assert_eq!(
            created.skill_ids,
            vec!["alpha".to_string(), "beta".to_string()]
        );
        assert_eq!(created.name, "自定义助理");

        let updated = update_agent_with_connection(
            &mut connection,
            &created.id,
            AgentInput {
                name: "自定义助理 v2".to_string(),
                summary: "负责复杂处理".to_string(),
                description: "更新后的说明".to_string(),
                system_prompt: "".to_string(),
                skill_ids: vec!["gamma".to_string()],
                default_provider_id: "anthropic".to_string(),
                default_model: "claude-sonnet-4-0".to_string(),
                execution_mode: Some("supervisor".to_string()),
                collaboration_config: Some(AgentCollaborationConfig {
                    allowed_delegate_agent_ids: vec!["worker-1".to_string()],
                    handoff_prompt: "只在必要时委派".to_string(),
                    shared_context_policy: "summary".to_string(),
                }),
                accent_color: None,
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig::default(),
            },
        )
        .expect("update agent");

        assert_eq!(updated.name, "自定义助理 v2");
        assert_eq!(updated.skill_ids, vec!["gamma".to_string()]);
        assert_eq!(updated.execution_mode, "supervisor");
        assert_eq!(
            updated
                .collaboration_config
                .as_ref()
                .map(|item| item.shared_context_policy.as_str()),
            Some("summary")
        );
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
                name: "项目助理".to_string(),
                summary: "项目推进".to_string(),
                description: "负责项目推进".to_string(),
                system_prompt: "".to_string(),
                skill_ids: vec![],
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: Some("single".to_string()),
                collaboration_config: None,
                accent_color: None,
                bot_configs: HashMap::new(),
                heartbeat_config: AgentHeartbeatConfig::default(),
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
            system_prompt: "避免省略关键确认步骤".to_string(),
            skill_ids: vec!["alpha".to_string(), "beta".to_string()],
            default_provider_id: "openai".to_string(),
            default_model: "gpt-4.1".to_string(),
            execution_mode: "single".to_string(),
            collaboration_config: None,
            accent_color: None,
        })
        .expect("prompt");

        assert!(prompt.contains("项目助理"));
        assert!(prompt.contains("负责项目推进"));
        assert!(prompt.contains("已挂载技能"));
        assert!(prompt.contains("避免省略关键确认步骤"));
    }

    #[test]
    fn create_agent_persists_heartbeat_config() {
        let mut connection = connection();

        let created = create_agent_with_connection(
            &mut connection,
            AgentInput {
                name: "提醒助理".to_string(),
                summary: "会定时提醒".to_string(),
                description: "负责晨会提醒和日报抓取".to_string(),
                system_prompt: "".to_string(),
                skill_ids: vec![],
                default_provider_id: "openai".to_string(),
                default_model: "gpt-4.1".to_string(),
                execution_mode: Some("single".to_string()),
                collaboration_config: None,
                accent_color: None,
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
