//! PI session sanitizing and quarantining for provider replay state.
//!
//! Some providers reject history that carries `reasoning_content` /
//! `thinkingSignature` metadata back to the API. These helpers strip that
//! state from JSONL session files (with a backup), or quarantine the whole
//! file when rewriting is not safe. Moved out of `lib.rs` to keep that file
//! within its size ratchet.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn strip_provider_replay_state(value: &mut serde_json::Value) -> bool {
    let mut changed = false;
    match value {
        serde_json::Value::Object(map) => {
            for key in [
                "reasoning_content",
                "reasoningContent",
                "reasoning_details",
                "reasoningDetails",
                "reasoning",
                "thinkingSignature",
                "thoughtSignature",
            ] {
                if map.remove(key).is_some() {
                    changed = true;
                }
            }

            for child in map.values_mut() {
                if strip_provider_replay_state(child) {
                    changed = true;
                }
            }
        }
        serde_json::Value::Array(items) => {
            let original_len = items.len();
            items.retain(|item| {
                let is_thinking = item
                    .get("type")
                    .and_then(|kind| kind.as_str())
                    .map(|kind| {
                        matches!(
                            kind,
                            "thinking"
                                | "reasoning"
                                | "reasoning_content"
                                | "reasoningContent"
                                | "redacted_thinking"
                        )
                    })
                    .unwrap_or(false);
                !is_thinking
            });
            if items.len() != original_len {
                changed = true;
            }
            for child in items {
                if strip_provider_replay_state(child) {
                    changed = true;
                }
            }
        }
        _ => {}
    }
    changed
}

pub(crate) fn sanitize_pi_session_replay_state(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(path)
        .map_err(|error| format!("读取 pi session 以修复 reasoning 历史失败: {error}"))?;
    let mut changed = false;
    let mut sanitized_lines = Vec::new();

    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(mut value) => {
                if strip_provider_replay_state(&mut value) {
                    changed = true;
                }
                sanitized_lines.push(
                    serde_json::to_string(&value)
                        .map_err(|error| format!("序列化修复后的 pi session 失败: {error}"))?,
                );
            }
            Err(_) => sanitized_lines.push(line.to_string()),
        }
    }

    if !changed {
        return Ok(false);
    }

    let backup_path = path.with_extension(format!(
        "jsonl.reasoning-bak-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    ));
    crate::runtime_paths::write_private_file(&backup_path, raw.as_bytes()).map_err(|error| {
        format!(
            "备份污染的 pi session 失败 {}: {error}",
            backup_path.display()
        )
    })?;

    let mut sanitized = sanitized_lines.join("\n");
    sanitized.push('\n');
    crate::runtime_paths::write_private_file(path, sanitized.as_bytes())
        .map_err(|error| format!("写回修复后的 pi session 失败 {}: {error}", path.display()))?;
    Ok(true)
}

pub(crate) fn quarantine_pi_session_file(path: &Path, reason: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let backup_path = path.with_extension(format!(
        "jsonl.quarantine-{}-{}",
        reason
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .trim_matches('-'),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    ));
    fs::rename(path, &backup_path).map_err(|error| {
        format!(
            "隔离污染的 pi session 失败 {} -> {}: {error}",
            path.display(),
            backup_path.display()
        )
    })?;
    Ok(true)
}

pub(crate) fn is_provider_reasoning_history_rejection_error(error: &str) -> bool {
    let lower = error.trim().to_ascii_lowercase();
    (lower.contains("reasoning_content")
        || lower.contains("reasoning content")
        || lower.contains("thinking mode")
        || lower.contains("reasoning mode"))
        && (lower.contains("must be passed back")
            || lower.contains("pass back")
            || lower.contains("missing")
            || lower.contains("required"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn detects_provider_reasoning_history_rejection() {
        assert!(is_provider_reasoning_history_rejection_error(
            "400 The reasoning_content in the thinking mode must be passed back to the API."
        ));
        assert!(!is_provider_reasoning_history_rejection_error(
            "400 invalid api key"
        ));
    }

    #[test]
    fn sanitize_pi_session_replay_state_strips_reasoning_metadata() {
        let path = std::env::temp_dir().join(format!(
            "nineclaw-session-sanitize-test-{}.jsonl",
            Uuid::new_v4()
        ));
        fs::write(
            &path,
            r#"{"type":"message","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hidden","thinkingSignature":"reasoning_content"},{"type":"text","text":"visible"}],"reasoning_content":"opaque","reasoning_details":[{"x":1}]}}"#,
        )
        .expect("write contaminated session");

        let changed = sanitize_pi_session_replay_state(&path).expect("sanitize session");
        let sanitized = fs::read_to_string(&path).expect("read sanitized");
        let _ = fs::remove_file(&path);

        assert!(changed);
        assert!(sanitized.contains("visible"));
        assert!(!sanitized.contains("reasoning_content"));
        assert!(!sanitized.contains("thinkingSignature"));
        assert!(!sanitized.contains(r#""type":"thinking""#));
    }

    #[test]
    fn quarantine_pi_session_file_moves_contaminated_session_as_backup() {
        let path = std::env::temp_dir().join(format!(
            "nineclaw-session-quarantine-test-{}.jsonl",
            Uuid::new_v4()
        ));
        fs::write(&path, "{}\n").expect("write session");

        let changed = quarantine_pi_session_file(&path, "reasoning-history").expect("quarantine");
        assert!(changed);
        assert!(!path.exists());

        let parent = path.parent().expect("temp parent");
        let file_name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("file stem");
        let backup = fs::read_dir(parent)
            .expect("read temp")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|candidate| {
                candidate
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(|name| name.starts_with(file_name) && name.contains("quarantine"))
                    .unwrap_or(false)
            })
            .expect("backup exists");
        let _ = fs::remove_file(backup);
    }
}
