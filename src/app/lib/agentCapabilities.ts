import type { AgentCapabilityPolicy, AgentSkillStrategy } from '../../types'

export const DEFAULT_MAX_DYNAMIC_SKILLS = 4

export function createDefaultAgentCapabilityPolicy(): AgentCapabilityPolicy {
  return {
    strategy: 'hybrid',
    requiredSkillIds: [],
    forbiddenSkillIds: [],
    maxDynamicSkills: DEFAULT_MAX_DYNAMIC_SKILLS,
  }
}

export function createStaticAgentCapabilityPolicy(): AgentCapabilityPolicy {
  return {
    strategy: 'static',
    requiredSkillIds: [],
    forbiddenSkillIds: [],
    maxDynamicSkills: DEFAULT_MAX_DYNAMIC_SKILLS,
  }
}

export function normalizeAgentSkillStrategy(value: unknown): AgentSkillStrategy {
  if (value === 'hybrid' || value === 'dynamic') {
    return value
  }
  return 'static'
}

export function dedupeSkillIds(values: string[]): string[] {
  return Array.from(
    new Set(
      values
        .flatMap((value) => value.split(','))
        .map((value) => value.trim())
        .filter(Boolean),
    ),
  )
}

export function normalizeAgentCapabilityPolicy(
  policy?: Partial<AgentCapabilityPolicy> | null,
  fallback: AgentCapabilityPolicy = createStaticAgentCapabilityPolicy(),
): AgentCapabilityPolicy {
  return {
    strategy: normalizeAgentSkillStrategy(policy?.strategy ?? fallback.strategy),
    requiredSkillIds: dedupeSkillIds(policy?.requiredSkillIds ?? fallback.requiredSkillIds),
    forbiddenSkillIds: dedupeSkillIds(
      (policy?.forbiddenSkillIds ?? fallback.forbiddenSkillIds).filter(
        (skillId) => !(policy?.requiredSkillIds ?? fallback.requiredSkillIds).includes(skillId),
      ),
    ),
    maxDynamicSkills: Math.max(
      1,
      Math.min(8, Math.trunc(policy?.maxDynamicSkills ?? fallback.maxDynamicSkills ?? DEFAULT_MAX_DYNAMIC_SKILLS)),
    ),
  }
}

export function formatAgentSkillStrategyLabel(strategy: AgentSkillStrategy): string {
  if (strategy === 'dynamic') {
    return '全动态'
  }
  if (strategy === 'hybrid') {
    return '混合动态'
  }
  return '静态挂载'
}

export function formatSkillIdList(value: string[]): string {
  return dedupeSkillIds(value).join(', ')
}
