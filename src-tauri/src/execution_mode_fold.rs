//! `execution_mode` 日志 fold（research-dsh report 03 step ④）。
//!
//! `execution_mode` 这类软引导状态不再只靠 DB 列：会话事件流里追加
//! `ModeSet` 整值替换事件，当前模式 = 事件流的纯 fold（最后一条 ModeSet，
//! 无则回退 DB/默认）。resume/fork/压缩时只需重放 fold 即可恢复一致状态，
//! 崩溃后事件流仍是权威。这是第一个样板状态，后续 policy 翻转照此办理。

use crate::managed_runtime::{self, SessionEventKind, SessionEventRecord};
use std::path::Path;

/// 会话事件流的纯 fold：取 `[0, end)` 内最后一条 `ModeSet` 的 mode；
/// 无任何 ModeSet 时返回 `None`（调用方回退 DB/默认值）。
pub(crate) fn fold_execution_mode(
    events: &[SessionEventRecord],
    end: Option<usize>,
) -> Option<String> {
    let end = end.unwrap_or(events.len()).min(events.len());
    events[..end]
        .iter()
        .rev()
        .find(|event| event.kind == SessionEventKind::ModeSet)
        .and_then(|event| event.detail.as_ref()?.get("mode")?.as_str())
        .map(ToOwned::to_owned)
}

/// 读取会话事件列表（JSONL 逐行反序列化，坏行跳过）。
pub(crate) fn load_session_events(agent_home: &Path, session_id: &str) -> Vec<SessionEventRecord> {
    let path = managed_runtime::session_log_path_for(agent_home, session_id);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| serde_json::from_str::<SessionEventRecord>(line).ok())
        .collect()
}

/// 会话的权威 execution_mode：fold 命中用事件流，否则回退 DB/默认。
pub(crate) fn resolved_execution_mode(
    agent_home: &Path,
    session_id: &str,
    fallback: &str,
) -> String {
    fold_execution_mode(&load_session_events(agent_home, session_id), None)
        .unwrap_or_else(|| fallback.to_string())
}

/// 追加一条 `ModeSet` 整值替换事件（会话开始/模式翻转时调用）。
pub(crate) fn append_mode_set_event(agent_home: Option<&Path>, session_id: &str, mode: &str) {
    let mode = mode.trim();
    if mode.is_empty() {
        return;
    }
    managed_runtime::append_session_event_quiet(
        agent_home,
        session_id,
        SessionEventKind::ModeSet,
        format!("execution_mode: {mode}"),
        Some(serde_json::json!({ "mode": mode })),
    );
}

/// `SessionEventKind` → DB 存储字符串（镜像表写入用）。
pub(crate) fn session_event_kind_str(kind: &SessionEventKind) -> &'static str {
    match kind {
        SessionEventKind::Prompt => "prompt",
        SessionEventKind::ToolCall => "tool_call",
        SessionEventKind::ToolResult => "tool_result",
        SessionEventKind::AssistantOutput => "assistant_output",
        SessionEventKind::Decision => "decision",
        SessionEventKind::RuntimeError => "runtime_error",
        SessionEventKind::RuntimeRetry => "runtime_retry",
        SessionEventKind::ModeSet => "mode_set",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_agent_home(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "nineclaw-mode-fold-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&path).expect("create agent home");
        path
    }

    fn record(kind: SessionEventKind, mode: Option<&str>) -> SessionEventRecord {
        SessionEventRecord {
            id: format!("evt-{}", uuid::Uuid::new_v4().simple()),
            session_id: "session-1".to_string(),
            kind,
            summary: "test".to_string(),
            detail: mode.map(|value| serde_json::json!({ "mode": value })),
            created_at: 0,
        }
    }

    #[test]
    fn fold_returns_last_mode_set_in_order() {
        let events = vec![
            record(SessionEventKind::ModeSet, Some("single")),
            record(SessionEventKind::Prompt, None),
            record(SessionEventKind::ModeSet, Some("worker")),
            record(SessionEventKind::AssistantOutput, None),
            record(SessionEventKind::ModeSet, Some("single")),
        ];
        assert_eq!(fold_execution_mode(&events, None).as_deref(), Some("single"));
    }

    #[test]
    fn fold_returns_none_without_mode_set() {
        let events = vec![record(SessionEventKind::Prompt, None)];
        assert_eq!(fold_execution_mode(&events, None), None);
        assert_eq!(fold_execution_mode(&[], None), None);
    }

    #[test]
    fn fork_point_fold_restores_mode_at_cut() {
        let events = vec![
            record(SessionEventKind::ModeSet, Some("single")),
            record(SessionEventKind::ModeSet, Some("worker")),
            record(SessionEventKind::Prompt, None),
        ];
        // 在第一条 ModeSet 之后、第二条之前 fork：子会话 fold 出 single。
        assert_eq!(fold_execution_mode(&events, Some(1)).as_deref(), Some("single"));
        // fork 到事件流末尾：fold 出 worker。
        assert_eq!(fold_execution_mode(&events, Some(2)).as_deref(), Some("worker"));
        assert_eq!(fold_execution_mode(&events, None).as_deref(), Some("worker"));
    }

    #[test]
    fn append_and_load_roundtrip_via_session_log() {
        let agent_home = temp_agent_home("roundtrip");
        append_mode_set_event(Some(&agent_home), "session-1", "worker");
        append_mode_set_event(Some(&agent_home), "session-1", "single");

        let events = load_session_events(&agent_home, "session-1");
        let _ = fs::remove_dir_all(&agent_home);

        assert_eq!(events.len(), 2);
        assert_eq!(fold_execution_mode(&events, None).as_deref(), Some("single"));
    }

    #[test]
    fn resolved_falls_back_to_db_default_without_events() {
        let agent_home = temp_agent_home("fallback");
        let mode = resolved_execution_mode(&agent_home, "missing-session", "single");
        let _ = fs::remove_dir_all(&agent_home);
        assert_eq!(mode, "single");
    }
}
