use chrono::{DateTime, Local, Timelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::agents::{self, ConversationAgentConfig};

const STATUS_DRAFT: &str = "draft";
const STATUS_ACTIVE: &str = "active";
const STATUS_PAUSED: &str = "paused";
const STATUS_DELETED: &str = "deleted";
const TASK_TYPE_REMINDER: &str = "reminder";
const TASK_TYPE_AGENT_PROMPT: &str = "agent_prompt";
const SCHEDULE_TYPE_INTERVAL: &str = "interval";
const SCHEDULE_TYPE_DAILY_TIME: &str = "daily_time";
pub const SCHEDULE_TYPE_ONCE_AT: &str = "once_at";
const DELIVERY_KIND_DESKTOP: &str = "desktop_session";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskRecord {
    pub id: String,
    pub agent_id: String,
    pub source_session_id: String,
    pub creator_user_id: String,
    pub title: String,
    pub intent_summary: String,
    pub task_type: String,
    pub schedule_type: String,
    pub timezone: String,
    pub payload_json: String,
    pub schedule_json: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskListItem {
    pub id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub source_session_id: String,
    pub title: String,
    pub intent_summary: String,
    pub task_type: String,
    pub schedule_type: String,
    pub timezone: String,
    pub goal: String,
    pub interval_minutes: Option<i64>,
    pub daily_times: Vec<String>,
    #[serde(default)]
    pub run_at_ms: Option<i64>,
    #[serde(default)]
    pub result_in_new_session: bool,
    pub status: String,
    pub next_run_at: Option<i64>,
    pub last_run_at: Option<i64>,
    pub delivery_kind: String,
    pub delivery_target: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskUpdateInput {
    pub title: String,
    pub goal: String,
    pub schedule_type: String,
    pub timezone: String,
    pub interval_minutes: Option<i64>,
    pub daily_times: Vec<String>,
    #[serde(default)]
    pub run_at_ms: Option<i64>,
    #[serde(default)]
    pub result_in_new_session: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskDeliveryRecord {
    pub id: String,
    pub task_id: String,
    pub run_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub title: String,
    pub content: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<ConversationAgentConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskPromptResult {
    pub handled: bool,
    pub needs_clarification: bool,
    pub assistant_message: String,
    pub task_id: Option<String>,
    pub draft_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskDeliveryTarget {
    pub kind: String,
    pub session_id: String,
    #[serde(default)]
    pub result_in_new_session: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskPayload {
    pub goal: String,
    pub reminder_text: String,
    pub prompt_template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntervalSchedule {
    pub every_minutes: i64,
    pub start_at: Option<i64>,
    pub end_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyTimeSchedule {
    pub times: Vec<String>,
    pub days_of_week: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnceAtSchedule {
    pub run_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AgentTaskDraftData {
    title: String,
    intent_summary: String,
    task_type: String,
    schedule_type: String,
    timezone: String,
    interval_minutes: Option<i64>,
    daily_times: Vec<String>,
    #[serde(default)]
    run_at_ms: Option<i64>,
    #[serde(default)]
    result_in_new_session: bool,
    goal: String,
}

#[derive(Debug, Clone)]
pub struct SchedulerTaskDefinition {
    pub task: AgentTaskRecord,
    pub delivery: AgentTaskDeliveryTarget,
}

#[derive(Debug, Clone)]
struct AgentTaskDraftRow {
    id: String,
    draft_json: String,
}

pub fn ensure_agent_task_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_tasks (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                source_session_id TEXT NOT NULL,
                creator_user_id TEXT NOT NULL,
                title TEXT NOT NULL,
                intent_summary TEXT NOT NULL,
                task_type TEXT NOT NULL,
                schedule_type TEXT NOT NULL,
                timezone TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                schedule_json TEXT NOT NULL,
                delivery_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_tasks_agent_status
                ON agent_tasks(agent_id, status, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_agent_tasks_session_status
                ON agent_tasks(source_session_id, status, updated_at DESC);
            CREATE TABLE IF NOT EXISTS agent_task_drafts (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                source_session_id TEXT NOT NULL,
                draft_json TEXT NOT NULL,
                missing_fields_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(agent_id, source_session_id)
            );
            CREATE TABLE IF NOT EXISTS agent_task_deliveries (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                run_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                delivered_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_agent_task_deliveries_session
                ON agent_task_deliveries(session_id, delivered_at, created_at DESC);",
        )
        .map_err(|error| format!("初始化 agent task 数据表失败: {error}"))?;
    ensure_agent_task_deliveries_agent_column(connection)?;
    Ok(())
}

fn ensure_agent_task_deliveries_agent_column(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA table_info(agent_task_deliveries)")
        .map_err(|error| format!("读取 agent_task_deliveries 列信息失败: {error}"))?;
    let mut has_column = false;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("遍历 agent_task_deliveries 列失败: {error}"))?;
    for name in rows {
        let name = name.map_err(|error| format!("读取列名失败: {error}"))?;
        if name == "agent_snapshot_json" {
            has_column = true;
            break;
        }
    }
    if !has_column {
        connection
            .execute(
                "ALTER TABLE agent_task_deliveries ADD COLUMN agent_snapshot_json TEXT",
                [],
            )
            .map_err(|error| format!("迁移 agent_task_deliveries 失败: {error}"))?;
    }
    Ok(())
}

/// 将已过期的一次性任务自动暂停，避免列表中长期显示「运行中」却永不触发。
pub fn pause_expired_once_at_tasks(connection: &mut Connection) -> Result<(), String> {
    ensure_agent_task_schema(connection)?;
    let now = crate::chrono_like_timestamp();
    let mut statement = connection
        .prepare(
            "SELECT id, schedule_json FROM agent_tasks
             WHERE status = ?1 AND schedule_type = ?2",
        )
        .map_err(|error| format!("查询一次性任务失败: {error}"))?;
    let rows = statement
        .query_map(params![STATUS_ACTIVE, SCHEDULE_TYPE_ONCE_AT], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("读取一次性任务行失败: {error}"))?;

    for row in rows {
        let (id, schedule_json) = row.map_err(|error| format!("解析一次性任务行失败: {error}"))?;
        if let Ok(schedule) = serde_json::from_str::<OnceAtSchedule>(&schedule_json) {
            if schedule.run_at_ms <= now {
                connection
                    .execute(
                        "UPDATE agent_tasks SET status = ?2, updated_at = ?3 WHERE id = ?1",
                        params![id, STATUS_PAUSED, now],
                    )
                    .map_err(|error| format!("暂停过期一次性任务失败: {error}"))?;
            }
        }
    }
    Ok(())
}

pub fn handle_prompt(
    app: &tauri::AppHandle,
    prompt: &str,
    session_id: &str,
    agent_id: &str,
) -> Result<AgentTaskPromptResult, String> {
    let trimmed_prompt = prompt.trim();
    if trimmed_prompt.is_empty() || agent_id.trim().is_empty() || session_id.trim().is_empty() {
        return Ok(AgentTaskPromptResult {
            handled: false,
            needs_clarification: false,
            assistant_message: String::new(),
            task_id: None,
            draft_id: None,
        });
    }

    let mut connection = crate::open_history_db(app)?;
    ensure_agent_task_schema(&connection)?;
    let active_draft = load_active_draft(&connection, agent_id, session_id)?;
    let has_existing_draft = active_draft.is_some();
    if !has_existing_draft && !looks_like_task_request(trimmed_prompt) {
        return Ok(AgentTaskPromptResult {
            handled: false,
            needs_clarification: false,
            assistant_message: String::new(),
            task_id: None,
            draft_id: None,
        });
    }

    let base_draft = active_draft
        .as_ref()
        .and_then(|row| serde_json::from_str::<AgentTaskDraftData>(&row.draft_json).ok())
        .unwrap_or_default();
    let mut merged = merge_prompt_into_draft(base_draft, trimmed_prompt);
    let missing_fields = collect_missing_fields(&merged);
    let now = crate::chrono_like_timestamp();

    if !missing_fields.is_empty() {
        let draft_id = active_draft
            .as_ref()
            .map(|item| item.id.clone())
            .unwrap_or_else(|| format!("draft_{}", uuid::Uuid::new_v4().simple()));
        upsert_draft(
            &mut connection,
            &draft_id,
            agent_id,
            session_id,
            &merged,
            &missing_fields,
            now,
        )?;
        return Ok(AgentTaskPromptResult {
            handled: true,
            needs_clarification: true,
            assistant_message: build_follow_up_question(&merged, &missing_fields),
            task_id: None,
            draft_id: Some(draft_id),
        });
    }

    if let Some(existing) = active_draft {
        delete_draft(&connection, &existing.id)?;
    }

    let schedule_hint = format_schedule_hint_for_llm(&merged);
    if let Some((t, s)) = crate::refine_agent_task_metadata(
        app,
        agent_id,
        &merged.goal,
        &merged.task_type,
        &schedule_hint,
    ) {
        merged.title = t;
        merged.intent_summary = s;
    } else {
        merged.title = build_title(&merged.goal, &merged.task_type);
        merged.intent_summary = summarize_goal_fallback(&merged.goal);
    }

    let task_id = format!("agent_task_{}", uuid::Uuid::new_v4().simple());
    let record = build_task_record(&task_id, agent_id, session_id, &merged, now)?;
    insert_task(&mut connection, &record, &merged)?;
    let _ = crate::scheduler::sync_materialized_jobs(app);

    Ok(AgentTaskPromptResult {
        handled: true,
        needs_clarification: false,
        assistant_message: build_created_message(&record, &merged),
        task_id: Some(task_id),
        draft_id: None,
    })
}

pub fn list_delivery_records(
    app: &tauri::AppHandle,
    session_ids: &[String],
) -> Result<Vec<AgentTaskDeliveryRecord>, String> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }

    let connection = crate::open_history_db(app)?;
    ensure_agent_task_schema(&connection)?;

    let placeholders = std::iter::repeat("?")
        .take(session_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, task_id, run_id, agent_id, session_id, title, content, created_at, agent_snapshot_json
         FROM agent_task_deliveries
         WHERE delivered_at IS NULL
           AND session_id IN ({placeholders})
         ORDER BY created_at ASC"
    );
    let values = session_ids
        .iter()
        .cloned()
        .map(rusqlite::types::Value::from);
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("读取 task delivery 失败: {error}"))?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(values), |row| {
            let agent_json: Option<String> = row.get(8)?;
            let agent = agent_json
                .as_deref()
                .and_then(|text| serde_json::from_str::<ConversationAgentConfig>(text).ok());
            Ok(AgentTaskDeliveryRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                run_id: row.get(2)?,
                agent_id: row.get(3)?,
                session_id: row.get(4)?,
                title: row.get(5)?,
                content: row.get(6)?,
                created_at: row.get(7)?,
                agent,
            })
        })
        .map_err(|error| format!("解析 task delivery 失败: {error}"))?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|error| format!("读取 task delivery 行失败: {error}"))?);
    }

    if !records.is_empty() {
        let ids = records
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        mark_deliveries_as_seen(&connection, &ids)?;
    }

    Ok(records)
}

pub fn list_tasks(
    app: &tauri::AppHandle,
    agent_id: Option<&str>,
) -> Result<Vec<AgentTaskListItem>, String> {
    let connection = crate::open_history_db(app)?;
    ensure_agent_task_schema(&connection)?;
    crate::scheduler::ensure_scheduler_schema(&connection)?;

    let mut sql = String::from(
        "SELECT
            t.id,
            t.agent_id,
            COALESCE(a.name, t.agent_id) AS agent_name,
            t.source_session_id,
            t.title,
            t.intent_summary,
            t.task_type,
            t.schedule_type,
            t.timezone,
            t.payload_json,
            t.schedule_json,
            t.status,
            MIN(j.next_run_at) AS next_run_at,
            MAX(j.last_run_at) AS last_run_at,
            t.delivery_json,
            t.created_at,
            t.updated_at
         FROM agent_tasks t
         LEFT JOIN agents a
           ON a.id = t.agent_id
         LEFT JOIN scheduled_jobs j
           ON j.source_kind = 'agent_task'
          AND j.source_schedule_id = t.id",
    );
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    let trimmed_agent_id = agent_id.map(str::trim).filter(|value| !value.is_empty());
    if trimmed_agent_id.is_some() {
        sql.push_str(" WHERE t.agent_id = ?1");
        values.push(rusqlite::types::Value::from(
            trimmed_agent_id.unwrap().to_string(),
        ));
    }
    sql.push_str(
        " GROUP BY
            t.id, t.agent_id, a.name, t.source_session_id, t.title, t.intent_summary,
            t.task_type, t.schedule_type, t.timezone, t.payload_json, t.schedule_json, t.status, t.delivery_json,
            t.created_at, t.updated_at
          ORDER BY t.updated_at DESC, lower(t.title) ASC",
    );

    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("读取 agent tasks 列表失败: {error}"))?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(values), |row| {
            let payload_json: String = row.get(9)?;
            let payload =
                serde_json::from_str::<AgentTaskPayload>(&payload_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        9,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
                    )
                })?;
            let schedule_json: String = row.get(10)?;
            let schedule_type: String = row.get(7)?;
            let (interval_minutes, daily_times, run_at_ms) = match schedule_type.as_str() {
                SCHEDULE_TYPE_INTERVAL => {
                    let schedule =
                        serde_json::from_str::<IntervalSchedule>(&schedule_json).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                10,
                                rusqlite::types::Type::Text,
                                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
                            )
                        })?;
                    (Some(schedule.every_minutes), Vec::new(), None)
                }
                SCHEDULE_TYPE_ONCE_AT => {
                    let schedule =
                        serde_json::from_str::<OnceAtSchedule>(&schedule_json).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                10,
                                rusqlite::types::Type::Text,
                                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
                            )
                        })?;
                    (None, Vec::new(), Some(schedule.run_at_ms))
                }
                _ => {
                    let schedule =
                        serde_json::from_str::<DailyTimeSchedule>(&schedule_json).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                10,
                                rusqlite::types::Type::Text,
                                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
                            )
                        })?;
                    (None, schedule.times, None)
                }
            };
            let delivery_json: String = row.get(14)?;
            let delivery = serde_json::from_str::<AgentTaskDeliveryTarget>(&delivery_json)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        14,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
                    )
                })?;
            Ok(AgentTaskListItem {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                agent_name: row.get(2)?,
                source_session_id: row.get(3)?,
                title: row.get(4)?,
                intent_summary: row.get(5)?,
                task_type: row.get(6)?,
                schedule_type,
                timezone: row.get(8)?,
                goal: payload.goal,
                interval_minutes,
                daily_times,
                run_at_ms,
                result_in_new_session: delivery.result_in_new_session,
                status: row.get(11)?,
                next_run_at: row.get(12)?,
                last_run_at: row.get(13)?,
                delivery_kind: delivery.kind,
                delivery_target: delivery.session_id,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
            })
        })
        .map_err(|error| format!("解析 agent tasks 列表失败: {error}"))?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|error| format!("读取 agent tasks 列表行失败: {error}"))?);
    }
    Ok(items)
}

