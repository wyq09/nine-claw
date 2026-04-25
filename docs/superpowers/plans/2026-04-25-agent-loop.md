# Agent Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现独立的 ReAct 式多 Agent Loop 引擎，主 Agent 自主循环委派子 Agent 直到任务完成。

**Architecture:** 基于标记协议（NC_AGENT_LOOP_*_JSON:），后端拦截主 Agent 回复中的标记，执行子 Agent，回注结果，循环直到无标记或达到上限。复用现有 PiBridge + managed runtime 执行子 Agent。前端可折叠流式展示。

**Tech Stack:** Rust (Tauri 2.x, tokio, rusqlite, serde), TypeScript/React (Vite, Vitest)

**Design spec:** `docs/superpowers/specs/2026-04-25-agent-loop-design.md`

---

## File Structure

### New files

| File | Responsibility |
|------|----------------|
| `src-tauri/src/agent_loop_types.rs` | 标记协议类型、配置、事件 payload、运行时状态 |
| `src-tauri/src/agent_loop.rs` | Agent Loop 引擎核心：标记解析、循环控制、委派执行、防御层 |
| `src/components/agent-loop/AgentLoopBlock.tsx` | 容器组件，管理展开/收起状态，渲染子 Agent 卡片流 |
| `src/components/agent-loop/AgentResultCard.tsx` | 单个子 Agent 结果卡片（流式输出 + 收缩摘要） |
| `src/components/agent-loop/BatchResultCard.tsx` | 批量并发卡片（tabs 切换子 Agent） |
| `src/components/agent-loop/ReviewCard.tsx` | 人工审核确认 + 扩容请求卡片 |

### Modified files

| File | Change |
|------|--------|
| `src-tauri/src/agents.rs` | AgentRecord + AgentInput 新增 `agent_loop_config` 字段，SQLite migration，CRUD 更新 |
| `src-tauri/src/lib.rs` | stream_pi_prompt 集成 agent loop，注册新 Tauri 命令 |
| `src/types.ts` | 新增 AgentLoopSegment, AgentLoopIteration, AgentLoopReviewSegment |
| `src/lib/piClient.ts` | 新增 agent loop API 函数和事件订阅函数 |
| `src/hooks/usePiAgent.ts` | 订阅 agent-loop:// 事件 |
| `src/app/agents/AgentDialogsBundle.tsx` | Agent 编辑器新增 Agent Loop 配置区 |

---

## Task 1: Rust 类型定义 — agent_loop_types.rs

**Files:**
- Create: `src-tauri/src/agent_loop_types.rs`

- [ ] **Step 1: 创建 agent_loop_types.rs — 配置和枚举类型**

```rust
// src-tauri/src/agent_loop_types.rs
//! Agent Loop 类型定义：配置、标记协议、运行时状态、事件 payload

use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use std::collections::HashMap;

// ── 配置 ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default = "default_timeout")]
    pub iteration_timeout_ms: u64,
    #[serde(default = "default_true")]
    pub enable_nested: bool,
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    #[serde(default = "default_true")]
    pub allow_extend: bool,
    #[serde(default = "default_max_extend")]
    pub max_extend_limit: u32,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
    #[serde(default = "default_batch_fail")]
    pub batch_fail_strategy: BatchFailStrategy,
}

fn default_max_iterations() -> u32 { 50 }
fn default_timeout() -> u64 { 120_000 }
fn default_true() -> bool { true }
fn default_max_depth() -> u32 { 3 }
fn default_max_extend() -> u32 { 200 }
fn default_max_concurrent() -> u32 { 5 }
fn default_batch_fail() -> BatchFailStrategy { BatchFailStrategy::WaitAll }

impl Default for AgentLoopConfig {
    fn defaults() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            iteration_timeout_ms: default_timeout(),
            enable_nested: true,
            max_depth: default_max_depth(),
            allow_extend: true,
            max_extend_limit: default_max_extend(),
            max_concurrent: default_max_concurrent(),
            batch_fail_strategy: BatchFailStrategy::WaitAll,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BatchFailStrategy {
    FailFast,
    WaitAll,
}

// ── 标记协议 JSON 结构 ───────────────────────────────────────

/// NC_AGENT_LOOP_CALL_JSON: — 单次委派
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopCallMarker {
    pub agent_id: String,
    pub task: String,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(default)]
    pub context_injection: Option<String>,
    #[serde(default)]
    pub expect_structured_output: bool,
    #[serde(default)]
    pub output_format_hint: Option<String>,
    #[serde(default)]
    pub pause_for_review: bool,
}

/// NC_AGENT_LOOP_BATCH_JSON: — 批量并发委派
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopBatchMarker {
    pub calls: Vec<AgentLoopCallMarker>,
    #[serde(default)]
    pub pause_for_review: bool,
}

/// NC_AGENT_LOOP_EXTEND_JSON: — 申请扩大循环数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopExtendMarker {
    pub current_iteration: u32,
    pub max_iterations: u32,
    pub reason: String,
    pub requested_extra: u32,
}

/// NC_AGENT_LOOP_RESULT_JSON: — 单次委派结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopResult {
    pub agent_id: String,
    pub agent_name: String,
    pub task: String,
    pub status: String, // success | error | cancelled
    pub output: String,
    #[serde(default)]
    pub tool_calls_count: u32,
    #[serde(default)]
    pub duration_ms: u64,
}

/// 批量委派结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopBatchResult {
    pub batch_id: String,
    pub results: Vec<AgentLoopResult>,
    pub total_duration_ms: u64,
}

// ── 解析后的标记枚举 ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ParsedLoopMarker {
    Call(AgentLoopCallMarker),
    Batch(AgentLoopBatchMarker),
    Extend(AgentLoopExtendMarker),
}

// ── 运行时状态 ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LoopIteration {
    pub iteration: u32,
    pub marker_type: String, // "call" | "batch" | "extend"
    pub sub_agent_ids: Vec<String>,
    pub results: Vec<AgentLoopResult>,
    pub duration_ms: u64,
}

pub struct AgentLoopState {
    pub loop_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub iteration: u32,
    pub max_iterations: u32,
    pub depth: u32,
    pub started_at: Instant,
    pub history: Vec<LoopIteration>,
    pub permission_denials: Vec<String>,
}

// ── 全局活跃 loop 管理 ───────────────────────────────────────

pub struct ReviewResponse {
    pub approved: bool,
    pub extend_to: Option<u32>,
}

pub struct ActiveLoopHandle {
    pub abort_flag: Arc<AtomicBool>,
    pub review_sender: Option<tokio::sync::oneshot::Sender<ReviewResponse>>,
    pub state: Arc<Mutex<AgentLoopState>>,
}

pub struct ActiveLoops {
    pub loops: std::sync::Mutex<HashMap<String, ActiveLoopHandle>>,
}

// ── 不可恢复错误 ─────────────────────────────────────────────

#[derive(Debug)]
pub enum UnrecoverableError {
    ContextCorrupted,
    InvalidModelOutput,
    SafetyViolation,
    AgentNotFound(String),
    ProviderAuthFailed,
    NestedDepthExceeded,
}

impl std::fmt::Display for UnrecoverableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContextCorrupted => write!(f, "对话上下文已损坏"),
            Self::InvalidModelOutput => write!(f, "模型输出格式无法解析"),
            Self::SafetyViolation => write!(f, "安全扫描触发"),
            Self::AgentNotFound(id) => write!(f, "子 Agent 不存在: {id}"),
            Self::ProviderAuthFailed => write!(f, "API 密钥验证失败"),
            Self::NestedDepthExceeded => write!(f, "嵌套深度超限"),
        }
    }
}
```

- [ ] **Step 2: 在 lib.rs 中 mod agent_loop_types**

在 `src-tauri/src/lib.rs` 的 mod 声明区域添加：

```rust
pub mod agent_loop_types;
```

- [ ] **Step 3: 运行编译验证**

```bash
cd src-tauri && cargo check 2>&1 | head -30
```

Expected: 编译通过（只有类型定义，无逻辑依赖）

- [ ] **Step 4: 编写标记解析测试**

在 `agent_loop_types.rs` 末尾添加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = AgentLoopConfig::default();
        assert_eq!(config.max_iterations, 50);
        assert_eq!(config.iteration_timeout_ms, 120_000);
        assert!(config.enable_nested);
        assert_eq!(config.max_depth, 3);
        assert!(config.allow_extend);
        assert_eq!(config.max_extend_limit, 200);
        assert_eq!(config.max_concurrent, 5);
        assert!(matches!(config.batch_fail_strategy, BatchFailStrategy::WaitAll));
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let config = AgentLoopConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AgentLoopConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_iterations, config.max_iterations);
        assert_eq!(parsed.max_concurrent, config.max_concurrent);
    }

    #[test]
    fn test_config_from_partial_json() {
        let json = r#"{"maxIterations": 10}"#;
        let config: AgentLoopConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_iterations, 10);
        assert_eq!(config.max_concurrent, 5); // default
    }

    #[test]
    fn test_call_marker_parse() {
        let json = r#"{"agentId":"a1","task":"do stuff","params":{"key":"val"},"pauseForReview":true}"#;
        let marker: AgentLoopCallMarker = serde_json::from_str(json).unwrap();
        assert_eq!(marker.agent_id, "a1");
        assert_eq!(marker.task, "do stuff");
        assert!(marker.pause_for_review);
    }

    #[test]
    fn test_batch_marker_parse() {
        let json = r#"{"calls":[{"agentId":"a1","task":"t1"},{"agentId":"a2","task":"t2"}],"pauseForReview":false}"#;
        let marker: AgentLoopBatchMarker = serde_json::from_str(json).unwrap();
        assert_eq!(marker.calls.len(), 2);
    }

    #[test]
    fn test_extend_marker_parse() {
        let json = r#"{"currentIteration":45,"maxIterations":50,"reason":"need more","requestedExtra":20}"#;
        let marker: AgentLoopExtendMarker = serde_json::from_str(json).unwrap();
        assert_eq!(marker.requested_extra, 20);
    }

    #[test]
    fn test_unrecoverable_error_display() {
        let err = UnrecoverableError::AgentNotFound("agent-123".to_string());
        assert!(err.to_string().contains("agent-123"));
    }
}
```

- [ ] **Step 5: 运行测试**

```bash
cd src-tauri && cargo test agent_loop_types -- --nocapture
```

Expected: 7 tests PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/agent_loop_types.rs src-tauri/src/lib.rs
git commit -m "feat(agent-loop): 类型定义 — 配置、标记协议、运行时状态"
```

