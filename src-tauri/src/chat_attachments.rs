use crate::agent_workspace;
use base64::{engine::general_purpose::STANDARD as BASE64_ENGINE, Engine as Base64Engine};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAttachmentUpload {
    pub file_name: String,
    pub mime_type: Option<String>,
    pub data_base64: Option<String>,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedChatAttachment {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub mime_type: String,
    pub size: u64,
    pub kind: String,
}

pub fn persist_chat_attachments(
    agent_id: &str,
    session_id: Option<&str>,
    uploads: Vec<ChatAttachmentUpload>,
) -> Result<Vec<PersistedChatAttachment>, String> {
    let trimmed_agent_id = agent_id.trim();
    if trimmed_agent_id.is_empty() {
        return Err("缺少可写入附件的智能体 ID".to_string());
    }
    if uploads.is_empty() {
        return Ok(Vec::new());
    }

    let scope = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("desktop");

    let mut persisted = Vec::new();
    for upload in uploads {
        let prepared = prepare_upload(upload)?;
        let path = agent_workspace::persist_agent_inbound_artifact(
            trimmed_agent_id,
            scope,
            &prepared.file_name,
            &prepared.data,
        )?;
        agent_workspace::register_agent_attachment_source(
            trimmed_agent_id,
            &prepared.file_name,
            &path,
            prepared.mime_type.as_deref(),
            Some(&format!("session={scope}")),
        )?;
        let mime_type = prepared.mime_type.unwrap_or_default();
        let kind = infer_attachment_kind(path.to_string_lossy().as_ref(), &mime_type).to_string();

        persisted.push(PersistedChatAttachment {
            id: Uuid::new_v4().to_string(),
            file_name: prepared.file_name,
            file_path: path.display().to_string(),
            mime_type,
            size: prepared.data.len() as u64,
            kind,
        });
    }

    Ok(persisted)
}

struct PreparedUpload {
    file_name: String,
    mime_type: Option<String>,
    data: Vec<u8>,
}

fn prepare_upload(upload: ChatAttachmentUpload) -> Result<PreparedUpload, String> {
    let source_path = upload
        .source_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let file_name = if upload.file_name.trim().is_empty() {
        source_path
            .and_then(|value| Path::new(value).file_name())
            .and_then(|value| value.to_str())
            .unwrap_or("attachment.bin")
            .to_string()
    } else {
        upload.file_name.trim().to_string()
    };

    let data = if let Some(source_path) = source_path {
        fs::read(source_path)
            .map_err(|error| format!("读取附件源文件失败 {}: {error}", source_path))?
    } else if let Some(data_base64) = upload
        .data_base64
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        BASE64_ENGINE
            .decode(data_base64)
            .map_err(|error| format!("解码附件 base64 失败: {error}"))?
    } else {
        return Err(format!("附件 `{file_name}` 缺少可导入的数据"));
    };

    Ok(PreparedUpload {
        file_name,
        mime_type: upload
            .mime_type
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        data,
    })
}

fn infer_attachment_kind(reference: &str, mime_type: &str) -> &'static str {
    let normalized = format!("{} {}", reference.to_lowercase(), mime_type.to_lowercase());
    if normalized.contains("image/")
        || normalized.ends_with(".png")
        || normalized.ends_with(".jpg")
        || normalized.ends_with(".jpeg")
        || normalized.ends_with(".gif")
        || normalized.ends_with(".webp")
        || normalized.ends_with(".bmp")
        || normalized.ends_with(".svg")
    {
        "image"
    } else if normalized.contains("video/")
        || normalized.ends_with(".mp4")
        || normalized.ends_with(".mov")
        || normalized.ends_with(".webm")
        || normalized.ends_with(".m4v")
        || normalized.ends_with(".avi")
        || normalized.ends_with(".mkv")
    {
        "video"
    } else if normalized.contains("audio/")
        || normalized.ends_with(".mp3")
        || normalized.ends_with(".wav")
        || normalized.ends_with(".m4a")
        || normalized.ends_with(".aac")
        || normalized.ends_with(".ogg")
        || normalized.ends_with(".opus")
        || normalized.ends_with(".amr")
    {
        "audio"
    } else {
        "file"
    }
}
