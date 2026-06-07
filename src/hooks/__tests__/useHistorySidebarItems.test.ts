import { renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { HistoryItem } from '../../types'
import { useHistorySidebarItems } from '../useHistorySidebarItems'

function makeHistoryItem(id: string, answer: string): HistoryItem {
  return {
    id,
    title: `session ${id}`,
    status: 'done',
    createdAt: 1,
    updatedAt: 2,
    turns: [
      {
        id: `turn-${id}`,
        prompt: 'prompt',
        answer,
        thinking: '',
        status: 'done',
        createdAt: 1,
        completedAt: 2,
        activity: [],
        toolCalls: [],
        responseSegments: [],
      },
    ],
  }
}

describe('useHistorySidebarItems', () => {
  it('keeps unchanged sidebar item references stable across history updates', () => {
    const initial = [makeHistoryItem('a', 'old'), makeHistoryItem('b', 'same')]
    const { result, rerender } = renderHook(
      ({ history }: { history: HistoryItem[] }) => useHistorySidebarItems(history),
      { initialProps: { history: initial } },
    )
    const firstA = result.current[0]
    const firstB = result.current[1]

    rerender({
      history: [makeHistoryItem('a', 'streamed answer'), makeHistoryItem('b', 'same')],
    })

    expect(result.current[0]).not.toBe(firstA)
    expect(result.current[1]).toBe(firstB)
  })
})
