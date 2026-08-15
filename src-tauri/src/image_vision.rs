//! 系统级识图（Vision）网关。
//!
//! 让纯文本主模型通过 `image_analyze` Tool 借用一个支持视觉的模型来理解
//! 用户发来的图片 / 视频截图 / 扫描 PDF：Tool 只传路径，后端负责读取、
//! 抽帧并调用系统默认识图模型，把识别文本回传给主模型继续推理。

use crate::app_constants::IMAGE_VISION_SYSTEM_STATE_KEY;
use crate::history_app_state::open_history_db;
use crate::prompt_attachments::{extract_video_frames, PromptImageInput};
use crate::provider_runtime::{
    anthropic_messages_url, normalize_anthropic_base_url, normalize_provider_base_url,
    normalize_provider_api_format,
};
use base64::engine::general_purpose::STANDARD as BASE64_ENGINE;
use base64::Engine as _;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tauri::AppHandle;

const MAX_INPUT_SOURCES: usize = 6;
const MAX_DERIVED_IMAGES: usize = 12;
const MAX_LOCAL_FILE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_DOWNLOAD_BYTES: usize = 25 * 1024 * 1024;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 2048;
const VISION_REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
pub(crate) const DEFAULT_VISION_PROMPT: &str = "请详细描述画面内容，重点包括：图中出现的文字（尽量逐字转述）、数字、界面元素、人物、物体、场景和图表数据等可能帮助回答用户的细节。";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageVisionSystemConfig {
    #[serde(default)]
    pub(crate) api_format: String,
    #[serde(default)]
    pub(crate) base_url: String,
    #[serde(default)]
    pub(crate) api_key: String,
    #[serde(default)]
    pub(crate) model: String,
    #[serde(default = "default_max_output_tokens")]
    pub(crate) max_output_tokens: u32,
    #[serde(default)]
    pub(crate) default_prompt: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ImageVisionRuntimeConfig {
    pub(crate) api_format: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) max_output_tokens: u32,
    pub(crate) default_prompt: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageVisionSource {
    #[serde(default)]
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageVisionAnalyzeRequest {
    #[serde(default)]
    pub(crate) images: Vec<ImageVisionSource>,
    #[serde(default)]
    pub(crate) prompt: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageVisionAnalyzeResponse {
    pub(crate) text: String,
    pub(crate) model: String,
    pub(crate) api_format: String,
    pub(crate) image_count: usize,
    pub(crate) notes: Vec<String>,
}

fn default_max_output_tokens() -> u32 {
    DEFAULT_MAX_OUTPUT_TOKENS
}

pub(crate) fn normalize_vision_config(config: &ImageVisionSystemConfig) -> ImageVisionSystemConfig {
    let api_format = normalize_provider_api_format(&config.api_format, "").to_string();
    let base_url = match api_format.as_str() {
        "anthropic" => normalize_anthropic_base_url(&config.base_url),
        _ => normalize_provider_base_url(&config.base_url).to_string(),
    };
    let default_prompt = config.default_prompt.trim().to_string();
    ImageVisionSystemConfig {
        api_format,
        base_url,
        api_key: config.api_key.trim().to_string(),
        model: config.model.trim().to_string(),
        max_output_tokens: config.max_output_tokens.clamp(256, 16384),
        default_prompt: if default_prompt.is_empty() {
            DEFAULT_VISION_PROMPT.to_string()
        } else {
            default_prompt
        },
    }
}

pub(crate) fn is_complete_vision_config(config: &ImageVisionSystemConfig) -> bool {
    !config.base_url.trim().is_empty()
        && !config.api_key.trim().is_empty()
        && !config.model.trim().is_empty()
}

fn parse_stored_vision_config(raw: Option<&str>) -> Option<ImageVisionSystemConfig> {
    raw.and_then(|value| serde_json::from_str::<ImageVisionSystemConfig>(value).ok())
        .map(|config| normalize_vision_config(&config))
}

#[tauri::command]
pub(crate) fn load_image_vision_preferences(app: AppHandle) -> Result<Option<String>, String> {
    let connection = open_history_db(&app)?;
    connection
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![IMAGE_VISION_SYSTEM_STATE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取识图模型配置失败: {error}"))
}

#[tauri::command]
pub(crate) fn save_image_vision_preferences(
    app: AppHandle,
    image_vision_system_payload: String,
) -> Result<(), String> {
    let normalized = parse_stored_vision_config(Some(image_vision_system_payload.as_str()))
        .ok_or("识图模型配置格式不合法")?;
    let payload = serde_json::to_string(&normalized)
        .map_err(|error| format!("序列化识图模型配置失败: {error}"))?;
    let connection = open_history_db(&app)?;
    let updated_at = crate::time_util::chrono_like_timestamp();
    connection
        .execute(
            "INSERT INTO app_state (key, value, updated_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![IMAGE_VISION_SYSTEM_STATE_KEY, payload, updated_at],
        )
        .map_err(|error| format!("保存识图模型配置失败: {error}"))?;
    Ok(())
}

pub(crate) fn resolve_default_image_vision_runtime(
    app: &AppHandle,
) -> Result<Option<ImageVisionRuntimeConfig>, String> {
    let raw = load_image_vision_preferences(app.clone())?;
    let Some(config) = parse_stored_vision_config(raw.as_deref()) else {
        return Ok(None);
    };
    if !is_complete_vision_config(&config) {
        return Ok(None);
    }
    Ok(Some(ImageVisionRuntimeConfig {
        api_format: config.api_format,
        base_url: config.base_url,
        api_key: config.api_key,
        model: config.model,
        max_output_tokens: config.max_output_tokens,
        default_prompt: if config.default_prompt.trim().is_empty() {
            DEFAULT_VISION_PROMPT.to_string()
        } else {
            config.default_prompt
        },
    }))
}

// ---------------------------------------------------------------------------
// 上游请求构造（纯函数，便于单测）
// ---------------------------------------------------------------------------

pub(crate) fn build_openai_vision_body(
    model: &str,
    max_output_tokens: u32,
    prompt: &str,
    images: &[PromptImageInput],
) -> Value {
    let mut content = vec![json!({ "type": "text", "text": prompt })];
    for image in images {
        content.push(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:{};base64,{}", image.mime_type, image.data) },
        }));
    }
    json!({
        "model": model,
        "max_tokens": max_output_tokens,
        "messages": [{ "role": "user", "content": content }],
    })
}

pub(crate) fn build_anthropic_vision_body(
    model: &str,
    max_output_tokens: u32,
    prompt: &str,
    images: &[PromptImageInput],
) -> Value {
    let mut content = Vec::new();
    for image in images {
        content.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": image.mime_type,
                "data": image.data,
            },
        }));
    }
    content.push(json!({ "type": "text", "text": prompt }));
    json!({
        "model": model,
        "max_tokens": max_output_tokens,
        "messages": [{ "role": "user", "content": content }],
    })
}