pub fn pause_task(app: &tauri::AppHandle, task_id: &str) -> Result<(), String> {
    update_task_status(app, task_id, STATUS_PAUSED)
}

pub fn resume_task(app: &tauri::AppHandle, task_id: &str) -> Result<(), String> {
    update_task_status(app, task_id, STATUS_ACTIVE)
}

pub fn delete_task(app: &tauri::AppHandle, task_id: &str) -> Result<(), String> {
    update_task_status(app, task_id, STATUS_DELETED)
}

pub fn update_task(
    app: &tauri::AppHandle,
    task_id: &str,
    input: &AgentTaskUpdateInput,
) -> Result<(), String> {
    let connection = crate::open_history_db(app)?;
    ensure_agent_task_schema(&connection)?;
    let existing = load_task_record(&connection, task_id)?
        .ok_or_else(|| "未找到对应的定时任务".to_string())?;
    if existing.status == STATUS_DELETED {
        return Err("已删除的任务不能再编辑".to_string());
    }

    let goal = input.goal.trim();
    if goal.is_empty() {
        return Err("任务内容不能为空".to_string());
    }

    let schedule_type = input.schedule_type.trim();
    if schedule_type != SCHEDULE_TYPE_INTERVAL
        && schedule_type != SCHEDULE_TYPE_DAILY_TIME
        && schedule_type != SCHEDULE_TYPE_ONCE_AT
    {
        return Err("暂不支持这种触发方式".to_string());
    }

    let timezone = if input.timezone.trim().is_empty() {
        "Asia/Shanghai".to_string()
    } else {
        input.timezone.trim().to_string()
    };

    let now = crate::chrono_like_timestamp();

    let schedule_json = if schedule_type == SCHEDULE_TYPE_INTERVAL {
        let every_minutes = input.interval_minutes.unwrap_or(0);
        if every_minutes <= 0 {
            return Err("间隔任务至少需要大于 0 的分钟数".to_string());
        }
        serde_json::to_string(&IntervalSchedule {
            every_minutes: every_minutes.clamp(1, 24 * 60),
            start_at: Some(now),
            end_at: None,
        })
        .map_err(|error| format!("序列化 interval schedule 失败: {error}"))?
    } else if schedule_type == SCHEDULE_TYPE_ONCE_AT {
        let run_at = input.run_at_ms.ok_or_else(|| "请指定一次性任务的执行时间".to_string())?;
        if run_at <= now {
            return Err("一次性任务的执行时间须晚于当前时间".to_string());
        }
        serde_json::to_string(&OnceAtSchedule { run_at_ms: run_at })
            .map_err(|error| format!("序列化 once_at schedule 失败: {error}"))?
    } else {
        let mut times = input
            .daily_times
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        times.sort();
        times.dedup();
        if times.is_empty() {
            return Err("每天定时任务至少需要一个触发时间".to_string());
        }
        serde_json::to_string(&DailyTimeSchedule {
            times,
            days_of_week: vec![1, 2, 3, 4, 5, 6, 7],
        })
        .map_err(|error| format!("序列化 daily schedule 失败: {error}"))?
    };

    let existing_delivery_json: String = connection
        .query_row(
            "SELECT delivery_json FROM agent_tasks WHERE id = ?1",
            params![task_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("读取任务投递配置失败: {error}"))?;
    let mut delivery: AgentTaskDeliveryTarget =
        serde_json::from_str(&existing_delivery_json)
            .map_err(|error| format!("解析任务投递配置失败: {error}"))?;
    let want_new_session = input
        .result_in_new_session
        .unwrap_or(delivery.result_in_new_session);
    if want_new_session {
        if delivery.session_id == existing.source_session_id {
            delivery.session_id = format!("task_sess_{}", uuid::Uuid::new_v4().simple());
        }
    } else {
        delivery.session_id = existing.source_session_id.clone();
    }
    delivery.result_in_new_session = want_new_session;
    let delivery_json = serde_json::to_string(&delivery)
        .map_err(|error| format!("序列化任务投递配置失败: {error}"))?;

    let payload_json = serde_json::to_string(&AgentTaskPayload {
        goal: goal.to_string(),
        reminder_text: if existing.task_type == TASK_TYPE_REMINDER {
            goal.to_string()
        } else {
            String::new()
        },
        prompt_template: if existing.task_type == TASK_TYPE_AGENT_PROMPT {
            goal.to_string()
        } else {
            String::new()
        },
    })
    .map_err(|error| format!("序列化 task payload 失败: {error}"))?;

    let schedule_hint = format_schedule_hint_for_update(input, schedule_type);
    let (title, intent_summary) =
        match crate::refine_agent_task_metadata(app, &existing.agent_id, goal, &existing.task_type, &schedule_hint)
        {
            Some((t, s)) => {
                let title = if input.title.trim().is_empty() {
                    t
                } else {
                    input.title.trim().to_string()
                };
                (title, s)
            }
            None => {
                let title = if input.title.trim().is_empty() {
                    build_title(goal, &existing.task_type)
                } else {
                    input.title.trim().to_string()
                };
                (title, summarize_goal_fallback(goal))
            }
        };
    connection
        .execute(
            "UPDATE agent_tasks
             SET title = ?2,
                 intent_summary = ?3,
                 schedule_type = ?4,
                 timezone = ?5,
                 payload_json = ?6,
                 schedule_json = ?7,
                 delivery_json = ?8,
                 updated_at = ?9
             WHERE id = ?1",
            params![
                task_id,
                title,
                intent_summary,
                schedule_type,
                timezone,
                payload_json,
                schedule_json,
                delivery_json,
                now
            ],
        )
        .map_err(|error| format!("更新定时任务失败: {error}"))?;
    let _ = crate::scheduler::sync_materialized_jobs(app);
    Ok(())
}

pub fn record_delivery(
    app: &tauri::AppHandle,
    connection: &Connection,
    task_id: &str,
    run_id: &str,
    agent_id: &str,
    session_id: &str,
    title: &str,
    content: &str,
) -> Result<AgentTaskDeliveryRecord, String> {
    ensure_agent_task_schema(connection)?;
    let now = crate::chrono_like_timestamp();
    let id = format!("delivery_{}", uuid::Uuid::new_v4().simple());
    let agent_snapshot = agents::get_conversation_agent_config(app, agent_id).ok().flatten();
    let agent_json = agent_snapshot
        .as_ref()
        .and_then(|cfg| serde_json::to_string(cfg).ok());
    connection
        .execute(
            "INSERT INTO agent_task_deliveries (
                id, task_id, run_id, agent_id, session_id, title, content, created_at, agent_snapshot_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id.clone(),
                task_id,
                run_id,
                agent_id,
                session_id,
                title,
                content,
                now,
                agent_json
            ],
        )
        .map_err(|error| format!("写入 task delivery 失败: {error}"))?;
    Ok(AgentTaskDeliveryRecord {
        id,
        task_id: task_id.to_string(),
        run_id: run_id.to_string(),
        agent_id: agent_id.to_string(),
        session_id: session_id.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        created_at: now,
        agent: agent_snapshot,
    })
}

