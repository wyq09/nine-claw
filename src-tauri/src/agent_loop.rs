//! Agent Loop marker parsing engine.
//!
//! Provides functions for extracting, parsing, stripping, and formatting the
//! structured markers (`NC_AGENT_LOOP_*`) that the LLM emits to control the
//! agent loop runtime.

use crate::agent_loop_types::{
    AgentLoopBatchResult, AgentLoopBatchMarker, AgentLoopCallMarker,
    AgentLoopExtendMarker, AgentLoopResult, ParsedLoopMarker,
};

use chrono::Local;

// ---------------------------------------------------------------------------
// Marker constants
// ---------------------------------------------------------------------------

/// Marker for a single agent call.
pub const MARKER_CALL: &str = "NC_AGENT_LOOP_CALL_JSON:";
/// Marker for a batch of concurrent agent calls.
pub const MARKER_BATCH: &str = "NC_AGENT_LOOP_BATCH_JSON:";
/// Marker to request an iteration-limit extension.
pub const MARKER_EXTEND: &str = "NC_AGENT_LOOP_EXTEND_JSON:";
/// Marker signalling the loop should terminate.
pub const MARKER_FINAL: &str = "NC_AGENT_LOOP_FINAL:";
/// Marker carrying a single agent result back to the LLM.
pub const MARKER_RESULT: &str = "NC_AGENT_LOOP_RESULT_JSON:";

/// All prefix markers (everything except FINAL which has no JSON payload).
const LOOP_MARKERS: &[&str] = &[MARKER_BATCH, MARKER_CALL, MARKER_EXTEND];

// ---------------------------------------------------------------------------
// extract_first_loop_marker
// ---------------------------------------------------------------------------

