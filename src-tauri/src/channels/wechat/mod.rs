pub mod api;
pub mod types;

use crate::agent_workspace;
use crate::agents::ConversationAgentConfig;
use crate::dev_trace::dev_trace;
use crate::media_directives::{
    build_media_directive_line, parse_markdown_media_reference, parse_media_directive_fields,
    parse_plain_media_path_reference,
};
use openssl::symm::Cipher;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64_ENGINE, Engine as Base64Engine};
use serde_json::Value;
use tauri::AppHandle;

use self::api::WeChatApi;
use self::types::*;
use crate::channels::im_message_merge_window_ms;
use crate::channels::im_reply_format::{resolve_reply_card_items, wechat_im_text_segments};
use crate::channels::pi_bridge::{PiBridge, PiProcessOutcome, PiRunHandle};
use crate::channels::types::{BotMessage, ChannelStatus, MediaPayload, MediaType};
use crate::channels::Channel;
use crate::prompt_attachments::PromptAttachmentInput;

/// Truncate `s` to at most `max_chars` Unicode characters (not bytes).
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
        .map(|state| {
            !state.running
                && !state.queued
                && state.pending_texts.is_empty()
                && state.pending_attachments.is_empty()
        })
        .unwrap_or(false);

    if should_remove {
        guard.remove(user_id);
    }
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
    let parsed = parse_media_directive_fields(line)?;
    let path = parsed.path;
    if !Path::new(&path).is_absolute() {
        return None;
    }

    let media_type = match parsed.media_type.as_deref() {
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

    let file_name = parsed.name.unwrap_or_else(|| {
        Path::new(&path)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "attachment".to_string())
    });

    Some(ParsedMediaItem {
        media_type,
        file_name,
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
    } else if is_audio_path(&path) {
        MediaType::Audio
    } else {
        MediaType::File
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

fn parse_plain_path_media(line: &str) -> Option<ParsedMediaItem> {
    let reference = parse_plain_media_path_reference(line)?;
    let path = reference.path;

    let media_type = if is_image_path(&path) {
        MediaType::Image
    } else if is_video_path(&path) {
        MediaType::Video
    } else if is_audio_path(&path) {
        MediaType::Audio
    } else {
        MediaType::File
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

fn split_text_and_media(content: &str) -> (String, Vec<ParsedMediaItem>) {
    let mut text_lines = Vec::new();
    let mut media_items = Vec::new();

    for line in content.lines() {
        if let Some(item) = parse_media_directive(line)
            .or_else(|| parse_markdown_media(line))
            .or_else(|| parse_plain_path_media(line))
        {
            media_items.push(item);
        } else {
            text_lines.push(line);
        }
    }

    (text_lines.join("\n").trim().to_string(), media_items)
}

fn outbound_media_type_slug(media_type: &MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "image",
        MediaType::Video => "video",
        MediaType::Audio => "audio",
        MediaType::File => "file",
    }
}

fn outbound_media_label(media_type: &MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "图片",
        MediaType::Video => "视频",
        MediaType::Audio => "语音",
        MediaType::File => "文件",
    }
}

fn persist_outbound_media_items(
    agent_id: Option<&str>,
    user_id: &str,
    media_items: Vec<ParsedMediaItem>,
) -> Vec<ParsedMediaItem> {
    let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return media_items;
    };

    media_items
        .into_iter()
        .map(|item| {
            let resolved_path =
                agent_workspace::resolve_agent_media_reference(Some(agent_id), &item.file_path)
                    .unwrap_or_else(|| PathBuf::from(&item.file_path));
            let source_path = resolved_path.as_path();
            match agent_workspace::persist_agent_outbound_artifact(
                agent_id,
                user_id,
                &item.file_name,
                source_path,
            ) {
                Ok(stable_path) => {
                    let _ = agent_workspace::register_agent_outbound_artifact_source(
                        agent_id,
                        &item.file_name,
                        &stable_path,
                        None,
                        Some(&format!("user={user_id} channel=wechat")),
                    );
                    ParsedMediaItem {
                        file_path: stable_path.display().to_string(),
                        ..item
                    }
                }
                Err(error) => {
                    log::warn!("归档微信出站媒体失败 {}: {}", item.file_path, error);
                    item
                }
            }
        })
        .collect()
}

fn build_outbound_display_text(text_reply: &str, media_items: &[ParsedMediaItem]) -> String {
    let mut lines: Vec<String> = Vec::new();
    if !text_reply.trim().is_empty() {
        lines.push(text_reply.trim().to_string());
    }
    for item in media_items {
        lines.push(build_media_directive_line(
            outbound_media_type_slug(&item.media_type),
            &item.file_path,
            Some(&item.file_name),
            Some(outbound_media_label(&item.media_type)),
        ));
    }
    lines.join("\n")
}

fn clean_media_fallback_text(text_reply: &str, has_media: bool) -> String {
    if !has_media {
        return text_reply.trim().to_string();
    }

    let filtered = text_reply
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.contains("不能可靠地把本地原文件作为可下载附件送达")
                && !trimmed.contains("不能稳定把本地文件作为可下载附件真正送达")
                && !trimmed.contains("当前通道似乎不能稳定")
                && !trimmed.contains("如果你要我继续“发文件”")
                && !trimmed.contains("大概率还是会失败")
                && !trimmed.contains("任选其一")
                && !trimmed.contains("最推荐")
                && !trimmed.contains("压缩到下载目录")
        })
        .collect::<Vec<_>>()
        .join("\n");

    filtered.trim().to_string()
}

fn inbound_media_label(media_type: &MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "图片",
        MediaType::Video => "视频",
        MediaType::Audio => "语音",
        MediaType::File => "文件",
    }
}

