use chrono::{DateTime, Local, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::agent_task_schedule::{
    build_schedule_hint, normalize_monthly_days, normalize_time_values, normalize_weekly_days,
    DailyTimeSchedule, IntervalSchedule, MonthlyTimeSchedule, OnceAtSchedule, WeeklyTimeSchedule,
    SCHEDULE_TYPE_DAILY_TIME, SCHEDULE_TYPE_INTERVAL, SCHEDULE_TYPE_MONTHLY_TIME,
    SCHEDULE_TYPE_ONCE_AT, SCHEDULE_TYPE_WEEKLY_TIME,
};
use crate::agents::{self, ConversationAgentConfig};

const STATUS_ACTIVE: &str = "active";
const STATUS_PAUSED: &str = "paused";
const STATUS_DELETED: &str = "deleted";
const TASK_TYPE_REMINDER: &str = "reminder";
const TASK_TYPE_AGENT_PROMPT: &str = "agent_prompt";
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
    #[serde(default)]
    pub daily_times: Vec<String>,
    #[serde(default)]
    pub weekly_days: Vec<u32>,
    #[serde(default)]
    pub monthly_days: Vec<u32>,
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
    #[serde(default)]
    pub interval_minutes: Option<i64>,
    #[serde(default)]
    pub daily_times: Vec<String>,
    #[serde(default)]
    pub weekly_days: Vec<u32>,
    #[serde(default)]
    pub monthly_days: Vec<u32>,
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
pub struct AgentTaskDeliveryTarget {
    /// 旧库行可能仅有 sessionId / resultInNewSession，无 kind；与桌面会话投递语义一致。
    #[serde(default = "default_agent_task_delivery_kind")]
    pub kind: String,
    pub session_id: String,
    #[serde(default)]
    pub result_in_new_session: bool,
}

fn default_agent_task_delivery_kind() -> String {
    DELIVERY_KIND_DESKTOP.to_string()
}

/// 兼容旧数据：`sessionId` 缺失或为 snake_case / 调度器 `targetUserId`；`kind` 可为调度器 `channelId`。
fn parse_agent_task_delivery_json(
    raw: &str,
    fallback_source_session_id: &str,
) -> Result<AgentTaskDeliveryTarget, String> {
    use serde_json::Value;

    let mut value: Value =
        serde_json::from_str(raw).map_err(|e| format!("解析 delivery_json 失败: {e}"))?;

    let obj = value
        .as_object_mut()
        .ok_or_else(|| "delivery_json 必须是 JSON 对象".to_string())?;

    if !obj.contains_key("sessionId") {
        let sid = obj
            .remove("session_id")
            .or_else(|| obj.remove("targetUserId"))
            .or_else(|| obj.remove("target_user_id"));
        let sid = sid.or_else(|| {
            if fallback_source_session_id.is_empty() {
                None
            } else {
                Some(Value::String(fallback_source_session_id.to_string()))
            }
        });
        if let Some(v) = sid {
            obj.insert("sessionId".to_string(), v);
        }
    }

    if !obj.contains_key("kind") {
        if let Some(k) = obj.remove("channelId").or_else(|| obj.remove("channel_id")) {
            obj.insert("kind".to_string(), k);
        }
    }

    serde_json::from_value(value).map_err(|e| format!("delivery_json 与投递格式不匹配: {e}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskPayload {
    pub goal: String,
    pub reminder_text: String,
    pub prompt_template: String,
}

#[derive(Debug, Clone)]
pub struct SchedulerTaskDefinition {
    pub task: AgentTaskRecord,
    pub delivery: AgentTaskDeliveryTarget,
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
        sql.push_str(" WHERE t.agent_id = ?1 AND t.status != 'deleted'");
        values.push(rusqlite::types::Value::from(
            trimmed_agent_id.unwrap().to_string(),
        ));
    } else {
        sql.push_str(" WHERE t.status != 'deleted'");
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
            let (interval_minutes, daily_times, weekly_days, monthly_days, run_at_ms) =
                match schedule_type.as_str() {
                    SCHEDULE_TYPE_INTERVAL => {
                        let schedule = serde_json::from_str::<IntervalSchedule>(&schedule_json)
                            .map_err(|error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    10,
                                    rusqlite::types::Type::Text,
                                    Box::new(std::io::Error::new(
                                        std::io::ErrorKind::InvalidData,
                                        error,
                                    )),
                                )
                            })?;
                        (
                            Some(schedule.every_minutes),
                            Vec::new(),
                            Vec::new(),
                            Vec::new(),
                            None,
                        )
                    }
                    SCHEDULE_TYPE_ONCE_AT => {
                        let schedule = serde_json::from_str::<OnceAtSchedule>(&schedule_json)
                            .map_err(|error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    10,
                                    rusqlite::types::Type::Text,
                                    Box::new(std::io::Error::new(
                                        std::io::ErrorKind::InvalidData,
                                        error,
                                    )),
                                )
                            })?;
                        (
                            None,
                            Vec::new(),
                            Vec::new(),
                            Vec::new(),
                            Some(schedule.run_at_ms),
                        )
                    }
                    SCHEDULE_TYPE_WEEKLY_TIME => {
                        let schedule = serde_json::from_str::<WeeklyTimeSchedule>(&schedule_json)
                            .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                10,
                                rusqlite::types::Type::Text,
                                Box::new(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    error,
                                )),
                            )
                        })?;
                        (
                            None,
                            normalize_time_values(&schedule.times),
                            normalize_weekly_days(&schedule.days_of_week),
                            Vec::new(),
                            None,
                        )
                    }
                    SCHEDULE_TYPE_MONTHLY_TIME => {
                        let schedule = serde_json::from_str::<MonthlyTimeSchedule>(&schedule_json)
                            .map_err(|error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    10,
                                    rusqlite::types::Type::Text,
                                    Box::new(std::io::Error::new(
                                        std::io::ErrorKind::InvalidData,
                                        error,
                                    )),
                                )
                            })?;
                        (
                            None,
                            normalize_time_values(&schedule.times),
                            Vec::new(),
                            normalize_monthly_days(&schedule.days_of_month),
                            None,
                        )
                    }
                    _ => {
                        let schedule = serde_json::from_str::<DailyTimeSchedule>(&schedule_json)
                            .map_err(|error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    10,
                                    rusqlite::types::Type::Text,
                                    Box::new(std::io::Error::new(
                                        std::io::ErrorKind::InvalidData,
                                        error,
                                    )),
                                )
                            })?;
                        (
                            None,
                            normalize_time_values(&schedule.times),
                            Vec::new(),
                            Vec::new(),
                            None,
                        )
                    }
                };
            let source_session_id: String = row.get(3)?;
            let delivery_json: String = row.get(14)?;
            let delivery =
                parse_agent_task_delivery_json(&delivery_json, &source_session_id).map_err(
                    |error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            14,
                            rusqlite::types::Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                error,
                            )),
                        )
                    },
                )?;
            Ok(AgentTaskListItem {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                agent_name: row.get(2)?,
                source_session_id,
                title: row.get(4)?,
                intent_summary: row.get(5)?,
                task_type: row.get(6)?,
                schedule_type,
                timezone: row.get(8)?,
                goal: payload.goal,
                interval_minutes,
                daily_times,
                weekly_days,
                monthly_days,
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
        && schedule_type != SCHEDULE_TYPE_WEEKLY_TIME
        && schedule_type != SCHEDULE_TYPE_MONTHLY_TIME
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
        let run_at = input
            .run_at_ms
            .ok_or_else(|| "请指定一次性任务的执行时间".to_string())?;
        if run_at <= now {
            return Err("一次性任务的执行时间须晚于当前时间".to_string());
        }
        serde_json::to_string(&OnceAtSchedule { run_at_ms: run_at })
            .map_err(|error| format!("序列化 once_at schedule 失败: {error}"))?
    } else if schedule_type == SCHEDULE_TYPE_WEEKLY_TIME {
        let times = normalize_time_values(&input.daily_times);
        let days = normalize_weekly_days(&input.weekly_days);
        if days.is_empty() {
            return Err("每周任务至少需要一个星期几".to_string());
        }
        if times.is_empty() {
            return Err("每周任务至少需要一个触发时间".to_string());
        }
        serde_json::to_string(&WeeklyTimeSchedule {
            times,
            days_of_week: days,
        })
        .map_err(|error| format!("序列化 weekly schedule 失败: {error}"))?
    } else if schedule_type == SCHEDULE_TYPE_MONTHLY_TIME {
        let times = normalize_time_values(&input.daily_times);
        let days = normalize_monthly_days(&input.monthly_days);
        if days.is_empty() {
            return Err("每月任务至少需要一个日期".to_string());
        }
        if times.is_empty() {
            return Err("每月任务至少需要一个触发时间".to_string());
        }
        serde_json::to_string(&MonthlyTimeSchedule {
            times,
            days_of_month: days,
        })
        .map_err(|error| format!("序列化 monthly schedule 失败: {error}"))?
    } else {
        let times = normalize_time_values(&input.daily_times);
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
        parse_agent_task_delivery_json(&existing_delivery_json, &existing.source_session_id)
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

    let schedule_hint = format_schedule_hint_for_update(input, schedule_type);
    let (title, intent_summary, payload_goal) = match crate::refine_agent_task_metadata(
        app,
        &existing.agent_id,
        goal,
        &existing.task_type,
        &schedule_hint,
    ) {
        Some((t, s, g)) => {
            let title = if input.title.trim().is_empty() {
                t
            } else {
                input.title.trim().to_string()
            };
            (title, s, g)
        }
        None => {
            let title = if input.title.trim().is_empty() {
                build_title(goal, &existing.task_type)
            } else {
                input.title.trim().to_string()
            };
            (title, summarize_goal_fallback(goal), goal.to_string())
        }
    };

    let payload_json = serde_json::to_string(&AgentTaskPayload {
        goal: payload_goal.clone(),
        reminder_text: if existing.task_type == TASK_TYPE_REMINDER {
            payload_goal.clone()
        } else {
            String::new()
        },
        prompt_template: if existing.task_type == TASK_TYPE_AGENT_PROMPT {
            payload_goal
        } else {
            String::new()
        },
    })
    .map_err(|error| format!("序列化 task payload 失败: {error}"))?;
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
    let agent_snapshot = agents::get_conversation_agent_config(app, agent_id)
        .ok()
        .flatten();
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
            let source_session_id: String = row.get(2)?;
            let task = AgentTaskRecord {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                source_session_id,
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
            let delivery =
                parse_agent_task_delivery_json(&delivery_json, &task.source_session_id).map_err(
                    |error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            11,
                            rusqlite::types::Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                error,
                            )),
                        )
                    },
                )?;
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

fn format_schedule_hint_for_update(input: &AgentTaskUpdateInput, schedule_type: &str) -> String {
    build_schedule_hint(
        schedule_type,
        input.interval_minutes,
        &input.daily_times,
        &input.weekly_days,
        &input.monthly_days,
        input.run_at_ms,
    )
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
