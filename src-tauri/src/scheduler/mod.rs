use crate::agent_task_schedule::{
    DailyTimeSchedule as AgentDailyTimeSchedule, IntervalSchedule as AgentIntervalSchedule,
    MonthlyTimeSchedule as AgentMonthlyTimeSchedule, OnceAtSchedule,
    WeeklyTimeSchedule as AgentWeeklyTimeSchedule, SCHEDULE_TYPE_MONTHLY_TIME,
    SCHEDULE_TYPE_ONCE_AT, SCHEDULE_TYPE_WEEKLY_TIME,
};
use crate::agent_tasks::{self, AgentTaskPayload};
use crate::agent_workspace;
use crate::agents::{self, AgentRecord};
use crate::channels::pi_bridge::{PiBridge, PiProcessOutcome};
use crate::channels::types::{MediaPayload, MediaType};
use crate::media_directives::{parse_markdown_media_reference, parse_media_directive_fields};
use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, FixedOffset, LocalResult, NaiveTime, TimeZone,
    Timelike, Utc,
};
use chrono_tz::Tz;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

const SCHEDULER_SCAN_INTERVAL_SECS: u64 = 30;
const LEADER_LEASE_TTL_MS: i64 = 90_000;
const MAX_PARALLEL_RUNS: usize = 2;
const MAX_OUTPUT_CHARS: usize = 2_400;
const AI_CHUNK_SIZE: usize = 900;
const SOURCE_KIND_AGENT_HEARTBEAT: &str = "agent_heartbeat";
const SOURCE_KIND_AGENT_TASK: &str = "agent_task";
const TRIGGER_TYPE_DAILY_TIME: &str = "daily_time";
const TRIGGER_TYPE_INTERVAL: &str = "interval";
const TRIGGER_TYPE_ONCE_AT: &str = "once_at";
const TRIGGER_TYPE_WEEKLY_TIME: &str = "weekly_time";
const TRIGGER_TYPE_MONTHLY_TIME: &str = "monthly_time";
const LEASE_KEY_DAEMON_LEADER: &str = "daemon_leader";
const MACOS_LAUNCH_AGENT_LABEL: &str = "com.wuyq.nineclaw.scheduler";
#[cfg(target_os = "windows")]
const WINDOWS_TASK_NAME: &str = "NineClaw Scheduler";

