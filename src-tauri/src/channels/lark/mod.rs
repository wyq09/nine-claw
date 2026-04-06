use crate::agent_workspace;
use crate::agents::ConversationAgentConfig;
use crate::channels::im_message_merge_window_ms;
use crate::channels::pi_bridge::{PiBridge, PiProcessOutcome, PiRunHandle};
use crate::channels::types::{BotMessage, ChannelStatus, MediaPayload, MediaType};
use crate::channels::Channel;
use crate::dev_trace::dev_trace;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const TEXT_CHUNK_SIZE: usize = 3000;
const STREAM_CHUNK_SIZE: usize = 500;
const REQUEST_TIMEOUT_SECS: u64 = 90;
const STARTUP_TIMEOUT_SECS: u64 = 20;

#[derive(Clone, Debug)]
struct WorkItem {
    session_user_id: String,
}

#[derive(Clone)]
struct UserTurnState {
    pending_texts: Vec<String>,
    receive_target: ReplyTarget,
    last_inbound_at: i64,
    queued: bool,
    running: bool,
    active_run: Option<Arc<PiRunHandle>>,
}

#[derive(Clone, Debug)]
struct InboundAttachmentPayload {
    media_type: MediaType,
    file_name: String,
    temp_path: String,
    transcript: Option<String>,
}

#[derive(Clone, Debug)]
struct InboundAttachment {
    media_type: MediaType,
    saved_path: String,
    transcript: Option<String>,
}

#[derive(Clone, Debug)]
struct InboundMessagePayload {
    prompt_text: String,
    display_text: String,
}

#[derive(Clone, Debug)]
struct ReplyTarget {
    receive_id: String,
    receive_id_type: String,
}

#[derive(Clone, Debug)]
struct ParsedMediaItem {
    media_type: MediaType,
    file_name: String,
    file_path: String,
}

pub struct LarkChannel {
    channel_id: String,
    app_id: String,
    app_secret: String,
    running: Arc<AtomicBool>,
    status: Arc<Mutex<ChannelStatus>>,
    helper_stdin: Arc<Mutex<Option<ChildStdin>>>,
    helper_child: Arc<Mutex<Option<Child>>>,
    reply_targets: Arc<Mutex<HashMap<String, ReplyTarget>>>,
    pending_requests: Arc<Mutex<HashMap<String, mpsc::Sender<Result<(), String>>>>>,
    ai_provider_id: String,
    ai_api_format: String,
    ai_base_url: String,
    ai_api_key: String,
    ai_model: String,
    agent_config: Option<ConversationAgentConfig>,
}

impl LarkChannel {
    pub fn new(channel_id: &str, app_id: &str, app_secret: &str) -> Self {
        Self {
            channel_id: channel_id.to_string(),
            app_id: app_id.to_string(),
            app_secret: app_secret.to_string(),
            running: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(ChannelStatus::Disconnected)),
            helper_stdin: Arc::new(Mutex::new(None)),
            helper_child: Arc::new(Mutex::new(None)),
            reply_targets: Arc::new(Mutex::new(HashMap::new())),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            ai_provider_id: String::new(),
            ai_api_format: "openai".to_string(),
            ai_base_url: String::new(),
            ai_api_key: String::new(),
            ai_model: String::new(),
            agent_config: None,
        }
    }

    pub fn set_ai_config(
        &mut self,
        provider_id: &str,
        api_format: &str,
        base_url: &str,
        api_key: &str,
        model: &str,
        agent_config: Option<ConversationAgentConfig>,
    ) {
        self.ai_provider_id = provider_id.to_string();
        self.ai_api_format = api_format.to_string();
        self.ai_base_url = base_url.to_string();
        self.ai_api_key = api_key.to_string();
        self.ai_model = model.to_string();
        self.agent_config = agent_config;
    }

    fn bound_agent_debug_label(&self) -> String {
        self.agent_config
            .as_ref()
            .map(|agent| format!("{} ({})", agent.name, agent.id))
            .unwrap_or_else(|| "未绑定智能体".to_string())
    }

    fn resolve_helper_node_path(
        pi_runtime: &crate::pi_runtime::PiRuntimeLocation,
    ) -> Result<PathBuf, String> {
        if cfg!(target_os = "macos") {
            if let Some(system_node) = crate::pi_runtime::resolve_command_path(&["node"]) {
                return Ok(system_node);
            }
        }

        if let Some(resource_root) = pi_runtime.resource_root.as_ref() {
            let bundled_name = if cfg!(target_os = "windows") {
                "node.exe"
            } else {
                "node"
            };
            let bundled_node = resource_root.join(bundled_name);
            if bundled_node.is_file() {
                return Ok(bundled_node);
            }
        }

        crate::pi_runtime::resolve_command_path(&["node.exe", "node"])
            .ok_or_else(|| "未找到 Node.js，无法启动飞书机器人".to_string())
    }
}

