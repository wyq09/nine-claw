use crate::agent_workspace;
use crate::agents::{self, AgentHeartbeatSchedule, AgentHeartbeatTask, AgentRecord};
use crate::channels::pi_bridge::PiBridge;
use crate::channels::types::{MediaPayload, MediaType};
use chrono::{DateTime, Datelike, Duration as ChronoDuration, FixedOffset, TimeZone, Utc};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use uuid::Uuid;

const HEARTBEAT_SCAN_INTERVAL_SECS: u64 = 30;
const HEARTBEAT_GRACE_MINUTES: i64 = 10;
const MAX_OUTPUT_CHARS: usize = 2_400;
const HEARTBEAT_AI_CHUNK_SIZE: usize = 900;

static HEARTBEAT_SCHEDULER_STARTED: OnceLock<()> = OnceLock::new();

#[derive(Clone)]
struct DueHeartbeat {
    agent: AgentRecord,
    task: AgentHeartbeatTask,
    schedule: AgentHeartbeatSchedule,
    scheduled_for: DateTime<FixedOffset>,
}

struct ShellTaskOutcome {
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    elapsed_ms: u128,
}

#[derive(Clone, Debug)]
struct ParsedMediaItem {
    media_type: MediaType,
    file_name: String,
    file_path: String,
}

pub fn ensure_heartbeat_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_heartbeat_runs (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                schedule_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                scheduled_for INTEGER NOT NULL,
                status TEXT NOT NULL,
                summary TEXT,
                details TEXT,
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(agent_id, schedule_id, scheduled_for)
            );
            CREATE INDEX IF NOT EXISTS idx_agent_heartbeat_runs_agent_schedule
            ON agent_heartbeat_runs(agent_id, schedule_id, scheduled_for DESC);",
        )
        .map_err(|error| format!("初始化心跳任务运行表失败: {error}"))?;

    Ok(())
}

pub fn start_heartbeat_scheduler(app: AppHandle) {
    if HEARTBEAT_SCHEDULER_STARTED.set(()).is_err() {
        return;
    }

    thread::spawn(move || {
        if let Err(error) = scan_due_heartbeats(&app) {
            log::warn!("首次扫描心跳任务失败: {}", error);
        }

        loop {
            thread::sleep(Duration::from_secs(HEARTBEAT_SCAN_INTERVAL_SECS));
            if let Err(error) = scan_due_heartbeats(&app) {
                log::warn!("扫描心跳任务失败: {}", error);
            }
        }
    });
}

fn scan_due_heartbeats(app: &AppHandle) -> Result<(), String> {
    let agents = agents::list_agents(app)?;
    let now = Utc::now();
    let mut due_items = Vec::new();

    for agent in agents {
        if agent.heartbeat_config.tasks.is_empty() || agent.heartbeat_config.schedules.is_empty() {
            continue;
        }

        let offset = parse_timezone_offset(&agent.heartbeat_config.timezone);
        let due_for_agent = collect_due_heartbeats(agent, offset, now)?;
        due_items.extend(due_for_agent);
    }

    for due in due_items {
        if let Err(error) = execute_due_heartbeat(app, due) {
            log::warn!("执行心跳任务失败: {}", error);
        }
    }

    Ok(())
}

fn collect_due_heartbeats(
    agent: AgentRecord,
    offset: FixedOffset,
    now_utc: DateTime<Utc>,
) -> Result<Vec<DueHeartbeat>, String> {
    let now_local = now_utc.with_timezone(&offset);
    let grace = ChronoDuration::minutes(HEARTBEAT_GRACE_MINUTES);
    let task_map: HashMap<_, _> = agent
        .heartbeat_config
        .tasks
        .iter()
        .cloned()
        .map(|task| (task.id.clone(), task))
        .collect();
    let mut due_items = Vec::new();

    for schedule in &agent.heartbeat_config.schedules {
        if !schedule.enabled || schedule.schedule_type != "daily" || schedule.task_id.is_empty() {
            continue;
        }
        let Some(task) = task_map.get(&schedule.task_id).cloned() else {
            continue;
        };

        for time_text in &schedule.times {
            let Some(scheduled_for) = compute_due_time(now_local, time_text, grace) else {
                continue;
            };

            due_items.push(DueHeartbeat {
                agent: agent.clone(),
                task: task.clone(),
                schedule: schedule.clone(),
                scheduled_for,
            });
        }
    }

    Ok(due_items)
}

