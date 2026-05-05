use crate::agent_capabilities::{new_agent_default_capability_policy, AgentCapabilityPolicy};
use crate::agents::{
    default_allowed_tool_ids, AgentCollaborationConfig, AgentHeartbeatConfig,
    AgentScenarioLlmConfig,
};
use serde::Deserialize;
use std::collections::HashMap;

const JIUJIE_XIA_PRESET_SOURCE: &str =
    include_str!("../resources/default-agent-presets/jiujiexia.json");
const COLOR_MASTER_PRESET_SOURCE: &str =
    include_str!("../resources/default-agent-presets/color-master.json");
const PSYCH_MARKETER_PRESET_SOURCE: &str =
    include_str!("../resources/default-agent-presets/psych-marketer.json");
const XIAO8_PRESET_SOURCE: &str =
    include_str!("../resources/default-agent-presets/xiao8-frontend.json");
const BAIDU_MARKETING_PRESET_SOURCE: &str =
    include_str!("../resources/default-agent-presets/baidu-marketing-expert.json");
const AGENT_BUILDER_PRESET_SOURCE: &str =
    include_str!("../resources/default-agent-presets/agent-builder-default.json");

pub const DEFAULT_PRIMARY_AGENT_ID: &str = "jiujiexia";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPreset {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    #[serde(default)]
    pub system_prompt: String,
    pub default_provider_id: String,
    pub default_model: String,
    #[serde(default = "default_execution_mode")]
    pub execution_mode: String,
    #[serde(default)]
    pub accent_color: Option<String>,
    #[serde(default)]
    pub avatar_uri: Option<String>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default = "default_allowed_tool_ids")]
    pub allowed_tool_ids: Vec<String>,
    #[serde(default = "new_agent_default_capability_policy")]
    pub capability_policy: AgentCapabilityPolicy,
    #[serde(default)]
    pub collaboration_config: Option<AgentCollaborationConfig>,
    #[serde(default)]
    pub heartbeat_config: AgentHeartbeatConfig,
    #[serde(default)]
    pub scenario_llm_config: Option<AgentScenarioLlmConfig>,
    #[serde(default)]
    pub workspace_files: HashMap<String, String>,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPresetSummary {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    pub accent_color: Option<String>,
    pub skill_ids: Vec<String>,
    pub allowed_tool_ids: Vec<String>,
    pub is_default: bool,
}

fn default_execution_mode() -> String {
    "single".to_string()
}

fn parse_preset(source: &str, label: &str) -> AgentPreset {
    serde_json::from_str::<AgentPreset>(source)
        .unwrap_or_else(|error| panic!("解析默认智能体预设 {label} 失败: {error}"))
}

pub fn load_default_agent_presets() -> Vec<AgentPreset> {
    vec![
        parse_preset(JIUJIE_XIA_PRESET_SOURCE, "jiujiexia"),
        parse_preset(COLOR_MASTER_PRESET_SOURCE, "color-master"),
        parse_preset(PSYCH_MARKETER_PRESET_SOURCE, "psych-marketer"),
        parse_preset(XIAO8_PRESET_SOURCE, "xiao8-frontend"),
        parse_preset(BAIDU_MARKETING_PRESET_SOURCE, "baidu-marketing-expert"),
        parse_preset(AGENT_BUILDER_PRESET_SOURCE, "agent-builder-default"),
    ]
}

pub fn list_default_agent_preset_summaries() -> Vec<AgentPresetSummary> {
    load_default_agent_presets()
        .into_iter()
        .map(|preset| AgentPresetSummary {
            id: preset.id,
            name: preset.name,
            summary: preset.summary,
            description: preset.description,
            accent_color: preset.accent_color,
            skill_ids: preset.skill_ids,
            allowed_tool_ids: preset.allowed_tool_ids,
            is_default: preset.is_default,
        })
        .collect()
}

pub fn find_default_agent_preset(agent_id: &str) -> Option<AgentPreset> {
    load_default_agent_presets()
        .into_iter()
        .find(|preset| preset.id == agent_id)
}

#[cfg(test)]
mod tests {
    use super::{find_default_agent_preset, load_default_agent_presets, DEFAULT_PRIMARY_AGENT_ID};
    use std::collections::HashSet;

    #[test]
    fn default_agent_presets_have_unique_ids_and_single_default() {
        let presets = load_default_agent_presets();
        assert!(
            presets.len() >= 4,
            "expected a main agent plus several children"
        );

        let ids: HashSet<_> = presets.iter().map(|preset| preset.id.as_str()).collect();
        assert_eq!(ids.len(), presets.len(), "preset ids must be unique");

        let defaults: Vec<_> = presets.iter().filter(|preset| preset.is_default).collect();
        assert_eq!(
            defaults.len(),
            1,
            "exactly one preset should be marked default"
        );
        assert_eq!(defaults[0].id, DEFAULT_PRIMARY_AGENT_ID);
    }

    #[test]
    fn can_find_primary_preset() {
        let preset = find_default_agent_preset(DEFAULT_PRIMARY_AGENT_ID).expect("primary preset");
        assert_eq!(preset.name, "九节虾");
        assert!(!preset.workspace_files.is_empty());
    }
}
