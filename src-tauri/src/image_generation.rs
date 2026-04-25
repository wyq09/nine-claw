use crate::app_constants::{IMAGE_GENERATION_SYSTEM_STATE_KEY, IMAGE_PROVIDER_CONFIGS_STATE_KEY};
use crate::history_app_state::open_history_db;
use crate::provider_runtime::normalize_provider_base_url;
use base64::Engine as _;
use reqwest::header::CONTENT_TYPE;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tauri::AppHandle;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageGenerationPreferencesPayload {
    pub(crate) image_provider_configs: Option<String>,
    pub(crate) image_generation_system: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageGenerationRuntimeConfig {
    pub(crate) provider_id: String,
    pub(crate) adapter_type: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) size: String,
    pub(crate) resolution: String,
    pub(crate) background: String,
    pub(crate) output_format: String,
    pub(crate) quality: String,
    pub(crate) count: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredImageProviderRow {
    #[serde(default = "default_adapter_type")]
    adapter_type: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    model: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredImageSystemRow {
    #[serde(default)]
    default_provider_id: String,
    #[serde(default = "default_image_size")]
    size: String,
    #[serde(default = "default_resolution")]
    resolution: String,
    #[serde(default = "default_background")]
    background: String,
    #[serde(default = "default_output_format")]
    output_format: String,
    #[serde(default = "default_quality")]
    quality: String,
    #[serde(default = "default_count")]
    count: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageGenerateProxyRequest {
    pub(crate) prompt: String,
    #[serde(default)]
    pub(crate) size: Option<String>,
    #[serde(default)]
    pub(crate) resolution: Option<String>,
    #[serde(default)]
    pub(crate) background: Option<String>,
    #[serde(default)]
    pub(crate) output_format: Option<String>,
    #[serde(default)]
    pub(crate) quality: Option<String>,
    #[serde(default)]
    pub(crate) moderation: Option<String>,
    #[serde(default)]
    pub(crate) output_compression: Option<u32>,
    #[serde(default)]
    pub(crate) count: Option<u32>,
    #[serde(default)]
    pub(crate) negative_prompt: Option<String>,
    #[serde(default)]
    pub(crate) seed: Option<i64>,
    #[serde(default)]
    pub(crate) image_urls: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) mask_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageGenerateProxyResponse {
    pub(crate) provider_id: String,
    pub(crate) adapter_type: String,
    pub(crate) model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) revised_prompt: Option<String>,
    pub(crate) images: Vec<ImageArtifactPayload>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageArtifactPayload {
    pub(crate) mime_type: String,
    pub(crate) data_base64: String,
}

fn default_adapter_type() -> String {
    "openai_compatible".to_string()
}

fn default_image_size() -> String {
    "1:1".to_string()
}

fn default_background() -> String {
    "auto".to_string()
}

fn default_resolution() -> String {
    "1k".to_string()
}

fn default_output_format() -> String {
    "png".to_string()
}

fn default_quality() -> String {
    "auto".to_string()
}

fn default_count() -> u32 {
    1
}

fn normalize_image_system(config: StoredImageSystemRow) -> StoredImageSystemRow {
    let size = {
        let trimmed = config.size.trim();
        if trimmed.is_empty() {
            default_image_size()
        } else {
            trimmed.to_string()
        }
    };
    let resolution = match config.resolution.trim() {
        "2k" => "2k".to_string(),
        "4k" => "4k".to_string(),
        _ => default_resolution(),
    };
    let background = match config.background.trim() {
        "transparent" => "transparent".to_string(),
        "opaque" => "opaque".to_string(),
        _ => default_background(),
    };
    let output_format = match config.output_format.trim() {
        "jpeg" => "jpeg".to_string(),
        "webp" => "webp".to_string(),
        _ => default_output_format(),
    };
    let quality = match config.quality.trim() {
        "low" => "low".to_string(),
        "medium" => "medium".to_string(),
        "high" => "high".to_string(),
        _ => default_quality(),
    };
    let count = config.count.clamp(1, 4);
    StoredImageSystemRow {
        default_provider_id: config.default_provider_id.trim().to_string(),
        size,
        resolution,
        background,
        output_format,
        quality,
        count,
    }
}

fn normalize_image_provider(provider: StoredImageProviderRow) -> StoredImageProviderRow {
    let adapter_type = match provider.adapter_type.trim() {
        "openai_images" => "openai_images".to_string(),
        "apimart_gpt_image_2" => "apimart_gpt_image_2".to_string(),
        _ => default_adapter_type(),
    };
    StoredImageProviderRow {
        adapter_type,
        base_url: normalize_provider_base_url(&provider.base_url).to_string(),
        api_key: provider.api_key.trim().to_string(),
        model: provider.model.trim().to_string(),
    }
}

fn is_complete_image_provider(provider: &StoredImageProviderRow) -> bool {
    !provider.base_url.trim().is_empty()
        && !provider.api_key.trim().is_empty()
        && !provider.model.trim().is_empty()
}

fn resolved_provider_id<'a>(
    preferred: &'a str,
    providers: &'a HashMap<String, StoredImageProviderRow>,
) -> Option<&'a str> {
    let preferred = preferred.trim();
    if !preferred.is_empty() {
        if let Some(provider) = providers.get(preferred) {
            if is_complete_image_provider(provider) {
                return Some(preferred);
            }
        }
    }
    providers
        .iter()
        .find(|(_, provider)| is_complete_image_provider(provider))
        .map(|(provider_id, _)| provider_id.as_str())
}

#[tauri::command]
pub(crate) fn load_image_generation_preferences(
    app: AppHandle,
) -> Result<ImageGenerationPreferencesPayload, String> {
    let connection = open_history_db(&app)?;

    let image_provider_configs = connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![IMAGE_PROVIDER_CONFIGS_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取生图 Provider 配置失败: {error}"))?;

    let image_generation_system = connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![IMAGE_GENERATION_SYSTEM_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取生图默认配置失败: {error}"))?;

    Ok(ImageGenerationPreferencesPayload {
        image_provider_configs,
        image_generation_system,
    })
}

#[tauri::command]
pub(crate) fn save_image_generation_preferences(
    app: AppHandle,
    image_provider_configs_payload: String,
    image_generation_system_payload: String,
) -> Result<(), String> {
    let connection = open_history_db(&app)?;
    let updated_at = crate::time_util::chrono_like_timestamp();

    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![
                IMAGE_PROVIDER_CONFIGS_STATE_KEY,
                image_provider_configs_payload,
                updated_at
            ],
        )
        .map_err(|error| format!("保存生图 Provider 配置失败: {error}"))?;

    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![
                IMAGE_GENERATION_SYSTEM_STATE_KEY,
                image_generation_system_payload,
                updated_at
            ],
        )
        .map_err(|error| format!("保存生图默认配置失败: {error}"))?;

    Ok(())
}

pub(crate) fn resolve_default_image_generation_runtime(
    app: &AppHandle,
) -> Result<Option<ImageGenerationRuntimeConfig>, String> {
    let prefs = load_image_generation_preferences(app.clone())?;
    let providers_raw = prefs
        .image_provider_configs
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    if providers_raw.is_empty() {
        return Ok(None);
    }

    let providers_map_raw: HashMap<String, StoredImageProviderRow> =
        serde_json::from_str(&providers_raw)
            .map_err(|error| format!("解析生图 Provider 配置失败: {error}"))?;
    let providers = providers_map_raw
        .into_iter()
        .map(|(provider_id, provider)| (provider_id, normalize_image_provider(provider)))
        .collect::<HashMap<_, _>>();

    let system = prefs
        .image_generation_system
        .as_deref()
        .and_then(|raw| serde_json::from_str::<StoredImageSystemRow>(raw).ok())
        .map(normalize_image_system)
        .unwrap_or_else(|| {
            normalize_image_system(StoredImageSystemRow {
                default_provider_id: String::new(),
                size: default_image_size(),
                resolution: default_resolution(),
                background: default_background(),
                output_format: default_output_format(),
                quality: default_quality(),
                count: default_count(),
            })
        });

    let Some(provider_id) = resolved_provider_id(&system.default_provider_id, &providers) else {
        return Ok(None);
    };
    let provider = providers
        .get(provider_id)
        .ok_or_else(|| "未找到默认生图 Provider".to_string())?;

    Ok(Some(ImageGenerationRuntimeConfig {
        provider_id: provider_id.to_string(),
        adapter_type: provider.adapter_type.clone(),
        base_url: provider.base_url.clone(),
        api_key: provider.api_key.clone(),
        model: provider.model.clone(),
        size: system.size,
        resolution: system.resolution,
        background: system.background,
        output_format: system.output_format,
        quality: system.quality,
        count: system.count,
    }))
}

pub(crate) async fn dispatch_image_generation(
    client: &reqwest::Client,
    runtime: &ImageGenerationRuntimeConfig,
    payload: &ImageGenerateProxyRequest,
) -> Result<ImageGenerateProxyResponse, String> {
    let prompt = payload.prompt.trim();
    if prompt.is_empty() {
        return Err("图片描述不能为空".to_string());
    }

    match runtime.adapter_type.trim() {
        "openai_images" | "openai_compatible" => {
            dispatch_openai_images(client, runtime, payload).await
        }
        "apimart_gpt_image_2" => dispatch_apimart_gpt_image_2(client, runtime, payload).await,
        other => Err(format!("暂不支持的图片适配器: {other}")),
    }
}

async fn dispatch_openai_images(
    client: &reqwest::Client,
    runtime: &ImageGenerationRuntimeConfig,
    payload: &ImageGenerateProxyRequest,
) -> Result<ImageGenerateProxyResponse, String> {
    let count = payload.count.unwrap_or(runtime.count).clamp(1, 4);
    let size = payload
        .size
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.size.as_str());
    let background = payload
        .background
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.background.as_str());
    let resolution = payload
        .resolution
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.resolution.as_str());
    let output_format = payload
        .output_format
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.output_format.as_str());
    let quality = payload
        .quality
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.quality.as_str());

    let mut body = json!({
        "model": runtime.model,
        "prompt": payload.prompt.trim(),
        "n": count,
        "size": size,
        "response_format": "b64_json",
    });
    if let Some(map) = body.as_object_mut() {
        if !background.is_empty() {
            map.insert("background".to_string(), json!(background));
        }
        if !output_format.is_empty() {
            map.insert("output_format".to_string(), json!(output_format));
        }
        if !quality.is_empty() {
            map.insert("quality".to_string(), json!(quality));
        }
        if !resolution.is_empty() {
            map.insert("resolution".to_string(), json!(resolution));
        }
        if let Some(moderation) = payload
            .moderation
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            map.insert("moderation".to_string(), json!(moderation));
        }
        if let Some(output_compression) = payload.output_compression {
            map.insert(
                "output_compression".to_string(),
                json!(output_compression.clamp(0, 100)),
            );
        }
        if let Some(negative_prompt) = payload
            .negative_prompt
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            map.insert("negative_prompt".to_string(), json!(negative_prompt));
        }
        if let Some(seed) = payload.seed {
            map.insert("seed".to_string(), json!(seed));
        }
        if let Some(image_urls) = payload.image_urls.as_ref() {
            let filtered = image_urls
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .take(16)
                .map(|value| value.to_string())
                .collect::<Vec<_>>();
            if !filtered.is_empty() {
                map.insert("image_urls".to_string(), json!(filtered));
            }
        }
        if let Some(mask_url) = payload
            .mask_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            map.insert("mask_url".to_string(), json!(mask_url));
        }
    }

    let endpoint = format!(
        "{}/images/generations",
        runtime.base_url.trim_end_matches('/')
    );
    let response = client
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .bearer_auth(runtime.api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("生图请求失败({endpoint}): {error}"))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取生图响应失败: {error}"))?;
    if !status.is_success() {
        let preview: String = text.chars().take(280).collect();
        return Err(format!(
            "生图网关返回 HTTP {} — {}",
            status.as_u16(),
            preview
        ));
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("生图响应不是合法 JSON: {error}"))?;
    if let Some(error) = parsed.get("error") {
        return Err(format!("生图 API 返回错误: {error}"));
    }

    let revised_prompt = parsed
        .get("data")
        .and_then(|data| data.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("revised_prompt"))
        .and_then(|item| item.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let mut images = Vec::new();
    if let Some(items) = parsed.get("data").and_then(|data| data.as_array()) {
        for item in items {
            if let Some(base64_data) = item.get("b64_json").and_then(|value| value.as_str()) {
                let trimmed = base64_data.trim();
                if !trimmed.is_empty() {
                    images.push(ImageArtifactPayload {
                        mime_type: image_format_to_mime(output_format),
                        data_base64: trimmed.to_string(),
                    });
                    continue;
                }
            }
            if let Some(url) = item.get("url").and_then(|value| value.as_str()) {
                if let Some(downloaded) = download_remote_image(client, url).await? {
                    images.push(downloaded);
                }
            }
        }
    }

    if images.is_empty() {
        let preview: String = text.chars().take(240).collect();
        return Err(format!("生图响应中没有可用图片数据: {preview}"));
    }

    Ok(ImageGenerateProxyResponse {
        provider_id: runtime.provider_id.clone(),
        adapter_type: runtime.adapter_type.clone(),
        model: runtime.model.clone(),
        task_id: None,
        revised_prompt,
        images,
    })
}

