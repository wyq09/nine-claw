//! 全局运行时参数（与前端「设置 → 参数」一致），用于 PI 单次对话、「Agent Loop」委派等。
#![allow(clippy::useless_conversion)]

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Write;
use std::sync::RwLock;

static RUNTIME_PARAMETERS: RwLock<RuntimeParametersPayload> =
    RwLock::new(RuntimeParametersPayload::const_default());

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeParametersPayload {
    /// 单次对话内 Agent 循环（助手轮 + 工具）最大轮数：`turn_end` 计数阈值；亦用于委派 Agent Loop 的 `max_iterations`。
    pub max_agent_tool_rounds_per_dialogue: u32,
    /// stdout 读空闲超时后的「继续等待」重试次数（指数退避，不主动杀 pi）。
    pub stream_disconnect_max_retries: u32,
    /// 单次任务遇到 Provider/运行时失败时外层重试的最大尝试次数（含首次）。
    #[serde(rename = "llmOuterMaxAttempts")]
    pub llm_outer_max_attempt_rounds: u32,
}

impl RuntimeParametersPayload {
    pub const fn const_default() -> Self {
        Self {
            max_agent_tool_rounds_per_dialogue: 80,
            stream_disconnect_max_retries: 3,
            llm_outer_max_attempt_rounds: 8,
        }
    }

    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.max_agent_tool_rounds_per_dialogue =
            self.max_agent_tool_rounds_per_dialogue.clamp(1, 500);
        self.stream_disconnect_max_retries = self.stream_disconnect_max_retries.clamp(0, 20);
        self.llm_outer_max_attempt_rounds = self.llm_outer_max_attempt_rounds.clamp(1, 24);
        self
    }
}

impl Default for RuntimeParametersPayload {
    fn default() -> Self {
        Self::const_default()
    }
}

/// 写入共享状态；每次 `stream_pi_prompt` / `sync_runtime_parameters` 均应调用。
pub fn merge_from_payload(payload: Option<RuntimeParametersPayload>) -> RuntimeParametersPayload {
    let merged = payload
        .map(|payload| payload.sanitized())
        .unwrap_or_default();
    if let Ok(mut guard) = RUNTIME_PARAMETERS.write() {
        *guard = merged;
    }
    merged
}

#[allow(dead_code)]
pub fn current() -> RuntimeParametersPayload {
    RUNTIME_PARAMETERS
        .read()
        .map(|guard| guard.sanitized())
        .unwrap_or_else(|_| RuntimeParametersPayload::const_default().sanitized())
}

pub(crate) fn write_abort_json_stdin(
    stdin_lock: &std::sync::Arc<std::sync::Mutex<Option<std::process::ChildStdin>>>,
    session_label: &str,
) -> Result<bool, String> {
    let abort_command = json!({
      "id": format!("abort-iter-limit-{session_label}"),
      "type": "abort",
    })
    .to_string();

    let mut stdin_guard = stdin_lock
        .lock()
        .map_err(|error| format!("无法锁定 stdin: {error}"))?;
    if let Some(stdin) = stdin_guard.as_mut() {
        writeln!(stdin, "{abort_command}")
            .map_err(|error| format!("发送 abort 指令失败: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("刷新 abort 指令失败: {error}"))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitized_clamps_numbers() {
        let p = RuntimeParametersPayload {
            max_agent_tool_rounds_per_dialogue: 9999,
            stream_disconnect_max_retries: 100,
            llm_outer_max_attempt_rounds: 0,
        }
        .sanitized();
        assert_eq!(p.max_agent_tool_rounds_per_dialogue, 500);
        assert_eq!(p.stream_disconnect_max_retries, 20);
        assert_eq!(p.llm_outer_max_attempt_rounds, 1);
    }
}