/// Search `text` for the first agent-loop marker and return the parsed marker
/// together with the **byte offset** of the marker's start position.
///
/// Search order: `BATCH` first (since `CALL` is a textual subset of `BATCH`),
/// then `CALL`, then `EXTEND`.
pub fn extract_first_loop_marker(text: &str) -> Option<(ParsedLoopMarker, usize)> {
    // Collect candidates: (byte_offset, marker_tag)
    let mut candidates: Vec<(usize, &str)> = Vec::new();

    for &marker in LOOP_MARKERS {
        if let Some(pos) = text.find(marker) {
            candidates.push((pos, marker));
        }
    }

    // Pick the earliest occurrence.
    candidates.sort_by_key(|(pos, _)| *pos);
    let (offset, tag) = candidates.first()?;

    let json_start = *offset + tag.len();
    let remaining = &text[json_start..];
    let value = parse_json_after_marker(remaining)?;

    match *tag {
        MARKER_BATCH => {
            let batch: AgentLoopBatchMarker = serde_json::from_value(value).ok()?;
            Some((ParsedLoopMarker::Batch(batch), *offset))
        }
        MARKER_CALL => {
            let call: AgentLoopCallMarker = serde_json::from_value(value).ok()?;
            Some((ParsedLoopMarker::Call(call), *offset))
        }
        MARKER_EXTEND => {
            let extend: AgentLoopExtendMarker = serde_json::from_value(value).ok()?;
            Some((ParsedLoopMarker::Extend(extend), *offset))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// parse_json_after_marker
// ---------------------------------------------------------------------------

/// Starting from the text immediately after a marker tag, find the first `{`,
/// then match braces (respecting strings and escape sequences) to extract a
/// complete JSON value.
pub fn parse_json_after_marker(remaining: &str) -> Option<serde_json::Value> {
    let start = remaining.find('{')?;
    let bytes = remaining.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut i = start;

    while i < bytes.len() {
        let ch = bytes[i];
        if in_string {
            if ch == b'\\' {
                // Skip escaped character.
                i += 2;
                continue;
            }
            if ch == b'"' {
                in_string = false;
            }
        } else {
            match ch {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let json_str = &remaining[start..=i];
                        return serde_json::from_str(json_str).ok();
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }

    None
}

// ---------------------------------------------------------------------------
// has_final_marker
// ---------------------------------------------------------------------------

/// Return `true` if `text` contains `NC_AGENT_LOOP_FINAL:`.
pub fn has_final_marker(text: &str) -> bool {
    text.contains(MARKER_FINAL)
}

// ---------------------------------------------------------------------------
// strip_loop_markers
// ---------------------------------------------------------------------------

/// Remove every line that contains any `NC_AGENT_LOOP` marker, then collapse
/// consecutive blank lines into a single blank line and trim trailing blanks.
pub fn strip_loop_markers(text: &str) -> String {
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            !line.contains(MARKER_CALL)
                && !line.contains(MARKER_BATCH)
                && !line.contains(MARKER_EXTEND)
                && !line.contains(MARKER_FINAL)
                && !line.contains(MARKER_RESULT)
        })
        .collect();

    // Collapse runs of blank lines.
    let mut result = String::new();
    let mut prev_blank = false;
    for line in &kept {
        let is_blank = line.trim().is_empty();
        if is_blank && prev_blank {
            continue; // skip consecutive blank
        }
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(line);
        prev_blank = is_blank;
    }

    result
}

// ---------------------------------------------------------------------------
// format_single_result / format_batch_result
// ---------------------------------------------------------------------------

/// Format a single agent-loop result as a single-line marker string.
pub fn format_single_result(result: &AgentLoopResult) -> String {
    let json = serde_json::to_string(result).unwrap_or_else(|_| "{}".into());
    format!("{}{}\n", MARKER_RESULT, json)
}

/// Format a batch result as a single-line marker string.
pub fn format_batch_result(result: &AgentLoopBatchResult) -> String {
    let json = serde_json::to_string(result).unwrap_or_else(|_| "{}".into());
    format!("{}{}\n", MARKER_RESULT, json)
}

// ---------------------------------------------------------------------------
// Defense layer: heal_orphaned_tool_calls
// ---------------------------------------------------------------------------

/// Scan `history` for assistant messages whose `tool_calls` entries have no
/// matching `tool` result (identified by `tool_call_id`). For every orphan,
/// inject a synthetic tool-result message immediately after the assistant
/// message so the conversation history remains well-formed for the LLM.
pub fn heal_orphaned_tool_calls(history: &mut Vec<serde_json::Value>) {
    // Collect the IDs of all tool results already present.
    let mut answered_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for msg in history.iter() {
        if msg.get("role").and_then(|r| r.as_str()) == Some("tool") {
            if let Some(id) = msg.get("tool_call_id").and_then(|v| v.as_str()) {
                answered_ids.insert(id.to_string());
            }
        }
    }

    // Walk assistant messages and find orphaned tool_calls.
    // We collect insertions as (index_after_assistant, synthetic_value) and
    // apply them in reverse order so indices remain valid.
    let mut insertions: Vec<(usize, serde_json::Value)> = Vec::new();

    for (i, msg) in history.iter().enumerate() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let tool_calls = match msg.get("tool_calls").and_then(|v| v.as_array()) {
            Some(arr) if !arr.is_empty() => arr,
            _ => continue,
        };

        for tc in tool_calls {
            let tc_id = match tc.get("id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };
            if !answered_ids.contains(&tc_id) {
                let synthetic = serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tc_id,
                    "content": "[NineClaw] 此工具调用因会话中断未完成，请根据已有信息继续。"
                });
                answered_ids.insert(tc_id);
                insertions.push((i + 1, synthetic));
            }
        }
    }

    // Apply in reverse order to keep indices stable.
    for (idx, val) in insertions.into_iter().rev() {
        history.insert(idx, val);
    }
}

// ---------------------------------------------------------------------------
// Defense layer: inject_dynamic_context
// ---------------------------------------------------------------------------

/// Find the **last** message with `"role": "system"` in `history` and append
/// a dynamic context block (current time + permission denials) to its
/// `content`. Appending at the end protects the KV Cache prefix.
pub fn inject_dynamic_context(
    history: &mut Vec<serde_json::Value>,
    permission_denials: &[String],
) {
    let now_str = Local::now().format("%Y-%m-%d %H:%M %Z").to_string();
    let denials_text = if permission_denials.is_empty() {
        "无".to_string()
    } else {
        permission_denials.join("，")
    };

    let context_block = format!(
        "\n\n[动态上下文]\n当前时间：{}\n权限拒绝：{}",
        now_str, denials_text
    );

    // Find the last system message.
    let last_sys_idx = history
        .iter()
        .rposition(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("system"));

    if let Some(idx) = last_sys_idx {
        let content = history[idx]
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let new_content = format!("{}{}", content, context_block);
        history[idx]["content"] = serde_json::Value::String(new_content);
    }
}

// ---------------------------------------------------------------------------
// Defense layer: compress_assistant_message
// ---------------------------------------------------------------------------

/// If an assistant message has a non-empty `tool_calls` array **and** its
/// `content` is fewer than 50 characters of trimmed text, clear the content
/// to save tokens. Substantive reasoning text is preserved.
pub fn compress_assistant_message(msg: &mut serde_json::Value) {
    let has_tool_calls = msg
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .map_or(false, |arr| !arr.is_empty());

    if !has_tool_calls {
        return;
    }

    let content_len = msg
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().len())
        .unwrap_or(0);

    if content_len < 50 {
        msg["content"] = serde_json::Value::String(String::new());
    }
}