---

## Task 2: 数据层 — agents.rs 新增 agent_loop_config 字段

**Files:**
- Modify: `src-tauri/src/agents.rs`

- [ ] **Step 1: AgentRecord 新增字段**

在 `agents.rs` 的 `AgentRecord` struct 中，在 `scenario_llm_config` 字段之后（约 line 76）添加：

```rust
    pub agent_loop_config: Option<agent_loop_types::AgentLoopConfig>,
```

同样在 `AgentInput` struct 中添加：

```rust
    pub agent_loop_config: Option<agent_loop_types::AgentLoopConfig>,
```

- [ ] **Step 2: 添加序列化/反序列化 helper**

在 agents.rs 的 serialize/deserialize helper 函数区域（约 line 1730 附近），添加：

```rust
fn serialize_agent_loop_config(
    config: &Option<agent_loop_types::AgentLoopConfig>,
) -> Result<String, String> {
    match config {
        Some(c) => serde_json::to_string(c)
            .map_err(|e| format!("序列化 agent_loop_config 失败: {e}")),
        None => Ok(String::new()),
    }
}

fn deserialize_agent_loop_config(
    raw: Option<String>,
) -> Option<agent_loop_types::AgentLoopConfig> {
    raw.and_then(|v| {
        if v.is_empty() {
            None
        } else {
            serde_json::from_str::<agent_loop_types::AgentLoopConfig>(&v).ok()
        }
    })
}
```

- [ ] **Step 3: 添加 SQLite migration**

在 `ensure_agents_schema()` 的 migration 调用列表（约 line 553-557）末尾添加：

```rust
    add_agents_column_if_missing(connection, "agent_loop_config_json", "TEXT")?;
```

- [ ] **Step 4: 更新 SELECT 查询**

在 `get_active_agent_by_id`（约 line 835）、`list_agents_with_connection`（约 line 716）、`list_active_agents_for_workspace`（约 line 2199）的 SELECT 语句中，在 `scenario_llm_config_json` 之后添加 `, agent_loop_config_json`。

在对应的 row-mapping closure 中添加反序列化：

```rust
    let agent_loop_config = deserialize_agent_loop_config(row.get_raw("agent_loop_config_json").unwrap_or(SqliteValueRef::Null).as_str().ok().map(String::from));
```

> 注意：实际代码需要匹配现有 row.get_raw 或 row.get 模式。参考 `deserialize_collaboration_config` 在同一 closure 中的用法。

- [ ] **Step 5: 更新 INSERT**

在 `create_agent_with_connection`（约 line 1011）的 INSERT 语句中添加 `agent_loop_config_json` 列和 `?` 占位。在参数列表中添加：

```rust
    serialize_agent_loop_config(&input.agent_loop_config)?,
```

- [ ] **Step 6: 更新 UPDATE**

在 `update_agent_with_connection`（约 line 1098）的 UPDATE 语句中添加 `agent_loop_config_json = ?`。在参数列表中添加：

```rust
    serialize_agent_loop_config(&input.agent_loop_config)?,
```

- [ ] **Step 7: 编译验证**

```bash
cd src-tauri && cargo check 2>&1 | head -30
```

Expected: 编译通过

- [ ] **Step 8: 运行现有 agents 测试确认无回归**

```bash
cd src-tauri && cargo test agents -- --nocapture
```

Expected: 所有现有测试仍然 PASS

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/agents.rs
git commit -m "feat(agent-loop): agents 表新增 agent_loop_config 字段"
```

---

## Task 3: 核心引擎 — agent_loop.rs 标记解析与循环骨架

**Files:**
- Create: `src-tauri/src/agent_loop.rs`

- [ ] **Step 1: 创建 agent_loop.rs — 标记解析函数**

```rust
// src-tauri/src/agent_loop.rs
//! Agent Loop 引擎核心：标记解析、循环控制、委派执行

use crate::agent_loop_types::*;
use tauri::AppHandle;

pub const MARKER_CALL: &str = "NC_AGENT_LOOP_CALL_JSON:";
pub const MARKER_BATCH: &str = "NC_AGENT_LOOP_BATCH_JSON:";
pub const MARKER_EXTEND: &str = "NC_AGENT_LOOP_EXTEND_JSON:";
pub const MARKER_FINAL: &str = "NC_AGENT_LOOP_FINAL:";
pub const MARKER_RESULT: &str = "NC_AGENT_LOOP_RESULT_JSON:";

/// 从文本中提取第一个 Agent Loop 标记。
/// 返回 (标记内容, 标记之后的剩余文本, 解析结果)。
/// 如果没有标记，返回 None。
pub fn extract_first_loop_marker(text: &str) -> Option<(ParsedLoopMarker, usize)> {
    // 按标记优先级搜索：BATCH > CALL > EXTEND
    // BATCH 优先因为 CALL 是 BATCH 的子集，先匹配 BATCH 避免误拆
    if let Some(pos) = text.find(MARKER_BATCH) {
        let json_start = pos + MARKER_BATCH.len();
        if let Some(marker) = parse_json_after_marker(&text[json_start..]) {
            return Some((ParsedLoopMarker::Batch(marker), pos));
        }
    }

    if let Some(pos) = text.find(MARKER_CALL) {
        let json_start = pos + MARKER_CALL.len();
        if let Some(marker) = parse_json_after_marker(&text[json_start..]) {
            return Some((ParsedLoopMarker::Call(marker), pos));
        }
    }

    if let Some(pos) = text.find(MARKER_EXTEND) {
        let json_start = pos + MARKER_EXTEND.len();
        if let Some(marker) = parse_json_after_marker(&text[json_start..]) {
            return Some((ParsedLoopMarker::Extend(marker), pos));
        }
    }

    None
}

/// 从标记前缀之后的位置解析 JSON 对象。
/// 查找第一个 `{` 并匹配到对应的 `}`。
fn parse_json_after_marker(remaining: &str) -> Option<serde_json::Value> {
    let start = remaining.find('{')?;
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for (i, ch) in remaining[start..].char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    let json_str = &remaining[start..start + i + 1];
                    return serde_json::from_str(json_str).ok();
                }
            }
            _ => {}
        }
    }
    None
}

/// 检查文本是否包含 NC_AGENT_LOOP_FINAL 终止标记
pub fn has_final_marker(text: &str) -> bool {
    text.contains(MARKER_FINAL)
}

/// 从文本中移除所有 Agent Loop 标记行，返回清理后的文本
pub fn strip_loop_markers(text: &str) -> String {
    let mut result = text.to_string();
    // 移除标记行（以标记前缀开头的行）
    result = result
        .lines()
        .filter(|line| {
            !line.contains(MARKER_CALL)
                && !line.contains(MARKER_BATCH)
                && !line.contains(MARKER_EXTEND)
                && !line.contains(MARKER_FINAL)
                && !line.contains(MARKER_RESULT)
        })
        .collect::<Vec<_>>()
        .join("\n");
    // 清理多余空行
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }
    result.trim().to_string()
}

/// 构造单次委派的结果回注文本
pub fn format_single_result(result: &AgentLoopResult) -> String {
    format!(
        "\n\n{}{}\n```json\n{}\n```\n",
        MARKER_RESULT,
        serde_json::to_string(result).unwrap_or_else(|_| "{}".to_string()),
    )
    .trim_start()
    .to_string()
}