static EMBEDDED_SCHEDULER_STARTED: OnceLock<()> = OnceLock::new();
static ACTIVE_RUNS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// 桌面端任务推送写入本地历史时，每次执行使用独立会话，与创建任务时的会话解耦。
fn desktop_delivery_history_session_id() -> String {
    format!("task_sess_{}", Uuid::new_v4().simple())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledJobRecord {
    pub id: String,
    pub source_kind: String,
    pub owner_agent_id: String,
    pub source_schedule_id: String,
    pub source_task_id: String,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub timezone: String,
    pub trigger_type: String,
    pub trigger_spec_json: String,
    pub payload_json: String,
    pub delivery_json: String,
    pub next_run_at: Option<i64>,
    pub last_run_at: Option<i64>,
    pub last_synced_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledJobRunRecord {
    pub id: String,
    pub job_id: String,
    pub scheduled_for: i64,
    pub claimed_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub status: String,
    pub attempt: i64,
    pub worker_id: Option<String>,
    pub summary: Option<String>,
    pub details: Option<String>,
    pub error: Option<String>,
    pub output_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerServiceStatus {
    pub installed: bool,
    pub platform: String,
    pub detail: String,
    pub launcher_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerRuntimeStatus {
    pub service: SchedulerServiceStatus,
    pub daemon_active: bool,
    pub leader_owner_id: Option<String>,
    pub leader_leased_until: Option<i64>,
    pub active_run_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerSyncResult {
    pub job_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DailyTimeTrigger {
    time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeeklyTimeTrigger {
    day_of_week: u32,
    time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonthlyTimeTrigger {
    day_of_month: u32,
    time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IntervalTrigger {
    every_minutes: i64,
    start_at: Option<i64>,
    end_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchedulerPayload {
    task_type: String,
    task_name: String,
    task_description: String,
    message_template: String,
    command: String,
    working_directory: String,
    timeout_sec: i64,
    notify_on_success: bool,
    notify_on_failure: bool,
    reminder_text: String,
    prompt_template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchedulerDelivery {
    channel_id: String,
    target_user_id: String,
    target_label: String,
    schedule_name: String,
}

#[derive(Debug, Clone)]
struct MaterializedJob {
    id: String,
    source_kind: String,
    owner_agent_id: String,
    source_schedule_id: String,
    source_task_id: String,
    kind: String,
    name: String,
    description: String,
    enabled: bool,
    timezone: String,
    trigger_type: String,
    trigger_spec_json: String,
    payload_json: String,
    delivery_json: String,
}

#[derive(Debug, Clone)]
struct ExistingJobState {
    kind: String,
    enabled: bool,
    timezone: String,
    trigger_type: String,
    trigger_spec_json: String,
    payload_json: String,
    delivery_json: String,
    next_run_at: Option<i64>,
}

#[derive(Debug, Clone)]
struct DueJobCandidate {
    id: String,
    source_kind: String,
    source_schedule_id: String,
    owner_agent_id: String,
    kind: String,
    timezone: String,
    trigger_type: String,
    trigger_spec_json: String,
    payload_json: String,
    delivery_json: String,
    next_run_at: i64,
}

#[derive(Debug, Clone)]
struct ClaimedRun {
    run_id: String,
    job_id: String,
    source_kind: String,
    source_schedule_id: String,
    owner_agent_id: String,
    kind: String,
    timezone: String,
    payload_json: String,
    delivery_json: String,
    scheduled_for: i64,
}

#[derive(Debug, Clone)]
struct ShellTaskOutcome {
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    elapsed_ms: u128,
}

#[derive(Debug, Clone)]
struct TaskExecutionResult {
    success: bool,
    summary: String,
    details: String,
    error: Option<String>,
    sent_message: Option<String>,
    output_path: Option<String>,
}

#[derive(Clone, Debug)]
struct ParsedMediaItem {
    media_type: MediaType,
    file_name: String,
    file_path: String,
}

enum ResolvedTimezone {
    Named(Tz),
    Fixed(FixedOffset),
}

fn active_runs() -> &'static Mutex<HashSet<String>> {
    ACTIVE_RUNS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Leader lease renewal requires a **stable** owner id per process. A random UUID per tick breaks
/// `ON CONFLICT ... WHERE owner_id = ?2` and stops all scheduler work after the first interval.
fn embedded_scheduler_owner_id() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    CELL.get_or_init(|| format!("embedded:{}", std::process::id()))
        .clone()
}

fn daemon_scheduler_owner_id() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    CELL.get_or_init(|| format!("daemon:{}", std::process::id()))
        .clone()
}

fn manual_scheduler_owner_id() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    CELL.get_or_init(|| format!("manual:{}", std::process::id()))
        .clone()
}

pub fn ensure_scheduler_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS scheduled_jobs (
                id TEXT PRIMARY KEY,
                source_kind TEXT NOT NULL,
                owner_agent_id TEXT NOT NULL,
                source_schedule_id TEXT NOT NULL,
                source_task_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                timezone TEXT NOT NULL,
                trigger_type TEXT NOT NULL,
                trigger_spec_json TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                delivery_json TEXT NOT NULL,
                next_run_at INTEGER,
                last_run_at INTEGER,
                last_synced_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_due
                ON scheduled_jobs(enabled, next_run_at);
            CREATE INDEX IF NOT EXISTS idx_scheduled_jobs_owner
                ON scheduled_jobs(owner_agent_id, source_kind);
            CREATE TABLE IF NOT EXISTS scheduled_job_runs (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                scheduled_for INTEGER NOT NULL,
                claimed_at INTEGER NOT NULL,
                started_at INTEGER,
                finished_at INTEGER,
                status TEXT NOT NULL,
                attempt INTEGER NOT NULL DEFAULT 1,
                worker_id TEXT,
                summary TEXT,
                details TEXT,
                error TEXT,
                output_path TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(job_id, scheduled_for, attempt)
            );
            CREATE INDEX IF NOT EXISTS idx_scheduled_job_runs_job
                ON scheduled_job_runs(job_id, scheduled_for DESC);
            CREATE INDEX IF NOT EXISTS idx_scheduled_job_runs_status
                ON scheduled_job_runs(status, updated_at DESC);
            CREATE TABLE IF NOT EXISTS scheduler_leases (
                lease_key TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL,
                leased_until INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )
        .map_err(|error| format!("初始化 scheduler 数据表失败: {error}"))?;

    Ok(())
}

pub fn start_embedded_scheduler(app: AppHandle) {
    if EMBEDDED_SCHEDULER_STARTED.set(()).is_err() {
        return;
    }

    thread::spawn(move || {
        let owner_id = embedded_scheduler_owner_id();
        if let Err(error) = scheduler_tick(&app, &owner_id) {
            log::warn!("首次扫描 scheduler 失败: {}", error);
        }

        loop {
            thread::sleep(Duration::from_secs(SCHEDULER_SCAN_INTERVAL_SECS));
            if let Err(error) = scheduler_tick(&app, &owner_id) {
                log::warn!("扫描 scheduler 失败: {}", error);
            }
        }
    });
}

pub fn run_daemon(app: AppHandle) -> Result<(), String> {
    let owner_id = daemon_scheduler_owner_id();
    if let Err(error) = scheduler_tick(&app, &owner_id) {
        log::warn!("scheduler daemon 首次扫描失败: {}", error);
    }

    if should_run_daemon_once() {
        return Ok(());
    }

    loop {
        thread::sleep(Duration::from_secs(SCHEDULER_SCAN_INTERVAL_SECS));
        if let Err(error) = scheduler_tick(&app, &owner_id) {
            log::warn!("scheduler daemon 扫描失败: {}", error);
        }
    }
}

pub fn sync_materialized_jobs(app: &AppHandle) -> Result<SchedulerSyncResult, String> {
    let agent_records = agents::list_agents(app)?;
    let mut connection = crate::open_history_db(app)?;
    ensure_scheduler_schema(&connection)?;
    let job_count = sync_materialized_jobs_with_connection(&mut connection, &agent_records)?;
    Ok(SchedulerSyncResult { job_count })
}

pub fn trigger_agent_task_now(app: &AppHandle, task_id: &str) -> Result<(), String> {
    let _ = sync_materialized_jobs(app)?;
    let connection = crate::open_history_db(app)?;
    ensure_scheduler_schema(&connection)?;
    let now = crate::chrono_like_timestamp();
    let owner_id = manual_scheduler_owner_id();

    let candidate = connection
        .query_row(
            "SELECT id, owner_agent_id, kind, timezone, payload_json, delivery_json, source_kind, source_schedule_id
             FROM scheduled_jobs
             WHERE source_kind = ?1
               AND source_schedule_id = ?2
               AND enabled = 1
             ORDER BY lower(name) ASC
             LIMIT 1",
            params![SOURCE_KIND_AGENT_TASK, task_id],
            |row| {
                Ok(ClaimedRun {
                    run_id: format!("run_{}", Uuid::new_v4().simple()),
                    job_id: row.get(0)?,
                    source_kind: row.get(6)?,
                    source_schedule_id: row.get(7)?,
                    owner_agent_id: row.get(1)?,
                    kind: row.get(2)?,
                    timezone: row.get(3)?,
                    payload_json: row.get(4)?,
                    delivery_json: row.get(5)?,
                    scheduled_for: now,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取可执行任务失败: {error}"))?
        .ok_or_else(|| "当前任务不可执行，可能已经暂停、删除或尚未同步到调度器".to_string())?;

    connection
        .execute(
            "INSERT INTO scheduled_job_runs (
                id, job_id, scheduled_for, claimed_at, started_at, status, attempt, worker_id, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?4, 'running', 1, ?5, ?4, ?4)",
            params![candidate.run_id, candidate.job_id, now, now, owner_id],
        )
        .map_err(|error| format!("登记立即执行记录失败: {error}"))?;

    spawn_run_worker(app.clone(), candidate);
    Ok(())
}

pub fn list_jobs(app: &AppHandle) -> Result<Vec<ScheduledJobRecord>, String> {
    let _ = sync_materialized_jobs(app);
    let connection = crate::open_history_db(app)?;
    ensure_scheduler_schema(&connection)?;
    let mut statement = connection
        .prepare(
            "SELECT
                id,
                source_kind,
                owner_agent_id,
                source_schedule_id,
                source_task_id,
                kind,
                name,
                description,
                enabled,
                timezone,
                trigger_type,
                trigger_spec_json,
                payload_json,
                delivery_json,
                next_run_at,
                last_run_at,
                last_synced_at,
                created_at,
                updated_at
            FROM scheduled_jobs
            ORDER BY enabled DESC, COALESCE(next_run_at, updated_at) ASC, lower(name) ASC",
        )
        .map_err(|error| format!("读取 scheduler 任务失败: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(ScheduledJobRecord {
                id: row.get(0)?,
                source_kind: row.get(1)?,
                owner_agent_id: row.get(2)?,
                source_schedule_id: row.get(3)?,
                source_task_id: row.get(4)?,
                kind: row.get(5)?,
                name: row.get(6)?,
                description: row.get(7)?,
                enabled: row.get::<_, i64>(8)? != 0,
                timezone: row.get(9)?,
                trigger_type: row.get(10)?,
                trigger_spec_json: row.get(11)?,
                payload_json: row.get(12)?,
                delivery_json: row.get(13)?,
                next_run_at: row.get(14)?,
                last_run_at: row.get(15)?,
                last_synced_at: row.get(16)?,
                created_at: row.get(17)?,
                updated_at: row.get(18)?,
            })
        })
        .map_err(|error| format!("解析 scheduler 任务失败: {error}"))?;

    let mut jobs = Vec::new();
    for row in rows {
        jobs.push(row.map_err(|error| format!("读取 scheduler 任务行失败: {error}"))?);
    }
    Ok(jobs)
}

pub fn list_job_runs(
    app: &AppHandle,
    limit: Option<u32>,
) -> Result<Vec<ScheduledJobRunRecord>, String> {
    let connection = crate::open_history_db(app)?;
    ensure_scheduler_schema(&connection)?;
    let limit = limit.unwrap_or(100).clamp(1, 500) as i64;
    let mut statement = connection
        .prepare(
            "SELECT
                id,
                job_id,
                scheduled_for,
                claimed_at,
                started_at,
                finished_at,
                status,
                attempt,
                worker_id,
                summary,
                details,
                error,
                output_path,
                created_at,
                updated_at
            FROM scheduled_job_runs
            ORDER BY scheduled_for DESC, created_at DESC
            LIMIT ?1",
        )
        .map_err(|error| format!("读取 scheduler 运行记录失败: {error}"))?;

    let rows = statement
        .query_map(params![limit], |row| {
            Ok(ScheduledJobRunRecord {
                id: row.get(0)?,
                job_id: row.get(1)?,
                scheduled_for: row.get(2)?,
                claimed_at: row.get(3)?,
                started_at: row.get(4)?,
                finished_at: row.get(5)?,
                status: row.get(6)?,
                attempt: row.get(7)?,
                worker_id: row.get(8)?,
                summary: row.get(9)?,
                details: row.get(10)?,
                error: row.get(11)?,
                output_path: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .map_err(|error| format!("解析 scheduler 运行记录失败: {error}"))?;

    let mut runs = Vec::new();
    for row in rows {
        runs.push(row.map_err(|error| format!("读取 scheduler 运行记录行失败: {error}"))?);
    }
    Ok(runs)
}

pub fn trigger_job_now(app: &AppHandle, job_id: &str) -> Result<(), String> {
    let connection = crate::open_history_db(app)?;
    ensure_scheduler_schema(&connection)?;
    let now = crate::chrono_like_timestamp();
    let updated = connection
        .execute(
            "UPDATE scheduled_jobs
             SET next_run_at = ?2, updated_at = ?2
             WHERE id = ?1 AND enabled = 1",
            params![job_id, now],
        )
        .map_err(|error| format!("更新 scheduler 任务触发时间失败: {error}"))?;
    if updated == 0 {
        return Err("未找到可立即执行的 scheduler 任务".to_string());
    }
    let _ = scheduler_tick(app, &manual_scheduler_owner_id());
    Ok(())
}

pub fn runtime_status(app: &AppHandle) -> Result<SchedulerRuntimeStatus, String> {
    let connection = crate::open_history_db(app)?;
    ensure_scheduler_schema(&connection)?;
    let leader = connection
        .query_row(
            "SELECT owner_id, leased_until FROM scheduler_leases WHERE lease_key = ?1",
            params![LEASE_KEY_DAEMON_LEADER],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|error| format!("读取 scheduler leader lease 失败: {error}"))?;
    let now = crate::chrono_like_timestamp();
    let (leader_owner_id, leader_leased_until, daemon_active) = match leader {
        Some((owner_id, leased_until)) => {
            let active = leased_until > now;
            (Some(owner_id), Some(leased_until), active)
        }
        None => (None, None, false),
    };

    Ok(SchedulerRuntimeStatus {
        service: service_status()?,
        daemon_active,
        leader_owner_id,
        leader_leased_until,
        active_run_count: active_runs()
            .lock()
            .map(|runs| runs.len())
            .unwrap_or_default(),
    })
}

pub fn install_service() -> Result<SchedulerServiceStatus, String> {
    #[cfg(target_os = "macos")]
    {
        let launcher_path = macos_launch_agent_path()?;
        let exe = current_executable_path()?;
        let logs_dir = scheduler_logs_dir()?;
        if let Some(parent) = launcher_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("创建 LaunchAgents 目录失败: {error}"))?;
        }
        fs::create_dir_all(&logs_dir)
            .map_err(|error| format!("创建 scheduler 日志目录失败: {error}"))?;
        let plist = render_macos_launch_agent(&exe, &logs_dir);
        fs::write(&launcher_path, plist).map_err(|error| {
            format!("写入 LaunchAgent 失败 {}: {error}", launcher_path.display())
        })?;
        let _ = Command::new("launchctl")
            .args(["unload", launcher_path.to_string_lossy().as_ref()])
            .status();
        let status = Command::new("launchctl")
            .args(["load", launcher_path.to_string_lossy().as_ref()])
            .status()
            .map_err(|error| format!("加载 LaunchAgent 失败: {error}"))?;
        if !status.success() {
            return Err(format!("launchctl load 失败: {status}"));
        }
        return service_status();
    }

    #[cfg(target_os = "windows")]
    {
        let exe = current_executable_path()?;
        let task_command = format!("\"{}\" --scheduler-daemon", exe.display());
        let create_status = Command::new("schtasks")
            .args([
                "/Create",
                "/TN",
                WINDOWS_TASK_NAME,
                "/SC",
                "ONLOGON",
                "/RL",
                "LIMITED",
                "/F",
                "/TR",
                task_command.as_str(),
            ])
            .status()
            .map_err(|error| format!("创建 Windows 计划任务失败: {error}"))?;
        if !create_status.success() {
            return Err(format!("schtasks /Create 失败: {create_status}"));
        }
        let _ = Command::new("schtasks")
            .args(["/Run", "/TN", WINDOWS_TASK_NAME])
            .status();
        return service_status();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("当前平台暂未实现 scheduler service install".to_string())
    }
}

pub fn uninstall_service() -> Result<SchedulerServiceStatus, String> {
    #[cfg(target_os = "macos")]
    {
        let launcher_path = macos_launch_agent_path()?;
        if launcher_path.exists() {
            let _ = Command::new("launchctl")
                .args(["unload", launcher_path.to_string_lossy().as_ref()])
                .status();
            fs::remove_file(&launcher_path).map_err(|error| {
                format!("删除 LaunchAgent 失败 {}: {error}", launcher_path.display())
            })?;
        }
        return service_status();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("schtasks")
            .args(["/End", "/TN", WINDOWS_TASK_NAME])
            .status();
        let status = Command::new("schtasks")
            .args(["/Delete", "/TN", WINDOWS_TASK_NAME, "/F"])
            .status()
            .map_err(|error| format!("删除 Windows 计划任务失败: {error}"))?;
        if !status.success() {
            return Err(format!("schtasks /Delete 失败: {status}"));
        }
        return service_status();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("当前平台暂未实现 scheduler service uninstall".to_string())
    }
}

pub fn service_status() -> Result<SchedulerServiceStatus, String> {
    #[cfg(target_os = "macos")]
    {
        let launcher_path = macos_launch_agent_path()?;
        let installed = launcher_path.exists();
        return Ok(SchedulerServiceStatus {
            installed,
            platform: "macos".to_string(),
            detail: if installed {
                "LaunchAgent 已安装".to_string()
            } else {
                "LaunchAgent 未安装".to_string()
            },
            launcher_path: Some(launcher_path.to_string_lossy().to_string()),
        });
    }

    #[cfg(target_os = "windows")]
    {
        let installed = Command::new("schtasks")
            .args(["/Query", "/TN", WINDOWS_TASK_NAME])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        return Ok(SchedulerServiceStatus {
            installed,
            platform: "windows".to_string(),
            detail: if installed {
                "Windows Task Scheduler 任务已安装".to_string()
            } else {
                "Windows Task Scheduler 任务未安装".to_string()
            },
            launcher_path: None,
        });
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(SchedulerServiceStatus {
            installed: false,
            platform: std::env::consts::OS.to_string(),
            detail: "当前平台暂未实现".to_string(),
            launcher_path: None,
        })
    }
}

fn should_run_daemon_once() -> bool {
    std::env::var("NINECLAW_SCHEDULER_RUN_ONCE")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn scheduler_tick(app: &AppHandle, owner_id: &str) -> Result<(), String> {
    let mut connection = crate::open_history_db(app)?;
    ensure_scheduler_schema(&connection)?;
    if !acquire_or_renew_leader_lease(&connection, owner_id)? {
        return Ok(());
    }
    let agent_records = agents::list_agents(app)?;
    sync_materialized_jobs_with_connection(&mut connection, &agent_records)?;
    let claims = claim_due_jobs(&mut connection, owner_id)?;
    drop(connection);

    for claim in claims {
        spawn_run_worker(app.clone(), claim);
    }

    Ok(())
}

fn spawn_run_worker(app: AppHandle, claim: ClaimedRun) {
    let run_id = claim.run_id.clone();
    if let Ok(mut runs) = active_runs().lock() {
        runs.insert(run_id.clone());
    }

    thread::spawn(move || {
        let result = execute_claimed_run(&app, &claim);
        let finalize_result = finalize_run(&app, &claim, result);
        if let Err(error) = finalize_result {
            log::warn!("更新 scheduler 运行结果失败: {}", error);
        }
        if let Ok(mut runs) = active_runs().lock() {
            runs.remove(&run_id);
        }
    });
}

fn acquire_or_renew_leader_lease(connection: &Connection, owner_id: &str) -> Result<bool, String> {
    let now = crate::chrono_like_timestamp();
    let leased_until = now + LEADER_LEASE_TTL_MS;
    let updated = connection
        .execute(
            "INSERT INTO scheduler_leases (lease_key, owner_id, leased_until, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(lease_key) DO UPDATE SET
                owner_id = excluded.owner_id,
                leased_until = excluded.leased_until,
                updated_at = excluded.updated_at
             WHERE scheduler_leases.leased_until < ?4 OR scheduler_leases.owner_id = ?2",
            params![LEASE_KEY_DAEMON_LEADER, owner_id, leased_until, now],
        )
        .map_err(|error| format!("更新 scheduler leader lease 失败: {error}"))?;
    Ok(updated > 0)
}

fn sync_materialized_jobs_with_connection(
    connection: &mut Connection,
    agent_records: &[AgentRecord],
) -> Result<usize, String> {
    ensure_scheduler_schema(connection)?;
    agent_tasks::pause_expired_once_at_tasks(connection)?;
    let existing_map = load_existing_job_states(connection)?;
    let now = crate::chrono_like_timestamp();
    let mut desired_jobs = materialize_jobs_from_agents(agent_records)?;
    desired_jobs.extend(materialize_jobs_from_agent_tasks(connection)?);
    let desired_ids: HashSet<_> = desired_jobs.iter().map(|job| job.id.as_str()).collect();

    let transaction = connection
        .transaction()
        .map_err(|error| format!("同步 scheduler 任务事务失败: {error}"))?;

    for job in &desired_jobs {
        let next_run_at = determine_next_run_at(job, existing_map.get(job.id.as_str()), now)?;
        transaction
            .execute(
                "INSERT INTO scheduled_jobs (
                    id, source_kind, owner_agent_id, source_schedule_id, source_task_id, kind,
                    name, description, enabled, timezone, trigger_type, trigger_spec_json,
                    payload_json, delivery_json, next_run_at, last_synced_at, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16, ?16)
                ON CONFLICT(id) DO UPDATE SET
                    source_kind = excluded.source_kind,
                    owner_agent_id = excluded.owner_agent_id,
                    source_schedule_id = excluded.source_schedule_id,
                    source_task_id = excluded.source_task_id,
                    kind = excluded.kind,
                    name = excluded.name,
                    description = excluded.description,
                    enabled = excluded.enabled,
                    timezone = excluded.timezone,
                    trigger_type = excluded.trigger_type,
                    trigger_spec_json = excluded.trigger_spec_json,
                    payload_json = excluded.payload_json,
                    delivery_json = excluded.delivery_json,
                    next_run_at = excluded.next_run_at,
                    last_synced_at = excluded.last_synced_at,
                    updated_at = excluded.updated_at",
                params![
                    job.id,
                    job.source_kind,
                    job.owner_agent_id,
                    job.source_schedule_id,
                    job.source_task_id,
                    job.kind,
                    job.name,
                    job.description,
                    if job.enabled { 1 } else { 0 },
                    job.timezone,
                    job.trigger_type,
                    job.trigger_spec_json,
                    job.payload_json,
                    job.delivery_json,
                    next_run_at,
                    now,
                ],
            )
            .map_err(|error| format!("写入 materialized scheduler 任务失败: {error}"))?;
    }

    if desired_ids.is_empty() {
        transaction
            .execute(
                "DELETE FROM scheduled_jobs WHERE source_kind IN (?1, ?2)",
                params![SOURCE_KIND_AGENT_HEARTBEAT, SOURCE_KIND_AGENT_TASK],
            )
            .map_err(|error| format!("清理 materialized 任务失败: {error}"))?;
    } else {
        let placeholders = std::iter::repeat("?")
            .take(desired_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "DELETE FROM scheduled_jobs
             WHERE source_kind IN (?1, ?2)
               AND id NOT IN ({placeholders})"
        );
        let mut params_vec = vec![
            rusqlite::types::Value::from(SOURCE_KIND_AGENT_HEARTBEAT.to_string()),
            rusqlite::types::Value::from(SOURCE_KIND_AGENT_TASK.to_string()),
        ];
        for id in &desired_ids {
            params_vec.push(rusqlite::types::Value::from((*id).to_string()));
        }
        transaction
            .execute(&sql, rusqlite::params_from_iter(params_vec))
            .map_err(|error| format!("清理旧 scheduler 任务失败: {error}"))?;
    }

    transaction
        .commit()
        .map_err(|error| format!("提交 scheduler 同步事务失败: {error}"))?;

    Ok(desired_jobs.len())
}

fn load_existing_job_states(
    connection: &Connection,
) -> Result<HashMap<String, ExistingJobState>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, kind, enabled, timezone, trigger_type, trigger_spec_json,
                    payload_json, delivery_json, next_run_at
             FROM scheduled_jobs
             WHERE source_kind IN (?1, ?2)",
        )
        .map_err(|error| format!("读取 scheduler 既有任务失败: {error}"))?;

    let rows = statement
        .query_map(
            params![SOURCE_KIND_AGENT_HEARTBEAT, SOURCE_KIND_AGENT_TASK],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ExistingJobState {
                        kind: row.get(1)?,
                        enabled: row.get::<_, i64>(2)? != 0,
                        timezone: row.get(3)?,
                        trigger_type: row.get(4)?,
                        trigger_spec_json: row.get(5)?,
                        payload_json: row.get(6)?,
                        delivery_json: row.get(7)?,
                        next_run_at: row.get(8)?,
                    },
                ))
            },
        )
        .map_err(|error| format!("解析 scheduler 既有任务失败: {error}"))?;

    let mut states = HashMap::new();
    for row in rows {
        let (id, state) = row.map_err(|error| format!("读取 scheduler 既有任务行失败: {error}"))?;
        states.insert(id, state);
    }
    Ok(states)
}

fn materialize_jobs_from_agents(
    agent_records: &[AgentRecord],
) -> Result<Vec<MaterializedJob>, String> {
    let mut jobs = Vec::new();

    for agent in agent_records {
        let task_map: HashMap<_, _> = agent
            .heartbeat_config
            .tasks
            .iter()
            .cloned()
            .map(|task| (task.id.clone(), task))
            .collect();

        for schedule in &agent.heartbeat_config.schedules {
            let Some(task) = task_map.get(&schedule.task_id) else {
                continue;
            };

            for time in &schedule.times {
                let trigger = DailyTimeTrigger { time: time.clone() };
                let payload = SchedulerPayload {
                    task_type: task.task_type.clone(),
                    task_name: task.name.clone(),
                    task_description: task.description.clone(),
                    message_template: task.message_template.clone(),
                    command: task.command.clone(),
                    working_directory: task.working_directory.clone(),
                    timeout_sec: task.timeout_sec,
                    notify_on_success: task.notify_on_success,
                    notify_on_failure: task.notify_on_failure,
                    reminder_text: task.description.clone(),
                    prompt_template: String::new(),
                };
                let delivery = SchedulerDelivery {
                    channel_id: schedule.channel_id.clone(),
                    target_user_id: schedule.target_user_id.clone(),
                    target_label: schedule.target_label.clone(),
                    schedule_name: schedule.name.clone(),
                };

                jobs.push(MaterializedJob {
                    id: materialized_job_id(&agent.id, &schedule.id, time),
                    source_kind: SOURCE_KIND_AGENT_HEARTBEAT.to_string(),
                    owner_agent_id: agent.id.clone(),
                    source_schedule_id: schedule.id.clone(),
                    source_task_id: task.id.clone(),
                    kind: normalize_job_kind(&task.task_type),
                    name: format!("{} · {} @ {}", agent.name, schedule.name, time),
                    description: task.description.clone(),
                    enabled: schedule.enabled && task.enabled,
                    timezone: normalize_scheduler_timezone(&agent.heartbeat_config.timezone),
                    trigger_type: TRIGGER_TYPE_DAILY_TIME.to_string(),
                    trigger_spec_json: serde_json::to_string(&trigger)
                        .map_err(|error| format!("序列化 scheduler trigger 失败: {error}"))?,
                    payload_json: serde_json::to_string(&payload)
                        .map_err(|error| format!("序列化 scheduler payload 失败: {error}"))?,
                    delivery_json: serde_json::to_string(&delivery)
                        .map_err(|error| format!("序列化 scheduler delivery 失败: {error}"))?,
                });
            }
        }
    }

    Ok(jobs)
}

fn materialize_jobs_from_agent_tasks(
    connection: &Connection,
) -> Result<Vec<MaterializedJob>, String> {
    let task_defs = agent_tasks::list_active_tasks_for_scheduler(connection)?;
    let mut jobs = Vec::new();

    for item in task_defs {
        let payload: AgentTaskPayload = serde_json::from_str(&item.task.payload_json)
            .map_err(|error| format!("解析 agent task payload 失败: {error}"))?;
        let base_payload = SchedulerPayload {
            task_type: item.task.task_type.clone(),
            task_name: item.task.title.clone(),
            task_description: item.task.intent_summary.clone(),
            message_template: String::new(),
            command: String::new(),
            working_directory: String::new(),
            timeout_sec: 180,
            notify_on_success: true,
            notify_on_failure: true,
            reminder_text: payload.reminder_text,
            prompt_template: payload.prompt_template,
        };
        let delivery = SchedulerDelivery {
            channel_id: item.delivery.kind,
            target_user_id: item.delivery.session_id.clone(),
            target_label: item.delivery.session_id.clone(),
            schedule_name: item.task.title.clone(),
        };
        match item.task.schedule_type.as_str() {
            SCHEDULE_TYPE_ONCE_AT => {
                let schedule: OnceAtSchedule = serde_json::from_str(&item.task.schedule_json)
                    .map_err(|error| format!("解析 once_at agent task schedule 失败: {error}"))?;
                let now_ms = crate::chrono_like_timestamp();
                let enabled = item.task.status == "active" && schedule.run_at_ms > now_ms;
                jobs.push(MaterializedJob {
                    id: format!("task:{}:once", item.task.id),
                    source_kind: SOURCE_KIND_AGENT_TASK.to_string(),
                    owner_agent_id: item.task.agent_id.clone(),
                    source_schedule_id: item.task.id.clone(),
                    source_task_id: item.task.id.clone(),
                    kind: match item.task.task_type.as_str() {
                        "agent_prompt" => "agent_prompt".to_string(),
                        _ => "notify".to_string(),
                    },
                    name: item.task.title.clone(),
                    description: item.task.intent_summary.clone(),
                    enabled,
                    timezone: normalize_scheduler_timezone(&item.task.timezone),
                    trigger_type: TRIGGER_TYPE_ONCE_AT.to_string(),
                    trigger_spec_json: serde_json::to_string(&schedule)
                        .map_err(|error| format!("序列化 once_at trigger 失败: {error}"))?,
                    payload_json: serde_json::to_string(&base_payload).map_err(|error| {
                        format!("序列化 agent task scheduler payload 失败: {error}")
                    })?,
                    delivery_json: serde_json::to_string(&delivery)
                        .map_err(|error| format!("序列化 agent task delivery 失败: {error}"))?,
                });
            }
            SCHEDULE_TYPE_WEEKLY_TIME => {
                let schedule: AgentWeeklyTimeSchedule =
                    serde_json::from_str(&item.task.schedule_json).map_err(|error| {
                        format!("解析 weekly agent task schedule 失败: {error}")
                    })?;
                for day in schedule.days_of_week {
                    for time in &schedule.times {
                        jobs.push(MaterializedJob {
                            id: format!("task:{}:w{}:{}", item.task.id, day, time.replace(':', "")),
                            source_kind: SOURCE_KIND_AGENT_TASK.to_string(),
                            owner_agent_id: item.task.agent_id.clone(),
                            source_schedule_id: item.task.id.clone(),
                            source_task_id: item.task.id.clone(),
                            kind: match item.task.task_type.as_str() {
                                "agent_prompt" => "agent_prompt".to_string(),
                                _ => "notify".to_string(),
                            },
                            name: format!("{} @ 周{} {}", item.task.title, day, time),
                            description: item.task.intent_summary.clone(),
                            enabled: item.task.status == "active",
                            timezone: normalize_scheduler_timezone(&item.task.timezone),
                            trigger_type: TRIGGER_TYPE_WEEKLY_TIME.to_string(),
                            trigger_spec_json: serde_json::to_string(&WeeklyTimeTrigger {
                                day_of_week: day,
                                time: time.clone(),
                            })
                            .map_err(|error| format!("序列化 weekly trigger 失败: {error}"))?,
                            payload_json: serde_json::to_string(&base_payload).map_err(
                                |error| {
                                    format!("序列化 agent task scheduler payload 失败: {error}")
                                },
                            )?,
                            delivery_json: serde_json::to_string(&delivery).map_err(|error| {
                                format!("序列化 agent task delivery 失败: {error}")
                            })?,
                        });
                    }
                }
            }
            SCHEDULE_TYPE_MONTHLY_TIME => {
                let schedule: AgentMonthlyTimeSchedule =
                    serde_json::from_str(&item.task.schedule_json).map_err(|error| {
                        format!("解析 monthly agent task schedule 失败: {error}")
                    })?;
                for day in schedule.days_of_month {
                    for time in &schedule.times {
                        jobs.push(MaterializedJob {
                            id: format!("task:{}:m{}:{}", item.task.id, day, time.replace(':', "")),
                            source_kind: SOURCE_KIND_AGENT_TASK.to_string(),
                            owner_agent_id: item.task.agent_id.clone(),
                            source_schedule_id: item.task.id.clone(),
                            source_task_id: item.task.id.clone(),
                            kind: match item.task.task_type.as_str() {
                                "agent_prompt" => "agent_prompt".to_string(),
                                _ => "notify".to_string(),
                            },
                            name: format!("{} @ 每月{}号 {}", item.task.title, day, time),
                            description: item.task.intent_summary.clone(),
                            enabled: item.task.status == "active",
                            timezone: normalize_scheduler_timezone(&item.task.timezone),
                            trigger_type: TRIGGER_TYPE_MONTHLY_TIME.to_string(),
                            trigger_spec_json: serde_json::to_string(&MonthlyTimeTrigger {
                                day_of_month: day,
                                time: time.clone(),
                            })
                            .map_err(|error| format!("序列化 monthly trigger 失败: {error}"))?,
                            payload_json: serde_json::to_string(&base_payload).map_err(
                                |error| {
                                    format!("序列化 agent task scheduler payload 失败: {error}")
                                },
                            )?,
                            delivery_json: serde_json::to_string(&delivery).map_err(|error| {
                                format!("序列化 agent task delivery 失败: {error}")
                            })?,
                        });
                    }
                }
            }
            TRIGGER_TYPE_INTERVAL => {
                let schedule: AgentIntervalSchedule =
                    serde_json::from_str(&item.task.schedule_json).map_err(|error| {
                        format!("解析 interval agent task schedule 失败: {error}")
                    })?;
                let trigger = IntervalTrigger {
                    every_minutes: schedule.every_minutes,
                    start_at: schedule.start_at,
                    end_at: schedule.end_at,
                };
                jobs.push(MaterializedJob {
                    id: format!("task:{}", item.task.id),
                    source_kind: SOURCE_KIND_AGENT_TASK.to_string(),
                    owner_agent_id: item.task.agent_id.clone(),
                    source_schedule_id: item.task.id.clone(),
                    source_task_id: item.task.id.clone(),
                    kind: match item.task.task_type.as_str() {
                        "agent_prompt" => "agent_prompt".to_string(),
                        _ => "notify".to_string(),
                    },
                    name: item.task.title.clone(),
                    description: item.task.intent_summary.clone(),
                    enabled: item.task.status == "active",
                    timezone: normalize_scheduler_timezone(&item.task.timezone),
                    trigger_type: TRIGGER_TYPE_INTERVAL.to_string(),
                    trigger_spec_json: serde_json::to_string(&trigger)
                        .map_err(|error| format!("序列化 interval trigger 失败: {error}"))?,
                    payload_json: serde_json::to_string(&base_payload).map_err(|error| {
                        format!("序列化 agent task scheduler payload 失败: {error}")
                    })?,
                    delivery_json: serde_json::to_string(&delivery)
                        .map_err(|error| format!("序列化 agent task delivery 失败: {error}"))?,
                });
            }
            _ => {
                let schedule: AgentDailyTimeSchedule =
                    serde_json::from_str(&item.task.schedule_json)
                        .map_err(|error| format!("解析 daily agent task schedule 失败: {error}"))?;
                let times = if schedule.times.is_empty() {
                    vec![agent_tasks::default_daily_time_for_now()]
                } else {
                    schedule.times
                };
                for time in times {
                    jobs.push(MaterializedJob {
                        id: format!("task:{}:{}", item.task.id, time.replace(':', "")),
                        source_kind: SOURCE_KIND_AGENT_TASK.to_string(),
                        owner_agent_id: item.task.agent_id.clone(),
                        source_schedule_id: item.task.id.clone(),
                        source_task_id: item.task.id.clone(),
                        kind: match item.task.task_type.as_str() {
                            "agent_prompt" => "agent_prompt".to_string(),
                            _ => "notify".to_string(),
                        },
                        name: format!("{} @ {}", item.task.title, time),
                        description: item.task.intent_summary.clone(),
                        enabled: item.task.status == "active",
                        timezone: normalize_scheduler_timezone(&item.task.timezone),
                        trigger_type: TRIGGER_TYPE_DAILY_TIME.to_string(),
                        trigger_spec_json: serde_json::to_string(&DailyTimeTrigger {
                            time: time.clone(),
                        })
                        .map_err(|error| format!("序列化 daily trigger 失败: {error}"))?,
                        payload_json: serde_json::to_string(&base_payload).map_err(|error| {
                            format!("序列化 agent task scheduler payload 失败: {error}")
                        })?,
                        delivery_json: serde_json::to_string(&delivery)
                            .map_err(|error| format!("序列化 agent task delivery 失败: {error}"))?,
                    });
                }
            }
        }
    }

    Ok(jobs)
}

fn materialized_job_id(agent_id: &str, schedule_id: &str, time: &str) -> String {
    format!("hb:{}:{}:{}", agent_id, schedule_id, time.replace(':', ""))
}

fn normalize_job_kind(task_type: &str) -> String {
    match task_type.trim() {
        "shell" => "shell".to_string(),
        _ => "notify".to_string(),
    }
}

fn determine_next_run_at(
    job: &MaterializedJob,
    existing: Option<&ExistingJobState>,
    now: i64,
) -> Result<Option<i64>, String> {
    if !job.enabled {
        return Ok(None);
    }

    if let Some(existing) = existing {
        let changed = existing.kind != job.kind
            || existing.enabled != job.enabled
            || existing.timezone != job.timezone
            || existing.trigger_type != job.trigger_type
            || existing.trigger_spec_json != job.trigger_spec_json
            || existing.payload_json != job.payload_json
            || existing.delivery_json != job.delivery_json;
        if !changed {
            if let Some(next_run_at) = existing.next_run_at {
                return Ok(Some(next_run_at));
            }
        }
    }

    compute_next_run_at(
        &job.timezone,
        &job.trigger_type,
        &job.trigger_spec_json,
        now,
    )
}

fn claim_due_jobs(connection: &mut Connection, owner_id: &str) -> Result<Vec<ClaimedRun>, String> {
    let available_slots = MAX_PARALLEL_RUNS.saturating_sub(
        active_runs()
            .lock()
            .map(|runs| runs.len())
            .unwrap_or_default(),
    );
    if available_slots == 0 {
        return Ok(Vec::new());
    }

    let now = crate::chrono_like_timestamp();
    let candidates = load_due_candidates(connection, now, available_slots)?;
    let mut claims = Vec::new();

    for candidate in candidates {
        let transaction = connection
            .transaction()
            .map_err(|error| format!("claim scheduler 任务事务失败: {error}"))?;
        let snapshot = transaction
            .query_row(
                "SELECT
                    id,
                    source_kind,
                    source_schedule_id,
                    owner_agent_id,
                    kind,
                    name,
                    timezone,
                    trigger_type,
                    trigger_spec_json,
                    payload_json,
                    delivery_json,
                    next_run_at
                 FROM scheduled_jobs
                 WHERE id = ?1 AND enabled = 1",
                params![candidate.id],
                |row| {
                    Ok(DueJobCandidate {
                        id: row.get(0)?,
                        source_kind: row.get(1)?,
                        source_schedule_id: row.get(2)?,
                        owner_agent_id: row.get(3)?,
                        kind: row.get(4)?,
                        timezone: row.get(6)?,
                        trigger_type: row.get(7)?,
                        trigger_spec_json: row.get(8)?,
                        payload_json: row.get(9)?,
                        delivery_json: row.get(10)?,
                        next_run_at: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(|error| format!("重新读取 due scheduler 任务失败: {error}"))?;
        let Some(snapshot) = snapshot else {
            transaction
                .commit()
                .map_err(|error| format!("提交空 scheduler claim 事务失败: {error}"))?;
            continue;
        };
        if snapshot.next_run_at > now {
            transaction
                .commit()
                .map_err(|error| format!("提交延后 scheduler claim 事务失败: {error}"))?;
            continue;
        }

        let run_id = format!("job_run_{}", Uuid::new_v4().simple());
        let inserted = transaction
            .execute(
                "INSERT OR IGNORE INTO scheduled_job_runs (
                    id, job_id, scheduled_for, claimed_at, started_at, status, attempt, worker_id, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?4, 'running', 1, ?5, ?4, ?4)",
                params![run_id, snapshot.id, snapshot.next_run_at, now, owner_id],
            )
            .map_err(|error| format!("写入 scheduler 运行记录失败: {error}"))?;

        let next_run_at = compute_next_run_at(
            &snapshot.timezone,
            &snapshot.trigger_type,
            &snapshot.trigger_spec_json,
            snapshot.next_run_at,
        )?;
        transaction
            .execute(
                "UPDATE scheduled_jobs SET next_run_at = ?2, updated_at = ?3 WHERE id = ?1",
                params![snapshot.id, next_run_at, now],
            )
            .map_err(|error| format!("推进 scheduler 下一次触发时间失败: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("提交 scheduler claim 事务失败: {error}"))?;

        if inserted == 0 {
            continue;
        }

        claims.push(ClaimedRun {
            run_id,
            job_id: snapshot.id,
            source_kind: snapshot.source_kind,
            source_schedule_id: snapshot.source_schedule_id,
            owner_agent_id: snapshot.owner_agent_id,
            kind: snapshot.kind,
            timezone: snapshot.timezone,
            payload_json: snapshot.payload_json,
            delivery_json: snapshot.delivery_json,
            scheduled_for: snapshot.next_run_at,
        });
    }

    Ok(claims)
}

fn load_due_candidates(
    connection: &Connection,
    now: i64,
    limit: usize,
) -> Result<Vec<DueJobCandidate>, String> {
    let mut statement = connection
        .prepare(
            "SELECT
                id,
                source_kind,
                source_schedule_id,
                owner_agent_id,
                kind,
                name,
                timezone,
                trigger_type,
                trigger_spec_json,
                payload_json,
                delivery_json,
                next_run_at
            FROM scheduled_jobs
            WHERE enabled = 1
              AND next_run_at IS NOT NULL
              AND next_run_at <= ?1
            ORDER BY next_run_at ASC, lower(name) ASC
            LIMIT ?2",
        )
        .map_err(|error| format!("读取 due scheduler 任务失败: {error}"))?;

    let rows = statement
        .query_map(params![now, limit as i64], |row| {
            Ok(DueJobCandidate {
                id: row.get(0)?,
                source_kind: row.get(1)?,
                source_schedule_id: row.get(2)?,
                owner_agent_id: row.get(3)?,
                kind: row.get(4)?,
                timezone: row.get(6)?,
                trigger_type: row.get(7)?,
                trigger_spec_json: row.get(8)?,
                payload_json: row.get(9)?,
                delivery_json: row.get(10)?,
                next_run_at: row.get(11)?,
            })
        })
        .map_err(|error| format!("解析 due scheduler 任务失败: {error}"))?;

    let mut candidates = Vec::new();
    for row in rows {
        candidates.push(row.map_err(|error| format!("读取 due scheduler 行失败: {error}"))?);
    }
    Ok(candidates)
}

fn task_delivery_record_task_id(claim: &ClaimedRun) -> String {
    if claim.source_kind == SOURCE_KIND_AGENT_TASK {
        claim.source_schedule_id.clone()
    } else {
        claim.job_id.clone()
    }
}

/// Pushes plain-text task results to IM channels that are both **bound** on the agent and have a
/// non-empty `target_user_id` on a heartbeat schedule. Skips the primary delivery target, logs and
/// continues on errors.
fn push_scheduler_result_to_auxiliary_im_channels(
    _app: &AppHandle,
    agent: &AgentRecord,
    primary_delivery: &SchedulerDelivery,
    raw_content: &str,
) {
    let text = auxiliary_im_plain_text(raw_content);
    if text.trim().is_empty() {
        return;
    }
    let primary_rt = runtime_channel_id(&agent.id, &primary_delivery.channel_id);
    let primary_uid = primary_delivery.target_user_id.trim();

    let manager = match crate::channel_manager().lock() {
        Ok(guard) => guard,
        Err(error) => {
            log::warn!("scheduler IM 广播：无法锁定通道管理器: {error}");
            return;
        }
    };

    for schedule in &agent.heartbeat_config.schedules {
        if !schedule.enabled {
            continue;
        }
        let ch = schedule.channel_id.trim();
        if ch.is_empty() || ch == "desktop_session" || ch == "peer" {
            continue;
        }
        let Some(bot_cfg) = agent.bot_configs.get(ch) else {
            continue;
        };
        if !bot_cfg.enabled || bot_cfg.im_channel_paused {
            continue;
        }
        let uid = schedule.target_user_id.trim();
        if uid.is_empty() {
            continue;
        }
        let rt = runtime_channel_id(&agent.id, ch);
        if rt == primary_rt && uid == primary_uid {
            continue;
        }
        if let Err(error) = manager.send_message(&rt, uid, &text) {
            log::warn!(
                "scheduler 任务结果 IM 推送跳过: agent={} channel={} user={} error={}",
                agent.id,
                rt,
                uid,
                error
            );
        }
    }
}

fn auxiliary_im_plain_text(raw: &str) -> String {
    let (text, _) = split_text_and_media(raw);
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    let fallback = raw.trim();
    if fallback.is_empty() {
        String::new()
    } else {
        "[定时任务] 输出包含媒体或附件，请在桌面端会话查看完整结果。".to_string()
    }
}

fn execute_claimed_run(app: &AppHandle, claim: &ClaimedRun) -> Result<TaskExecutionResult, String> {
    let payload: SchedulerPayload = serde_json::from_str(&claim.payload_json)
        .map_err(|error| format!("解析 scheduler payload 失败: {error}"))?;
    let delivery: SchedulerDelivery = serde_json::from_str(&claim.delivery_json)
        .map_err(|error| format!("解析 scheduler delivery 失败: {error}"))?;
    let agent = agents::get_agent_record(app, &claim.owner_agent_id)?
        .ok_or_else(|| "scheduler 对应的智能体不存在或已归档".to_string())?;

    match claim.kind.as_str() {
        "shell" => execute_shell_job(app, claim, &agent, &payload, &delivery),
        "agent_prompt" => execute_agent_prompt_job(app, claim, &agent, &payload, &delivery),
        _ => execute_notify_job(app, claim, &agent, &payload, &delivery),
    }
}

fn finalize_run(
    app: &AppHandle,
    claim: &ClaimedRun,
    result: Result<TaskExecutionResult, String>,
) -> Result<(), String> {
    let now = crate::chrono_like_timestamp();
    let (status, summary, details, error, output_path, sent_message) = match result {
        Ok(result) => (
            if result.success { "success" } else { "error" }.to_string(),
            result.summary,
            result.details,
            result.error,
            result.output_path,
            result.sent_message,
        ),
        Err(error) => (
            "error".to_string(),
            "scheduler 任务执行失败".to_string(),
            String::new(),
            Some(error),
            None,
            None,
        ),
    };

    let connection = crate::open_history_db(app)?;
    ensure_scheduler_schema(&connection)?;
    connection
        .execute(
            "UPDATE scheduled_job_runs
             SET status = ?2, summary = ?3, details = ?4, error = ?5, output_path = ?6,
                 finished_at = ?7, updated_at = ?7
             WHERE id = ?1",
            params![
                claim.run_id,
                status,
                summary,
                if details.trim().is_empty() {
                    None::<String>
                } else {
                    Some(details.clone())
                },
                error.clone(),
                output_path.clone(),
                now
            ],
        )
        .map_err(|update_error| format!("更新 scheduler 运行结果失败: {update_error}"))?;
    connection
        .execute(
            "UPDATE scheduled_jobs SET last_run_at = ?2, updated_at = ?3 WHERE id = ?1",
            params![claim.job_id, claim.scheduled_for, now],
        )
        .map_err(|error| format!("更新 scheduler 最后执行时间失败: {error}"))?;

    if let Some(agent) = agents::get_agent_record(app, &claim.owner_agent_id)? {
        let delivery: SchedulerDelivery = serde_json::from_str(&claim.delivery_json)
            .map_err(|parse_error| format!("解析 scheduler delivery 失败: {parse_error}"))?;
        let payload: SchedulerPayload = serde_json::from_str(&claim.payload_json)
            .map_err(|parse_error| format!("解析 scheduler payload 失败: {parse_error}"))?;
        let user_message = format!(
            "[系统定时任务] 规则：{}；任务：{}；计划时间：{}；目标：{}",
            delivery.schedule_name,
            payload.task_name,
            format_scheduled_for(claim.scheduled_for, &claim.timezone),
            display_target(&delivery)
        );
        let assistant_message = match &sent_message {
            Some(text) if !text.trim().is_empty() => text.clone(),
            _ => {
                if let Some(error) = error.clone() {
                    format!("执行失败：{error}")
                } else if details.trim().is_empty() {
                    summary.clone()
                } else {
                    format!("{}\n\n{}", summary, details)
                }
            }
        };

        if agent_tasks::is_desktop_delivery_kind(&delivery.channel_id) {
            let needs_desktop_delivery = status == "error" || sent_message.is_none();
            if needs_desktop_delivery {
                if let Ok(conn) = crate::open_history_db(app) {
                    let title = if delivery.schedule_name.trim().is_empty() {
                        "定时任务".to_string()
                    } else {
                        delivery.schedule_name.clone()
                    };
                    let session_for_history = desktop_delivery_history_session_id();
                    if let Ok(record) = agent_tasks::record_delivery(
                        app,
                        &conn,
                        &task_delivery_record_task_id(claim),
                        &claim.run_id,
                        &agent.id,
                        &session_for_history,
                        &title,
                        &assistant_message,
                    ) {
                        let _ = app.emit("agent-task-delivery", record);
                    }
                }
            }
        }

        let _ = agent_workspace::append_agent_memory_entry(
            &agent.id,
            &delivery.target_user_id,
            &user_message,
            &assistant_message,
        );

        push_scheduler_result_to_auxiliary_im_channels(app, &agent, &delivery, &assistant_message);
    }

    // 仅在实际「到点」触发时暂停一次性任务；「立即执行」的 scheduled_for 为当前时间，与 run_at_ms 不同，不暂停。
    if claim.source_kind == SOURCE_KIND_AGENT_TASK && claim.job_id.ends_with(":once") {
        if let Ok(spec_json) = connection.query_row(
            "SELECT trigger_spec_json FROM scheduled_jobs WHERE id = ?1",
            params![&claim.job_id],
            |row| row.get::<_, String>(0),
        ) {
            if let Ok(spec) = serde_json::from_str::<OnceAtSchedule>(&spec_json) {
                if (claim.scheduled_for - spec.run_at_ms).abs() < 120_000 {
                    let _ = agent_tasks::pause_task(app, &claim.source_schedule_id);
                }
            }
        }
    }

    Ok(())
}

fn execute_notify_job(
    app: &AppHandle,
    claim: &ClaimedRun,
    agent: &AgentRecord,
    payload: &SchedulerPayload,
    delivery: &SchedulerDelivery,
) -> Result<TaskExecutionResult, String> {
    let now_label = format_scheduled_for(claim.scheduled_for, &claim.timezone);
    let sent_message = resolve_task_message(
        app,
        claim,
        agent,
        payload,
        delivery,
        None,
        format!(
            "【{}】\n{}\n触发时间：{}",
            payload.task_name,
            if payload.reminder_text.trim().is_empty() && payload.task_description.trim().is_empty()
            {
                "定时任务已触发。".to_string()
            } else if !payload.reminder_text.trim().is_empty() {
                payload.reminder_text.trim().to_string()
            } else {
                payload.task_description.trim().to_string()
            },
            now_label
        ),
    )?;
    send_task_message(app, claim, agent, delivery, &sent_message)?;

    Ok(TaskExecutionResult {
        success: true,
        summary: format!("已向 {} 发送提醒", delivery.target_user_id),
        details: sent_message.clone(),
        error: None,
        sent_message: Some(sent_message),
        output_path: None,
    })
}

fn execute_agent_prompt_job(
    app: &AppHandle,
    claim: &ClaimedRun,
    agent: &AgentRecord,
    payload: &SchedulerPayload,
    delivery: &SchedulerDelivery,
) -> Result<TaskExecutionResult, String> {
    let generated = execute_agent_prompt(app, claim, agent, payload, delivery)?;
    send_task_message(app, claim, agent, delivery, &generated)?;

    Ok(TaskExecutionResult {
        success: true,
        summary: format!("已执行智能体定时任务：{}", payload.task_name),
        details: generated.clone(),
        error: None,
        sent_message: Some(generated),
        output_path: None,
    })
}

fn execute_shell_job(
    app: &AppHandle,
    claim: &ClaimedRun,
    agent: &AgentRecord,
    payload: &SchedulerPayload,
    delivery: &SchedulerDelivery,
) -> Result<TaskExecutionResult, String> {
    let outcome = run_shell_task(agent, claim, payload)?;
    let output_path = write_shell_artifacts(&claim.run_id, &outcome)?;
    let summary = if outcome.success {
        format!("任务执行成功，耗时 {} ms", outcome.elapsed_ms)
    } else {
        format!(
            "任务执行失败，退出码 {}，耗时 {} ms",
            outcome
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "未知".to_string()),
            outcome.elapsed_ms
        )
    };
    let details = build_shell_details(&outcome);
    let should_notify = if outcome.success {
        payload.notify_on_success
    } else {
        payload.notify_on_failure
    };
    let sent_message = if should_notify {
        let message = resolve_task_message(
            app,
            claim,
            agent,
            payload,
            delivery,
            Some(&outcome),
            default_shell_message(payload, &outcome),
        )?;
        send_task_message(app, claim, agent, delivery, &message)?;
        Some(message)
    } else {
        None
    };

    Ok(TaskExecutionResult {
        success: outcome.success,
        summary,
        details,
        error: if outcome.success {
            None
        } else {
            Some(trim_to_chars(&outcome.stderr, MAX_OUTPUT_CHARS))
        },
        sent_message,
        output_path: Some(output_path.to_string_lossy().to_string()),
    })
}

fn resolve_task_message(
    app: &AppHandle,
    claim: &ClaimedRun,
    agent: &AgentRecord,
    payload: &SchedulerPayload,
    delivery: &SchedulerDelivery,
    outcome: Option<&ShellTaskOutcome>,
    fallback: String,
) -> Result<String, String> {
    if !payload.message_template.trim().is_empty() {
        return Ok(render_message(
            claim,
            agent,
            payload,
            delivery,
            outcome,
            Some(fallback),
        ));
    }

    match generate_agentic_message(app, claim, agent, payload, delivery, outcome) {
        Ok(Some(message)) if !message.trim().is_empty() => Ok(message),
        Ok(_) => Ok(fallback),
        Err(error) => {
            log::warn!(
                "生成 scheduler 智能体文案失败，已回退默认文案: agent={} job={} error={}",
                agent.id,
                claim.job_id,
                error
            );
            Ok(fallback)
        }
    }
}

fn render_message(
    claim: &ClaimedRun,
    agent: &AgentRecord,
    payload: &SchedulerPayload,
    delivery: &SchedulerDelivery,
    outcome: Option<&ShellTaskOutcome>,
    fallback: Option<String>,
) -> String {
    if payload.message_template.trim().is_empty() {
        return fallback.unwrap_or_default();
    }

    let now_label = format_scheduled_for(claim.scheduled_for, &claim.timezone);
    let mut rendered = payload.message_template.clone();
    let replacements = [
        ("agent_id", agent.id.as_str()),
        ("agent_name", agent.name.as_str()),
        ("task_id", claim.job_id.as_str()),
        ("task_name", payload.task_name.as_str()),
        ("schedule_id", delivery.schedule_name.as_str()),
        ("schedule_name", delivery.schedule_name.as_str()),
        ("target_user_id", delivery.target_user_id.as_str()),
        ("target_label", delivery.target_label.as_str()),
        ("now", now_label.as_str()),
        ("description", payload.task_description.as_str()),
        ("command", payload.command.as_str()),
    ];

    for (key, value) in replacements {
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
    }

    if let Some(outcome) = outcome {
        let exit_code = outcome
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "未知".to_string());
        rendered = rendered.replace("{{stdout}}", outcome.stdout.trim());
        rendered = rendered.replace("{{stderr}}", outcome.stderr.trim());
        rendered = rendered.replace("{{exit_code}}", &exit_code);
        rendered = rendered.replace("{{elapsed_ms}}", &outcome.elapsed_ms.to_string());
        rendered = rendered.replace(
            "{{status}}",
            if outcome.success { "success" } else { "error" },
        );
    } else {
        for key in ["stdout", "stderr", "exit_code", "elapsed_ms", "status"] {
            rendered = rendered.replace(&format!("{{{{{key}}}}}"), "");
        }
    }

    rendered.trim().to_string()
}

fn scheduler_session_label(
    delivery: &SchedulerDelivery,
    claim: &ClaimedRun,
    agent_id: &str,
) -> String {
    let trimmed = delivery.target_user_id.trim();
    if trimmed.is_empty() {
        format!("scheduler-session:{agent_id}:{}", claim.run_id)
    } else {
        trimmed.to_string()
    }
}

fn run_scheduler_pi_prompt(
    bridge: &PiBridge,
    app: &AppHandle,
    channel_id: &str,
    synthetic_user: &str,
    prompt: &str,
    claim: &ClaimedRun,
    delivery: &SchedulerDelivery,
    agent: &AgentRecord,
    runtime_model: &str,
    usage_suffix: &str,
) -> Result<String, String> {
    let outcome = bridge.process_message_interruptible(
        channel_id,
        synthetic_user,
        prompt,
        AI_CHUNK_SIZE,
        |_| {},
        |_| {},
    )?;
    match outcome {
        PiProcessOutcome::Completed(result) => {
            let session_key = scheduler_session_label(delivery, claim, &agent.id);
            if let Err(error) = crate::record_token_usage_for_scheduler_pi_completion(
                app,
                format!("{}:{}", claim.run_id, usage_suffix),
                &session_key,
                agent,
                runtime_model,
                result.usage,
                result.usage_meta,
            ) {
                log::warn!("写入 scheduler 任务用量统计失败: {}", error);
            }
            Ok(result.full_text)
        }
        PiProcessOutcome::Aborted => Err("pi 处理被中断".to_string()),
    }
}

fn generate_agentic_message(
    app: &AppHandle,
    claim: &ClaimedRun,
    agent: &AgentRecord,
    payload: &SchedulerPayload,
    delivery: &SchedulerDelivery,
    outcome: Option<&ShellTaskOutcome>,
) -> Result<Option<String>, String> {
    let Some(agent_config) = agents::get_conversation_agent_config(app, &agent.id)? else {
        return Ok(None);
    };

    let runtime = match crate::resolve_im_llm_runtime(
        app,
        &agent.default_provider_id,
        &agent.default_model,
    ) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    let base_normalized = crate::normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );

    let prompt = build_agentic_prompt(claim, payload, delivery, outcome)?;
    let pi_runtime = crate::pi_runtime::require_pi_runtime_location(app)?;
    let bridge = PiBridge::new(
        pi_runtime,
        &runtime.provider_id,
        &runtime.api_format,
        &base_normalized,
        &runtime.api_key,
        &runtime.model,
        Some(agent_config),
    );
    let synthetic_user = format!("{}#scheduler", delivery.target_user_id);
    let channel_id = runtime_channel_id(&agent.id, &delivery.channel_id);
    let message = run_scheduler_pi_prompt(
        &bridge,
        app,
        &channel_id,
        &synthetic_user,
        &prompt,
        claim,
        delivery,
        agent,
        &runtime.model,
        "agentic-msg",
    )?;
    let trimmed = message.trim().to_string();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed))
    }
}

fn execute_agent_prompt(
    app: &AppHandle,
    claim: &ClaimedRun,
    agent: &AgentRecord,
    payload: &SchedulerPayload,
    delivery: &SchedulerDelivery,
) -> Result<String, String> {
    let Some(agent_config) = agents::get_conversation_agent_config(app, &agent.id)? else {
        return Err("找不到用于执行定时任务的智能体配置".to_string());
    };

    let runtime =
        crate::resolve_im_llm_runtime(app, &agent.default_provider_id, &agent.default_model)?;
    let base_normalized = crate::normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );
    let prompt = build_scheduled_agent_prompt(claim, payload, delivery)?;
    let pi_runtime = crate::pi_runtime::require_pi_runtime_location(app)?;
    let bridge = PiBridge::new(
        pi_runtime,
        &runtime.provider_id,
        &runtime.api_format,
        &base_normalized,
        &runtime.api_key,
        &runtime.model,
        Some(agent_config),
    );
    let synthetic_user = format!("{}#scheduled-agent-task", delivery.target_user_id);
    let channel_id = runtime_channel_id(&agent.id, &delivery.channel_id);
    let message = run_scheduler_pi_prompt(
        &bridge,
        app,
        &channel_id,
        &synthetic_user,
        &prompt,
        claim,
        delivery,
        agent,
        &runtime.model,
        "agent-prompt",
    )?;
    let trimmed = message.trim().to_string();
    if trimmed.is_empty() {
        Err("智能体定时任务没有返回内容".to_string())
    } else {
        Ok(trimmed)
    }
}

fn build_scheduled_agent_prompt(
    claim: &ClaimedRun,
    payload: &SchedulerPayload,
    delivery: &SchedulerDelivery,
) -> Result<String, String> {
    let prompt_body = if payload.prompt_template.trim().is_empty() {
        payload.task_description.trim()
    } else {
        payload.prompt_template.trim()
    };
    if prompt_body.is_empty() {
        return Err("智能体定时任务缺少 prompt 内容".to_string());
    }

    Ok(format!(
        "你正在执行一条由用户提前创建的定时任务。\n\n当前触发时间：{}\n任务名称：{}\n目标：{}\n\n请直接完成下面这项任务，并把最终要发给用户的正文直接写出来，不要解释系统内部机制：\n\n{}",
        format_scheduled_for(claim.scheduled_for, &claim.timezone),
        payload.task_name,
        display_target(delivery),
        prompt_body
    ))
}

fn build_agentic_prompt(
    claim: &ClaimedRun,
    payload: &SchedulerPayload,
    delivery: &SchedulerDelivery,
    outcome: Option<&ShellTaskOutcome>,
) -> Result<String, String> {
    let target_display = display_target(delivery);
    let mut sections = vec![
        "你正在替该智能体执行一次定时任务。请直接产出最终要发送给用户的消息正文，不要解释你在执行系统任务。".to_string(),
        format!("当前时间：{}", format_scheduled_for(claim.scheduled_for, &claim.timezone)),
        format!("目标用户：{}", target_display),
        format!("规则名称：{}", delivery.schedule_name),
        format!("任务名称：{}", payload.task_name),
    ];

    if !payload.task_description.trim().is_empty() {
        sections.push(format!("任务说明：{}", payload.task_description.trim()));
    }

    if let Some(instructions) =
        agent_workspace::read_agent_heartbeat_instructions(&claim.owner_agent_id)?
    {
        sections.push(instructions);
    }

    if let Some(outcome) = outcome {
        sections.push(format!(
            "执行结果：状态={}，退出码={}，耗时={} ms。",
            if outcome.success { "success" } else { "error" },
            outcome
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "未知".to_string()),
            outcome.elapsed_ms
        ));
        if !outcome.stdout.trim().is_empty() {
            sections.push(format!(
                "stdout（已截断）:\n```text\n{}\n```",
                trim_to_chars(&outcome.stdout, 1_500)
            ));
        }
        if !outcome.stderr.trim().is_empty() {
            sections.push(format!(
                "stderr（已截断）:\n```text\n{}\n```",
                trim_to_chars(&outcome.stderr, 1_000)
            ));
        }
    }

    sections.push(
        "输出要求：1. 直接写用户会看到的正文；2. 尽量简洁、有行动导向；3. 如果需要发送本地图片、文件或视频，使用单独一行 `::nc-media{type=\"image|file|video\" path=\"/absolute/path\"}`；4. 不要输出额外的 JSON、说明或分析过程。".to_string(),
    );

    Ok(sections.join("\n\n"))
}

fn send_task_message(
    app: &AppHandle,
    claim: &ClaimedRun,
    agent: &AgentRecord,
    delivery: &SchedulerDelivery,
    content: &str,
) -> Result<(), String> {
    let runtime_channel_id = runtime_channel_id(&agent.id, &delivery.channel_id);
    let (text_content, media_items) = split_text_and_media(content);
    if text_content.is_empty() && media_items.is_empty() {
        return Err("scheduler 任务没有可发送的文本或媒体内容".to_string());
    }

    if agent_tasks::is_desktop_delivery_kind(&delivery.channel_id) {
        let connection = crate::open_history_db(app)?;
        let title = if delivery.schedule_name.trim().is_empty() {
            "定时任务".to_string()
        } else {
            delivery.schedule_name.clone()
        };
        let outbound = if text_content.is_empty() {
            agent_tasks::default_desktop_delivery_message(&title, "定时任务已触发。")
        } else {
            text_content.clone()
        };
        let session_for_history = desktop_delivery_history_session_id();
        let record = agent_tasks::record_delivery(
            app,
            &connection,
            &task_delivery_record_task_id(claim),
            &claim.run_id,
            &agent.id,
            &session_for_history,
            &title,
            &outbound,
        )?;
        let _ = app.emit("agent-task-delivery", record);
        return Ok(());
    }

    let manager = crate::channel_manager()
        .lock()
        .map_err(|error| format!("锁定 IM 通道管理器失败: {error}"))?;
    if !text_content.is_empty() {
        manager.send_message(&runtime_channel_id, &delivery.target_user_id, &text_content)?;
    }

    for media in media_items {
        let data = fs::read(&media.file_path)
            .map_err(|error| format!("读取 scheduler 媒体文件失败 {}: {error}", media.file_path))?;
        let payload = MediaPayload {
            media_type: media.media_type,
            file_name: media.file_name,
            data,
        };
        manager.send_media(&runtime_channel_id, &delivery.target_user_id, &payload)?;
    }

    Ok(())
}

fn runtime_channel_id(agent_id: &str, schedule_channel_id: &str) -> String {
    if schedule_channel_id.contains(':') {
        schedule_channel_id.to_string()
    } else {
        format!("{schedule_channel_id}:{agent_id}")
    }
}

fn run_shell_task(
    agent: &AgentRecord,
    claim: &ClaimedRun,
    payload: &SchedulerPayload,
) -> Result<ShellTaskOutcome, String> {
    let workspace_root = agent_workspace::resolve_workspace_root()?;
    let agent_home = workspace_root.join("agents").join(&agent.id);
    let workdir = resolve_task_workdir(&agent_home, &payload.working_directory);
    let started_at = Instant::now();

    let mut command = if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", payload.command.as_str()]);
        cmd
    } else {
        let mut cmd = Command::new("zsh");
        cmd.args(["-lc", payload.command.as_str()]);
        cmd
    };

    command
        .current_dir(&workdir)
        .env("NINECLAW_AGENT_ID", &agent.id)
        .env("NINECLAW_AGENT_NAME", &agent.name)
        .env("NINECLAW_WORKSPACE_ROOT", workspace_root.as_os_str())
        .env("NINECLAW_AGENT_HOME", agent_home.as_os_str())
        .env("NINECLAW_RUN_ID", &claim.run_id)
        .env(
            "NINECLAW_SCHEDULED_FOR",
            format_scheduled_for(claim.scheduled_for, &claim.timezone),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("启动 scheduler shell 任务失败: {error}"))?;
    let timeout = Duration::from_secs(payload.timeout_sec.max(10) as u64);
    let deadline = Instant::now() + timeout;

    loop {
        if let Some(_status) = child
            .try_wait()
            .map_err(|error| format!("检查 scheduler shell 任务状态失败: {error}"))?
        {
            break;
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .map_err(|error| format!("回收超时 scheduler shell 任务失败: {error}"))?;
            return Ok(ShellTaskOutcome {
                success: false,
                exit_code: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: format!(
                    "任务超时（{} 秒）\n{}",
                    timeout.as_secs(),
                    String::from_utf8_lossy(&output.stderr)
                ),
                elapsed_ms: started_at.elapsed().as_millis(),
            });
        }

        thread::sleep(Duration::from_millis(150));
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("读取 scheduler shell 任务输出失败: {error}"))?;
    Ok(ShellTaskOutcome {
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        elapsed_ms: started_at.elapsed().as_millis(),
    })
}

fn write_shell_artifacts(run_id: &str, outcome: &ShellTaskOutcome) -> Result<PathBuf, String> {
    let root = agent_workspace::resolve_workspace_root()?;
    let artifact_dir = root.join("scheduler").join("runs").join(run_id);
    fs::create_dir_all(&artifact_dir)
        .map_err(|error| format!("创建 scheduler artifact 目录失败: {error}"))?;
    fs::write(artifact_dir.join("stdout.txt"), outcome.stdout.as_bytes())
        .map_err(|error| format!("写入 scheduler stdout artifact 失败: {error}"))?;
    fs::write(artifact_dir.join("stderr.txt"), outcome.stderr.as_bytes())
        .map_err(|error| format!("写入 scheduler stderr artifact 失败: {error}"))?;
    fs::write(
        artifact_dir.join("meta.txt"),
        format!(
            "success={}\nexit_code={}\nelapsed_ms={}\n",
            outcome.success,
            outcome
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            outcome.elapsed_ms
        ),
    )
    .map_err(|error| format!("写入 scheduler meta artifact 失败: {error}"))?;
    Ok(artifact_dir)
}

fn default_shell_message(payload: &SchedulerPayload, outcome: &ShellTaskOutcome) -> String {
    let mut lines = vec![
        format!("【{}】", payload.task_name),
        if outcome.success {
            "任务执行成功。".to_string()
        } else {
            format!(
                "任务执行失败，退出码 {}。",
                outcome
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "未知".to_string())
            )
        },
    ];

    if !outcome.stdout.trim().is_empty() {
        lines.push("stdout 摘要：".to_string());
        lines.push(trim_to_chars(&outcome.stdout, 600));
    }
    if !outcome.stderr.trim().is_empty() {
        lines.push("stderr 摘要：".to_string());
        lines.push(trim_to_chars(&outcome.stderr, 400));
    }

    lines.join("\n")
}

fn build_shell_details(outcome: &ShellTaskOutcome) -> String {
    let mut sections = vec![format!(
        "退出码：{}\n耗时：{} ms",
        outcome
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "未知".to_string()),
        outcome.elapsed_ms
    )];

    if !outcome.stdout.trim().is_empty() {
        sections.push(format!(
            "stdout:\n{}",
            trim_to_chars(&outcome.stdout, MAX_OUTPUT_CHARS)
        ));
    }
    if !outcome.stderr.trim().is_empty() {
        sections.push(format!(
            "stderr:\n{}",
            trim_to_chars(&outcome.stderr, MAX_OUTPUT_CHARS)
        ));
    }

    sections.join("\n\n")
}

