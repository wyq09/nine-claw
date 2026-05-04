# Agent Loop 4-Phase Cycle Design

Date: 2026-05-05

## Problem

The current agent loop is an LLM-driven marker-iteration pattern: the LLM freely decides when to observe, plan, act, and stop. The harness has no structural separation of these phases. This leads to:

- No active environment observation before actions
- Pre-action guards exist as skeleton code but are not wired in
- No task-level retry — only infrastructure-level single retry
- No completion verification — the loop ends when no marker is found

## Goal

Restructure the agent loop into a harness-driven 4-phase cycle:

1. **Observe** — harness collects environment snapshot
2. **Plan** — LLM outputs structured action plan
3. **Execute** — delegated execution with 3-layer pre-guards
4. **Verify** — LLM self-evaluation with score + evidence

Plus: failure retry with LLM self-repair, and human approval UI for high-risk actions.

## Architecture

```
┌─────────────────────────────────────────────────┐
│              run_agent_loop (refactored)          │
│                                                  │
│  loop {                                          │
│    1. OBSERVE  — harness collects env snapshot   │
│    2. PLAN     — LLM outputs action plan (JSON)  │
│    3. EXECUTE  — guarded delegation              │
│       ├─ Guard 1: Policy (budget/retry/budget)   │
│       ├─ Guard 2: Safety (CapabilityPolicy)      │
│       └─ Guard 3: Human approval (popup)         │
│    4. VERIFY   — LLM self-score (0-10+evidence)  │
│       ├─ score >= threshold × N → done           │
│       └─ failed action → inject error → retry    │
│  }                                               │
└─────────────────────────────────────────────────┘
```

Key decision: phases are driven by the harness, not by LLM markers. Existing markers (`CALL`, `BATCH`, `FINAL`) are used only as the Plan phase output format. A new `VERIFY` marker is the only addition.

## Phase 1: Observe

### Data Sources

| Source | Collection method | When |
|--------|-------------------|------|
| Workspace state | Read WORKING.md (first 50 lines) + MEMORY.md (first 20 lines) | Every iteration |
| Conversation context | Last 10 messages from current session | Every iteration |
| Loop state | Iteration count, failed actions, budget used | Every iteration |
| File changes | Diff file list in agent workspace dir vs previous snapshot | First + after execute |
| Git status | `git status --short` with 2s timeout | First + after execute |

### Implementation

New function in `agent_loop.rs`:

```rust
struct EnvironmentSnapshot {
    workspace_summary: String,
    recent_file_changes: String,
    conversation_context: String,
    loop_state: LoopStateSummary,
}

struct LoopStateSummary {
    iteration: u32,
    failed_actions: Vec<FailedAction>,
    total_duration_ms: u64,
    budget_remaining_ms: Option<u64>,
}

fn collect_environment_snapshot(
    app: &AppHandle,
    agent_id: &str,
    session_id: &str,
    previous_snapshot: Option<&EnvironmentSnapshot>,
    loop_state: &LoopStateSummary,
) -> EnvironmentSnapshot
```

Injected into system prompt via `inject_dynamic_context()` extension as structured text:

```
[环境快照 - 轮次 3/50]
工作焦点: 正在实现用户认证模块
最近对话: 用户要求添加 JWT 登录 → 已创建 auth.rs → 编译报错缺少依赖
本轮变化: +src/auth.rs, +src/auth_test.rs, -Cargo.lock
失败动作: 执行 "cargo build" 失败 (缺少 serde_json 依赖)
剩余预算: 420s / 600s
```

Performance: workspace reads limited to 20-50 lines; git status with 2s timeout; file diff scoped to agent workspace dir only.

## Phase 2: Plan

The observe snapshot + task description is sent to the LLM. The prompt instructs the LLM to output only action markers:

- `NC_AGENT_LOOP_CALL_JSON` — single delegation
- `NC_AGENT_LOOP_BATCH_JSON` — concurrent batch
- `NC_AGENT_LOOP_FINAL_JSON` — task complete

If the LLM outputs unstructured text with no marker, the harness treats it as "no more actions needed" and proceeds to Verify.

