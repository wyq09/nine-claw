//! Turn-level tool-selection decision engine.
//!
//! Given settings, the trimmed user prompt, and the whitelist's runtime tool
//! names, produce a [`TurnDecision`]: which gateable tools to prune, whether
//! Jev's harness-profile answer overrides the keyword selection, and a
//! human-readable reason. Every failure path degrades silently to the
//! legacy behavior (full whitelist, keyword harness).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::tool_router::jev_client::{
    call_jev, gateable_input_catalog, http_transport, JevCallError, JevCallInput, JevCallOutcome,
    JevHttpRequest, JevHttpResponse, JevTransport, JevUsage,
};
use crate::tool_router::policy::{
    is_core_tool, question_id_for_tool, GateableTool, PROFILE_CONFIDENCE_MIN,
    PRUNE_BELOW_PROBABILITY, RETRY_AFTER_MS,
};
use crate::tool_router::settings::{ToolRouterMode, ToolRouterSettings};

pub(crate) struct DecisionInput<'a> {
    pub(crate) settings: &'a ToolRouterSettings,
    pub(crate) user_message: &'a str,
    pub(crate) agent_id: &'a str,
    pub(crate) agent_name: &'a str,
    /// Runtime tool names allowed for the agent this turn (output of
    /// `agents::runtime_tool_names_for_allowed_tool_ids`).
    pub(crate) whitelist: &'a [String],
    /// Harness profile the keyword-based `select_harness` picked
    /// ("code" | "chat" | "default"), when known.
    pub(crate) keyword_profile: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TurnDecision {
    /// "legacy" | "jev" | "fallback".
    pub(crate) source: &'static str,
    pub(crate) requested_model: String,
    pub(crate) resolved_model: Option<String>,
    /// Effective harness profile for the turn ("code" | "chat" | "default").
    pub(crate) profile: Option<String>,
    /// "jev" when the profile came from a confident Choice answer,
    /// "keyword" when it stayed with select_harness.
    pub(crate) profile_source: Option<&'static str>,
    /// Whitelist entries pruned this turn (runtime names, whitelist order).
    pub(crate) pruned_tools: Vec<String>,
    pub(crate) reason: String,
    pub(crate) latency_ms: u64,
    pub(crate) usage: Option<JevUsage>,
}

/// Compute the turn decision. The HTTP call runs on a dedicated std thread
/// (reqwest::blocking panics inside a tokio worker; `prepare_managed_runtime`
/// is called from both async and sync chains) and the caller waits at most
/// `settings.timeout_ms`.
pub(crate) fn decide(input: &DecisionInput) -> TurnDecision {
    decide_bounded(input, http_transport)
}

/// Transport-injecting variant of [`decide`] that keeps the overall-timeout
/// thread wrapper, so tests can exercise the timeout path without touching
/// the network.
pub(crate) fn decide_bounded<F>(input: &DecisionInput, transport: F) -> TurnDecision
where
    F: Fn(&JevHttpRequest) -> Result<JevHttpResponse, String> + Send + Sync + 'static,
{
    let gate = match prepare_call(input) {
        PreparedCall::Skipped(decision) => return decision,
        PreparedCall::Ready(call) => call,
    };
    let started = Instant::now();
    let budget_ms = input.settings.timeout_ms;
    let call_input = build_call_input(input, &gate.gateable, RETRY_AFTER_MS);
    let outcome = run_with_overall_timeout(budget_ms, move || {
        call_jev(&call_input, &transport)
    });
    let latency_ms = elapsed_ms(started);
    match outcome {
        Some(result) => interpret(input, &gate, result, latency_ms),
        None => fallback_decision(
            input,
            &gate,
            format!("Jev 调用超时（{budget_ms}ms），保留完整白名单"),
            budget_ms,
        ),
    }
}

/// Same as [`decide`] but with an injected transport and no worker thread —
/// the seam unit tests use to exercise the decision logic with canned
/// responses, and the guard proving legacy mode never sends HTTP.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn decide_with_transport(input: &DecisionInput, transport: &JevTransport) -> TurnDecision {
    let gate = match prepare_call(input) {
        PreparedCall::Skipped(decision) => return decision,
        PreparedCall::Ready(call) => call,
    };
    let started = Instant::now();
    let call_input = build_call_input(input, &gate.gateable, RETRY_AFTER_MS);
    let result = call_jev(&call_input, transport);
    interpret(input, &gate, result, elapsed_ms(started))
}

