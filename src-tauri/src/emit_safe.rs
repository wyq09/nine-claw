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
}
