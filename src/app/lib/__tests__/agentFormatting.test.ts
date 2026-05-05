import { describe, expect, it } from 'vitest'
import {
  createDefaultAgentAllowedToolIds,
  createEmptyAgentDraft,
  normalizeAgentAllowedToolIds,
  normalizeAgentDraft,
  validateAgentDraft,
} from '../appFormatting'

describe('agent draft formatting', () => {
  it('normalizes the simplified agent configuration fields', () => {
    const draft = createEmptyAgentDraft('openai', 'gpt-5.4')
    const normalized = normalizeAgentDraft({
      ...draft,
      id: ' reviewer_agent ',
      name: '  Review Agent  ',
      description: '  Reviews ${ARG}  ',
      avatarUri: '  /tmp/reviewer.png  ',
      triggerCondition: '  用户需要代码审查时  ',
      manualTriggerOnly: true,
      systemPrompt: '  Focus on ${ARG}.  ',
      skillIds: [' pdf ', 'pdf', 'pptx'],
      capabilityPolicy: {
        strategy: 'dynamic',
        requiredSkillIds: ['anything'],
        forbiddenSkillIds: [],
        maxDynamicSkills: 8,
      },
    })

    expect(normalized.id).toBe('reviewer_agent')
    expect(normalized.name).toBe('Review Agent')
    expect(normalized.avatarUri).toBe('/tmp/reviewer.png')
    expect(normalized.triggerCondition).toBe('用户需要代码审查时')
    expect(normalized.manualTriggerOnly).toBe(true)
    expect(normalized.systemPrompt).toBe('Focus on ${ARG}.')
    expect(normalized.skillIds).toEqual(['pdf', 'pptx'])
    expect(normalized.allowedToolIds).toEqual(createDefaultAgentAllowedToolIds())
    expect(normalized.capabilityPolicy?.strategy).toBe('static')
    expect(normalized.allowedToolIds).toContain('memory_read')
    expect(normalized.allowedToolIds).toContain('memory_store')
    expect(normalized.allowedToolIds).toContain('chat_search')
  })

  it('normalizes agent tool permissions and runtime aliases', () => {
    expect(normalizeAgentAllowedToolIds(['read', 'find', 'agent_delegate', 'unknown', 'read_file'])).toEqual([
      'read_file',
      'glob',
      'agent_spawn',
    ])
    expect(normalizeAgentAllowedToolIds([])).toEqual([])
    expect(normalizeAgentAllowedToolIds(undefined)).toEqual(createDefaultAgentAllowedToolIds())
    expect(normalizeAgentAllowedToolIds(['memory_read', 'memory_store', 'chat_search'])).toEqual([
      'memory_read',
      'memory_store',
      'chat_search',
    ])
  })

  it('rejects invalid editable Agent_ID values', () => {
    const draft = createEmptyAgentDraft('openai', 'gpt-5.4')
    expect(validateAgentDraft({ ...draft, id: 'bad id', name: 'Agent', description: 'Desc' })).toBe(
      'Agent_ID 只能包含英文字母、数字、下划线和连字符。',
    )
  })
})