fn resolve_task_workdir(agent_home: &Path, configured: &str) -> PathBuf {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return agent_home.to_path_buf();
    }

    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        candidate
    } else {
        agent_home.join(candidate)
    }
}

fn compute_next_run_at(
    timezone: &str,
    trigger_type: &str,
    trigger_spec_json: &str,
    after_ms: i64,
) -> Result<Option<i64>, String> {
    match trigger_type {
        TRIGGER_TYPE_INTERVAL => {
            let trigger: IntervalTrigger = serde_json::from_str(trigger_spec_json)
                .map_err(|error| format!("解析 scheduler interval trigger 失败: {error}"))?;
            let base = trigger.start_at.unwrap_or(after_ms);
            let step = trigger.every_minutes.max(1) * 60 * 1000;
            let mut next = if after_ms < base {
                base
            } else {
                after_ms + step
            };
            if let Some(end_at) = trigger.end_at {
                if next > end_at {
                    return Ok(None);
                }
            }
            if next <= after_ms {
                next = after_ms + step;
            }
            Ok(Some(next))
        }
        TRIGGER_TYPE_DAILY_TIME => {
            let trigger: DailyTimeTrigger = serde_json::from_str(trigger_spec_json)
                .map_err(|error| format!("解析 scheduler trigger 失败: {error}"))?;
            Ok(Some(compute_next_daily_run_at(
                &normalize_scheduler_timezone(timezone),
                &trigger.time,
                after_ms,
            )?))
        }
        TRIGGER_TYPE_WEEKLY_TIME => {
            let trigger: WeeklyTimeTrigger = serde_json::from_str(trigger_spec_json)
                .map_err(|error| format!("解析 scheduler weekly trigger 失败: {error}"))?;
            Ok(Some(compute_next_weekly_run_at(
                &normalize_scheduler_timezone(timezone),
                trigger.day_of_week,
                &trigger.time,
                after_ms,
            )?))
        }
        TRIGGER_TYPE_MONTHLY_TIME => {
            let trigger: MonthlyTimeTrigger = serde_json::from_str(trigger_spec_json)
                .map_err(|error| format!("解析 scheduler monthly trigger 失败: {error}"))?;
            Ok(Some(compute_next_monthly_run_at(
                &normalize_scheduler_timezone(timezone),
                trigger.day_of_month,
                &trigger.time,
                after_ms,
            )?))
        }
        TRIGGER_TYPE_ONCE_AT => {
            let trigger: OnceAtSchedule = serde_json::from_str(trigger_spec_json)
                .map_err(|error| format!("解析 scheduler once_at trigger 失败: {error}"))?;
            if after_ms >= trigger.run_at_ms {
                return Ok(None);
            }
            Ok(Some(trigger.run_at_ms))
        }
        _ => Err(format!("不支持的 scheduler trigger 类型: {trigger_type}")),
    }
}

