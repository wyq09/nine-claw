use crate::app_constants::{
    CUSTOM_PROVIDER_META_STATE_KEY, PI_RUNTIME_DIR_NAME, PROVIDER_CONFIGS_STATE_KEY,
};
use crate::history_app_state::open_history_db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::AppHandle;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderRuntimeConfig {
    pub(crate) provider_id: String,
    pub(crate) api_format: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProviderPrefsRow {
    #[serde(default)]
    added: bool,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    api_format: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderPreferencesPayload {
    pub(crate) provider_configs: Option<String>,
    pub(crate) custom_provider_meta: Option<String>,
}

pub(crate) fn pi_runtime_dir() -> PathBuf {
    std::env::temp_dir().join(PI_RUNTIME_DIR_NAME)
}

fn default_provider_api_format(provider_id: &str) -> &'static str {
    match provider_id {
        "anthropic" => "anthropic",
        _ => "openai",
    }
}

pub(crate) fn normalize_provider_base_url(value: &str) -> &str {
    value.trim().trim_end_matches('/')
}

pub(crate) fn normalize_anthropic_base_url(value: &str) -> String {
    normalize_provider_base_url(value)
        .trim_end_matches("/v1/messages")
        .trim_end_matches("/messages")
        .trim_end_matches("/v1")
        .trim_end_matches('/')
        .to_string()
}

pub(crate) fn anthropic_messages_url(base_url: &str) -> String {
    if base_url.ends_with("/v1") {
        format!("{base_url}/messages")
    } else {
        format!("{base_url}/v1/messages")
    }
}

pub(crate) fn normalize_provider_api_format(value: &str, provider_id: &str) -> &'static str {
    match value.trim() {
        "anthropic" => "anthropic",
        "openai" => "openai",
        _ => default_provider_api_format(provider_id),
    }
}

/// `pi-ai` OpenAI-compat：推理类模型需开启 `supportsReasoningEffort`，否则部分网关/模型组合下 RPC 可能无 stdout 事件。
pub(crate) fn openai_pi_compat_supports_reasoning_effort(model: &str) -> bool {
    let m = model.trim().to_ascii_lowercase();
    m.contains("gpt-5")
        || m.contains("reasoning")
        || m.contains("thinking")
        || m.contains("-think")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
        || m.contains("glm")
        || m.contains("deepseek-r1")
        || m.contains("deepseek-reasoner")
        || m.contains("kimi")
        || m.contains("moonshot")
}

pub(crate) fn openai_pi_compat_requires_explicit_thinking_disable(model: &str) -> bool {
    let m = model.trim().to_ascii_lowercase();
    m.contains("deepseek-v4")
        || m.contains("deepseek_v4")
        || m.contains("deepseek v4")
        || m.contains("deepseek-v4-pro")
        || m.contains("deepseek_v4_pro")
}

pub(crate) fn should_force_pi_thinking_off(
    provider_config: &ProviderRuntimeConfig,
    disable_reasoning_effort: bool,
) -> bool {
    disable_reasoning_effort
        || openai_pi_compat_requires_explicit_thinking_disable(&provider_config.model)
}

/// 微信/飞书 IM 必须使用绑定智能体的默认模型；Base URL / API Key 从应用全局 Provider 配置读取。
pub(crate) fn resolve_im_llm_runtime(
    app: &AppHandle,
    default_provider_id: &str,
    default_model: &str,
) -> Result<ProviderRuntimeConfig, String> {
    let provider_id = default_provider_id.trim().to_string();
    let model = default_model.trim().to_string();
    if provider_id.is_empty() || model.is_empty() {
        return Err("智能体未配置默认 Provider 或模型".to_string());
    }

    let prefs = load_provider_preferences(app.clone())?;
    let raw = prefs
        .provider_configs
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    if raw.is_empty() {
        return Err("未找到全局 Provider 配置".to_string());
    }

    let map: HashMap<String, StoredProviderPrefsRow> =
        serde_json::from_str(&raw).map_err(|e| format!("解析 Provider 配置失败: {e}"))?;

    let row = map
        .get(provider_id.as_str())
        .ok_or_else(|| format!("全局设置中未找到 Provider「{provider_id}」"))?;

    if !row.added {
        return Err(format!("请先在设置中添加 Provider「{provider_id}」"));
    }

    let base_url = row.base_url.trim().to_string();
    let api_key = row.api_key.trim().to_string();
    if base_url.is_empty() || api_key.is_empty() {
        return Err(format!(
            "Provider「{provider_id}」的 Base URL 或 API Key 未填写完整"
        ));
    }

    let api_format_raw = row.api_format.trim();
    let api_format = if api_format_raw.is_empty() {
        default_provider_api_format(&provider_id).to_string()
    } else {
        normalize_provider_api_format(api_format_raw, &provider_id).to_string()
    };

    Ok(ProviderRuntimeConfig {
        provider_id,
        api_format,
        base_url,
        api_key,
        model,
    })
}

pub(crate) fn normalized_provider_runtime_base_url(
    base_url: &str,
    api_format: &str,
    provider_id: &str,
) -> String {
    match normalize_provider_api_format(api_format, provider_id) {
        "anthropic" => normalize_anthropic_base_url(base_url),
        _ => normalize_provider_base_url(base_url).to_string(),
    }
}

#[tauri::command]
pub(crate) fn load_provider_preferences(
    app: AppHandle,
) -> Result<ProviderPreferencesPayload, String> {
    let connection = open_history_db(&app)?;

    let provider_configs = connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![PROVIDER_CONFIGS_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取 Provider 配置失败: {error}"))?;

    let custom_provider_meta = connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![CUSTOM_PROVIDER_META_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取自定义 Provider 元数据失败: {error}"))?;

    Ok(ProviderPreferencesPayload {
        provider_configs,
        custom_provider_meta,
    })
}

#[tauri::command]
pub(crate) fn save_provider_preferences(
    app: AppHandle,
    provider_configs_payload: String,
    custom_provider_meta_payload: String,
) -> Result<(), String> {
    let connection = open_history_db(&app)?;
    let updated_at = crate::time_util::chrono_like_timestamp();

    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![
                PROVIDER_CONFIGS_STATE_KEY,
                provider_configs_payload,
                updated_at
            ],
        )
        .map_err(|error| format!("保存 Provider 配置失败: {error}"))?;

    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![
                CUSTOM_PROVIDER_META_STATE_KEY,
                custom_provider_meta_payload,
                updated_at
            ],
        )
        .map_err(|error| format!("保存自定义 Provider 元数据失败: {error}"))?;

    Ok(())
}
