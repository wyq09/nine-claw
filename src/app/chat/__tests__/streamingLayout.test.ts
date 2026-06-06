import { describe, expect, it } from 'vitest'
import type { ConversationTurn } from '../../../types'
import { getStreamingTurnLayoutRevision } from '../streamingLayout'

function buildTurn(overrides: Partial<ConversationTurn> = {}): ConversationTurn {
  return {
    id: 'turn-1',
    prompt: 'prompt',
    answer: '',
    status: 'running',
    createdAt: 1,
    thinking: '',
    activity: [],
    toolCalls: [],
    responseSegments: [],
    ...overrides,
  }
}

describe('getStreamingTurnLayoutRevision', () => {
  it('keeps small text deltas in the same auto-scroll bucket', () => {
    const baseTurn = buildTurn({ answer: 'a'.repeat(10) })
    const nextTurn = buildTurn({ answer: 'a'.repeat(40) })

    expect(getStreamingTurnLayoutRevision(baseTurn)).toBe(0)
    expect(getStreamingTurnLayoutRevision(nextTurn)).toBe(0)
  })

  it('advances revision when streamed content crosses a bucket or structure changes', () => {
    const withText = buildTurn({ answer: 'a'.repeat(48) })
    const withTool = buildTurn({
      answer: 'a'.repeat(48),
      toolCalls: [
        {
          toolCallId: 'tool-1',
          toolName: 'search',
          argsText: '',
          resultText: '',
          state: 'running',
          createdAt: 1,
        },
      ],
    })

    expect(getStreamingTurnLayoutRevision(withText)).toBe(1)
    expect(getStreamingTurnLayoutRevision(withTool)).toBe(5)
  })
})
