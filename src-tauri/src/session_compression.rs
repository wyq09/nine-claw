#![allow(dead_code)]

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::Path;

const DEFAULT_TOKEN_THRESHOLD: u64 = 150_000;
const DEFAULT_MESSAGE_COUNT_THRESHOLD: usize = 200;
const DEFAULT_TARGET_COMPRESSED_TOKENS: u64 = 10_000;
const DEFAULT_MAX_RECENT_MESSAGES: usize = 20;
const DEFAULT_IDLE_TOKEN_THRESHOLD: u64 = 20_000;
const TOOL_RESULT_MAX_CHARS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompressionConfig {
    pub token_threshold: u64,
    pub message_count_threshold: usize,
    pub target_compressed_tokens: u64,
    pub max_recent_messages: usize,
    pub idle_compression_enabled: bool,
    pub idle_compression_delay_ms: u64,
    pub idle_token_threshold: u64,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            token_threshold: DEFAULT_TOKEN_THRESHOLD,
            message_count_threshold: DEFAULT_MESSAGE_COUNT_THRESHOLD,
            target_compressed_tokens: DEFAULT_TARGET_COMPRESSED_TOKENS,
            max_recent_messages: DEFAULT_MAX_RECENT_MESSAGES,
            idle_compression_enabled: true,
            idle_compression_delay_ms: 90_000,
            idle_token_threshold: DEFAULT_IDLE_TOKEN_THRESHOLD,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompressionReason {
    TokenThreshold,
    MessageCountThreshold,
    Idle,
    ModelSwitch,
}

#[derive(Debug, Clone)]
pub(crate) struct CompressionPlan {
    pub reason: CompressionReason,
    pub compression_level: u64,
    pub original_token_count: u64,
    pub original_message_count: usize,
    pub recent_entries: Vec<Value>,
    pub archive_entries: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompressionArchiveInfo {
    pub path: String,
    pub topics: Option<String>,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CacheMetrics {
    pub total_api_calls: u64,
    pub total_cache_hits: u64,
    pub total_cache_misses: u64,
    pub total_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub average_cache_hit_rate: f64,
}

pub(crate) fn load_session_entries(session_path: &Path) -> Result<Vec<Value>, String> {
    if !session_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(session_path)
        .map_err(|error| format!("读取 PI session 失败 {}: {error}", session_path.display()))?;
    Ok(content
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect())
}

pub(crate) fn plan_compression(
    entries: &[Value],
    used_tokens: u64,
    config: &CompressionConfig,
    force_idle: bool,
) -> Option<CompressionPlan> {
    let message_entries = message_entries(entries);
    let message_count = message_entries.len();
    let reason = if force_idle {
        if !config.idle_compression_enabled
            || message_count <= config.max_recent_messages
            || used_tokens < config.idle_token_threshold
        {
            return None;
        }
        CompressionReason::Idle
    } else if used_tokens >= config.token_threshold {
        CompressionReason::TokenThreshold
    } else if message_count >= config.message_count_threshold {
        CompressionReason::MessageCountThreshold
    } else {
        return None;
    };

    let compression_level = next_compression_level(entries);
    let recent_entries =
        select_recent_with_tool_pairs(&message_entries, config.max_recent_messages);
    let recent_ids = recent_entries
        .iter()
        .filter_map(entry_id)
        .collect::<HashSet<_>>();
    let archive_entries = message_entries
        .into_iter()
        .filter(|entry| {
            entry_id(entry)
                .map(|id| !recent_ids.contains(&id))
                .unwrap_or(true)
        })
        .filter(|entry| !is_system_injected_message(entry) && !is_compressed_summary_message(entry))
        .collect::<Vec<_>>();
    if archive_entries.is_empty() {
        return None;
    }

    Some(CompressionPlan {
        reason,
        compression_level,
        original_token_count: used_tokens,
        original_message_count: message_count,
        recent_entries,
        archive_entries,
    })
}

pub(crate) fn plan_model_switch_compression(
    entries: &[Value],
    used_tokens: u64,
    config: &CompressionConfig,
) -> Option<CompressionPlan> {
    let estimated_tokens = estimate_entries_tokens(entries);
    let effective_tokens = used_tokens.max(estimated_tokens);
    let message_entries = message_entries(entries);
    if effective_tokens <= config.target_compressed_tokens
        || message_entries.len() <= config.max_recent_messages
    {
        return None;
    }

    let compression_level = next_compression_level(entries);
    let recent_entries =
        select_recent_with_tool_pairs(&message_entries, config.max_recent_messages);
    let recent_ids = recent_entries
        .iter()
        .filter_map(entry_id)
        .collect::<HashSet<_>>();
    let archive_entries = message_entries
        .into_iter()
        .filter(|entry| {
            entry_id(entry)
                .map(|id| !recent_ids.contains(&id))
                .unwrap_or(true)
        })
        .filter(|entry| !is_system_injected_message(entry) && !is_compressed_summary_message(entry))
        .collect::<Vec<_>>();
    if archive_entries.is_empty() {
        return None;
    }

    Some(CompressionPlan {
        reason: CompressionReason::ModelSwitch,
        compression_level,
        original_token_count: effective_tokens,
        original_message_count: recent_entries.len() + archive_entries.len(),
        recent_entries,
        archive_entries,
    })
}

pub(crate) fn estimate_entries_tokens(entries: &[Value]) -> u64 {
    entries
        .iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("message"))
        .map(estimate_entry_tokens)
        .sum()
}

pub(crate) fn build_compression_instruction(level: u64, config: &CompressionConfig) -> Value {
    json!({
        "role": "user",
        "content": format_compression_prompt(level, config),
        "system_injected": true,
        "metadata": {
            "system_injected": true,
            "purpose": "insert_then_compress",
            "compression_level": level
        }
    })
}

pub(crate) fn write_chunk_archive(
    root: &Path,
    session_id: &str,
    chunk_index: u64,
    plan: &CompressionPlan,
    topics: Option<&str>,
) -> Result<CompressionArchiveInfo, String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("创建压缩归档目录失败 {}: {error}", root.display()))?;
    let path = root.join(format!(
        "{}-chunk-{:04}.md",
        safe_file_stem(session_id),
        chunk_index
    ));
    let markdown = build_chunk_markdown(session_id, chunk_index, plan, topics);
    fs::write(&path, markdown)
        .map_err(|error| format!("写入压缩归档失败 {}: {error}", path.display()))?;
    Ok(CompressionArchiveInfo {
        path: path.to_string_lossy().to_string(),
        topics: topics.map(ToOwned::to_owned),
        message_count: plan.archive_entries.len(),
    })
}

