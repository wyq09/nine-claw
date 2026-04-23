use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const TASK_CREATOR_SKILL_ID: &str = "nineclaw-task-creator";
pub const TEAM_SUPERVISOR_ORCHESTRATOR_SKILL_ID: &str = "team-supervisor-orchestrator";
pub const DEFAULT_MAX_DYNAMIC_SKILLS: usize = 4;
const MIN_DYNAMIC_SKILLS: usize = 1;
const MAX_DYNAMIC_SKILLS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilityPolicy {
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub required_skill_ids: Vec<String>,
    #[serde(default)]
    pub forbidden_skill_ids: Vec<String>,
    #[serde(default = "default_max_dynamic_skills")]
    pub max_dynamic_skills: usize,
}

impl Default for AgentCapabilityPolicy {
    fn default() -> Self {
        static_capability_policy()
    }
}

fn default_strategy() -> String {
    "static".to_string()
}

fn default_max_dynamic_skills() -> usize {
    DEFAULT_MAX_DYNAMIC_SKILLS
}

pub fn static_capability_policy() -> AgentCapabilityPolicy {
    AgentCapabilityPolicy {
        strategy: "static".to_string(),
        required_skill_ids: Vec::new(),
        forbidden_skill_ids: Vec::new(),
        max_dynamic_skills: DEFAULT_MAX_DYNAMIC_SKILLS,
    }
}

pub fn new_agent_default_capability_policy() -> AgentCapabilityPolicy {
    AgentCapabilityPolicy {
        strategy: "hybrid".to_string(),
        required_skill_ids: Vec::new(),
        forbidden_skill_ids: Vec::new(),
        max_dynamic_skills: DEFAULT_MAX_DYNAMIC_SKILLS,
    }
}

pub fn normalize_capability_policy(
    policy: Option<AgentCapabilityPolicy>,
    default_policy: AgentCapabilityPolicy,
) -> AgentCapabilityPolicy {
    let mut normalized = policy.unwrap_or(default_policy);
    normalized.strategy = normalize_strategy(&normalized.strategy);
    normalized.required_skill_ids = dedupe_skill_ids(normalized.required_skill_ids);
    normalized.forbidden_skill_ids = dedupe_skill_ids(normalized.forbidden_skill_ids);
    normalized.max_dynamic_skills = normalized
        .max_dynamic_skills
        .clamp(MIN_DYNAMIC_SKILLS, MAX_DYNAMIC_SKILLS);

    let required = normalized
        .required_skill_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    normalized
        .forbidden_skill_ids
        .retain(|skill_id| !required.contains(skill_id));
    normalized
}

pub fn normalize_strategy(value: &str) -> String {
    match value.trim() {
        "hybrid" => "hybrid".to_string(),
        "dynamic" => "dynamic".to_string(),
        _ => "static".to_string(),
    }
}

pub fn dedupe_skill_ids(skill_ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for skill_id in skill_ids {
        let trimmed = skill_id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
}

pub fn required_system_skill_ids_for_mode(execution_mode: &str) -> Vec<String> {
    let mut skill_ids = vec![TASK_CREATOR_SKILL_ID.to_string()];
    if execution_mode.trim() == "supervisor" {
        skill_ids.push(TEAM_SUPERVISOR_ORCHESTRATOR_SKILL_ID.to_string());
    }
    skill_ids
}

pub fn effective_required_skill_ids(
    policy: &AgentCapabilityPolicy,
    execution_mode: &str,
) -> Vec<String> {
    let mut skill_ids = policy.required_skill_ids.clone();
    skill_ids.extend(required_system_skill_ids_for_mode(execution_mode));
    dedupe_skill_ids(skill_ids)
}

pub fn effective_forbidden_skill_ids(
    policy: &AgentCapabilityPolicy,
    execution_mode: &str,
) -> Vec<String> {
    let required = effective_required_skill_ids(policy, execution_mode)
        .into_iter()
        .collect::<HashSet<_>>();
    policy
        .forbidden_skill_ids
        .iter()
        .filter(|skill_id| !required.contains(*skill_id))
        .cloned()
        .collect()
}

pub fn mount_static_skill_ids(
    preferred_skill_ids: &[String],
    policy: &AgentCapabilityPolicy,
    execution_mode: &str,
) -> Vec<String> {
    let mut mounted = preferred_skill_ids.to_vec();
    mounted.extend(effective_required_skill_ids(policy, execution_mode));
    dedupe_skill_ids(mounted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_policy_dedupes_and_bounds_values() {
        let normalized = normalize_capability_policy(
            Some(AgentCapabilityPolicy {
                strategy: "dynamic".to_string(),
                required_skill_ids: vec!["alpha".into(), "alpha".into()],
                forbidden_skill_ids: vec!["alpha".into(), "beta".into()],
                max_dynamic_skills: 99,
            }),
            static_capability_policy(),
        );

        assert_eq!(normalized.required_skill_ids, vec!["alpha".to_string()]);
        assert_eq!(normalized.forbidden_skill_ids, vec!["beta".to_string()]);
        assert_eq!(normalized.max_dynamic_skills, MAX_DYNAMIC_SKILLS);
    }

    #[test]
    fn effective_required_adds_system_skills_by_mode() {
        let policy = AgentCapabilityPolicy {
            strategy: "hybrid".to_string(),
            required_skill_ids: vec!["alpha".into()],
            forbidden_skill_ids: vec![],
            max_dynamic_skills: DEFAULT_MAX_DYNAMIC_SKILLS,
        };

        assert_eq!(
            effective_required_skill_ids(&policy, "single"),
            vec!["alpha".to_string(), TASK_CREATOR_SKILL_ID.to_string()]
        );
        assert_eq!(
            effective_required_skill_ids(&policy, "supervisor"),
            vec![
                "alpha".to_string(),
                TASK_CREATOR_SKILL_ID.to_string(),
                TEAM_SUPERVISOR_ORCHESTRATOR_SKILL_ID.to_string(),
            ]
        );
    }
}