fn compute_due_time(
    now_local: DateTime<FixedOffset>,
    time_text: &str,
    grace: ChronoDuration,
) -> Option<DateTime<FixedOffset>> {
    let (hour, minute) = parse_time_parts(time_text)?;
    for day_offset in [0_i64, -1_i64] {
        let date = now_local.date_naive() + ChronoDuration::days(day_offset);
        let scheduled = now_local.timezone().with_ymd_and_hms(
            date.year(),
            date.month(),
            date.day(),
            hour,
            minute,
            0,
        );
        let scheduled = scheduled.single()?;
        if scheduled <= now_local && now_local - scheduled <= grace {
            return Some(scheduled);
        }
    }
    None
}

fn execute_due_heartbeat(app: &AppHandle, due: DueHeartbeat) -> Result<(), String> {
    let mut connection = crate::open_history_db(app)?;
    let scheduled_for_ms = due.scheduled_for.timestamp_millis();
    let Some(run_id) = claim_run(
        &connection,
        &due.agent.id,
        &due.schedule.id,
        &due.task.id,
        scheduled_for_ms,
    )?
    else {
        return Ok(());
    };

    let result = execute_task(app, &due);
    let (status, summary, details, error_message, sent_message) = match result {
        Ok(task_result) => {
            let status = if task_result.success {
                "success"
            } else {
                "error"
            };
            (
                status.to_string(),
                task_result.summary,
                task_result.details,
                task_result.error,
                task_result.sent_message,
            )
        }
        Err(error) => (
            "error".to_string(),
            "心跳任务执行失败".to_string(),
            String::new(),
            Some(error),
            None,
        ),
    };

    finalize_run(
        &mut connection,
        &run_id,
        &status,
        &summary,
        if details.is_empty() {
            None
        } else {
            Some(details.as_str())
        },
        error_message.as_deref(),
    )?;

    let memory_user_message = format!(
        "[系统心跳] 规则：{}；任务：{}；计划时间：{}；目标：{}",
        due.schedule.name,
        due.task.name,
        due.scheduled_for.format("%Y-%m-%d %H:%M %:z"),
        due.schedule
            .target_label
            .trim()
            .strip_prefix('@')
            .unwrap_or_else(|| due.schedule.target_user_id.as_str())
    );
    let memory_assistant_message = sent_message.unwrap_or_else(|| {
        if let Some(error) = error_message {
            format!("执行失败：{error}")
        } else if details.is_empty() {
            summary.clone()
        } else {
            format!("{}\n\n{}", summary, details)
        }
    });
    let _ = agent_workspace::append_agent_memory_entry(
        &due.agent.id,
        &due.schedule.target_user_id,
        &memory_user_message,
        &memory_assistant_message,
    );

    Ok(())
}

fn claim_run(
    connection: &Connection,
    agent_id: &str,
    schedule_id: &str,
    task_id: &str,
    scheduled_for: i64,
) -> Result<Option<String>, String> {
    let run_id = format!("run_{}", Uuid::new_v4().simple());
    let now = crate::chrono_like_timestamp();
    let inserted = connection
        .execute(
            "INSERT OR IGNORE INTO agent_heartbeat_runs (
                id, agent_id, schedule_id, task_id, scheduled_for, status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6, ?6)",
            params![run_id, agent_id, schedule_id, task_id, scheduled_for, now],
        )
        .map_err(|error| format!("写入心跳运行记录失败: {error}"))?;

    if inserted == 0 {
        Ok(None)
    } else {
        Ok(Some(run_id))
    }
}

fn finalize_run(
    connection: &mut Connection,
    run_id: &str,
    status: &str,
    summary: &str,
    details: Option<&str>,
    error: Option<&str>,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE agent_heartbeat_runs
            SET status = ?2, summary = ?3, details = ?4, error = ?5, updated_at = ?6
            WHERE id = ?1",
            params![
                run_id,
                status,
                summary,
                details,
                error,
                crate::chrono_like_timestamp()
            ],
        )
        .map_err(|update_error| format!("更新心跳运行结果失败: {update_error}"))?;

    Ok(())
}

struct TaskExecutionResult {
    success: bool,
    summary: String,
    details: String,
    error: Option<String>,
    sent_message: Option<String>,
}

fn execute_task(app: &AppHandle, due: &DueHeartbeat) -> Result<TaskExecutionResult, String> {
    match due.task.task_type.as_str() {
        "shell" => execute_shell_heartbeat_task(app, due),
        _ => execute_notify_heartbeat_task(app, due),
    }
}