async fn dispatch_apimart_gpt_image_2(
    client: &reqwest::Client,
    runtime: &ImageGenerationRuntimeConfig,
    payload: &ImageGenerateProxyRequest,
) -> Result<ImageGenerateProxyResponse, String> {
    let count = payload.count.unwrap_or(runtime.count).clamp(1, 4);
    let size = payload
        .size
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.size.as_str());
    let resolution = payload
        .resolution
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.resolution.as_str());
    let background = payload
        .background
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.background.as_str());
    let output_format = payload
        .output_format
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.output_format.as_str());
    let quality = payload
        .quality
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.quality.as_str());

    let mut body = json!({
        "model": runtime.model,
        "prompt": payload.prompt.trim(),
        "size": size,
        "resolution": resolution,
        "quality": quality,
        "background": background,
        "output_format": output_format,
        "n": count,
    });
    if let Some(map) = body.as_object_mut() {
        if let Some(moderation) = payload
            .moderation
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            map.insert("moderation".to_string(), json!(moderation));
        }
        if let Some(output_compression) = payload.output_compression {
            map.insert(
                "output_compression".to_string(),
                json!(output_compression.clamp(0, 100)),
            );
        }
        if let Some(negative_prompt) = payload
            .negative_prompt
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            map.insert("negative_prompt".to_string(), json!(negative_prompt));
        }
        if let Some(seed) = payload.seed {
            map.insert("seed".to_string(), json!(seed));
        }
        if let Some(image_urls) = payload.image_urls.as_ref() {
            let filtered = image_urls
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .take(16)
                .map(|value| value.to_string())
                .collect::<Vec<_>>();
            if !filtered.is_empty() {
                map.insert("image_urls".to_string(), json!(filtered));
            }
        }
        if let Some(mask_url) = payload
            .mask_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            map.insert("mask_url".to_string(), json!(mask_url));
        }
    }

    let endpoint = format!(
        "{}/images/generations",
        runtime.base_url.trim_end_matches('/')
    );
    let response = client
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .bearer_auth(runtime.api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("提交 APIMart 生图任务失败({endpoint}): {error}"))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取 APIMart 提交响应失败: {error}"))?;
    if !status.is_success() {
        let preview: String = text.chars().take(280).collect();
        return Err(format!(
            "APIMart 生图提交返回 HTTP {} — {}",
            status.as_u16(),
            preview
        ));
    }

    let task_id = extract_apimart_task_id(&text)?;
    poll_apimart_task(client, runtime, &task_id, output_format).await
}

