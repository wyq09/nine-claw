use crate::agents::{self, ConversationAgentConfig};
use crate::dev_trace::{dev_trace, dev_trace_block};
use crate::pi_runtime::{self, PiRuntimeLocation};
use crate::pi_timeouts;
use crate::prompt_attachments::{self, PreparedPromptInput, PromptAttachmentInput};
use crate::skills;
use md5::{Digest, Md5};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ABORT_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const CHILD_KILL_GRACE_TIMEOUT: Duration = Duration::from_secs(1);

fn pi_reuse_im_enabled() -> bool {
    std::env::var("NINECLAW_PI_REUSE_IM")
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

struct ImPooledPi {
    child: Child,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stdout_rx: mpsc::Receiver<Result<String, String>>,
    stderr_buffer: Arc<Mutex<String>>,
    fingerprint: String,
}

type ImPiPoolMap = HashMap<String, Arc<Mutex<Option<ImPooledPi>>>>;

fn im_pi_pool() -> &'static Mutex<ImPiPoolMap> {
    static POOL: OnceLock<Mutex<ImPiPoolMap>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fingerprint_im_pi_turn(
    provider_id: &str,
    api_format: &str,
    base_url: &str,
    api_key: &str,
    model: &str,
    session_path: &str,
    system_prompt_sections: &[(String, String)],
    skill_paths: &[PathBuf],
) -> String {
    let mut blob: Vec<u8> = Vec::new();
    blob.extend_from_slice(provider_id.as_bytes());
    blob.push(0);
    blob.extend_from_slice(api_format.as_bytes());
    blob.push(0);
    blob.extend_from_slice(base_url.as_bytes());
    blob.push(0);
    blob.extend_from_slice(api_key.as_bytes());
    blob.push(0);
    blob.extend_from_slice(model.as_bytes());
    blob.push(0);
    blob.extend_from_slice(session_path.as_bytes());
    blob.push(0);
    for (label, content) in system_prompt_sections {
        blob.extend_from_slice(label.as_bytes());
        blob.push(1);
        blob.extend_from_slice(content.as_bytes());
        blob.push(2);
    }
    for path in skill_paths {
        blob.extend_from_slice(path.as_os_str().as_encoded_bytes());
        blob.push(3);
    }
    format!("{:x}", Md5::digest(&blob))
}

fn spawn_pi_stdout_logger<R>(
    reader: R,
    scope: &'static str,
    tx: mpsc::Sender<Result<String, String>>,
) where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        dev_trace(scope, trimmed);
                    }
                    let _ = tx.send(Ok(line));
                }
                Err(error) => {
                    let message = error.to_string();
                    dev_trace(scope, format!("读取失败: {message}"));
                    let _ = tx.send(Err(message));
                    break;
                }
            }
        }
    });
}

fn spawn_pi_stderr_logger<R>(reader: R, scope: &'static str, buffer: Arc<Mutex<String>>)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        dev_trace(scope, trimmed);
                    }
                    if let Ok(mut stderr) = buffer.lock() {
                        stderr.push_str(&line);
                        stderr.push('\n');
                    }
                }
                Err(error) => {
                    let message = error.to_string();
                    dev_trace(scope, format!("读取失败: {message}"));
                    if let Ok(mut stderr) = buffer.lock() {
                        stderr.push_str(&message);
                        stderr.push('\n');
                    }
                    break;
                }
            }
        }
    });
}

#[derive(Clone)]
pub struct PiRunHandle {
    abort_requested: Arc<AtomicBool>,
    pid: u32,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

impl PiRunHandle {
    pub fn abort(&self) -> Result<(), String> {
        self.abort_requested.store(true, Ordering::SeqCst);

        let abort_command = json!({
            "id": format!("abort-{}", self.pid),
            "type": "abort",
        })
        .to_string();

        let write_result = (|| -> Result<(), String> {
            let mut stdin = self
                .stdin
                .lock()
                .map_err(|error| format!("无法锁定 pi abort stdin: {error}"))?;
            let stdin = stdin
                .as_mut()
                .ok_or_else(|| "pi stdin 已关闭，无法发送 abort 指令".to_string())?;
            writeln!(stdin, "{abort_command}")
                .map_err(|error| format!("发送 pi abort 指令失败: {error}"))?;
            stdin
                .flush()
                .map_err(|error| format!("刷新 pi abort 指令失败: {error}"))?;
            Ok(())
        })();

        if write_result.is_ok() {
            return Ok(());
        }

        let status = Command::new("kill")
            .args(["-TERM", &self.pid.to_string()])
            .status()
            .map_err(|error| format!("中止 pi 进程失败: {error}"))?;

        if status.success() {
            Ok(())
        } else {
            Err(format!("中止 pi 进程失败，退出码: {status}"))
        }
    }