fn compute_next_daily_run_at(
    timezone: &str,
    time_text: &str,
    after_ms: i64,
) -> Result<i64, String> {
    let time = parse_daily_time(time_text)?;
    let after_utc = DateTime::<Utc>::from_timestamp_millis(after_ms).unwrap_or_else(Utc::now);
    match resolve_timezone(timezone) {
        ResolvedTimezone::Named(tz) => {
            let local_after = after_utc.with_timezone(&tz);
            for day_offset in 0_i64..=7_i64 {
                let date = local_after.date_naive() + ChronoDuration::days(day_offset);
                let naive = date.and_time(time);
                match tz.from_local_datetime(&naive) {
                    LocalResult::Single(candidate) => {
                        let candidate_utc = candidate.with_timezone(&Utc);
                        if candidate_utc > after_utc {
                            return Ok(candidate_utc.timestamp_millis());
                        }
                    }
                    LocalResult::Ambiguous(first, second) => {
                        for candidate in [first, second] {
                            let candidate_utc = candidate.with_timezone(&Utc);
                            if candidate_utc > after_utc {
                                return Ok(candidate_utc.timestamp_millis());
                            }
                        }
                    }
                    LocalResult::None => continue,
                }
            }
        }
        ResolvedTimezone::Fixed(offset) => {
            let local_after = after_utc.with_timezone(&offset);
            for day_offset in 0_i64..=7_i64 {
                let date = local_after.date_naive() + ChronoDuration::days(day_offset);
                let candidate = offset
                    .with_ymd_and_hms(
                        date.year(),
                        date.month(),
                        date.day(),
                        time.hour() as u32,
                        time.minute() as u32,
                        0,
                    )
                    .single();
                if let Some(candidate) = candidate {
                    let candidate_utc = candidate.with_timezone(&Utc);
                    if candidate_utc > after_utc {
                        return Ok(candidate_utc.timestamp_millis());
                    }
                }
            }
        }
    }

    Err("无法计算下一次 scheduler 触发时间".to_string())
}

