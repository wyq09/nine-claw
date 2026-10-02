//! Per-turn AI tool selection ("Jev tool router").
//!
//! Before the PI agent subprocess starts, this layer asks TypeSafe's Jev
//! model over plain HTTPS which optional tools this turn actually needs and
//! which harness profile (code/chat/default) fits, then prunes the runtime
//! tool set accordingly. A settings switch selects legacy mode (default) vs
//! jev mode; every failure degrades silently to legacy behavior.
//!
//! Integration point: `managed_runtime::prepare_managed_runtime` calls
//! [`apply_tool_selection_to_harness`] after `restrict_harness_to_agent_tools`,
//! so both the desktop chain (lib.rs `stream_pi_prompt`) and the bot chain
//! (channels/pi_bridge.rs) get the pruned tool set and the
//! `pi://tool-selection` event.

pub(crate) mod decision;
pub(crate) mod jev_client;
pub(crate) mod policy;
pub(crate) mod settings;

use std::path::Path;

use serde::Serialize;

use crate::agents::ConversationAgentConfig;
use crate::managed_runtime::{HarnessDefinition, SelectedHarness};
use crate::tool_router::decision::{decide, DecisionInput, TurnDecision};
use crate::tool_router::settings::{load_tool_router_settings, ToolRouterSettings};

pub(crate) use settings::{load_tool_router_settings_command, save_tool_router_settings_command};