struct PreparedGate {
    gateable: Vec<&'static GateableTool>,
}

enum PreparedCall {
    Skipped(TurnDecision),
    Ready(PreparedGate),
}

/// Policy gates evaluated before any HTTP work: mode switch, API key, and
/// whether the whitelist even contains gateable tools. Any miss skips the
/// call entirely (source = "legacy").
fn prepare_call(input: &DecisionInput) -> PreparedCall {
    if input.settings.mode != ToolRouterMode::Jev {
        return PreparedCall::Skipped(legacy_decision(input, "工具路由处于 legacy 模式，未调用 Jev"));
    }
    if input.settings.api_key.trim().is_empty() {
        return PreparedCall::Skipped(legacy_decision(
            input,
            "jev 模式缺少 API Key，跳过 Jev 调用",
        ));
    }
    let gateable = gateable_input_catalog(input.whitelist);
    if gateable.is_empty() {
        return PreparedCall::Skipped(legacy_decision(
            input,
            "白名单中没有可被 Jev 决策的可选工具，跳过调用",
        ));
    }
    PreparedCall::Ready(PreparedGate { gateable })
}

fn build_call_input(
    input: &DecisionInput,
    gateable: &[&'static GateableTool],
    retry_after_ms: u64,
) -> JevCallInput {
    JevCallInput {
        api_key: input.settings.api_key.clone(),
        model: input.settings.model.clone(),
        user_message: input.user_message.to_string(),
        agent_id: input.agent_id.to_string(),
        agent_name: input.agent_name.to_string(),
        gateable: gateable.to_vec(),
        timeout_ms: input.settings.timeout_ms,
        retry_after_ms,
    }
}

fn interpret(
    input: &DecisionInput,
    gate: &PreparedGate,
    result: Result<JevCallOutcome, JevCallError>,
    latency_ms: u64,
) -> TurnDecision {
    match result {
        Ok(outcome) => {
            let pruned = prune_tools(input.whitelist, &gate.gateable, &outcome.response.answers);
            let (profile, profile_source) =
                resolve_profile(outcome.response.answers.get("harness_profile"), input.keyword_profile);
            let mut reason = if pruned.is_empty() {
                "Jev 决策生效：本轮保留全部白名单工具".to_string()
            } else {
                format!("Jev 决策生效：裁剪 {} 个工具", pruned.len())
            };
            let resolved_model = outcome.response.model.clone();
            if let Some(resolved) = resolved_model.as_deref() {
                if !resolved.is_empty() && resolved != input.settings.model.trim() {
                    log::warn!(
                        "Jev 版本漂移：请求 {}，实际 {}",
                        input.settings.model,
                        resolved
                    );
                    reason.push_str(&format!(
                        "；版本漂移：请求 {}，实际 {}",
                        input.settings.model, resolved
                    ));
                }
            }
            TurnDecision {
                source: "jev",
                requested_model: input.settings.model.clone(),
                resolved_model,
                profile,
                profile_source,
                pruned_tools: pruned,
                reason,
                latency_ms,
                usage: outcome.response.usage.clone(),
            }
        }
        Err(error) => fallback_decision(
            input,
            gate,
            format!("Jev 调用失败（{error}），保留完整白名单"),
            latency_ms,
        ),
    }
}

fn fallback_decision(
    input: &DecisionInput,
    _gate: &PreparedGate,
    reason: String,
    latency_ms: u64,
) -> TurnDecision {
    TurnDecision {
        source: "fallback",
        requested_model: input.settings.model.clone(),
        resolved_model: None,
        profile: keyword_profile_string(input.keyword_profile),
        profile_source: keyword_profile_source(input.keyword_profile),
        pruned_tools: Vec::new(),
        reason,
        latency_ms,
        usage: None,
    }
}

fn legacy_decision(input: &DecisionInput, reason: &str) -> TurnDecision {
    TurnDecision {
        source: "legacy",
        requested_model: input.settings.model.clone(),
        resolved_model: None,
        profile: keyword_profile_string(input.keyword_profile),
        profile_source: keyword_profile_source(input.keyword_profile),
        pruned_tools: Vec::new(),
        reason: reason.to_string(),
        latency_ms: 0,
        usage: None,
    }
}

fn keyword_profile_string(keyword_profile: Option<&str>) -> Option<String> {
    keyword_profile
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn keyword_profile_source(keyword_profile: Option<&str>) -> Option<&'static str> {
    keyword_profile
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|_| "keyword")
}