impl Channel for LarkChannel {
    fn start(&mut self, app: AppHandle) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("飞书通道已在运行".to_string());
        }
        if self.app_id.trim().is_empty() || self.app_secret.trim().is_empty() {
            return Err("飞书 App ID / App Secret 未配置".to_string());
        }

        if self.helper_child.lock().unwrap().is_some() {
            let _ = self.stop();
        }

        let helper_path = resolve_lark_helper_path(&app)?;
        let pi_runtime = crate::pi_runtime::require_pi_runtime_location(&app)?;
        let node_path = Self::resolve_helper_node_path(&pi_runtime)?;

        let mut child = Command::new(node_path)
            .arg(&helper_path)
            .arg("--app-id")
            .arg(&self.app_id)
            .arg("--app-secret")
            .arg(&self.app_secret)
            .arg("--channel-id")
            .arg(&self.channel_id)
            .arg("--agent-label")
            .arg(self.bound_agent_debug_label())
            .current_dir(
                helper_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from(".")),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("启动飞书机器人辅助进程失败: {error}"))?;

        let helper_stdin = child
            .stdin
            .take()
            .ok_or_else(|| "飞书机器人辅助进程未暴露 stdin".to_string())?;
        let helper_stdout = child
            .stdout
            .take()
            .ok_or_else(|| "飞书机器人辅助进程未暴露 stdout".to_string())?;
        let helper_stderr = child
            .stderr
            .take()
            .ok_or_else(|| "飞书机器人辅助进程未暴露 stderr".to_string())?;

        *self.helper_stdin.lock().unwrap() = Some(helper_stdin);
        *self.helper_child.lock().unwrap() = Some(child);
        *self.status.lock().unwrap() = ChannelStatus::Connecting;
        self.running.store(true, Ordering::SeqCst);

        // `start()` should only succeed after the helper confirms the long connection is ready.
        let (startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();
        let startup_signal = Arc::new(Mutex::new(Some(startup_tx)));

        emit_bot_status(
            &app,
            &self.channel_id,
            "",
            "processing",
            &format!(
                "飞书机器人启动中，当前绑定智能体: {}",
                self.bound_agent_debug_label()
            ),
        );

        let (work_tx, work_rx) = mpsc::channel::<WorkItem>();
        let user_states: Arc<Mutex<HashMap<String, UserTurnState>>> =
            Arc::new(Mutex::new(HashMap::new()));

        {
            let running = self.running.clone();
            let app_handle = app.clone();
            let channel_id = self.channel_id.clone();
            let status = self.status.clone();
            let reply_targets = self.reply_targets.clone();
            let pending_requests = self.pending_requests.clone();
            let agent_config = self.agent_config.clone();
            let user_states = user_states.clone();
            let startup_signal = startup_signal.clone();

            thread::spawn(move || {
                let reader = BufReader::new(helper_stdout);
                for line_result in reader.lines() {
                    let line = match line_result {
                        Ok(line) => line,
                        Err(error) => {
                            if running.load(Ordering::SeqCst) {
                                emit_bot_status(
                                    &app_handle,
                                    &channel_id,
                                    "",
                                    "error",
                                    &format!("读取飞书机器人输出失败: {error}"),
                                );
                                *status.lock().unwrap() =
                                    ChannelStatus::Error(format!("读取辅助进程失败: {error}"));
                                notify_startup_result(
                                    &startup_signal,
                                    Err(format!("读取飞书机器人输出失败: {error}")),
                                );
                            }
                            break;
                        }
                    };

                    if line.trim().is_empty() {
                        continue;
                    }

                    let value: serde_json::Value = match serde_json::from_str(&line) {
                        Ok(value) => value,
                        Err(error) => {
                            emit_bot_status(
                                &app_handle,
                                &channel_id,
                                "",
                                "warn",
                                &format!("飞书辅助进程输出了无法解析的数据: {error}"),
                            );
                            continue;
                        }
                    };

                    match value.get("type").and_then(|item| item.as_str()) {
                        Some("status") => {
                            let level = value
                                .get("level")
                                .and_then(|item| item.as_str())
                                .unwrap_or("done");
                            let message = value
                                .get("message")
                                .and_then(|item| item.as_str())
                                .unwrap_or("飞书机器人状态更新");
                            match level {
                                "error" => {
                                    *status.lock().unwrap() =
                                        ChannelStatus::Error(message.to_string());
                                    notify_startup_result(
                                        &startup_signal,
                                        Err(message.to_string()),
                                    );
                                }
                                "processing" => {
                                    *status.lock().unwrap() = ChannelStatus::Connecting;
                                }
                                "done" => {
                                    *status.lock().unwrap() = ChannelStatus::Connected;
                                    notify_startup_result(&startup_signal, Ok(()));
                                }
                                _ => {
                                    if !matches!(*status.lock().unwrap(), ChannelStatus::Connected)
                                    {
                                        *status.lock().unwrap() = ChannelStatus::Connecting;
                                    }
                                }
                            }
                            emit_bot_status(&app_handle, &channel_id, "", level, message);
                        }
                        Some("response") => {
                            let request_id = value
                                .get("requestId")
                                .and_then(|item| item.as_str())
                                .unwrap_or_default()
                                .to_string();
                            if request_id.is_empty() {
                                continue;
                            }
                            let ok = value
                                .get("ok")
                                .and_then(|item| item.as_bool())
                                .unwrap_or(false);
                            let error = value
                                .get("error")
                                .and_then(|item| item.as_str())
                                .map(str::to_string)
                                .unwrap_or_else(|| "飞书机器人请求失败".to_string());

                            if let Some(sender) =
                                pending_requests.lock().unwrap().remove(request_id.as_str())
                            {
                                let _ = sender.send(if ok { Ok(()) } else { Err(error) });
                            }
                        }
                        Some("message") => {
                            let session_user_id = value
                                .get("sessionUserId")
                                .and_then(|item| item.as_str())
                                .unwrap_or_default()
                                .trim()
                                .to_string();
                            let text = value
                                .get("text")
                                .and_then(|item| item.as_str())
                                .unwrap_or_default()
                                .trim()
                                .to_string();
                            let receive_id = value
                                .get("receiveId")
                                .and_then(|item| item.as_str())
                                .unwrap_or_default()
                                .trim()
                                .to_string();
                            let receive_id_type = value
                                .get("receiveIdType")
                                .and_then(|item| item.as_str())
                                .unwrap_or("chat_id")
                                .trim()
                                .to_string();

                            let attachments = value
                                .get("attachments")
                                .and_then(parse_inbound_attachment_payloads)
                                .unwrap_or_default();

                            if session_user_id.is_empty()
                                || receive_id.is_empty()
                                || (text.is_empty() && attachments.is_empty())
                            {
                                continue;
                            }

                            let inbound = match materialize_inbound_message(
                                agent_config.as_ref().map(|config| config.id.as_str()),
                                &session_user_id,
                                &text,
                                attachments,
                            ) {
                                Ok(Some(payload)) => payload,
                                Ok(None) => continue,
                                Err(error) => {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &session_user_id,
                                        "warn",
                                        &format!("解析飞书入站附件失败: {error}"),
                                    );
                                    continue;
                                }
                            };

                            reply_targets.lock().unwrap().insert(
                                session_user_id.clone(),
                                ReplyTarget {
                                    receive_id: receive_id.clone(),
                                    receive_id_type: receive_id_type.clone(),
                                },
                            );

                            emit_bot_message(
                                &app_handle,
                                &channel_id,
                                &session_user_id,
                                "inbound",
                                &inbound.display_text,
                                agent_config.as_ref(),
                            );

                            let pending_abort = {
                                let mut guard = match user_states.lock() {
                                    Ok(guard) => guard,
                                    Err(_) => {
                                        running.store(false, Ordering::SeqCst);
                                        break;
                                    }
                                };

                                let state =
                                    guard.entry(session_user_id.clone()).or_insert_with(|| {
                                        UserTurnState {
                                            pending_texts: Vec::new(),
                                            receive_target: ReplyTarget {
                                                receive_id: receive_id.clone(),
                                                receive_id_type: receive_id_type.clone(),
                                            },
                                            last_inbound_at: now_timestamp_ms(),
                                            queued: false,
                                            running: false,
                                            active_run: None,
                                        }
                                    });

                                state.pending_texts.push(inbound.prompt_text);
                                state.receive_target = ReplyTarget {
                                    receive_id: receive_id.clone(),
                                    receive_id_type: receive_id_type.clone(),
                                };
                                state.last_inbound_at = now_timestamp_ms();

                                let should_queue = !state.running && !state.queued;
                                if should_queue {
                                    state.queued = true;
                                }

                                let active_run = if state.running {
                                    state.active_run.clone()
                                } else {
                                    None
                                };

                                (should_queue, active_run)
                            };

                            if let Some(run_handle) = pending_abort.1 {
                                let _ = run_handle.abort();
                            }

                            if pending_abort.0
                                && work_tx.send(WorkItem { session_user_id }).is_err()
                            {
                                running.store(false, Ordering::SeqCst);
                                break;
                            }
                        }
                        _ => {}
                    }
                }

                if running.load(Ordering::SeqCst) {
                    running.store(false, Ordering::SeqCst);
                    drain_pending_requests(&pending_requests, "飞书机器人辅助进程已退出");
                    *status.lock().unwrap() =
                        ChannelStatus::Error("飞书机器人辅助进程已退出".to_string());
                    notify_startup_result(
                        &startup_signal,
                        Err("飞书机器人辅助进程已退出".to_string()),
                    );
                    emit_bot_status(
                        &app_handle,
                        &channel_id,
                        "",
                        "error",
                        "飞书机器人辅助进程已退出",
                    );
                }
            });
        }

        {
            let running = self.running.clone();
            let app_handle = app.clone();
            let channel_id = self.channel_id.clone();
            let status = self.status.clone();
            let startup_signal = startup_signal.clone();

            thread::spawn(move || {
                let reader = BufReader::new(helper_stderr);
                for line_result in reader.lines() {
                    let line = match line_result {
                        Ok(line) => line,
                        Err(_) => break,
                    };
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    emit_bot_status(
                        &app_handle,
                        &channel_id,
                        "",
                        "warn",
                        &format!("飞书辅助进程: {trimmed}"),
                    );
                }

                if running.load(Ordering::SeqCst)
                    && matches!(*status.lock().unwrap(), ChannelStatus::Connecting)
                {
                    notify_startup_result(
                        &startup_signal,
                        Err("飞书机器人启动期间辅助进程提前退出".to_string()),
                    );
                    emit_bot_status(
                        &app_handle,
                        &channel_id,
                        "",
                        "warn",
                        "飞书机器人仍在连接中，请确认开放平台已开启长连接订阅和机器人权限",
                    );
                }
            });
        }

        {
            let running = self.running.clone();
            let app_handle = app.clone();
            let channel_id = self.channel_id.clone();
            let helper_stdin = self.helper_stdin.clone();
            let pending_requests = self.pending_requests.clone();
            let ai_pid = self.ai_provider_id.clone();
            let ai_fmt = self.ai_api_format.clone();
            let ai_base = self.ai_base_url.clone();
            let ai_key = self.ai_api_key.clone();
            let ai_mdl = self.ai_model.clone();
            let agent_config = self.agent_config.clone();
            let user_states = user_states.clone();
            let pi_runtime = pi_runtime.clone();

            thread::spawn(move || {
                let bridge = PiBridge::new(
                    pi_runtime,
                    &ai_pid,
                    &ai_fmt,
                    &ai_base,
                    &ai_key,
                    &ai_mdl,
                    agent_config.clone(),
                );

                while running.load(Ordering::SeqCst) {
                    let item = match work_rx.recv_timeout(Duration::from_secs(1)) {
                        Ok(item) => item,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    };

                    let session_user_id = item.session_user_id;

                    while running.load(Ordering::SeqCst) {
                        let wait_more_ms = {
                            let guard = match user_states.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            let Some(state) = guard.get(&session_user_id) else {
                                break;
                            };
                            if state.pending_texts.is_empty() {
                                None
                            } else {
                                let elapsed = now_timestamp_ms() - state.last_inbound_at;
                                let merge_ms = im_message_merge_window_ms() as i64;
                                if elapsed >= merge_ms {
                                    None
                                } else {
                                    Some((merge_ms - elapsed) as u64)
                                }
                            }
                        };

                        if let Some(wait_ms) = wait_more_ms {
                            thread::sleep(Duration::from_millis(wait_ms));
                            continue;
                        }

                        let (prompt_text, receive_target) = {
                            let mut guard = match user_states.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            let Some(state) = guard.get_mut(&session_user_id) else {
                                break;
                            };
                            if state.pending_texts.is_empty() {
                                state.queued = false;
                                state.running = false;
                                state.active_run = None;
                                break;
                            }

                            state.queued = false;
                            state.running = true;
                            state.active_run = None;

                            (
                                merge_pending_user_messages(&std::mem::take(
                                    &mut state.pending_texts,
                                )),
                                state.receive_target.clone(),
                            )
                        };

                        emit_bot_status(
                            &app_handle,
                            &channel_id,
                            &session_user_id,
                            "processing",
                            &format!(
                                "正在按智能体 {} 处理: {}",
                                agent_config
                                    .as_ref()
                                    .map(|agent| format!("{} ({})", agent.name, agent.id))
                                    .unwrap_or_else(|| "未绑定智能体".to_string()),
                                truncate_chars(&prompt_text, 30)
                            ),
                        );
                        dev_trace(
                            "lark",
                            format!(
                                "开始处理: user={} prompt_chars={}",
                                session_user_id,
                                prompt_text.chars().count()
                            ),
                        );

                        let user_id_for_chunk = session_user_id.clone();
                        let user_id_for_state = session_user_id.clone();
                        let app_for_cb = app_handle.clone();
                        let state_for_run = user_states.clone();
                        let result = bridge.process_message_interruptible(
                            &channel_id,
                            &session_user_id,
                            &prompt_text,
                            STREAM_CHUNK_SIZE,
                            |chunk: &str| {
                                emit_bot_message(
                                    &app_for_cb,
                                    &channel_id,
                                    &user_id_for_chunk,
                                    "outbound_chunk",
                                    chunk,
                                    agent_config.as_ref(),
                                );
                            },
                            move |run_handle| {
                                if let Ok(mut guard) = state_for_run.lock() {
                                    if let Some(state) = guard.get_mut(&user_id_for_state) {
                                        state.active_run = Some(run_handle);
                                    }
                                }
                            },
                        );

                        let has_pending_followup = {
                            let mut guard = match user_states.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            let state = match guard.get_mut(&session_user_id) {
                                Some(state) => state,
                                None => break,
                            };
                            state.running = false;
                            state.active_run = None;
                            !state.pending_texts.is_empty()
                        };

                        match result {
                            Ok(PiProcessOutcome::Completed(full_text)) => {
                                dev_trace(
                                    "lark",
                                    format!(
                                        "处理完成: user={} chars={}",
                                        session_user_id,
                                        full_text.chars().count()
                                    ),
                                );
                                if has_pending_followup {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &session_user_id,
                                        "processing",
                                        "检测到用户追加消息，正在合并后重新处理",
                                    );
                                    continue;
                                }

                                let (text_reply, media_items) = split_text_and_media(&full_text);
                                if text_reply.is_empty() && media_items.is_empty() {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &session_user_id,
                                        "warn",
                                        "模型返回了空回复",
                                    );
                                    emit_bot_message(
                                        &app_handle,
                                        &channel_id,
                                        &session_user_id,
                                        "outbound_done",
                                        "",
                                        agent_config.as_ref(),
                                    );
                                    cleanup_idle_user_state(&user_states, &session_user_id);
                                    break;
                                }

                                if let Some(agent_id) =
                                    agent_config.as_ref().map(|config| config.id.as_str())
                                {
                                    let _ = agent_workspace::append_agent_memory_entry(
                                        agent_id,
                                        &session_user_id,
                                        &prompt_text,
                                        if text_reply.is_empty() {
                                            &full_text
                                        } else {
                                            &text_reply
                                        },
                                    );
                                }

                                if !text_reply.is_empty() {
                                    if let Err(error) = send_text_chunks(
                                        &helper_stdin,
                                        &pending_requests,
                                        &receive_target,
                                        &text_reply,
                                    ) {
                                        emit_bot_status(
                                            &app_handle,
                                            &channel_id,
                                            &session_user_id,
                                            "error",
                                            &format!("发送飞书文本回复失败: {error}"),
                                        );
                                    }
                                }

                                for media in media_items {
                                    if let Err(error) = send_media_item(
                                        &helper_stdin,
                                        &pending_requests,
                                        &receive_target,
                                        &media,
                                    ) {
                                        emit_bot_status(
                                            &app_handle,
                                            &channel_id,
                                            &session_user_id,
                                            "error",
                                            &format!("发送飞书媒体失败: {error}"),
                                        );
                                    }
                                }

                                emit_bot_status(
                                    &app_handle,
                                    &channel_id,
                                    &session_user_id,
                                    "done",
                                    &format!("回复 {} 字符完成", full_text.len()),
                                );
                                emit_bot_message(
                                    &app_handle,
                                    &channel_id,
                                    &session_user_id,
                                    "outbound_done",
                                    &full_text,
                                    agent_config.as_ref(),
                                );
                                cleanup_idle_user_state(&user_states, &session_user_id);
                                break;
                            }
                            Ok(PiProcessOutcome::Aborted) => {
                                if has_pending_followup {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &session_user_id,
                                        "processing",
                                        "收到用户新消息，正在中断上一轮并重新处理",
                                    );
                                    continue;
                                }
                                cleanup_idle_user_state(&user_states, &session_user_id);
                                break;
                            }
                            Err(error) => {
                                dev_trace(
                                    "lark",
                                    format!("处理失败: user={} error={}", session_user_id, error),
                                );
                                if has_pending_followup {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &session_user_id,
                                        "warn",
                                        &format!("上一轮处理失败，已切换到用户最新消息: {error}"),
                                    );
                                    continue;
                                }

                                emit_bot_status(
                                    &app_handle,
                                    &channel_id,
                                    &session_user_id,
                                    "error",
                                    &format!("飞书机器人处理失败: {error}"),
                                );
                                let error_msg = format!("[处理失败: {error}]");
                                let _ = send_text_chunks(
                                    &helper_stdin,
                                    &pending_requests,
                                    &receive_target,
                                    &error_msg,
                                );
                                emit_bot_message(
                                    &app_handle,
                                    &channel_id,
                                    &session_user_id,
                                    "error",
                                    &error_msg,
                                    agent_config.as_ref(),
                                );
                                cleanup_idle_user_state(&user_states, &session_user_id);
                                break;
                            }
                        }
                    }
                }
            });
        }

        match startup_rx.recv_timeout(Duration::from_secs(STARTUP_TIMEOUT_SECS)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                let _ = self.stop();
                Err(error)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let timeout_message = format!(
                    "飞书长连接启动超时（{} 秒），请确认开放平台已开启长连接订阅、事件订阅和机器人权限",
                    STARTUP_TIMEOUT_SECS
                );
                emit_bot_status(&app, &self.channel_id, "", "error", &timeout_message);
                let _ = self.stop();
                Err(timeout_message)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let error = "飞书机器人启动握手被意外中断".to_string();
                let _ = self.stop();
                Err(error)
            }
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        self.running.store(false, Ordering::SeqCst);

        let _ = send_helper_command(
            &self.helper_stdin,
            &json!({
                "type": "stop",
            }),
        );

        if let Some(mut child) = self.helper_child.lock().unwrap().take() {
            if let Ok(None) = child.try_wait() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }

        *self.helper_stdin.lock().unwrap() = None;
        self.reply_targets.lock().unwrap().clear();
        drain_pending_requests(&self.pending_requests, "飞书机器人已停止");
        *self.status.lock().unwrap() = ChannelStatus::Disconnected;
        Ok(())
    }

    fn send_message(&self, user_id: &str, content: &str) -> Result<(), String> {
        let target = resolve_send_target(&self.reply_targets, user_id);
        send_text_chunks(&self.helper_stdin, &self.pending_requests, &target, content)
    }

    fn send_media(&self, user_id: &str, media: &MediaPayload) -> Result<(), String> {
        let target = resolve_send_target(&self.reply_targets, user_id);
        let temp_path = write_temp_media_file(media)?;
        let result = send_media_item(
            &self.helper_stdin,
            &self.pending_requests,
            &target,
            &ParsedMediaItem {
                media_type: media.media_type.clone(),
                file_name: media.file_name.clone(),
                file_path: temp_path.to_string_lossy().to_string(),
            },
        );
        let _ = fs::remove_file(&temp_path);
        result
    }

    fn status(&self) -> ChannelStatus {
        self.status.lock().unwrap().clone()
    }
}

