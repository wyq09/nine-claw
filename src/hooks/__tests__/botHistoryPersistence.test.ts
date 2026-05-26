import { describe, expect, it } from 'vitest'
import type { BotMessageEvent } from '../../lib/piClient'
import { buildPersistedBotConversationForInbound, serializeConversationTurnForStructuredUpdate } from '../piAgent/botHistoryPersistence'
import { buildNewTurn } from '../piAgent/piAgentPure'

describe('botHistoryPersistence', () => {
  it('builds a persisted session shell for a first inbound bot message', () => {
    const message = {
      channel_id: 'c1',
      user_id: 'u1',
      direction: 'inbound',
      content: 'hello',
      timestamp: 123,
      agent: {
        id: 'agent-1',
        name: '九节虾',
        summary: 'sum',
        description: 'desc',
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
    } satisfies BotMessageEvent

    const turn = buildNewTurn(message.content)

    const result = buildPersistedBotConversationForInbound({
      historyId: 'session-1',
      message,
      now: message.timestamp,
      turn,
    })

    expect(result.forceCreateSession).toBe(true)
    expect(result.item).toMatchObject({
      id: 'session-1',
      title: expect.any(String),
      status: 'running',
      botTarget: { channelId: 'c1', userId: 'u1' },
      turns: [{ prompt: 'hello', status: 'running' }],
    })
  })

  it('serializes a turn snapshot for structured updates', () => {
    const turn = buildNewTurn('hello')
    turn.answer = 'world'
    turn.status = 'done'
    turn.completedAt = 456

    const serialized = serializeConversationTurnForStructuredUpdate(turn)

    expect(serialized).toMatchObject({
      id: turn.id,
      answer: 'world',
      status: 'done',
      completedAt: 456,
      usageJson: null,
    })
  })
})