pub fn list_active_tasks_for_scheduler(
    connection: &Connection,
) -> Result<Vec<SchedulerTaskDefinition>, String> {
    ensure_agent_task_schema(connection)?;
    let mut statement = connection
        .prepare(
            "SELECT
                id, agent_id, source_session_id, creator_user_id, title, intent_summary,
                task_type, schedule_type, timezone, payload_json, schedule_json, delivery_json,
                status, created_at, updated_at
             FROM agent_tasks
             WHERE status = ?1",
        )
        .map_err(|error| format!("读取 agent tasks 失败: {error}"))?;
    let rows = statement
        .query_map(params![STATUS_ACTIVE], |row| {
            let task = AgentTaskRecord {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                source_session_id: row.get(2)?,
                creator_user_id: row.get(3)?,
                title: row.get(4)?,
                intent_summary: row.get(5)?,
                task_type: row.get(6)?,
                schedule_type: row.get(7)?,
                timezone: row.get(8)?,
                payload_json: row.get(9)?,
                schedule_json: row.get(10)?,
                status: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            };
            let delivery_json: String = row.get(11)?;
            let delivery = serde_json::from_str::<AgentTaskDeliveryTarget>(&delivery_json)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        11,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
                    )
                })?;
            Ok(SchedulerTaskDefinition { task, delivery })
        })
        .map_err(|error| format!("解析 agent tasks 失败: {error}"))?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|error| format!("读取 agent tasks 行失败: {error}"))?);
    }
    Ok(items)
}

