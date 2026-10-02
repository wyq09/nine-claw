//! Jev 工具路由策略常量与可决策工具目录。
//!
//! Every knob that decides how a turn's tool set is gated lives here so it
//! can be reviewed and boundary-tested in one place: decision thresholds,
//! the pinned Jev model, the overall budget, the core (never-pruned) tool
//! set, and the gateable tool catalog with the English question wording the
//! Jev API expects.

/// TypeSafe Jev systemone endpoint. Plain HTTPS POST with a bearer token —
/// deliberately no SDK, per user requirement.
pub(crate) const JEV_API_URL: &str = "https://api.typesafe.ai/v1/systemone";

/// Pinned Jev model version. Never default to `jev-latest`: version drift
/// (API echoing a different model than requested) is surfaced in the
/// `pi://tool-selection` event and logged.
pub(crate) const DEFAULT_JEV_MODEL: &str = "jev-1.13.0";

/// Prune a gateable tool only when Jev is confidently sure it is NOT needed
/// (noul probability strictly below this). Borderline answers keep the
/// tool — 宁多勿缺.
pub(crate) const PRUNE_BELOW_PROBABILITY: f64 = 0.30;

/// Trust the harness-profile Choice answer only at/above this confidence;
/// below it the keyword-based harness selection stays (profileSource =
/// "keyword").
pub(crate) const PROFILE_CONFIDENCE_MIN: f64 = 0.35;

/// Hard cap for `state.userMessage` (chars) so the request body stays
/// bounded even for very long prompts.
pub(crate) const USER_MESSAGE_CHAR_CAP: usize = 12000;

/// Wait before the single 429/529 retry attempt.
pub(crate) const RETRY_AFTER_MS: u64 = 400;

/// Default overall budget for the whole Jev round trip (caller side).
pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 2000;

/// Upper clamp for a user-configured timeout; keeps a fat-fingered value
/// from stalling every turn.
pub(crate) const MAX_TIMEOUT_MS: u64 = 30000;

/// Valid harness profiles Jev may choose between.
pub(crate) const HARNESS_PROFILES: &[&str] = &["code", "chat", "default"];

/// Runtime tool names that are never pruned regardless of Jev answers:
/// core file/command tools, PI builtins, and ask_user. Everything else in
/// the agent whitelist is a gating candidate.
pub(crate) const CORE_TOOLS: &[&str] = &[
    "read", "write", "edit", "bash", "grep", "glob", "ls", "find", "ask_user",
];

/// A tool that Jev may prune for a turn, with the one-line description
/// embedded in its noul question.
pub(crate) struct GateableTool {
    /// PI runtime tool name. Authoritative sources: `agents.rs`
    /// `runtime_tool_names_for_allowed_tool_ids` (allowed_tool_ids → runtime
    /// names) and the `name:` registrations in `src/runtime-tools/*.mjs`.
    pub(crate) runtime_name: &'static str,
    /// One-line English description used inside the noul question.
    pub(crate) description: &'static str,
}

