//! Agent Loop type definitions.
//!
//! Foundation types for the Agent Loop feature: configuration, markers, runtime
//! state, and error types used by `agent_loop.rs` (engine) and `agents.rs`
//! (data model).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{Mutex as TokioMutex, oneshot};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Strategy for handling failures within a batch of agent calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BatchFailStrategy {
    /// Abort the entire batch as soon as one call fails.
    FailFast,
    /// Wait for every call to finish regardless of individual failures.
    WaitAll,
}

impl Default for BatchFailStrategy {
    fn default() -> Self {
        Self::WaitAll
    }
}

/// Top-level configuration for an agent loop run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    /// 单轮委派超时（毫秒），超时则强制中断该轮并继续。
    #[serde(default = "default_iteration_timeout_ms")]
    pub iteration_timeout_ms: u64,
    /// 整体超时（毫秒），超时则强制结束循环。0 表示不限制。
    #[serde(default = "default_total_timeout_ms")]
    pub total_timeout_ms: u64,
    #[serde(default = "default_true")]
    pub enable_nested: bool,
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    #[serde(default = "default_true")]
    pub allow_extend: bool,
    #[serde(default = "default_max_extend_limit")]
    pub max_extend_limit: u32,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
    #[serde(default)]
    pub batch_fail_strategy: BatchFailStrategy,
}

fn default_max_iterations() -> u32 {
    50
}
fn default_iteration_timeout_ms() -> u64 {
    120_000
}
fn default_total_timeout_ms() -> u64 {
    600_000
}
fn default_true() -> bool {
    true
}
fn default_max_depth() -> u32 {
    3
}
fn default_max_extend_limit() -> u32 {
    200
}
fn default_max_concurrent() -> u32 {
    5
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            iteration_timeout_ms: default_iteration_timeout_ms(),
            total_timeout_ms: default_total_timeout_ms(),
            enable_nested: default_true(),
            max_depth: default_max_depth(),
            allow_extend: default_true(),
            max_extend_limit: default_max_extend_limit(),
            max_concurrent: default_max_concurrent(),
            batch_fail_strategy: BatchFailStrategy::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Marker JSON structs — produced by the LLM to request loop actions
// ---------------------------------------------------------------------------

/// Marker for a single agent call within the loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopCallMarker {
    pub agent_id: String,
    pub task: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub context_injection: Option<serde_json::Value>,
    #[serde(default)]
    pub expect_structured_output: bool,
    pub output_format_hint: Option<String>,
    #[serde(default)]
    pub pause_for_review: bool,
}

/// Marker for a batch of agent calls to execute concurrently.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopBatchMarker {
    pub calls: Vec<AgentLoopCallMarker>,
    #[serde(default)]
    pub pause_for_review: bool,
}

/// Marker to request an extension of the iteration limit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopExtendMarker {
    pub current_iteration: u32,
    pub max_iterations: u32,
    pub reason: String,
    pub requested_extra: u32,
}

/// Result of a single agent call within the loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopResult {
    pub agent_id: String,
    pub agent_name: String,
    pub task: String,
    pub status: String,
    pub output: String,
    #[serde(default)]
    pub tool_calls_count: u32,
    #[serde(default)]
    pub duration_ms: u64,
}

/// Result of a batch of agent calls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentLoopBatchResult {
    pub batch_id: String,
    pub results: Vec<AgentLoopResult>,
    pub total_duration_ms: u64,
}

/// Parsed form of a marker emitted by the LLM inside the agent loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ParsedLoopMarker {
    Call(AgentLoopCallMarker),
    Batch(AgentLoopBatchMarker),
    Extend(AgentLoopExtendMarker),
}

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

/// Snapshot of a single iteration within the loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopIteration {
    pub iteration: u32,
    pub marker_type: String,
    pub sub_agent_ids: Vec<String>,
    pub results: Vec<AgentLoopResult>,
    pub duration_ms: u64,
}

/// Full state of a running agent loop.
#[derive(Debug)]
pub struct AgentLoopState {
    pub loop_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub iteration: u32,
    pub max_iterations: u32,
    pub depth: u32,
    pub started_at: std::time::Instant,
    pub history: Vec<LoopIteration>,
    pub permission_denials: Vec<String>,
}

/// Response from a human-in-the-loop review gate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResponse {
    pub approved: bool,
    pub extend_to: Option<u32>,
}

/// Handle held by the active-loops registry for a running loop.
pub struct ActiveLoopHandle {
    pub abort_flag: Arc<AtomicBool>,
    pub review_sender: Option<oneshot::Sender<ReviewResponse>>,
    pub state: Arc<TokioMutex<AgentLoopState>>,
}

/// Global registry of all currently running agent loops.
pub struct ActiveLoops {
    pub loops: StdMutex<HashMap<String, ActiveLoopHandle>>,
}

// ---------------------------------------------------------------------------
// Unrecoverable errors
// ---------------------------------------------------------------------------