    pub fn is_abort_requested(&self) -> bool {
        self.abort_requested.load(Ordering::SeqCst)
    }
}

pub enum PiProcessOutcome {
    Completed(String),
    Aborted,
}

struct ChildExitOutcome {
    status: Option<ExitStatus>,
    timed_out: bool,
}

/// Manages pi sessions for bot channel message processing.
/// Temporary IM session context stays isolated per (channel_id, user_id) pair.
/// Long-term memory is still shared at the agent workspace layer because each
/// turn re-injects the bound agent's md files into the system prompt.
pub struct PiBridge {
    // sessions field reserved for future persistent-session reuse
    #[allow(dead_code)]
    sessions: Mutex<HashMap<String, ()>>,
    pi_runtime: PiRuntimeLocation,
    provider_id: String,
    api_format: String,
    base_url: String,
    api_key: String,
    model: String,
    agent_config: Option<ConversationAgentConfig>,
}

impl PiBridge {
    pub fn new(
        pi_runtime: PiRuntimeLocation,
        provider_id: &str,
        api_format: &str,
        base_url: &str,
        api_key: &str,
        model: &str,
        agent_config: Option<ConversationAgentConfig>,
    ) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            pi_runtime,
            provider_id: provider_id.to_string(),
            api_format: api_format.to_string(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            agent_config,
        }
    }

    /// Generate a deterministic session key from (channel, user) pair.
    ///
    /// Cross-channel sharing belongs to the persistent workspace memory layer,
    /// not the transient IM session file. We intentionally keep bot sessions
    /// isolated so WeChat and Lark can each maintain their own short-term turns.
    fn session_key(&self, channel_id: &str, user_id: &str) -> String {
        let mut hasher = Md5::new();
        hasher.update(format!("nineclaw:{}:{}", channel_id, user_id).as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn session_file_path(key: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nineclaw-bot-session-{key}.jsonl"))
    }

    fn ephemeral_session_file_path(key: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("nineclaw-bot-session-{key}-media-{nonce}.jsonl"))
    }

    fn pi_runtime_dir() -> PathBuf {
        std::env::temp_dir().join("nineclaw-pi-runtime")
    }

    fn attachment_requires_fresh_session(attachment: &PromptAttachmentInput) -> bool {
        let kind = attachment.kind.trim().to_ascii_lowercase();
        if kind == "image" || kind == "video" {
            return true;
        }

        let mime = attachment.mime_type.trim().to_ascii_lowercase();
        mime.starts_with("image/") || mime.starts_with("video/")
    }

    fn has_multimodal_attachments(attachments: &[PromptAttachmentInput]) -> bool {
        attachments
            .iter()
            .any(Self::attachment_requires_fresh_session)
    }

    fn log_attachment_debug(
        channel_id: &str,
        user_id: &str,
        attachments: &[PromptAttachmentInput],
    ) {
        for (index, attachment) in attachments.iter().enumerate() {
            let path = attachment.file_path.trim();
            let bytes = if path.is_empty() {
                None
            } else {
                fs::read(path).ok()
            };
            let size = bytes.as_ref().map(|data| data.len()).unwrap_or(0);
            let digest = bytes
                .as_ref()
                .map(|data| format!("{:x}", Md5::digest(data)))
                .unwrap_or_else(|| "unavailable".to_string());
            dev_trace(
                "bot.pi",
                format!(
                    "prompt_attachment: channel={} user={} index={} kind={} mime={} path={} bytes={} md5={}",
                    channel_id,
                    user_id,
                    index,
                    attachment.kind.trim(),
                    attachment.mime_type.trim(),
                    path,
                    size,
                    digest
                ),
            );
        }
    }

    fn normalize_provider_base_url(value: &str) -> &str {
        value.trim().trim_end_matches('/')
    }

    fn normalize_anthropic_base_url(value: &str) -> String {
        Self::normalize_provider_base_url(value)
            .trim_end_matches("/v1/messages")
            .trim_end_matches("/messages")
            .trim_end_matches("/v1")
            .trim_end_matches('/')
            .to_string()
    }

    fn default_provider_api_format(provider_id: &str) -> &'static str {
        match provider_id {
            "anthropic" => "anthropic",
            _ => "openai",
        }
    }