fn default_extension(media_type: &MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "png",
        MediaType::Video => "mp4",
        MediaType::Audio => "mp3",
        MediaType::File => "bin",
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
    let display_text = display_lines.join("\n").trim().to_string();
    let prompt_attachments = attachments
        .iter()
        .map(|attachment| PromptAttachmentInput {
            file_name: Path::new(&attachment.saved_path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            file_path: attachment.saved_path.clone(),
            mime_type: Path::new(&attachment.saved_path)
                .is_file()
                .then(|| crate::infer_media_mime_type(Path::new(&attachment.saved_path), None))
                .unwrap_or_default(),
            kind: match attachment.media_type {
                MediaType::Image => "image",
                MediaType::Video => "video",
                MediaType::Audio => "audio",
                MediaType::File => "file",
            }
            .to_string(),
            transcript: attachment.transcript.clone(),
        })
        .collect();

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
                section.push_str(" 已接收，但当前没有可用文件路径");
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
        display_text,
        prompt_attachments,
    }
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn lookup_extra_string(extra: &HashMap<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = extra.get(*key).and_then(|value| value.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn lookup_container_string(
    extra: &HashMap<String, Value>,
    container_keys: &[&str],
    field_keys: &[&str],
) -> Option<String> {
    for container_key in container_keys {
        let Some(map) = extra
            .get(*container_key)
            .and_then(|value| value.as_object())
        else {
            continue;
        };
        for field_key in field_keys {
            if let Some(value) = map.get(*field_key).and_then(|value| value.as_str()) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

fn resolve_item_string(
    direct_value: Option<&str>,
    nested_extra: Option<&HashMap<String, Value>>,
    item_extra: &HashMap<String, Value>,
    container_keys: &[&str],
    field_keys: &[&str],
) -> Option<String> {
    normalize_optional_string(direct_value)
        .or_else(|| nested_extra.and_then(|extra| lookup_extra_string(extra, field_keys)))
        .or_else(|| lookup_container_string(item_extra, container_keys, field_keys))
        .or_else(|| lookup_extra_string(item_extra, field_keys))
}

fn extract_text_item_content(item: &MessageItem) -> Option<String> {
    resolve_item_string(
        item.text_item
            .as_ref()
            .and_then(|text_item| text_item.text.as_deref()),
        item.text_item.as_ref().map(|text_item| &text_item.extra),
        &item.extra,
        &["text_item", "text"],
        &["text", "content"],
    )
}

fn extract_image_fields(item: &MessageItem) -> (Option<String>, Option<String>) {
    (
        resolve_item_string(
            item.image_item
                .as_ref()
                .and_then(|image_item| image_item.image_base64.as_deref()),
            item.image_item.as_ref().map(|image_item| &image_item.extra),
            &item.extra,
            &["image_item", "image"],
            &["image_base64", "base64", "data", "content"],
        ),
        resolve_item_string(
            item.image_item
                .as_ref()
                .and_then(|image_item| image_item.image_url.as_deref()),
            item.image_item.as_ref().map(|image_item| &image_item.extra),
            &item.extra,
            &["image_item", "image"],
            &["image_url", "url", "download_url", "file_url"],
        ),
    )
}

fn extract_file_fields(item: &MessageItem) -> (Option<String>, Option<String>, Option<String>) {
    (
        resolve_item_string(
            item.file_item
                .as_ref()
                .and_then(|file_item| file_item.file_name.as_deref()),
            item.file_item.as_ref().map(|file_item| &file_item.extra),
            &item.extra,
            &["file_item", "file"],
            &["file_name", "name", "title"],
        ),
        resolve_item_string(
            item.file_item
                .as_ref()
                .and_then(|file_item| file_item.file_base64.as_deref()),
            item.file_item.as_ref().map(|file_item| &file_item.extra),
            &item.extra,
            &["file_item", "file"],
            &["file_base64", "base64", "data", "content"],
        ),
        resolve_item_string(
            item.file_item
                .as_ref()
                .and_then(|file_item| file_item.file_url.as_deref()),
            item.file_item.as_ref().map(|file_item| &file_item.extra),
            &item.extra,
            &["file_item", "file"],
            &["file_url", "url", "download_url"],
        ),
    )
}

fn extract_video_fields(item: &MessageItem) -> (Option<String>, Option<String>, Option<String>) {
    (
        resolve_item_string(
            None,
            item.video_item.as_ref().map(|video_item| &video_item.extra),
            &item.extra,
            &["video_item", "video"],
            &["file_name", "name", "title"],
        ),
        resolve_item_string(
            item.video_item
                .as_ref()
                .and_then(|video_item| video_item.video_base64.as_deref()),
            item.video_item.as_ref().map(|video_item| &video_item.extra),
            &item.extra,
            &["video_item", "video"],
            &["video_base64", "base64", "data", "content"],
        ),
        resolve_item_string(
            item.video_item
                .as_ref()
                .and_then(|video_item| video_item.video_url.as_deref()),
            item.video_item.as_ref().map(|video_item| &video_item.extra),
            &item.extra,
            &["video_item", "video"],
            &["video_url", "url", "download_url", "file_url"],
        ),
    )
}

fn extract_voice_fields(
    item: &MessageItem,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    (
        resolve_item_string(
            item.voice_item
                .as_ref()
                .and_then(|voice_item| voice_item.file_name.as_deref()),
            item.voice_item.as_ref().map(|voice_item| &voice_item.extra),
            &item.extra,
            &["voice_item", "voice", "audio_item", "audio"],
            &["file_name", "name", "title"],
        ),
        resolve_item_string(
            item.voice_item
                .as_ref()
                .and_then(|voice_item| voice_item.voice_base64.as_deref()),
            item.voice_item.as_ref().map(|voice_item| &voice_item.extra),
            &item.extra,
            &["voice_item", "voice", "audio_item", "audio"],
            &["voice_base64", "audio_base64", "base64", "data", "content"],
        ),
        resolve_item_string(
            item.voice_item
                .as_ref()
                .and_then(|voice_item| voice_item.voice_url.as_deref()),
            item.voice_item.as_ref().map(|voice_item| &voice_item.extra),
            &item.extra,
            &["voice_item", "voice", "audio_item", "audio"],
            &["voice_url", "audio_url", "url", "download_url", "file_url"],
        ),
        resolve_item_string(
            item.voice_item
                .as_ref()
                .and_then(|voice_item| voice_item.text.as_deref()),
            item.voice_item.as_ref().map(|voice_item| &voice_item.extra),
            &item.extra,
            &["voice_item", "voice", "audio_item", "audio"],
            &["text", "transcript", "asr_text"],
        ),
    )
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CdnMediaRef {
    encrypt_query_param: Option<String>,
    aes_key: Option<String>,
    full_url: Option<String>,
}

const WECHAT_DEFAULT_CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

fn lookup_container_media_string(
    extra: &HashMap<String, Value>,
    container_keys: &[&str],
    field_keys: &[&str],
) -> Option<String> {
    for container_key in container_keys {
        let Some(map) = extra
            .get(*container_key)
            .and_then(|value| value.as_object())
        else {
            continue;
        };
        let Some(media) = map.get("media").and_then(|value| value.as_object()) else {
            continue;
        };
        for field_key in field_keys {
            if let Some(value) = media.get(*field_key).and_then(|value| value.as_str()) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

fn extract_cdn_media_fields(
    item_extra: &HashMap<String, Value>,
    container_keys: &[&str],
    media: Option<&CdnMedia>,
    preferred_aes_key: Option<&str>,
) -> Option<CdnMediaRef> {
    let encrypt_query_param =
        normalize_optional_string(media.and_then(|media| media.encrypt_query_param.as_deref()))
            .or_else(|| {
                lookup_container_media_string(item_extra, container_keys, &["encrypt_query_param"])
            });
    let full_url = normalize_optional_string(media.and_then(|media| media.full_url.as_deref()))
        .or_else(|| lookup_container_media_string(item_extra, container_keys, &["full_url"]));
    let aes_key = normalize_optional_string(preferred_aes_key)
        .or_else(|| normalize_optional_string(media.and_then(|media| media.aes_key.as_deref())))
        .or_else(|| {
            lookup_container_media_string(item_extra, container_keys, &["aes_key", "aeskey"])
        });

    if encrypt_query_param.is_none() && full_url.is_none() && aes_key.is_none() {
        return None;
    }

    Some(CdnMediaRef {
        encrypt_query_param,
        aes_key,
        full_url,
    })
}

fn extract_image_cdn_fields(item: &MessageItem) -> Option<CdnMediaRef> {
    extract_cdn_media_fields(
        &item.extra,
        &["image_item", "image"],
        item.image_item
            .as_ref()
            .and_then(|image_item| image_item.media.as_ref()),
        item.image_item
            .as_ref()
            .and_then(|image_item| image_item.aeskey.as_deref()),
    )
}

fn extract_file_cdn_fields(item: &MessageItem) -> Option<CdnMediaRef> {
    extract_cdn_media_fields(
        &item.extra,
        &["file_item", "file"],
        item.file_item
            .as_ref()
            .and_then(|file_item| file_item.media.as_ref()),
        None,
    )
}

fn extract_video_cdn_fields(item: &MessageItem) -> Option<CdnMediaRef> {
    extract_cdn_media_fields(
        &item.extra,
        &["video_item", "video"],
        item.video_item
            .as_ref()
            .and_then(|video_item| video_item.media.as_ref()),
        None,
    )
}

fn extract_voice_cdn_fields(item: &MessageItem) -> Option<CdnMediaRef> {
    extract_cdn_media_fields(
        &item.extra,
        &["voice_item", "voice", "audio_item", "audio"],
        item.voice_item
            .as_ref()
            .and_then(|voice_item| voice_item.media.as_ref()),
        None,
    )
}

fn file_name_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    let last_segment = trimmed
        .split('?')
        .next()
        .unwrap_or(trimmed)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .trim();
    if last_segment.is_empty() {
        return None;
    }

    urlencoding::decode(last_segment)
        .map(|value| value.into_owned())
        .ok()
        .or_else(|| Some(last_segment.to_string()))
}

fn download_url_bytes(
    rt: &tokio::runtime::Runtime,
    api: &WeChatApi,
    url: &str,
) -> Result<Vec<u8>, String> {
    rt.block_on(api.download_attachment(url))
}

fn download_public_url_bytes(
    rt: &tokio::runtime::Runtime,
    api: &WeChatApi,
    url: &str,
) -> Result<Vec<u8>, String> {
    rt.block_on(api.download_public_attachment(url))
}

fn build_wechat_cdn_download_url(encrypt_query_param: &str) -> String {
    format!(
        "{}/download?encrypted_query_param={}",
        WECHAT_DEFAULT_CDN_BASE_URL,
        urlencoding::encode(encrypt_query_param)
    )
}

fn decode_hex_bytes(raw: &str) -> Result<Vec<u8>, String> {
    let trimmed = raw.trim();
    if trimmed.len() % 2 != 0 {
        return Err(format!("十六进制长度必须为偶数，实际为 {}", trimmed.len()));
    }

    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    for chunk in trimmed.as_bytes().chunks(2) {
        let pair =
            std::str::from_utf8(chunk).map_err(|error| format!("解析十六进制失败: {error}"))?;
        let value = u8::from_str_radix(pair, 16)
            .map_err(|error| format!("解析十六进制失败 {pair}: {error}"))?;
        bytes.push(value);
    }
    Ok(bytes)
}

fn parse_wechat_aes_key(raw_key: &str) -> Result<Vec<u8>, String> {
    let trimmed = raw_key.trim();
    if trimmed.is_empty() {
        return Err("AES key 为空".to_string());
    }

    if trimmed.len() == 32
        && trimmed
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return decode_hex_bytes(trimmed);
    }

    let decoded = BASE64_ENGINE
        .decode(trimmed)
        .map_err(|error| format!("解析 AES key Base64 失败: {error}"))?;
    if decoded.len() == 16 {
        return Ok(decoded);
    }

    if decoded.len() == 32 && decoded.iter().all(|byte| byte.is_ascii_hexdigit()) {
        let ascii_hex = std::str::from_utf8(&decoded)
            .map_err(|error| format!("解析 AES key ASCII 十六进制失败: {error}"))?;
        return decode_hex_bytes(ascii_hex);
    }

    Err(format!(
        "AES key 解码后长度异常，期望 16 字节或 32 字符十六进制，实际 {} 字节",
        decoded.len()
    ))
}

fn decrypt_wechat_cdn_payload(ciphertext: &[u8], aes_key: &str) -> Result<Vec<u8>, String> {
    let key = parse_wechat_aes_key(aes_key)?;
    openssl::symm::decrypt(Cipher::aes_128_ecb(), &key, None, ciphertext)
        .map_err(|error| format!("AES-128-ECB 解密失败: {error}"))
}

fn download_cdn_bytes(
    rt: &tokio::runtime::Runtime,
    api: &WeChatApi,
    cdn_media: &CdnMediaRef,
) -> Result<Vec<u8>, String> {
    let download_url = if let Some(full_url) = cdn_media
        .full_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        full_url.to_string()
    } else if let Some(encrypt_query_param) = cdn_media
        .encrypt_query_param
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        build_wechat_cdn_download_url(encrypt_query_param)
    } else {
        return Err("缺少微信 CDN 下载地址".to_string());
    };

    let encrypted = download_public_url_bytes(rt, api, &download_url)?;
    if let Some(aes_key) = cdn_media
        .aes_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        decrypt_wechat_cdn_payload(&encrypted, aes_key)
    } else {
        Ok(encrypted)
    }
}

fn persist_wechat_attachment(
    rt: &tokio::runtime::Runtime,
    api: &WeChatApi,
    agent_id: &str,
    user_id: &str,
    media_type: MediaType,
    file_name_hint: Option<&str>,
    base64_data: Option<&str>,
    url_data: Option<&str>,
    cdn_media: Option<&CdnMediaRef>,
    transcript: Option<&str>,
) -> Result<Option<InboundAttachment>, String> {
    let data = if let Some(raw) = base64_data.filter(|value| !value.trim().is_empty()) {
        Some(
            BASE64_ENGINE
                .decode(raw.trim())
                .map_err(|error| format!("解析附件 Base64 失败: {error}"))?,
        )
    } else if let Some(url) = url_data.filter(|value| !value.trim().is_empty()) {
        Some(download_url_bytes(rt, api, url.trim())?)
    } else if let Some(cdn_media) = cdn_media {
        Some(download_cdn_bytes(rt, api, cdn_media)?)
    } else {
        None
    };

    if data.is_none() && transcript.unwrap_or_default().trim().is_empty() {
        return Ok(None);
    }

    let file_name = file_name_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| url_data.and_then(file_name_from_url))
        .or_else(|| {
            cdn_media
                .and_then(|media| media.full_url.as_deref())
                .and_then(file_name_from_url)
        })
        .unwrap_or_else(|| {
            format!(
                "{}.{}",
                match media_type {
                    MediaType::Image => "image",
                    MediaType::Video => "video",
                    MediaType::Audio => "voice",
                    MediaType::File => "file",
                },
                default_extension(&media_type)
            )
        });

    let saved_path = if let Some(bytes) = data {
        agent_workspace::persist_agent_inbound_artifact(agent_id, user_id, &file_name, &bytes)?
            .to_string_lossy()
            .to_string()
    } else {
        String::new()
    };

    Ok(Some(InboundAttachment {
        media_type,
        saved_path,
        transcript: transcript
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    }))
}

fn extract_inbound_message(
    rt: &tokio::runtime::Runtime,
    api: &WeChatApi,
    items: &Option<Vec<MessageItem>>,
    agent_id: Option<&str>,
    user_id: &str,
) -> Result<Option<InboundMessagePayload>, String> {
    let Some(items) = items else {
        return Ok(None);
    };

    let mut text = String::new();
    let mut attachments = Vec::new();

    for item in items {
        if item.item_type == Some(MSG_ITEM_TYPE_TEXT) || item.text_item.is_some() {
            if let Some(item_text) = extract_text_item_content(item) {
                text.push_str(&item_text);
            }
        }

        if let Some(agent_id) = agent_id {
            let (image_base64, image_url) = extract_image_fields(item);
            let image_cdn = extract_image_cdn_fields(item);
            if item.item_type == Some(MSG_ITEM_TYPE_IMAGE)
                || item.image_item.is_some()
                || image_base64.is_some()
                || image_url.is_some()
                || image_cdn.is_some()
            {
                if let Some(attachment) = persist_wechat_attachment(
                    rt,
                    api,
                    agent_id,
                    user_id,
                    MediaType::Image,
                    Some("image.png"),
                    image_base64.as_deref(),
                    image_url.as_deref(),
                    image_cdn.as_ref(),
                    None,
                )? {
                    attachments.push(attachment);
                }
            }

            let (file_name, file_base64, file_url) = extract_file_fields(item);
            let file_cdn = extract_file_cdn_fields(item);
            if item.item_type == Some(MSG_ITEM_TYPE_FILE)
                || item.file_item.is_some()
                || file_base64.is_some()
                || file_url.is_some()
                || file_cdn.is_some()
            {
                if let Some(attachment) = persist_wechat_attachment(
                    rt,
                    api,
                    agent_id,
                    user_id,
                    MediaType::File,
                    file_name.as_deref(),
                    file_base64.as_deref(),
                    file_url.as_deref(),
                    file_cdn.as_ref(),
                    None,
                )? {
                    attachments.push(attachment);
                }
            }

            let (video_name, video_base64, video_url) = extract_video_fields(item);
            let video_cdn = extract_video_cdn_fields(item);
            if item.item_type == Some(MSG_ITEM_TYPE_VIDEO)
                || item.video_item.is_some()
                || video_base64.is_some()
                || video_url.is_some()
                || video_cdn.is_some()
            {
                if let Some(attachment) = persist_wechat_attachment(
                    rt,
                    api,
                    agent_id,
                    user_id,
                    MediaType::Video,
                    video_name.as_deref().or(Some("video.mp4")),
                    video_base64.as_deref(),
                    video_url.as_deref(),
                    video_cdn.as_ref(),
                    None,
                )? {
                    attachments.push(attachment);
                }
            }

            let (voice_name, voice_base64, voice_url, voice_text) = extract_voice_fields(item);
            let voice_cdn = extract_voice_cdn_fields(item);
            if item.item_type == Some(MSG_ITEM_TYPE_VOICE)
                || item.voice_item.is_some()
                || voice_base64.is_some()
                || voice_url.is_some()
                || voice_text.is_some()
                || voice_cdn.is_some()
            {
                if let Some(attachment) = persist_wechat_attachment(
                    rt,
                    api,
                    agent_id,
                    user_id,
                    MediaType::Audio,
                    voice_name.as_deref().or(Some("voice.mp3")),
                    voice_base64.as_deref(),
                    voice_url.as_deref(),
                    voice_cdn.as_ref(),
                    voice_text.as_deref(),
                )? {
                    attachments.push(attachment);
                }
            }
        }
    }

    let payload = build_inbound_prompt(&text, &attachments);
    if payload.prompt_text.is_empty() {
        return Ok(None);
    }
    Ok(Some(payload))
}

const DEFAULT_BOT_TYPE: &str = "3";
const MAX_QR_REFRESH: u32 = 3;
const CHUNK_SIZE: usize = 3900;
/// Interval between consecutive getUpdates polls (seconds).
const POLL_INTERVAL_SECS: u64 = 3;
/// Interval between typing indicator refreshes (seconds).
const TYPING_INTERVAL_SECS: u64 = 5;
/// Chunk size for on_chunk frontend streaming (characters).
const STREAM_CHUNK_SIZE: usize = 500;

/// Internal message routed from monitor thread to worker thread.
struct WorkItem {
    user_id: String,
}

#[derive(Clone)]
struct UserTurnState {
    pending_texts: Vec<String>,
    pending_attachments: Vec<PromptAttachmentInput>,
    latest_context_token: String,
    last_inbound_at: i64,
    queued: bool,
    running: bool,
    active_run: Option<Arc<PiRunHandle>>,
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
    prompt_attachments: Vec<PromptAttachmentInput>,
}

#[derive(Clone, Debug)]
struct ParsedMediaItem {
    media_type: MediaType,
    file_name: String,
    file_path: String,
}

/// WeChat Channel — fully self-contained.
///
/// Manages its own monitor thread (getUpdates long-poll), worker thread
/// (AI via pi subprocess), and reply delivery (sendMessage with
/// context_token + all required iLink ghost fields).
pub struct WeChatChannel {
    channel_id: String,
    token: String,
    base_url: String,
    route_tag: Option<String>,
    running: Arc<AtomicBool>,
    status: Arc<Mutex<ChannelStatus>>,
    sync_buf: Arc<Mutex<String>>,
    /// Per-user context_token cache (user_id → latest context_token).
    context_tokens: Arc<Mutex<HashMap<String, String>>>,
    /// AI provider config so the channel can run pi independently.
    ai_provider_id: String,
    ai_api_format: String,
    ai_base_url: String,
    ai_api_key: String,
    ai_model: String,
    agent_config: Option<ConversationAgentConfig>,
}

impl WeChatChannel {
    pub fn new(channel_id: &str, token: &str, base_url: &str, route_tag: Option<&str>) -> Self {
        let url = if base_url.is_empty() {
            "https://ilinkai.weixin.qq.com".to_string()
        } else {
            base_url.to_string()
        };
        Self {
            channel_id: channel_id.to_string(),
            token: token.to_string(),
            base_url: url,
            route_tag: route_tag.map(|s| s.to_string()),
            running: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(ChannelStatus::Disconnected)),
            sync_buf: Arc::new(Mutex::new(String::new())),
            context_tokens: Arc::new(Mutex::new(HashMap::new())),
            ai_provider_id: String::new(),
            ai_api_format: "openai".to_string(),
            ai_base_url: String::new(),
            ai_api_key: String::new(),
            ai_model: String::new(),
            agent_config: None,
        }
    }

    /// Configure the AI provider so the channel can call pi for replies.
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

    /// QR code login: fetch QR and poll until confirmed.
    pub fn login_with_qr(&self, app: &AppHandle) -> Result<WechatLoginResult, String> {
        let base_url = self.base_url.clone();
        let channel_id = self.channel_id.clone();

        let qr_resp = block_on_async(WeChatApi::get_bot_qrcode(&base_url, DEFAULT_BOT_TYPE))?;
        let qrcode = qr_resp
            .qrcode
            .clone()
            .ok_or_else(|| "QR 码响应缺少 qrcode 字段".to_string())?;
        let qrcode_content = qr_resp
            .qrcode_img_content
            .clone()
            .ok_or_else(|| "QR 码响应缺少 qrcode_img_content 字段".to_string())?;

        let qr_data_uri = generate_qr_data_uri(&qrcode_content)?;
        crate::emit_safe::emit_safe(app,
            "bot://qr-code",
            serde_json::json!({
                "channelId": &channel_id, "qrcodeUrl": &qr_data_uri, "status": "waiting"
            }),
        );

        let mut refresh_count: u32 = 0;
        let deadline = std::time::Instant::now() + Duration::from_secs(300);

        while std::time::Instant::now() < deadline {
            match block_on_async(WeChatApi::get_qrcode_status(&base_url, &qrcode)) {
                Ok(sr) => match sr.status.as_deref() {
                    Some("wait") => {}
                    Some("scaned") => {
                        crate::emit_safe::emit_safe(app,
                            "bot://qr-code",
                            serde_json::json!({
                                "channelId": &channel_id, "status": "scanned"
                            }),
                        );
                    }
                    Some("expired") => {
                        refresh_count += 1;
                        if refresh_count > MAX_QR_REFRESH {
                            return Ok(WechatLoginResult {
                                connected: false,
                                message: "二维码多次过期，请重新登录".to_string(),
                                ..Default::default()
                            });
                        }
                        let nq =
                            block_on_async(WeChatApi::get_bot_qrcode(&base_url, DEFAULT_BOT_TYPE))?;
                        let nc = nq.qrcode_img_content.as_deref().unwrap_or("");
                        let nd = generate_qr_data_uri(nc)?;
                        crate::emit_safe::emit_safe(app,
                            "bot://qr-code",
                            serde_json::json!({
                                "channelId": &channel_id, "qrcodeUrl": &nd, "status": "refreshed"
                            }),
                        );
                    }
                    Some("confirmed") => {
                        let bot_token = sr
                            .bot_token
                            .clone()
                            .ok_or_else(|| "登录成功但未收到 bot_token".to_string())?;
                        let ilink_bot_id = sr
                            .ilink_bot_id
                            .clone()
                            .ok_or_else(|| "登录成功但未收到 ilink_bot_id".to_string())?;
                        crate::emit_safe::emit_safe(app,
                            "bot://qr-code",
                            serde_json::json!({
                                "channelId": &channel_id, "status": "confirmed"
                            }),
                        );
                        return Ok(WechatLoginResult {
                            connected: true,
                            bot_token: Some(bot_token),
                            account_id: Some(ilink_bot_id),
                            base_url: sr.baseurl.clone(),
                            user_id: sr.ilink_user_id.clone(),
                            message: "微信连接成功".to_string(),
                        });
                    }
                    _ => {}
                },
                Err(e) if e == "timeout" => {}
                Err(e) => return Err(format!("查询扫码状态失败: {e}")),
            }
            thread::sleep(Duration::from_secs(2));
        }

        Ok(WechatLoginResult {
            connected: false,
            message: "登录超时，请重试".to_string(),
            ..Default::default()
        })
    }

    fn bound_agent_debug_label(&self) -> String {
        self.agent_config
            .as_ref()
            .map(|agent| format!("{} ({})", agent.name, agent.id))
            .unwrap_or_else(|| "未绑定智能体".to_string())
    }
}

// ── Channel trait implementation ──

impl Channel for WeChatChannel {
    fn start(&mut self, app: AppHandle) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("微信通道已在运行".to_string());
        }
        if self.token.is_empty() {
            return Err("微信 token 未配置，请先登录".to_string());
        }
        let pi_runtime = crate::pi_runtime::require_pi_runtime_location(&app)?;

        *self.status.lock().unwrap() = ChannelStatus::Connected;
        self.running.store(true, Ordering::SeqCst);

        emit_bot_status(
            &app,
            &self.channel_id,
            "",
            "done",
            &format!(
                "微信机器人已启动，当前绑定智能体: {}",
                self.bound_agent_debug_label()
            ),
        );

        // Local channel: monitor → worker
        let (work_tx, work_rx) = mpsc::channel::<WorkItem>();
        let user_states: Arc<Mutex<HashMap<String, UserTurnState>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // ── Thread 1: Monitor (getUpdates long-poll) ──
        {
            let running = self.running.clone();
            let token = self.token.clone();
            let base_url = self.base_url.clone();
            let route_tag = self.route_tag.clone();
            let sync_buf = self.sync_buf.clone();
            let status = self.status.clone();
            let context_tokens = self.context_tokens.clone();
            let agent_config = self.agent_config.clone();
            let app_handle = app.clone();
            let channel_id = self.channel_id.clone();
            let user_states = user_states.clone();

            thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::error!("微信监控线程: 创建 runtime 失败: {e}");
                        running.store(false, Ordering::SeqCst);
                        return;
                    }
                };
                let api = WeChatApi::new(&base_url, &token, route_tag.as_deref());

                while running.load(Ordering::SeqCst) {
                    let buf = sync_buf.lock().unwrap().clone();
                    match rt.block_on(api.get_updates(&buf)) {
                        Ok(resp) => {
                            if let Some(new_buf) = resp.get_updates_buf {
                                *sync_buf.lock().unwrap() = new_buf;
                            }
                            if let Some(errcode) = resp.errcode {
                                if errcode == -14 {
                                    log::info!("WeChat session expired (errcode -14)");
                                    running.store(false, Ordering::SeqCst);
                                    *status.lock().unwrap() =
                                        ChannelStatus::Error("会话已过期".to_string());
                                    break;
                                }
                            }
                            if let Some(msgs) = resp.msgs {
                                for msg in msgs {
                                    if msg.message_type != Some(MSG_TYPE_USER) {
                                        continue;
                                    }
                                    let from_user = msg.from_user_id.clone().unwrap_or_default();

                                    // Save context_token — CRITICAL for replies
                                    if let Some(ref ct) = msg.context_token {
                                        if !ct.is_empty() {
                                            context_tokens
                                                .lock()
                                                .unwrap()
                                                .insert(from_user.clone(), ct.clone());
                                        }
                                    }

                                    let inbound = match extract_inbound_message(
                                        &rt,
                                        &api,
                                        &msg.item_list,
                                        agent_config.as_ref().map(|config| config.id.as_str()),
                                        &from_user,
                                    ) {
                                        Ok(Some(payload)) => payload,
                                        Ok(None) => continue,
                                        Err(error) => {
                                            emit_bot_status(
                                                &app_handle,
                                                &channel_id,
                                                &from_user,
                                                "warn",
                                                &format!("解析微信入站附件失败: {error}"),
                                            );
                                            continue;
                                        }
                                    };

                                    log::info!(
                                        "微信收到消息: 用户={from_user} 内容={}",
                                        truncate_chars(&inbound.display_text, 50)
                                    );
                                    dev_trace(
                                        "wechat",
                                        format!(
                                            "收到消息: user={} chars={} text={}",
                                            from_user,
                                            inbound.display_text.chars().count(),
                                            truncate_chars(&inbound.display_text, 80)
                                        ),
                                    );

                                    // Emit inbound event to frontend (history integration)
                                    emit_bot_message(
                                        &app_handle,
                                        &channel_id,
                                        &from_user,
                                        "inbound",
                                        &inbound.display_text,
                                        agent_config.as_ref(),
                                    );

                                    let ct = msg.context_token.clone().unwrap_or_default();
                                    let control_update = {
                                        let mut guard = match user_states.lock() {
                                            Ok(guard) => guard,
                                            Err(_) => {
                                                running.store(false, Ordering::SeqCst);
                                                break;
                                            }
                                        };

                                        let state =
                                            guard.entry(from_user.clone()).or_insert_with(|| {
                                                UserTurnState {
                                                    pending_texts: Vec::new(),
                                                    pending_attachments: Vec::new(),
                                                    latest_context_token: ct.clone(),
                                                    last_inbound_at: now_timestamp_ms(),
                                                    queued: false,
                                                    running: false,
                                                    active_run: None,
                                                }
                                            });

                                        state.pending_texts.push(inbound.prompt_text);
                                        state
                                            .pending_attachments
                                            .extend(inbound.prompt_attachments.clone());
                                        state.latest_context_token = ct.clone();
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

                                    if let Some(run_handle) = control_update.1 {
                                        let _ = run_handle.abort();
                                    }

                                    if control_update.0
                                        && work_tx.send(WorkItem { user_id: from_user }).is_err()
                                    {
                                        log::warn!("Worker 已关闭，停止微信监控");
                                        running.store(false, Ordering::SeqCst);
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if !running.load(Ordering::SeqCst) {
                                break;
                            }
                            if e != "timeout" {
                                log::warn!("微信轮询失败: {e}");
                            }
                        }
                    }
                    // 定时轮询间隔 — 确保每个周期都有休眠，避免狂请求
                    if running.load(Ordering::SeqCst) {
                        thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));
                    }
                }
            });
        }

        // ── Thread 2: Worker (AI processing + streaming reply delivery) ──
        {
            let running = self.running.clone();
            let token = self.token.clone();
            let base_url = self.base_url.clone();
            let route_tag = self.route_tag.clone();
            let context_tokens = self.context_tokens.clone();
            let ai_pid = self.ai_provider_id.clone();
            let ai_fmt = self.ai_api_format.clone();
            let ai_base = self.ai_base_url.clone();
            let ai_key = self.ai_api_key.clone();
            let ai_mdl = self.ai_model.clone();
            let agent_config = self.agent_config.clone();
            let app_handle = app.clone();
            let channel_id = self.channel_id.clone();
            let user_states = user_states.clone();

            thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::error!("微信 Worker: 创建 runtime 失败: {e}");
                        running.store(false, Ordering::SeqCst);
                        return;
                    }
                };
                let api = WeChatApi::new(&base_url, &token, route_tag.as_deref());
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

                    let user_id = item.user_id;

                    while running.load(Ordering::SeqCst) {
                        let wait_more_ms = {
                            let guard = match user_states.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            let Some(state) = guard.get(&user_id) else {
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

                        let (prompt_text, prompt_attachments, context_token) = {
                            let mut guard = match user_states.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            let Some(state) = guard.get_mut(&user_id) else {
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
                                std::mem::take(&mut state.pending_attachments),
                                state.latest_context_token.clone(),
                            )
                        };

                        log::info!(
                            "Worker 开始处理: 用户={} 内容={}",
                            user_id,
                            truncate_chars(&prompt_text, 50)
                        );
                        dev_trace(
                            "wechat",
                            format!(
                                "开始处理: user={} prompt_chars={} merged_count={}",
                                user_id,
                                prompt_text.chars().count(),
                                prompt_text.matches("【用户追加消息").count() + 1
                            ),
                        );
                        emit_bot_status(
                            &app_handle,
                            &channel_id,
                            &user_id,
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

                        let ct = if context_token.is_empty() {
                            context_tokens
                                .lock()
                                .unwrap()
                                .get(&user_id)
                                .cloned()
                                .unwrap_or_default()
                        } else {
                            context_token
                        };
                        let ct_opt = if ct.is_empty() {
                            None
                        } else {
                            Some(ct.as_str())
                        };

                        // Start typing indicator before AI processing
                        let typing_stop = start_typing_loop(
                            api.clone(),
                            user_id.clone(),
                            ct_opt.map(|_| ct.clone()),
                            running.clone(),
                        );

                        let user_id_for_chunk = user_id.clone();
                        let user_id_for_state = user_id.clone();
                        let app_for_cb = app_handle.clone();
                        let state_for_run = user_states.clone();

                        let result = bridge.process_message_with_attachments_interruptible(
                            &channel_id,
                            &user_id,
                            &prompt_text,
                            &prompt_attachments,
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
                            |_| {},
                        );

                        // Stop typing indicator before sending reply
                        typing_stop.store(true, Ordering::SeqCst);
                        if ct_opt.is_some() {
                            let _ = rt.block_on(api.send_typing(&user_id, ct_opt, 0));
                        }

                        let has_pending_followup = {
                            let mut guard = match user_states.lock() {
                                Ok(guard) => guard,
                                Err(_) => break,
                            };
                            let state = match guard.get_mut(&user_id) {
                                Some(state) => state,
                                None => break,
                            };
                            state.running = false;
                            state.active_run = None;
                            !state.pending_texts.is_empty()
                        };

                        match result {
                            Ok(PiProcessOutcome::Completed(result)) => {
                                let full_text = result.full_text;
                                let usage = result.usage;
                                let usage_meta = result.usage_meta;
                                let is_control_command = result.control_command.is_some();
                                if has_pending_followup {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &user_id,
                                        "processing",
                                        "检测到用户追加消息，正在合并后重新处理",
                                    );
                                    continue;
                                }

                                let (text_reply, media_items) = split_text_and_media(&full_text);
                                let media_items = persist_outbound_media_items(
                                    agent_config.as_ref().map(|config| config.id.as_str()),
                                    &user_id,
                                    media_items,
                                );
                                let cleaned_text_reply =
                                    clean_media_fallback_text(&text_reply, !media_items.is_empty());
                                let display_reply =
                                    build_outbound_display_text(&cleaned_text_reply, &media_items);

                                if text_reply.is_empty() && media_items.is_empty() {
                                    log::warn!("pi 返回空回复");
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &user_id,
                                        "warn",
                                        "pi 返回了空回复，请检查 provider 配置",
                                    );
                                    emit_bot_message_with_usage(
                                        &app_handle,
                                        &channel_id,
                                        &user_id,
                                        "outbound_done",
                                        "",
                                        agent_config.as_ref(),
                                        usage.clone(),
                                        usage_meta.clone(),
                                    );
                                    cleanup_idle_user_state(&user_states, &user_id);
                                    break;
                                }
                                log::info!(
                                    "Worker 完成: 用户={} 回复 {} 字符",
                                    user_id,
                                    full_text.len()
                                );
                                dev_trace(
                                    "wechat",
                                    format!(
                                        "处理完成: user={} chars={} media_items={}",
                                        user_id,
                                        full_text.chars().count(),
                                        media_items.len()
                                    ),
                                );
                                emit_bot_status(
                                    &app_handle,
                                    &channel_id,
                                    &user_id,
                                    "done",
                                    &format!("回复 {} 字符完成", full_text.len()),
                                );
                                if !is_control_command {
                                    if let Some(agent_id) =
                                        agent_config.as_ref().map(|config| config.id.as_str())
                                    {
                                        let _ = agent_workspace::append_agent_memory_entry(
                                            agent_id,
                                            &user_id,
                                            &prompt_text,
                                            if display_reply.is_empty() {
                                                &full_text
                                            } else {
                                                &display_reply
                                            },
                                        );
                                    }
                                }
                                if !cleaned_text_reply.is_empty() {
                                    let cards =
                                        resolve_reply_card_items(&cleaned_text_reply, false);
                                    for seg in wechat_im_text_segments(&cards) {
                                        if !seg.is_empty() {
                                            send_reply_chunks(&rt, &api, &user_id, &seg, ct_opt);
                                        }
                                    }
                                }
                                for media in &media_items {
                                    if let Err(error) =
                                        send_media_item(&rt, &api, &user_id, media, ct_opt)
                                    {
                                        log::error!("发送媒体消息失败: {error}");
                                    }
                                }
                                emit_bot_message_with_usage(
                                    &app_handle,
                                    &channel_id,
                                    &user_id,
                                    "outbound_done",
                                    &display_reply,
                                    agent_config.as_ref(),
                                    usage,
                                    usage_meta,
                                );
                                cleanup_idle_user_state(&user_states, &user_id);
                                break;
                            }
                            Ok(PiProcessOutcome::Aborted) => {
                                if has_pending_followup {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &user_id,
                                        "processing",
                                        "收到用户新消息，正在中断上一轮并重新处理",
                                    );
                                    continue;
                                }
                                cleanup_idle_user_state(&user_states, &user_id);
                                break;
                            }
                            Err(e) => {
                                if has_pending_followup {
                                    emit_bot_status(
                                        &app_handle,
                                        &channel_id,
                                        &user_id,
                                        "warn",
                                        &format!("上一轮处理失败，已切换到用户最新消息: {e}"),
                                    );
                                    continue;
                                }

                                log::error!("pi 处理失败: {e}");
                                dev_trace(
                                    "wechat",
                                    format!("处理失败: user={} error={}", user_id, e),
                                );
                                emit_bot_status(
                                    &app_handle,
                                    &channel_id,
                                    &user_id,
                                    "error",
                                    &format!("pi 失败: {e}"),
                                );
                                let error_msg = format!("[处理失败: {e}]");
                                send_reply_chunks(&rt, &api, &user_id, &error_msg, ct_opt);
                                emit_bot_message(
                                    &app_handle,
                                    &channel_id,
                                    &user_id,
                                    "error",
                                    &error_msg,
                                    agent_config.as_ref(),
                                );
                                cleanup_idle_user_state(&user_states, &user_id);
                                break;
                            }
                        }
                    }
                }
            });
        }

        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        self.running.store(false, Ordering::SeqCst);
        *self.status.lock().unwrap() = ChannelStatus::Disconnected;
        Ok(())
    }

    fn send_message(&self, user_id: &str, content: &str) -> Result<(), String> {
        let api = WeChatApi::new(&self.base_url, &self.token, self.route_tag.as_deref());
        let ct = self.context_tokens.lock().unwrap().get(user_id).cloned();
        // Iterate by char boundary to avoid splitting multi-byte characters
        let chars: Vec<(usize, char)> = content.char_indices().collect();
        let mut start = 0;
        while start < chars.len() {
            let end_idx = (start + CHUNK_SIZE).min(chars.len());
            let byte_start = chars[start].0;
            let byte_end = if end_idx < chars.len() {
                chars[end_idx].0
            } else {
                content.len()
            };
            let chunk = &content[byte_start..byte_end];
            block_on_async(api.send_message(user_id, chunk, ct.as_deref()))?;
            start = end_idx;
        }
        Ok(())
    }

    fn send_media(&self, user_id: &str, media: &MediaPayload) -> Result<(), String> {
        let api = WeChatApi::new(&self.base_url, &self.token, self.route_tag.as_deref());
        let ct = self.context_tokens.lock().unwrap().get(user_id).cloned();
        let item_type = match media.media_type {
            MediaType::Image => MSG_ITEM_TYPE_IMAGE,
            MediaType::Video => MSG_ITEM_TYPE_VIDEO,
            MediaType::Audio | MediaType::File => MSG_ITEM_TYPE_FILE,
        };

        block_on_async(api.send_binary_media(
            user_id,
            item_type,
            &media.file_name,
            &media.data,
            ct.as_deref(),
        ))
    }

    fn status(&self) -> ChannelStatus {
        self.status.lock().unwrap().clone()
    }
}