async fn poll_apimart_task(
    client: &reqwest::Client,
    runtime: &ImageGenerationRuntimeConfig,
    task_id: &str,
    output_format: &str,
) -> Result<ImageGenerateProxyResponse, String> {
    const INITIAL_DELAY: Duration = Duration::from_secs(10);
    const POLL_INTERVAL: Duration = Duration::from_secs(4);
    const MAX_ATTEMPTS: usize = 45;

    tokio::time::sleep(INITIAL_DELAY).await;

    let endpoint = format!(
        "{}/tasks/{}?language=zh",
        runtime.base_url.trim_end_matches('/'),
        task_id
    );

    for _ in 0..MAX_ATTEMPTS {
        let response = client
            .get(&endpoint)
            .bearer_auth(runtime.api_key.trim())
            .send()
            .await
            .map_err(|error| format!("查询 APIMart 任务失败({endpoint}): {error}"))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| format!("读取 APIMart 任务响应失败: {error}"))?;
        if !status.is_success() {
            let preview: String = text.chars().take(280).collect();
            return Err(format!(
                "APIMart 任务查询返回 HTTP {} — {}",
                status.as_u16(),
                preview
            ));
        }

        let parsed: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("APIMart 任务响应不是合法 JSON: {error}"))?;
        let task_data = parsed
            .get("data")
            .ok_or_else(|| "APIMart 任务响应缺少 data 字段".to_string())?;
        let task_status = task_data
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();

        match task_status {
            "completed" => {
                let images = extract_apimart_images(client, task_data).await?;
                if images.is_empty() {
                    return Err("APIMart 任务完成，但结果中没有可用图片".to_string());
                }
                return Ok(ImageGenerateProxyResponse {
                    provider_id: runtime.provider_id.clone(),
                    adapter_type: runtime.adapter_type.clone(),
                    model: runtime.model.clone(),
                    task_id: Some(task_id.to_string()),
                    revised_prompt: None,
                    images: images
                        .into_iter()
                        .map(|image| ImageArtifactPayload {
                            mime_type: if image.mime_type.trim().is_empty() {
                                image_format_to_mime(output_format)
                            } else {
                                image.mime_type
                            },
                            data_base64: image.data_base64,
                        })
                        .collect(),
                });
            }
            "failed" => {
                let message = task_data
                    .get("error")
                    .or_else(|| task_data.get("message"))
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "APIMart 任务执行失败".to_string());
                return Err(format!("APIMart 生图任务失败: {message}"));
            }
            "cancelled" => {
                return Err("APIMart 生图任务已取消".to_string());
            }
            "pending" | "processing" | "submitted" | "in_progress" => {
                tokio::time::sleep(POLL_INTERVAL).await;
            }
            other => {
                tokio::time::sleep(POLL_INTERVAL).await;
                if !other.is_empty() {
                    continue;
                }
            }
        }
    }

    Err(format!(
        "APIMart 生图任务轮询超时: {task_id}，请稍后重试或降低 quality / resolution"
    ))
}