fn resolve_lark_helper_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("bot-helpers").join("lark-helper.cjs"));
        candidates.push(
            resource_dir
                .join("resources")
                .join("bot-helpers")
                .join("lark-helper.cjs"),
        );
    }

    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("bot-helpers")
            .join("lark-helper.cjs"),
    );

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("未找到飞书机器人 helper，请先执行 npm run build:lark-helper".to_string())
}

fn notify_startup_result(
    startup_signal: &Arc<Mutex<Option<mpsc::Sender<Result<(), String>>>>>,
    result: Result<(), String>,
) {
    if let Ok(mut guard) = startup_signal.lock() {
        if let Some(sender) = guard.take() {
            let _ = sender.send(result);
        }
    }
}

fn merge_pending_user_messages(messages: &[String]) -> String {
    if messages.len() <= 1 {
        return messages.first().cloned().unwrap_or_default();
    }

    messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            if index == 0 {
                message.clone()
            } else {
                format!("【用户追加消息 {}】\n{}", index, message)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn cleanup_idle_user_state(
    user_states: &Arc<Mutex<HashMap<String, UserTurnState>>>,
    user_id: &str,
) {
    let mut guard = match user_states.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let should_remove = guard
        .get(user_id)
        .map(|state| !state.running && !state.queued && state.pending_texts.is_empty())
        .unwrap_or(false);

    if should_remove {
        guard.remove(user_id);
    }
}

fn send_text_chunks(
    helper_stdin: &Arc<Mutex<Option<ChildStdin>>>,
    pending_requests: &Arc<Mutex<HashMap<String, mpsc::Sender<Result<(), String>>>>>,
    target: &ReplyTarget,
    content: &str,
) -> Result<(), String> {
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    let mut start = 0;
    while start < chars.len() {
        let end_idx = (start + TEXT_CHUNK_SIZE).min(chars.len());
        let byte_start = chars[start].0;
        let byte_end = if end_idx < chars.len() {
            chars[end_idx].0
        } else {
            content.len()
        };
        let chunk = &content[byte_start..byte_end];
        send_helper_request(
            helper_stdin,
            pending_requests,
            json!({
                "type": "send_text",
                "receiveId": target.receive_id,
                "receiveIdType": target.receive_id_type,
                "content": chunk,
            }),
        )?;
        start = end_idx;
    }
    Ok(())
}

fn send_media_item(
    helper_stdin: &Arc<Mutex<Option<ChildStdin>>>,
    pending_requests: &Arc<Mutex<HashMap<String, mpsc::Sender<Result<(), String>>>>>,
    target: &ReplyTarget,
    media: &ParsedMediaItem,
) -> Result<(), String> {
    send_helper_request(
        helper_stdin,
        pending_requests,
        json!({
            "type": "send_media",
            "receiveId": target.receive_id,
            "receiveIdType": target.receive_id_type,
            "mediaType": match media.media_type {
                MediaType::Image => "image",
                MediaType::File => "file",
                MediaType::Video => "video",
                MediaType::Audio => "audio",
            },
            "filePath": media.file_path,
            "fileName": media.file_name,
        }),
    )
}

fn send_helper_request(
    helper_stdin: &Arc<Mutex<Option<ChildStdin>>>,
    pending_requests: &Arc<Mutex<HashMap<String, mpsc::Sender<Result<(), String>>>>>,
    mut payload: serde_json::Value,
) -> Result<(), String> {
    let request_id = Uuid::new_v4().to_string();
    if let Some(object) = payload.as_object_mut() {
        object.insert("requestId".to_string(), json!(request_id));
    }

    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    pending_requests
        .lock()
        .map_err(|error| format!("无法锁定飞书请求队列: {error}"))?
        .insert(request_id.clone(), tx);

    let send_result = send_helper_command(helper_stdin, &payload);
    if let Err(error) = send_result {
        pending_requests
            .lock()
            .ok()
            .and_then(|mut guard| guard.remove(&request_id));
        return Err(error);
    }

    match rx.recv_timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS)) {
        Ok(result) => result,
        Err(_) => {
            pending_requests
                .lock()
                .ok()
                .and_then(|mut guard| guard.remove(&request_id));
            Err("等待飞书机器人响应超时".to_string())
        }
    }
}

