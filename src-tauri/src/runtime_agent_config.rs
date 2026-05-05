use crate::agents::{self, ConversationAgentConfig};
use tauri::AppHandle;

pub fn refresh_runtime_agent_config(
    app: &AppHandle,
    snapshot: Option<ConversationAgentConfig>,
) -> Result<Option<ConversationAgentConfig>, String> {
    refresh_runtime_agent_config_with_optional_app(Some(app), snapshot)
}

pub fn refresh_runtime_agent_config_with_optional_app(
    app: Option<&AppHandle>,
    snapshot: Option<ConversationAgentConfig>,
) -> Result<Option<ConversationAgentConfig>, String> {
    let Some(snapshot) = snapshot else {
        return Ok(None);
    };

    let agent_id = snapshot.id.trim();
    if agent_id.is_empty() {
        return Ok(Some(snapshot));
    }

    let Some(app) = app else {
        return Ok(Some(snapshot));
    };
    let live = agents::get_conversation_agent_config(app, agent_id)?;
    Ok(Some(prefer_live_agent_config(snapshot, live)))
}

fn prefer_live_agent_config(
    snapshot: ConversationAgentConfig,
    live: Option<ConversationAgentConfig>,
) -> ConversationAgentConfig {
    live.unwrap_or(snapshot)
}

#[cfg(test)]
mod tests {
    use super::{prefer_live_agent_config, refresh_runtime_agent_config_with_optional_app};
    use crate::agent_capabilities::AgentCapabilityPolicy;
    use crate::agents::{AgentCollaborationConfig, ConversationAgentConfig};

    fn config(id: &str, name: &str, prompt: &str, delegates: &[&str]) -> ConversationAgentConfig {
        ConversationAgentConfig {
            id: id.to_string(),
            name: name.to_string(),
            summary: format!("{name} summary"),
            description: format!("{name} description"),
            trigger_condition: String::new(),
            manual_trigger_only: false,
            system_prompt: prompt.to_string(),
            capability_policy: AgentCapabilityPolicy::default(),
            skill_ids: vec![],
            allowed_tool_ids: vec!["read".to_string()],
            default_provider_id: "openai".to_string(),
            default_model: "gpt-5".to_string(),
            execution_mode: "single".to_string(),
            collaboration_config: Some(AgentCollaborationConfig {
                allowed_delegate_agent_ids: delegates.iter().map(|item| item.to_string()).collect(),
                handoff_prompt: "优先找最合适的人".to_string(),
                shared_context_policy: "summary".to_string(),
            }),
            accent_color: None,
            avatar_uri: None,
            scenario_llm_config: None,
            agent_loop_config: None,
        }
    }

    #[test]
    fn refresh_prefers_live_config_when_available() {
        let snapshot = config("sup", "旧主智能体", "旧提示词", &["aso", "x8"]);
        let live = config(
            "sup",
            "新主智能体",
            "新提示词，包含 PsychMarketer",
            &["aso", "x8", "psych"],
        );

        let resolved = prefer_live_agent_config(snapshot, Some(live));

        assert_eq!(resolved.name, "新主智能体");
        assert!(resolved.system_prompt.contains("PsychMarketer"));
        assert_eq!(
            resolved
                .collaboration_config
                .expect("collab")
                .allowed_delegate_agent_ids,
            vec!["aso".to_string(), "x8".to_string(), "psych".to_string()]
        );
    }

    #[test]
    fn refresh_keeps_snapshot_when_live_config_missing() {
        let snapshot = config("sup", "旧主智能体", "旧提示词", &["aso", "x8"]);

        let resolved = prefer_live_agent_config(snapshot.clone(), None);

        assert_eq!(resolved.name, snapshot.name);
        assert_eq!(resolved.system_prompt, snapshot.system_prompt);
        assert_eq!(
            resolved
                .collaboration_config
                .expect("collab")
                .allowed_delegate_agent_ids,
            vec!["aso".to_string(), "x8".to_string()]
        );
    }

    #[test]
    fn refresh_without_app_handle_keeps_snapshot() {
        let snapshot = config("sup", "旧主智能体", "旧提示词", &["aso", "x8"]);

        let resolved = refresh_runtime_agent_config_with_optional_app(None, Some(snapshot.clone()))
            .expect("refresh without app handle");

        assert_eq!(resolved, Some(snapshot));
    }
}
