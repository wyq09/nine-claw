use crate::agent_capabilities::{
    effective_forbidden_skill_ids, effective_required_skill_ids, mount_static_skill_ids,
};
use crate::agents::ConversationAgentConfig;
use crate::skills::{self, SkillDefinition};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::HashSet;

const STATIC_EAGER_SKILL_LIMIT: usize = 3;
const STATIC_LAZY_MOUNT_LIMIT: usize = 2;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillBrokerDecision {
    pub strategy: String,
    pub mounted_skill_ids: Vec<String>,
    pub skipped_skill_ids: Vec<String>,
    pub reasons: Vec<String>,
}

pub fn select_skills_for_turn(
    agent: &ConversationAgentConfig,
    prompt: &str,
    recent_context: &[String],
) -> Result<SkillBrokerDecision, String> {
    let policy = &agent.capability_policy;
    let required = effective_required_skill_ids(policy, &agent.execution_mode);
    if policy.strategy == "static" {
        let mounted = select_static_skill_ids(agent, &required, prompt, recent_context)?;
        return Ok(SkillBrokerDecision {
            strategy: "static".to_string(),
            skipped_skill_ids: Vec::new(),
            reasons: build_static_strategy_reasons(agent, &mounted, &required),
            mounted_skill_ids: mounted,
        });
    }

    let available = skills::list_runtime_available_skills()?;
    let forbidden = effective_forbidden_skill_ids(policy, &agent.execution_mode)
        .into_iter()
        .collect::<HashSet<_>>();
    let preferred = agent.skill_ids.iter().cloned().collect::<HashSet<_>>();
    let query = build_query(prompt, recent_context);
    let is_analysis_like = looks_like_analysis_request(&query);
    let max_dynamic = policy.max_dynamic_skills.max(1);

    let mut scored = available
        .into_iter()
        .filter(|skill| !forbidden.contains(&skill.id))
        .filter_map(|skill| {
            let (score, reasons) = score_skill(
                &skill,
                &query,
                &agent.execution_mode,
                preferred.contains(&skill.id),
                is_analysis_like,
            );
            if score < 20 {
                return None;
            }
            Some((Reverse(score), skill.id.clone(), skill, reasons))
        })
        .collect::<Vec<_>>();

    scored.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    let mut mounted = required.clone();
    let mut reasons = vec![format!(
        "{}: 命中 prompt/context 后从 {} 个可用技能中筛选",
        policy.strategy,
        scored.len()
    )];
    let mut chosen = HashSet::new();
    for (_, _, skill, skill_reasons) in scored.into_iter().take(max_dynamic) {
        if chosen.insert(skill.id.clone()) {
            reasons.push(format!("{}: {}", skill.id, skill_reasons.join("；")));
            mounted.push(skill.id);
        }
    }
    let mounted = crate::agent_capabilities::dedupe_skill_ids(mounted);
    let mounted_set = mounted.iter().cloned().collect::<HashSet<_>>();
    let skipped_skill_ids = forbidden
        .into_iter()
        .chain(
            agent
                .skill_ids
                .iter()
                .filter(|id| !mounted_set.contains(*id))
                .cloned(),
        )
        .collect::<Vec<_>>();

    Ok(SkillBrokerDecision {
        strategy: policy.strategy.clone(),
        mounted_skill_ids: mounted,
        skipped_skill_ids: crate::agent_capabilities::dedupe_skill_ids(skipped_skill_ids),
        reasons,
    })
}

pub fn runtime_skill_prompt(decision: &SkillBrokerDecision) -> Option<String> {
    if decision.mounted_skill_ids.is_empty() {
        return None;
    }
    Some(format!(
        "本轮已装配技能：{}。",
        decision.mounted_skill_ids.join("、")
    ))
}

