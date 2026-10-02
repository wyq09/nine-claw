//! Plain-HTTPS client for TypeSafe's Jev systemone API.
//!
//! The HTTP layer is a thin transport fn (`JevTransport`) so everything
//! around it — the exact request JSON shape, the single 429/529 retry, and
//! response parsing — is unit-tested with canned bytes instead of a live
//! network call. The production transport uses `reqwest::blocking`, which
//! MUST run off the tokio runtime (see `decision::run_with_overall_timeout`,
//! which spawns a dedicated std thread).

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::tool_router::policy::{
    gateable_catalog_for, noul_instructions, truncate_user_message, GateableTool,
    HARNESS_PROFILE_CRITERIA_CHAT, HARNESS_PROFILE_CRITERIA_CODE,
    HARNESS_PROFILE_CRITERIA_DEFAULT, HARNESS_PROFILE_INSTRUCTIONS,
    HARNESS_PROFILE_QUESTION_ID, JEV_API_URL,
};

/// A transport performs one HTTP round trip. Tests inject closures
/// returning canned statuses/bodies; production uses [`http_transport`].
pub(crate) type JevTransport<'a> = dyn Fn(&JevHttpRequest) -> Result<JevHttpResponse, String> + 'a;

pub(crate) struct JevHttpRequest {
    pub(crate) url: String,
    /// Sent as `Authorization: Bearer <token>`.
    pub(crate) bearer_token: String,
    pub(crate) body: String,
    /// Per-attempt request timeout.
    pub(crate) timeout_ms: u64,
}

pub(crate) struct JevHttpResponse {
    pub(crate) status: u16,
    pub(crate) body: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(crate) struct JevAnswer {
    #[serde(rename = "type", default)]
    pub(crate) answer_type: Option<String>,
    #[serde(default)]
    pub(crate) choice: Option<String>,
    #[serde(default)]
    pub(crate) probabilities: Option<HashMap<String, f64>>,
    #[serde(default)]
    pub(crate) confidence: Option<f64>,
    #[serde(default)]
    pub(crate) noul: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(crate) struct JevUsage {
    #[serde(default)]
    pub(crate) input_tokens: u64,
    #[serde(default)]
    pub(crate) output_tokens: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct JevResponse {
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) answers: HashMap<String, JevAnswer>,
    #[serde(default)]
    pub(crate) usage: Option<JevUsage>,
}

/// Everything needed to perform one Jev call for a turn.
pub(crate) struct JevCallInput {
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) user_message: String,
    pub(crate) agent_id: String,
    pub(crate) agent_name: String,
    /// Gateable catalog entries already intersected with the whitelist.
    pub(crate) gateable: Vec<&'static GateableTool>,
    /// Overall round-trip budget.
    pub(crate) timeout_ms: u64,
    /// Delay before the single 429/529 retry (tests shrink it).
    pub(crate) retry_after_ms: u64,
}

#[derive(Debug)]
pub(crate) struct JevCallOutcome {
    pub(crate) response: JevResponse,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JevCallError {
    Transport(String),
    /// HTTP status the API documents as non-retryable (401/422/5xx besides
    /// 529) or a retry that stayed out of budget.
    Status { status: u16, body: String },
    Parse(String),
}

impl std::fmt::Display for JevCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JevCallError::Transport(message) => write!(f, "网络请求失败: {message}"),
            JevCallError::Status { status, .. } => write!(f, "HTTP {status}"),
            JevCallError::Parse(message) => write!(f, "响应解析失败: {message}"),
        }
    }
}