The harness uses existing `extract_first_loop_marker()` but restricts to Call / Batch / Final only.

## Phase 3: Execute with 3-Layer Pre-Guards

Before calling `execute_delegate_with_timeout`, three guard layers run sequentially. Any rejection stops the action.

### Guard 1: Policy Control

Checks:
- Budget remaining sufficient for action estimated duration
- Per-action retry count within `max_retries` (default 3)
- Same action not exceeded `max_consecutive_failures` (default 3)

```rust
struct RetryBudget {
    max_retries: u32,
    retry_backoff_ms: Vec<u64>,        // [1000, 3000, 9000]
    per_action_failures: HashMap<String, u32>,
    max_consecutive_failures: u32,
}

struct GuardDecision {
    allowed: bool,
    reason: String,
    needs_approval: bool,
    retry_eligible: bool,
}
```

Result: `PASS` / `REJECT(reason)`.

### Guard 2: Safety Rules

Uses existing `AgentCapabilityPolicy` plus new fields:

```rust
pub struct AgentCapabilityPolicy {
    // Existing fields (unchanged)
    pub strategy: String,
    pub required_skill_ids: Vec<String>,
    pub forbidden_skill_ids: Vec<String>,
    pub max_dynamic_skills: usize,
    // New fields (backward compatible, #[serde(default)])
    #[serde(default)]
    pub forbidden_paths: Vec<String>,
    #[serde(default)]
    pub high_risk_actions: Vec<String>,
}
```

Result: `PASS` / `NEEDS_APPROVAL` / `REJECT(reason)`.

### Guard 3: Human Approval

Triggered when any of:
- `call.pause_for_review == true`
- Guard 2 returned `NEEDS_APPROVAL`
- Action matches a `high_risk_actions` pattern

Uses `tokio::sync::oneshot` channel to wait for frontend response. 30s default timeout, default-deny on timeout.

### Rejection Handling

Rejected actions generate a `NC_AGENT_LOOP_RESULT_JSON` with `status: "rejected"` and the rejection reason. Injected back into context for the LLM to see and adjust.

## Phase 4: Verify + Failure Retry

### Verify

After each execute phase, the harness forces one LLM evaluation call with a dedicated prompt. The LLM outputs:

```json
{
  "score": 7,
  "evidence": "已创建 auth.rs，cargo check 通过",
  "remaining": ["需要添加 JWT 过期处理"],
  "shouldContinue": true
}
```

New marker: `NC_AGENT_LOOP_VERIFY_JSON`.

```rust
struct VerifyConfig {
    score_threshold: u8,       // default 8
    consecutive_required: u8,  // default 2
    enabled: bool,             // can disable (reverts to current behavior)
}

struct VerifyResult {
    score: u8,
    evidence: String,
    remaining: Vec<String>,
    should_continue: bool,
}
```

Completion: `score >= threshold` for `consecutive_required` consecutive iterations, or `shouldContinue == false`.

### Failure Retry

When an action returns `status: "error"` or `"rejected"` (from guard):

1. `should_retry()` checks: consecutive failures < max, retry budget remaining, error is retryable (infrastructure error or LLM output error; policy rejection is not retryable)
2. Wait backoff interval (1s → 3s → 9s)
3. Construct repair prompt: original task + last output + error message + "analyze and fix"
4. Re-invoke PiBridge with same agent_id, same iteration
5. On success: record with `retryCount` metadata. On failure: increment retry counter, loop back to step 1

Retry results include metadata:

```json
{
  "agentId": "coder",
  "task": "实现认证模块",
  "status": "success",
  "output": "...",
  "retryCount": 2,
  "totalDurationMs": 15000,
  "originalError": "cargo build failed: ..."
}
```

## Loop Termination Conditions

| Condition | Source | Behavior |
|-----------|--------|----------|
| verify score >= threshold × N | Verify | Normal completion |
| FINAL marker from LLM | Plan | Normal completion |
| No marker + verify score >= threshold | Plan + Verify | Normal completion |
| max_iterations exceeded | Existing | Forced stop |
| total_timeout exceeded | Existing | Forced stop |
| abort_flag set | Existing | User abort |
| All actions rejected, no retry possible | Execute + Guard | Stop + report |

