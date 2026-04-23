import { describe, expect, it } from 'vitest'
import {
  createDefaultAgentCapabilityPolicy,
  createStaticAgentCapabilityPolicy,
  formatAgentSkillStrategyLabel,
  normalizeAgentCapabilityPolicy,
} from '../agentCapabilities'

describe('normalizeAgentCapabilityPolicy', () => {
  it('creates a hybrid default for new drafts', () => {
    expect(createDefaultAgentCapabilityPolicy()).toEqual({
      strategy: 'hybrid',
      requiredSkillIds: [],
      forbiddenSkillIds: [],
      maxDynamicSkills: 4,
    })
  })

  it('dedupes ids and removes forbidden ids that are also required', () => {
    const normalized = normalizeAgentCapabilityPolicy(
      {
        strategy: 'dynamic',
        requiredSkillIds: ['alpha', 'alpha'],
        forbiddenSkillIds: ['alpha', 'beta'],
        maxDynamicSkills: 99,
      },
      createStaticAgentCapabilityPolicy(),
    )

    expect(normalized).toEqual({
      strategy: 'dynamic',
      requiredSkillIds: ['alpha'],
      forbiddenSkillIds: ['beta'],
      maxDynamicSkills: 8,
    })
  })
})

describe('formatAgentSkillStrategyLabel', () => {
  it('formats all supported strategy labels', () => {
    expect(formatAgentSkillStrategyLabel('static')).toBe('静态挂载')
    expect(formatAgentSkillStrategyLabel('hybrid')).toBe('混合动态')
    expect(formatAgentSkillStrategyLabel('dynamic')).toBe('全动态')
  })
})