pub(crate) fn extract_openai_vision_text(body: &Value) -> String {
    let content = body
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"));
    match content {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
        _ => String::new(),
    }
}

pub(crate) fn extract_anthropic_vision_text(body: &Value) -> String {
    body.get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 媒体收集：本地路径 / URL → PromptImageInput 列表
// ---------------------------------------------------------------------------

fn collect_local_source(path: &str, images: &mut Vec<PromptImageInput>, notes: &mut Vec<String>) {
    let resolved = Path::new(path);
    let metadata = match fs::metadata(resolved) {
        Ok(metadata) => metadata,
        Err(error) => {
            notes.push(format!("无法读取文件 {path}: {error}"));
            return;
        }
    };
    if !metadata.is_file() {
        notes.push(format!("{path} 不是常规文件，已跳过"));
        return;
    }
    if metadata.len() > MAX_LOCAL_FILE_BYTES {
        notes.push(format!("{path} 超过 25MB，已跳过"));
        return;
    }

    let mime = crate::infer_media_mime_type(resolved, None);
    if mime.starts_with("image/") {
        match fs::read(resolved) {
            Ok(bytes) => images.push(PromptImageInput {
                content_type: "image".to_string(),
                data: BASE64_ENGINE.encode(bytes),
                mime_type: mime,
            }),
            Err(error) => notes.push(format!("读取图片 {path} 失败: {error}")),
        }
        return;
    }
    if mime.starts_with("video/") {
        let frame_count = match extract_video_frames(resolved, images) {
            Ok(count) => count,
            Err(error) => {
                notes.push(format!("视频抽帧失败 {path}: {error}"));
                0
            }
        };
        if frame_count == 0 {
            notes.push(format!("{path} 未能提取任何视频帧（本机可能缺少 ffmpeg）"));
        } else {
            notes.push(format!("已从视频 {path} 提取 {frame_count} 帧关键画面"));
        }
        return;
    }
    notes.push(format!("{path} 不是图片或视频文件（{mime}），已跳过"));
}

async fn collect_remote_source(
    client: &reqwest::Client,
    url: &str,
    images: &mut Vec<PromptImageInput>,
    notes: &mut Vec<String>,
) {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        notes.push(format!("仅支持 http/https 链接，已跳过 {trimmed}"));
        return;
    }
    let response = match client.get(trimmed).timeout(VISION_REQUEST_TIMEOUT).send().await {
        Ok(response) => response,
        Err(error) => {
            notes.push(format!("下载图片失败 {trimmed}: {error}"));
            return;
        }
    };
    if !response.status().is_success() {
        notes.push(format!("下载图片失败 {trimmed}: HTTP {}", response.status()));
        return;
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            notes.push(format!("读取图片内容失败 {trimmed}: {error}"));
            return;
        }
    };
    if bytes.len() > MAX_DOWNLOAD_BYTES {
        notes.push(format!("{trimmed} 超过 25MB，已跳过"));
        return;
    }
    let mime = if content_type.starts_with("image/") {
        content_type.clone()
    } else {
        sniff_image_mime_from_bytes(&bytes).unwrap_or(content_type)
    };
    if !mime.starts_with("image/") {
        notes.push(format!("{trimmed} 不是图片（{mime}），已跳过"));
        return;
    }
    images.push(PromptImageInput {
        content_type: "image".to_string(),
        data: BASE64_ENGINE.encode(bytes.as_ref()),
        mime_type: mime,
    });
}

