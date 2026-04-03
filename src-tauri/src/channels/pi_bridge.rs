use crate::agents::{self, ConversationAgentConfig};
use crate::skills;
use md5::{Digest, Md5};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// Manages per-user pi sessions for bot channel message processing.
/// Each (channel_id, user_id) pair gets its own pi subprocess with an independent session file.
pub struct PiBridge {
    // sessions field reserved for future persistent-session reuse
    #[allow(dead_code)]
    sessions: Mutex<HashMap<String, ()>>,
    provider_id: String,
    api_format: String,
    base_url: String,
    api_key: String,
    model: String,
    agent_config: Option<ConversationAgentConfig>,
}

impl PiBridge {
    pub fn new(
        provider_id: &str,
        api_format: &str,
        base_url: &str,
        api_key: &str,
        model: &str,
        agent_config: Option<ConversationAgentConfig>,
    ) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            provider_id: provider_id.to_string(),
            api_format: api_format.to_string(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            agent_config,
        }
    }

    /// Generate a deterministic session key from (channel, user) pair.
    fn session_key(channel_id: &str, user_id: &str) -> String {
        let mut hasher = Md5::new();
        hasher.update(format!("nineclaw:{}:{}", channel_id, user_id).as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn session_file_path(key: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nineclaw-bot-session-{key}.jsonl"))
    }

    fn pi_runtime_dir() -> PathBuf {
        std::env::temp_dir().join("nineclaw-pi-runtime")
    }

    fn default_provider_base_url(provider_id: &str) -> Option<&'static str> {
        match provider_id {
            "openai" => Some("https://api.openai.com/v1"),
            "anthropic" => Some("https://api.anthropic.com"),
            "deepseek" => Some("https://api.deepseek.com"),
            "doubao" => Some("https://ark.cn-beijing.volces.com/api/v3"),
            "siliconflow" => Some("https://api.siliconflow.cn/v1"),
            _ => None,
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

        match (provider_id, api_format) {
            ("openai", "openai") => {
                let Some(default_base_url) = Self::default_provider_base_url(provider_id) else {
                    return None;
                };

                if base_url == Self::normalize_provider_base_url(default_base_url) {
                    return None;
                }

                let mut provider = serde_json::Map::new();
                provider.insert("baseUrl".to_string(), json!(base_url));
                provider.insert("api".to_string(), json!("openai-completions"));
                provider.insert(
                    "apiKey".to_string(),
                    json!(if self.api_key.trim().is_empty() {
                        "DUMMY_KEY"
                    } else {
                        self.api_key.trim()
                    }),
                );
                provider.insert(
                    "compat".to_string(),
                    json!({
                        "supportsDeveloperRole": false,
                        "supportsReasoningEffort": false
                    }),
                );
                provider.insert(
                    "models".to_string(),
                    json!([{ "id": model, "api": "openai-completions" }]),
                );
                let mut providers = serde_json::Map::new();
                providers.insert(provider_id.to_string(), serde_json::Value::Object(provider));
                Some(json!({ "providers": providers }))
            }
            ("anthropic", "anthropic") => {
                let Some(default_base_url) = Self::default_provider_base_url(provider_id) else {
                    return None;
                };

                if base_url == Self::normalize_anthropic_base_url(default_base_url) {
                    return None;
                }

                let mut provider = serde_json::Map::new();
                provider.insert("baseUrl".to_string(), json!(base_url));
                provider.insert("api".to_string(), json!("anthropic-messages"));
                provider.insert(
                    "apiKey".to_string(),
                    json!(if self.api_key.trim().is_empty() {
                        "DUMMY_KEY"
                    } else {
                        self.api_key.trim()
                    }),
                );
                provider.insert(
                    "models".to_string(),
                    json!([{ "id": model, "api": "anthropic-messages" }]),
                );
                let mut providers = serde_json::Map::new();
                providers.insert(provider_id.to_string(), serde_json::Value::Object(provider));
                Some(json!({ "providers": providers }))
            }
            _ => {
                let mut provider = serde_json::Map::new();
                provider.insert("baseUrl".to_string(), json!(base_url));
                match api_format {
                    "anthropic" => {
                        provider.insert("api".to_string(), json!("anthropic-messages"));
                        provider.insert(
                            "apiKey".to_string(),
                            json!(if self.api_key.trim().is_empty() {
                                "DUMMY_KEY"
                            } else {
                                self.api_key.trim()
                            }),
                        );
                        provider.insert(
                            "models".to_string(),
                            json!([{ "id": model, "api": "anthropic-messages" }]),
                        );
                    }
                    _ => {
                        provider.insert("api".to_string(), json!("openai-completions"));
                        provider.insert(
                            "apiKey".to_string(),
                            json!(if self.api_key.trim().is_empty() {
                                "DUMMY_KEY"
                            } else {
                                self.api_key.trim()
                            }),
                        );
                        provider.insert(
                            "compat".to_string(),
                            json!({
                                "supportsDeveloperRole": false,
                                "supportsReasoningEffort": false
                            }),
                        );
                        provider.insert(
                            "models".to_string(),
                            json!([{ "id": model, "api": "openai-completions" }]),
                        );
                    }
                }
                let mut providers = serde_json::Map::new();
                providers.insert(provider_id.to_string(), serde_json::Value::Object(provider));
                Some(json!({ "providers": providers }))
            }
        }
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

    /// Process an incoming message through pi, calling `on_chunk` every `chunk_size` characters.
    pub fn process_message<F>(
        &self,
        channel_id: &str,
        user_id: &str,
        prompt: &str,
        chunk_size: usize,
        mut on_chunk: F,
    ) -> Result<String, String>
    where
        F: FnMut(&str),
    {
        let key = Self::session_key(channel_id, user_id);
        let session_path = Self::session_file_path(&key);
        let runtime_dir = Self::prepare_runtime_dir()?;

        // Write models config if needed
        let models_path = runtime_dir.join("models.json");
        if let Some(config) = self.build_provider_models_config() {
            let content = serde_json::to_vec_pretty(&config)
                .map_err(|e| format!("序列化 models 配置失败: {e}"))?;
            fs::write(&models_path, content).map_err(|e| format!("写入 models.json 失败: {e}"))?;
        } else if models_path.exists() {
            fs::remove_file(&models_path).map_err(|e| format!("清理旧的 models.json 失败: {e}"))?;
        }

        let mut cmd = Command::new("pi");
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

        if !self.provider_id.is_empty() {
            cmd.args(["--provider", &self.provider_id]);
        }
        if !self.model.is_empty() {
            cmd.args(["--model", &self.model]);
        }
        if !self.api_key.is_empty() {
            cmd.args(["--api-key", &self.api_key]);
        }

        if let Some(agent_config) = self.agent_config.as_ref() {
            if let Some(system_prompt) = agents::build_agent_system_prompt(agent_config) {
                cmd.args(["--append-system-prompt", &system_prompt]);
            }

            for skill_path in skills::resolve_skill_directories(&agent_config.skill_ids)? {
                let skill_path = skill_path.to_string_lossy().to_string();
                cmd.args(["--skill", &skill_path]);
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("启动 pi 失败（请确认 pi 已安装且在 PATH 中）: {e}"))?;

        // Write prompt to stdin
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "无法获取 pi stdin".to_string())?;
        {
            let prompt_cmd = json!({
                "id": "prompt-1",
                "type": "prompt",
                "message": prompt,
            })
            .to_string();
            writeln!(stdin, "{}", prompt_cmd).map_err(|e| format!("写入 prompt 失败: {e}"))?;
            stdin
                .flush()
                .map_err(|e| format!("flush stdin 失败: {e}"))?;
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "无法获取 pi stdout".to_string())?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| "无法获取 pi stderr".to_string())?;

        let mut full_text = String::new();
        let mut chunk_buffer = String::new();
        let mut saw_agent_end = false;
        let mut saw_done = false;
        let mut saw_any_output = false;
        let mut saw_prompt_response = false;

        for line in BufReader::new(stdout).lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            if !line.trim().is_empty() {
                saw_any_output = true;
            }

            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
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
                    return Err(format!("pi 流错误: {reason}"));
                }
            }

            if line_type == "agent_end" {
                saw_agent_end = true;
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
                    return Err(format!("pi RPC 错误: {err}"));
                }
            }
        }

        // Flush remaining chunk
        if !chunk_buffer.is_empty() {
            on_chunk(&chunk_buffer);
        }

        drop(stdin);

        let exit_status = child.wait().map_err(|e| format!("等待 pi 退出失败: {e}"))?;

        let mut stderr_text = String::new();
        let _ = stderr.read_to_string(&mut stderr_text);

        // If pi exited without producing any assistant events, surface a more useful reason.
        if full_text.is_empty() && !saw_done && !saw_agent_end {
            let reason = if !stderr_text.trim().is_empty() {
                stderr_text.trim().to_string()
            } else if saw_prompt_response || saw_any_output {
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
            } else {
                format!("pi 退出码 {exit_status}，无输出内容")
            };
            return Err(reason);
        }

        if !exit_status.success() {
            let reason = if !stderr_text.trim().is_empty() {
                stderr_text.trim().to_string()
            } else {
                format!("pi 退出码异常: {exit_status}")
            };
            return Err(reason);
        }

        Ok(full_text)
    }
}