/// Semantic gating candidates. Names verified against the runtime tool .mjs
/// registrations (`image_generate`, `image_analyze`, `agent_delegate`,
/// `nineclaw_external_api`, `mcp_tool`, memory_*, task_*). Tools absent
/// from a turn's whitelist are simply never asked about. `skill-creator` is
/// registered by the runtime extension but is not whitelist-controllable
/// (absent from `DEFAULT_ALLOWED_TOOL_IDS`), so it can never appear in a
/// turn's tool set and is not gated.
pub(crate) const GATEABLE_TOOLS: &[GateableTool] = &[
    GateableTool {
        runtime_name: "web_search",
        description: "Search the web for up-to-date public information",
    },
    GateableTool {
        runtime_name: "web_fetch",
        description: "Fetch and read a specific web page by URL",
    },
    GateableTool {
        runtime_name: "image_generate",
        description: "Generate images from a text description",
    },
    GateableTool {
        runtime_name: "image_task_query",
        description: "Query the status or result of an async image generation task",
    },
    GateableTool {
        runtime_name: "image_analyze",
        description: "Analyze image or video attachments and return descriptions",
    },
    GateableTool {
        runtime_name: "agent_delegate",
        description: "Delegate a subtask to another agent and collect its result",
    },
    GateableTool {
        runtime_name: "nineclaw_external_api",
        description: "Call configured external HTTP APIs through the credential proxy",
    },
    GateableTool {
        runtime_name: "memory_update",
        description: "Update an existing persistent memory entry",
    },
    GateableTool {
        runtime_name: "memory_search",
        description: "Search the agent's persistent memories",
    },
    GateableTool {
        runtime_name: "memory_read",
        description: "Read a specific persistent memory entry",
    },
    GateableTool {
        runtime_name: "memory_delete",
        description: "Delete a persistent memory entry",
    },
    GateableTool {
        runtime_name: "memory_store",
        description: "Store a new persistent memory entry",
    },
    GateableTool {
        runtime_name: "memory_save",
        description: "Save or upsert a persistent memory entry",
    },
    GateableTool {
        runtime_name: "memory_get",
        description: "Get a persistent memory entry by key",
    },
    GateableTool {
        runtime_name: "memory_forget",
        description: "Forget persistent memory entries",
    },
    GateableTool {
        runtime_name: "memory_list",
        description: "List persistent memory entries",
    },
    GateableTool {
        runtime_name: "chat_search",
        description: "Search historical chat conversations",
    },
    GateableTool {
        runtime_name: "create_scheduled_task",
        description: "Create a scheduled or recurring task",
    },
    GateableTool {
        runtime_name: "query_scheduled_task",
        description: "Query scheduled task runs and status",
    },
    GateableTool {
        runtime_name: "query_scheduled_task_info",
        description: "Query detailed info of one scheduled task",
    },
    GateableTool {
        runtime_name: "mcp_tool",
        description: "Call tools provided by configured MCP servers",
    },
    GateableTool {
        runtime_name: "mcp_config",
        description: "View and manage MCP server configuration",
    },
];

/// Stable question id for a runtime tool name (dash becomes underscore so
/// ids stay identifier-safe, e.g. `skill-creator` → `tool_skill_creator`).
pub(crate) fn question_id_for_tool(runtime_name: &str) -> String {
    let mut id = String::with_capacity(runtime_name.len() + 6);
    id.push_str("tool_");
    for ch in runtime_name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            id.push(ch);
        } else {
            id.push('_');
        }
    }
    id
}

pub(crate) fn is_core_tool(runtime_name: &str) -> bool {
    CORE_TOOLS.contains(&runtime_name)
}

/// Catalog entries that actually appear in this turn's whitelist — only
/// those are worth asking Jev about.
pub(crate) fn gateable_catalog_for(whitelist: &[String]) -> Vec<&'static GateableTool> {
    GATEABLE_TOOLS
        .iter()
        .filter(|tool| whitelist.iter().any(|name| name == tool.runtime_name))
        .collect()
}

/// The noul question text for one tool. English wording: Jev is trained
/// primarily on English (the user message itself stays in its original
/// language inside `state`).
pub(crate) fn noul_instructions(tool: &GateableTool) -> String {
    format!(
        "Might this turn's task require calling the tool `{}` ({})? Answer yes if the user's request or the assistant's plausible next steps need it.",
        tool.runtime_name, tool.description
    )
}

pub(crate) const HARNESS_PROFILE_QUESTION_ID: &str = "harness_profile";
pub(crate) const HARNESS_PROFILE_INSTRUCTIONS: &str =
    "Which working profile fits this turn's task best?";
