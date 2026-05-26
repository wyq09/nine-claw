import { describe, expect, it } from 'vitest'
import {
  buildSessionContextInjection,
  emptyCacheMetrics,
  shouldCompressBeforeModelSwitch,
  updateCacheMetrics,
} from '../cache-lifecycle'

describe('cache lifecycle helpers', () => {
  it('C3/C6 keeps session context as a system-injected user message', () => {
    const message = buildSessionContextInjection({
      date: '2026-05-20',
      model: 'gpt-test',
      cwd: '/repo',
      previousSummary: 'last summary',
    })

    expect(message.role).toBe('user')
    expect(message.system_injected).toBe(true)
    expect(String(message.content)).toContain('[session_context]')
    expect(String(message.content)).toContain('[previous_session_summary]')
  })

  it('C4 recommends compression before model switch when history is large', () => {
    expect(
      shouldCompressBeforeModelSwitch({
        currentModel: 'a',
        nextModel: 'b',
        config: { targetCompressedTokens: 10 },
        messages: [{ role: 'user', content: 'x'.repeat(200) }],
      }),
    ).toBe(true)
    expect(
      shouldCompressBeforeModelSwitch({
        currentModel: 'a',
        nextModel: 'a',
        config: { targetCompressedTokens: 10 },
        messages: [{ role: 'user', content: 'x'.repeat(200) }],
      }),
    ).toBe(false)
  })

  it('C7 updates cache hit rate and savings metrics per API call', () => {
    const next = updateCacheMetrics(emptyCacheMetrics(), {
      totalInputTokens: 1_000,
      cachedInputTokens: 750,
      actualCost: 0.25,
      estimatedCostWithoutCache: 1,
    })

    expect(next.totalApiCalls).toBe(1)
    expect(next.totalCacheHits).toBe(1)
    expect(next.averageCacheHitRate).toBe(0.75)
    expect(next.cacheSavings).toBe(0.75)
  })
})