fn send_helper_command(
    helper_stdin: &Arc<Mutex<Option<ChildStdin>>>,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let mut guard = helper_stdin
        .lock()
        .map_err(|error| format!("无法锁定飞书机器人 stdin: {error}"))?;
    let stdin = guard
        .as_mut()
        .ok_or_else(|| "飞书机器人未运行".to_string())?;
    serde_json::to_writer(&mut *stdin, payload)
        .map_err(|error| format!("写入飞书机器人命令失败: {error}"))?;
    stdin
        .write_all(b"\n")
        .map_err(|error| format!("发送飞书机器人命令失败: {error}"))?;
    stdin
        .flush()
        .map_err(|error| format!("刷新飞书机器人命令失败: {error}"))
}

fn drain_pending_requests(
    pending_requests: &Arc<Mutex<HashMap<String, mpsc::Sender<Result<(), String>>>>>,
    message: &str,
) {
    let mut guard = match pending_requests.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    for (_, sender) in guard.drain() {
        let _ = sender.send(Err(message.to_string()));
    }
}

fn resolve_send_target(
    reply_targets: &Arc<Mutex<HashMap<String, ReplyTarget>>>,
    user_id: &str,
) -> ReplyTarget {
    if let Ok(guard) = reply_targets.lock() {
        if let Some(target) = guard.get(user_id) {
            return target.clone();
        }
    }
    parse_explicit_receive_target(user_id)
}