fn compute_next_weekly_run_at(
    timezone: &str,
    day_of_week: u32,
    time_text: &str,
    after_ms: i64,
) -> Result<i64, String> {
    compute_next_calendar_run_at(timezone, time_text, after_ms, |date| {
        date.weekday().number_from_monday() == day_of_week
    })
}

fn compute_next_monthly_run_at(
    timezone: &str,
    day_of_month: u32,
    time_text: &str,
    after_ms: i64,
) -> Result<i64, String> {
    compute_next_calendar_run_at(timezone, time_text, after_ms, |date| {
        date.day() == day_of_month
    })
}

fn compute_next_calendar_run_at<F>(
    timezone: &str,
    time_text: &str,
    after_ms: i64,
    matcher: F,
) -> Result<i64, String>
where
    F: Fn(chrono::NaiveDate) -> bool,
{
    let time = parse_daily_time(time_text)?;
    let after_utc = DateTime::<Utc>::from_timestamp_millis(after_ms).unwrap_or_else(Utc::now);
    match resolve_timezone(timezone) {
        ResolvedTimezone::Named(tz) => {
            let local_after = after_utc.with_timezone(&tz);
            for day_offset in 0_i64..=370_i64 {
                let date = local_after.date_naive() + ChronoDuration::days(day_offset);
                if !matcher(date) {
                    continue;
                }
                let naive = date.and_time(time);
                match tz.from_local_datetime(&naive) {
                    LocalResult::Single(candidate) => {
                        let candidate_utc = candidate.with_timezone(&Utc);
                        if candidate_utc > after_utc {
                            return Ok(candidate_utc.timestamp_millis());
                        }
                    }
                    LocalResult::Ambiguous(first, second) => {
                        for candidate in [first, second] {
                            let candidate_utc = candidate.with_timezone(&Utc);
                            if candidate_utc > after_utc {
                                return Ok(candidate_utc.timestamp_millis());
                            }
                        }
                    }
                    LocalResult::None => continue,
                }
            }
        }
        ResolvedTimezone::Fixed(offset) => {
            let local_after = after_utc.with_timezone(&offset);
            for day_offset in 0_i64..=370_i64 {
                let date = local_after.date_naive() + ChronoDuration::days(day_offset);
                if !matcher(date) {
                    continue;
                }
                let candidate = offset
                    .with_ymd_and_hms(
                        date.year(),
                        date.month(),
                        date.day(),
                        time.hour() as u32,
                        time.minute() as u32,
                        0,
                    )
                    .single();
                if let Some(candidate) = candidate {
                    let candidate_utc = candidate.with_timezone(&Utc);
                    if candidate_utc > after_utc {
                        return Ok(candidate_utc.timestamp_millis());
                    }
                }
            }
        }
    }

    Err("无法计算下一次 scheduler 触发时间".to_string())
}

