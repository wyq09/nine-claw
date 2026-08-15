use crate::infer_media_mime_type;
use base64::{engine::general_purpose::STANDARD as BASE64_ENGINE, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

const MAX_INLINE_TEXT_LINES: usize = 2000;
const MAX_INLINE_TEXT_BYTES: usize = 50 * 1024;
const MAX_PDF_TEXT_PAGES: usize = 20;
const MAX_PDF_RENDER_PAGES: usize = 4;
const MAX_VIDEO_FRAMES: usize = 4;
const DERIVED_IMAGE_MAX_WIDTH: usize = 1280;

#[derive(Debug, Clone, Default, Deserialize)]
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
pub(crate) struct PromptImageInput {
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

#[derive(Debug, Clone)]
struct AudioTranscription {
    text: String,
    source_label: &'static str,
    lang: Option<String>,
    emotion: Option<String>,
    event: Option<String>,
    duration_secs: Option<f64>,
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

pub(crate) fn attachments_include_visual_context(attachments: &[PromptAttachmentInput]) -> bool {
    attachments.iter().any(|attachment| {
        is_image_attachment(attachment)
            || is_video_attachment(attachment)
            || is_pdf_attachment(attachment)
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
            let note = unsupported_attachment_note(
                attachment,
                "图片附件缺少本地文件路径，无法作为多模态输入发送。",
            );
            return Ok(Some(format!("<file name=\"{name_attr}\">{note}</file>")));
        }

        let resolved_path = Path::new(path);
        let bytes = fs::read(resolved_path)
            .map_err(|error| format!("读取图片附件失败 {}: {error}", resolved_path.display()))?;
        if bytes.is_empty() {
            let note =
                unsupported_attachment_note(attachment, "图片附件为空，无法作为多模态输入发送。");
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
        if let Some(transcript) = attachment_transcript(attachment) {
            return Ok(Some(render_file_block(
                &name_attr,
                &render_transcription_text(&transcript, path),
            )));
        }

        return Ok(None);
    }

    let resolved_path = Path::new(path);

    if is_pdf_attachment(attachment) {
        return build_pdf_attachment_block(attachment, resolved_path, &name_attr, images);
    }

    if is_audio_attachment(attachment) {
        return build_audio_attachment_block(attachment, resolved_path, &name_attr);
    }

    if is_video_attachment(attachment) {
        return build_video_attachment_block(attachment, resolved_path, &name_attr, images);
    }

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

fn build_pdf_attachment_block(
    attachment: &PromptAttachmentInput,
    resolved_path: &Path,
    name_attr: &str,
    images: &mut Vec<PromptImageInput>,
) -> Result<Option<String>, String> {
    if let Some(text) = extract_pdf_text_best_effort(resolved_path) {
        let inline_text = truncate_inline_text(&text, resolved_path.to_string_lossy().as_ref());
        return Ok(Some(render_file_block(name_attr, &inline_text)));
    }

    let rendered_pages = render_pdf_pages_best_effort(resolved_path, images);
    if rendered_pages > 0 {
        let body = format!(
            "未提取到可复制文本，已将前 {rendered_pages} 页作为图片输入发送给模型。\nPDF 路径：{}",
            resolved_path.display()
        );
        return Ok(Some(render_file_block(name_attr, &body)));
    }

    let note = unsupported_attachment_note(
        attachment,
        "PDF 附件已接收，但当前既无法提取文本，也无法渲染页面预览。",
    );
    Ok(Some(format!("<file name=\"{name_attr}\">{note}</file>")))
}

fn build_audio_attachment_block(
    attachment: &PromptAttachmentInput,
    resolved_path: &Path,
    name_attr: &str,
) -> Result<Option<String>, String> {
    if let Some(transcript) = attachment_transcript(attachment) {
        return Ok(Some(render_file_block(
            name_attr,
            &render_transcription_text(&transcript, resolved_path.to_string_lossy().as_ref()),
        )));
    }

    if let Some(transcript) = transcribe_media_best_effort(resolved_path) {
        return Ok(Some(render_file_block(
            name_attr,
            &render_transcription_text(&transcript, resolved_path.to_string_lossy().as_ref()),
        )));
    }

    let note = unsupported_attachment_note(attachment, "语音附件已接收，但当前没有可用转写结果。");
    Ok(Some(format!("<file name=\"{name_attr}\">{note}</file>")))
}

fn build_video_attachment_block(
    attachment: &PromptAttachmentInput,
    resolved_path: &Path,
    name_attr: &str,
    images: &mut Vec<PromptImageInput>,
) -> Result<Option<String>, String> {
    let frame_count = extract_video_frames_best_effort(resolved_path, images);
    let transcript =
        attachment_transcript(attachment).or_else(|| transcribe_media_best_effort(resolved_path));

    if frame_count == 0 && transcript.is_none() {
        let note = unsupported_attachment_note(
            attachment,
            "视频附件已接收，但当前无法提取画面或音轨转写。",
        );
        return Ok(Some(format!("<file name=\"{name_attr}\">{note}</file>")));
    }

    let mut parts = Vec::new();
    if frame_count > 0 {
        parts.push(format!(
            "已从视频中提取 {frame_count} 帧，并作为图片输入发送给模型。"
        ));
    }
    if let Some(transcript) = transcript {
        parts.push(format!(
            "视频音轨转写：\n{}",
            render_transcription_text(&transcript, resolved_path.to_string_lossy().as_ref())
        ));
    }
    parts.push(format!("原视频路径：{}", resolved_path.display()));

    Ok(Some(render_file_block(name_attr, &parts.join("\n\n"))))
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

fn normalized_attachment_signature(attachment: &PromptAttachmentInput) -> String {
    format!(
        "{} {} {}",
        attachment.kind.to_ascii_lowercase(),
        attachment.mime_type.to_ascii_lowercase(),
        attachment.file_path.to_ascii_lowercase()
    )
}

fn is_image_attachment(attachment: &PromptAttachmentInput) -> bool {
    let normalized = normalized_attachment_signature(attachment);
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
    let normalized = normalized_attachment_signature(attachment);
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
    let normalized = normalized_attachment_signature(attachment);
    normalized.contains("video/")
        || normalized.ends_with(".mp4")
        || normalized.ends_with(".mov")
        || normalized.ends_with(".webm")
        || normalized.ends_with(".m4v")
        || normalized.ends_with(".avi")
        || normalized.ends_with(".mkv")
}

fn is_pdf_attachment(attachment: &PromptAttachmentInput) -> bool {
    let normalized = normalized_attachment_signature(attachment);
    normalized.contains("application/pdf") || normalized.ends_with(".pdf")
}

fn is_text_attachment(attachment: &PromptAttachmentInput, bytes: &[u8]) -> bool {
    if is_audio_attachment(attachment)
        || is_video_attachment(attachment)
        || is_pdf_attachment(attachment)
    {
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

fn attachment_transcript(attachment: &PromptAttachmentInput) -> Option<AudioTranscription> {
    attachment
        .transcript
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|text| AudioTranscription {
            text: text.to_string(),
            source_label: "IM 转写",
            lang: None,
            emotion: None,
            event: None,
            duration_secs: None,
        })
}

fn transcribe_media_best_effort(path: &Path) -> Option<AudioTranscription> {
    match transcribe_media_with_coli(path) {
        Ok(result) => result,
        Err(error) => {
            log::warn!("附件本地 ASR 失败 {}: {error}", path.display());
            None
        }
    }
}

fn transcribe_media_with_coli(path: &Path) -> Result<Option<AudioTranscription>, String> {
    let output = match Command::new("coli")
        .args(["asr", "-j", "--model", "sensevoice"])
        .arg(path)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("执行 coli asr 失败: {error}")),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("退出码 {:?}", output.status.code())
        };
        return Err(format!("coli asr 执行失败: {details}"));
    }

    let json: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("解析 coli asr 输出失败: {error}"))?;
    let text = json
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    Ok(text.map(|text| AudioTranscription {
        text,
        source_label: "本地 ASR",
        lang: json
            .get("lang")
            .and_then(Value::as_str)
            .and_then(normalize_asr_tag),
        emotion: json
            .get("emotion")
            .and_then(Value::as_str)
            .and_then(normalize_asr_tag),
        event: json
            .get("event")
            .and_then(Value::as_str)
            .and_then(normalize_asr_tag),
        duration_secs: json.get("duration").and_then(Value::as_f64),
    }))
}

fn normalize_asr_tag(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_start_matches("<|").trim_end_matches("|>");
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn render_transcription_text(transcript: &AudioTranscription, path: &str) -> String {
    let mut body = truncate_inline_text(&transcript.text, path);
    let mut metadata = Vec::new();
    metadata.push(format!("来源：{}", transcript.source_label));
    if let Some(lang) = transcript.lang.as_deref() {
        metadata.push(format!("语言：{lang}"));
    }
    if let Some(emotion) = transcript.emotion.as_deref() {
        metadata.push(format!("情绪：{emotion}"));
    }
    if let Some(event) = transcript.event.as_deref() {
        metadata.push(format!("事件：{event}"));
    }
    if let Some(duration_secs) = transcript.duration_secs {
        metadata.push(format!("时长：{duration_secs:.1}s"));
    }

    if !metadata.is_empty() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(&format!("[{}]", metadata.join(" · ")));
    }
    body
}

fn render_file_block(name_attr: &str, body: &str) -> String {
    format!("<file name=\"{name_attr}\">\n{body}\n</file>")
}

fn extract_pdf_text_best_effort(path: &Path) -> Option<String> {
    match extract_pdf_text(path) {
        Ok(result) => result,
        Err(error) => {
            log::warn!("PDF 文本提取失败 {}: {error}", path.display());
            None
        }
    }
}

fn extract_pdf_text(path: &Path) -> Result<Option<String>, String> {
    let output = match Command::new("pdftotext")
        .args([
            "-enc",
            "UTF-8",
            "-nopgbrk",
            "-f",
            "1",
            "-l",
            &MAX_PDF_TEXT_PAGES.to_string(),
        ])
        .arg(path)
        .arg("-")
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("执行 pdftotext 失败: {error}")),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("pdftotext 退出码 {:?}", output.status.code())
        } else {
            format!("pdftotext 失败: {stderr}")
        });
    }

    let text = decode_text_bytes(&output.stdout);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