// ---------------------------------------------------------------------------
// Defense layer: prepare_loop_iteration
// ---------------------------------------------------------------------------

/// Prepare the conversation history for a new agent-loop iteration:
/// heal orphaned tool calls first, then inject dynamic context.
pub fn prepare_loop_iteration(
    history: &mut Vec<serde_json::Value>,
    permission_denials: &[String],
) {
    heal_orphaned_tool_calls(history);
    inject_dynamic_context(history, permission_denials);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_call_marker() {
        let text = format!(
            "Some preamble\n{}{}\nMore text",
            MARKER_CALL,
            r#"{"agentId":"agent-1","task":"do stuff","params":{},"expectStructuredOutput":false,"pauseForReview":false}"#
        );
        let (marker, offset) = extract_first_loop_marker(&text).unwrap();
        assert_eq!(offset, text.find(MARKER_CALL).unwrap());
        match marker {
            ParsedLoopMarker::Call(call) => {
                assert_eq!(call.agent_id, "agent-1");
                assert_eq!(call.task, "do stuff");
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn test_extract_batch_marker_takes_priority() {
        // BATCH appears after CALL in the text, but BATCH is searched first
        // and should still win even though CALL is earlier.
        let text = format!(
            "{}{}\n{}{}",
            MARKER_CALL,
            r#"{"agentId":"a","task":"t","params":{}}"#,
            MARKER_BATCH,
            r#"{"calls":[{"agentId":"b","task":"b1","params":{}}]}"#
        );
        let (marker, _offset) = extract_first_loop_marker(&text).unwrap();
        // CALL is at offset 0, BATCH is later — earliest wins, so this is Call.
        // But we verify the parser works when BATCH comes first in search order.
        match marker {
            ParsedLoopMarker::Call(call) => {
                // CALL is earlier in text, so it is the first found.
                assert_eq!(call.agent_id, "a");
            }
            other => panic!("expected Call (earliest), got {other:?}"),
        }

        // Now put BATCH before CALL to confirm BATCH wins.
        let text2 = format!(
            "{}{}\n{}{}",
            MARKER_BATCH,
            r#"{"calls":[{"agentId":"b","task":"b1","params":{}}]}"#,
            MARKER_CALL,
            r#"{"agentId":"a","task":"t","params":{}}"#
        );
        let (marker2, _) = extract_first_loop_marker(&text2).unwrap();
        match marker2 {
            ParsedLoopMarker::Batch(batch) => {
                assert_eq!(batch.calls.len(), 1);
                assert_eq!(batch.calls[0].agent_id, "b");
            }
            other => panic!("expected Batch, got {other:?}"),
        }
    }

    #[test]
    fn test_extract_extend_marker() {
        let text = format!(
            "{}{}",
            MARKER_EXTEND,
            r#"{"currentIteration":45,"maxIterations":50,"reason":"need more","requestedExtra":10}"#
        );
        let (marker, _) = extract_first_loop_marker(&text).unwrap();
        match marker {
            ParsedLoopMarker::Extend(ext) => {
                assert_eq!(ext.current_iteration, 45);
                assert_eq!(ext.requested_extra, 10);
                assert_eq!(ext.reason, "need more");
            }
            other => panic!("expected Extend, got {other:?}"),
        }
    }

    #[test]
    fn test_no_marker_returns_none() {
        let text = "Just some plain text without any markers.";
        assert!(extract_first_loop_marker(text).is_none());
    }

    #[test]
    fn test_has_final_marker() {
        let positive = format!("blah {} done", MARKER_FINAL);
        assert!(has_final_marker(&positive));

        let negative = "blah NC_AGENT_LOOP_CALL_JSON: stuff";
        assert!(!has_final_marker(negative));
    }

    #[test]
    fn test_strip_markers() {
        let text = format!(
            "Line 1\n{}{{\"a\":1}}\nLine 2\n{} done\nLine 3",
            MARKER_CALL, MARKER_FINAL
        );
        let stripped = strip_loop_markers(&text);
        assert_eq!(stripped, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_parse_json_with_nested_braces() {
        let json = r#"{"outer":{"inner":{"deep":"value"},"arr":[1,2,3]},"x":"}tricky"}"#;
        let remaining = format!("  {}", json);
        let val = parse_json_after_marker(&remaining).unwrap();
        assert_eq!(val["outer"]["inner"]["deep"], "value");
        assert_eq!(val["x"], "}tricky");
    }

    #[test]
    fn test_format_single_result() {
        let result = AgentLoopResult {
            agent_id: "a1".into(),
            agent_name: "Alpha".into(),
            task: "do thing".into(),
            status: "ok".into(),
            output: "done".into(),
            tool_calls_count: 3,
            duration_ms: 1500,
        };
        let formatted = format_single_result(&result);
        assert!(formatted.starts_with(MARKER_RESULT));
        assert!(formatted.contains("\"agentName\":\"Alpha\""));
        assert!(formatted.ends_with('\n'));
    }

    // -----------------------------------------------------------------------
    // Defense layer tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_heal_orphaned_tool_calls_no_orphans() {
        let mut history = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({
                "role": "assistant",
                "content": "let me check",
                "tool_calls": [
                    {"id": "call_1", "type": "function", "function": {"name": "search", "arguments": "{}"}}
                ]
            }),
            serde_json::json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "result data"
            }),
        ];
        let original_len = history.len();
        heal_orphaned_tool_calls(&mut history);
        assert_eq!(history.len(), original_len, "no synthetic messages should be injected");
    }

    #[test]
    fn test_heal_orphaned_tool_calls_with_orphan() {
        let mut history = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({
                "role": "assistant",
                "content": "let me check",
                "tool_calls": [
                    {"id": "call_orphan", "type": "function", "function": {"name": "search", "arguments": "{}"}}
                ]
            }),
            serde_json::json!({"role": "user", "content": "any update?"}),
        ];
        heal_orphaned_tool_calls(&mut history);

        // Should have injected a synthetic tool result.
        assert_eq!(history.len(), 4);
        let injected = &history[2];
        assert_eq!(injected["role"], "tool");
        assert_eq!(injected["tool_call_id"], "call_orphan");
        assert!(injected["content"].as_str().unwrap().contains("[NineClaw]"));
    }

    #[test]
    fn test_inject_dynamic_context() {
        let mut history = vec![
            serde_json::json!({"role": "system", "content": "You are helpful."}),
            serde_json::json!({"role": "user", "content": "hi"}),
        ];
        let denials = vec!["file_write".to_string(), "shell_exec".to_string()];
        inject_dynamic_context(&mut history, &denials);

        let sys_content = history[0]["content"].as_str().unwrap();
        assert!(sys_content.contains("[动态上下文]"), "should contain context marker");
        assert!(sys_content.contains("权限拒绝：file_write，shell_exec"), "should list denials");
        assert!(sys_content.starts_with("You are helpful."), "original content preserved at start");
    }

    #[test]
    fn test_compress_assistant_message_short_text_removed() {
        let mut msg = serde_json::json!({
            "role": "assistant",
            "content": "让我看看",
            "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "search", "arguments": "{}"}}
            ]
        });
        compress_assistant_message(&mut msg);
        assert_eq!(msg["content"].as_str().unwrap(), "", "short content should be cleared");
    }

    #[test]
    fn test_compress_assistant_message_long_text_kept() {
        let long_reasoning = "我需要先分析一下当前的情况，然后根据已有的信息来决定下一步的操作方案。这个任务涉及到多个步骤。";
        let mut msg = serde_json::json!({
            "role": "assistant",
            "content": long_reasoning,
            "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "search", "arguments": "{}"}}
            ]
        });
        compress_assistant_message(&mut msg);
        assert_eq!(msg["content"].as_str().unwrap(), long_reasoning, "long content should be kept");
    }

    #[test]
    fn test_compress_assistant_message_no_tool_calls_kept() {
        let mut msg = serde_json::json!({
            "role": "assistant",
            "content": "好的"
        });
        compress_assistant_message(&mut msg);
        assert_eq!(msg["content"].as_str().unwrap(), "好的", "content without tool_calls should be kept");
    }
}
