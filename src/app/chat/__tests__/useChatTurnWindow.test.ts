import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ConversationTurn } from '../../../types'
import { useChatTurnWindow } from '../useChatTurnWindow'

function buildTurns(prefix: string, count: number): ConversationTurn[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `${prefix}-${index}`,
    prompt: `prompt ${index}`,
    answer: `answer ${index}`,
    thinking: '',
    status: 'done',
    createdAt: index,
    completedAt: index,
    activity: [],
    toolCalls: [],
    responseSegments: [],
  }))
}

describe('useChatTurnWindow', () => {
  let rafCallbacks: Map<number, FrameRequestCallback>
  let rafId: number

  beforeEach(() => {
    rafCallbacks = new Map()
    rafId = 1
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      const id = rafId++
      rafCallbacks.set(id, callback)
      return id
    })
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => {
      rafCallbacks.delete(id)
    })
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  const flushAnimationFrames = () => {
    const callbacks = [...rafCallbacks.values()]
    rafCallbacks.clear()
    act(() => {
      callbacks.forEach((callback) => callback(16))
    })
  }

  it('keeps the session switch first render small before warming the window', () => {
    const scrollParentRef = { current: null }
    const { result, rerender } = renderHook(
      ({ turns, sessionId }) => useChatTurnWindow(turns, sessionId, scrollParentRef),
      {
        initialProps: {
          turns: buildTurns('a', 20),
          sessionId: 'session-a',
        },
      },
    )

    flushAnimationFrames()
    expect(result.current.visibleTurns).toHaveLength(10)

    rerender({
      turns: buildTurns('b', 30),
      sessionId: 'session-b',
    })

    expect(result.current.visibleTurns.map((turn) => turn.id)).toEqual([
      'b-26',
      'b-27',
      'b-28',
      'b-29',
    ])

    flushAnimationFrames()
    expect(result.current.visibleTurns).toHaveLength(10)
    expect(result.current.visibleRangeStart).toBe(20)
  })
})
