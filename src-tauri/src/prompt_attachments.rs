use crate::infer_media_mime_type;
use base64::{engine::general_purpose::STANDARD as BASE64_ENGINE, Engine as _};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const MAX_INLINE_TEXT_LINES: usize = 2000;
const MAX_INLINE_TEXT_BYTES: usize = 50 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptAttachmentInput {
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub transcript: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptImageInput {
    #[serde(rename = "type")]
    pub content_type: String,
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Clone)]
pub struct PreparedPromptInput {
    pub message: String,
    pub images: Vec<PromptImageInput>,
}

pub fn prepare_prompt_input(
    message: &str,
    attachments: &[PromptAttachmentInput],
) -> Result<PreparedPromptInput, String> {
    let normalized_message = if attachments.is_empty() {
        message.trim().to_string()
    } else {
        strip_nc_media_directive_lines(message)
    };

    if attachments.is_empty() {
        return Ok(PreparedPromptInput {
            message: normalized_message,
            images: Vec::new(),
        });
    }

    let mut sections = Vec::new();
    if !normalized_message.is_empty() {
        sections.push(normalized_message);
    }

    let mut images = Vec::new();
    for attachment in attachments {
        if let Some(block) = build_attachment_block(attachment, &mut images)? {
            sections.push(block);
        }
    }

    Ok(PreparedPromptInput {
        message: sections.join("\n\n").trim().to_string(),
        images,
    })
}

fn strip_nc_media_directive_lines(message: &str) -> String {
    message
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with("::nc-media{") && trimmed.ends_with('}'))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn build_attachment_block(
    attachment: &PromptAttachmentInput,
    images: &mut Vec<PromptImageInput>,
) -> Result<Option<String>, String> {
    let path = attachment.file_path.trim();
    let file_name = attachment_label(attachment);
    let name_attr = escape_xml_attr(&file_name);

    if is_image_attachment(attachment) {
        if path.is_empty() {
            let note = unsupported_attachment_note(attachment, "图片附件缺少本地文件路径，无法作为多模态输入发送。");
            return Ok(Some(format!("<file name=\"{name_attr}\">{note}</file>")));
        }

        let resolved_path = Path::new(path);
        let bytes = fs::read(resolved_path)
            .map_err(|error| format!("读取图片附件失败 {}: {error}", resolved_path.display()))?;
        if bytes.is_empty() {
            let note = unsupported_attachment_note(attachment, "图片附件为空，无法作为多模态输入发送。");
            return Ok(Some(format!("<file name=\"{name_attr}\">{note}</file>")));
        }

        images.push(PromptImageInput {
            content_type: "image".to_string(),
            data: BASE64_ENGINE.encode(bytes),
            mime_type: normalized_mime_type(attachment, resolved_path),
        });

        return Ok(Some(format!("<file name=\"{name_attr}\"></file>")));
    }

    if path.is_empty() {
        let transcript = attachment
            .transcript
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(transcript) = transcript {
            return Ok(Some(format!(
                "<file name=\"{name_attr}\">\n{}\n</file>",
                transcript
            )));
        }

        return Ok(None);
    }

    let resolved_path = Path::new(path);
    let bytes = fs::read(resolved_path)
        .map_err(|error| format!("读取附件失败 {}: {error}", resolved_path.display()))?;

    if is_text_attachment(attachment, &bytes) {
        let text = decode_text_bytes(&bytes);
        let inline_text = truncate_inline_text(&text, path);
        return Ok(Some(format!(
            "<file name=\"{name_attr}\">\n{}\n</file>",
            inline_text
        )));
    }

    let note = unsupported_attachment_note(
        attachment,
        "当前 runtime 只能把图片作为多模态输入直接发送给模型；该附件会以说明文字提供给模型。",
    );
    Ok(Some(format!("<file name=\"{name_attr}\">{note}</file>")))
}

fn attachment_label(attachment: &PromptAttachmentInput) -> String {
    let file_name = attachment.file_name.trim();
    if !file_name.is_empty() {
        return file_name.to_string();
    }

    let file_path = attachment.file_path.trim();
    if !file_path.is_empty() {
        return file_path.to_string();
    }

    "attachment".to_string()
}

fn normalized_mime_type(attachment: &PromptAttachmentInput, path: &Path) -> String {
    infer_media_mime_type(
        path,
        Some(attachment.mime_type.trim()).filter(|value| !value.is_empty()),
    )
}

fn is_image_attachment(attachment: &PromptAttachmentInput) -> bool {
    let normalized = format!(
        "{} {} {}",
        attachment.kind.to_ascii_lowercase(),
        attachment.mime_type.to_ascii_lowercase(),
        attachment.file_path.to_ascii_lowercase()
    );
    normalized.contains("image/")
        || normalized.contains(" image ")
        || normalized.ends_with(".png")
        || normalized.ends_with(".jpg")
        || normalized.ends_with(".jpeg")
        || normalized.ends_with(".gif")
        || normalized.ends_with(".webp")
        || normalized.ends_with(".bmp")
        || normalized.ends_with(".svg")
}