fn parse_explicit_receive_target(user_id: &str) -> ReplyTarget {
    if let Some((prefix, value)) = user_id.split_once(':') {
        if matches!(
            prefix,
            "chat_id" | "open_id" | "user_id" | "union_id" | "email"
        ) && !value.trim().is_empty()
        {
            return ReplyTarget {
                receive_id: value.trim().to_string(),
                receive_id_type: prefix.trim().to_string(),
            };
        }
    }

    ReplyTarget {
        receive_id: user_id.trim().to_string(),
        receive_id_type: "chat_id".to_string(),
    }
}

fn write_temp_media_file(media: &MediaPayload) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("nineclaw-lark-media");
    fs::create_dir_all(&dir).map_err(|error| format!("创建飞书媒体临时目录失败: {error}"))?;

    let extension = Path::new(&media.file_name)
        .extension()
        .and_then(|item| item.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();

    let temp_path = dir.join(format!("{}{}", Uuid::new_v4(), extension));
    fs::write(&temp_path, &media.data)
        .map_err(|error| format!("写入飞书媒体临时文件失败: {error}"))?;
    Ok(temp_path)
}

fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

fn now_timestamp_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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

fn is_audio_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp3" | "wav" | "ogg" | "opus" | "m4a" | "aac" | "amr" | "silk")
    )
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
        Some("audio") | Some("voice") => MediaType::Audio,
        Some("file") => MediaType::File,
        _ => {
            if is_image_path(&path) {
                MediaType::Image
            } else if is_video_path(&path) {
                MediaType::Video
            } else if is_audio_path(&path) {
                MediaType::Audio
            } else {
                MediaType::File
            }
        }
    };

    let file_name = Path::new(&path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string());

    Some(ParsedMediaItem {
        media_type,
        file_name,
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
    } else if is_audio_path(path) {
        MediaType::Audio
    } else {
        MediaType::File
    };

    let file_name = Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string());

    Some(ParsedMediaItem {
        media_type,
        file_name,
        file_path: path.to_string(),
    })
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