fn render_pdf_pages_best_effort(path: &Path, images: &mut Vec<PromptImageInput>) -> usize {
    match render_pdf_pages(path, images) {
        Ok(count) => count,
        Err(error) => {
            log::warn!("PDF 页面渲染失败 {}: {error}", path.display());
            0
        }
    }
}

fn render_pdf_pages(path: &Path, images: &mut Vec<PromptImageInput>) -> Result<usize, String> {
    let temp_dir = create_temporary_artifact_dir("pdf-pages")?;
    let prefix = temp_dir.join("page");

    let result = (|| -> Result<usize, String> {
        let output = match Command::new("pdftoppm")
            .args(["-f", "1", "-l", &MAX_PDF_RENDER_PAGES.to_string(), "-png"])
            .arg(path)
            .arg(&prefix)
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(format!("执行 pdftoppm 失败: {error}")),
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                format!("pdftoppm 退出码 {:?}", output.status.code())
            } else {
                format!("pdftoppm 失败: {stderr}")
            });
        }

        let mut page_paths = fs::read_dir(&temp_dir)
            .map_err(|error| format!("读取 PDF 渲染目录失败: {error}"))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|candidate| {
                candidate
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("png"))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        page_paths.sort();

        let mut count = 0usize;
        for page_path in page_paths {
            let bytes = fs::read(&page_path).map_err(|error| {
                format!("读取 PDF 页面预览失败 {}: {error}", page_path.display())
            })?;
            if bytes.is_empty() {
                continue;
            }
            images.push(PromptImageInput {
                content_type: "image".to_string(),
                data: BASE64_ENGINE.encode(bytes),
                mime_type: "image/png".to_string(),
            });
            count += 1;
        }

        Ok(count)
    })();

    let _ = fs::remove_dir_all(&temp_dir);
    result
}