fn execute_notify_heartbeat_task(
    app: &AppHandle,
    due: &DueHeartbeat,
) -> Result<TaskExecutionResult, String> {
    let now_label = due.scheduled_for.format("%Y-%m-%d %H:%M").to_string();
    let sent_message = resolve_task_message(
        app,
        due,
        None,
        format!(
            "【{}】\n{}\n触发时间：{}",
            due.task.name,
            if due.task.description.trim().is_empty() {
                "定时心跳已触发。".to_string()
            } else {
                due.task.description.trim().to_string()
            },
            now_label
        ),
    );
    send_task_message(due, &sent_message)?;

    Ok(TaskExecutionResult {
        success: true,
        summary: format!("已向 {} 发送提醒", due.schedule.target_user_id),
        details: sent_message.clone(),
        error: None,
        sent_message: Some(sent_message),
    })
}

fn execute_shell_heartbeat_task(
    app: &AppHandle,
    due: &DueHeartbeat,
) -> Result<TaskExecutionResult, String> {
    let outcome = run_shell_task(app, due)?;
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
        due.task.notify_on_success
    } else {
        due.task.notify_on_failure
    };
    let sent_message = if should_notify {
        let message = resolve_task_message(
            app,
            due,
            Some(&outcome),
            default_shell_message(due, &outcome),
        );
        send_task_message(due, &message)?;
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
            Some(outcome.stderr.clone())
        },
        sent_message,
    })
}