fn inbound_media_label(media_type: &MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "图片",
        MediaType::Video => "视频",
        MediaType::Audio => "语音",
        MediaType::File => "文件",
    }
}

fn attachment_display_line(attachment: &InboundAttachment) -> String {
    let mut line = if attachment.saved_path.is_empty() {
        format!("[收到{}]", inbound_media_label(&attachment.media_type))
    } else {
        format!(
            "[收到{}] {}",
            inbound_media_label(&attachment.media_type),
            attachment.saved_path
        )
    };
    if let Some(transcript) = attachment.transcript.as_deref() {
        if !transcript.trim().is_empty() {
            line.push_str(" | 转写: ");
            line.push_str(transcript.trim());
        }
    }
    line
}

fn build_inbound_prompt(text: &str, attachments: &[InboundAttachment]) -> InboundMessagePayload {
    let normalized_text = text.trim();
    let mut display_lines = Vec::new();
    if !normalized_text.is_empty() {
        display_lines.push(normalized_text.to_string());
    }
    for attachment in attachments {
        display_lines.push(attachment_display_line(attachment));
    }

    let mut prompt_parts = Vec::new();
    if !normalized_text.is_empty() {
        prompt_parts.push(normalized_text.to_string());
    }
    if !attachments.is_empty() {
        let mut section = String::from("用户还发送了以下附件，请按当前智能体能力处理：");
        for attachment in attachments {
            section.push_str("\n- ");
            section.push_str(inbound_media_label(&attachment.media_type));
            if attachment.saved_path.is_empty() {
                section.push_str(" 已接收，但未落盘路径");
            } else {
                section.push_str(" 文件路径: ");
                section.push_str(&attachment.saved_path);
            }
            if let Some(transcript) = attachment.transcript.as_deref() {
                if !transcript.trim().is_empty() {
                    section.push_str("\n  语音转写: ");
                    section.push_str(transcript.trim());
                }
            }
        }
        prompt_parts.push(section);
    }

    InboundMessagePayload {
        prompt_text: prompt_parts.join("\n\n").trim().to_string(),
        display_text: display_lines.join("\n").trim().to_string(),
    }
}

