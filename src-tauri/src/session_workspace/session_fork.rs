//! 会话分支（fork）：把源会话截止到某一轮（含该轮）的历史复制成新会话。
//!
//! 与 PI 自身的 `SessionManager.forkFrom` 语义对齐：新 JSONL 写一个新
//! header（`parentSession` 指向源文件做溯源），正文按 fork 点截断后
//! 原样复制。除 PI 会话文件外，还需要同步复制三份状态：
//! 1. SQLite `chat_sessions` / `chat_turns`（前端会话列表与消息渲染）；
//! 2. agent 侧会话事件流（`memory/sessions/<sid>.jsonl`，`execution_mode`
//!    的权威值是事件流 fold，fork 后子会话 fold 出与 fork 点一致的模式）；
//! 3. 多模态摘要（`runtime_multimodal_summaries`，后续文本轮会把这些
//!    图片问答摘要注入上下文）。
//!
//! 工作区两种模式：`share` 共用源会话目录；`copy` 把源当前工作区内容
//! 完整复制到新会话的默认目录。核心切点计算是纯函数，路径由调用方
//! 注入，便于不依赖 AppHandle 做单元测试。

use crate::storage;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const WORKSPACE_MODE_SHARE: &str = "share";
pub(crate) const WORKSPACE_MODE_COPY: &str = "copy";

#[derive(Debug, Clone)]
pub(crate) struct ChatForkParams {
    pub source_session_id: String,
    /// fork 点：新分支包含该轮（含）之前的所有轮次。
    pub fork_turn_id: String,
    pub new_session_id: String,
    /// `share` 共享工作区；`copy` 独立拷贝。
    pub workspace_mode: String,
}

/// 由命令层注入的运行时文件路径（生产取 app data 目录，测试注入临时目录）。
#[derive(Debug, Clone)]
pub(crate) struct ChatForkRuntimePaths {
    pub source_jsonl: PathBuf,
    pub target_jsonl: PathBuf,
    /// 源 JSONL 缺 header 时的 cwd 兜底（agent home）。
    pub fallback_cwd: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatForkOutcome {    pub new_session_id: String,
    pub copied_turns: usize,
    pub kept_pi_entries: usize,
    pub copied_event_lines: usize,
}

// ── PI JSONL 纯函数 ────────────────────────────────────────────────────

fn parse_line(line: &str) -> Option<Value> {
    if line.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<Value>(line).ok()
}

fn is_session_header(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("session")
}

/// 提取 `message.role == "user"` 条目的文本内容；content 兼容字符串与
/// 分块数组（拼接其中 `type == "text"` 的 text）。
fn user_message_text(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let message = value.get("message")?;
    if message.get("role").and_then(Value::as_str) != Some("user") {
        return None;
    }
    match message.get("content")? {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let mut text = String::new();
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(chunk) = part.get("text").and_then(Value::as_str) {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(chunk);
                    }
                }
            }
            Some(text)
        }
        _ => None,
    }
}

/// PI 会话文件按 fork 点截断后的结果。
pub(crate) struct PiForkPrefix {
    /// 保留下来的正文行（不含 header，原样保留不重排）。
    pub kept_lines: Vec<String>,
    /// 匹配到 fork 范围内轮次的 user 消息条数。
    pub matched_user_entries: usize,
}

/// `lines` 中 `after` 之后的下一个 user 消息行下标（不含 header）。
fn next_user_entry_line(lines: &[&str], after: usize) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .skip(after + 1)
        .find(|(_, line)| {
            parse_line(line)
                .map(|value| !is_session_header(&value) && user_message_text(&value).is_some())
                .unwrap_or(false)
        })
        .map(|(index, _)| index)
}