fn resolve_timezone(raw: &str) -> ResolvedTimezone {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ResolvedTimezone::Named(chrono_tz::Asia::Shanghai);
    }
    if let Ok(tz) = trimmed.parse::<Tz>() {
        return ResolvedTimezone::Named(tz);
    }
    match trimmed {
        "PRC" => ResolvedTimezone::Named(chrono_tz::Asia::Shanghai),
        "UTC" | "Etc/UTC" | "GMT" => ResolvedTimezone::Named(chrono_tz::UTC),
        value => {
            let seconds = parse_explicit_offset(value).unwrap_or(8 * 3600);
            let offset = FixedOffset::east_opt(seconds)
                .unwrap_or_else(|| FixedOffset::east_opt(8 * 3600).expect("valid default offset"));
            ResolvedTimezone::Fixed(offset)
        }
    }
}

fn normalize_scheduler_timezone(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "Asia/Shanghai".to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_daily_time(value: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(value.trim(), "%H:%M")
        .map_err(|error| format!("解析 scheduler 时间失败「{value}」: {error}"))
}

fn parse_explicit_offset(value: &str) -> Option<i32> {
    let normalized = value.strip_prefix("UTC").unwrap_or(value).trim();
    let sign = if normalized.starts_with('-') { -1 } else { 1 };
    let value = normalized.trim_start_matches(['+', '-']);
    let (hour_raw, minute_raw) = value.split_once(':')?;
    let hour: i32 = hour_raw.parse().ok()?;
    let minute: i32 = minute_raw.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(sign * (hour * 3600 + minute * 60))
}

fn format_scheduled_for(timestamp_ms: i64, timezone: &str) -> String {
    let utc = DateTime::<Utc>::from_timestamp_millis(timestamp_ms).unwrap_or_else(Utc::now);
    match resolve_timezone(timezone) {
        ResolvedTimezone::Named(tz) => utc
            .with_timezone(&tz)
            .format("%Y-%m-%d %H:%M %:z")
            .to_string(),
        ResolvedTimezone::Fixed(offset) => utc
            .with_timezone(&offset)
            .format("%Y-%m-%d %H:%M %:z")
            .to_string(),
    }
}

fn display_target(delivery: &SchedulerDelivery) -> &str {
    if delivery.target_label.trim().is_empty() {
        delivery.target_user_id.as_str()
    } else {
        delivery.target_label.trim()
    }
}

fn split_text_and_media(content: &str) -> (String, Vec<ParsedMediaItem>) {
    let mut text_lines = Vec::new();
    let mut media_items = Vec::new();

    for line in content.lines() {
        if let Some(item) = parse_media_directive(line).or_else(|| parse_markdown_media(line)) {
            media_items.push(item);
        } else {
            text_lines.push(line);
        }
    }

    (text_lines.join("\n").trim().to_string(), media_items)
}

fn parse_media_directive(line: &str) -> Option<ParsedMediaItem> {
    let parsed = parse_media_directive_fields(line)?;
    let path = parsed.path;
    if !Path::new(&path).is_absolute() {
        return None;
    }

    let media_type = match parsed.media_type.as_deref() {
        Some("image") => MediaType::Image,
        Some("video") => MediaType::Video,
        Some("file") => MediaType::File,
        _ => {
            if is_image_path(&path) {
                MediaType::Image
            } else if is_video_path(&path) {
                MediaType::Video
            } else {
                MediaType::File
            }
        }
    };

    Some(ParsedMediaItem {
        media_type,
        file_name: parsed.name.unwrap_or_else(|| file_name_from_path(&path)),
        file_path: path,
    })
}

fn parse_markdown_media(line: &str) -> Option<ParsedMediaItem> {
    let reference = parse_markdown_media_reference(line)?;
    let path = reference.path;
    if !Path::new(&path).is_absolute() {
        return None;
    }

    let media_type = if line.trim().starts_with("![") || is_image_path(&path) {
        MediaType::Image
    } else if is_video_path(&path) {
        MediaType::Video
    } else {
        MediaType::File
    };

    Some(ParsedMediaItem {
        media_type,
        file_name: file_name_from_path(&path),
        file_path: path,
    })
}

fn is_image_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg")
    )
}