fn parse_inbound_attachment_payloads(
    value: &serde_json::Value,
) -> Option<Vec<InboundAttachmentPayload>> {
    let items = value.as_array()?;
    let mut attachments = Vec::new();
    for item in items {
        let media_type = match item.get("mediaType").and_then(|value| value.as_str()) {
            Some("image") => MediaType::Image,
            Some("video") => MediaType::Video,
            Some("audio") | Some("voice") => MediaType::Audio,
            Some("file") => MediaType::File,
            _ => continue,
        };
        let temp_path = item
            .get("filePath")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let file_name = item
            .get("fileName")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let transcript = item
            .get("transcript")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if temp_path.is_empty() && transcript.is_none() {
            continue;
        }
        attachments.push(InboundAttachmentPayload {
            media_type,
            file_name,
            temp_path,
            transcript,
        });
    }
    Some(attachments)
}

fn materialize_inbound_message(
    agent_id: Option<&str>,
    user_id: &str,
    text: &str,
    attachments: Vec<InboundAttachmentPayload>,
) -> Result<Option<InboundMessagePayload>, String> {
    let mut persisted = Vec::new();

    if let Some(agent_id) = agent_id {
        for attachment in attachments {
            let saved_path = if attachment.temp_path.trim().is_empty() {
                String::new()
            } else {
                let bytes = fs::read(&attachment.temp_path)
                    .map_err(|error| format!("读取飞书临时附件失败: {error}"))?;
                let file_name = if attachment.file_name.trim().is_empty() {
                    Path::new(&attachment.temp_path)
                        .file_name()
                        .map(|value| value.to_string_lossy().to_string())
                        .unwrap_or_else(|| "attachment.bin".to_string())
                } else {
                    attachment.file_name.clone()
                };
                let path = agent_workspace::persist_agent_inbound_artifact(
                    agent_id, user_id, &file_name, &bytes,
                )?;
                let _ = fs::remove_file(&attachment.temp_path);
                path.to_string_lossy().to_string()
            };
            persisted.push(InboundAttachment {
                media_type: attachment.media_type,
                saved_path,
                transcript: attachment.transcript,
            });
        }
    }

    let payload = build_inbound_prompt(text, &persisted);
    if payload.prompt_text.is_empty() {
        return Ok(None);
    }
    Ok(Some(payload))
}