/// 计算源 PI JSONL 的保留前缀。
///
/// 切点定位：按顺序扫描 user 消息条目，与 `allowed_prompts`（fork 范围内
/// 各轮的 trimmed prompt，按序）做后缀匹配——桌面端发出的 user 消息是
/// `<nineclaw_turn_context>…</nineclaw_turn_context>\n\n{原始 prompt}`，
/// 原始 prompt 恒为后缀。多模态轮走临时会话文件、不落在主 JSONL，因此
/// 匹配允许跳过（从当前位置向后扫描首个命中的 prompt）。截断点在最后
/// 一个命中条目之后、下一个 user 条目之前。完全匹配不到时退化为按
/// 序数保留（纯文本会话等价于按轮数切）。
pub(crate) fn plan_pi_session_prefix(raw: &str, allowed_prompts: &[String]) -> PiForkPrefix {
    let lines: Vec<&str> = raw.lines().filter(|line| !line.trim().is_empty()).collect();
    let allowed: Vec<&str> = allowed_prompts
        .iter()
        .map(|prompt| prompt.trim())
        .filter(|prompt| !prompt.is_empty())
        .collect();
    if allowed.is_empty() {
        return PiForkPrefix {
            kept_lines: Vec::new(),
            matched_user_entries: 0,
        };
    }

    let mut cursor = 0usize; // allowed 中下一个待匹配的下标
    let mut matched_user_entries = 0usize;
    let mut last_allowed_user_line: Option<usize> = None;
    for (index, line) in lines.iter().enumerate() {
        let Some(value) = parse_line(line) else {
            continue;
        };
        if is_session_header(&value) {
            continue;
        }
        let Some(text) = user_message_text(&value) else {
            continue;
        };
        let text = text.trim_end();
        let mut hit: Option<usize> = None;
        for (candidate, prompt) in allowed.iter().enumerate().skip(cursor) {
            if text.ends_with(prompt) {
                hit = Some(candidate);
                break;
            }
        }
        if let Some(candidate) = hit {
            cursor = candidate + 1;
            matched_user_entries += 1;
            last_allowed_user_line = Some(index);
        }
    }

    let cut_line = match last_allowed_user_line {
        // 匹配成功：截断到下一个 user 条目之前。
        Some(last) => next_user_entry_line(&lines, last).unwrap_or(lines.len()),
        // 兜底：按序数保留前 allowed.len() 个 user 条目（含最后一轮的
        // 后续非 user 条目，到下一个 user 条目为止）。
        None => {
            let mut seen = 0usize;
            let mut last_kept_user_line: Option<usize> = None;
            for (index, line) in lines.iter().enumerate() {
                let Some(value) = parse_line(line) else {
                    continue;
                };
                if !is_session_header(&value) && user_message_text(&value).is_some() {
                    seen += 1;
                    if seen >= allowed.len() {
                        last_kept_user_line = Some(index);
                        break;
                    }
                }
            }
            match last_kept_user_line {
                Some(last) => next_user_entry_line(&lines, last).unwrap_or(lines.len()),
                None => 0,
            }
        }
    };

    let kept_lines = lines[..cut_line.min(lines.len())]
        .iter()
        .filter_map(|line| {
            let value = parse_line(line)?;
            if is_session_header(&value) {
                return None;
            }
            Some((*line).to_string())
        })
        .collect();
    PiForkPrefix {
        kept_lines,
        matched_user_entries,
    }
}

/// 生成 fork 后新文件的 header 行：优先沿用源 header（保留 version/cwd
/// 等字段），覆盖 id/timestamp 并写入 `parentSession` 溯源。
pub(crate) fn render_fork_header(
    source_header: Option<&Value>,
    new_session_id: &str,
    timestamp_iso: &str,
    fallback_cwd: &Path,
    parent_session_path: &Path,
) -> Result<String, String> {
    let mut header = match source_header {
        Some(value) if value.is_object() => value.clone(),
        _ => json!({ "type": "session", "version": 3 }),
    };
    let map = header
        .as_object_mut()
        .ok_or_else(|| "源会话 header 不是对象".to_string())?;
    map.insert("type".to_string(), json!("session"));
    if !map
        .get("version")
        .and_then(Value::as_i64)
        .is_some_and(|version| (1..=3).contains(&version))
    {
        map.insert("version".to_string(), json!(3));
    }
    map.insert("id".to_string(), json!(new_session_id));
    map.insert("timestamp".to_string(), json!(timestamp_iso));
    let cwd = map
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| fallback_cwd.to_string_lossy().to_string());
    map.insert("cwd".to_string(), json!(cwd));
    map.insert(
        "parentSession".to_string(),
        json!(parent_session_path.to_string_lossy().to_string()),
    );
    serde_json::to_string(&header).map_err(|error| format!("序列化 fork header 失败: {error}"))
}

/// 读取源 JSONL 的 header（首个 `type == "session"` 行）。
fn read_session_header(raw: &str) -> Option<Value> {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .find_map(parse_line)
        .filter(is_session_header)
}

// ── agent 事件流纯函数 ─────────────────────────────────────────────────