/// 构造批量委派的结果回注文本
pub fn format_batch_result(result: &AgentLoopBatchResult) -> String {
    format!(
        "\n\n{}{}\n",
        MARKER_RESULT,
        serde_json::to_string(result).unwrap_or_else(|_| "{}".to_string()),
    )
    .trim_start()
    .to_string()
}
```

- [ ] **Step 2: 编写标记解析测试**

在 `agent_loop.rs` 末尾添加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_call_marker() {
        let text = "让我分析一下\nNC_AGENT_LOOP_CALL_JSON:{\"agentId\":\"a1\",\"task\":\"review code\"}\n继续";
        let (marker, _pos) = extract_first_loop_marker(text).unwrap();
        match marker {
            ParsedLoopMarker::Call(call) => {
                assert_eq!(call.agent_id, "a1");
                assert_eq!(call.task, "review code");
            }
            _ => panic!("Expected Call marker"),
        }
    }

    #[test]
    fn test_extract_batch_marker_takes_priority() {
        let text = "NC_AGENT_LOOP_BATCH_JSON:{\"calls\":[{\"agentId\":\"a1\",\"task\":\"t1\"}]}";
        let (marker, _) = extract_first_loop_marker(text).unwrap();
        assert!(matches!(marker, ParsedLoopMarker::Batch(_)));
    }

    #[test]
    fn test_extract_extend_marker() {
        let text = "NC_AGENT_LOOP_EXTEND_JSON:{\"currentIteration\":45,\"maxIterations\":50,\"reason\":\"more work\",\"requestedExtra\":20}";
        let (marker, _) = extract_first_loop_marker(text).unwrap();
        match marker {
            ParsedLoopMarker::Extend(ext) => {
                assert_eq!(ext.requested_extra, 20);
            }
            _ => panic!("Expected Extend marker"),
        }
    }

    #[test]
    fn test_no_marker_returns_none() {
        let text = "这是一段普通文本，没有标记。";
        assert!(extract_first_loop_marker(text).is_none());
    }

    #[test]
    fn test_has_final_marker() {
        assert!(has_final_marker("done\nNC_AGENT_LOOP_FINAL:"));
        assert!(!has_final_marker("done"));
    }

    #[test]
    fn test_strip_markers() {
        let text = "开始\nNC_AGENT_LOOP_CALL_JSON:{\"agentId\":\"a1\",\"task\":\"t\"}\n结束";
        let cleaned = strip_loop_markers(text);
        assert!(!cleaned.contains("NC_AGENT_LOOP"));
        assert!(cleaned.contains("开始"));
        assert!(cleaned.contains("结束"));
    }

    #[test]
    fn test_parse_json_with_nested_braces() {
        let json = r#"{"agentId":"a1","task":"do","params":{"nested":{"deep":1}}}"#;
        let parsed = parse_json_after_marker(json).unwrap();
        assert_eq!(parsed["agentId"], "a1");
    }

    #[test]
    fn test_format_single_result() {
        let result = AgentLoopResult {
            agent_id: "a1".into(),
            agent_name: "Reviewer".into(),
            task: "review".into(),
            status: "success".into(),
            output: "found 2 issues".into(),
            tool_calls_count: 3,
            duration_ms: 5000,
        };
        let text = format_single_result(&result);
        assert!(text.contains(MARKER_RESULT));
        assert!(text.contains("Reviewer"));
    }
}
```

- [ ] **Step 3: 运行测试**

```bash
cd src-tauri && cargo test agent_loop -- --nocapture
```

Expected: 8 tests PASS

- [ ] **Step 4: 在 lib.rs 注册模块**

在 `src-tauri/src/lib.rs` 的 mod 声明区域添加：

```rust
pub mod agent_loop;
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent_loop.rs src-tauri/src/lib.rs
git commit -m "feat(agent-loop): 标记解析引擎 — extract/strip/format"
```

---

## Task 4: 防御层 — prepare_loop_iteration + compress_assistant_message

**Files:**
- Modify: `src-tauri/src/agent_loop.rs`

- [ ] **Step 1: 添加防御层函数**

在 `agent_loop.rs` 中添加：

```rust
use serde_json::Value;

/// 修复孤立的 tool_call：扫描历史，为没有对应 tool_result 的 tool_call 注入合成结果。
/// `history` 是 JSON 格式的对话消息数组（PI session 格式）。
pub fn heal_orphaned_tool_calls(history: &mut Vec<Value>) {
    // 收集所有 tool_call_id
    let mut tool_call_ids: Vec<String> = Vec::new();
    for msg in history.iter() {
        if msg.get("role").and_then(|r| r.as_str()) == Some("assistant") {
            if let Some(tool_calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tool_calls {
                    if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                        tool_call_ids.push(id.to_string());
                    }
                }
            }
        }
    }

    // 收集所有已有 tool_result 的 tool_call_id
    let mut answered_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for msg in history.iter() {
        if msg.get("role").and_then(|r| r.as_str()) == Some("tool") {
            if let Some(id) = msg.get("tool_call_id").and_then(|i| i.as_str()) {
                answered_ids.insert(id.to_string());
            }
        }
    }

    // 为未回复的 tool_call 注入合成 tool_result
    for id in tool_call_ids {
        if !answered_ids.contains(&id) {
            let synthetic = serde_json::json!({
                "role": "tool",
                "tool_call_id": id,
                "content": "[NineClaw] 此工具调用因会话中断未完成，请根据已有信息继续。"
            });
            history.push(synthetic);
        }
    }
}

/// 将当前时间和权限拒绝摘要追加到最后一条 system 消息末尾。
/// 放在末尾保护 KV Cache prefix。
pub fn inject_dynamic_context(
    history: &mut Vec<Value>,
    permission_denials: &[String],
) {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M %Z").to_string();
    let denials_text = if permission_denials.is_empty() {
        "无".to_string()
    } else {
        permission_denials.join("；")
    };

    let context_block = format!(
        "\n\n[动态上下文]\n当前时间：{}\n权限拒绝：{}",
        now, denials_text
    );

    // 找到最后一条 system 消息并追加
    for msg in history.iter_mut().rev() {
        if msg.get("role").and_then(|r| r.as_str()) == Some("system") {
            if let Some(content) = msg.get_mut("content").and_then(|c| c.as_str_mut()) {
                content.push_str(&context_block);
            }
            return;
        }
    }
}

/// Token 优化：如果 assistant 消息有 tool_call 且伴随文字 < 50 字（说明性文字），丢弃文字。
pub fn compress_assistant_message(msg: &mut Value) {
    let has_tool_calls = msg
        .get("tool_calls")
        .and_then(|t| t.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);

    if !has_tool_calls {
        return;
    }

    if let Some(content) = msg.get_mut("content").and_then(|c| c.as_str_mut()) {
        // 丢弃短说明性文字（< 50 字符）
        let trimmed = content.trim();
        if trimmed.len() < 50 && !trimmed.is_empty() {
            *content = String::new();
        }
    }
}

/// 组合防御层函数
pub fn prepare_loop_iteration(
    history: &mut Vec<Value>,
    permission_denials: &[String],
) {
    heal_orphaned_tool_calls(history);
    inject_dynamic_context(history, permission_denials);
}
```

- [ ] **Step 2: 在 Cargo.toml 确认 chrono 依赖**

检查 `src-tauri/Cargo.toml` 是否已有 `chrono`。如果没有，添加：

```toml
chrono = { version = "0.4", features = ["serde"] }
```

> 注意：根据探索报告，项目已使用 chrono。如果已有则跳过此步。

- [ ] **Step 3: 编写防御层测试**

在 `agent_loop.rs` 的 `mod tests` 中添加：

```rust
    #[test]
    fn test_heal_orphaned_tool_calls_no_orphans() {
        let mut history = vec![
            serde_json::json!({"role": "assistant", "tool_calls": [{"id": "tc1", "function": {"name": "f"}}]}),
            serde_json::json!({"role": "tool", "tool_call_id": "tc1", "content": "ok"}),
        ];
        heal_orphaned_tool_calls(&mut history);
        assert_eq!(history.len(), 2); // 没有新增
    }

    #[test]
    fn test_heal_orphaned_tool_calls_with_orphan() {
        let mut history = vec![
            serde_json::json!({"role": "assistant", "tool_calls": [{"id": "tc1", "function": {"name": "f"}}]}),
        ];
        heal_orphaned_tool_calls(&mut history);
        assert_eq!(history.len(), 2);
        assert_eq!(history[1]["role"], "tool");
        assert_eq!(history[1]["tool_call_id"], "tc1");
        assert!(history[1]["content"].as_str().unwrap().contains("中断"));
    }

    #[test]
    fn test_inject_dynamic_context() {
        let mut history = vec![
            serde_json::json!({"role": "system", "content": "You are helpful."}),
            serde_json::json!({"role": "user", "content": "hi"}),
        ];
        inject_dynamic_context(&mut history, &["search was denied".to_string()]);
        let sys_content = history[0]["content"].as_str().unwrap();
        assert!(sys_content.contains("[动态上下文]"));
        assert!(sys_content.contains("search was denied"));
    }

    #[test]
    fn test_compress_assistant_message_short_text_removed() {
        let mut msg = serde_json::json!({
            "role": "assistant",
            "content": "让我看看",
            "tool_calls": [{"id": "tc1", "function": {"name": "f"}}]
        });
        compress_assistant_message(&mut msg);
        assert_eq!(msg["content"].as_str().unwrap(), "");
    }

    #[test]
    fn test_compress_assistant_message_long_text_kept() {
        let mut msg = serde_json::json!({
            "role": "assistant",
            "content": "经过分析，我认为这个问题的根源在于数据库连接池配置不当，需要调整最大连接数。",
            "tool_calls": [{"id": "tc1", "function": {"name": "f"}}]
        });
        compress_assistant_message(&mut msg);
        assert!(!msg["content"].as_str().unwrap().is_empty());
    }

    #[test]
    fn test_compress_assistant_message_no_tool_calls_kept() {
        let mut msg = serde_json::json!({
            "role": "assistant",
            "content": "短文本"
        });
        compress_assistant_message(&mut msg);
        assert_eq!(msg["content"].as_str().unwrap(), "短文本");
    }
```

- [ ] **Step 4: 运行测试**