/// Build the exact request body per docs.typesafe.ai/api.md: `state` carries
/// the (original-language) user message plus the calling agent, `model` is
/// the pinned version, `questions` holds the harness-profile choice question
/// plus one noul question per gateable tool. Question wording is English;
/// the user message itself is never translated.
pub(crate) fn build_request_body(input: &JevCallInput) -> Value {
    let mut questions = serde_json::Map::new();
    questions.insert(
        HARNESS_PROFILE_QUESTION_ID.to_string(),
        json!({
            "type": "choice",
            "instructions": HARNESS_PROFILE_INSTRUCTIONS,
            "criteria": {
                "code": HARNESS_PROFILE_CRITERIA_CODE,
                "chat": HARNESS_PROFILE_CRITERIA_CHAT,
                "default": HARNESS_PROFILE_CRITERIA_DEFAULT
            }
        }),
    );
    for tool in &input.gateable {
        questions.insert(
            crate::tool_router::policy::question_id_for_tool(tool.runtime_name),
            json!({
                "type": "noul",
                "instructions": noul_instructions(tool)
            }),
        );
    }

    json!({
        "state": {
            "userMessage": truncate_user_message(&input.user_message),
            "agent": {
                "id": input.agent_id.trim(),
                "name": input.agent_name.trim()
            }
        },
        "model": input.model.trim(),
        "questions": Value::Object(questions)
    })
}

pub(crate) fn build_http_request(input: &JevCallInput) -> Result<JevHttpRequest, String> {
    let body = serde_json::to_string(&build_request_body(input))
        .map_err(|error| format!("序列化 Jev 请求失败: {error}"))?;
    Ok(JevHttpRequest {
        url: JEV_API_URL.to_string(),
        bearer_token: input.api_key.trim().to_string(),
        body,
        timeout_ms: input.timeout_ms,
    })
}

/// One attempt plus a single retry on 429/529 (rate limit / overloaded) if
/// the overall budget still allows the extra delay. Everything else fails
/// immediately — 401 bad key, 422 validation, other 5xx.
pub(crate) fn call_jev(
    input: &JevCallInput,
    transport: &JevTransport,
) -> Result<JevCallOutcome, JevCallError> {
    let request = build_http_request(input).map_err(JevCallError::Transport)?;
    let started = std::time::Instant::now();
    let budget = Duration::from_millis(input.timeout_ms.max(1));
    let retry_delay = Duration::from_millis(input.retry_after_ms.min(input.timeout_ms.max(1)));

    let mut attempt = 0u8;
    loop {
        attempt += 1;
        let response = transport(&request).map_err(JevCallError::Transport)?;
        let failed = JevCallError::Status {
            status: response.status,
            body: response.body.clone(),
        };
        match response.status {
            200..=299 => {
                let parsed = parse_response_body(&response.body)?;
                return Ok(JevCallOutcome { response: parsed });
            }
            429 | 529 if attempt == 1 => {
                if started.elapsed() + retry_delay >= budget {
                    return Err(failed);
                }
                std::thread::sleep(retry_delay);
                continue;
            }
            _ => return Err(failed),
        }
    }
}

fn parse_response_body(body: &str) -> Result<JevResponse, JevCallError> {
    serde_json::from_str(body).map_err(|error| JevCallError::Parse(error.to_string()))
}

/// Production transport: plain HTTPS POST via the proxy-aware blocking
/// client, bearer auth, per-request timeout. Must be called from a
/// non-tokio thread (`reqwest::blocking` panics inside a runtime worker).
pub(crate) fn http_transport(request: &JevHttpRequest) -> Result<JevHttpResponse, String> {
    let client = crate::proxy_settings::build_blocking_http_client();
    let response = client
        .post(&request.url)
        .bearer_auth(request.bearer_token.trim())
        .header("content-type", "application/json")
        .timeout(Duration::from_millis(request.timeout_ms.max(1)))
        .body(request.body.clone())
        .send()
        .map_err(|error| format!("Jev 请求失败: {error}"))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .map_err(|error| format!("读取 Jev 响应失败: {error}"))?;
    Ok(JevHttpResponse { status, body })
}

