use crate::agent_capabilities::{
    dedupe_skill_ids, normalize_capability_policy, AgentCapabilityPolicy,
    TEAM_SUPERVISOR_ORCHESTRATOR_SKILL_ID,
};
use crate::agents::{AgentCollaborationConfig, AgentInput, AgentRecord};

pub(crate) fn build_supervisor_agent_input(
    record: &AgentRecord,
    delegate_ids: Vec<String>,
) -> AgentInput {
    let mut collab = record
        .collaboration_config
        .clone()
        .unwrap_or(AgentCollaborationConfig {
            allowed_delegate_agent_ids: vec![],
            handoff_prompt: String::new(),
            shared_context_policy: "session".to_string(),
        });
    collab.allowed_delegate_agent_ids = dedupe_string_ids(delegate_ids);

    AgentInput {
        id: Some(record.id.clone()),
        name: record.name.clone(),
        summary: record.summary.clone(),
        description: record.description.clone(),
        trigger_condition: record.trigger_condition.clone(),
        manual_trigger_only: record.manual_trigger_only,
        system_prompt: record.system_prompt.clone(),
        capability_policy: Some(ensure_required_capability_skill(
            record.capability_policy.clone(),
            TEAM_SUPERVISOR_ORCHESTRATOR_SKILL_ID,
        )),
        skill_ids: dedupe_string_ids(record.skill_ids.clone()),
        default_provider_id: record.default_provider_id.clone(),
        default_model: record.default_model.clone(),
        execution_mode: Some(record.execution_mode.clone()),
        collaboration_config: Some(collab),
        accent_color: record.accent_color.clone(),
        scenario_llm_config: record.scenario_llm_config.clone(),
        agent_loop_config: record.agent_loop_config.clone(),
        bot_configs: record.bot_configs.clone(),
        heartbeat_config: record.heartbeat_config.clone(),
    }
}

fn ensure_required_capability_skill(
    mut capability_policy: AgentCapabilityPolicy,
    required_skill_id: &str,
) -> AgentCapabilityPolicy {
    capability_policy
        .required_skill_ids
        .push(required_skill_id.to_string());
    normalize_capability_policy(Some(capability_policy), AgentCapabilityPolicy::default())
}

fn dedupe_string_ids(values: Vec<String>) -> Vec<String> {
    dedupe_skill_ids(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{AgentHeartbeatConfig, AgentScenarioLlmConfig, AgentScenarioLlmSlot};
    use std::collections::HashMap;

    fn sample_record() -> AgentRecord {
        AgentRecord {
            id: "main-agent".to_string(),
            name: "Main Agent".to_string(),
            summary: "负责团队协调".to_string(),
            description: "负责拆解复杂任务并协调团队成员。".to_string(),
            trigger_condition: String::new(),
            manual_trigger_only: false,
            system_prompt: "先拆解再协调".to_string(),
            capability_policy: AgentCapabilityPolicy::default(),
            skill_ids: vec!["nineclaw-task-creator".to_string()],
            default_provider_id: "openai".to_string(),
            default_model: "gpt-5.4".to_string(),
            is_builtin: false,
            is_archived: false,
            execution_mode: "supervisor".to_string(),
            collaboration_config: None,
            accent_color: Some("#123456".to_string()),
            bot_configs: HashMap::new(),
            heartbeat_config: AgentHeartbeatConfig::default(),
            scenario_llm_config: Some(AgentScenarioLlmConfig {
                title_generation: Some(AgentScenarioLlmSlot {
                    provider_id: "openai".to_string(),
                    model: "gpt-5.4-mini".to_string(),
                }),
                memory_extraction: None,
                task_push_notification_copy: None,
            }),
            agent_loop_config: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn build_supervisor_agent_input_auto_mounts_team_skill() {
        let input = build_supervisor_agent_input(
            &sample_record(),
            vec!["worker-a".to_string(), "worker-b".to_string()],
        );

        assert_eq!(
            input.capability_policy.expect("policy").required_skill_ids,
            vec!["team-supervisor-orchestrator".to_string()]
        );
        assert_eq!(
            input
                .collaboration_config
                .as_ref()
                .map(|item| item.allowed_delegate_agent_ids.clone()),
            Some(vec!["worker-a".to_string(), "worker-b".to_string()])
        );
    }

    #[test]
    fn build_supervisor_agent_input_preserves_collaboration_and_dedupes() {
        let mut record = sample_record();
        record.skill_ids = vec!["alpha".to_string(), "alpha".to_string()];
        record.capability_policy = AgentCapabilityPolicy {
            strategy: "static".to_string(),
            required_skill_ids: vec!["team-supervisor-orchestrator".to_string()],
            forbidden_skill_ids: vec![],
            max_dynamic_skills: 4,
        };
        record.collaboration_config = Some(AgentCollaborationConfig {
            allowed_delegate_agent_ids: vec!["old-worker".to_string()],
            handoff_prompt: "优先派给最合适的人".to_string(),
            shared_context_policy: "summary".to_string(),
        });

        let input = build_supervisor_agent_input(
            &record,
            vec![
                " worker-a ".to_string(),
                "worker-a".to_string(),
                "".to_string(),
                "worker-b".to_string(),
            ],
        );

        assert_eq!(input.skill_ids, vec!["alpha".to_string()]);
        assert_eq!(
            input
                .capability_policy
                .as_ref()
                .map(|policy| policy.required_skill_ids.clone()),
            Some(vec!["team-supervisor-orchestrator".to_string()])
        );
        let collab = input.collaboration_config.expect("collaboration config");
        assert_eq!(
            collab.allowed_delegate_agent_ids,
            vec!["worker-a".to_string(), "worker-b".to_string()]
        );
        assert_eq!(collab.handoff_prompt, "优先派给最合适的人");
        assert_eq!(collab.shared_context_policy, "summary");
    }
}