pub(crate) fn append_compaction_entry(
    session_path: &Path,
    summary: &str,
    plan: &CompressionPlan,
    archive: Option<&CompressionArchiveInfo>,
) -> Result<(), String> {
    let first_kept_entry_id = plan
        .recent_entries
        .first()
        .and_then(entry_id)
        .ok_or_else(|| "压缩后没有可保留的 recent entry".to_string())?;
    let leaf_id = load_session_entries(session_path)?
        .iter()
        .rev()
        .find_map(entry_id);
    let entry = json!({
        "type": "compaction",
        "id": format!("nc-compact-{}", Utc::now().timestamp_millis()),
        "parentId": leaf_id,
        "timestamp": Utc::now().to_rfc3339(),
        "summary": strip_topics(summary),
        "firstKeptEntryId": first_kept_entry_id,
        "tokensBefore": plan.original_token_count,
        "details": {
            "strategy": "insert_then_compress",
            "compressionLevel": plan.compression_level,
            "archive": archive,
        },
        "fromHook": true
    });
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(session_path)
        .map_err(|error| format!("打开 PI session 失败 {}: {error}", session_path.display()))?;
    writeln!(file, "{entry}").map_err(|error| {
        format!(
            "追加 PI compaction 失败 {}: {error}",
            session_path.display()
        )
    })
}

pub(crate) fn parse_topics(content: &str) -> Option<String> {
    let start = content.find("<topics>")? + "<topics>".len();
    let end = content.find("</topics>")?;
    let topics = content[start..end].trim();
    if topics.is_empty() {
        None
    } else {
        Some(topics.to_string())
    }
}

pub(crate) fn update_cache_metrics(
    previous: &CacheMetrics,
    input_tokens: u64,
    cache_read: u64,
) -> CacheMetrics {
    let total_api_calls = previous.total_api_calls + 1;
    let total_input_tokens = previous.total_input_tokens + input_tokens;
    let cached_input_tokens = previous.cached_input_tokens + cache_read;
    CacheMetrics {
        total_api_calls,
        total_cache_hits: previous.total_cache_hits + u64::from(cache_read > 0),
        total_cache_misses: previous.total_cache_misses + u64::from(cache_read == 0),
        total_input_tokens,
        cached_input_tokens,
        average_cache_hit_rate: if total_input_tokens == 0 {
            0.0
        } else {
            cached_input_tokens as f64 / total_input_tokens as f64
        },
    }
}