fn update_task_status(app: &tauri::AppHandle, task_id: &str, status: &str) -> Result<(), String> {
    let connection = crate::open_history_db(app)?;
    ensure_agent_task_schema(&connection)?;
    let now = crate::chrono_like_timestamp();
    let updated = connection
        .execute(
            "UPDATE agent_tasks
             SET status = ?2, updated_at = ?3
             WHERE id = ?1",
            params![task_id, status, now],
        )
        .map_err(|error| format!("更新 agent task 状态失败: {error}"))?;
    if updated == 0 {
        return Err("未找到对应的定时任务".to_string());
    }
    let _ = crate::scheduler::sync_materialized_jobs(app);
    Ok(())
}

fn load_task_record(
    connection: &Connection,
    task_id: &str,
) -> Result<Option<AgentTaskRecord>, String> {
    ensure_agent_task_schema(connection)?;
    connection
        .query_row(
            "SELECT
                id, agent_id, source_session_id, creator_user_id, title, intent_summary,
                task_type, schedule_type, timezone, payload_json, schedule_json, status,
                created_at, updated_at
             FROM agent_tasks
             WHERE id = ?1",
            params![task_id],
            |row| {
                Ok(AgentTaskRecord {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    source_session_id: row.get(2)?,
                    creator_user_id: row.get(3)?,
                    title: row.get(4)?,
                    intent_summary: row.get(5)?,
                    task_type: row.get(6)?,
                    schedule_type: row.get(7)?,
                    timezone: row.get(8)?,
                    payload_json: row.get(9)?,
                    schedule_json: row.get(10)?,
                    status: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取定时任务失败: {error}"))
}

fn looks_like_task_request(prompt: &str) -> bool {
    let text = prompt.trim();
    if text.is_empty() {
        return false;
    }
    [
        "提醒我",
        "定时",
        "每隔",
        "每 ",
        "每",
        "每天",
        "每日",
        "分钟",
        "小时",
        "早上",
        "晚上",
        "下午",
        "上午",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn merge_prompt_into_draft(mut draft: AgentTaskDraftData, prompt: &str) -> AgentTaskDraftData {
    let schedule_type = detect_schedule_type(prompt);
    if !schedule_type.is_empty() {
        draft.schedule_type = schedule_type.clone();
    }
    if draft.timezone.trim().is_empty() {
        draft.timezone = "Asia/Shanghai".to_string();
    }

    match draft.schedule_type.as_str() {
        SCHEDULE_TYPE_INTERVAL => {
            if let Some(minutes) = extract_interval_minutes(prompt) {
                draft.interval_minutes = Some(minutes.clamp(1, 24 * 60));
            }
        }
        SCHEDULE_TYPE_DAILY_TIME => {
            let times = extract_daily_times(prompt);
            if !times.is_empty() {
                let mut merged = draft.daily_times.clone();
                merged.extend(times);
                let mut seen = HashSet::new();
                merged.retain(|item| seen.insert(item.clone()));
                merged.sort();
                draft.daily_times = merged;
            }
        }
        SCHEDULE_TYPE_ONCE_AT => {}
        _ => {}
    }

    let goal = extract_goal(prompt);
    if !goal.is_empty() {
        draft.goal = goal.clone();
        draft.intent_summary = goal.clone();
        draft.task_type = if prompt.contains("提醒") {
            TASK_TYPE_REMINDER.to_string()
        } else {
            TASK_TYPE_AGENT_PROMPT.to_string()
        };
        draft.title = build_title(&goal, &draft.task_type);
    }

    draft
}

fn collect_missing_fields(draft: &AgentTaskDraftData) -> Vec<String> {
    let mut items = Vec::new();
    if draft.goal.trim().is_empty() {
        items.push("task_goal".to_string());
    }
    if draft.schedule_type.trim().is_empty() {
        items.push("schedule_type".to_string());
    } else if draft.schedule_type == SCHEDULE_TYPE_INTERVAL
        && draft.interval_minutes.unwrap_or(0) <= 0
    {
        items.push("interval_minutes".to_string());
    } else if draft.schedule_type == SCHEDULE_TYPE_DAILY_TIME && draft.daily_times.is_empty() {
        items.push("daily_times".to_string());
    } else if draft.schedule_type == SCHEDULE_TYPE_ONCE_AT {
        let now = crate::chrono_like_timestamp();
        if draft.run_at_ms.map(|ms| ms <= now).unwrap_or(true) {
            items.push("run_at_ms".to_string());
        }
    }
    items
}

fn build_follow_up_question(draft: &AgentTaskDraftData, missing_fields: &[String]) -> String {
    let missing = missing_fields
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if missing.contains("task_goal") && missing.contains("schedule_type") {
        return "想让这个智能体定时做什么？触发方式是“每隔几分钟/几小时”“每天几点”，还是“指定某个时间只执行一次”？"
            .to_string();
    }
    if missing.contains("task_goal") {
        return "这条定时任务具体要做什么？直接告诉我任务内容就行。".to_string();
    }
    if missing.contains("schedule_type") {
        return format!(
            "任务内容我记下了：{}。触发方式是“每隔几分钟/几小时”“每天几点”，还是“指定某个时间只执行一次”？",
            draft.goal.trim()
        );
    }
    if missing.contains("interval_minutes") {
        return format!(
            "我会让智能体定时执行“{}”。请补充具体间隔，比如“每 10 分钟”或“每 2 小时”。",
            draft.goal.trim()
        );
    }
    if missing.contains("daily_times") {
        return format!(
            "我会让智能体每天定时执行“{}”。请补充具体时间，比如“每天 09:00”或“每天早上 9 点”。",
            draft.goal.trim()
        );
    }
    if missing.contains("run_at_ms") {
        return format!(
            "我会让智能体在指定时间只执行一次“{}”。请说明具体的日期和时间（例如明天 15:30），或在任务中心里选择时间。",
            draft.goal.trim()
        );
    }
    "还差一点信息，请继续补充。".to_string()
}

fn build_task_record(
    task_id: &str,
    agent_id: &str,
    session_id: &str,
    draft: &AgentTaskDraftData,
    now: i64,
) -> Result<AgentTaskRecord, String> {
    let payload = AgentTaskPayload {
        goal: draft.goal.clone(),
        reminder_text: if draft.task_type == TASK_TYPE_REMINDER {
            draft.goal.clone()
        } else {
            String::new()
        },
        prompt_template: if draft.task_type == TASK_TYPE_AGENT_PROMPT {
            draft.goal.clone()
        } else {
            String::new()
        },
    };
    let schedule_json = if draft.schedule_type == SCHEDULE_TYPE_INTERVAL {
        serde_json::to_string(&IntervalSchedule {
            every_minutes: draft.interval_minutes.unwrap_or(10),
            start_at: Some(now),
            end_at: None,
        })
        .map_err(|error| format!("序列化 interval schedule 失败: {error}"))?
    } else if draft.schedule_type == SCHEDULE_TYPE_ONCE_AT {
        let run_at = draft
            .run_at_ms
            .ok_or_else(|| "一次性任务缺少执行时间".to_string())?;
        serde_json::to_string(&OnceAtSchedule { run_at_ms: run_at })
            .map_err(|error| format!("序列化 once_at schedule 失败: {error}"))?
    } else {
        serde_json::to_string(&DailyTimeSchedule {
            times: draft.daily_times.clone(),
            days_of_week: vec![1, 2, 3, 4, 5, 6, 7],
        })
        .map_err(|error| format!("序列化 daily schedule 失败: {error}"))?
    };
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| format!("序列化 task payload 失败: {error}"))?;

    Ok(AgentTaskRecord {
        id: task_id.to_string(),
        agent_id: agent_id.to_string(),
        source_session_id: session_id.to_string(),
        creator_user_id: format!("desktop:{session_id}"),
        title: draft.title.clone(),
        intent_summary: draft.intent_summary.clone(),
        task_type: draft.task_type.clone(),
        schedule_type: draft.schedule_type.clone(),
        timezone: draft.timezone.clone(),
        payload_json,
        schedule_json,
        status: STATUS_ACTIVE.to_string(),
        created_at: now,
        updated_at: now,
    })
}

fn insert_task(
    connection: &mut Connection,
    record: &AgentTaskRecord,
    draft: &AgentTaskDraftData,
) -> Result<(), String> {
    let session_id = if draft.result_in_new_session {
        format!("task_sess_{}", uuid::Uuid::new_v4().simple())
    } else {
        record.source_session_id.clone()
    };
    let delivery = serde_json::to_string(&AgentTaskDeliveryTarget {
        kind: DELIVERY_KIND_DESKTOP.to_string(),
        session_id,
        result_in_new_session: draft.result_in_new_session,
    })
    .map_err(|error| format!("序列化 task delivery 失败: {error}"))?;
    connection
        .execute(
            "INSERT INTO agent_tasks (
                id, agent_id, source_session_id, creator_user_id, title, intent_summary,
                task_type, schedule_type, timezone, payload_json, schedule_json, delivery_json,
                status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                record.id,
                record.agent_id,
                record.source_session_id,
                record.creator_user_id,
                record.title,
                record.intent_summary,
                record.task_type,
                record.schedule_type,
                record.timezone,
                record.payload_json,
                record.schedule_json,
                delivery,
                record.status,
                record.created_at,
                record.updated_at
            ],
        )
        .map_err(|error| format!("写入 agent task 失败: {error}"))?;
    Ok(())
}

fn load_active_draft(
    connection: &Connection,
    agent_id: &str,
    session_id: &str,
) -> Result<Option<AgentTaskDraftRow>, String> {
    ensure_agent_task_schema(connection)?;
    connection
        .query_row(
            "SELECT id, agent_id, source_session_id, draft_json, missing_fields_json
             FROM agent_task_drafts
             WHERE agent_id = ?1 AND source_session_id = ?2 AND status = ?3",
            params![agent_id, session_id, STATUS_DRAFT],
            |row| {
                Ok(AgentTaskDraftRow {
                    id: row.get(0)?,
                    draft_json: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取 task draft 失败: {error}"))
}

fn upsert_draft(
    connection: &mut Connection,
    draft_id: &str,
    agent_id: &str,
    session_id: &str,
    draft: &AgentTaskDraftData,
    missing_fields: &[String],
    now: i64,
) -> Result<(), String> {
    let draft_json =
        serde_json::to_string(draft).map_err(|error| format!("序列化 task draft 失败: {error}"))?;
    let missing_json = serde_json::to_string(missing_fields)
        .map_err(|error| format!("序列化 task draft missing fields 失败: {error}"))?;
    connection
        .execute(
            "INSERT INTO agent_task_drafts (
                id, agent_id, source_session_id, draft_json, missing_fields_json, status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            ON CONFLICT(agent_id, source_session_id) DO UPDATE SET
                draft_json = excluded.draft_json,
                missing_fields_json = excluded.missing_fields_json,
                status = excluded.status,
                updated_at = excluded.updated_at",
            params![
                draft_id,
                agent_id,
                session_id,
                draft_json,
                missing_json,
                STATUS_DRAFT,
                now
            ],
        )
        .map_err(|error| format!("写入 task draft 失败: {error}"))?;
    Ok(())
}

fn delete_draft(connection: &Connection, draft_id: &str) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM agent_task_drafts WHERE id = ?1",
            params![draft_id],
        )
        .map_err(|error| format!("删除 task draft 失败: {error}"))?;
    Ok(())
}

fn format_once_at_label(run_at_ms: i64) -> String {
    match DateTime::<Utc>::from_timestamp_millis(run_at_ms) {
        Some(dt) => dt.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string(),
        None => format!("{run_at_ms}"),
    }
}

fn format_schedule_hint_for_llm(draft: &AgentTaskDraftData) -> String {
    match draft.schedule_type.as_str() {
        SCHEDULE_TYPE_INTERVAL => {
            let m = draft.interval_minutes.unwrap_or(10);
            format!("每 {m} 分钟重复")
        }
        SCHEDULE_TYPE_ONCE_AT => draft
            .run_at_ms
            .map(|ms| format!("一次性，计划时间 {}", format_once_at_label(ms)))
            .unwrap_or_else(|| "一次性定时".to_string()),
        _ => {
            if draft.daily_times.is_empty() {
                "每日定时".to_string()
            } else {
                format!("每日 {}", draft.daily_times.join("、"))
            }
        }
    }
}

fn format_schedule_hint_for_update(input: &AgentTaskUpdateInput, schedule_type: &str) -> String {
    match schedule_type {
        SCHEDULE_TYPE_INTERVAL => format!("每 {} 分钟重复", input.interval_minutes.unwrap_or(10)),
        SCHEDULE_TYPE_ONCE_AT => input
            .run_at_ms
            .map(|ms| format!("一次性，{}", format_once_at_label(ms)))
            .unwrap_or_else(|| "一次性定时".to_string()),
        _ => {
            if input.daily_times.is_empty() {
                "每日定时".to_string()
            } else {
                format!("每日 {}", input.daily_times.join("、"))
            }
        }
    }
}

fn summarize_goal_fallback(goal: &str) -> String {
    let t = goal.trim();
    if t.is_empty() {
        return String::new();
    }
    const MAX: usize = 100;
    if t.chars().count() <= MAX {
        return t.to_string();
    }
    format!("{}…", t.chars().take(MAX).collect::<String>())
}

fn mark_deliveries_as_seen(connection: &Connection, ids: &[String]) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "UPDATE agent_task_deliveries
         SET delivered_at = ?1
         WHERE id IN ({placeholders})"
    );
    let mut values = vec![rusqlite::types::Value::from(crate::chrono_like_timestamp())];
    values.extend(ids.iter().cloned().map(rusqlite::types::Value::from));
    connection
        .execute(&sql, rusqlite::params_from_iter(values))
        .map_err(|error| format!("确认 task delivery 失败: {error}"))?;
    Ok(())
}

fn build_created_message(record: &AgentTaskRecord, draft: &AgentTaskDraftData) -> String {
    let schedule_summary = if record.schedule_type == SCHEDULE_TYPE_INTERVAL {
        format!("每 {} 分钟触发一次", draft.interval_minutes.unwrap_or(10))
    } else if record.schedule_type == SCHEDULE_TYPE_ONCE_AT {
        draft
            .run_at_ms
            .map(|ms| format!("一次性 · {}", format_once_at_label(ms)))
            .unwrap_or_else(|| "一次性定时".to_string())
    } else {
        format!("每天 {}", draft.daily_times.join("、"))
    };
    format!(
        "已经记下这条定时任务。\n\n创建智能体：{}\n任务：{}\n触发：{}\n时区：{}\n后续执行会继续使用这个智能体自己的记忆和技能。",
        record.agent_id, record.title, schedule_summary, record.timezone
    )
}

fn detect_schedule_type(prompt: &str) -> String {
    let text = prompt.trim();
    if text.contains("一次性")
        || text.contains("只执行一次")
        || text.contains("就一次")
        || text.contains("单次")
    {
        return SCHEDULE_TYPE_ONCE_AT.to_string();
    }
    if text.contains("每隔") || text.contains("分钟") || text.contains("小时") {
        if extract_interval_minutes(text).is_some() {
            return SCHEDULE_TYPE_INTERVAL.to_string();
        }
    }
    if text.contains("每天")
        || text.contains("每日")
        || text.contains("早上")
        || text.contains("上午")
        || text.contains("中午")
        || text.contains("下午")
        || text.contains("晚上")
    {
        if !extract_daily_times(text).is_empty() {
            return SCHEDULE_TYPE_DAILY_TIME.to_string();
        }
    }
    String::new()
}

fn extract_goal(prompt: &str) -> String {
    let text = prompt
        .replace('，', " ")
        .replace('。', " ")
        .replace(',', " ")
        .replace('.', " ");
    let mut goal = text.trim().to_string();
    for marker in [
        "每隔", "每 ", "每天", "每日", "早上", "上午", "中午", "下午", "晚上", "分钟", "小时",
        "点半", "点",
    ] {
        if let Some(index) = goal.find(marker) {
            if marker == "分钟" || marker == "小时" || marker == "点半" || marker == "点" {
                let candidate = goal[index + marker.len()..].trim();
                if !candidate.is_empty() {
                    goal = candidate.to_string();
                }
            }
        }
    }
    let fillers = [
        "帮我", "给我", "请", "一下", "定时", "安排", "设置", "创建", "每隔", "每天", "每日",
    ];
    for filler in fillers {
        goal = goal.replace(filler, " ");
    }
    goal = goal.split_whitespace().collect::<Vec<_>>().join(" ");
    goal.trim().to_string()
}

fn build_title(goal: &str, task_type: &str) -> String {
    let prefix = if task_type == TASK_TYPE_REMINDER {
        "提醒"
    } else {
        "定时任务"
    };
    let goal = goal.trim();
    if goal.is_empty() {
        prefix.to_string()
    } else {
        let shortened = goal.chars().take(18).collect::<String>();
        format!("{prefix} · {shortened}")
    }
}

fn extract_interval_minutes(prompt: &str) -> Option<i64> {
    let normalized = prompt.replace('个', "");
    if let Some(index) = normalized.find("分钟") {
        let number = extract_number_near(&normalized[..index])?;
        return Some(number);
    }
    if let Some(index) = normalized.find("小时") {
        let number = extract_number_near(&normalized[..index])?;
        return Some(number * 60);
    }
    None
}

fn extract_number_near(input: &str) -> Option<i64> {
    let trimmed = input.trim();
    let digits = trimmed
        .chars()
        .rev()
        .take_while(|char| char.is_ascii_digit() || is_chinese_number_char(*char))
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    if let Ok(value) = digits.parse::<i64>() {
        return Some(value);
    }
    parse_chinese_number(&digits)
}

fn extract_daily_times(prompt: &str) -> Vec<String> {
    let normalized = prompt.replace('：', ":").replace('点', ":");
    let chars = normalized.chars().collect::<Vec<_>>();
    let mut times = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        if !chars[index].is_ascii_digit() && !is_chinese_number_char(chars[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len()
            && (chars[index].is_ascii_digit() || is_chinese_number_char(chars[index]))
        {
            index += 1;
        }
        let hour_raw = chars[start..index].iter().collect::<String>();
        let Some(hour) = parse_digit_or_chinese(&hour_raw) else {
            continue;
        };
        if index >= chars.len() || chars[index] != ':' {
            continue;
        }
        index += 1;
        let minute = if index < chars.len() && chars[index] == '半' {
            index += 1;
            30
        } else {
            let minute_start = index;
            while index < chars.len()
                && (chars[index].is_ascii_digit() || is_chinese_number_char(chars[index]))
            {
                index += 1;
            }
            if minute_start == index {
                0
            } else {
                let minute_raw = chars[minute_start..index].iter().collect::<String>();
                let Some(parsed_minute) = parse_digit_or_chinese(&minute_raw) else {
                    continue;
                };
                parsed_minute
            }
        };
        if hour > 23 || minute > 59 {
            continue;
        }
        let mut adjusted_hour = hour;
        let prefix = prompt[..prompt.find(&hour_raw).unwrap_or(0)].to_string();
        if prefix.contains("下午") || prefix.contains("晚上") {
            if adjusted_hour < 12 {
                adjusted_hour += 12;
            }
        } else if prefix.contains("凌晨") && adjusted_hour == 12 {
            adjusted_hour = 0;
        }
        times.push(format!("{adjusted_hour:02}:{minute:02}"));
    }
    let mut seen = HashSet::new();
    times.retain(|item| seen.insert(item.clone()));
    times.sort();
    times
}

fn parse_digit_or_chinese(raw: &str) -> Option<u32> {
    if raw.is_empty() {
        return None;
    }
    if let Ok(value) = raw.parse::<u32>() {
        return Some(value);
    }
    parse_chinese_number(raw).map(|value| value as u32)
}

fn parse_chinese_number(raw: &str) -> Option<i64> {
    let mut result = 0_i64;
    let mut current = 0_i64;
    for char in raw.chars() {
        match char {
            '零' | '〇' => {}
            '一' => current += 1,
            '二' | '两' => current += 2,
            '三' => current += 3,
            '四' => current += 4,
            '五' => current += 5,
            '六' => current += 6,
            '七' => current += 7,
            '八' => current += 8,
            '九' => current += 9,
            '十' => {
                current = if current == 0 { 10 } else { current * 10 };
                result += current;
                current = 0;
            }
            _ => return None,
        }
    }
    let value = result + current;
    if value > 0 {
        Some(value)
    } else {
        None
    }
}

fn is_chinese_number_char(char: char) -> bool {
    matches!(
        char,
        '零' | '〇' | '一' | '二' | '两' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
    )
}

pub fn default_desktop_delivery_message(title: &str, content: &str) -> String {
    format!("【{title}】\n{content}")
}

pub fn default_daily_time_for_now() -> String {
    let now: DateTime<Local> = Local::now();
    format!("{:02}:{:02}", now.hour(), now.minute())
}

pub fn is_desktop_delivery_kind(kind: &str) -> bool {
    kind == DELIVERY_KIND_DESKTOP
}