// ── Free helper functions ──

fn block_on_async<F, T>(fut: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| format!("创建 tokio runtime 失败: {e}"))?;
            rt.block_on(fut)
        }
    }
}

fn generate_qr_data_uri(content: &str) -> Result<String, String> {
    use qrcode::QrCode;
    let code = QrCode::new(content.as_bytes()).map_err(|e| format!("生成二维码失败: {e}"))?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(256, 256)
        .quiet_zone(true)
        .build();
    Ok(format!(
        "data:image/svg+xml;utf8,{}",
        urlencoding::encode(&svg)
    ))
}

/// Start a periodic typing indicator loop for a user.
/// Returns an `Arc<AtomicBool>` that should be set to `true` to stop the loop.
fn start_typing_loop(
    api: WeChatApi,
    user_id: String,
    context_token: Option<String>,
    channel_running: Arc<AtomicBool>,
) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();

    thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return,
        };
        let ct_ref = context_token.as_deref();

        // Send initial typing indicator immediately
        let _ = rt.block_on(api.send_typing(&user_id, ct_ref, 1));

        // Keep refreshing every TYPING_INTERVAL_SECS until stopped
        while !stop_flag.load(Ordering::SeqCst) && channel_running.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_secs(TYPING_INTERVAL_SECS));
            if stop_flag.load(Ordering::SeqCst) || !channel_running.load(Ordering::SeqCst) {
                break;
            }
            let _ = rt.block_on(api.send_typing(&user_id, ct_ref, 1));
        }
    });

    stop
}