fn extract_apimart_task_id(text: &str) -> Result<String, String> {
    let parsed: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("APIMart 提交响应不是合法 JSON: {error}"))?;
    if let Some(error) = parsed.get("error") {
        return Err(format!("APIMart 生图 API 返回错误: {error}"));
    }
    parsed
        .get("data")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("task_id"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "APIMart 提交响应缺少 task_id: {}",
                text.chars().take(240).collect::<String>()
            )
        })
}

async fn extract_apimart_images(
    client: &reqwest::Client,
    task_data: &serde_json::Value,
) -> Result<Vec<ImageArtifactPayload>, String> {
    let mut images = Vec::new();
    if let Some(entries) = task_data
        .get("result")
        .and_then(|value| value.get("images"))
        .and_then(|value| value.as_array())
    {
        for entry in entries {
            if let Some(urls) = entry.get("url").and_then(|value| value.as_array()) {
                for url in urls.iter().filter_map(|value| value.as_str()) {
                    if let Some(downloaded) = download_remote_image(client, url).await? {
                        images.push(downloaded);
                    }
                }
            }
        }
    }
    Ok(images)
}

async fn download_remote_image(
    client: &reqwest::Client,
    url: &str,
) -> Result<Option<ImageArtifactPayload>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("下载远程图片失败({url}): {error}"))?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let mime_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or("image/png")
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| "image/png".to_string());
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("读取远程图片内容失败: {error}"))?;
    Ok(Some(ImageArtifactPayload {
        mime_type,
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    }))
}