fn select_static_skill_ids(
    agent: &ConversationAgentConfig,
    required: &[String],
    prompt: &str,
    recent_context: &[String],
) -> Result<Vec<String>, String> {
    if agent.skill_ids.len() <= STATIC_EAGER_SKILL_LIMIT {
        return Ok(mount_static_skill_ids(
            &agent.skill_ids,
            &agent.capability_policy,
            &agent.execution_mode,
        ));
    }

    let available = skills::list_runtime_available_skills()?;
    let available_by_id = available
        .into_iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect::<HashMap<_, _>>();
    let query = build_query(prompt, recent_context);
    let is_analysis_like = looks_like_analysis_request(&query);

    let mut scored = agent
        .skill_ids
        .iter()
        .filter_map(|skill_id| {
            let skill = available_by_id.get(skill_id)?;
            let (score, reasons) =
                score_skill(skill, &query, &agent.execution_mode, true, is_analysis_like);
            Some((Reverse(score), skill.id.clone(), reasons))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    let mut mounted = required.to_vec();
    let lazy_limit = agent
        .capability_policy
        .max_dynamic_skills
        .clamp(1, STATIC_LAZY_MOUNT_LIMIT);

    for (_, skill_id, _) in scored.into_iter().take(lazy_limit) {
        mounted.push(skill_id);
    }

    if mounted.len() == required.len() {
        mounted.extend(agent.skill_ids.iter().take(lazy_limit).cloned());
    }

    Ok(crate::agent_capabilities::dedupe_skill_ids(mounted))
}

fn build_static_strategy_reasons(
    agent: &ConversationAgentConfig,
    mounted: &[String],
    required: &[String],
) -> Vec<String> {
    let mounted_non_required = mounted
        .iter()
        .filter(|skill_id| !required.contains(*skill_id))
        .count();
    if agent.skill_ids.len() <= STATIC_EAGER_SKILL_LIMIT {
        vec![format!(
            "static: 偏好技能较少，直接挂载 {} 个偏好技能，并附加必需系统技能 {} 个",
            mounted_non_required,
            required.len()
        )]
    } else {
        vec![format!(
            "static-lazy: 偏好技能 {} 个，按当前 prompt 只挂载最相关的 {} 个，并附加必需系统技能 {} 个",
            agent.skill_ids.len(),
            mounted_non_required,
            required.len()
        )]
    }
}

fn build_query(prompt: &str, recent_context: &[String]) -> String {
    let mut query = String::new();
    query.push_str(prompt.trim());
    for item in recent_context {
        if !query.is_empty() {
            query.push('\n');
        }
        query.push_str(item.trim());
    }
    query.to_lowercase()
}

fn looks_like_analysis_request(query: &str) -> bool {
    [
        "分析",
        "解释",
        "总结",
        "review",
        "explain",
        "summarize",
        "why",
    ]
    .iter()
    .any(|needle| query.contains(needle))
}

fn score_skill(
    skill: &SkillDefinition,
    query: &str,
    execution_mode: &str,
    preferred: bool,
    is_analysis_like: bool,
) -> (i32, Vec<String>) {
    let mut score = 0;
    let mut reasons = Vec::new();

    if preferred {
        score += 24;
        reasons.push("命中智能体偏好技能".to_string());
    }

    if !skill.modes.is_empty() {
        if skill
            .modes
            .iter()
            .any(|mode| mode == execution_mode || mode == "all")
        {
            score += 8;
            reasons.push(format!("适配 {} 模式", execution_mode));
        } else {
            score -= 12;
        }
    }

    let mut matched_phrases = Vec::new();
    for phrase in skill
        .triggers
        .iter()
        .chain(skill.examples.iter())
        .chain(skill.capabilities.iter())
    {
        let normalized = phrase.trim().to_lowercase();
        if normalized.len() >= 2 && query.contains(&normalized) {
            score += 20;
            matched_phrases.push(phrase.trim().to_string());
        }
    }
    if !matched_phrases.is_empty() {
        reasons.push(format!("命中触发词 {}", matched_phrases.join(" / ")));
    }

    for literal in [
        skill.id.as_str(),
        skill.name.as_str(),
        skill.description.as_str(),
    ] {
        let normalized = literal.trim().to_lowercase();
        if normalized.len() >= 2 && query.contains(&normalized) {
            score += 12;
            reasons.push(format!("直接命中 {}", literal.trim()));
            break;
        }
    }

    if skill.requires_auth {
        score -= 2;
        reasons.push("需要鉴权，降低默认优先级".to_string());
    }
    if is_analysis_like && skill.side_effect_level == "high" {
        score -= 16;
        reasons.push("当前更像分析型请求，降低高副作用工具优先级".to_string());
    }

    (score, reasons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_capabilities::{new_agent_default_capability_policy, AgentCapabilityPolicy};
    use crate::skills;

    fn sample_agent(strategy: &str) -> ConversationAgentConfig {
        ConversationAgentConfig {
            id: "agent".to_string(),
            name: "Agent".to_string(),
            summary: "summary".to_string(),
            description: "desc".to_string(),
            trigger_condition: String::new(),
            manual_trigger_only: false,
            system_prompt: String::new(),
            skill_ids: vec!["pptx".to_string()],
            capability_policy: AgentCapabilityPolicy {
                strategy: strategy.to_string(),
                required_skill_ids: vec![],
                forbidden_skill_ids: vec![],
                max_dynamic_skills: 2,
                forbidden_paths: vec![],
                high_risk_actions: vec![],
            },
            default_provider_id: "openai".to_string(),
            default_model: "gpt-5.4".to_string(),
            allowed_tool_ids: crate::agents::default_allowed_tool_ids(),
            execution_mode: "single".to_string(),
            collaboration_config: None,
            accent_color: None,
            avatar_uri: None,
            scenario_llm_config: None,
            agent_loop_config: None,
        }
    }

    #[test]
    fn runtime_skill_prompt_formats_decision() {
        let prompt = runtime_skill_prompt(&SkillBrokerDecision {
            strategy: "hybrid".to_string(),
            mounted_skill_ids: vec!["alpha".into(), "beta".into()],
            skipped_skill_ids: vec![],
            reasons: vec!["alpha: hit".into()],
        })
        .expect("prompt");

        assert_eq!(prompt, "本轮已装配技能：alpha、beta。");
    }

    #[test]
    fn runtime_skill_prompt_is_observability_only() {
        let prompt = runtime_skill_prompt(&SkillBrokerDecision {
            strategy: "hybrid".to_string(),
            mounted_skill_ids: vec!["alpha".into()],
            skipped_skill_ids: vec![],
            reasons: vec![],
        })
        .expect("prompt");

        assert!(
            prompt.contains("本轮已装配技能"),
            "callers may log this string, but must not append it to the system prompt"
        );
    }

    #[test]
    fn static_strategy_mounts_preferred_and_required() {
        let mut agent = sample_agent("static");
        agent.capability_policy.required_skill_ids = vec!["required".to_string()];
        let decision = select_skills_for_turn(&agent, "make slides", &[]).expect("decision");

        assert_eq!(
            decision.mounted_skill_ids,
            vec![
                "pptx".to_string(),
                "required".to_string(),
                crate::agent_capabilities::ATOMIC_TOOLS_SKILL_ID.to_string(),
                crate::agent_capabilities::TASK_CREATOR_SKILL_ID.to_string()
            ]
        );
    }

    #[test]
    fn static_strategy_with_many_skills_lazy_mounts_only_top_matches() {
        let available = skills::list_runtime_available_skills().expect("runtime skills");
        let target_skill = available
            .iter()
            .find(|skill| {
                skill
                    .triggers
                    .iter()
                    .chain(skill.examples.iter())
                    .chain(skill.capabilities.iter())
                    .any(|item| !item.trim().is_empty())
            })
            .expect("at least one skill with examples");
        let target_phrase = target_skill
            .triggers
            .iter()
            .chain(target_skill.examples.iter())
            .chain(target_skill.capabilities.iter())
            .find(|item| !item.trim().is_empty())
            .expect("target phrase")
            .trim()
            .to_string();
        let mut preferred = vec![target_skill.id.clone()];
        preferred.extend(
            available
                .iter()
                .filter(|skill| skill.id != target_skill.id)
                .take(3)
                .map(|skill| skill.id.clone()),
        );
        assert_eq!(
            preferred.len(),
            4,
            "need four available skills for lazy-mount test"
        );

        let mut agent = sample_agent("static");
        agent.skill_ids = preferred;
        let decision = select_skills_for_turn(&agent, &target_phrase, &[]).expect("decision");

        assert!(decision
            .reasons
            .iter()
            .any(|reason| reason.contains("static-lazy")));
        assert!(decision.mounted_skill_ids.contains(&target_skill.id));
        assert_eq!(
            decision.mounted_skill_ids.len(),
            2 + 2,
            "2 lazy-mounted preferred skills + 2 required system skills"
        );
    }

    #[test]
    fn score_skill_prefers_trigger_matches_and_penalizes_high_side_effect_analysis() {
        let skill = SkillDefinition {
            id: "pptx".to_string(),
            name: "PPTX".to_string(),
            description: "create slide decks".to_string(),
            path: std::path::PathBuf::from("/tmp/pptx"),
            triggers: vec!["slides".to_string()],
            examples: vec!["make a deck".to_string()],
            capabilities: vec!["export pptx".to_string()],
            requires_auth: false,
            side_effect_level: "high".to_string(),
            modes: vec!["worker".to_string()],
        };

        let (worker_score, worker_reasons) =
            score_skill(&skill, "need slides for the board", "worker", true, false);
        let (analysis_score, analysis_reasons) =
            score_skill(&skill, "please analyze these slides", "worker", false, true);

        assert!(worker_score > analysis_score);
        assert!(worker_reasons
            .iter()
            .any(|reason| reason.contains("偏好技能")));
        assert!(analysis_reasons
            .iter()
            .any(|reason| reason.contains("高副作用")));
    }

    #[test]
    fn default_policy_is_hybrid_for_new_agents() {
        let policy = new_agent_default_capability_policy();
        assert_eq!(policy.strategy, "hybrid");
    }
}