/// Send a reply to WeChat, splitting into ≤CHUNK_SIZE character chunks.
fn send_reply_chunks(
    rt: &tokio::runtime::Runtime,
    api: &WeChatApi,
    user_id: &str,
    content: &str,
    context_token: Option<&str>,
) {
    let mut start = 0;
    // Iterate over char boundaries to avoid splitting a multi-byte character
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    while start < chars.len() {
        let end_idx = (start + CHUNK_SIZE).min(chars.len());
        let byte_start = chars[start].0;
        let byte_end = if end_idx < chars.len() {
            chars[end_idx].0
        } else {
            content.len()
        };
        let chunk = &content[byte_start..byte_end];
        if let Err(e) = rt.block_on(api.send_message(user_id, chunk, context_token)) {
            log::error!("发送微信回复失败: {e}");
        }
        start = end_idx;
    }
}

fn send_media_item(
    rt: &tokio::runtime::Runtime,
    api: &WeChatApi,
    user_id: &str,
    media: &ParsedMediaItem,
    context_token: Option<&str>,
) -> Result<(), String> {
    let data = std::fs::read(&media.file_path)
        .map_err(|error| format!("读取媒体文件失败 {}: {error}", media.file_path))?;
    let item_type = match media.media_type {
        MediaType::Image => MSG_ITEM_TYPE_IMAGE,
        MediaType::Video => MSG_ITEM_TYPE_VIDEO,
        MediaType::Audio | MediaType::File => MSG_ITEM_TYPE_FILE,
    };

    rt.block_on(api.send_binary_media(user_id, item_type, &media.file_name, &data, context_token))
}