/// 把源会话事件流截断到第 `keep_prompt_events` 个 `Prompt` 事件（含），
/// 并把 `session_id` 改写为新会话。事件流里每轮顺序固定为
/// `ModeSet → Prompt → 本轮输出`，下一轮的 ModeSet 紧跟在上一轮输出
/// 之后，因此第 N 个 Prompt 之后再遇到 ModeSet 或下一个 Prompt 即停
/// ——保住的恰好是前 N 轮的全部事件。fork 后子会话对事件流做 fold
/// 得到的 execution_mode 与 fork 点一致。
pub(crate) fn rewrite_events_for_fork(
    raw: &str,
    new_session_id: &str,
    keep_prompt_events: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut prompt_seen = 0usize;
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(mut value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = value.get("kind").and_then(Value::as_str);
        if prompt_seen >= keep_prompt_events
            && (kind == Some("prompt") || kind == Some("mode_set"))
        {
            break;
        }
        if kind == Some("prompt") {
            prompt_seen += 1;
        }
        if let Some(map) = value.as_object_mut() {
            map.insert(
                "session_id".to_string(),
                json!(new_session_id.to_string()),
            );
            map.insert(
                "id".to_string(),
                json!(format!("sess_evt_{}", uuid::Uuid::new_v4().simple())),
            );
        }
        if let Ok(serialized) = serde_json::to_string(&value) {
            out.push(serialized);
        }
    }
    out
}

// ── 编排 ───────────────────────────────────────────────────────────────

fn fork_title(source_title: &str) -> String {
    let trimmed = source_title.trim();
    let base = if trimmed.is_empty() { "未命名会话" } else { trimmed };
    let title = format!("分支 · {base}");
    title.chars().take(120).collect()
}

fn copy_chat_turn_row(
    conn: &Connection,
    turn: &storage::chat_history::ChatTurn,
    new_session_id: &str,
    new_turn_id: &str,
    new_index: i32,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO chat_turns (
            id, session_id, turn_index, prompt, answer, thinking, status, created_at,
            completed_at, usage_json, response_segments_json, tool_calls_json, activity_json, speaker_agent_id
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            new_turn_id,
            new_session_id,
            new_index,
            turn.prompt,
            turn.answer,
            turn.thinking,
            turn.status,
            turn.created_at,
            turn.completed_at,
            turn.usage_json,
            turn.response_segments_json,
            turn.tool_calls_json,
            turn.activity_json,
            turn.speaker_agent_id,
        ],
    )
    .map_err(|e| format!("复制聊天轮次失败: {e}"))?;
    Ok(())
}

fn copy_multimodal_summaries(
    conn: &Connection,
    source_summary_key: &str,
    new_summary_key: &str,
    cutoff_ms: i64,
) -> Result<usize, String> {
    let rows = storage::core_memory::list_runtime_multimodal_summaries(
        conn,
        source_summary_key,
        100,
    )?;
    let mut copied = 0usize;
    for row in rows {
        if row.created_at > cutoff_ms {
            continue;
        }
        storage::core_memory::insert_runtime_multimodal_summary(
            conn,
            new_summary_key,
            &row.user_prompt,
            &row.assistant_response,
        )?;
        copied += 1;
    }
    Ok(copied)
}

fn copy_session_event_log(agent_home: &Path, source_id: &str, new_id: &str, keep_prompts: usize) -> usize {
    let source_log = crate::managed_runtime::session_log_path_for(agent_home, source_id);
    let Ok(raw) = fs::read_to_string(&source_log) else {
        return 0;
    };
    let rewritten = rewrite_events_for_fork(&raw, new_id, keep_prompts);
    if rewritten.is_empty() {
        return 0;
    }
    let target_log = crate::managed_runtime::session_log_path_for(agent_home, new_id);
    if let Some(parent) = target_log.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut content = rewritten.join("\n");
    content.push('\n');
    if fs::write(&target_log, content).is_err() {
        return 0;
    }
    rewritten.len()
}

