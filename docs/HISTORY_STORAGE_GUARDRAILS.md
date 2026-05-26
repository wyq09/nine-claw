# History Storage Guardrails

- `chat_sessions` / `chat_turns` are the source of truth.
- Front-end history snapshots are recovery data only; they must never replace SQLite structured history.
- Critical edges must hit structured SQLite immediately instead of waiting for a later blob flush.
- Snapshot flushes are allowed to be async only as a high-frequency mirror. The current guardrail is `10ms` when idle and `20ms` while a turn is still running.
- Startup hydration must be partial-failure tolerant. If structured SQLite succeeds, a snapshot failure must not blank the UI; if snapshot succeeds, a transient structured read failure must not blank the UI.
- Do not apply a hard “recent N sessions only” cap while rebuilding in-memory history state from persisted sources. If the UI needs pagination or virtualization, do it at the render layer.
- `save_history_state` must only merge/upsert. It must not delete sessions that are missing from the payload.
- `clear_history_state` and explicit delete actions are the only allowed destructive paths.
- Scheduler/background daemons must not auto-start IM bot services. Only the main interactive app process is allowed to own those channels.
- The main desktop UI must stay single-instance. A second UI launch should focus the existing window instead of starting another process that points at the same SQLite history.
- In `tauri dev`, restart-on-change is normal. A reload must never be allowed to erase persisted history.