/// Convenience for tests/callers: gateable catalog restricted to a whitelist.
pub(crate) fn gateable_input_catalog(whitelist: &[String]) -> Vec<&'static GateableTool> {
    gateable_catalog_for(whitelist)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_router::policy::RETRY_AFTER_MS;

    fn sample_input() -> JevCallInput {
        JevCallInput {
            api_key: "sk-jev-test".to_string(),
            model: "jev-1.13.0".to_string(),
            user_message: "帮我画一只猫".to_string(),
            agent_id: "agent_1".to_string(),
            agent_name: "画家".to_string(),
            gateable: gateable_input_catalog(&[
                "read".to_string(),
                "image_generate".to_string(),
                "web_search".to_string(),
            ]),
            timeout_ms: 2000,
            retry_after_ms: RETRY_AFTER_MS,
        }
    }

    #[test]
    fn request_body_has_exact_documented_shape() {
        let body = build_request_body(&sample_input());
        let obj = body.as_object().expect("object body");
        let mut keys: Vec<&str> = obj.keys().map(|key| key.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["model", "questions", "state"]);

        let state = obj.get("state").expect("state").as_object().expect("obj");
        assert_eq!(state.get("userMessage").unwrap(), "帮我画一只猫");
        let agent = state.get("agent").unwrap().as_object().expect("obj");
        assert_eq!(agent.get("id").unwrap(), "agent_1");
        assert_eq!(agent.get("name").unwrap(), "画家");

        assert_eq!(obj.get("model").unwrap(), "jev-1.13.0");

        let questions = obj.get("questions").unwrap().as_object().expect("obj");
        assert_eq!(questions.len(), 3, "harness_profile + 2 gateable tools");
        let profile = questions.get("harness_profile").unwrap();
        assert_eq!(profile.get("type").unwrap(), "choice");
        assert_eq!(
            profile.get("instructions").unwrap(),
            "Which working profile fits this turn's task best?"
        );
        let criteria = profile.get("criteria").unwrap().as_object().expect("obj");
        assert_eq!(criteria.len(), 3);
        assert_eq!(
            criteria.get("code").unwrap(),
            "Writing, modifying, debugging, or reviewing code; builds, tests, refactors, running commands, project files"
        );

        let image_question = questions.get("tool_image_generate").unwrap();
        assert_eq!(image_question.get("type").unwrap(), "noul");
        let instructions = image_question.get("instructions").unwrap().as_str().unwrap();
        assert!(instructions.contains("`image_generate`"));
        assert!(instructions.contains("Generate images from a text description"));
        // Core tools never get a question.
        assert!(questions.get("tool_read").is_none());
    }

    #[test]
    fn http_request_carries_bearer_endpoint_and_serialized_body() {
        let request = build_http_request(&sample_input()).expect("build request");
        assert_eq!(request.url, "https://api.typesafe.ai/v1/systemone");
        assert_eq!(request.bearer_token, "sk-jev-test");
        assert_eq!(request.timeout_ms, 2000);
        let parsed: Value = serde_json::from_str(&request.body).expect("body is valid JSON");
        assert_eq!(parsed.get("model").unwrap(), "jev-1.13.0");
    }

    fn ok_body() -> String {
        serde_json::json!({
            "model": "jev-1.13.0",
            "answers": {
                "harness_profile": {
                    "type": "choice",
                    "choice": "code",
                    "probabilities": {"code": 0.8, "chat": 0.1, "default": 0.1},
                    "confidence": 0.7
                },
                "tool_image_generate": {"type": "noul", "noul": 0.12},
                "tool_web_search": {"type": "noul", "noul": 0.82}
            },
            "usage": {"input_tokens": 100, "output_tokens": 40}
        })
        .to_string()
    }

    #[test]
    fn parses_choice_and_noul_answers_with_usage() {
        let transport = |_: &JevHttpRequest| {
            Ok(JevHttpResponse {
                status: 200,
                body: ok_body(),
            })
        };
        let outcome = call_jev(&sample_input(), &transport).expect("call succeeds");
        let profile = outcome
            .response
            .answers
            .get("harness_profile")
            .expect("profile answer");
        assert_eq!(profile.choice.as_deref(), Some("code"));
        assert_eq!(profile.confidence, Some(0.7));
        assert_eq!(
            profile
                .probabilities
                .as_ref()
                .and_then(|p| p.get("code"))
                .copied(),
            Some(0.8)
        );
        assert_eq!(
            outcome
                .response
                .answers
                .get("tool_image_generate")
                .and_then(|a| a.noul),
            Some(0.12)
        );
        assert_eq!(
            outcome.response.usage,
            Some(JevUsage {
                input_tokens: 100,
                output_tokens: 40
            })
        );
        assert_eq!(outcome.response.model.as_deref(), Some("jev-1.13.0"));
    }

    #[test]
    fn non_retryable_status_fails_immediately_without_second_call() {
        let attempts = std::cell::Cell::new(0u32);
        let transport = |_: &JevHttpRequest| {
            attempts.set(attempts.get() + 1);
            Ok(JevHttpResponse {
                status: 401,
                body: "unauthorized".to_string(),
            })
        };
        let error = call_jev(&sample_input(), &transport).unwrap_err();
        assert_eq!(
            error,
            JevCallError::Status {
                status: 401,
                body: "unauthorized".to_string()
            }
        );
        assert_eq!(attempts.get(), 1);
    }

    #[test]
    fn rate_limit_retries_exactly_once_then_succeeds() {
        let attempts = std::cell::Cell::new(0u32);
        let transport = |_: &JevHttpRequest| {
            let n = attempts.get() + 1;
            attempts.set(n);
            if n == 1 {
                Ok(JevHttpResponse {
                    status: 429,
                    body: "rate limited".to_string(),
                })
            } else {
                Ok(JevHttpResponse {
                    status: 200,
                    body: ok_body(),
                })
            }
        };
        let mut input = sample_input();
        input.retry_after_ms = 1;
        let outcome = call_jev(&input, &transport).expect("retry succeeds");
        assert_eq!(attempts.get(), 2);
        assert!(outcome
            .response
            .answers
            .contains_key("harness_profile"));
    }

    #[test]
    fn overloaded_529_retries_once_but_gives_up_after_second_failure() {
        let attempts = std::cell::Cell::new(0u32);
        let transport = |_: &JevHttpRequest| {
            attempts.set(attempts.get() + 1);
            Ok(JevHttpResponse {
                status: 529,
                body: "overloaded".to_string(),
            })
        };
        let mut input = sample_input();
        input.retry_after_ms = 1;
        let error = call_jev(&input, &transport).unwrap_err();
        assert!(matches!(error, JevCallError::Status { status: 529, .. }));
        assert_eq!(attempts.get(), 2, "exactly one retry, never more");
    }

    #[test]
    fn retry_is_skipped_when_budget_is_exhausted() {
        let attempts = std::cell::Cell::new(0u32);
        let transport = |_: &JevHttpRequest| {
            attempts.set(attempts.get() + 1);
            Ok(JevHttpResponse {
                status: 429,
                body: "rate limited".to_string(),
            })
        };
        // Tiny budget: delay (clamped to budget) + elapsed >= budget.
        let mut input = sample_input();
        input.timeout_ms = 1;
        input.retry_after_ms = 5000;
        let error = call_jev(&input, &transport).unwrap_err();
        assert!(matches!(error, JevCallError::Status { status: 429, .. }));
        assert_eq!(attempts.get(), 1);
    }

    #[test]
    fn transport_errors_and_bad_json_surface_as_errors() {
        let transport = |_: &JevHttpRequest| Err("connection refused".to_string());
        assert_eq!(
            call_jev(&sample_input(), &transport).unwrap_err(),
            JevCallError::Transport("connection refused".to_string())
        );

        let bad_json = |_: &JevHttpRequest| {
            Ok(JevHttpResponse {
                status: 200,
                body: "not json".to_string(),
            })
        };
        assert!(matches!(
            call_jev(&sample_input(), &bad_json).unwrap_err(),
            JevCallError::Parse(_)
        ));
    }

    #[test]
    fn tolerates_missing_optional_response_fields() {
        let transport = |_: &JevHttpRequest| {
            Ok(JevHttpResponse {
                status: 200,
                body: serde_json::json!({"answers": {}}).to_string(),
            })
        };
        let outcome = call_jev(&sample_input(), &transport).expect("minimal body parses");
        assert_eq!(outcome.response.model, None);
        assert_eq!(outcome.response.usage, None);
        assert!(outcome.response.answers.is_empty());
    }
}