    fn normalize_provider_api_format(value: &str, provider_id: &str) -> &'static str {
        match value.trim() {
            "anthropic" => "anthropic",
            "openai" => "openai",
            _ => Self::default_provider_api_format(provider_id),
        }
    }

    fn normalized_runtime_base_url(&self) -> String {
        match Self::normalize_provider_api_format(&self.api_format, self.provider_id.trim()) {
            "anthropic" => Self::normalize_anthropic_base_url(&self.base_url),
            _ => Self::normalize_provider_base_url(&self.base_url).to_string(),
        }
    }

    fn runtime_provider_id(&self) -> String {
        let trimmed = self.provider_id.trim();
        if trimmed.is_empty() {
            return "nineclaw-runtime-provider".to_string();
        }

        let digest = format!("{:x}", Md5::digest(trimmed.as_bytes()));
        format!("nineclaw-runtime-{}", &digest[..12])
    }

    fn scrub_anthropic_process_env(command: &mut Command) {
        for key in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_OAUTH_TOKEN",
        ] {
            command.env_remove(key);
        }
    }

    fn extract_text_content_from_message(value: Option<&serde_json::Value>) -> Option<String> {
        let Some(content) = value
            .and_then(|item| item.get("content"))
            .and_then(|item| item.as_array())
        else {
            return None;
        };

        let joined = content
            .iter()
            .filter_map(|entry| entry.get("text").and_then(|text| text.as_str()))
            .collect::<Vec<_>>()
            .join("");

        if joined.is_empty() {
            None
        } else {
            Some(joined)
        }
    }

    fn build_provider_models_config(&self) -> Option<serde_json::Value> {
        let provider_id = self.provider_id.trim();
        let base_url = self.normalized_runtime_base_url();
        let model = self.model.trim();
        let api_format = Self::normalize_provider_api_format(&self.api_format, provider_id);

        if provider_id.is_empty() || base_url.is_empty() || model.is_empty() {
            return None;
        }

        let mut provider = serde_json::Map::new();
        provider.insert("baseUrl".to_string(), json!(base_url));
        provider.insert(
            "apiKey".to_string(),
            json!(if self.api_key.trim().is_empty() {
                "DUMMY_KEY"
            } else {
                self.api_key.trim()
            }),
        );
        match api_format {
            "anthropic" => {
                provider.insert("api".to_string(), json!("anthropic-messages"));
                // Keep bot/runtime requests aligned with custom Anthropic
                // gateways that expect `Authorization: Bearer <apiKey>`.
                provider.insert("authHeader".to_string(), json!(true));
                provider.insert(
                    "models".to_string(),
                    json!([{ "id": model, "api": "anthropic-messages", "input": ["text", "image"] }]),
                );
            }
            _ => {
                provider.insert("api".to_string(), json!("openai-completions"));
                provider.insert(
                    "compat".to_string(),
                    json!({
                        "supportsDeveloperRole": false,
                        "supportsReasoningEffort": false
                    }),
                );
                provider.insert(
                    "models".to_string(),
                    json!([{ "id": model, "api": "openai-completions", "input": ["text", "image"] }]),
                );
            }
        }
        let mut providers = serde_json::Map::new();
        providers.insert(
            self.runtime_provider_id(),
            serde_json::Value::Object(provider),
        );
        Some(json!({ "providers": providers }))
    }

    fn prepare_runtime_dir() -> Result<PathBuf, String> {
        let dir = Self::pi_runtime_dir();
        fs::create_dir_all(&dir).map_err(|e| format!("创建 pi runtime 目录失败: {e}"))?;

        fs::write(dir.join("auth.json"), "{}").map_err(|e| format!("写入 auth.json 失败: {e}"))?;

        Ok(dir)
    }

    fn extract_assistant_error_from_message(message: &serde_json::Value) -> Option<String> {
        if message.get("role").and_then(|v| v.as_str()) != Some("assistant") {
            return None;
        }

        let error_message = message
            .get("errorMessage")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim();
        if !error_message.is_empty() {
            return Some(error_message.to_string());
        }

        let stop_reason = message
            .get("stopReason")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if stop_reason == "error" {
            return Some("assistant 返回了 error stopReason，但未提供错误详情".to_string());
        }

        None
    }

    fn extract_assistant_error(value: &serde_json::Value) -> Option<String> {
        value
            .get("message")
            .and_then(Self::extract_assistant_error_from_message)
            .or_else(|| {
                value
                    .get("messages")
                    .and_then(|v| v.as_array())
                    .and_then(|messages| {
                        messages
                            .iter()
                            .rev()
                            .find_map(Self::extract_assistant_error_from_message)
                    })
            })
    }

    fn wait_for_child_exit(
        child: &mut Child,
        timeout: Duration,
    ) -> Result<ChildExitOutcome, String> {
        let deadline = Instant::now() + timeout;

        loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| format!("检查 pi 退出状态失败: {error}"))?
            {
                return Ok(ChildExitOutcome {
                    status: Some(status),
                    timed_out: false,
                });
            }

            if Instant::now() >= deadline {
                let _ = child.kill();
                let kill_deadline = Instant::now() + CHILD_KILL_GRACE_TIMEOUT;
                while Instant::now() < kill_deadline {
                    if let Some(status) = child
                        .try_wait()
                        .map_err(|error| format!("回收超时 pi 进程失败: {error}"))?
                    {
                        return Ok(ChildExitOutcome {
                            status: Some(status),
                            timed_out: true,
                        });
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                return Ok(ChildExitOutcome {
                    status: None,
                    timed_out: true,
                });
            }

            thread::sleep(Duration::from_millis(100));
        }
    }

    fn spawn_pi_child_fresh(
        &self,
        mut cmd: Command,
        channel_id: &str,
        user_id: &str,
        system_prompt_chars: usize,
        prepared_input: &PreparedPromptInput,
        system_prompt_sections: &[(String, String)],
    ) -> Result<
        (
            Child,
            Arc<Mutex<Option<ChildStdin>>>,
            mpsc::Receiver<Result<String, String>>,
            Arc<Mutex<String>>,
        ),
        String,
    > {
        let mut child = cmd.spawn().map_err(|e| {
            format!(
                "启动 pi 失败（executable={}）: {e}",
                self.pi_runtime.executable.display()
            )
        })?;
        dev_trace(
            "bot.pi",
            format!(
                "启动 pi: channel={} user={} pid={} provider={} model={} prompt_chars={} system_prompt_chars={} pi_path={}",
                channel_id,
                user_id,
                child.id(),
                self.provider_id,
                self.model,
                prepared_input.message.chars().count(),
                system_prompt_chars,
                self.pi_runtime.executable.display()
            ),
        );
        for (label, content) in system_prompt_sections {
            dev_trace(
                "bot.pi",
                format!(
                    "system_prompt_part: channel={} user={} label={} chars={}",
                    channel_id,
                    user_id,
                    label,
                    content.chars().count()
                ),
            );
            dev_trace_block(
                "bot.pi",
                format!(
                    "system_prompt_part channel={} user={} label={} chars={}",
                    channel_id,
                    user_id,
                    label,
                    content.chars().count()
                ),
                content,
            );
        }

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "无法获取 pi stdin".to_string())?;
        let stdin = Arc::new(Mutex::new(Some(stdin)));

        {
            let mut stdin_guard = stdin
                .lock()
                .map_err(|e| format!("锁定 pi stdin 失败: {e}"))?;
            let stdin_writer = stdin_guard
                .as_mut()
                .ok_or_else(|| "pi stdin 已关闭，无法写入 prompt".to_string())?;
            let prompt_cmd = json!({
                "id": "prompt-1",
                "type": "prompt",
                "message": prepared_input.message,
                "images": prepared_input.images,
            })
            .to_string();
            writeln!(stdin_writer, "{prompt_cmd}").map_err(|e| format!("写入 prompt 失败: {e}"))?;
            stdin_writer
                .flush()
                .map_err(|e| format!("flush stdin 失败: {e}"))?;
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "无法获取 pi stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "无法获取 pi stderr".to_string())?;
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let stderr_buffer = Arc::new(Mutex::new(String::new()));
        spawn_pi_stdout_logger(stdout, "bot.pi.raw", stdout_tx);
        spawn_pi_stderr_logger(stderr, "bot.pi.stderr", stderr_buffer.clone());

        Ok((child, stdin, stdout_rx, stderr_buffer))
    }

    /// Process an incoming message through pi, calling `on_chunk` every `chunk_size` characters.
    pub fn process_message<F>(
        &self,
        channel_id: &str,
        user_id: &str,
        prompt: &str,
        chunk_size: usize,
        on_chunk: F,
    ) -> Result<String, String>
    where
        F: FnMut(&str),
    {
        match self.process_message_with_attachments_interruptible(
            channel_id,
            user_id,
            prompt,
            &[],
            chunk_size,
            on_chunk,
            |_| {},
        )? {
            PiProcessOutcome::Completed(full_text) => Ok(full_text),
            PiProcessOutcome::Aborted => Err("pi 处理被中断".to_string()),
        }
    }

    pub fn process_message_interruptible<F, S>(
        &self,
        channel_id: &str,
        user_id: &str,
        prompt: &str,
        chunk_size: usize,
        on_chunk: F,
        on_run_start: S,
    ) -> Result<PiProcessOutcome, String>
    where
        F: FnMut(&str),
        S: FnOnce(Arc<PiRunHandle>),
    {
        self.process_message_with_attachments_interruptible(
            channel_id,
            user_id,
            prompt,
            &[],
            chunk_size,
            on_chunk,
            on_run_start,
        )
    }

    pub fn process_message_with_attachments_interruptible<F, S>(
        &self,
        channel_id: &str,
        user_id: &str,
        prompt: &str,
        attachments: &[PromptAttachmentInput],
        chunk_size: usize,
        mut on_chunk: F,
        on_run_start: S,
    ) -> Result<PiProcessOutcome, String>
    where
        F: FnMut(&str),
        S: FnOnce(Arc<PiRunHandle>),
    {
        let key = self.session_key(channel_id, user_id);
        let fresh_multimodal_session = Self::has_multimodal_attachments(attachments);
        let session_path = if fresh_multimodal_session {
            Self::ephemeral_session_file_path(&key)
        } else {
            Self::session_file_path(&key)
        };
        let runtime_dir = Self::prepare_runtime_dir()?;
        let prepared_input = prompt_attachments::prepare_prompt_input(prompt, attachments)?;
        Self::log_attachment_debug(channel_id, user_id, attachments);
        dev_trace(
            "bot.pi",
            format!(
                "prompt_payload: channel={} user={} message_chars={} images={} fresh_multimodal_session={} session={}",
                channel_id,
                user_id,
                prepared_input.message.chars().count(),
                prepared_input.images.len(),
                fresh_multimodal_session,
                session_path.display()
            ),
        );

        // Write models config if needed
        let models_path = runtime_dir.join("models.json");
        if let Some(config) = self.build_provider_models_config() {
            let content = serde_json::to_vec_pretty(&config)
                .map_err(|e| format!("序列化 models 配置失败: {e}"))?;
            fs::write(&models_path, content).map_err(|e| format!("写入 models.json 失败: {e}"))?;
        } else if models_path.exists() {
            fs::remove_file(&models_path).map_err(|e| format!("清理旧的 models.json 失败: {e}"))?;
        }

        let mut cmd = Command::new(&self.pi_runtime.executable);
        pi_runtime::apply_runtime_environment(&mut cmd, &self.pi_runtime);
        cmd.args([
            "--mode",
            "rpc",
            "--session",
            &session_path.to_string_lossy(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PI_CODING_AGENT_DIR", &runtime_dir);

        if Self::normalize_provider_api_format(&self.api_format, self.provider_id.trim())
            == "anthropic"
        {
            Self::scrub_anthropic_process_env(&mut cmd);
        }

        let runtime_provider_id = self.runtime_provider_id();
        if !runtime_provider_id.is_empty() {
            cmd.args(["--provider", &runtime_provider_id]);
        }
        if !self.model.is_empty() {
            cmd.args(["--model", &self.model]);
        }
        if !self.api_key.is_empty() {
            cmd.args(["--api-key", &self.api_key]);
        }

        let mut skill_paths: Vec<PathBuf> = Vec::new();
        let mut system_prompt_chars = 0usize;
        let mut system_prompt_sections: Vec<(String, String)> = Vec::new();
        if let Some(agent_config) = self.agent_config.as_ref() {
            if let Ok(workspace_root) = crate::agent_workspace::resolve_workspace_root() {
                let agent_home = workspace_root.join("agents").join(&agent_config.id);
                cmd.current_dir(&agent_home)
                    .env("NINECLAW_AGENT_ID", &agent_config.id)
                    .env("NINECLAW_AGENT_NAME", &agent_config.name)
                    .env("NINECLAW_WORKSPACE_ROOT", workspace_root.as_os_str())
                    .env("NINECLAW_AGENT_HOME", agent_home.as_os_str());
            }

            if let Some(system_prompt) = agents::build_agent_system_prompt_for_prompt(
                agent_config,
                Some(prepared_input.message.as_str()),
            ) {
                system_prompt_chars += system_prompt.chars().count();
                system_prompt_sections
                    .push(("agent_system_prompt".to_string(), system_prompt.clone()));
                cmd.args(["--append-system-prompt", &system_prompt]);
            }

            let media_prompt = "当前回复目标是 IM 用户。如果你需要把本地生成的图片、文件或视频真正发送给用户，请单独输出一行 `::nc-media{type=\"image|file|video\" path=\"/absolute/path/to/file\"}`。该指令行不要附加解释文字；普通文本说明单独写在其他行。";
            system_prompt_chars += media_prompt.chars().count();
            system_prompt_sections.push(("im_media".to_string(), media_prompt.to_string()));
            cmd.args(["--append-system-prompt", media_prompt]);
            let memory_isolation_prompt = "记忆隔离规则：当前智能体只能使用自己的私有工作区记忆。禁止读取、引用、总结或迁移其他智能体 `agents/<other-agent-id>/` 下的任何 markdown 记忆文件。";
            system_prompt_chars += memory_isolation_prompt.chars().count();
            system_prompt_sections.push((
                "memory_isolation".to_string(),
                memory_isolation_prompt.to_string(),
            ));
            cmd.args(["--append-system-prompt", memory_isolation_prompt]);

            for skill_path in skills::resolve_skill_directories(&agent_config.skill_ids)? {
                skill_paths.push(skill_path.clone());
                cmd.args(["--skill", &skill_path.to_string_lossy()]);
            }
        }

        let session_path_str = session_path.to_string_lossy().to_string();
        let fingerprint = fingerprint_im_pi_turn(
            self.provider_id.trim(),
            self.api_format.trim(),
            self.base_url.trim(),
            self.api_key.trim(),
            self.model.trim(),
            &session_path_str,
            &system_prompt_sections,
            &skill_paths,
        );

        let pool_slot = {
            let mut map = im_pi_pool()
                .lock()
                .map_err(|_| "锁定 IM pi 池失败".to_string())?;
            map.entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(None)))
                .clone()
        };
        let mut pool_guard = pool_slot
            .lock()
            .map_err(|_| "锁定 IM pi 会话槽失败".to_string())?;

        if fresh_multimodal_session {
            if let Some(mut prev) = pool_guard.take() {
                let prev_pid = prev.child.id();
                let _ = prev.child.kill();
                let _ = Self::wait_for_child_exit(&mut prev.child, CHILD_KILL_GRACE_TIMEOUT);
                dev_trace(
                    "bot.pi",
                    format!(
                        "清理旧池化 pi: channel={} user={} old_pid={} reason=fresh_multimodal_session",
                        channel_id, user_id, prev_pid
                    ),
                );
            }
        }

        let (mut child, stdin, stdout_rx, stderr_buffer, reused_from_pool) =
            if pi_reuse_im_enabled() && !fresh_multimodal_session {
                if let Some(mut prev) = pool_guard.take() {
                    let still_running = prev
                        .child
                        .try_wait()
                        .map_err(|e| format!("检查池化 pi 状态失败: {e}"))?
                        .is_none();
                    if prev.fingerprint == fingerprint && still_running {
                        dev_trace(
                            "bot.pi",
                            format!(
                                "复用池化 pi: channel={} user={} pid={}",
                                channel_id,
                                user_id,
                                prev.child.id(),
                            ),
                        );
                        let ImPooledPi {
                            child,
                            stdin,
                            stdout_rx,
                            stderr_buffer,
                            fingerprint: _,
                        } = prev;
                        (child, stdin, stdout_rx, stderr_buffer, true)
                    } else {
                        let _ = prev.child.kill();
                        let _ =
                            Self::wait_for_child_exit(&mut prev.child, CHILD_KILL_GRACE_TIMEOUT);
                        drop(prev);
                        let (c, i, o, e) = self.spawn_pi_child_fresh(
                            cmd,
                            channel_id,
                            user_id,
                            system_prompt_chars,
                            &prepared_input,
                            &system_prompt_sections,
                        )?;
                        (c, i, o, e, false)
                    }
                } else {
                    let (c, i, o, e) = self.spawn_pi_child_fresh(
                        cmd,
                        channel_id,
                        user_id,
                        system_prompt_chars,
                        &prepared_input,
                        &system_prompt_sections,
                    )?;
                    (c, i, o, e, false)
                }
            } else {
                let (c, i, o, e) = self.spawn_pi_child_fresh(
                    cmd,
                    channel_id,
                    user_id,
                    system_prompt_chars,
                    &prepared_input,
                    &system_prompt_sections,
                )?;
                (c, i, o, e, false)
            };

        let run_handle = Arc::new(PiRunHandle {
            abort_requested: Arc::new(AtomicBool::new(false)),
            pid: child.id(),
            stdin: stdin.clone(),
        });

        if reused_from_pool {
            let prompt_ns = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let prompt_id = format!("prompt-{prompt_ns}");
            {
                let mut stdin_guard = stdin
                    .lock()
                    .map_err(|e| format!("锁定 pi stdin 失败: {e}"))?;
                let stdin_writer = stdin_guard
                    .as_mut()
                    .ok_or_else(|| "pi stdin 已关闭，无法写入 prompt".to_string())?;
                let prompt_cmd = json!({
                    "id": prompt_id,
                    "type": "prompt",
                    "message": prepared_input.message,
                    "images": prepared_input.images,
                })
                .to_string();
                writeln!(stdin_writer, "{prompt_cmd}")
                    .map_err(|e| format!("写入 prompt 失败: {e}"))?;
                stdin_writer
                    .flush()
                    .map_err(|e| format!("flush stdin 失败: {e}"))?;
            }
        }
        on_run_start(run_handle.clone());

        let mut full_text = String::new();
        let mut chunk_buffer = String::new();
        let mut saw_agent_end = false;
        let mut saw_done = false;
        let mut saw_any_output = false;
        let mut saw_prompt_response = false;
        let mut saw_abort_event = false;
        let mut saw_assistant_activity = false;
        let started_at = Instant::now();
        let ttft_start = Instant::now();
        let mut logged_ttft = false;
        let pi_total_runtime_timeout = pi_timeouts::pi_total_runtime_timeout();
        let pi_first_output_timeout = pi_timeouts::pi_first_output_timeout();
        let pi_idle_output_timeout = pi_timeouts::pi_idle_output_timeout();

        loop {
            if started_at.elapsed() >= pi_total_runtime_timeout {
                let _ = child.kill();
                let _ = Self::wait_for_child_exit(&mut child, CHILD_KILL_GRACE_TIMEOUT);
                if let Ok(mut stdin_guard) = stdin.lock() {
                    let _ = stdin_guard.take();
                }
                let error = format!(
                    "pi 总运行超时（>{} 秒）",
                    pi_total_runtime_timeout.as_secs()
                );
                dev_trace(
                    "bot.pi",
                    format!(
                        "总超时: channel={} user={} error={}",
                        channel_id, user_id, error
                    ),
                );
                return Err(error);
            }

            let base_timeout = if saw_any_output {
                pi_idle_output_timeout
            } else {
                pi_first_output_timeout
            };
            let remaining_total = pi_total_runtime_timeout
                .checked_sub(started_at.elapsed())
                .unwrap_or(Duration::from_secs(0));
            let timeout = base_timeout.min(remaining_total);
            let line = match stdout_rx.recv_timeout(timeout) {
                Ok(Ok(line)) => {
                    if !logged_ttft {
                        logged_ttft = true;
                        dev_trace(
                            "bot.pi",
                            format!(
                                "stdout_ttft_ms={} channel={} user={} reused_pool={}",
                                ttft_start.elapsed().as_millis(),
                                channel_id,
                                user_id,
                                reused_from_pool
                            ),
                        );
                    }
                    line
                }
                Ok(Err(_)) => {
                    if run_handle.is_abort_requested() {
                        saw_abort_event = true;
                        break;
                    }
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {
                    let timeout_reason = if timeout == remaining_total {
                        format!(
                            "pi 总运行超时（>{} 秒）",
                            pi_total_runtime_timeout.as_secs()
                        )
                    } else if saw_any_output {
                        format!(
                            "等待 pi 后续输出超时（>{} 秒）",
                            pi_idle_output_timeout.as_secs()
                        )
                    } else {
                        format!(
                            "等待 pi 首包输出超时（>{} 秒）",
                            pi_first_output_timeout.as_secs()
                        )
                    };
                    dev_trace(
                        "bot.pi",
                        format!(
                            "超时: channel={} user={} error={}",
                            channel_id, user_id, timeout_reason
                        ),
                    );
                    let _ = child.kill();
                    let _ = Self::wait_for_child_exit(&mut child, CHILD_KILL_GRACE_TIMEOUT);
                    if let Ok(mut stdin_guard) = stdin.lock() {
                        let _ = stdin_guard.take();
                    }
                    return Err(timeout_reason);
                }
                Err(RecvTimeoutError::Disconnected) => break,
            };
            if !line.trim().is_empty() {
                saw_any_output = true;
            }

            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => {
                    if run_handle.is_abort_requested() {
                        saw_abort_event = true;
                        break;
                    }
                    continue;
                }
            };

            let line_type = value
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            if matches!(
                line_type,
                "message_start" | "message_end" | "turn_end" | "agent_end"
            ) {
                if let Some(err) = Self::extract_assistant_error(&value) {
                    dev_trace(
                        "bot.pi",
                        format!(
                            "assistant error: channel={} user={} error={}",
                            channel_id, user_id, err
                        ),
                    );
                    return Err(format!("pi assistant 错误: {err}"));
                }
            }

            if line_type == "message_update" {
                let event = value.get("assistantMessageEvent");
                let delta_type = event
                    .and_then(|v| v.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                if delta_type == "text_delta" {
                    let text = event
                        .and_then(|v| v.get("delta"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if !text.is_empty() {
                        saw_assistant_activity = true;
                        full_text.push_str(text);
                        chunk_buffer.push_str(text);
                        if chunk_buffer.len() >= chunk_size {
                            on_chunk(&chunk_buffer);
                            chunk_buffer.clear();
                        }
                    }
                }

                if delta_type == "done" {
                    if let Some(final_text) = Self::extract_text_content_from_message(
                        event.and_then(|v| v.get("message")),
                    ) {
                        saw_assistant_activity = true;
                        let missing_text = if full_text.is_empty() {
                            final_text
                        } else if let Some(suffix) = final_text.strip_prefix(&full_text) {
                            suffix.to_string()
                        } else if final_text != full_text {
                            final_text
                        } else {
                            String::new()
                        };

                        if !missing_text.is_empty() {
                            full_text.push_str(&missing_text);
                            chunk_buffer.push_str(&missing_text);
                        }
                    }
                    saw_done = true;
                    break;
                }

                if delta_type == "error" {
                    let reason = event
                        .and_then(|v| v.get("reason"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    if reason == "aborted" {
                        saw_abort_event = true;
                        break;
                    }
                    dev_trace(
                        "bot.pi",
                        format!(
                            "流错误: channel={} user={} error={}",
                            channel_id, user_id, reason
                        ),
                    );
                    return Err(format!("pi 流错误: {reason}"));
                }
            }

            if line_type == "agent_end" {
                saw_agent_end = true;
                if run_handle.is_abort_requested() {
                    saw_abort_event = true;
                }
                break;
            }

            // Surface explicit error responses from pi
            if line_type == "response" {
                let command = value
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let success = value
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if command == "prompt" && success {
                    saw_prompt_response = true;
                }
                if !success {
                    let err = value
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("未知错误");
                    if run_handle.is_abort_requested() {
                        saw_abort_event = true;
                        break;
                    }
                    dev_trace(
                        "bot.pi",
                        format!(
                            "RPC 错误: channel={} user={} error={}",
                            channel_id, user_id, err
                        ),
                    );
                    return Err(format!("pi RPC 错误: {err}"));
                }
            }
        }

        // Flush remaining chunk
        if !chunk_buffer.is_empty() && !run_handle.is_abort_requested() && !saw_abort_event {
            on_chunk(&chunk_buffer);
        }

        let stderr_text = stderr_buffer
            .lock()
            .map(|stderr| stderr.clone())
            .unwrap_or_else(|_| String::new());

        let close_stdin = || {
            if let Ok(mut stdin_guard) = stdin.lock() {
                let _ = stdin_guard.take();
            }
        };

        if run_handle.is_abort_requested() || saw_abort_event {
            close_stdin();
            let _ = child.kill();
            let _ = Self::wait_for_child_exit(&mut child, ABORT_WAIT_TIMEOUT);
            dev_trace(
                "bot.pi",
                format!("已中止: channel={} user={}", channel_id, user_id),
            );
            return Ok(PiProcessOutcome::Aborted);
        }

        let still_alive_after_turn = child
            .try_wait()
            .map_err(|e| format!("检查 pi 退出状态失败: {e}"))?
            .is_none();

        if full_text.is_empty() && !saw_done && !saw_agent_end {
            close_stdin();
            let exit_outcome = Self::wait_for_child_exit(&mut child, Duration::from_secs(30))?;
            let exit_status = exit_outcome.status;
            let reason = if !stderr_text.trim().is_empty() {
                stderr_text.trim().to_string()
            } else if saw_prompt_response || saw_any_output || saw_assistant_activity {
                if self.provider_id.trim().is_empty()
                    || self.base_url.trim().is_empty()
                    || self.model.trim().is_empty()
                {
                    "pi 已接收消息，但当前微信 Bot 没有关联完整的 Provider 配置，因此没有返回 assistant 回复。请在设置中补全 Base URL、API Key 和模型后重新连接。".to_string()
                } else {
                    let provider_label = if self.api_format.trim().is_empty() {
                        self.provider_id.trim().to_string()
                    } else {
                        format!(
                            "{} ({})",
                            self.provider_id.trim(),
                            Self::normalize_provider_api_format(
                                &self.api_format,
                                self.provider_id.trim()
                            )
                        )
                    };
                    format!(
                        "pi 已接收消息，但没有返回 assistant 回复（provider={}，model={}）。请检查 Provider 配置或确认当前 pi 版本是否变更了 RPC 输出行为。",
                        provider_label,
                        self.model.trim()
                    )
                }
            } else if exit_outcome.timed_out {
                "pi 在回收时超时，运行时已主动脱离该卡死进程。".to_string()
            } else {
                match exit_status {
                    Some(status) => format!("pi 退出码 {status}，无输出内容"),
                    None => "pi 已被请求终止，但进程仍未退出且没有输出内容".to_string(),
                }
            };
            dev_trace(
                "bot.pi",
                format!(
                    "无回复退出: channel={} user={} error={}",
                    channel_id, user_id, reason
                ),
            );
            return Err(reason);
        }

        if pi_reuse_im_enabled() && !fresh_multimodal_session && still_alive_after_turn {
            *pool_guard = Some(ImPooledPi {
                child,
                stdin,
                stdout_rx,
                stderr_buffer,
                fingerprint,
            });
            dev_trace(
                "bot.pi",
                format!(
                    "完成(池化回收): channel={} user={} chars={}",
                    channel_id,
                    user_id,
                    full_text.chars().count()
                ),
            );
            return Ok(PiProcessOutcome::Completed(full_text));
        }

        close_stdin();
        let exit_outcome = Self::wait_for_child_exit(&mut child, Duration::from_secs(30))?;
        let exit_status = exit_outcome.status;

        if exit_status.map(|status| !status.success()).unwrap_or(true) {
            let reason = if exit_outcome.timed_out && stderr_text.trim().is_empty() {
                "等待 pi 退出超时，运行时已主动脱离该卡死进程。".to_string()
            } else if !stderr_text.trim().is_empty() {
                stderr_text.trim().to_string()
            } else {
                match exit_status {
                    Some(status) => format!("pi 退出码异常: {status}"),
                    None => "pi 已被请求终止，但进程仍未退出".to_string(),
                }
            };
            dev_trace(
                "bot.pi",
                format!(
                    "异常退出: channel={} user={} error={}",
                    channel_id, user_id, reason
                ),
            );
            return Err(reason);
        }

        dev_trace(
            "bot.pi",
            format!(
                "完成: channel={} user={} chars={}",
                channel_id,
                user_id,
                full_text.chars().count()
            ),
        );
        Ok(PiProcessOutcome::Completed(full_text))
    }
}