fn is_video_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v")
    )
}

fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string())
}

fn trim_to_chars(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

fn current_executable_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|error| format!("读取当前可执行文件路径失败: {error}"))
}

fn scheduler_logs_dir() -> Result<PathBuf, String> {
    let home = home_dir()?;
    Ok(home.join("Library").join("Logs").join("NineClaw"))
}

#[cfg(target_os = "macos")]
fn macos_launch_agent_path() -> Result<PathBuf, String> {
    let home = home_dir()?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{MACOS_LAUNCH_AGENT_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn render_macos_launch_agent(exe: &Path, logs_dir: &Path) -> String {
    let stdout_path = logs_dir.join("scheduler.stdout.log");
    let stderr_path = logs_dir.join("scheduler.stderr.log");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>--scheduler-daemon</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>ProcessType</key>
  <string>Background</string>
  <key>StandardOutPath</key>
  <string>{}</string>
  <key>StandardErrorPath</key>
  <string>{}</string>
</dict>
</plist>
"#,
        xml_escape(MACOS_LAUNCH_AGENT_LABEL),
        xml_escape(&exe.to_string_lossy()),
        xml_escape(&stdout_path.to_string_lossy()),
        xml_escape(&stderr_path.to_string_lossy())
    )
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn home_dir() -> Result<PathBuf, String> {
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home));
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        return Ok(PathBuf::from(home));
    }
    Err("无法推断用户 HOME 目录".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn compute_next_daily_run_supports_iana_timezone() {
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 4, 8, 0, 30, 0)
            .single()
            .expect("utc now")
            .timestamp_millis();
        let next = compute_next_daily_run_at("Asia/Shanghai", "09:00", now).expect("next");
        let rendered = format_scheduled_for(next, "Asia/Shanghai");
        assert!(rendered.starts_with("2026-04-08 09:00"));
    }

    #[test]
    fn compute_next_weekly_run_supports_weekday_trigger() {
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 4, 8, 0, 30, 0)
            .single()
            .expect("utc now")
            .timestamp_millis();
        let next = compute_next_weekly_run_at("Asia/Shanghai", 5, "09:00", now).expect("next");
        let rendered = format_scheduled_for(next, "Asia/Shanghai");
        assert!(rendered.starts_with("2026-04-10 09:00"));
    }

    #[test]
    fn compute_next_monthly_run_supports_day_of_month_trigger() {
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 4, 8, 0, 30, 0)
            .single()
            .expect("utc now")
            .timestamp_millis();
        let next = compute_next_monthly_run_at("Asia/Shanghai", 20, "09:00", now).expect("next");
        let rendered = format_scheduled_for(next, "Asia/Shanghai");
        assert!(rendered.starts_with("2026-04-20 09:00"));
    }

    #[test]
    fn split_text_and_media_extracts_directives_and_markdown_paths() {
        let (text, media) = split_text_and_media(
            "早上好\n::nc-media{type=\"image\" path=\"/tmp/report.png\"}\n[日报](/tmp/daily.pdf)",
        );

        assert_eq!(text, "早上好");
        assert_eq!(media.len(), 2);
    }

    #[test]
    fn materialize_daily_agent_task_accepts_legacy_schedule_without_times() {
        let connection = Connection::open_in_memory().expect("memory db");
        agent_tasks::ensure_agent_task_schema(&connection).expect("schema");

        connection
            .execute(
                "INSERT INTO agent_tasks (
                    id, agent_id, source_session_id, creator_user_id, title, intent_summary,
                    task_type, schedule_type, timezone, payload_json, schedule_json, delivery_json,
                    status, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    "task_legacy_daily",
                    "agent_1",
                    "session_1",
                    "user_1",
                    "旧每日任务",
                    "兼容旧 schedule_json",
                    "reminder",
                    "daily_time",
                    "Asia/Shanghai",
                    r#"{"goal":"提醒我","reminderText":"提醒我","promptTemplate":""}"#,
                    r#"{"daysOfWeek":[1,2,3,4,5,6,7]}"#,
                    r#"{"kind":"desktop_session","sessionId":"session_1","resultInNewSession":false}"#,
                    "active",
                    0_i64,
                    0_i64,
                ],
            )
            .expect("insert task");

        let jobs = materialize_jobs_from_agent_tasks(&connection).expect("materialize jobs");

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].trigger_type, "daily_time");
        assert!(jobs[0].id.starts_with("task:task_legacy_daily:"));
    }
}
