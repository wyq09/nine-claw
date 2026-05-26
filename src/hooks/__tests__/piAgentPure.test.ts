import { describe, expect, it } from 'vitest'
import {
  createEmptyHistoryItem,
  deriveConversationTitle,
  deriveFirstUserTurnConversationTitle,
  parseHistorySnapshot,
} from '../piAgent/piAgentPure'

describe('deriveFirstUserTurnConversationTitle', () => {
  it('ignores assistant reply style follow-up topics', () => {
    const prompt = '咖啡店海报'

    expect(deriveConversationTitle(prompt, '我还能顺手补一份社媒日历策划方案')).toBe('咖啡店海报')
    expect(deriveFirstUserTurnConversationTitle(prompt)).toBe('咖啡店海报')
  })
})

describe('createEmptyHistoryItem', () => {
  it('creates a persisted empty session shell with metadata', () => {
    const item = createEmptyHistoryItem({
      agent: {
        id: 'agent-1',
        name: '阿虾',
        summary: '总结专家',
        description: '负责总结',
        systemPrompt: '',
        capabilityPolicy: {
          strategy: 'static',
          requiredSkillIds: [],
          forbiddenSkillIds: [],
          maxDynamicSkills: 0,
        },
        skillIds: [],
        allowedToolIds: [],
        defaultProviderId: 'openai',
        defaultModel: 'gpt-4o-mini',
        executionMode: 'single',
      },
      sessionLlm: {
        providerId: 'openai',
        model: 'gpt-4o-mini',
      },
      workspaceId: 'workspace-1',
      createdAt: 123,
    })

    expect(item.title).toBe('新会话')
    expect(item.status).toBe('done')
    expect(item.turns).toEqual([])
    expect(item.workspaceId).toBe('workspace-1')
    expect(item.sessionLlmProviderId).toBe('openai')
    expect(item.sessionLlmModel).toBe('gpt-4o-mini')
    expect(item.agent?.id).toBe('agent-1')
    expect(item.createdAt).toBe(123)
    expect(item.updatedAt).toBe(123)
  })
})

describe('parseHistorySnapshot', () => {
  it('preserves a stored llm-generated title when hydrating history', () => {
    const history = parseHistorySnapshot(
      JSON.stringify([
        {
          id: 'session-1',
          title: '咖啡店开业海报',
          status: 'done',
          createdAt: 100,
          updatedAt: 200,
          turns: [
            {
              id: 'turn-1',
              prompt: '帮我写一版咖啡店开业海报文案',
              answer: '可以，我先给你一个主视觉方向。',
              status: 'done',
              createdAt: 100,
              thinking: '',
              activity: [],
              toolCalls: [],
            },
          ],
        },
      ]),
    )

    expect(history[0]?.title).toBe('咖啡店开业海报')
  })

  it('regenerates placeholder titles from the first turn', () => {
    const history = parseHistorySnapshot(
      JSON.stringify([
        {
          id: 'session-1',
          title: '新会话',
          status: 'done',
          createdAt: 100,
          updatedAt: 200,
          turns: [
            {
              id: 'turn-1',
              prompt: '帮我写一版咖啡店开业海报文案',
              answer: '',
              status: 'done',
              createdAt: 100,
              thinking: '',
              activity: [],
              toolCalls: [],
            },
          ],
        },
      ]),
    )

    expect(history[0]?.title).toBe(deriveConversationTitle('帮我写一版咖啡店开业海报文案'))
  })

  it('does not silently truncate sessions beyond 30 items during snapshot hydration', () => {
    const payload = JSON.stringify(
      Array.from({ length: 33 }, (_, index) => ({
        id: `session-${index + 1}`,
        title: `会话 ${index + 1}`,
        status: 'done',
        createdAt: 100 + index,
        updatedAt: 200 + index,
        turns: [],
      })),
    )

    const history = parseHistorySnapshot(payload)

    expect(history).toHaveLength(33)
    expect(history.at(-1)?.id).toBe('session-33')
  })
})