/// Emit a `bot://status` event to the frontend for diagnostic display.
fn emit_bot_status(app: &AppHandle, channel_id: &str, user_id: &str, level: &str, message: &str) {
    crate::emit_safe::emit_safe(app,
        "bot://status",
        serde_json::json!({
            "channelId": channel_id,
            "userId": user_id,
            "level": level,   // "processing" | "done" | "warn" | "error"
            "message": message,
            "timestamp": now_timestamp_ms(),
        }),
    );
}

/// Emit a `bot://message` event to the frontend for history tracking.
fn emit_bot_message(
    app: &AppHandle,
    channel_id: &str,
    user_id: &str,
    direction: &str,
    content: &str,
    agent: Option<&ConversationAgentConfig>,
) {
    emit_bot_message_with_usage(
        app, channel_id, user_id, direction, content, agent, None, None,
    )
}

fn emit_bot_message_with_usage(
    app: &AppHandle,
    channel_id: &str,
    user_id: &str,
    direction: &str,
    content: &str,
    agent: Option<&ConversationAgentConfig>,
    usage: Option<crate::PiTokenUsagePayload>,
    usage_meta: Option<crate::PiUsageMetadataPayload>,
) {
    let payload = BotMessage {
        channel_id: channel_id.to_string(),
        user_id: user_id.to_string(),
        direction: direction.to_string(),
        content: content.to_string(),
        timestamp: now_timestamp_ms(),
        agent: agent.cloned(),
        usage,
        usage_meta,
    };
    crate::emit_safe::emit_safe(app, "bot://message", &payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_flattened_image_fields() {
        let item = MessageItem {
            item_type: Some(MSG_ITEM_TYPE_IMAGE),
            extra: HashMap::from([
                ("image_url".to_string(), json!("https://example.com/a.png")),
                ("image_base64".to_string(), json!("Zm9v")),
            ]),
            ..Default::default()
        };

        let (base64_data, url_data) = extract_image_fields(&item);
        assert_eq!(base64_data.as_deref(), Some("Zm9v"));
        assert_eq!(url_data.as_deref(), Some("https://example.com/a.png"));
    }

    #[test]
    fn extracts_nested_file_fields_from_extra_container() {
        let item = MessageItem {
            item_type: Some(MSG_ITEM_TYPE_FILE),
            extra: HashMap::from([(
                "file_item".to_string(),
                json!({
                    "file_name": "report.pdf",
                    "download_url": "/media/report.pdf"
                }),
            )]),
            ..Default::default()
        };

        let (file_name, _base64_data, url_data) = extract_file_fields(&item);
        assert_eq!(file_name.as_deref(), Some("report.pdf"));
        assert_eq!(url_data.as_deref(), Some("/media/report.pdf"));
    }

    #[test]
    fn derives_file_name_from_url() {
        assert_eq!(
            file_name_from_url("https://example.com/files/%E6%8A%A5%E5%91%8A.pdf?sig=1").as_deref(),
            Some("报告.pdf")
        );
    }

    #[test]
    fn extracts_cdn_image_fields() {
        let item = MessageItem {
            item_type: Some(MSG_ITEM_TYPE_IMAGE),
            image_item: Some(ImageItem {
                aeskey: Some("00112233445566778899aabbccddeeff".to_string()),
                media: Some(CdnMedia {
                    encrypt_query_param: Some("encrypted-token".to_string()),
                    full_url: Some("https://cdn.example.com/download?id=1".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let cdn = extract_image_cdn_fields(&item).expect("cdn fields");
        assert_eq!(cdn.encrypt_query_param.as_deref(), Some("encrypted-token"));
        assert_eq!(
            cdn.full_url.as_deref(),
            Some("https://cdn.example.com/download?id=1")
        );
        assert_eq!(
            cdn.aes_key.as_deref(),
            Some("00112233445566778899aabbccddeeff")
        );
    }

    #[test]
    fn parses_wechat_aes_keys_from_hex_and_base64_hex() {
        let raw_hex = "00112233445566778899aabbccddeeff";
        let decoded_hex = parse_wechat_aes_key(raw_hex).expect("parse raw hex key");
        assert_eq!(decoded_hex, decode_hex_bytes(raw_hex).expect("hex decode"));

        let base64_of_hex = BASE64_ENGINE.encode(raw_hex.as_bytes());
        let decoded_base64_hex =
            parse_wechat_aes_key(&base64_of_hex).expect("parse base64-encoded hex key");
        assert_eq!(decoded_base64_hex, decoded_hex);
    }

    #[test]
    fn parses_media_directive_with_quoted_space_path() {
        let item = parse_media_directive(
            r#"::nc-media{type="file" path="/tmp/weekly report.pdf" name="weekly report.pdf"}"#,
        )
        .expect("media directive with spaces");

        assert!(matches!(item.media_type, MediaType::File));
        assert_eq!(item.file_path, "/tmp/weekly report.pdf");
        assert_eq!(item.file_name, "weekly report.pdf");
    }

    #[test]
    fn parses_percent_encoded_markdown_media_path() {
        let item = parse_markdown_media("[日报](/tmp/weekly%20report.pdf)")
            .expect("markdown media with encoded path");

        assert!(matches!(item.media_type, MediaType::File));
        assert_eq!(item.file_path, "/tmp/weekly report.pdf");
        assert_eq!(item.file_name, "weekly report.pdf");
    }

    #[test]
    fn parses_plain_bullet_media_path() {
        let item = parse_plain_path_media("- /Users/demo/Desktop/wechat_login_screenshot.png")
            .expect("plain path media");

        assert!(matches!(item.media_type, MediaType::Image));
        assert_eq!(
            item.file_path,
            "/Users/demo/Desktop/wechat_login_screenshot.png"
        );
        assert_eq!(item.file_name, "wechat_login_screenshot.png");
    }

    #[test]
    fn parses_plain_file_name_media_path() {
        let item =
            parse_plain_path_media("- wechat_login_screenshot.png").expect("plain file name media");

        assert!(matches!(item.media_type, MediaType::Image));
        assert_eq!(item.file_path, "wechat_login_screenshot.png");
        assert_eq!(item.file_name, "wechat_login_screenshot.png");
    }

    #[test]
    fn builds_outbound_display_text_with_encoded_media_directive() {
        let text = build_outbound_display_text(
            "请查收",
            &[ParsedMediaItem {
                media_type: MediaType::File,
                file_name: "weekly report.pdf".to_string(),
                file_path: "/tmp/weekly report.pdf".to_string(),
            }],
        );

        assert_eq!(
            text,
            "请查收\n::nc-media{type=\"file\" path=\"/tmp/weekly%20report.pdf\" name=\"weekly%20report.pdf\" label=\"%E6%96%87%E4%BB%B6\"}"
        );
    }
}