fn extract_video_frames_best_effort(path: &Path, images: &mut Vec<PromptImageInput>) -> usize {
    match extract_video_frames(path, images) {
        Ok(count) => count,
        Err(error) => {
            log::warn!("视频抽帧失败 {}: {error}", path.display());
            0
        }
    }
}

pub(crate) fn extract_video_frames(path: &Path, images: &mut Vec<PromptImageInput>) -> Result<usize, String> {
    let temp_dir = create_temporary_artifact_dir("video-frames")?;
    let duration_secs = video_duration_seconds(path).unwrap_or(None);
    let timestamps = compute_video_frame_timestamps(duration_secs);

    let result = {
        let mut count = 0usize;
        for (index, timestamp) in timestamps.iter().enumerate() {
            let frame_path = temp_dir.join(format!("frame-{index:02}.jpg"));
            match extract_video_frame(path, &frame_path, *timestamp) {
                Ok(Some(bytes)) => {
                    images.push(PromptImageInput {
                        content_type: "image".to_string(),
                        data: BASE64_ENGINE.encode(bytes),
                        mime_type: "image/jpeg".to_string(),
                    });
                    count += 1;
                }
                Ok(None) => {}
                Err(error) => {
                    log::warn!(
                        "视频抽帧单帧失败 {} @ {:.2}s: {error}",
                        path.display(),
                        timestamp
                    );
                }
            }
        }

        Ok(count)
    };

    let _ = fs::remove_dir_all(&temp_dir);
    result
}