/// Event name uses the colon convention (`pi://stream` style); valid for
/// Tauri v2 event names.
pub(crate) const TOOL_SELECTION_EVENT_NAME: &str = "pi://tool-selection";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolSelectionUsage {
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolSelectionEvent {
    pub(crate) session_id: Option<String>,
    /// "legacy" | "jev" | "fallback".
    pub(crate) source: String,
    /// "legacy" | "jev" — the configured switch, independent of outcome.
    pub(crate) mode: String,
    pub(crate) requested_model: String,
    pub(crate) resolved_model: Option<String>,
    /// "code" | "chat" | "default" | null.
    pub(crate) profile: Option<String>,
    /// "jev" | "keyword" | null.
    pub(crate) profile_source: Option<String>,
    pub(crate) enabled_tools: Vec<String>,
    pub(crate) pruned_tools: Vec<String>,
    pub(crate) reason: String,
    pub(crate) latency_ms: u64,
    pub(crate) usage: Option<ToolSelectionUsage>,
}

pub(crate) fn emit_tool_selection_event(
    app: &tauri::AppHandle,
    payload: &ToolSelectionEvent,
) {
    crate::emit_safe::emit_safe(app, TOOL_SELECTION_EVENT_NAME, payload.clone());
}

/// One-line Chinese note appended to the turn's system prompt (via the
/// harness `promptAppend` field the runtime extension already appends)
/// whenever pruning actually removed tools.
pub(crate) fn build_pruned_tools_prompt_line(enabled_tools: &[String]) -> String {
    format!(
        "本轮实际启用工具：{}（白名单中其余工具本轮未启用）。",
        enabled_tools.join("、")
    )
}

/// Load settings for the turn, degrading to safe defaults on any error.
fn load_settings_or_default(app: Option<&tauri::AppHandle>) -> ToolRouterSettings {
    app.and_then(|app| {
        load_tool_router_settings(app)
            .map_err(|error| {
                log::warn!("读取 tool router 设置失败，按 legacy 处理: {error}");
                error
            })
            .ok()
    })
    .unwrap_or_default()
}

/// Keyword profile implied by the already-selected harness definition name.
fn keyword_profile_of(harness: &SelectedHarness) -> Option<&str> {
    let name = harness.definition.name.trim();
    if policy::HARNESS_PROFILES.contains(&name) {
        Some(name)
    } else {
        None
    }
}

fn profile_name_of(definition: &HarnessDefinition) -> Option<String> {
    let name = definition.name.trim();
    if policy::HARNESS_PROFILES.contains(&name) {
        Some(name.to_string())
    } else {
        None
    }
}

/// Swap in the harness definition for a Jev-selected profile, keeping the
/// already-restricted active tools. Returns the original definition when
/// the target file is missing/unreadable — never fails the turn.
fn swap_harness_definition(
    agent_home: &Path,
    harness: &SelectedHarness,
    profile: &str,
) -> HarnessDefinition {
    let target = crate::managed_runtime::harness_file_for_profile(agent_home, profile);
    match std::fs::read_to_string(&target)
        .map_err(|error| error.to_string())
        .and_then(|content| {
            serde_json::from_str::<HarnessDefinition>(&content)
                .map_err(|error| format!("解析 harness 文件失败 {error}"))
        }) {
        Ok(mut definition) => {
            definition.active_tools = harness.definition.active_tools.clone();
            definition
        }
        Err(error) => {
            log::warn!("切换 Jev profile harness 失败（保留 keyword 结果）: {error}");
            harness.definition.clone()
        }
    }
}

/// Apply the turn's tool-selection decision to the (already restricted)
/// harness: prune tools, swap the profile when Jev is confident, append the
/// Chinese enabled-tools note to the system-prompt append, and emit the
/// `pi://tool-selection` event. Never fails the turn — on any internal
/// error the incoming harness is returned unchanged.
pub(crate) fn apply_tool_selection_to_harness(
    agent_home: &Path,
    harness: SelectedHarness,
    prompt: Option<&str>,
    agent_config: &ConversationAgentConfig,
    session_id: &str,
) -> SelectedHarness {
    let app = crate::managed_runtime::injected_app_handle();
    let settings = load_settings_or_default(app.as_ref());
    let whitelist =
        crate::agents::runtime_tool_names_for_allowed_tool_ids(&agent_config.allowed_tool_ids);
    let keyword_profile = keyword_profile_of(&harness);
    let input = DecisionInput {
        settings: &settings,
        user_message: prompt.unwrap_or_default(),
        agent_id: &agent_config.id,
        agent_name: &agent_config.name,
        whitelist: &whitelist,
        keyword_profile,
    };
    let decision = decide(&input);

    let unchanged = harness.definition.clone();
    let mut definition = harness.definition.clone();
    if let Some(profile) = decision
        .profile
        .as_deref()
        .filter(|profile| keyword_profile != Some(*profile))
    {
        definition = swap_harness_definition(agent_home, &harness, profile);
    }
    if !decision.pruned_tools.is_empty() {
        definition
            .active_tools
            .retain(|tool| !decision.pruned_tools.contains(tool));
    }

    let effective_profile = profile_name_of(&definition);
    let enabled_tools = definition.active_tools.clone();
    let pruned_tools = decision.pruned_tools.clone();

    if !decision.pruned_tools.is_empty() {
        let note = build_pruned_tools_prompt_line(&enabled_tools);
        if definition.prompt_append.trim().is_empty() {
            definition.prompt_append = note;
        } else {
            definition.prompt_append = format!("{}\n{}", definition.prompt_append.trim_end(), note);
        }
    }

    let result = SelectedHarness {
        file_path: harness.file_path.clone(),
        definition,
    };

    // The effective harness file was already written by the restrict step;
    // rewrite it only when the decision actually changed the definition.
    if result.definition != unchanged {
        if let Ok(content) = serde_json::to_vec_pretty(&result.definition) {
            if let Err(error) = std::fs::write(&result.file_path, content) {
                log::warn!(
                    "写入 Jev 裁剪后的 harness 失败，保留未裁剪版本: {} ({error})",
                    result.file_path.display()
                );
                return harness;
            }
        }
    }

    if let Some(app) = app.as_ref() {
        let event = build_event(
            &decision,
            &settings,
            session_id,
            effective_profile.as_deref(),
            enabled_tools,
            pruned_tools,
        );
        emit_tool_selection_event(app, &event);
    }
    result
}

fn build_event(
    decision: &TurnDecision,
    settings: &ToolRouterSettings,
    session_id: &str,
    effective_profile: Option<&str>,
    enabled_tools: Vec<String>,
    pruned_tools: Vec<String>,
) -> ToolSelectionEvent {
    let profile = decision.profile.clone().or_else(|| effective_profile.map(str::to_string));
    ToolSelectionEvent {
        session_id: Some(session_id.to_string()),
        source: decision.source.to_string(),
        mode: settings.mode.as_str().to_string(),
        requested_model: decision.requested_model.clone(),
        resolved_model: decision.resolved_model.clone(),
        profile,
        profile_source: decision
            .profile_source
            .map(str::to_string)
            .or_else(|| effective_profile.map(|_| "keyword".to_string())),
        enabled_tools,
        pruned_tools,
        reason: decision.reason.clone(),
        latency_ms: decision.latency_ms,
        usage: decision.usage.as_ref().map(|usage| ToolSelectionUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_name_uses_colon_convention() {
        assert_eq!(TOOL_SELECTION_EVENT_NAME, "pi://tool-selection");
        assert!(TOOL_SELECTION_EVENT_NAME
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '/' | ':' | '_')));
    }

    #[test]
    fn event_payload_serializes_exact_camel_case_keys() {
        let event = ToolSelectionEvent {
            session_id: Some("s-1".to_string()),
            source: "jev".to_string(),
            mode: "jev".to_string(),
            requested_model: "jev-1.13.0".to_string(),
            resolved_model: Some("jev-1.13.0".to_string()),
            profile: Some("code".to_string()),
            profile_source: Some("jev".to_string()),
            enabled_tools: vec!["read".to_string(), "write".to_string()],
            pruned_tools: vec!["image_generate".to_string()],
            reason: "Jev 决策生效".to_string(),
            latency_ms: 123,
            usage: Some(ToolSelectionUsage {
                input_tokens: 100,
                output_tokens: 40,
            }),
        };
        let value = serde_json::to_value(&event).expect("serialize event");
        assert_eq!(
            value,
            serde_json::json!({
                "sessionId": "s-1",
                "source": "jev",
                "mode": "jev",
                "requestedModel": "jev-1.13.0",
                "resolvedModel": "jev-1.13.0",
                "profile": "code",
                "profileSource": "jev",
                "enabledTools": ["read", "write"],
                "prunedTools": ["image_generate"],
                "reason": "Jev 决策生效",
                "latencyMs": 123,
                "usage": {"inputTokens": 100, "outputTokens": 40}
            })
        );
    }

    #[test]
    fn event_payload_allows_null_optionals() {
        let event = ToolSelectionEvent {
            session_id: None,
            source: "legacy".to_string(),
            mode: "legacy".to_string(),
            requested_model: "jev-1.13.0".to_string(),
            resolved_model: None,
            profile: None,
            profile_source: None,
            enabled_tools: Vec::new(),
            pruned_tools: Vec::new(),
            reason: String::new(),
            latency_ms: 0,
            usage: None,
        };
        let value = serde_json::to_value(&event).expect("serialize event");
        assert!(value.get("sessionId").is_some_and(serde_json::Value::is_null));
        assert!(value.get("resolvedModel").is_some_and(serde_json::Value::is_null));
        assert!(value.get("usage").is_some_and(serde_json::Value::is_null));
    }

    #[test]
    fn pruned_tools_prompt_line_matches_required_wording() {
        let line = build_pruned_tools_prompt_line(&["read".to_string(), "bash".to_string()]);
        assert_eq!(line, "本轮实际启用工具：read、bash（白名单中其余工具本轮未启用）。");
    }

    fn agent_with_tools(tool_ids: &[&str]) -> ConversationAgentConfig {
        ConversationAgentConfig {
            id: "agent_1".to_string(),
            name: "助手".to_string(),
            summary: String::new(),
            description: String::new(),
            trigger_condition: String::new(),
            manual_trigger_only: false,
            system_prompt: String::new(),
            capability_policy: crate::agent_capabilities::static_capability_policy(),
            skill_ids: Vec::new(),
            allowed_tool_ids: tool_ids.iter().map(|id| id.to_string()).collect(),
            default_provider_id: String::new(),
            default_model: String::new(),
            execution_mode: "single".to_string(),
            collaboration_config: None,
            accent_color: None,
            avatar_uri: None,
            scenario_llm_config: None,
            agent_loop_config: None,
        }
    }

    fn harness_with_tools(name: &str, tools: &[&str]) -> SelectedHarness {
        SelectedHarness {
            file_path: std::env::temp_dir().join("nineclaw-tool-router-harness-test.json"),
            definition: HarnessDefinition {
                name: name.to_string(),
                description: String::new(),
                prompt_append: "既有提示".to_string(),
                active_tools: tools.iter().map(|t| t.to_string()).collect(),
                session_replay_limit: 12,
                enable_external_api_proxy: false,
                auto_retry_on_runtime_failure: true,
            },
        }
    }

    #[test]
    fn swap_harness_definition_loads_target_profile_and_keeps_active_tools() {
        let root = std::env::temp_dir().join(format!(
            "nineclaw-tr-swap-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).expect("create root");
        crate::managed_runtime::ensure_agent_runtime_scaffold(&root).expect("scaffold");

        let harness = harness_with_tools("default", &["read", "web_search"]);
        let swapped = swap_harness_definition(&root, &harness, "code");
        assert_eq!(swapped.name, "code");
        // Already-restricted active tools survive the swap.
        assert_eq!(swapped.active_tools, harness.definition.active_tools);
        // promptAppend comes from the swapped profile file.
        assert!(swapped.prompt_append.contains("code"));

        // Unknown profile falls back to the default harness file.
        let to_default = swap_harness_definition(&root, &harness, "vibes");
        assert_eq!(to_default.name, "default");

        // Missing harness file keeps the keyword definition untouched.
        let empty_root = std::env::temp_dir().join(format!(
            "nineclaw-tr-miss-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&empty_root).expect("create root");
        let kept = swap_harness_definition(&empty_root, &harness, "chat");
        assert_eq!(kept, harness.definition);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(empty_root);
    }

    #[test]
    fn legacy_settings_leave_harness_completely_untouched() {
        // No injected app handle in tests → default settings (legacy) →
        // the harness must come back byte-identical, with no rewrite.
        let root = std::env::temp_dir().join(format!("nineclaw-tr-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).expect("create root");
        let harness = harness_with_tools("default", &["read", "web_search"]);
        let result = apply_tool_selection_to_harness(
            &root,
            harness.clone(),
            Some("帮我查点资料"),
            &agent_with_tools(&["read_file", "web_search"]),
            "session-1",
        );
        assert_eq!(result.definition, harness.definition);
        assert_eq!(result.file_path, harness.file_path);
        // Legacy mode never even writes the effective file.
        assert!(!harness.file_path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn build_event_reports_keyword_profile_for_legacy() {
        let settings = ToolRouterSettings::default();
        let decision = TurnDecision {
            source: "legacy",
            requested_model: settings.model.clone(),
            resolved_model: None,
            profile: None,
            profile_source: None,
            pruned_tools: Vec::new(),
            reason: "工具路由处于 legacy 模式，未调用 Jev".to_string(),
            latency_ms: 0,
            usage: None,
        };
        let event = build_event(
            &decision,
            &settings,
            "s-2",
            Some("default"),
            vec!["read".to_string()],
            Vec::new(),
        );
        assert_eq!(event.source, "legacy");
        assert_eq!(event.mode, "legacy");
        assert_eq!(event.profile.as_deref(), Some("default"));
        assert_eq!(event.profile_source.as_deref(), Some("keyword"));
        assert!(event.enabled_tools.contains(&"read".to_string()));
    }
}
