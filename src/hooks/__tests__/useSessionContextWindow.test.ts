import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'

vi.mock('../../lib/piClient', () => ({
  getSessionContextStats: vi.fn(),
}))

import { getSessionContextStats } from '../../lib/piClient'
import type { ConversationTurn } from '../../types'
import { useSessionContextWindow } from '../useSessionContextWindow'

const baseTurn = (usage: ConversationTurn['usage']): ConversationTurn => ({
  id: 't1',
  prompt: 'p',
  answer: 'a',
  status: 'done',
  createdAt: 1,
  activity: [],
  thinking: '',
  toolCalls: [],
  usage,
})

const mockGetStats = vi.mocked(getSessionContextStats)

describe('useSessionContextWindow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns null state initially with loading true', () => {
    const { result } = renderHook(() =>
      useSessionContextWindow({
        sessionId: 'abc',
        providerConfig: null,
      }),
    )
    expect(result.current.state).toBeNull()
    expect(result.current.loading).toBe(true)
    expect(result.current.error).toBeNull()
  })

  it('fetches stats when sessionId is provided', async () => {
    mockGetStats.mockResolvedValue({
      sessionId: 'abc',
      usedTokens: 5000,
      contextWindow: 128000,
      inputTokens: 3000,
      outputTokens: 2000,
      model: 'gpt-4',
      source: 'auto',
    })

    const { result } = renderHook(() =>
      useSessionContextWindow({
        sessionId: 'abc',
        providerConfig: null,
      }),
    )

    await waitFor(() => {
      expect(result.current.state).not.toBeNull()
    })

    expect(result.current.state!.percent).toBeCloseTo(3.90625, 4)
    expect(result.current.state!.stage).toBe('normal')
    expect(result.current.state!.source).toBe('auto')
    expect(result.current.state!.inputTokens).toBe(3000)
    expect(result.current.state!.outputTokens).toBe(2000)
  })

  it('isolates state per session', async () => {
    mockGetStats.mockImplementation(async (id: string) => ({
      sessionId: id,
      usedTokens: id === 'a' ? 100 : 200,
      contextWindow: 1000,
      inputTokens: 0,
      outputTokens: 0,
      model: null,
      source: 'auto',
    }))

    const { result, rerender } = renderHook(
      (props: { sessionId: string; providerConfig: null }) =>
        useSessionContextWindow(props),
      { initialProps: { sessionId: 'a', providerConfig: null } },
    )

    await waitFor(() => expect(result.current.state?.sessionId).toBe('a'))

    rerender({ sessionId: 'b', providerConfig: null })

    await waitFor(() => expect(result.current.state?.sessionId).toBe('b'))
    expect(result.current.state!.usedTokens).toBe(200)
  })

  it('handles fetch error gracefully', async () => {
    mockGetStats.mockRejectedValue(new Error('RPC failed'))

    const { result } = renderHook(() =>
      useSessionContextWindow({
        sessionId: 'abc',
        providerConfig: null,
      }),
    )

    await waitFor(() => expect(result.current.error).toBeTruthy())
    expect(result.current.state).toBeNull()
    expect(result.current.loading).toBe(false)
  })

  it('uses manual maxContextTokens when RPC returns null contextWindow', async () => {
    mockGetStats.mockResolvedValue({
      sessionId: 'abc',
      usedTokens: 5000,
      contextWindow: null,
      inputTokens: 3000,
      outputTokens: 2000,
      model: 'gpt-4',
      source: 'unknown',
    })

    const { result } = renderHook(() =>
      useSessionContextWindow({
        sessionId: 'abc',
        providerConfig: { maxContextTokens: 64000 },
      }),
    )

    await waitFor(() => expect(result.current.state).not.toBeNull())
    expect(result.current.state!.contextWindow).toBe(64000)
    expect(result.current.state!.source).toBe('manual')
  })

  it('does not block when refresh is called', () => {
    mockGetStats.mockImplementation(
      () => new Promise((resolve) => setTimeout(resolve, 5000)),
    )

    const { result } = renderHook(() =>
      useSessionContextWindow({
        sessionId: 'abc',
        providerConfig: null,
      }),
    )

    // refresh should not throw
    expect(() => result.current.refresh()).not.toThrow()
  })

  it('returns empty state for empty sessionId', () => {
    const { result } = renderHook(() =>
      useSessionContextWindow({
        sessionId: '',
        providerConfig: null,
      }),
    )
    expect(result.current.state).toBeNull()
  })

  it('uses cumulative usage from turns when RPC returns zeros', async () => {
    mockGetStats.mockResolvedValue({
      sessionId: 'abc',
      usedTokens: 0,
      contextWindow: 100000,
      inputTokens: 0,
      outputTokens: 0,
      model: null,
      source: 'unknown',
    })

    const turns: ConversationTurn[] = [
      baseTurn({
        inputTokens: 11360,
        outputTokens: 311,
        cacheReadTokens: 0,
        cacheWriteTokens: 0,
        totalTokens: 11671,
      }),
    ]

    const { result } = renderHook(() =>
      useSessionContextWindow({
        sessionId: 'abc',
        providerConfig: null,
        turns,
      }),
    )

    await waitFor(() => {
      expect(result.current.state).not.toBeNull()
    })

    expect(result.current.state!.inputTokens).toBe(11360)
    expect(result.current.state!.outputTokens).toBe(311)
    expect(result.current.state!.usedTokens).toBe(11671)
  })
})