pub(crate) const HARNESS_PROFILE_CRITERIA_CODE: &str = "Writing, modifying, debugging, or reviewing code; builds, tests, refactors, running commands, project files";
pub(crate) const HARNESS_PROFILE_CRITERIA_CHAT: &str = "Conversation, summarization, planning, Q&A, analysis, reviews that do not require editing files or running commands";
pub(crate) const HARNESS_PROFILE_CRITERIA_DEFAULT: &str =
    "Mixed or general tasks that are not clearly code-focused or pure conversation";

/// Truncate the user message for `state.userMessage`. Char-based (not byte)
/// so a Chinese prompt is not split mid-character.
pub(crate) fn truncate_user_message(message: &str) -> String {
    if message.chars().count() <= USER_MESSAGE_CHAR_CAP {
        return message.to_string();
    }
    let mut out: String = message.chars().take(USER_MESSAGE_CHAR_CAP).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_threshold_bands_are_documented_values() {
        // Boundary contract: 0.29 is pruned (strictly below 0.30), 0.30 and
        // 0.31 are kept (宁多勿缺 — borderline answers keep the tool).
        let below: f64 = "0.29".parse().expect("parse");
        let at_threshold: f64 = "0.30".parse().expect("parse");
        let above: f64 = "0.31".parse().expect("parse");
        assert!(below < PRUNE_BELOW_PROBABILITY);
        assert!(at_threshold >= PRUNE_BELOW_PROBABILITY);
        assert!(above >= PRUNE_BELOW_PROBABILITY);
        assert!((PRUNE_BELOW_PROBABILITY - 0.30).abs() < f64::EPSILON);
    }

    #[test]
    fn profile_confidence_min_is_documented_value() {
        assert_eq!(PROFILE_CONFIDENCE_MIN, 0.35);
    }

    #[test]
    fn pinned_model_is_never_latest() {
        assert_eq!(DEFAULT_JEV_MODEL, "jev-1.13.0");
        assert!(!DEFAULT_JEV_MODEL.contains("latest"));
    }

    #[test]
    fn core_tools_are_never_gateable() {
        for core in CORE_TOOLS {
            assert!(
                !GATEABLE_TOOLS
                    .iter()
                    .any(|tool| tool.runtime_name == *core),
                "{core} must not appear in the gateable catalog"
            );
            assert!(is_core_tool(core));
        }
        assert!(!is_core_tool("web_search"));
    }

    #[test]
    fn gateable_catalog_intersects_whitelist() {
        let whitelist = ["read".to_string(), "web_search".to_string(), "bash".to_string()];
        let catalog = gateable_catalog_for(&whitelist);
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].runtime_name, "web_search");

        let core_only = ["read".to_string()];
        assert!(gateable_catalog_for(&core_only).is_empty());
    }

    #[test]
    fn question_ids_are_stable_and_identifier_safe() {
        assert_eq!(question_id_for_tool("web_search"), "tool_web_search");
        assert_eq!(
            question_id_for_tool("nineclaw_external_api"),
            "tool_nineclaw_external_api"
        );
        assert_eq!(question_id_for_tool("skill-creator"), "tool_skill_creator");
    }

    #[test]
    fn gateable_names_match_runtime_tool_sources() {
        // Names must match the whitelisted runtime names produced by
        // agents::runtime_tool_names_for_allowed_tool_ids.
        let whitelist = crate::agents::runtime_tool_names_for_allowed_tool_ids(
            &crate::agents::default_allowed_tool_ids(),
        );
        for tool in GATEABLE_TOOLS {
            assert!(
                whitelist.iter().any(|name| name == tool.runtime_name),
                "gateable tool {} missing from default whitelist mapping",
                tool.runtime_name
            );
        }
    }

    #[test]
    fn truncates_long_user_messages_at_char_boundary() {
        let short = "修复这个 bug";
        assert_eq!(truncate_user_message(short), short);

        let long: String = "好".repeat(USER_MESSAGE_CHAR_CAP + 500);
        let truncated = truncate_user_message(&long);
        assert_eq!(truncated.chars().count(), USER_MESSAGE_CHAR_CAP + 1);
        assert!(truncated.ends_with('…'));
    }
}
