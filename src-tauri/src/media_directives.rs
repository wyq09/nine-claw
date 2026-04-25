use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MediaDirectiveFields {
    pub media_type: Option<String>,
    pub path: String,
    pub name: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkdownMediaReference {
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlainMediaPathReference {
    pub path: String,
}

fn decode_directive_value(value: &str) -> String {
    urlencoding::decode(value)
        .map(|decoded| decoded.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

fn encode_directive_value(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

fn encode_directive_path_value(value: &str) -> String {
    encode_directive_value(value)
        .replace("%2F", "/")
        .replace("%3A", ":")
        .replace("%5C", "\\")
}

fn parse_attributes(body: &str) -> Vec<(String, String)> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let key_start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            continue;
        }

        let key = body[key_start..index].trim();
        index += 1;

        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if key.is_empty() || index >= bytes.len() {
            continue;
        }

        let value = if matches!(bytes[index], b'"' | b'\'') {
            let quote = bytes[index];
            index += 1;
            let value_start = index;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            let value = &body[value_start..index];
            if index < bytes.len() && bytes[index] == quote {
                index += 1;
            }
            value
        } else {
            let value_start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            &body[value_start..index]
        };

        out.push((key.to_string(), decode_directive_value(value.trim())));
    }

    out
}

pub(crate) fn parse_media_directive_fields(line: &str) -> Option<MediaDirectiveFields> {
    let trimmed = line.trim();
    if !trimmed.starts_with("::nc-media{") || !trimmed.ends_with('}') {
        return None;
    }

    let body = &trimmed["::nc-media{".len()..trimmed.len() - 1];
    let mut media_type = None;
    let mut path = None;
    let mut name = None;
    let mut label = None;

    for (key, value) in parse_attributes(body) {
        match key.as_str() {
            "type" => media_type = Some(value),
            "path" => path = Some(value),
            "name" => name = Some(value),
            "label" => label = Some(value),
            _ => {}
        }
    }

    Some(MediaDirectiveFields {
        media_type,
        path: path?,
        name,
        label,
    })
}

pub(crate) fn parse_markdown_media_reference(line: &str) -> Option<MarkdownMediaReference> {
    let trimmed = line.trim();
    let normalized = trimmed.strip_prefix('!').unwrap_or(trimmed);
    if !normalized.starts_with('[') {
        return None;
    }

    let close_label = normalized.find("](")?;
    let end = normalized.rfind(')')?;
    if close_label <= 1 || end <= close_label + 2 {
        return None;
    }

    let label = normalized[1..close_label].trim();
    let raw_path = normalized[close_label + 2..end].trim();
    if raw_path.is_empty() {
        return None;
    }

    Some(MarkdownMediaReference {
        label: label.to_string(),
        path: decode_directive_value(raw_path),
    })
}

pub(crate) fn parse_plain_media_path_reference(line: &str) -> Option<PlainMediaPathReference> {
    let mut candidate = line.trim();
    if candidate.is_empty() {
        return None;
    }

    for prefix in ["- ", "* ", "• "] {
        if let Some(rest) = candidate.strip_prefix(prefix) {
            candidate = rest.trim();
            break;
        }
    }

    if let Some(dot_index) = candidate.find(". ") {
        let (head, rest) = candidate.split_at(dot_index);
        if !head.is_empty() && head.chars().all(|char| char.is_ascii_digit()) {
            candidate = rest[2..].trim();
        }
    }

    for (open, close) in [('`', '`'), ('"', '"'), ('\'', '\'')] {
        if candidate.starts_with(open) && candidate.ends_with(close) && candidate.len() >= 2 {
            candidate = candidate[1..candidate.len() - 1].trim();
            break;
        }
    }

    let path = decode_directive_value(candidate);
    if path.is_empty() {
        return None;
    }

    let has_reasonable_extension = Path::new(&path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|extension| !extension.is_empty() && extension.len() <= 10)
        .unwrap_or(false);
    let looks_absolute = has_reasonable_extension
        && (path.starts_with('/')
            || (path.len() >= 3
                && path.as_bytes()[1] == b':'
                && matches!(path.as_bytes()[2], b'/' | b'\\')
                && path.as_bytes()[0].is_ascii_alphabetic()));
    let looks_relative_path = has_reasonable_extension
        && (path.starts_with("./")
            || path.starts_with("../")
            || path.contains('/')
            || path.contains('\\'));
    let looks_file_name = has_reasonable_extension
        && path.chars().all(|char| {
            char.is_ascii_alphanumeric()
                || matches!(char, '.' | '_' | '-' | ' ' | '(' | ')' | '[' | ']')
        });
    if !(looks_absolute || looks_relative_path || looks_file_name) {
        return None;
    }

    Some(PlainMediaPathReference { path })
}

pub(crate) fn build_media_directive_line(
    media_type: &str,
    path: &str,
    name: Option<&str>,
    label: Option<&str>,
) -> String {
    let mut parts = vec![
        format!("type=\"{}\"", encode_directive_value(media_type)),
        format!("path=\"{}\"", encode_directive_path_value(path)),
    ];

    if let Some(name) = name.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("name=\"{}\"", encode_directive_value(name)));
    }

    if let Some(label) = label.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("label=\"{}\"", encode_directive_value(label)));
    }

    format!("::nc-media{{{}}}", parts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_directive_with_quoted_space_path() {
        let parsed = parse_media_directive_fields(
            r#"::nc-media{type="file" path="/tmp/weekly report.pdf" name="weekly report.pdf" label="文件"}"#,
        )
        .expect("directive");

        assert_eq!(parsed.media_type.as_deref(), Some("file"));
        assert_eq!(parsed.path, "/tmp/weekly report.pdf");
        assert_eq!(parsed.name.as_deref(), Some("weekly report.pdf"));
        assert_eq!(parsed.label.as_deref(), Some("文件"));
    }

    #[test]
    fn decodes_percent_encoded_directive_values() {
        let parsed = parse_media_directive_fields(
            r#"::nc-media{type="file" path="/tmp/weekly%20report.pdf" name="weekly%20report.pdf"}"#,
        )
        .expect("directive");

        assert_eq!(parsed.path, "/tmp/weekly report.pdf");
        assert_eq!(parsed.name.as_deref(), Some("weekly report.pdf"));
    }

    #[test]
    fn decodes_percent_encoded_markdown_path() {
        let parsed =
            parse_markdown_media_reference("[日报](/tmp/weekly%20report.pdf)").expect("markdown");

        assert_eq!(parsed.label, "日报");
        assert_eq!(parsed.path, "/tmp/weekly report.pdf");
    }

    #[test]
    fn builds_directive_line_with_encoded_values() {
        let line = build_media_directive_line(
            "file",
            "/tmp/weekly report.pdf",
            Some("weekly report.pdf"),
            Some("文件"),
        );

        assert_eq!(
            line,
            "::nc-media{type=\"file\" path=\"/tmp/weekly%20report.pdf\" name=\"weekly%20report.pdf\" label=\"%E6%96%87%E4%BB%B6\"}"
        );
    }

    #[test]
    fn parses_plain_bullet_media_path_reference() {
        let parsed = parse_plain_media_path_reference("- /tmp/weekly report.pdf").expect("path");
        assert_eq!(parsed.path, "/tmp/weekly report.pdf");
    }

    #[test]
    fn parses_plain_quoted_media_path_reference() {
        let parsed =
            parse_plain_media_path_reference("`/Users/demo/Desktop/test image.png`").expect("path");
        assert_eq!(parsed.path, "/Users/demo/Desktop/test image.png");
    }

    #[test]
    fn does_not_parse_plain_instruction_text_with_slash_as_path() {
        assert!(parse_plain_media_path_reference("压缩/裁剪这张").is_none());
    }

    #[test]
    fn does_not_parse_slash_command_help_as_plain_media_path() {
        assert!(parse_plain_media_path_reference("/new - 开启一个新的会话").is_none());
        assert!(parse_plain_media_path_reference("/help - 显示可用指令").is_none());
    }
}