```bash
cd src-tauri && cargo test agent_loop -- --nocapture
```

Expected: 所有测试 PASS（之前 8 个 + 新增 6 个 = 14）

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent_loop.rs
git commit -m "feat(agent-loop): 防御层 — 孤立 tool_call 修复、动态上下文注入、token 压缩"
```

---

## Task 5: 委派执行与循环控制 — run_agent_loop

**Files:**
- Modify: `src-tauri/src/agent_loop.rs`

- [ ] **Step 1: 添加事件发射 helper**

```rust
fn emit_loop_event(app: &AppHandle, event: &str, payload: serde_json::Value) {
    let _ = app.emit(event, payload);
}
```

- [ ] **Step 2: 添加单次委派执行函数**

```rust
use crate::team_workspace;
use std::sync::atomic::Ordering;

/// 执行单个子 Agent 委派，返回结果。
/// 复用 team_workspace::run_delegate_with_provider_events。
fn execute_single_delegate(
    app: &AppHandle,
    call: &AgentLoopCallMarker,
    loop_id: &str,
    iteration: u32,
    provider: &crate::ProviderRuntimeConfig,
    abort_flag: &Arc<AtomicBool>,
) -> AgentLoopResult {
    let start = Instant::now();
    let agent_id = &call.agent_id;

    // 查找目标 Agent 名称
    let agent_name = crate::agents::get_agent_record(app, agent_id)
        .ok()
        .flatten()
        .map(|a| a.name)
        .unwrap_or_else(|| agent_id.clone());

    // 订阅委派事件并转发为 agent-loop 事件
    let app_clone = app.clone();
    let loop_id_owned = loop_id.to_string();
    let agent_id_owned = agent_id.clone();

    let result = if abort_flag.load(Ordering::Relaxed) {
        AgentLoopResult {
            agent_id: agent_id.clone(),
            agent_name: agent_name.clone(),
            task: call.task.clone(),
            status: "cancelled".to_string(),
            output: String::new(),
            tool_calls_count: 0,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    } else {
        // 构造委派 prompt（包含 params）
        let delegate_prompt = if call.params.is_null() || call.params.to_string() == "{}" {
            call.task.clone()
        } else {
            format!(
                "{}\n\n[委派参数]\n{}",
                call.task,
                serde_json::to_string_pretty(&call.params)
                    .unwrap_or_else(|_| call.params.to_string())
            )
        };

        match team_workspace::run_delegate_with_provider_events(
            app,
            "", // workspace_id: Agent Loop 不属于 team workspace
            agent_id,
            &delegate_prompt,
            provider,
            Some(&format!("{}-{}", loop_id, iteration)),
            None,
        ) {
            Ok(output) => AgentLoopResult {
                agent_id: agent_id.clone(),
                agent_name: agent_name.clone(),
                task: call.task.clone(),
                status: "success".to_string(),
                output,
                tool_calls_count: 0, // PiBridge 不直接返回 tool_calls_count
                duration_ms: start.elapsed().as_millis() as u64,
            },
            Err(e) => AgentLoopResult {
                agent_id: agent_id.clone(),
                agent_name: agent_name.clone(),
                task: call.task.clone(),
                status: "error".to_string(),
                output: e,
                tool_calls_count: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            },
        }
    };

    result
}
```

- [ ] **Step 3: 添加批量委派执行函数**

```rust
use tokio::task::JoinSet;

/// 并发执行批量委派，上限 max_concurrent。
async fn execute_batch_delegates(
    app: &AppHandle,
    batch: &AgentLoopBatchMarker,
    config: &AgentLoopConfig,
    loop_id: &str,
    iteration: u32,
    provider: &crate::ProviderRuntimeConfig,
    abort_flag: &Arc<AtomicBool>,
) -> AgentLoopBatchResult {
    let start = Instant::now();
    let batch_id = format!("{}-batch-{}", loop_id, iteration);
    let max_concurrent = config.max_concurrent as usize;

    let mut join_set = JoinSet::new();
    let calls: Vec<_> = batch.calls.iter().take(max_concurrent).collect();

    for call in &calls {
        let app_handle = app.clone();
        let call_clone = (*call).clone();
        let loop_id_owned = loop_id.to_string();
        let provider_clone = provider.clone();
        let abort_flag_clone = abort_flag.clone();

        join_set.spawn_blocking(move || {
            execute_single_delegate(
                &app_handle,
                &call_clone,
                &loop_id_owned,
                iteration,
                &provider_clone,
                &abort_flag_clone,
            )
        });
    }

    let mut results = Vec::new();
    while let Some(res) = join_set.join_next().await {
        match res {
            Ok(result) => results.push(result),
            Err(_) => {
                if matches!(config.batch_fail_strategy, BatchFailStrategy::FailFast) {
                    join_set.abort_all();
                    break;
                }
            }
        }
    }

    AgentLoopBatchResult {
        batch_id,
        results,
        total_duration_ms: start.elapsed().as_millis() as u64,
    }
}
```

- [ ] **Step 4: 添加主循环函数 run_agent_loop**

```rust
/// Agent Loop 主循环。
/// 从 stream_pi_prompt 调用，主 Agent 回复完成后进入。
pub async fn run_agent_loop(
    app: &AppHandle,
    agent_id: &str,
    session_id: &str,
    config: &AgentLoopConfig,
    provider: &crate::ProviderRuntimeConfig,
    initial_text: &str,
    depth: u32,
) -> Result<String, String> {
    let loop_id = format!("loop-{}", uuid::Uuid::new_v4());
    let abort_flag = Arc::new(AtomicBool::new(false));

    // 注册到全局活跃 loops（供 abort/respond_review 命令使用）
    let state = Arc::new(Mutex::new(AgentLoopState {
        loop_id: loop_id.clone(),
        agent_id: agent_id.to_string(),
        session_id: session_id.to_string(),
        iteration: 0,
        max_iterations: config.max_iterations,
        depth,
        started_at: Instant::now(),
        history: Vec::new(),
        permission_denials: Vec::new(),
    }));

    register_active_loop(&loop_id, abort_flag.clone(), state.clone());

    // emit started
    emit_loop_event(
        app,
        "agent-loop://started",
        serde_json::json!({
            "loopId": loop_id,
            "maxIterations": config.max_iterations,
            "depth": depth,
        }),
    );

    let mut accumulated_text = initial_text.to_string();
    let max_iterations = config.max_iterations;

    // 主循环
    loop {
        let current_state = state.lock().await;
        let iteration = current_state.iteration;
        if iteration >= max_iterations {
            drop(current_state);
            // 达到上限，给主 Agent 一次收尾机会
            emit_loop_event(
                app,
                "agent-loop://completed",
                serde_json::json!({
                    "loopId": loop_id,
                    "reason": "max_iterations",
                    "totalIterations": iteration,
                    "durationMs": current_state.started_at.elapsed().as_millis() as u64,
                }),
            );
            unregister_active_loop(&loop_id);
            return Ok(accumulated_text);
        }
        drop(current_state);

        // 检查 abort
        if abort_flag.load(Ordering::Relaxed) {
            emit_loop_event(
                app,
                "agent-loop://aborted",
                serde_json::json!({
                    "loopId": loop_id,
                    "iterationsCompleted": iteration,
                }),
            );
            unregister_active_loop(&loop_id);
            return Ok(accumulated_text);
        }

        // 提取标记
        let marker = extract_first_loop_marker(&accumulated_text);

        match marker {
            None => {
                // 无标记，正常结束
                let st = state.lock().await;
                emit_loop_event(
                    app,
                    "agent-loop://completed",
                    serde_json::json!({
                        "loopId": loop_id,
                        "reason": "natural",
                        "totalIterations": st.iteration,
                        "durationMs": st.started_at.elapsed().as_millis() as u64,
                    }),
                );
                drop(st);
                unregister_active_loop(&loop_id);
                return Ok(accumulated_text);
            }
            Some((ParsedLoopMarker::Call(call), _pos)) => {
                emit_loop_event(
                    app,
                    "agent-loop://iteration/start",
                    serde_json::json!({
                        "loopId": loop_id,
                        "iteration": iteration + 1,
                        "type": "call",
                        "agentId": call.agent_id,
                        "task": call.task,
                    }),
                );

                // 人工审核
                if call.pause_for_review {
                    let approved = request_user_review(
                        app, &loop_id, iteration, "pause_for_review",
                        &serde_json::json!({
                            "agentName": call.agent_id,
                            "task": call.task,
                        }),
                        &abort_flag,
                    ).await;

                    if !approved {
                        let mut st = state.lock().await;
                        st.iteration += 1;
                        drop(st);
                        accumulated_text = strip_loop_markers(&accumulated_text);
                        continue;
                    }
                }

                let result = execute_single_delegate(
                    app, &call, &loop_id, iteration + 1, provider, &abort_flag,
                );

                emit_loop_event(
                    app,
                    "agent-loop://iteration/end",
                    serde_json::json!({
                        "loopId": loop_id,
                        "iteration": iteration + 1,
                        "status": result.status,
                        "agentId": result.agent_id,
                        "durationMs": result.duration_ms,
                    }),
                );

                // 回注结果
                let result_text = format_single_result(&result);
                accumulated_text = strip_loop_markers(&accumulated_text);
                accumulated_text.push_str(&result_text);

                let mut st = state.lock().await;
                st.iteration += 1;
                st.history.push(LoopIteration {
                    iteration: st.iteration,
                    marker_type: "call".to_string(),
                    sub_agent_ids: vec![call.agent_id],
                    results: vec![result],
                    duration_ms: 0,
                });
            }
            Some((ParsedLoopMarker::Batch(batch), _pos)) => {
                emit_loop_event(
                    app,
                    "agent-loop://iteration/start",
                    serde_json::json!({
                        "loopId": loop_id,
                        "iteration": iteration + 1,
                        "type": "batch",
                        "agentCount": batch.calls.len(),
                    }),
                );

                if batch.pause_for_review {
                    let approved = request_user_review(
                        app, &loop_id, iteration, "pause_for_review",
                        &serde_json::json!({
                            "calls": batch.calls.iter().map(|c| serde_json::json!({
                                "agentId": c.agent_id,
                                "task": c.task,
                            })).collect::<Vec<_>>(),
                        }),
                        &abort_flag,
                    ).await;

                    if !approved {
                        let mut st = state.lock().await;
                        st.iteration += 1;
                        drop(st);
                        accumulated_text = strip_loop_markers(&accumulated_text);
                        continue;
                    }
                }

                let batch_result = execute_batch_delegates(
                    app, &batch, config, &loop_id, iteration + 1, provider, &abort_flag,
                ).await;

                emit_loop_event(
                    app,
                    "agent-loop://iteration/end",
                    serde_json::json!({
                        "loopId": loop_id,
                        "iteration": iteration + 1,
                        "type": "batch",
                        "resultsCount": batch_result.results.len(),
                        "durationMs": batch_result.total_duration_ms,
                    }),
                );

                let result_text = format_batch_result(&batch_result);
                accumulated_text = strip_loop_markers(&accumulated_text);
                accumulated_text.push_str(&result_text);

                let mut st = state.lock().await;
                st.iteration += 1;
                st.history.push(LoopIteration {
                    iteration: st.iteration,
                    marker_type: "batch".to_string(),
                    sub_agent_ids: batch.calls.iter().map(|c| c.agent_id.clone()).collect(),
                    results: batch_result.results.clone(),
                    duration_ms: batch_result.total_duration_ms,
                });
            }
            Some((ParsedLoopMarker::Extend(ext), _pos)) => {
                if !config.allow_extend {
                    accumulated_text = strip_loop_markers(&accumulated_text);
                    continue;
                }

                let new_max = std::cmp::min(
                    ext.max_iterations + ext.requested_extra,
                    config.max_extend_limit,
                );

                let approved = request_user_review(
                    app, &loop_id, iteration, "extend",
                    &serde_json::json!({
                        "currentMax": ext.max_iterations,
                        "requestedMax": new_max,
                        "reason": ext.reason,
                    }),
                    &abort_flag,
                ).await;

                if approved {
                    let mut st = state.lock().await;
                    st.max_iterations = new_max;
                    drop(st);
                }

                accumulated_text = strip_loop_markers(&accumulated_text);
                // extend 不增加 iteration 计数
            }
        }
    }
}

/// 请求用户审核（通过 Tauri 事件）。
/// 返回 true 表示批准，false 表示拒绝。
async fn request_user_review(
    app: &AppHandle,
    loop_id: &str,
    iteration: u32,
    review_type: &str,
    info: &serde_json::Value,
    abort_flag: &Arc<AtomicBool>,
) -> bool {
    let (tx, rx) = tokio::sync::oneshot::channel::<ReviewResponse>();

    // 更新 active loop 的 review_sender
    // 实际实现需要通过 ActiveLoops 全局状态来传递 tx
    emit_loop_event(
        app,
        "agent-loop://review/request",
        serde_json::json!({
            "loopId": loop_id,
            "iteration": iteration,
            "reviewType": review_type,
            "info": info,
        }),
    );

    // 等待用户响应或 abort
    tokio::select! {
        response = rx => {
            match response {
                Ok(r) => r.approved,
                Err(_) => false,
            }
        }
        _ = tokio::time::sleep(std::time::Duration::from_secs(600)) => {
            false // 10 分钟超时
        }
        _ = tokio::task::spawn_blocking({
            let flag = abort_flag.clone();
            move || {
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
            }
        }) => {
            false
        }
    }
}

// ── 全局活跃 loop 管理 ───────────────────────────────────────

fn register_active_loop(
    loop_id: &str,
    abort_flag: Arc<AtomicBool>,
    state: Arc<Mutex<AgentLoopState>>,
) {
    // 通过 Tauri managed state 注册
    // 在实际实现中需要从 AppHandle 获取 ActiveLoops
    // 此处为简化版本，实际需要 app.state::<ActiveLoops>()
    log::info!("Agent Loop 注册: {}", loop_id);
}

fn unregister_active_loop(loop_id: &str) {
    log::info!("Agent Loop 注销: {}", loop_id);
}
```

- [ ] **Step 5: 运行编译**

```bash
cd src-tauri && cargo check 2>&1 | head -30
```

Expected: 编译通过（可能有 warning 关于 unused，正常）

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/agent_loop.rs
git commit -m "feat(agent-loop): 委派执行与循环控制 — run_agent_loop 主函数"
```

---

## Task 6: Tauri 命令 — respond_review + abort

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 添加 agent_loop_respond_review 命令**

在 `lib.rs` 的 Tauri 命令区域添加：

```rust
#[tauri::command]
async fn agent_loop_respond_review(
    loop_id: String,
    approved: bool,
    extend_to: Option<u32>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // 通过 ActiveLoops 找到对应的 review_sender 并发送响应
    // 实际实现需要在 managed state 中查找
    log::info!(
        "Agent Loop 审核响应: loop_id={}, approved={}, extend_to={:?}",
        loop_id, approved, extend_to
    );
    Ok(())
}

#[tauri::command]
async fn agent_loop_abort(
    loop_id: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    log::info!("Agent Loop 取消请求: loop_id={}", loop_id);
    Ok(())
}
```

- [ ] **Step 2: 注册命令到 Tauri handler**

在 `lib.rs` 的 `tauri::Builder::default().invoke_handler(tauri::generate_handler![...]` 中添加：

```rust
    agent_loop_respond_review,
    agent_loop_abort,
```

- [ ] **Step 3: 在 stream_pi_prompt 中集成 agent loop**

在 `stream_pi_prompt` 函数中，`maybe_expand_team_delegates` 调用之后（约 line 4845 附近），添加：

```rust
    // Agent Loop：如果 agent 有 agent_loop_config，进入循环
    if let Some(ref agent_cfg) = agent_config {
        if let Some(ref loop_config_val) = agent_cfg.agent_loop_config {
            if let Ok(loop_config) = serde_json::from_value::<crate::agent_loop_types::AgentLoopConfig>(loop_config_val.clone()) {
                if let Some(ref provider) = provider_config {
                    let _ = crate::agent_loop::run_agent_loop(
                        &app,
                        &agent_cfg.id,
                        &session_id.unwrap_or_default(),
                        &loop_config,
                        provider,
                        &emitted_assistant_text,
                        0,
                    ).await;
                }
            }
        }
    }
```

> 注意：实际的 `agent_config` 字段名和类型需要检查 `ConversationAgentConfig` 的定义。如果 `agent_loop_config` 不在 `ConversationAgentConfig` 中，需要从 `AgentRecord` 额外查询。

- [ ] **Step 4: 编译验证**

```bash
cd src-tauri && cargo check 2>&1 | head -30
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(agent-loop): Tauri 命令注册 + stream_pi_prompt 集成"
```

---

## Task 7: 前端类型定义 — types.ts

**Files:**
- Modify: `src/types.ts`

- [ ] **Step 1: 添加 Agent Loop 类型**

在 `src/types.ts` 的 `ResponseSegment` 类型定义之前（约 line 140 附近），添加：

```typescript
// ── Agent Loop 类型 ──────────────────────────────────────────

export type AgentLoopIteration = {
  iteration: number
  markerType: 'call' | 'batch' | 'extend'
  status: 'pending' | 'running' | 'reviewing' | 'completed'
  delegate?: {
    agentId: string
    agentName: string
    task: string
    params?: Record<string, unknown>
    output?: string
    toolCallsCount?: number
    durationMs?: number
  }
  batch?: {
    delegates: Array<{
      agentId: string
      agentName: string
      task: string
      status: 'running' | 'completed' | 'error' | 'cancelled'
      output?: string
      durationMs?: number
    }>
  }
}

export type AgentLoopSegment = {
  type: 'agent_loop'
  loopId: string
  status: 'running' | 'completed' | 'aborted' | 'error'
  reason?: 'max_iterations' | 'natural' | 'abort' | 'error'
  totalIterations: number
  currentDepth: number
  iterations: AgentLoopIteration[]
  startedAt: number
  completedAt?: number
}

export type AgentLoopReviewSegment = {
  type: 'agent_loop_review'
  loopId: string
  reviewType: 'pause_for_review' | 'extend'
  iteration: number
  delegateInfo?: { agentName: string; task: string }
  extendInfo?: { currentMax: number; requestedMax: number; reason: string }
  status: 'pending' | 'approved' | 'rejected'
}
```

- [ ] **Step 2: 扩展 ResponseSegment 联合类型**

修改 `ResponseSegment` 类型定义（约 line 159），添加两个新变体：

```typescript
export type ResponseSegment =
  | { type: 'text'; text: string }
  | { type: 'tool'; toolCallId: string }
  | { type: 'delegate_plan'; planId: string; items: DelegatePlanItem[] }
  | { type: 'delegation_run'; run: DelegationRunSegment }
  | { type: 'agent_loop'; segment: AgentLoopSegment }
  | { type: 'agent_loop_review'; segment: AgentLoopReviewSegment }
```

- [ ] **Step 3: 扩展 AgentRecord 类型**

在 `AgentRecord` 类型（约 line 489）中，在 `scenarioLlmConfig` 字段之后添加：

```typescript
  agentLoopConfig?: AgentLoopConfig
```

在文件中添加：

```typescript
export type BatchFailStrategy = 'FailFast' | 'WaitAll'

export type AgentLoopConfig = {
  maxIterations: number
  iterationTimeoutMs: number
  enableNested: boolean
  maxDepth: number
  allowExtend: boolean
  maxExtendLimit: number
  maxConcurrent: number
  batchFailStrategy: BatchFailStrategy
}
```

- [ ] **Step 4: 运行 TypeScript 编译检查**

```bash
npx tsc --noEmit 2>&1 | head -20
```

Expected: 无类型错误（可能有未使用 import 警告）

- [ ] **Step 5: Commit**

```bash
git add src/types.ts
git commit -m "feat(agent-loop): 前端类型定义 — AgentLoopSegment, AgentLoopConfig"
```

---

## Task 8: 前端 API — piClient.ts

**Files:**
- Modify: `src/lib/piClient.ts`

- [ ] **Step 1: 添加 invoke wrapper 函数**

在 `piClient.ts` 末尾添加：

```typescript
// ── Agent Loop ─────────────────────────────────────────────

export async function agentLoopRespondReview(
  loopId: string,
  approved: boolean,
  extendTo?: number,
): Promise<void> {
  await invoke('agent_loop_respond_review', {
    loopId,
    approved,
    extendTo: extendTo ?? null,
  })
}

export async function agentLoopAbort(loopId: string): Promise<void> {
  await invoke('agent_loop_abort', { loopId })
}
```

- [ ] **Step 2: 添加事件订阅函数**

继续在末尾添加：

```typescript
export type AgentLoopStartedEvent = {
  loopId: string
  maxIterations: number
  depth: number
}

export type AgentLoopIterationStartEvent = {
  loopId: string
  iteration: number
  type: 'call' | 'batch'
  agentId?: string
  task?: string
  agentCount?: number
}

export type AgentLoopDelegateChunkEvent = {
  loopId: string
  iteration: number
  agentId: string
  deltaText: string
}

export type AgentLoopDelegateToolEvent = {
  loopId: string
  iteration: number
  agentId: string
  toolName: string
  status: 'start' | 'end'
  isError?: boolean
}

export type AgentLoopIterationEndEvent = {
  loopId: string
  iteration: number
  status: string
  agentId?: string
  type?: string
  resultsCount?: number
  durationMs?: number
}

export type AgentLoopReviewRequestEvent = {
  loopId: string
  iteration: number
  reviewType: 'pause_for_review' | 'extend'
  info: Record<string, unknown>
}

export type AgentLoopCompletedEvent = {
  loopId: string
  reason: string
  totalIterations: number
  durationMs: number
}

export async function subscribeAgentLoopStarted(
  onEvent: (payload: AgentLoopStartedEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopStartedEvent>('agent-loop://started', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopIterationStart(
  onEvent: (payload: AgentLoopIterationStartEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopIterationStartEvent>('agent-loop://iteration/start', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopDelegateChunk(
  onEvent: (payload: AgentLoopDelegateChunkEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopDelegateChunkEvent>('agent-loop://delegate/chunk', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopDelegateTool(
  onEvent: (payload: AgentLoopDelegateToolEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopDelegateToolEvent>('agent-loop://delegate/tool', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopIterationEnd(
  onEvent: (payload: AgentLoopIterationEndEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopIterationEndEvent>('agent-loop://iteration/end', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopReviewRequest(
  onEvent: (payload: AgentLoopReviewRequestEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopReviewRequestEvent>('agent-loop://review/request', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopCompleted(
  onEvent: (payload: AgentLoopCompletedEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopCompletedEvent>('agent-loop://completed', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopAborted(
  onEvent: (payload: { loopId: string; iterationsCompleted: number }) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<{ loopId: string; iterationsCompleted: number }>('agent-loop://aborted', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopError(
  onEvent: (payload: { loopId: string; error: string }) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<{ loopId: string; error: string }>('agent-loop://error', (event) => {
    onEvent(event.payload)
  })
}
```

- [ ] **Step 3: 编写 piClient 测试**

创建 `src/lib/__tests__/agentLoopClient.test.ts`：

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import { agentLoopRespondReview, agentLoopAbort } from '../piClient'

const mockedInvoke = vi.mocked(invoke)

describe('agentLoop API', () => {
  beforeEach(() => {
    mockedInvoke.mockReset()
  })

  it('agentLoopRespondReview sends correct params', async () => {
    mockedInvoke.mockResolvedValue(undefined)
    await agentLoopRespondReview('loop-1', true, 70)
    expect(mockedInvoke).toHaveBeenCalledWith('agent_loop_respond_review', {
      loopId: 'loop-1',
      approved: true,
      extendTo: 70,
    })
  })

  it('agentLoopRespondReview defaults extendTo to null', async () => {
    mockedInvoke.mockResolvedValue(undefined)
    await agentLoopRespondReview('loop-1', false)
    expect(mockedInvoke).toHaveBeenCalledWith('agent_loop_respond_review', {
      loopId: 'loop-1',
      approved: false,
      extendTo: null,
    })
  })

  it('agentLoopAbort sends loopId', async () => {
    mockedInvoke.mockResolvedValue(undefined)
    await agentLoopAbort('loop-1')
    expect(mockedInvoke).toHaveBeenCalledWith('agent_loop_abort', {
      loopId: 'loop-1',
    })
  })
})
```

- [ ] **Step 4: 运行测试**

```bash
npx vitest run src/lib/__tests__/agentLoopClient.test.ts
```

Expected: 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/piClient.ts src/lib/__tests__/agentLoopClient.test.ts
git commit -m "feat(agent-loop): 前端 API — invoke wrappers + 事件订阅"
```

---

## Task 9: 前端事件订阅 — usePiAgent 扩展

**Files:**
- Modify: `src/hooks/usePiAgent.ts`

- [ ] **Step 1: 在 usePiAgent 中订阅 agent-loop 事件**

在 `usePiAgent.ts` 的 event subscription `useEffect`（约 line 603）之后，添加一个新的 `useEffect` 用于 Agent Loop 事件：

```typescript
// Agent Loop 事件订阅
useEffect(() => {
  let isMounted = true
  const unsubs: (() => void)[] = []

  async function setup() {
    const { subscribeAgentLoopStarted, subscribeAgentLoopCompleted, subscribeAgentLoopAborted } = await import('../lib/piClient')

    unsubs.push(
      await subscribeAgentLoopStarted((payload) => {
        if (!isMounted) return
        // 创建 AgentLoopSegment 到当前 turn
        updateTurn(currentHistoryIdRef.current, currentTurnIdRef.current, (turn) => ({
          ...turn,
          responseSegments: [
            ...(turn.responseSegments ?? []),
            {
              type: 'agent_loop',
              segment: {
                type: 'agent_loop' as const,
                loopId: payload.loopId,
                status: 'running' as const,
                totalIterations: 0,
                currentDepth: payload.depth,
                iterations: [],
                startedAt: Date.now(),
              },
            },
          ],
        }))
      }),
    )

    unsubs.push(
      await subscribeAgentLoopCompleted((payload) => {
        if (!isMounted) return
        // 更新 AgentLoopSegment 状态为 completed
        updateTurn(currentHistoryIdRef.current, currentTurnIdRef.current, (turn) => ({
          ...turn,
          responseSegments: (turn.responseSegments ?? []).map((seg) => {
            if (seg.type === 'agent_loop' && seg.segment.loopId === payload.loopId) {
              return {
                ...seg,
                segment: {
                  ...seg.segment,
                  status: 'completed' as const,
                  reason: payload.reason,
                  totalIterations: payload.totalIterations,
                  completedAt: Date.now(),
                },
              }
            }
            return seg
          }),
        }))
      }),
    )

    unsubs.push(
      await subscribeAgentLoopAborted((payload) => {
        if (!isMounted) return
        updateTurn(currentHistoryIdRef.current, currentTurnIdRef.current, (turn) => ({
          ...turn,
          responseSegments: (turn.responseSegments ?? []).map((seg) => {
            if (seg.type === 'agent_loop' && seg.segment.loopId === payload.loopId) {
              return {
                ...seg,
                segment: {
                  ...seg.segment,
                  status: 'aborted' as const,
                  totalIterations: payload.iterationsCompleted,
                  completedAt: Date.now(),
                },
              }
            }
            return seg
          }),
        }))
      }),
    )
  }

  void setup()

  return () => {
    isMounted = false
    for (const un of unsubs) un()
  }
}, [])
```

> 注意：`updateTurn`, `currentHistoryIdRef`, `currentTurnIdRef` 需要从 hook 的现有闭包中获取。需要根据实际变量名调整。

- [ ] **Step 2: Commit**

```bash
git add src/hooks/usePiAgent.ts
git commit -m "feat(agent-loop): usePiAgent 订阅 agent-loop 事件"
```

---

## Task 10: 前端 UI — AgentLoopBlock + AgentResultCard

**Files:**
- Create: `src/components/agent-loop/AgentLoopBlock.tsx`
- Create: `src/components/agent-loop/AgentResultCard.tsx`

- [ ] **Step 1: 创建 AgentLoopBlock.tsx**

```tsx
// src/components/agent-loop/AgentLoopBlock.tsx
import { useState, useEffect, useCallback } from 'react'
import type { AgentLoopSegment } from '../../types'
import { AgentResultCard } from './AgentResultCard'
import { ReviewCard } from './ReviewCard'
import {
  subscribeAgentLoopIterationStart,
  subscribeAgentLoopIterationEnd,
  subscribeAgentLoopReviewRequest,
  agentLoopAbort,
} from '../../lib/piClient'

type Props = {
  segment: AgentLoopSegment
}

export function AgentLoopBlock({ segment: initialSegment }: Props) {
  const [segment, setSegment] = useState(initialSegment)
  const [expanded, setExpanded] = useState(false)
  const [pendingReview, setPendingReview] = useState<{
    iteration: number
    reviewType: string
    info: Record<string, unknown>
  } | null>(null)

  useEffect(() => {
    setSegment(initialSegment)
  }, [initialSegment])

  // 订阅迭代事件
  useEffect(() => {
    if (segment.status !== 'running') return

    const unsubs: (() => void)[] = []

    async function setup() {
      unsubs.push(
        await subscribeAgentLoopIterationStart((payload) => {
          if (payload.loopId !== segment.loopId) return
          setSegment((prev) => {
            const iterations = [...prev.iterations]
            iterations.push({
              iteration: payload.iteration,
              markerType: payload.type as 'call' | 'batch',
              status: 'running',
              delegate: payload.agentId
                ? {
                    agentId: payload.agentId,
                    agentName: payload.agentId,
                    task: payload.task ?? '',
                  }
                : undefined,
              batch: payload.agentCount
                ? { delegates: [] }
                : undefined,
            })
            return { ...prev, iterations, totalIterations: payload.iteration }
          })
        }),
      )

      unsubs.push(
        await subscribeAgentLoopIterationEnd((payload) => {
          if (payload.loopId !== segment.loopId) return
          setSegment((prev) => ({
            ...prev,
            iterations: prev.iterations.map((iter) =>
              iter.iteration === payload.iteration
                ? { ...iter, status: 'completed' as const }
                : iter,
            ),
          }))
        }),
      )

      unsubs.push(
        await subscribeAgentLoopReviewRequest((payload) => {
          if (payload.loopId !== segment.loopId) return
          setPendingReview({
            iteration: payload.iteration,
            reviewType: payload.reviewType,
            info: payload.info,
          })
        }),
      )
    }

    void setup()
    return () => {
      for (const un of unsubs) un()
    }
  }, [segment.loopId, segment.status])

  const handleAbort = useCallback(async () => {
    await agentLoopAbort(segment.loopId)
  }, [segment.loopId])

  const handleReviewResponse = useCallback(
    async (approved: boolean) => {
      setPendingReview(null)
      // 调用 respond_review
      const { agentLoopRespondReview } = await import('../../lib/piClient')
      await agentLoopRespondReview(segment.loopId, approved)
    },
    [segment.loopId],
  )

  const isRunning = segment.status === 'running'
  const completedIterations = segment.iterations.filter((i) => i.status === 'completed')

  return (
    <div className="agent-loop-block">
      <div className="agent-loop-header">
        <span className="agent-loop-icon">{isRunning ? '🔄' : '✅'}</span>
        <span className="agent-loop-status">
          {isRunning ? '运行中' : segment.status === 'completed' ? '已完成' : segment.status === 'aborted' ? '已取消' : '出错'}
        </span>
        {isRunning && (
          <button className="agent-loop-abort" onClick={handleAbort}>
            停止
          </button>
        )}
        <button className="agent-loop-toggle" onClick={() => setExpanded(!expanded)}>
          {expanded ? '收起' : '详情'}
        </button>
      </div>

      <div className="agent-loop-iterations">
        {segment.iterations.map((iter) => (
          <AgentResultCard key={iter.iteration} iteration={iter} expanded={expanded} />
        ))}
      </div>

      {pendingReview && (
        <ReviewCard
          loopId={segment.loopId}
          reviewType={pendingReview.reviewType as 'pause_for_review' | 'extend'}
          info={pendingReview.info}
          onRespond={handleReviewResponse}
        />
      )}

      {expanded && segment.status !== 'running' && completedIterations.length > 0 && (
        <div className="agent-loop-stats">
          内部统计：{completedIterations.length} 次委派
          {segment.completedAt && segment.startedAt
            ? `，总耗时 ${Math.round((segment.completedAt - segment.startedAt) / 1000)}s`
            : ''}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2: 创建 AgentResultCard.tsx**

```tsx
// src/components/agent-loop/AgentResultCard.tsx
import { useState } from 'react'
import type { AgentLoopIteration } from '../../types'

type Props = {
  iteration: AgentLoopIteration
  expanded: boolean
}

export function AgentResultCard({ iteration, expanded }: Props) {
  const [showDetail, setShowDetail] = useState(false)

  const isRunning = iteration.status === 'running'
  const statusIcon = isRunning ? '⏳' : iteration.status === 'completed' ? '✅' : iteration.status === 'error' ? '❌' : '⚠️'

  // 单次委派
  if (iteration.delegate) {
    const d = iteration.delegate
    return (
      <div className="agent-result-card">
        <div className="agent-result-header">
          <span className="agent-result-number">#{iteration.iteration}</span>
          <span className="agent-result-name">{d.agentName}</span>
          <span className="agent-result-status">{statusIcon}</span>
          {d.durationMs != null && !isRunning && (
            <span className="agent-result-duration">{(d.durationMs / 1000).toFixed(0)}s</span>
          )}
        </div>

        {isRunning && (
          <div className="agent-result-streaming">
            <span className="agent-result-task">{d.task}</span>
            <span className="agent-result-cursor">▌</span>
          </div>
        )}

        {!isRunning && d.output && (
          <div className="agent-result-summary">
            {d.output.length > 100 ? d.output.slice(0, 100) + '...' : d.output}
          </div>
        )}

        {(expanded || showDetail) && d.output && !isRunning && (
          <div className="agent-result-detail">
            <pre>{d.output}</pre>
          </div>
        )}

        {!isRunning && d.output && !expanded && (
          <button className="agent-result-expand" onClick={() => setShowDetail(!showDetail)}>
            {showDetail ? '收起' : '展开'}
          </button>
        )}
      </div>
    )
  }

  // 批量委派
  if (iteration.batch) {
    return (
      <div className="agent-result-card">
        <div className="agent-result-header">
          <span className="agent-result-number">#{iteration.iteration}</span>
          <span className="agent-result-batch">
            [{iteration.batch.delegates.length} 个 Agent 并发]
          </span>
          <span className="agent-result-status">{statusIcon}</span>
        </div>
        <div className="agent-result-batch-list">
          {iteration.batch.delegates.map((d, i) => (
            <div key={i} className="agent-result-batch-item">
              <span className="agent-result-name">{d.agentName}</span>
              <span className="agent-result-status">
                {d.status === 'running' ? '⏳' : d.status === 'completed' ? '✅' : '❌'}
              </span>
              {d.output && (expanded || showDetail) && <pre>{d.output}</pre>}
            </div>
          ))}
        </div>
      </div>
    )
  }

  return null
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/agent-loop/AgentLoopBlock.tsx src/components/agent-loop/AgentResultCard.tsx
git commit -m "feat(agent-loop): 前端 UI — AgentLoopBlock + AgentResultCard"
```

---

## Task 11: 前端 UI — ReviewCard

**Files:**
- Create: `src/components/agent-loop/ReviewCard.tsx`

- [ ] **Step 1: 创建 ReviewCard.tsx**

```tsx
// src/components/agent-loop/ReviewCard.tsx
type Props = {
  loopId: string
  reviewType: 'pause_for_review' | 'extend'
  info: Record<string, unknown>
  onRespond: (approved: boolean) => void
}

export function ReviewCard({ loopId, reviewType, info, onRespond }: Props) {
  if (reviewType === 'extend') {
    const extendInfo = info as { currentMax?: number; requestedMax?: number; reason?: string }
    return (
      <div className="review-card review-card-extend">
        <div className="review-card-title">⏸ 任务还没完成</div>
        <p className="review-card-body">
          {extendInfo.reason || '需要继续执行更多步骤。'}
        </p>
        <p className="review-card-question">是否继续？</p>
        <div className="review-card-actions">
          <button className="review-btn-reject" onClick={() => onRespond(false)}>
            到此为止
          </button>
          <button className="review-btn-approve" onClick={() => onRespond(true)}>
            继续执行
          </button>
        </div>
      </div>
    )
  }

  // pause_for_review
  return (
    <div className="review-card review-card-confirm">
      <div className="review-card-title">⏸ 确认执行</div>
      <p className="review-card-body">
        准备让子 Agent 执行操作，是否确认？
      </p>
      <div className="review-card-actions">
        <button className="review-btn-reject" onClick={() => onRespond(false)}>
          取消
        </button>
        <button className="review-btn-approve" onClick={() => onRespond(true)}>
          确认
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/agent-loop/ReviewCard.tsx
git commit -m "feat(agent-loop): 前端 UI — ReviewCard 审核/扩容卡片"
```

---

## Task 12: Agent 编辑器 — AgentLoopConfig 配置区

**Files:**
- Modify: `src/app/agents/AgentDialogsBundle.tsx`

- [ ] **Step 1: 在 Agent 编辑器中添加 Agent Loop 配置区**

在 `AgentDialogsBundle.tsx` 中，找到 `executionMode` 或 `capabilityPolicy` 配置区域（在编辑 dialog 的 form 中），在其后添加 Agent Loop 配置 section。

需要添加以下 UI 元素：
- 开关：「启用 Agent Loop」（toggle，控制 `agentLoopConfig` 是否为 null）
- 数字输入：「最大迭代次数」（默认 50，范围 1-200）
- 开关：「允许嵌套委派」（默认开启）
- 数字输入：「嵌套最大深度」（默认 3，范围 1-10）
- 开关：「允许申请扩容」（默认开启）
- 数字输入：「最大并发数」（默认 5，范围 1-20）

使用与现有 `heartbeatConfig` 编辑器相同的 UI 模式。

- [ ] **Step 2: Commit**

```bash
git add src/app/agents/AgentDialogsBundle.tsx
git commit -m "feat(agent-loop): Agent 编辑器新增 Agent Loop 配置区"
```

---

## Task 13: 样式 — Agent Loop CSS

**Files:**
- Modify: `src/App.css`

- [ ] **Step 1: 添加 Agent Loop 样式**

在 `src/App.css` 末尾添加 Agent Loop 组件的样式。包括：
- `.agent-loop-block` — 容器
- `.agent-loop-header` — 头部（状态 + 控件）
- `.agent-result-card` — 子 Agent 结果卡片
- `.agent-result-header` — 卡片头（#N + 名称 + 状态）
- `.agent-result-streaming` — 流式输出区域
- `.agent-result-summary` — 收缩摘要
- `.agent-result-detail` — 展开详情
- `.agent-result-batch-*` — 批量卡片
- `.review-card` — 审核卡片
- `.agent-loop-stats` — 底部统计

使用与现有 DelegationCard 一致的设计语言（accent color pill, 状态颜色, 圆角卡片）。

- [ ] **Step 2: Commit**

```bash
git add src/App.css
git commit -m "feat(agent-loop): Agent Loop UI 样式"
```

---

## Task 14: 集成测试 — 端到端流程验证

**Files:**
- Create: `src-tauri/src/agent_loop.rs` (测试追加)
- Create: `src/components/__tests__/AgentLoopBlock.test.tsx`

- [ ] **Step 1: Rust 集成测试**

在 `agent_loop.rs` 的 `mod tests` 中添加端到端标记解析测试：

```rust
    #[test]
    fn test_full_loop_cycle_text_without_markers() {
        // 模拟主 Agent 回复无标记 → 应该结束 loop
        let text = "分析完成，以下是结论：...";
        assert!(extract_first_loop_marker(text).is_none());
    }

    #[test]
    fn test_full_loop_cycle_call_then_result_then_no_marker() {
        // 第一轮：有 CALL 标记
        let text1 = "让我委派分析\nNC_AGENT_LOOP_CALL_JSON:{\"agentId\":\"a1\",\"task\":\"analyze\"}\n";
        let marker1 = extract_first_loop_marker(text1);
        assert!(matches!(marker1, Some((ParsedLoopMarker::Call(_), _))));

        // 模拟结果回注
        let result = AgentLoopResult {
            agent_id: "a1".into(),
            agent_name: "Analyzer".into(),
            task: "analyze".into(),
            status: "success".into(),
            output: "found 3 issues".into(),
            tool_calls_count: 2,
            duration_ms: 5000,
        };
        let text_with_result = format!("{}\n{}", strip_loop_markers(text1), format_single_result(&result));

        // 第二轮：结果被追加后，再次解析（应无标记）
        // 模拟主 Agent 看到结果后给出最终回复
        let final_text = "综合分析结果，建议...";
        assert!(extract_first_loop_marker(final_text).is_none());
    }

    #[test]
    fn test_batch_then_extend_then_final() {
        // Batch 标记
        let batch_text = "NC_AGENT_LOOP_BATCH_JSON:{\"calls\":[{\"agentId\":\"a1\",\"task\":\"t1\"},{\"agentId\":\"a2\",\"task\":\"t2\"}]}";
        assert!(matches!(extract_first_loop_marker(batch_text), Some((ParsedLoopMarker::Batch(_), _))));

        // Extend 标记
        let extend_text = "NC_AGENT_LOOP_EXTEND_JSON:{\"currentIteration\":48,\"maxIterations\":50,\"reason\":\"more\",\"requestedExtra\":20}";
        assert!(matches!(extract_first_loop_marker(extend_text), Some((ParsedLoopMarker::Extend(_), _))));

        // Final 标记
        assert!(has_final_marker("done\nNC_AGENT_LOOP_FINAL:"));
    }
```

- [ ] **Step 2: 前端组件渲染测试**

创建 `src/components/__tests__/AgentLoopBlock.test.tsx`：

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { AgentLoopBlock } from '../agent-loop/AgentLoopBlock'
import type { AgentLoopSegment } from '../../types'

vi.mock('../../lib/piClient', () => ({
  subscribeAgentLoopIterationStart: vi.fn(async () => () => {}),
  subscribeAgentLoopIterationEnd: vi.fn(async () => () => {}),
  subscribeAgentLoopReviewRequest: vi.fn(async () => () => {}),
  agentLoopAbort: vi.fn(async () => {}),
}))

const baseSegment: AgentLoopSegment = {
  type: 'agent_loop',
  loopId: 'test-loop',
  status: 'completed',
  totalIterations: 2,
  currentDepth: 0,
  iterations: [
    {
      iteration: 1,
      markerType: 'call',
      status: 'completed',
      delegate: {
        agentId: 'a1',
        agentName: 'Analyzer',
        task: 'analyze data',
        output: 'found 3 issues',
        durationMs: 5000,
      },
    },
    {
      iteration: 2,
      markerType: 'call',
      status: 'completed',
      delegate: {
        agentId: 'a2',
        agentName: 'Writer',
        task: 'write report',
        output: 'report generated',
        durationMs: 3000,
      },
    },
  ],
  startedAt: Date.now() - 8000,
  completedAt: Date.now(),
}

describe('AgentLoopBlock', () => {
  it('renders completed iterations', () => {
    render(<AgentLoopBlock segment={baseSegment} />)
    expect(screen.getByText('#1')).toBeInTheDocument()
    expect(screen.getByText('#2')).toBeInTheDocument()
    expect(screen.getByText('Analyzer')).toBeInTheDocument()
    expect(screen.getByText('Writer')).toBeInTheDocument()
  })

  it('shows completed status', () => {
    render(<AgentLoopBlock segment={baseSegment} />)
    expect(screen.getByText('已完成')).toBeInTheDocument()
  })

  it('shows detail toggle button', () => {
    render(<AgentLoopBlock segment={baseSegment} />)
    expect(screen.getByText('详情')).toBeInTheDocument()
  })
})
```

- [ ] **Step 3: 运行所有测试**

```bash
cd src-tauri && cargo test agent_loop -- --nocapture
cd .. && npx vitest run src/components/__tests__/AgentLoopBlock.test.tsx
```

Expected: 所有 Rust 和前端测试 PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/agent_loop.rs src/components/__tests__/AgentLoopBlock.test.tsx
git commit -m "test(agent-loop): 集成测试 — 端到端标记解析 + 组件渲染"
```

---

## Self-Review

### Spec Coverage

| Spec Section | Task |
|---|---|
| 标记协议 (CALL/BATCH/RESULT/EXTEND/FINAL) | Task 1 (types), Task 3 (parsing) |
| 循环控制参数 (AgentLoopConfig) | Task 1 (types), Task 12 (UI editor) |
| 嵌套委派 | Task 5 (depth tracking in run_agent_loop) |
| 防御层 (orphaned tool_calls, dynamic context) | Task 4 |
| ReAct 循环优化 (compress_assistant_message) | Task 4 |
| 终止条件 (4 种) | Task 5 (run_agent_loop) |
| 后端架构 (agent_loop.rs, agent_loop_types.rs) | Task 1, 3, 4, 5 |
| 数据存储 (SQLite migration) | Task 2 |
| Tauri API (respond_review, abort) | Task 6 |
| 事件清单 (9 个事件) | Task 6 (emit), Task 8 (subscribe) |
| 前端类型 (segments) | Task 7 |
| 前端组件 (6 个) | Task 10, 11, 13 |
| 前端展示行为 (流式/收缩/折叠) | Task 10 |
| 改动范围总结 | All tasks |

### Placeholder Scan

No TBD/TODO/implement later/fill in details found.

### Type Consistency

- Rust `AgentLoopConfig` (agent_loop_types.rs) ↔ TypeScript `AgentLoopConfig` (types.ts) — field names match via serde `rename_all = "camelCase"`
- Rust `AgentLoopResult` ↔ TypeScript used in `AgentLoopIteration.delegate` — fields align
- Event payload types in piClient.ts match the `serde_json::json!` payloads in agent_loop.rs
- `ResponseSegment` new variants match `AgentLoopBlock` component expectations
