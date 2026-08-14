//! Compaction lifecycle lock events (research-dsh report 01 step ③).
//!
//! A compaction writes two lifecycle lines into the PI session file:
//! `compaction/start` first (acquiring the on-disk lock) and
//! `compaction/end` LAST, only after every side effect succeeded. A crash
//! therefore leaves a detectable "start without end" orphan lock instead of
//! a lying `end`. On top of the on-disk protocol, an in-process registry
//! rejects a second concurrent compaction for the same session ("busy").

use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub(crate) const COMPACTION_START: &str = "compaction/start";
pub(crate) const COMPACTION_END: &str = "compaction/end";

/// Append one lifecycle line (`type` = `compaction/start` | `compaction/end`)
/// to the session file. Callers MUST write `end` only after all side effects
/// succeeded — never on a failure path.
pub(crate) fn append_compaction_lifecycle_entry(
    session_path: &Path,
    kind: &str,
    id: &str,
    payload: Value,
) -> Result<(), String> {
    let entry = json!({
        "type": kind,
        "id": id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "details": payload,
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(session_path)
        .map_err(|error| format!("打开 PI session 失败 {}: {error}", session_path.display()))?;
    writeln!(file, "{entry}")
        .map_err(|error| format!("追加压缩锁事件失败 {}: {error}", session_path.display()))
}

/// Scan the log for a `compaction/start` whose matching `compaction/end`
/// never followed. Returns the orphan start entry, if any.
pub(crate) fn find_orphan_compaction_start(entries: &[Value]) -> Option<Value> {
    let mut pending: Option<(String, usize)> = None;
    for (index, entry) in entries.iter().enumerate() {
        let kind = entry.get("type").and_then(Value::as_str);
        match kind {
            Some(COMPACTION_START) => {
                let id = entry
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                pending = Some((id, index));
            }
            Some(COMPACTION_END) => {
                let id = entry.get("id").and_then(Value::as_str).unwrap_or_default();
                if let Some((start_id, _)) = &pending {
                    if start_id == id {
                        pending = None;
                    }
                }
            }
            _ => {}
        }
    }
    pending.map(|(_, index)| entries[index].clone())
}

/// Warn once per detection call when the session carries an unfinished
/// compaction lock (previous compaction crashed mid-flight). The lock does
/// not block future compactions — the new run supersedes it.
pub(crate) fn report_orphan_compaction(session_path: &Path, entries: &[Value]) {
    if let Some(orphan) = find_orphan_compaction_start(entries) {
        log::warn!(
            "检测到未完成的压缩锁（上次压缩中断）: session={} start={}",
            session_path.display(),
            orphan
        );
    }
}

static ACTIVE_LOCKS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

/// RAII guard for the in-process compaction lock; released on drop.
pub(crate) struct CompactionLockGuard {
    key: PathBuf,
}

impl Drop for CompactionLockGuard {
    fn drop(&mut self) {
        if let Some(locks) = ACTIVE_LOCKS.get() {
            if let Ok(mut active) = locks.lock() {
                active.remove(&self.key);
            }
        }
    }
}

/// Acquire the in-process compaction lock for a session file. A second
/// concurrent compaction for the same session is rejected as busy.
pub(crate) fn acquire_compaction_lock(session_path: &Path) -> Result<CompactionLockGuard, String> {
    let key = fs::canonicalize(session_path).unwrap_or_else(|_| session_path.to_path_buf());
    let locks = ACTIVE_LOCKS.get_or_init(|| Mutex::new(HashSet::new()));
    let mut active = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !active.insert(key.clone()) {
        return Err(format!(
            "该会话的压缩正在进行中（busy）: {}",
            key.display()
        ));
    }
    Ok(CompactionLockGuard { key })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_file(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "nineclaw-compaction-lock-{tag}-{}.jsonl",
            uuid::Uuid::new_v4().simple()
        ))
    }

    #[test]
    fn lifecycle_entries_append_in_order() {
        let path = temp_file("append");
        append_compaction_lifecycle_entry(&path, COMPACTION_START, "lock-1", json!({"sessionId":"s1"}))
            .expect("append start");
        append_compaction_lifecycle_entry(&path, COMPACTION_END, "lock-1", json!({"sessionId":"s1"}))
            .expect("append end");

        let content = fs::read_to_string(&path).expect("read");
        let lines = content.lines().collect::<Vec<_>>();
        let _ = fs::remove_file(&path);

        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"type\":\"compaction/start\""));
        assert!(lines[1].contains("\"type\":\"compaction/end\""));
    }

    #[test]
    fn orphan_start_without_end_is_detected() {
        let entries = vec![
            json!({"type":"message","message":{"role":"user","content":"hi"}}),
            json!({"type":"compaction/start","id":"lock-1","details":{}}),
            json!({"type":"message","message":{"role":"assistant","content":"yo"}}),
        ];
        let orphan = find_orphan_compaction_start(&entries).expect("orphan detected");
        assert_eq!(orphan.get("id").and_then(Value::as_str), Some("lock-1"));
    }

    #[test]
    fn matched_start_end_is_not_orphaned() {
        let entries = vec![
            json!({"type":"compaction/start","id":"lock-1","details":{}}),
            json!({"type":"compaction","summary":"done"}),
            json!({"type":"compaction/end","id":"lock-1","details":{}}),
        ];
        assert!(find_orphan_compaction_start(&entries).is_none());
    }

    #[test]
    fn second_concurrent_compaction_is_rejected_as_busy() {
        let path = temp_file("busy");
        fs::write(&path, "{}\n").expect("write session");

        let first = acquire_compaction_lock(&path).expect("first lock");
        let second = acquire_compaction_lock(&path);
        assert!(second.is_err(), "concurrent compaction must be rejected");

        drop(first);
        let third = acquire_compaction_lock(&path).expect("lock after release");
        drop(third);
        let _ = fs::remove_file(&path);
    }
}