fn video_duration_seconds(path: &Path) -> Result<Option<f64>, String> {
    let output = match Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("执行 ffprobe 失败: {error}")),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("ffprobe 退出码 {:?}", output.status.code())
        } else {
            format!("ffprobe 失败: {stderr}")
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    trimmed
        .parse::<f64>()
        .map(Some)
        .map_err(|error| format!("解析 ffprobe 时长失败: {error}"))
}

fn compute_video_frame_timestamps(duration_secs: Option<f64>) -> Vec<f64> {
    let mut timestamps = match duration_secs {
        Some(duration) if duration.is_finite() && duration > 0.0 => {
            let frame_count = if duration < 8.0 {
                2
            } else if duration < 30.0 {
                3
            } else {
                MAX_VIDEO_FRAMES
            };

            (0..frame_count)
                .map(|index| {
                    let ratio = (index + 1) as f64 / (frame_count + 1) as f64;
                    let timestamp = duration * ratio;
                    timestamp.min((duration - 0.1).max(0.0))
                })
                .collect::<Vec<_>>()
        }
        _ => vec![0.0],
    };

    timestamps.dedup_by(|left, right| (*left - *right).abs() < 0.05);
    timestamps
}

fn extract_video_frame(
    path: &Path,
    output_path: &Path,
    timestamp: f64,
) -> Result<Option<Vec<u8>>, String> {
    let timestamp_arg = format!("{timestamp:.3}");
    let output = match Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-ss", &timestamp_arg, "-i"])
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-q:v",
            "3",
            "-vf",
            &format!("scale={DERIVED_IMAGE_MAX_WIDTH}:-1:force_original_aspect_ratio=decrease"),
        ])
        .arg(output_path)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("执行 ffmpeg 失败: {error}")),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("ffmpeg 退出码 {:?}", output.status.code())
        } else {
            format!("ffmpeg 失败: {stderr}")
        });
    }

    if !output_path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(output_path)
        .map_err(|error| format!("读取视频帧失败 {}: {error}", output_path.display()))?;
    if bytes.is_empty() {
        Ok(None)
    } else {
        Ok(Some(bytes))
    }
}

fn create_temporary_artifact_dir(prefix: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir()
        .join("nineclaw-derived-attachments")
        .join(format!("{prefix}-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).map_err(|error| format!("创建临时附件目录失败: {error}"))?;
    Ok(dir)
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
        let addition_bytes = addition.len();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_inline_media_directives_only() {
        assert_eq!(
            strip_nc_media_directive_lines(
                "hello\n::nc-media{type=\"image\" path=\"/tmp/a.png\"}\nworld"
            ),
            "hello\nworld"
        );
    }

    #[test]
    fn transcript_only_audio_attachment_stays_inline() {
        let attachment = PromptAttachmentInput {
            file_name: "voice.mp3".to_string(),
            file_path: String::new(),
            mime_type: "audio/mpeg".to_string(),
            kind: "audio".to_string(),
            transcript: Some("你好，NineClaw".to_string()),
        };

        let mut images = Vec::new();
        let block = build_attachment_block(&attachment, &mut images)
            .unwrap()
            .unwrap();

        assert!(block.contains("你好，NineClaw"));
        assert!(block.contains("IM 转写"));
        assert!(images.is_empty());
    }

    #[test]
    fn detects_pdf_attachment_by_extension() {
        let attachment = PromptAttachmentInput {
            file_path: "/tmp/report.pdf".to_string(),
            ..PromptAttachmentInput::default()
        };
        assert!(is_pdf_attachment(&attachment));
    }

    #[test]
    fn computes_multiple_video_frame_timestamps_for_long_video() {
        let timestamps = compute_video_frame_timestamps(Some(40.0));
        assert_eq!(timestamps.len(), MAX_VIDEO_FRAMES);
        assert!(timestamps.iter().all(|value| *value > 0.0));
    }

    #[test]
    fn detects_visual_context_for_image_and_pdf_attachments() {
        let image = PromptAttachmentInput {
            file_name: "screen.png".to_string(),
            file_path: "/tmp/screen.png".to_string(),
            mime_type: "image/png".to_string(),
            kind: "image".to_string(),
            transcript: None,
        };
        let pdf = PromptAttachmentInput {
            file_name: "report.pdf".to_string(),
            file_path: "/tmp/report.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            kind: "file".to_string(),
            transcript: None,
        };
        let audio = PromptAttachmentInput {
            file_name: "voice.mp3".to_string(),
            file_path: "/tmp/voice.mp3".to_string(),
            mime_type: "audio/mpeg".to_string(),
            kind: "audio".to_string(),
            transcript: None,
        };

        assert!(attachments_include_visual_context(&[image]));
        assert!(attachments_include_visual_context(&[pdf]));
        assert!(!attachments_include_visual_context(&[audio]));
    }
}
