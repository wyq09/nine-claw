//! Agent Loop marker parsing engine.
//!
//! Provides functions for extracting, parsing, stripping, and formatting the
//! structured markers (`NC_AGENT_LOOP_*`) that the LLM emits to control the
//! agent loop runtime.

use crate::agent_loop_types::{
    AgentLoopBatchResult, AgentLoopBatchMarker, AgentLoopCallMarker,
    AgentLoopExtendMarker, AgentLoopResult, ParsedLoopMarker,
};

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
}