/// Whitelist entries to prune: gateable tools whose noul probability is
/// strictly below [`PRUNE_BELOW_PROBABILITY`]. Missing answers and
/// borderline values keep the tool; core tools are never prunable.
pub(crate) fn prune_tools(
    whitelist: &[String],
    gateable: &[&'static GateableTool],
    answers: &HashMap<String, crate::tool_router::jev_client::JevAnswer>,
) -> Vec<String> {
    let mut pruned = Vec::new();
    for name in whitelist {
        if is_core_tool(name) {
            continue;
        }
        if !gateable.iter().any(|tool| tool.runtime_name == name) {
            continue;
        }
        let Some(answer) = answers.get(&question_id_for_tool(name)) else {
            continue;
        };
        if let Some(noul) = answer.noul {
            if noul < PRUNE_BELOW_PROBABILITY {
                pruned.push(name.clone());
            }
        }
    }
    pruned
}

/// Trust the Choice answer only when its confidence reaches
/// [`PROFILE_CONFIDENCE_MIN`]; otherwise keep the keyword selection.
pub(crate) fn resolve_profile(
    answer: Option<&crate::tool_router::jev_client::JevAnswer>,
    keyword_profile: Option<&str>,
) -> (Option<String>, Option<&'static str>) {
    let keyword = (keyword_profile_string(keyword_profile), keyword_profile_source(keyword_profile));
    let Some(answer) = answer else {
        return keyword;
    };
    let confidence_ok = answer.confidence.unwrap_or(0.0) >= PROFILE_CONFIDENCE_MIN;
    let valid_choice = answer
        .choice
        .as_deref()
        .map(str::trim)
        .filter(|value| crate::tool_router::policy::HARNESS_PROFILES.contains(value));
    match (confidence_ok, valid_choice) {
        (true, Some(choice)) => (Some(choice.to_string()), Some("jev")),
        _ => keyword,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

/// Run `work` on a dedicated std thread and wait at most `timeout_ms`.
/// Returns `None` on timeout (the thread finishes and exits on its own;
/// per-request timeouts keep it short-lived).
pub(crate) fn run_with_overall_timeout<T: Send + 'static>(
    timeout_ms: u64,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver
        .recv_timeout(Duration::from_millis(timeout_ms.max(1)))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jev_settings() -> ToolRouterSettings {
        ToolRouterSettings {
            mode: ToolRouterMode::Jev,
            api_key: "sk-jev".to_string(),
            model: "jev-1.13.0".to_string(),
            timeout_ms: 2000,
        }
    }

    fn input_with<'a>(
        settings: &'a ToolRouterSettings,
        whitelist: &'a [String],
    ) -> DecisionInput<'a> {
        DecisionInput {
            settings,
            user_message: "帮我修个 bug 再画张图",
            agent_id: "agent_1",
            agent_name: "助手",
            whitelist,
            keyword_profile: Some("default"),
        }
    }

    fn wl(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    fn ok_transport(
        profile_choice: &str,
        confidence: f64,
        noul_by_tool: &[(&str, f64)],
    ) -> impl Fn(&JevHttpRequest) -> Result<JevHttpResponse, String> + 'static {
        let profile_choice = profile_choice.to_string();
        let noul_by_tool: Vec<(String, f64)> = noul_by_tool
            .iter()
            .map(|(tool, noul)| ((*tool).to_string(), *noul))
            .collect();
        move |_: &JevHttpRequest| {
            let mut answers = serde_json::Map::new();
            answers.insert(
                "harness_profile".to_string(),
                serde_json::json!({
                    "type": "choice",
                    "choice": profile_choice,
                    "probabilities": {"code": 0.8, "chat": 0.1, "default": 0.1},
                    "confidence": confidence
                }),
            );
            for (tool, noul) in &noul_by_tool {
                answers.insert(
                    question_id_for_tool(tool),
                    serde_json::json!({"type": "noul", "noul": noul}),
                );
            }
            Ok(JevHttpResponse {
                status: 200,
                body: serde_json::json!({
                    "model": "jev-1.13.0",
                    "answers": answers,
                    "usage": {"input_tokens": 10, "output_tokens": 4}
                })
                .to_string(),
            })
        }
    }

    #[test]
    fn legacy_mode_never_touches_the_transport() {
        let settings = ToolRouterSettings::default();
        let whitelist = wl(&["read", "web_search"]);
        let input = input_with(&settings, &whitelist);
        let transport = |_: &crate::tool_router::jev_client::JevHttpRequest| -> Result<crate::tool_router::jev_client::JevHttpResponse, String> {
            panic!("legacy mode must not construct or send any HTTP request");
        };
        let decision = decide_with_transport(&input, &transport);
        assert_eq!(decision.source, "legacy");
        assert!(decision.pruned_tools.is_empty());
        assert_eq!(decision.profile.as_deref(), Some("default"));
        assert_eq!(decision.profile_source, Some("keyword"));
        assert_eq!(decision.latency_ms, 0);
        assert!(decision.usage.is_none());
    }

    #[test]
    fn missing_api_key_skips_to_legacy() {
        let mut settings = jev_settings();
        settings.api_key = "  ".to_string();
        let whitelist = wl(&["read", "web_search"]);
        let input = input_with(&settings, &whitelist);
        let transport = |_: &crate::tool_router::jev_client::JevHttpRequest| -> Result<crate::tool_router::jev_client::JevHttpResponse, String> {
            panic!("missing api key must skip the HTTP call");
        };
        let decision = decide_with_transport(&input, &transport);
        assert_eq!(decision.source, "legacy");
        assert!(decision.reason.contains("API Key"));
    }

    #[test]
    fn empty_gateable_intersection_skips_to_legacy() {
        let settings = jev_settings();
        let whitelist = wl(&["read", "write", "bash"]);
        let input = input_with(&settings, &whitelist);
        let transport = |_: &crate::tool_router::jev_client::JevHttpRequest| -> Result<crate::tool_router::jev_client::JevHttpResponse, String> {
            panic!("nothing to decide must skip the HTTP call");
        };
        let decision = decide_with_transport(&input, &transport);
        assert_eq!(decision.source, "legacy");
        assert!(decision.reason.contains("可选工具"));
    }

    #[test]
    fn prunes_only_confident_no_and_keeps_borderline() {
        let settings = jev_settings();
        let whitelist = wl(&["read", "web_search", "image_generate", "mcp_tool"]);
        let input = input_with(&settings, &whitelist);
        // 0.29 → prune; 0.31 → keep; 0.30 → keep (not strictly below).
        let decision = decide_with_transport(
            &input,
            &ok_transport("default", 0.9, &[
                ("web_search", 0.29),
                ("image_generate", 0.31),
                ("mcp_tool", 0.30),
            ]),
        );
        assert_eq!(decision.source, "jev");
        assert_eq!(decision.pruned_tools, vec!["web_search".to_string()]);
    }

    #[test]
    fn core_tools_are_never_pruned_even_at_zero_probability() {
        let whitelist: Vec<String> = ["read", "write", "bash"]
            .iter()
            .map(|name| name.to_string())
            .collect();
        let answers = HashMap::from([(
            question_id_for_tool("read"),
            crate::tool_router::jev_client::JevAnswer {
                answer_type: Some("noul".to_string()),
                choice: None,
                probabilities: None,
                confidence: None,
                noul: Some(0.01),
            },
        )]);
        // read is core: not gateable, and prune_tools must skip it regardless.
        let pruned = prune_tools(&whitelist, &[], &answers);
        assert!(pruned.is_empty());
    }

    #[test]
    fn profile_confidence_gate_falls_back_to_keyword() {
        let settings = jev_settings();
        let whitelist = wl(&["read", "web_search"]);
        let input = input_with(&settings, &whitelist);

        let low = decide_with_transport(&input, &ok_transport("chat", 0.34, &[]));
        assert_eq!(low.profile.as_deref(), Some("default"));
        assert_eq!(low.profile_source, Some("keyword"));

        let high = decide_with_transport(&input, &ok_transport("chat", 0.35, &[]));
        assert_eq!(high.profile.as_deref(), Some("chat"));
        assert_eq!(high.profile_source, Some("jev"));
    }

    #[test]
    fn invalid_profile_choice_falls_back_to_keyword() {
        let settings = jev_settings();
        let whitelist = wl(&["read", "web_search"]);
        let input = input_with(&settings, &whitelist);
        let decision = decide_with_transport(&input, &ok_transport("vibes", 0.9, &[]));
        assert_eq!(decision.profile.as_deref(), Some("default"));
        assert_eq!(decision.profile_source, Some("keyword"));
    }

    #[test]
    fn version_drift_appends_reason_and_records_resolved_model() {
        let settings = jev_settings();
        let whitelist = wl(&["read", "web_search"]);
        let input = input_with(&settings, &whitelist);
        let transport = |_: &crate::tool_router::jev_client::JevHttpRequest| {
            Ok(crate::tool_router::jev_client::JevHttpResponse {
                status: 200,
                body: serde_json::json!({
                    "model": "jev-1.14.0",
                    "answers": {
                        "harness_profile": {"type": "choice", "choice": "code", "confidence": 0.9}
                    }
                })
                .to_string(),
            })
        };
        let decision = decide_with_transport(&input, &transport);
        assert_eq!(decision.source, "jev");
        assert_eq!(decision.resolved_model.as_deref(), Some("jev-1.14.0"));
        assert!(decision.reason.contains("版本漂移"));
        assert!(decision.reason.contains("jev-1.14.0"));
    }

    #[test]
    fn http_error_degrades_to_fallback_with_full_whitelist() {
        let settings = jev_settings();
        let whitelist = wl(&["read", "web_search"]);
        let input = input_with(&settings, &whitelist);
        let transport = |_: &crate::tool_router::jev_client::JevHttpRequest| {
            Ok(crate::tool_router::jev_client::JevHttpResponse {
                status: 401,
                body: "unauthorized".to_string(),
            })
        };
        let decision = decide_with_transport(&input, &transport);
        assert_eq!(decision.source, "fallback");
        assert!(decision.pruned_tools.is_empty());
        assert!(decision.reason.contains("401"));
        assert_eq!(decision.profile.as_deref(), Some("default"));
        assert_eq!(decision.profile_source, Some("keyword"));
    }

    #[test]
    fn usage_and_latency_flow_through_on_success() {
        let settings = jev_settings();
        let whitelist = wl(&["read", "web_search"]);
        let input = input_with(&settings, &whitelist);
        let decision = decide_with_transport(&input, &ok_transport("code", 0.8, &[]));
        assert_eq!(
            decision.usage,
            Some(JevUsage {
                input_tokens: 10,
                output_tokens: 4
            })
        );
        // latency is measured (>= 0); just assert the field exists.
        let _ = decision.latency_ms;
    }

    #[test]
    fn overall_timeout_returns_none_and_degrades() {
        // The transport sleeps past the budget; decide_bounded must time out
        // and degrade to fallback without any network access.
        let settings = ToolRouterSettings {
            mode: ToolRouterMode::Jev,
            api_key: "sk-jev".to_string(),
            model: "jev-1.13.0".to_string(),
            timeout_ms: 40,
        };
        let whitelist = wl(&["read", "web_search"]);
        let input = input_with(&settings, &whitelist);
        let decision = decide_bounded(&input, |_: &JevHttpRequest| {
            std::thread::sleep(Duration::from_millis(120));
            Ok(JevHttpResponse {
                status: 200,
                body: "{}".to_string(),
            })
        });
        assert_eq!(decision.source, "fallback");
        assert!(decision.reason.contains("超时"));
        assert!(decision.pruned_tools.is_empty());

        // The generic runner honors fast work too.
        let value = run_with_overall_timeout(1000, || 7);
        assert_eq!(value, Some(7));
        let slow = run_with_overall_timeout(10, || {
            std::thread::sleep(Duration::from_millis(120));
            9
        });
        assert_eq!(slow, None);
    }
}