fn default_shell_message(due: &DueHeartbeat, outcome: &ShellTaskOutcome) -> String {
    let mut lines = vec![
        format!("【{}】", due.task.name),
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

fn resolve_task_message(
    app: &AppHandle,
    due: &DueHeartbeat,
    outcome: Option<&ShellTaskOutcome>,
    fallback: String,
) -> String {
    if !due.task.message_template.trim().is_empty() {
        return render_message(due, outcome, Some(fallback));
    }

    match generate_agentic_heartbeat_message(app, due, outcome) {
        Ok(Some(message)) if !message.trim().is_empty() => message,
        Ok(_) => fallback,
        Err(error) => {
            log::warn!(
                "生成智能体心跳文案失败，已回退到默认文案: agent={} task={} error={}",
                due.agent.id,
                due.task.id,
                error
            );
            fallback
        }
    }
}

fn render_message(
    due: &DueHeartbeat,
    outcome: Option<&ShellTaskOutcome>,
    fallback: Option<String>,
) -> String {
    if due.task.message_template.trim().is_empty() {
        return fallback.unwrap_or_default();
    }

    let now_label = due.scheduled_for.format("%Y-%m-%d %H:%M").to_string();
    let mut rendered = due.task.message_template.clone();
    let replacements = [
        ("agent_id", due.agent.id.as_str()),
        ("agent_name", due.agent.name.as_str()),
        ("task_id", due.task.id.as_str()),
        ("task_name", due.task.name.as_str()),
        ("schedule_id", due.schedule.id.as_str()),
        ("schedule_name", due.schedule.name.as_str()),
        ("target_user_id", due.schedule.target_user_id.as_str()),
        ("target_label", due.schedule.target_label.as_str()),
        ("now", now_label.as_str()),
        ("description", due.task.description.as_str()),
        ("command", due.task.command.as_str()),
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

fn send_task_message(due: &DueHeartbeat, content: &str) -> Result<(), String> {
    let runtime_channel_id = runtime_channel_id(&due.agent.id, &due.schedule.channel_id);
    let (text_content, media_items) = split_text_and_media(content);
    if text_content.is_empty() && media_items.is_empty() {
        return Err("心跳任务没有可发送的文本或媒体内容".to_string());
    }

    let manager = crate::channel_manager()
        .lock()
        .map_err(|error| format!("锁定 IM 通道管理器失败: {error}"))?;
    if !text_content.is_empty() {
        manager.send_message(
            &runtime_channel_id,
            &due.schedule.target_user_id,
            &text_content,
        )?;
    }

    for media in media_items {
        let data = fs::read(&media.file_path)
            .map_err(|error| format!("读取心跳媒体文件失败 {}: {error}", media.file_path))?;
        let payload = MediaPayload {
            media_type: media.media_type,
            file_name: media.file_name,
            data,
        };
        manager.send_media(&runtime_channel_id, &due.schedule.target_user_id, &payload)?;
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

fn run_shell_task(_app: &AppHandle, due: &DueHeartbeat) -> Result<ShellTaskOutcome, String> {
    let workspace_root = agent_workspace::resolve_workspace_root()?;
    let agent_home = workspace_root.join("agents").join(&due.agent.id);
    let workdir = resolve_task_workdir(&agent_home, &due.task.working_directory);
    let started_at = Instant::now();

    let mut command = if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", due.task.command.as_str()]);
        cmd
    } else {
        let mut cmd = Command::new("zsh");
        cmd.args(["-lc", due.task.command.as_str()]);
        cmd
    };

    command
        .current_dir(&workdir)
        .env("NINECLAW_AGENT_ID", &due.agent.id)
        .env("NINECLAW_AGENT_NAME", &due.agent.name)
        .env("NINECLAW_WORKSPACE_ROOT", workspace_root.as_os_str())
        .env("NINECLAW_AGENT_HOME", agent_home.as_os_str())
        .env(
            "NINECLAW_HEARTBEAT_AT",
            due.scheduled_for
                .format("%Y-%m-%d %H:%M:%S %:z")
                .to_string(),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("启动 shell 任务失败: {error}"))?;
    let timeout = Duration::from_secs(due.task.timeout_sec.max(10) as u64);
    let deadline = Instant::now() + timeout;

    loop {
        if let Some(_status) = child
            .try_wait()
            .map_err(|error| format!("检查 shell 任务状态失败: {error}"))?
        {
            break;
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .map_err(|error| format!("回收超时 shell 任务失败: {error}"))?;
            return Ok(ShellTaskOutcome {
                success: false,
                exit_code: output.status.code(),
                stdout: trim_to_chars(&String::from_utf8_lossy(&output.stdout), MAX_OUTPUT_CHARS),
                stderr: trim_to_chars(
                    &format!(
                        "任务超时（{} 秒）\n{}",
                        timeout.as_secs(),
                        String::from_utf8_lossy(&output.stderr)
                    ),
                    MAX_OUTPUT_CHARS,
                ),
                elapsed_ms: started_at.elapsed().as_millis(),
            });
        }

        thread::sleep(Duration::from_millis(150));
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("读取 shell 任务输出失败: {error}"))?;
    Ok(ShellTaskOutcome {
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout: trim_to_chars(&String::from_utf8_lossy(&output.stdout), MAX_OUTPUT_CHARS),
        stderr: trim_to_chars(&String::from_utf8_lossy(&output.stderr), MAX_OUTPUT_CHARS),
        elapsed_ms: started_at.elapsed().as_millis(),
    })
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

fn parse_timezone_offset(raw: &str) -> FixedOffset {
    match raw.trim() {
        "" | "Asia/Shanghai" | "Asia/Chongqing" | "PRC" => {
            FixedOffset::east_opt(8 * 3600).expect("valid shanghai offset")
        }
        "UTC" | "Etc/UTC" | "GMT" => FixedOffset::east_opt(0).expect("valid utc offset"),
        value => parse_explicit_offset(value)
            .and_then(FixedOffset::east_opt)
            .unwrap_or_else(|| FixedOffset::east_opt(8 * 3600).expect("valid default offset")),
    }
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

fn parse_time_parts(value: &str) -> Option<(u32, u32)> {
    let (hour_raw, minute_raw) = value.split_once(':')?;
    let hour = hour_raw.parse().ok()?;
    let minute = minute_raw.parse().ok()?;
    if hour > 23 || minute > 59 {
        None
    } else {
        Some((hour, minute))
    }
}

fn trim_to_chars(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

fn generate_agentic_heartbeat_message(
    app: &AppHandle,
    due: &DueHeartbeat,
    outcome: Option<&ShellTaskOutcome>,
) -> Result<Option<String>, String> {
    let channel_key = configured_channel_key(&due.schedule.channel_id);
    let bot_config = due.agent.bot_configs.get(channel_key);

    let provider_id = bot_config
        .and_then(|config| config.ai_provider_id.as_ref())
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(due.agent.default_provider_id.trim());
    let model = bot_config
        .and_then(|config| config.ai_model.as_ref())
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(due.agent.default_model.trim());

    if provider_id.is_empty() || model.is_empty() {
        return Ok(None);
    }

    let api_format = bot_config
        .and_then(|config| config.ai_api_format.as_ref())
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(default_provider_api_format(provider_id));
    let base_url = bot_config
        .and_then(|config| config.ai_base_url.as_ref())
        .map(|value| value.trim())
        .unwrap_or_default();
    let api_key = bot_config
        .and_then(|config| config.ai_api_key.as_ref())
        .map(|value| value.trim())
        .unwrap_or_default();

    let Some(agent_config) = agents::get_conversation_agent_config(app, &due.agent.id)? else {
        return Ok(None);
    };

    let prompt = build_agentic_heartbeat_prompt(due, outcome)?;
    let pi_executable = crate::pi_runtime::require_pi_executable(app)?;
    let bridge = PiBridge::new(
        pi_executable,
        provider_id,
        api_format,
        base_url,
        api_key,
        model,
        Some(agent_config),
    );
    let synthetic_user = format!("{}#heartbeat", due.schedule.target_user_id);
    let channel_id = runtime_channel_id(&due.agent.id, &due.schedule.channel_id);
    let message = bridge.process_message(
        &channel_id,
        &synthetic_user,
        &prompt,
        HEARTBEAT_AI_CHUNK_SIZE,
        |_| {},
    )?;
    let trimmed = message.trim().to_string();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed))
    }
}

fn build_agentic_heartbeat_prompt(
    due: &DueHeartbeat,
    outcome: Option<&ShellTaskOutcome>,
) -> Result<String, String> {
    let target_display = if due.schedule.target_label.trim().is_empty() {
        due.schedule.target_user_id.as_str()
    } else {
        due.schedule.target_label.trim()
    };
    let mut sections = vec![
        "你正在替该智能体执行一次定时心跳任务。请直接产出最终要发送给用户的消息正文，不要解释你在执行系统任务。".to_string(),
        format!("当前时间：{}", due.scheduled_for.format("%Y-%m-%d %H:%M %:z")),
        format!("目标用户：{}", target_display),
        format!("规则名称：{}", due.schedule.name),
        format!("任务名称：{}", due.task.name),
    ];

    if !due.task.description.trim().is_empty() {
        sections.push(format!("任务说明：{}", due.task.description.trim()));
    }
    if let Some(instructions) = agent_workspace::read_agent_heartbeat_instructions(&due.agent.id)? {
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

fn configured_channel_key(channel_id: &str) -> &str {
    channel_id.split(':').next().unwrap_or(channel_id)
}

fn default_provider_api_format(provider_id: &str) -> &'static str {
    match provider_id {
        "anthropic" => "anthropic",
        _ => "openai",
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
    let trimmed = line.trim();
    if !trimmed.starts_with("::nc-media{") || !trimmed.ends_with('}') {
        return None;
    }

    let body = &trimmed["::nc-media{".len()..trimmed.len() - 1];
    let mut media_type = None;
    let mut path = None;

    for pair in body.split_whitespace() {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let normalized = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key {
            "type" => media_type = Some(normalized),
            "path" => path = Some(normalized),
            _ => {}
        }
    }

    let path = path?;
    if !Path::new(&path).is_absolute() {
        return None;
    }

    let media_type = match media_type.as_deref() {
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
        file_name: file_name_from_path(&path),
        file_path: path,
    })
}

fn parse_markdown_media(line: &str) -> Option<ParsedMediaItem> {
    let trimmed = line.trim();
    let start = trimmed.find('(')?;
    let end = trimmed.rfind(')')?;
    if end <= start + 1 {
        return None;
    }

    let path = trimmed[start + 1..end].trim();
    if !Path::new(path).is_absolute() {
        return None;
    }

    let media_type = if trimmed.starts_with("![") || is_image_path(path) {
        MediaType::Image
    } else if is_video_path(path) {
        MediaType::Video
    } else {
        MediaType::File
    };

    Some(ParsedMediaItem {
        media_type,
        file_name: file_name_from_path(path),
        file_path: path.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn split_text_and_media_extracts_directives_and_markdown_paths() {
        let (text, media) = split_text_and_media(
            "早上好\n::nc-media{type=\"image\" path=\"/tmp/report.png\"}\n[日报](/tmp/daily.pdf)",
        );

        assert_eq!(text, "早上好");
        assert_eq!(media.len(), 2);
    }

    #[test]
    fn compute_due_time_matches_recent_daily_slot() {
        let offset = FixedOffset::east_opt(8 * 3600).expect("offset");
        let now = offset
            .with_ymd_and_hms(2026, 4, 4, 8, 5, 0)
            .single()
            .expect("now");
        let due = compute_due_time(now, "08:00", ChronoDuration::minutes(10)).expect("due");

        assert_eq!(due.hour(), 8);
        assert_eq!(due.minute(), 0);
    }
}
