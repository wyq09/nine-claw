//! Detached session-pressure measurement (research-dsh report 01 step ④).
//!
//! The compaction trigger chain used to depend entirely on the provider
//! returning a `usage` payload — providers that omit usage silently disabled
//! compaction. `measure_session_pressure` prices the PI session surface
//! independently: it reuses the most recent successful provider usage as the
//! anchor when that anchor is plausible (≥ the heuristic floor), and falls
//! back to a fixed role-aware heuristic otherwise.

use crate::pi_usage::{usage_row_total_tokens, PiTokenUsagePayload};
use serde_json::Value;

/// Per-message framing overhead in the heuristic (dsh-style: every surface
/// entry costs a little beyond its content).
const FRAMING_TOKENS: u64 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PressureBaseline {
    /// Reusing the most recent successful provider usage (envelope matched,
    /// ≥ the heuristic floor).
    Usage { total_tokens: u64 },
    /// No usable anchor; the total is the heuristic estimate.
    Estimated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TokenSurfaceNode {
    pub seq: usize,
    pub tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionPressureSnapshot {
    /// Number of log entries consumed by this measurement.
    pub log_revision: usize,
    pub total_tokens: u64,
    pub baseline: PressureBaseline,
    pub nodes: Vec<TokenSurfaceNode>,
}

fn price_text(text: &str) -> u64 {
    text.chars().count().div_ceil(4) as u64
}

fn price_entry(entry: &Value) -> u64 {
    let Some(message) = entry.get("message") else {
        return FRAMING_TOKENS;
    };
    let content_tokens = match message.get("content") {
        Some(Value::String(text)) => price_text(text),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.get("text")
                    .and_then(Value::as_str)
                    .map(price_text)
                    .unwrap_or_else(|| price_text(&item.to_string()))
            })
            .sum::<u64>(),
        Some(other) => price_text(&other.to_string()),
        None => 0,
    };
    FRAMING_TOKENS + content_tokens
}

/// Price the current session surface, detached from provider usage.
///
/// Deterministic: same entries → same snapshot. O(entries), allocates once.
pub(crate) fn measure_session_pressure(
    entries: &[Value],
    last_usage: Option<&PiTokenUsagePayload>,
) -> SessionPressureSnapshot {
    let nodes = entries
        .iter()
        .enumerate()
        .map(|(seq, entry)| TokenSurfaceNode {
            seq,
            tokens: price_entry(entry),
        })
        .collect::<Vec<_>>();
    let heuristic = nodes.iter().map(|node| node.tokens).sum::<u64>();

    let (total_tokens, baseline) = match last_usage {
        Some(usage) if usage_row_total_tokens(usage) >= heuristic => {
            let anchored = usage_row_total_tokens(usage);
            (anchored, PressureBaseline::Usage { total_tokens: anchored })
        }
        // Stale/mismatched anchor (usage below what the surface plausibly
        // costs): trust the heuristic instead.
        _ => (heuristic, PressureBaseline::Estimated),
    };

    SessionPressureSnapshot {
        log_revision: entries.len(),
        total_tokens,
        baseline,
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entries() -> Vec<Value> {
        vec![
            json!({"type":"message","message":{"role":"user","content":"hello"}}),
            json!({"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"hi there"}]}}),
            json!({"type":"message","message":{"role":"tool","content":"tool output"}}),
        ]
    }

    #[test]
    fn measurement_is_deterministic() {
        let entries = entries();
        let first = measure_session_pressure(&entries, None);
        let second = measure_session_pressure(&entries, None);
        assert_eq!(first, second);
        assert_eq!(first.log_revision, 3);
        assert_eq!(first.nodes.len(), 3);
        assert_eq!(first.baseline, PressureBaseline::Estimated);
    }

    #[test]
    fn total_tokens_rise_monotonically_with_long_tool_results() {
        let mut entries = entries();
        let before = measure_session_pressure(&entries, None).total_tokens;

        entries.push(json!({
            "type":"message",
            "message":{"role":"tool","content": "x".repeat(4000)}
        }));
        let after = measure_session_pressure(&entries, None).total_tokens;

        assert!(after > before, "{after} must exceed {before}");
        assert!(after >= 1000, "long tool result must price at ~chars/4");
    }

    #[test]
    fn plausible_usage_anchor_is_reused() {
        let entries = entries();
        let heuristic = measure_session_pressure(&entries, None).total_tokens;
        let anchor = PiTokenUsagePayload {
            input_tokens: Some(heuristic + 100),
            output_tokens: Some(50),
            cache_read_tokens: None,
            cache_write_tokens: None,
            total_tokens: Some(heuristic + 150),
        };
        let snapshot = measure_session_pressure(&entries, Some(&anchor));
        assert_eq!(
            snapshot.baseline,
            PressureBaseline::Usage {
                total_tokens: heuristic + 150
            }
        );
        assert_eq!(snapshot.total_tokens, heuristic + 150);
    }

    #[test]
    fn implausible_usage_anchor_falls_back_to_heuristic() {
        let entries = entries();
        let heuristic = measure_session_pressure(&entries, None).total_tokens;
        let stale_anchor = PiTokenUsagePayload {
            input_tokens: Some(1),
            output_tokens: Some(1),
            cache_read_tokens: None,
            cache_write_tokens: None,
            total_tokens: Some(2),
        };
        let snapshot = measure_session_pressure(&entries, Some(&stale_anchor));
        assert_eq!(snapshot.baseline, PressureBaseline::Estimated);
        // 被拒绝的锚点必然 ≤ 启发式估计（doc 验收：heuristic ≥ usage 锚点）。
        assert!(snapshot.total_tokens >= usage_row_total_tokens(&stale_anchor));
        assert_eq!(snapshot.total_tokens, heuristic);
    }
}