fn image_format_to_mime(format: &str) -> String {
    match format {
        "jpeg" => "image/jpeg".to_string(),
        "webp" => "image/webp".to_string(),
        _ => "image/png".to_string(),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageTaskQueryResponse {
    pub(crate) task_id: String,
    pub(crate) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) progress: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) images: Option<Vec<ImageArtifactPayload>>,
}

pub(crate) async fn dispatch_image_task_query(
    client: &reqwest::Client,
    runtime: &ImageGenerationRuntimeConfig,
    task_id: &str,
) -> Result<ImageTaskQueryResponse, String> {
    let endpoint = format!(
        "{}/tasks/{}?language=zh",
        runtime.base_url.trim_end_matches('/'),
        task_id
    );
    let response = client
        .get(&endpoint)
        .bearer_auth(runtime.api_key.trim())
        .send()
        .await
        .map_err(|error| format!("查询图片任务失败({endpoint}): {error}"))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取任务查询响应失败: {error}"))?;
    if !status.is_success() {
        let preview: String = text.chars().take(280).collect();
        return Err(format!(
            "任务查询返回 HTTP {} — {}",
            status.as_u16(),
            preview
        ));
    }

    let parsed: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("任务查询响应不是合法 JSON: {error}"))?;
    let task_data = parsed
        .get("data")
        .ok_or_else(|| "任务查询响应缺少 data 字段".to_string())?;
    let task_status = task_data
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let progress = task_data
        .get("progress")
        .and_then(|value| value.as_u64())
        .map(|v| v as u32);

    match task_status.as_str() {
        "completed" => {
            let images = extract_apimart_images(client, task_data).await?;
            Ok(ImageTaskQueryResponse {
                task_id: task_id.to_string(),
                status: "completed".to_string(),
                progress: Some(100),
                error: None,
                images: if images.is_empty() {
                    None
                } else {
                    Some(images)
                },
            })
        }
        "failed" => {
            let message = task_data
                .get("error")
                .and_then(|value| value.get("message"))
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
                .or_else(|| {
                    task_data
                        .get("message")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                })
                .unwrap_or_else(|| "任务执行失败".to_string());
            Ok(ImageTaskQueryResponse {
                task_id: task_id.to_string(),
                status: "failed".to_string(),
                progress,
                error: Some(message),
                images: None,
            })
        }
        "cancelled" => Ok(ImageTaskQueryResponse {
            task_id: task_id.to_string(),
            status: "cancelled".to_string(),
            progress,
            error: Some("任务已取消".to_string()),
            images: None,
        }),
        _ => Ok(ImageTaskQueryResponse {
            task_id: task_id.to_string(),
            status: task_status,
            progress,
            error: None,
            images: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_default_runtime_and_falls_back_to_first_complete_provider() {
        let providers = serde_json::json!({
            "openai_image": {
                "adapterType": "openai_images",
                "baseUrl": "https://api.openai.com/v1",
                "apiKey": "",
                "model": "gpt-image-1"
            },
            "custom_image": {
                "adapterType": "openai_compatible",
                "baseUrl": "https://example.com/v1",
                "apiKey": "secret",
                "model": "flux-dev"
            }
        });
        let system = serde_json::json!({
            "defaultProviderId": "openai_image",
            "count": 9,
            "resolution": "4k"
        });

        let providers_map: HashMap<String, StoredImageProviderRow> =
            serde_json::from_value(providers).expect("providers");
        let providers_map = providers_map
            .into_iter()
            .map(|(provider_id, provider)| (provider_id, normalize_image_provider(provider)))
            .collect::<HashMap<_, _>>();
        let system: StoredImageSystemRow = serde_json::from_value(system).expect("system");
        let system = normalize_image_system(system);

        let provider_id =
            resolved_provider_id(&system.default_provider_id, &providers_map).expect("provider id");
        assert_eq!(provider_id, "custom_image");
        assert_eq!(system.count, 4);
        assert_eq!(system.resolution, "4k");
    }

    #[test]
    fn normalizes_image_formats_and_background_defaults() {
        let normalized = normalize_image_system(StoredImageSystemRow {
            default_provider_id: "custom_image".to_string(),
            size: "".to_string(),
            resolution: "invalid".to_string(),
            background: "invalid".to_string(),
            output_format: "jpeg".to_string(),
            quality: "high".to_string(),
            count: 0,
        });

        assert_eq!(normalized.size, "1:1");
        assert_eq!(normalized.resolution, "1k");
        assert_eq!(normalized.background, "auto");
        assert_eq!(normalized.output_format, "jpeg");
        assert_eq!(normalized.quality, "high");
        assert_eq!(normalized.count, 1);
    }

    #[test]
    fn extracts_apimart_task_id_from_submit_response() {
        let task_id = extract_apimart_task_id(
            r#"{"code":200,"data":[{"status":"submitted","task_id":"task_123"}]}"#,
        )
        .expect("task id");

        assert_eq!(task_id, "task_123");
    }
}