fn sniff_image_mime_from_bytes(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png".to_string())
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg".to_string())
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp".to_string())
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif".to_string())
    } else {
        None
    }
}

pub(crate) async fn dispatch_image_vision(
    client: &reqwest::Client,
    runtime: &ImageVisionRuntimeConfig,
    payload: &ImageVisionAnalyzeRequest,
) -> Result<ImageVisionAnalyzeResponse, String> {
    if payload.images.is_empty() {
        return Err("images 不能为空：请传入要识别的图片路径或 URL".to_string());
    }
    let sources: Vec<ImageVisionSource> = payload
        .images
        .iter()
        .take(MAX_INPUT_SOURCES)
        .cloned()
        .collect();

    let mut images: Vec<PromptImageInput> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for source in &sources {
        if images.len() >= MAX_DERIVED_IMAGES {
            notes.push("图片数量已达上限，其余输入已跳过".to_string());
            break;
        }
        if let Some(path) = source.path.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            collect_local_source(path, &mut images, &mut notes);
        } else if let Some(url) = source.url.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            collect_remote_source(client, url, &mut images, &mut notes).await;
        }
    }
    if images.is_empty() {
        let detail = if notes.is_empty() {
            "未提供有效的图片路径或 URL".to_string()
        } else {
            notes.join("；")
        };
        return Err(format!("没有可识别的图片：{detail}"));
    }
    images.truncate(MAX_DERIVED_IMAGES);

    let prompt = payload
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(runtime.default_prompt.trim())
        .to_string();
    let prompt = if prompt.is_empty() {
        DEFAULT_VISION_PROMPT.to_string()
    } else {
        prompt
    };

    let is_anthropic = runtime.api_format == "anthropic";
    let (url, body) = if is_anthropic {
        (
            anthropic_messages_url(&runtime.base_url),
            build_anthropic_vision_body(
                &runtime.model,
                runtime.max_output_tokens,
                &prompt,
                &images,
            ),
        )
    } else {
        (
            format!("{}/chat/completions", runtime.base_url.trim_end_matches('/')),
            build_openai_vision_body(
                &runtime.model,
                runtime.max_output_tokens,
                &prompt,
                &images,
            ),
        )
    };

    let mut request = client.post(&url).timeout(VISION_REQUEST_TIMEOUT).json(&body);
    if is_anthropic {
        request = request
            .header("x-api-key", runtime.api_key.as_str())
            .header("anthropic-version", "2023-06-01");
    } else {
        request = request.bearer_auth(runtime.api_key.as_str());
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("识图模型请求失败: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取识图模型响应失败: {error}"))?;
    if !status.is_success() {
        return Err(format!("识图模型返回错误（HTTP {status}）: {text}"));
    }
    let parsed: Value = serde_json::from_str(&text)
        .map_err(|error| format!("解析识图模型响应失败: {error}"))?;
    let vision_text = if is_anthropic {
        extract_anthropic_vision_text(&parsed)
    } else {
        extract_openai_vision_text(&parsed)
    };
    if vision_text.trim().is_empty() {
        return Err(format!("识图模型没有返回文本内容: {text}"));
    }

    Ok(ImageVisionAnalyzeResponse {
        text: vision_text,
        model: runtime.model.clone(),
        api_format: runtime.api_format.clone(),
        image_count: images.len(),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_image() -> PromptImageInput {
        PromptImageInput {
            content_type: "image".to_string(),
            data: "aGVsbG8=".to_string(),
            mime_type: "image/png".to_string(),
        }
    }

    #[test]
    fn normalizes_vision_config_defaults_and_formats() {
        let config = ImageVisionSystemConfig {
            api_format: "anthropic".to_string(),
            base_url: "https://api.anthropic.com/v1/messages/".to_string(),
            api_key: "  key  ".to_string(),
            model: " claude-sonnet-4-5 ".to_string(),
            max_output_tokens: 999_999,
            default_prompt: "".to_string(),
        };
        let normalized = normalize_vision_config(&config);
        assert_eq!(normalized.api_format, "anthropic");
        assert_eq!(normalized.base_url, "https://api.anthropic.com");
        assert_eq!(normalized.api_key, "key");
        assert_eq!(normalized.model, "claude-sonnet-4-5");
        assert_eq!(normalized.max_output_tokens, 16384);
        assert_eq!(normalized.default_prompt, DEFAULT_VISION_PROMPT);

        let openai_like = ImageVisionSystemConfig {
            api_format: "openai".to_string(),
            base_url: "https://api.example.com/v1/".to_string(),
            api_key: "k".to_string(),
            model: "gpt-4o-mini".to_string(),
            max_output_tokens: 1,
            default_prompt: "自定义提问".to_string(),
        };
        let normalized = normalize_vision_config(&openai_like);
        assert_eq!(normalized.base_url, "https://api.example.com/v1");
        assert_eq!(normalized.max_output_tokens, 256);
        assert_eq!(normalized.default_prompt, "自定义提问");
    }

    #[test]
    fn vision_config_completeness_requires_all_fields() {
        let mut config = ImageVisionSystemConfig {
            api_format: "openai".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            api_key: "k".to_string(),
            model: "vision-model".to_string(),
            max_output_tokens: 1024,
            default_prompt: String::new(),
        };
        assert!(is_complete_vision_config(&config));
        config.model = "  ".to_string();
        assert!(!is_complete_vision_config(&config));
    }

    #[test]
    fn builds_openai_vision_body_with_data_urls() {
        let body = build_openai_vision_body("m1", 512, "图中有什么", &[sample_image()]);
        assert_eq!(body["model"], "m1");
        assert_eq!(body["max_tokens"], 512);
        let message = &body["messages"][0];
        assert_eq!(message["role"], "user");
        assert_eq!(message["content"][0]["text"], "图中有什么");
        assert_eq!(
            message["content"][1]["image_url"]["url"],
            "data:image/png;base64,aGVsbG8="
        );
    }

    #[test]
    fn builds_anthropic_vision_body_with_base64_source() {
        let body = build_anthropic_vision_body("m2", 256, "描述", &[sample_image()]);
        assert_eq!(body["model"], "m2");
        assert_eq!(body["max_tokens"], 256);
        let content = &body["messages"][0]["content"];
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert_eq!(content[0]["source"]["data"], "aGVsbG8=");
        assert_eq!(content[1]["type"], "text");
    }

    #[test]
    fn extracts_vision_text_from_both_formats() {
        let openai = json!({"choices": [{"message": {"content": " 一只猫 "}}]});
        assert_eq!(extract_openai_vision_text(&openai), "一只猫");
        let openai_parts = json!({"choices": [{"message": {"content": [
            {"type": "text", "text": "第一行"},
            {"type": "text", "text": "第二行"},
        ]}}]});
        assert_eq!(extract_openai_vision_text(&openai_parts), "第一行\n第二行");
        assert_eq!(extract_openai_vision_text(&json!({})), "");

        let anthropic = json!({"content": [
            {"type": "text", "text": "画面里有两个人"},
        ]});
        assert_eq!(extract_anthropic_vision_text(&anthropic), "画面里有两个人");
        assert_eq!(extract_anthropic_vision_text(&json!({})), "");
    }

    #[test]
    fn sniffs_image_mime_from_magic_bytes() {
        assert_eq!(
            sniff_image_mime_from_bytes(b"\x89PNG\r\n\x1a\n....").as_deref(),
            Some("image/png")
        );
        assert_eq!(
            sniff_image_mime_from_bytes(b"\xff\xd8\xff\xe0....").as_deref(),
            Some("image/jpeg")
        );
        assert_eq!(sniff_image_mime_from_bytes(b"not-image").as_deref(), None);
    }

    #[tokio::test]
    async fn dispatch_rejects_empty_or_unresolvable_inputs() {
        let runtime = ImageVisionRuntimeConfig {
            api_format: "openai".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            api_key: "k".to_string(),
            model: "m".to_string(),
            max_output_tokens: 512,
            default_prompt: DEFAULT_VISION_PROMPT.to_string(),
        };
        let client = reqwest::Client::new();
        let empty = ImageVisionAnalyzeRequest {
            images: vec![],
            prompt: None,
        };
        let error = dispatch_image_vision(&client, &runtime, &empty)
            .await
            .unwrap_err();
        assert!(error.contains("images 不能为空"));

        let missing_file = ImageVisionAnalyzeRequest {
            images: vec![ImageVisionSource {
                path: Some("/definitely/not/existing.png".to_string()),
                url: None,
            }],
            prompt: None,
        };
        let error = dispatch_image_vision(&client, &runtime, &missing_file)
            .await
            .unwrap_err();
        assert!(error.contains("没有可识别的图片"));
    }

    #[tokio::test]
    async fn dispatch_reads_local_image_file() {
        let dir = std::env::temp_dir().join(format!("nineclaw-vision-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        let png_path = dir.join("sample.png");
        let mut png_bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        png_bytes.extend_from_slice(&[0u8; 16]);
        fs::write(&png_path, &png_bytes).expect("write sample png");

        let runtime = ImageVisionRuntimeConfig {
            api_format: "openai".to_string(),
            base_url: "http://127.0.0.1:1".to_string(),
            api_key: "k".to_string(),
            model: "m".to_string(),
            max_output_tokens: 512,
            default_prompt: DEFAULT_VISION_PROMPT.to_string(),
        };
        let payload = ImageVisionAnalyzeRequest {
            images: vec![ImageVisionSource {
                path: Some(png_path.to_string_lossy().to_string()),
                url: None,
            }],
            prompt: Some("这张图里有什么".to_string()),
        };
        // 上游不可达，但本地图片读取成功后才会发起请求，因此错误应是网络层失败。
        // no_proxy 避免测试环境的系统代理把连接拒绝改写成 502 响应。
        let client = reqwest::Client::builder().no_proxy().build().expect("test client");
        let error = dispatch_image_vision(&client, &runtime, &payload)
            .await
            .unwrap_err();
        assert!(
            error.contains("识图模型请求失败"),
            "unexpected dispatch error: {error}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}