/// 执行 fork。调用方需先确保源会话没有进行中的生成（命令层持有会话
/// 流互斥锁），并保证 `new_session_id` 全新。
pub(crate) fn fork_chat_session(
    conn: &Connection,
    params: &ChatForkParams,
    paths: &ChatForkRuntimePaths,
) -> Result<ChatForkOutcome, String> {
    let source_id = params.source_session_id.trim();
    let new_id = params.new_session_id.trim();
    if source_id.is_empty() || new_id.is_empty() {
        return Err("缺少会话 id".to_string());
    }
    if source_id == new_id {
        return Err("新分支会话 id 不能与源会话相同".to_string());
    }
    if params.workspace_mode != WORKSPACE_MODE_SHARE && params.workspace_mode != WORKSPACE_MODE_COPY
    {
        return Err(format!("未知的工作区模式: {}", params.workspace_mode));
    }

    let source = storage::chat_history::get_chat_session(conn, source_id)?
        .ok_or_else(|| "源会话不存在".to_string())?;
    if storage::chat_history::get_chat_session(conn, new_id)?.is_some() {
        return Err("目标会话 id 已存在".to_string());
    }
    let turns = storage::chat_history::list_chat_turns(conn, source_id)?;
    let fork_position = turns
        .iter()
        .position(|turn| turn.id == params.fork_turn_id.trim())
        .ok_or_else(|| "fork 点轮次不存在".to_string())?;
    let kept_turns = &turns[..=fork_position];
    let allowed_prompts: Vec<String> = kept_turns
        .iter()
        .map(|turn| turn.prompt.trim().to_string())
        .collect();
    let fork_turn = &kept_turns[fork_position];
    let cutoff_ms = fork_turn.completed_at.unwrap_or(fork_turn.created_at);

    // 1) 新 SQLite 会话 + 轮次拷贝（保留原始 created_at/completed_at）。
    storage::chat_history::create_chat_session(
        conn,
        &storage::chat_history::CreateChatSessionInput {
            id: new_id.to_string(),
            title: fork_title(&source.title),
            status: source.status.clone(),
            agent_id: source.agent_id.clone(),
            agent_snapshot_json: source.agent_snapshot_json.clone(),
            bot_target_json: source.bot_target_json.clone(),
            session_llm_provider_id: source.session_llm_provider_id.clone(),
            session_llm_model: source.session_llm_model.clone(),
            workspace_id: source.workspace_id.clone(),
        },
    )?;
    for (index, turn) in kept_turns.iter().enumerate() {
        copy_chat_turn_row(
            conn,
            turn,
            new_id,
            &format!("turn_{}", uuid::Uuid::new_v4().simple()),
            index as i32,
        )?;
    }

    // 2) 工作区：share 共用源目录；copy 建默认目录后整体复制源当前目录。
    let source_dirs =
        storage::session_workspace::get_session_workspace_dirs(conn, source_id)?;
    match params.workspace_mode.as_str() {
        WORKSPACE_MODE_SHARE => {
            let (topic, current) = match source_dirs.as_ref() {
                Some(dirs) => {
                    let topic = dirs
                        .topic_workspace_dir
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty());
                    let current = dirs
                        .current_workspace_dir
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty());
                    match (topic, current) {
                        (Some(topic), current) => (topic.to_string(), current.unwrap_or(topic).to_string()),
                        (None, _) => (String::new(), String::new()),
                    }
                }
                None => (String::new(), String::new()),
            };
            if topic.is_empty() {
                // 老会话从未初始化过工作区：退化为新会话默认目录。
                crate::session_workspace::create_default_for_new_session(
                    conn,
                    new_id,
                    source.workspace_id.as_deref(),
                )?;
            } else {
                storage::session_workspace::set_session_workspace_dirs(
                    conn, new_id, &topic, &current,
                )?;
            }
        }
        _ => {
            let state = crate::session_workspace::create_default_for_new_session(
                conn,
                new_id,
                source.workspace_id.as_deref(),
            )?;
            if let Some(dirs) = source_dirs.as_ref() {
                let source_current = dirs
                    .current_workspace_dir
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from);
                if let Some(source_current) = source_current {
                    if source_current.is_dir() && source_current != Path::new(&state.topic_workspace_dir)
                    {
                        crate::agent_workspace::copy_dir_all(&source_current, Path::new(&state.topic_workspace_dir))
                            .map_err(|error| {
                                format!(
                                    "复制会话工作区失败 {} -> {}: {error}",
                                    source_current.display(),
                                    state.topic_workspace_dir
                                )
                            })?;
                    }
                }
            }
        }
    }

    // 3) PI 会话 JSONL：新 header + 截断前缀。
    let raw_source = fs::read_to_string(&paths.source_jsonl).unwrap_or_default();
    let prefix = plan_pi_session_prefix(&raw_source, &allowed_prompts);
    log::info!(
        "session fork {} -> {}: 保留 {} 轮 / {} 条 PI 条目（匹配 user 消息 {} 条）",
        source_id,
        new_id,
        kept_turns.len(),
        prefix.kept_lines.len(),
        prefix.matched_user_entries
    );
    let timestamp_iso = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let header_line = render_fork_header(
        read_session_header(&raw_source).as_ref(),
        new_id,
        &timestamp_iso,
        &paths.fallback_cwd,
        &paths.source_jsonl,
    )?;
    let mut target_content = String::new();
    target_content.push_str(&header_line);
    target_content.push('\n');
    for line in &prefix.kept_lines {
        target_content.push_str(line);
        target_content.push('\n');
    }
    crate::runtime_paths::write_private_file(&paths.target_jsonl, target_content.as_bytes())?;

    // 4) 多模态摘要按 fork 点时间截断复制（后续文本轮注入图片问答上下文）。
    let _ = copy_multimodal_summaries(
        conn,
        &crate::session_summary_key(source_id),
        &crate::session_summary_key(new_id),
        cutoff_ms,
    );

    // 5) agent 侧事件流截断复制，execution_mode fold 与 fork 点一致。
    let copied_event_lines = source
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|agent_id| {
            crate::agent_workspace::resolve_workspace_root()
                .ok()
                .map(|root| root.join("agents").join(agent_id))
        })
        .map(|agent_home| {
            copy_session_event_log(&agent_home, source_id, new_id, kept_turns.len())
        })
        .unwrap_or(0);

    Ok(ChatForkOutcome {
        new_session_id: new_id.to_string(),
        copied_turns: kept_turns.len(),
        kept_pi_entries: prefix.kept_lines.len(),
        copied_event_lines,
    })
}