fn emit_bot_status(app: &AppHandle, channel_id: &str, user_id: &str, level: &str, message: &str) {
    let app_emit = app.clone();
    let channel_id = channel_id.to_string();
    let user_id = user_id.to_string();
    let level = level.to_string();
    let message = message.to_string();
    let timestamp = now_timestamp_ms();
    let payload = serde_json::json!({
        "channelId": channel_id,
        "userId": user_id,
        "level": level,
        "message": message,
        "timestamp": timestamp,
    });
    // 避免在 invoke 尚未返回时同步 eval 到 Webview，否则前端 await invoke 会与 emit 死锁。
    let _ = app.clone().run_on_main_thread(move || {
        let _ = app_emit.emit("bot://status", payload);
    });
}

fn emit_bot_message(
    app: &AppHandle,
    channel_id: &str,
    user_id: &str,
    direction: &str,
    content: &str,
    agent: Option<&ConversationAgentConfig>,
) {
    let payload = BotMessage {
        channel_id: channel_id.to_string(),
        user_id: user_id.to_string(),
        direction: direction.to_string(),
        content: content.to_string(),
        timestamp: now_timestamp_ms(),
        agent: agent.cloned(),
    };
    let app_emit = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let Err(error) = app_emit.emit("bot://message", &payload) {
            log::error!("emit bot://message 失败: {error}");
        }
    });
}