/// Errors that cannot be retried within the agent loop.
#[derive(Debug)]
pub enum UnrecoverableError {
    ContextCorrupted,
    InvalidModelOutput,
    SafetyViolation,
    AgentNotFound(String),
    ProviderAuthFailed,
    NestedDepthExceeded,
}

impl fmt::Display for UnrecoverableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextCorrupted => write!(f, "ContextCorrupted: loop context is unreadable"),
            Self::InvalidModelOutput => write!(f, "InvalidModelOutput: model output could not be parsed"),
            Self::SafetyViolation => write!(f, "SafetyViolation: loop action blocked by safety policy"),
            Self::AgentNotFound(id) => write!(f, "AgentNotFound: agent '{id}' does not exist"),
            Self::ProviderAuthFailed => write!(f, "ProviderAuthFailed: LLM provider authentication failed"),
            Self::NestedDepthExceeded => write!(f, "NestedDepthExceeded: maximum nesting depth exceeded"),
        }
    }
}

impl std::error::Error for UnrecoverableError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let cfg = AgentLoopConfig::default();
        assert_eq!(cfg.max_iterations, 50);
        assert_eq!(cfg.iteration_timeout_ms, 120_000);
        assert!(cfg.enable_nested);
        assert_eq!(cfg.max_depth, 3);
        assert!(cfg.allow_extend);
        assert_eq!(cfg.max_extend_limit, 200);
        assert_eq!(cfg.max_concurrent, 5);
        assert_eq!(cfg.batch_fail_strategy, BatchFailStrategy::WaitAll);
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let cfg = AgentLoopConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AgentLoopConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn test_config_from_partial_json() {
        let json = r#"{"maxIterations": 10}"#;
        let cfg: AgentLoopConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.max_iterations, 10);
        // rest should be defaults
        assert_eq!(cfg.iteration_timeout_ms, 120_000);
        assert!(cfg.enable_nested);
        assert_eq!(cfg.max_depth, 3);
        assert!(cfg.allow_extend);
        assert_eq!(cfg.max_extend_limit, 200);
        assert_eq!(cfg.max_concurrent, 5);
        assert_eq!(cfg.batch_fail_strategy, BatchFailStrategy::WaitAll);
    }

    #[test]
    fn test_call_marker_parse() {
        let json = r#"{
            "agentId": "agent-1",
            "task": "Summarize the document",
            "params": {"key": "value"},
            "contextInjection": {"system": "context"},
            "expectStructuredOutput": true,
            "outputFormatHint": "json",
            "pauseForReview": false
        }"#;
        let marker: AgentLoopCallMarker = serde_json::from_str(json).unwrap();
        assert_eq!(marker.agent_id, "agent-1");
        assert_eq!(marker.task, "Summarize the document");
        assert_eq!(marker.params["key"], "value");
        assert!(marker.context_injection.is_some());
        assert!(marker.expect_structured_output);
        assert_eq!(marker.output_format_hint.as_deref(), Some("json"));
        assert!(!marker.pause_for_review);
    }

    #[test]
    fn test_batch_marker_parse() {
        let json = r#"{
            "calls": [
                {"agentId": "a1", "task": "t1", "params": {}},
                {"agentId": "a2", "task": "t2", "params": {}}
            ],
            "pauseForReview": true
        }"#;
        let marker: AgentLoopBatchMarker = serde_json::from_str(json).unwrap();
        assert_eq!(marker.calls.len(), 2);
        assert_eq!(marker.calls[0].agent_id, "a1");
        assert_eq!(marker.calls[1].agent_id, "a2");
        assert!(marker.pause_for_review);
    }

    #[test]
    fn test_extend_marker_parse() {
        let json = r#"{
            "currentIteration": 45,
            "maxIterations": 50,
            "reason": "Need more iterations to complete sub-tasks",
            "requestedExtra": 20
        }"#;
        let marker: AgentLoopExtendMarker = serde_json::from_str(json).unwrap();
        assert_eq!(marker.current_iteration, 45);
        assert_eq!(marker.max_iterations, 50);
        assert_eq!(marker.reason, "Need more iterations to complete sub-tasks");
        assert_eq!(marker.requested_extra, 20);
    }

    #[test]
    fn test_unrecoverable_error_display() {
        assert_eq!(
            UnrecoverableError::ContextCorrupted.to_string(),
            "ContextCorrupted: loop context is unreadable"
        );
        assert_eq!(
            UnrecoverableError::InvalidModelOutput.to_string(),
            "InvalidModelOutput: model output could not be parsed"
        );
        assert_eq!(
            UnrecoverableError::SafetyViolation.to_string(),
            "SafetyViolation: loop action blocked by safety policy"
        );
        assert_eq!(
            UnrecoverableError::AgentNotFound("xyz".into()).to_string(),
            "AgentNotFound: agent 'xyz' does not exist"
        );
        assert_eq!(
            UnrecoverableError::ProviderAuthFailed.to_string(),
            "ProviderAuthFailed: LLM provider authentication failed"
        );
        assert_eq!(
            UnrecoverableError::NestedDepthExceeded.to_string(),
            "NestedDepthExceeded: maximum nesting depth exceeded"
        );
    }
}