fn is_audio_attachment(attachment: &PromptAttachmentInput) -> bool {
    let normalized = format!(
        "{} {} {}",
        attachment.kind.to_ascii_lowercase(),
        attachment.mime_type.to_ascii_lowercase(),
        attachment.file_path.to_ascii_lowercase()
    );
    normalized.contains("audio/")
        || normalized.contains(" voice ")
        || normalized.ends_with(".mp3")
        || normalized.ends_with(".wav")
        || normalized.ends_with(".m4a")
        || normalized.ends_with(".aac")
        || normalized.ends_with(".ogg")
        || normalized.ends_with(".opus")
        || normalized.ends_with(".amr")
        || normalized.ends_with(".silk")
}

fn is_video_attachment(attachment: &PromptAttachmentInput) -> bool {
    let normalized = format!(
        "{} {} {}",
        attachment.kind.to_ascii_lowercase(),
        attachment.mime_type.to_ascii_lowercase(),
        attachment.file_path.to_ascii_lowercase()
    );
    normalized.contains("video/")
        || normalized.ends_with(".mp4")
        || normalized.ends_with(".mov")
        || normalized.ends_with(".webm")
        || normalized.ends_with(".m4v")
        || normalized.ends_with(".avi")
        || normalized.ends_with(".mkv")
}

fn is_text_attachment(attachment: &PromptAttachmentInput, bytes: &[u8]) -> bool {
    if is_audio_attachment(attachment) || is_video_attachment(attachment) {
        return false;
    }

    let mime = attachment.mime_type.trim().to_ascii_lowercase();
    if mime.starts_with("text/") {
        return true;
    }

    if matches!(
        mime.as_str(),
        "application/json"
            | "application/ld+json"
            | "application/xml"
            | "application/javascript"
            | "application/x-javascript"
            | "application/x-sh"
            | "application/x-shellscript"
            | "application/x-yaml"
            | "application/yaml"
            | "application/toml"
            | "application/x-toml"
            | "application/csv"
            | "application/sql"
    ) {
        return true;
    }

    let extension = Path::new(attachment.file_path.trim())
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if matches!(
        extension.as_deref(),
        Some(
            "txt"
                | "md"
                | "markdown"
                | "json"
                | "jsonl"
                | "yaml"
                | "yml"
                | "toml"
                | "ini"
                | "cfg"
                | "conf"
                | "log"
                | "csv"
                | "tsv"
                | "xml"
                | "html"
                | "htm"
                | "css"
                | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "mjs"
                | "cjs"
                | "py"
                | "rs"
                | "go"
                | "java"
                | "kt"
                | "swift"
                | "sh"
                | "zsh"
                | "bash"
                | "sql"
                | "env"
                | "properties"
        )
    ) {
        return true;
    }

    looks_like_text_bytes(bytes)
}

fn looks_like_text_bytes(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }

    if bytes.contains(&0) {
        return false;
    }

    std::str::from_utf8(bytes).is_ok()
}

fn decode_text_bytes(bytes: &[u8]) -> String {
    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => text,
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn truncate_inline_text(text: &str, path: &str) -> String {
    let mut output = String::new();
    let mut output_lines = 0usize;
    let mut output_bytes = 0usize;
    let total_lines = text.lines().count().max(1);
    let mut truncated = false;

    for line in text.lines() {
        if output_lines >= MAX_INLINE_TEXT_LINES {
            truncated = true;
            break;
        }

        let separator = if output.is_empty() { "" } else { "\n" };
        let addition = format!("{separator}{line}");
        let addition_bytes = addition.as_bytes().len();
        if output_bytes + addition_bytes > MAX_INLINE_TEXT_BYTES {
            truncated = true;
            break;
        }

        output.push_str(&addition);
        output_lines += 1;
        output_bytes += addition_bytes;
    }

    if truncated {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(&format!(
            "[文件内容已截断：当前仅包含前 {output_lines} 行 / {}KB。需要更多内容时，请继续用 read 工具读取路径：{path}]",
            MAX_INLINE_TEXT_BYTES / 1024
        ));
    } else if output_lines < total_lines {
        output.push_str(&format!(
            "\n\n[该文件还有剩余内容，继续用 read 工具读取路径：{path}]"
        ));
    }

    output
}

fn unsupported_attachment_note(attachment: &PromptAttachmentInput, prefix: &str) -> String {
    let mut parts = vec![prefix.to_string()];

    if !attachment.file_path.trim().is_empty() {
        parts.push(format!("文件路径：{}", attachment.file_path.trim()));
    }

    if let Some(transcript) = attachment
        .transcript
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("已有转写：{transcript}"));
    }

    escape_xml_text(&parts.join("\n"))
}

fn escape_xml_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