fn message_entries(entries: &[Value]) -> Vec<Value> {
    entries
        .iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("message"))
        .cloned()
        .collect()
}

fn estimate_entry_tokens(entry: &Value) -> u64 {
    let text = entry
        .get("message")
        .map(message_text)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| entry.to_string());
    text.chars().count().div_ceil(4) as u64
}

fn message_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.get("text")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| item.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

fn next_compression_level(entries: &[Value]) -> u64 {
    entries
        .iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("compaction"))
        .filter_map(|entry| {
            entry
                .get("details")
                .and_then(|details| details.get("compressionLevel"))
                .and_then(Value::as_u64)
        })
        .max()
        .unwrap_or(0)
        + 1
}

fn select_recent_with_tool_pairs(entries: &[Value], max_recent: usize) -> Vec<Value> {
    let mut include = HashSet::new();
    let mut collected = 0usize;
    let mut index = entries.len();
    while index > 0 && collected < max_recent {
        index -= 1;
        include.insert(index);
        collected += 1;

        let call_ids = tool_call_ids(&entries[index]);
        if !call_ids.is_empty() {
            for result_index in tool_results_after(entries, index, &call_ids) {
                include.insert(result_index);
            }
        }
        let result_ids = tool_result_ids(&entries[index]);
        if !result_ids.is_empty() {
            if let Some(assistant_index) = assistant_before(entries, index, &result_ids) {
                if include.insert(assistant_index) {
                    collected += 1;
                }
                for result_index in tool_results_after(
                    entries,
                    assistant_index,
                    &tool_call_ids(&entries[assistant_index]),
                ) {
                    include.insert(result_index);
                }
            }
        }
    }
    let mut indexes = include.into_iter().collect::<Vec<_>>();
    indexes.sort_unstable();
    indexes
        .into_iter()
        .map(|idx| entries[idx].clone())
        .collect()
}

fn tool_results_after(
    entries: &[Value],
    assistant_index: usize,
    call_ids: &[String],
) -> Vec<usize> {
    let call_ids = call_ids.iter().cloned().collect::<HashSet<_>>();
    let mut out = Vec::new();
    for (offset, entry) in entries.iter().enumerate().skip(assistant_index + 1) {
        let result_ids = tool_result_ids(entry);
        if result_ids.is_empty() {
            break;
        }
        if result_ids.iter().any(|id| call_ids.contains(id)) {
            out.push(offset);
        }
    }
    out
}

fn assistant_before(entries: &[Value], tool_index: usize, result_ids: &[String]) -> Option<usize> {
    let result_ids = result_ids.iter().cloned().collect::<HashSet<_>>();
    entries
        .iter()
        .enumerate()
        .take(tool_index)
        .rev()
        .find(|(_, entry)| {
            entry
                .get("message")
                .and_then(|message| message.get("role"))
                .and_then(Value::as_str)
                == Some("assistant")
                && tool_call_ids(entry)
                    .iter()
                    .any(|id| result_ids.contains(id))
        })
        .map(|(index, _)| index)
}