/// fork 完成后组装返回给前端的新会话详情。turns 必须带上已复制的轮次，
/// 否则前端把新分支会话渲染成空白（数据在库里但界面没有）。
pub(crate) fn forked_session_detail(
    conn: &Connection,
    new_session_id: &str,
) -> Result<crate::commands_chat_workspace::ChatSessionDetail, String> {
    let session = storage::chat_history::get_chat_session(conn, new_session_id)?
        .ok_or_else(|| "刚创建的分支会话查询不到".to_string())?;
    let turns = storage::chat_history::list_chat_turns(conn, new_session_id)?;
    Ok(crate::commands_chat_workspace::ChatSessionDetail::from_session_and_turns(
        session, turns,
    ))
}

/// 由会话行与其轮次组装详情；`turns` 驱动前端消息渲染，不能缺省
/// （`From<ChatSession>` 恒为空，直接返回会把会话渲染成空白）。
/// impl 放在本模块：构造器的主要消费方是 fork 命令，且
/// commands_chat_workspace.rs 已顶到行数棘轮。
impl crate::commands_chat_workspace::ChatSessionDetail {
    pub(crate) fn from_session_and_turns(
        s: storage::chat_history::ChatSession,
        turns: Vec<storage::chat_history::ChatTurn>,
    ) -> Self {
        let mut detail = Self::from(s);
        detail.turns = turns;
        detail
    }
}

/// Tauri 命令：从源会话的某一轮（含）分叉出新会话。持有源会话生成
/// 互斥锁，避免 fork 读到半写状态；返回新会话详情（含复制的轮次）。
#[tauri::command]
pub(crate) fn chat_fork_session(
    app: tauri::AppHandle,
    source_session_id: String,
    fork_turn_id: String,
    new_session_id: String,
    workspace_mode: String,
) -> Result<crate::commands_chat_workspace::ChatSessionDetail, String> {
    let source_id = source_session_id.trim().to_string();
    let stream_mutex = crate::desktop_session_stream_mutex(&source_id);
    let _stream_guard = stream_mutex
        .try_lock()
        .map_err(|_| "该会话正在生成，请等待完成后再创建分支。".to_string())?;

    let conn = crate::history_app_state::storage_conn(&app)?;
    // cwd 兜底与 PI 进程一致：agent home（无 agent 时退回工作区根）。
    let fallback_cwd = {
        let agent_id = storage::chat_history::get_chat_session(&conn, &source_id)?
            .and_then(|session| session.agent_id)
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        match agent_id {
            Some(agent_id) => crate::agent_workspace::resolve_workspace_root()
                .map(|root| root.join("agents").join(agent_id))?,
            None => crate::agent_workspace::resolve_workspace_root()?,
        }
    };
    let params = ChatForkParams {
        source_session_id: source_id,
        fork_turn_id,
        new_session_id,
        workspace_mode,
    };
    let runtime_paths = ChatForkRuntimePaths {
        source_jsonl: pi_session_jsonl_path(&params.source_session_id),
        target_jsonl: pi_session_jsonl_path(&params.new_session_id),
        fallback_cwd,
    };
    let outcome = fork_chat_session(&conn, &params, &runtime_paths)?;
    log::info!("chat_fork_session 完成: {outcome:?}");
    crate::history_app_state::sync_history_v1_backup_from_structured(&conn)?;
    forked_session_detail(&conn, &params.new_session_id)
}

/// PI 主会话 JSONL 路径（与 lib.rs 的 session_file_path 同一套 key 规则）。
fn pi_session_jsonl_path(session_id: &str) -> PathBuf {
    crate::runtime_paths::session_file_path(
        crate::app_constants::PI_SESSION_FILE_PREFIX,
        &crate::session_summary_key(session_id),
    )
}

#[cfg(test)]
mod tests;