## Frontend: Human Approval Popup

### Event Flow

```
Guard 3 triggers
    → emit agent-loop://approval/request { loopId, iteration, action, riskLevel, reason, timeoutMs }
    → frontend shows modal
    → user clicks [允许/拒绝/仅本次允许]
    → invoke agent_loop_respond_approval { loopId, approved }
    → harness receives via oneshot channel, continues or rejects
```

### UI

Modal dialog at `NineClawApp.tsx` level (same level as QR scan dialog). Shows:
- Agent name and action task description
- Risk level and reason
- Countdown timer (default 30s, auto-deny on timeout)
- Three buttons: "允许" (allow + remember for this loop), "拒绝" (deny), "仅本次允许" (allow once)

### Backend

```rust
async fn request_guard_approval(
    app: &AppHandle,
    loop_id: &str,
    iteration: u32,
    action: &GuardedAction,
    timeout: Duration,
) -> Result<bool, String>

// New tauri command
#[tauri::command]
async fn agent_loop_respond_approval(
    loop_id: String,
    approved: bool,
) -> Result<(), String>
```

### New Events

| Event | When | Payload |
|-------|------|---------|
| `agent-loop://observe` | Observe phase done | `{ loopId, iteration, snapshot }` |
| `agent-loop://verify` | Verify phase done | `{ loopId, iteration, score, evidence, remaining }` |
| `agent-loop://guard/rejected` | Action rejected by guard | `{ loopId, iteration, action, guard, reason }` |
| `agent-loop://approval/request` | Human approval needed | `{ loopId, iteration, action, riskLevel, reason, timeoutMs }` |

Existing `agent-loop://iteration/start` and `iteration/end` events are extended with `guardDecision` and `retryCount` fields.

## New Types Summary (agent_loop_types.rs)

```rust
#[derive(Debug, Clone, PartialEq)]
enum LoopPhase {
    Observe,
    Plan,
    Execute,
    Verify,
}

struct EnvironmentSnapshot {
    workspace_summary: String,
    recent_file_changes: String,
    conversation_context: String,
    loop_state: LoopStateSummary,
}

struct LoopStateSummary {
    iteration: u32,
    failed_actions: Vec<FailedAction>,
    total_duration_ms: u64,
    budget_remaining_ms: Option<u64>,
}

struct FailedAction {
    action_id: String,
    task: String,
    error: String,
    attempt_count: u32,
}

struct GuardDecision {
    allowed: bool,
    reason: String,
    needs_approval: bool,
    retry_eligible: bool,
}

struct RetryBudget {
    max_retries: u32,
    retry_backoff_ms: Vec<u64>,
    per_action_failures: HashMap<String, u32>,
    max_consecutive_failures: u32,
}

struct VerifyConfig {
    score_threshold: u8,
    consecutive_required: u8,
    enabled: bool,
}

struct VerifyResult {
    score: u8,
    evidence: String,
    remaining: Vec<String>,
    should_continue: bool,
}

struct GuardedAction {
    agent_id: String,
    task: String,
    risk_level: String,
    reason: String,
}
```

## Files Changed

| File | Change |
|------|--------|
| `agent_loop.rs` | Refactor main loop into 4 phases; add `collect_environment_snapshot()`, `run_guard_chain()`, `run_verify()`, `should_retry()`, `request_guard_approval()` |
| `agent_loop_types.rs` | Add `LoopPhase`, `EnvironmentSnapshot`, `LoopStateSummary`, `FailedAction`, `GuardDecision`, `RetryBudget`, `VerifyConfig`, `VerifyResult`, `GuardedAction` |
| `agent_capabilities.rs` | Add `forbidden_paths`, `high_risk_actions` to `AgentCapabilityPolicy` |
| `lib.rs` | Add `agent_loop_respond_approval` tauri command; extend `AgentLoopConfig` with `VerifyConfig` and `RetryBudget` |
| `NineClawApp.tsx` | Add approval request listener + modal dialog component |