fn tool_call_ids(entry: &Value) -> Vec<String> {
    let Some(content) = entry
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("toolCall"))
        .filter_map(|block| {
            block
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn tool_result_ids(entry: &Value) -> Vec<String> {
    let Some(message) = entry.get("message") else {
        return Vec::new();
    };
    if message.get("role").and_then(Value::as_str) == Some("toolResult") {
        return message
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|block| {
                block
                    .get("toolUseId")
                    .or_else(|| block.get("tool_use_id"))
                    .or_else(|| block.get("toolCallId"))
                    .or_else(|| block.get("tool_call_id"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect();
    }
    Vec::new()
}

fn entry_id(entry: &Value) -> Option<String> {
    entry
        .get("id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn is_system_injected_message(entry: &Value) -> bool {
    entry
        .get("message")
        .and_then(|message| message.get("system_injected"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn is_compressed_summary_message(entry: &Value) -> bool {
    entry
        .get("message")
        .and_then(|message| message.get("compressed_summary"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn format_compression_prompt(level: u64, config: &CompressionConfig) -> String {
    crate::prompts::build_session_compression_prompt(
        level,
        config.target_compressed_tokens,
        compression_level_guidance(level),
    )
}

fn compression_level_guidance(level: u64) -> &'static str {
    match level {
        1 => "Level 1 要求：详细摘要。\n必须包含关键文件列表、技术决策、工具使用、错误与修复、当前任务状态。",
        2 => "Level 2 要求：简洁摘要。\n只保留关键文件、关键结果、主要未完成事项和可复用约束。",
        3 => "Level 3 要求：最小摘要。\n只保留文件/任务数量级统计、当前进行中工作、继续任务必需的少量上下文。",
        _ => "Level 4+ 要求：超精简一行。\n格式示例：Progress: X tasks, Y files. Recent: tool_a, tool_b.",
    }
}

fn build_chunk_markdown(
    session_id: &str,
    chunk_index: u64,
    plan: &CompressionPlan,
    topics: Option<&str>,
) -> String {
    let mut lines = vec![
        "---".to_string(),
        format!("session_id: {}", json!(session_id)),
        format!("chunk: {chunk_index}"),
        format!("compression_level: {}", plan.compression_level),
        format!("archived_at: {}", Utc::now().to_rfc3339()),
        format!("message_count: {}", plan.archive_entries.len()),
    ];
    if let Some(topics) = topics {
        lines.push(format!("topics: {}", json!(topics)));
    }
    lines.extend([
        "---".to_string(),
        String::new(),
        format!("# Session Chunk {chunk_index}"),
        String::new(),
        "> 本文件包含压缩时归档的原始对话。".to_string(),
        "> 可通过 file_reader 工具召回特定细节。".to_string(),
        String::new(),
    ]);
    for entry in &plan.archive_entries {
        append_entry_markdown(&mut lines, entry);
    }
    lines.join("\n")
}

fn append_entry_markdown(lines: &mut Vec<String>, entry: &Value) {
    let Some(message) = entry.get("message") else {
        return;
    };
    match message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "user" => {
            lines.push("## User".to_string());
            lines.push(String::new());
            lines.push(message_content_text(message));
            lines.push(String::new());
        }
        "assistant" => {
            lines.push("## Assistant".to_string());
            lines.push(String::new());
            lines.push(message_content_text(message));
            lines.push(String::new());
        }
        "toolResult" => {
            lines.push("### Tool Result: tool".to_string());
            lines.push(String::new());
            lines.push("```".to_string());
            lines.push(truncate(
                &message_content_text(message),
                TOOL_RESULT_MAX_CHARS,
            ));
            lines.push("```".to_string());
            lines.push(String::new());
        }
        _ => {}
    }
}

fn message_content_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn strip_topics(content: &str) -> String {
    let mut out = content.to_string();
    if let (Some(start), Some(end)) = (out.find("<topics>"), out.find("</topics>")) {
        let end = end + "</topics>".len();
        out.replace_range(start..end, "");
    }
    out.trim().to_string()
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let head = value.chars().take(max).collect::<String>();
    format!(
        "{head}\n... [truncated, {} chars total]",
        value.chars().count()
    )
}

fn safe_file_stem(value: &str) -> String {
    let stem = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(120)
        .collect::<String>();
    if stem.is_empty() {
        "session".to_string()
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            let path =
                std::env::temp_dir().join(format!("nineclaw-session-compression-test-{nonce}"));
            fs::create_dir_all(&path).expect("create tempdir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn message_entry(id: &str, role: &str, content: Value) -> Value {
        json!({
            "type": "message",
            "id": id,
            "parentId": null,
            "timestamp": "2026-05-20T00:00:00Z",
            "message": {
                "role": role,
                "content": content,
            }
        })
    }

    #[test]
    fn plans_threshold_compression_and_preserves_tool_pairs() {
        let entries = vec![
            message_entry("u1", "user", json!([{ "type": "text", "text": "old" }])),
            message_entry(
                "a1",
                "assistant",
                json!([{ "type": "toolCall", "id": "call-1", "name": "read", "arguments": {} }]),
            ),
            message_entry(
                "t1",
                "toolResult",
                json!([{ "type": "tool_result", "toolUseId": "call-1", "text": "result" }]),
            ),
            message_entry("u2", "user", json!([{ "type": "text", "text": "new" }])),
        ];
        let config = CompressionConfig {
            message_count_threshold: 4,
            max_recent_messages: 2,
            ..CompressionConfig::default()
        };

        let plan = plan_compression(&entries, 10, &config, false).expect("plan");

        assert_eq!(plan.reason, CompressionReason::MessageCountThreshold);
        assert_eq!(plan.archive_entries.len(), 1);
        assert_eq!(
            plan.recent_entries
                .iter()
                .filter_map(entry_id)
                .collect::<Vec<_>>(),
            vec!["a1", "t1", "u2"]
        );
    }

    #[test]
    fn plans_idle_compression_only_after_idle_threshold() {
        let entries = vec![
            message_entry("u1", "user", json!([{ "type": "text", "text": "old" }])),
            message_entry(
                "a1",
                "assistant",
                json!([{ "type": "text", "text": "reply" }]),
            ),
            message_entry("u2", "user", json!([{ "type": "text", "text": "new" }])),
        ];
        let config = CompressionConfig {
            max_recent_messages: 1,
            idle_token_threshold: 100,
            ..CompressionConfig::default()
        };

        assert!(plan_compression(&entries, 99, &config, true).is_none());
        let plan = plan_compression(&entries, 100, &config, true).expect("idle plan");

        assert_eq!(plan.reason, CompressionReason::Idle);
        assert_eq!(plan.original_token_count, 100);
    }

    #[test]
    fn plans_model_switch_compression_above_target() {
        let entries = vec![
            message_entry(
                "u1",
                "user",
                json!([{ "type": "text", "text": "x".repeat(80) }]),
            ),
            message_entry(
                "a1",
                "assistant",
                json!([{ "type": "text", "text": "y".repeat(80) }]),
            ),
            message_entry(
                "u2",
                "user",
                json!([{ "type": "text", "text": "z".repeat(80) }]),
            ),
        ];
        let config = CompressionConfig {
            target_compressed_tokens: 10,
            max_recent_messages: 1,
            ..CompressionConfig::default()
        };

        let plan = plan_model_switch_compression(&entries, 0, &config).expect("model switch plan");

        assert_eq!(plan.reason, CompressionReason::ModelSwitch);
        assert_eq!(plan.archive_entries.len(), 2);
        assert!(plan.original_token_count > config.target_compressed_tokens);
    }

    #[test]
    fn compression_instruction_uses_external_template_and_level_guidance() {
        let config = CompressionConfig {
            target_compressed_tokens: 8_192,
            ..CompressionConfig::default()
        };

        let level_1 = build_compression_instruction(1, &config);
        let level_4 = build_compression_instruction(4, &config);

        let level_1_content = level_1
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let level_4_content = level_4
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();

        assert!(level_1_content.contains("本次压缩级别：Level 1"));
        assert!(level_1_content.contains("目标摘要规模：约 8192 tokens 以内"));
        assert!(level_1_content.contains("Level 1 要求：详细摘要"));
        assert!(level_4_content.contains("Level 4+ 要求：超精简一行"));
        assert_eq!(
            level_1
                .get("metadata")
                .and_then(|metadata| metadata.get("purpose"))
                .and_then(Value::as_str),
            Some("insert_then_compress")
        );
    }

    #[test]
    fn writes_chunk_and_appends_compaction_entry() {
        let dir = TempDir::new();
        let session_path = dir.path().join("session.jsonl");
        let entries = vec![
            message_entry("u1", "user", json!([{ "type": "text", "text": "old" }])),
            message_entry("u2", "user", json!([{ "type": "text", "text": "new" }])),
        ];
        fs::write(
            &session_path,
            entries
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .expect("write session");
        let config = CompressionConfig {
            message_count_threshold: 2,
            max_recent_messages: 1,
            ..CompressionConfig::default()
        };
        let plan = plan_compression(&entries, 20, &config, false).expect("plan");
        let topics = parse_topics("<topics>测试,压缩</topics><summary>summary</summary>");
        let archive = write_chunk_archive(dir.path(), "session/1", 1, &plan, topics.as_deref())
            .expect("archive");

        append_compaction_entry(
            &session_path,
            "<topics>测试,压缩</topics><summary>summary</summary>",
            &plan,
            Some(&archive),
        )
        .expect("append");

        let content = fs::read_to_string(&session_path).expect("read");
        assert!(content.contains("\"type\":\"compaction\""));
        assert!(content.contains("\"strategy\":\"insert_then_compress\""));
        assert!(PathBuf::from(archive.path).exists());
    }

    #[test]
    fn updates_cache_metrics() {
        let next = update_cache_metrics(
            &CacheMetrics {
                total_api_calls: 0,
                total_cache_hits: 0,
                total_cache_misses: 0,
                total_input_tokens: 0,
                cached_input_tokens: 0,
                average_cache_hit_rate: 0.0,
            },
            100,
            60,
        );
        assert_eq!(next.total_api_calls, 1);
        assert_eq!(next.total_cache_hits, 1);
        assert_eq!(next.average_cache_hit_rate, 0.6);
    }
}
