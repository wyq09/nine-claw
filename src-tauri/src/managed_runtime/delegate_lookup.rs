//! Delegate agent lookup — extracted from `managed_runtime.rs` so the
//! delegate matching logic lives in its own reviewable module.
//!
//! Pure matching helpers over `AgentRecord`s: normalize/kebab agent ids and
//! roles into lookup keys, score fuzzy role/task matches, and format the
//! candidate list used in delegate error messages.

use std::collections::{HashMap, HashSet};

use crate::agents::AgentRecord;

/// Role hint attached to a candidate agent by the delegate dispatcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DelegateRoleHint {
    pub(crate) role: String,
}

fn normalize_delegate_lookup_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn kebab_case_delegate_lookup_key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_was_sep = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep && !out.is_empty() {
            out.push('-');
            last_was_sep = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn build_delegate_lookup_keys(
    agent: &AgentRecord,
    role_hint: Option<&DelegateRoleHint>,
) -> HashSet<String> {
    let mut keys = HashSet::new();
    for raw in [
        agent.id.trim(),
        agent.name.trim(),
        role_hint.map(|hint| hint.role.trim()).unwrap_or_default(),
    ] {
        if raw.is_empty() {
            continue;
        }
        let normalized = normalize_delegate_lookup_key(raw);
        if !normalized.is_empty() {
            keys.insert(normalized);
        }
        let kebab = kebab_case_delegate_lookup_key(raw);
        if !kebab.is_empty() {
            keys.insert(kebab.clone());
            keys.insert(format!("agent-{kebab}"));
        }
    }
    keys
}

fn tokenize_delegate_lookup_text(value: &str) -> Vec<String> {
    value
        .split(|ch: char| {
            !(matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_')
                || ('\u{4e00}'..='\u{9fff}').contains(&ch))
        })
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn build_delegate_search_haystack(
    agent: &AgentRecord,
    role_hint: Option<&DelegateRoleHint>,
) -> String {
    [
        agent.id.trim(),
        agent.name.trim(),
        role_hint.map(|hint| hint.role.trim()).unwrap_or_default(),
        agent.summary.trim(),
        agent.description.trim(),
    ]
    .into_iter()
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>()
    .join("\n")
    .to_lowercase()
}

fn score_delegate_agent_match(
    agent: &AgentRecord,
    role_hint: Option<&DelegateRoleHint>,
    requested_role: &str,
    task: &str,
) -> usize {
    let haystack = build_delegate_search_haystack(agent, role_hint);
    if haystack.is_empty() {
        return 0;
    }

    let mut score = 0usize;
    for needle in [requested_role, task] {
        let normalized = needle.trim().to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if haystack.contains(&normalized) {
            score += normalized.chars().count().max(1) * 10;
        }
        for token in tokenize_delegate_lookup_text(needle) {
            if token.chars().count() <= 1 {
                continue;
            }
            if haystack.contains(&token) {
                score += token.chars().count();
            }
        }
    }
    score
}

pub(crate) fn resolve_delegate_agent<'a>(
    candidate_agents: &'a [&AgentRecord],
    role_hints: &HashMap<String, DelegateRoleHint>,
    requested_role: &str,
    task: &str,
) -> Option<&'a AgentRecord> {
    let requested_role = requested_role.trim();
    if requested_role.is_empty() {
        return None;
    }
    let normalized = normalize_delegate_lookup_key(requested_role);
    let kebab = kebab_case_delegate_lookup_key(requested_role);
    let mut requested_keys = HashSet::new();
    requested_keys.insert(normalized.clone());
    if !kebab.is_empty() {
        requested_keys.insert(kebab.clone());
        requested_keys.insert(format!("agent-{kebab}"));
    }

    candidate_agents
        .iter()
        .copied()
        .find(|agent| agent.id == requested_role)
        .or_else(|| {
            candidate_agents.iter().copied().find(|agent| {
                let role_hint = role_hints.get(&agent.id);
                build_delegate_lookup_keys(agent, role_hint)
                    .iter()
                    .any(|key| requested_keys.contains(key))
            })
        })
        .or_else(|| {
            candidate_agents.iter().copied().find(|agent| {
                let role_hint = role_hints.get(&agent.id);
                let haystacks = [
                    normalize_delegate_lookup_key(&agent.name),
                    role_hint
                        .map(|hint| normalize_delegate_lookup_key(&hint.role))
                        .unwrap_or_default(),
                ];
                haystacks
                    .iter()
                    .filter(|value| !value.is_empty())
                    .any(|value| value.contains(&normalized))
            })
        })
        .or_else(|| {
            candidate_agents
                .iter()
                .copied()
                .filter_map(|agent| {
                    let role_hint = role_hints.get(&agent.id);
                    let score = score_delegate_agent_match(agent, role_hint, requested_role, task);
                    (score > 0).then_some((score, agent))
                })
                .max_by(|(left_score, left_agent), (right_score, right_agent)| {
                    left_score
                        .cmp(right_score)
                        .then_with(|| right_agent.updated_at.cmp(&left_agent.updated_at))
                })
                .map(|(_, agent)| agent)
        })
}

pub(crate) fn format_delegate_candidates(
    candidate_agents: &[&AgentRecord],
    role_hints: &HashMap<String, DelegateRoleHint>,
) -> String {
    if candidate_agents.is_empty() {
        return "无".to_string();
    }
    candidate_agents
        .iter()
        .map(|agent| {
            let mut label = format!("{}({})", agent.name, agent.id);
            if let Some(role_hint) = role_hints.get(&agent.id) {
                let role = role_hint.role.trim();
                if !role.is_empty() {
                    label.push_str(&format!(" role={role}"));
                }
            }
            label
        })
        .collect::<Vec<_>>()
        .join(", ")
}
