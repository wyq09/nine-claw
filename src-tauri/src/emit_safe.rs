//! Unified error-swallowing helper for Tauri event emission.
//!
//! Frontend-facing `emit` calls are notifications, not control flow: a
//! listener failing to serialize or a webview being gone must never fail the
//! surrounding streaming loop or bot pipeline. Every call site uses
//! [`emit_safe`] so failures are logged once, at one place, and never
//! propagate.

use serde::Serialize;
use tauri::Emitter;

/// Emit a Tauri event and swallow the result, logging a warning on failure.
///
/// Replaces both bare `app.emit(...)` calls (whose `Err` used to propagate
/// into streaming loops) and scattered `let _ = ...` / `if let Err` sites.
pub(crate) fn emit_safe<R, E, P>(emitter: &E, event: &str, payload: P)
where
    R: tauri::Runtime,
    E: Emitter<R>,
    P: Serialize + Clone,
{
    if let Err(error) = emitter.emit(event, payload) {
        log::warn!("emit {event} failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::Error;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::Listener;

    /// Payload that cannot be serialized, forcing `emit` to return `Err`.
    #[derive(Clone)]
    struct Unserializable;

    impl Serialize for Unserializable {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(S::Error::custom("payload refused to serialize"))
        }
    }

    #[test]
    fn emit_safe_delivers_event_to_registered_listener() {
        let app = tauri::test::mock_app();
        let received = Arc::new(AtomicUsize::new(0));
        let received_for_callback = Arc::clone(&received);
        app.listen("pi://stream", move |_event| {
            received_for_callback.fetch_add(1, Ordering::SeqCst);
        });

        emit_safe(app.handle(), "pi://stream", serde_json::json!({ "text": "hi" }));

        assert_eq!(received.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn emit_safe_swallows_emission_failures_without_propagating() {
        let app = tauri::test::mock_app();
        let received = Arc::new(AtomicUsize::new(0));
        let received_for_callback = Arc::clone(&received);
        app.listen("pi://stream", move |_event| {
            received_for_callback.fetch_add(1, Ordering::SeqCst);
        });

        // A payload that fails to serialize makes emit fail; emit_safe must
        // swallow it (log-only) instead of panicking or propagating.
        emit_safe(app.handle(), "pi://stream", Unserializable);
        assert_eq!(received.load(Ordering::SeqCst), 0);

        // The emitter keeps working afterwards.
        emit_safe(app.handle(), "pi://stream", serde_json::json!({ "text": "again" }));
        assert_eq!(received.load(Ordering::SeqCst), 1);
    }

    /// Tauri v2 事件名只允许字母数字与 `-` `/` `:` `_`。曾有带点的
    /// `session.llm_log.updated` / `workspace.llm_trace` 事件 emit 与
    /// listen 双侧被拒、实时刷新从未送达前端——这里固化该约束。
    #[test]
    fn dotted_event_names_are_rejected_but_colon_names_are_accepted() {
        let app = tauri::test::mock_app();
        for name in ["session:llm_log:updated", "workspace:llm_trace"] {
            assert!(
                app.handle().emit(name, serde_json::json!({ "ok": true })).is_ok(),
                "事件名 {name} 应合法"
            );
        }
        for name in ["session.llm_log.updated", "workspace.llm_trace"] {
            assert!(
                app.handle().emit(name, serde_json::json!({})).is_err(),
                "带点事件名 {name} 应被 Tauri 拒绝"
            );
        }
    }

    /// 提取 `emit_safe(...)` / `.emit(...)` 调用中的事件名字符串字面量，
    /// 供源码扫描测试校验字符集。
    fn extract_emit_event_names(source: &str) -> Vec<String> {
        let mut names = Vec::new();
        for marker in ["emit_safe(", ".emit("] {
            let mut rest = source;
            while let Some(index) = rest.find(marker) {
                let after = &rest[index + marker.len()..];
                if let Some(name) = first_event_string_literal(after) {
                    names.push(name);
                }
                rest = after;
            }
        }
        names
    }

    /// 跳过可选的首个非字符串参数（emitter），取事件名字符串字面量；
    /// 动态事件名（变量）返回 `None`。入参是 `emit_safe(`/`.emit(` 之后的文本。
    fn first_event_string_literal(mut tail: &str) -> Option<String> {
        tail = tail.trim_start();
        if !tail.starts_with('"') {
            let comma = tail.find(',')?;
            tail = tail[comma + 1..].trim_start();
        }
        if !tail.starts_with('"') {
            return None;
        }
        let end = tail[1..].find('"')? + 1;
        Some(tail[1..end].to_string())
    }

    fn is_valid_event_name(name: &str) -> bool {
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '/' | ':' | '_'))
    }

    /// 扫描全部 Rust 源码中 emit 调用的字面量事件名，防止再次引入
    /// Tauri 校验会拒绝的非法事件名。
    #[test]
    fn all_literal_emit_event_names_are_valid_for_tauri() {
        fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read source dir") {
                let path = entry.expect("read dir entry").path();
                if path.is_dir() {
                    collect_rs_files(&path, out);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    out.push(path);
                }
            }
        }

        let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect_rs_files(&source_root, &mut files);
        assert!(files.len() > 50, "源码扫描范围异常: {} 个文件", files.len());
        // 跳过 emit_safe.rs 自身：测试代码里的 marker 字符串字面量会被
        // 扫描器误认为 emit 调用（该模块的通用实现也不含业务事件名）。
        files.retain(|file| {
            file.file_name().and_then(|name| name.to_str()) != Some("emit_safe.rs")
        });

        let mut checked = 0usize;
        let mut invalid = Vec::new();
        for file in &files {
            let source = std::fs::read_to_string(file).expect("read source file");
            for name in extract_emit_event_names(&source) {
                checked += 1;
                if !is_valid_event_name(&name) {
                    invalid.push(format!(
                        "{}: \"{name}\"",
                        file.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                            .unwrap_or(file)
                            .display()
                    ));
                }
            }
        }
        assert!(checked > 10, "未扫描到任何 emit 事件名字面量");
        assert!(
            invalid.is_empty(),
            "存在 Tauri 会拒绝的非法事件名:\n{}",
            invalid.join("\n")
        );
    }
}
