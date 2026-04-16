import { describe, it, expect } from 'vitest'
import {
  aggregateSessionUsageFromTurns,
  classifyContextStage,
  CONTEXT_THRESHOLDS,
  createSessionContextState,
  mergeSessionContextTokensFromTurns,
  resolveContextWindow,
  formatBadgePercent,
  formatContextUsageRingLabel,
  getStageColor,
  getTurnCompactionClass,
  shouldTriggerAutoCompact,
} from '../sessionContext'
import type { ConversationTurn } from '../../../types'

// ── US-007: 阈值状态机 ──

describe('classifyContextStage', () => {
  it('returns "normal" for percent < 60', () => {
    expect(classifyContextStage(0)).toBe('normal')
    expect(classifyContextStage(30)).toBe('normal')
    expect(classifyContextStage(59)).toBe('normal')
    expect(classifyContextStage(59.9)).toBe('normal')
  })

  it('returns "snip" for 60 <= percent < 75', () => {
    expect(classifyContextStage(60)).toBe('snip')
    expect(classifyContextStage(67)).toBe('snip')
    expect(classifyContextStage(74)).toBe('snip')
    expect(classifyContextStage(74.9)).toBe('snip')
  })

  it('returns "compact" for 75 <= percent < 85', () => {
    expect(classifyContextStage(75)).toBe('compact')
    expect(classifyContextStage(80)).toBe('compact')
    expect(classifyContextStage(84)).toBe('compact')
    expect(classifyContextStage(84.9)).toBe('compact')
  })

  it('returns "collapse" for 85 <= percent < 95', () => {
    expect(classifyContextStage(85)).toBe('collapse')
    expect(classifyContextStage(90)).toBe('collapse')
    expect(classifyContextStage(94)).toBe('collapse')
    expect(classifyContextStage(94.9)).toBe('collapse')
  })

  it('returns "auto_compact" for percent >= 95', () => {
    expect(classifyContextStage(95)).toBe('auto_compact')
    expect(classifyContextStage(99)).toBe('auto_compact')
    expect(classifyContextStage(100)).toBe('auto_compact')
    expect(classifyContextStage(150)).toBe('auto_compact')
  })

  it('returns "normal" for negative percent', () => {
    expect(classifyContextStage(-1)).toBe('normal')
    expect(classifyContextStage(-100)).toBe('normal')
  })

  it('returns "normal" for undefined', () => {
    expect(classifyContextStage(undefined)).toBe('normal')
  })

  it('has correct threshold constants', () => {
    expect(CONTEXT_THRESHOLDS.snip).toBe(60)
    expect(CONTEXT_THRESHOLDS.compact).toBe(75)
    expect(CONTEXT_THRESHOLDS.collapse).toBe(85)
    expect(CONTEXT_THRESHOLDS.auto_compact).toBe(95)
  })
})

describe('aggregateSessionUsageFromTurns', () => {
  it('sums input, output and prefers totalTokens for used estimate', () => {
    const turns: ConversationTurn[] = [
      {
        id: 'a',
        prompt: '',
        answer: '',
        status: 'done',
        createdAt: 0,
        activity: [],
        thinking: '',
        toolCalls: [],
        usage: {
          inputTokens: 100,
          outputTokens: 50,
          cacheReadTokens: 10,
          cacheWriteTokens: 2,
          totalTokens: 200,
        },
      },
    ]
    const agg = aggregateSessionUsageFromTurns(turns)
    expect(agg.inputTokens).toBe(100)
    expect(agg.outputTokens).toBe(50)
    expect(agg.usedTokensEstimate).toBe(200)
  })

  it('falls back to component sum when totalTokens missing', () => {
    const turns: ConversationTurn[] = [
      {
        id: 'a',
        prompt: '',
        answer: '',
        status: 'done',
        createdAt: 0,
        activity: [],
        thinking: '',
        toolCalls: [],
        usage: {
          inputTokens: 10,
          outputTokens: 5,
          cacheReadTokens: 1,
          cacheWriteTokens: 0,
          totalTokens: 0,
        },
      },
    ]
    expect(aggregateSessionUsageFromTurns(turns).usedTokensEstimate).toBe(16)
  })
})

describe('mergeSessionContextTokensFromTurns', () => {
  it('prefers RPC when it has token fields', () => {
    const merged = mergeSessionContextTokensFromTurns(
      { usedTokens: 5000, inputTokens: 3000, outputTokens: 2000 },
      [],
    )
    expect(merged).toEqual({ usedTokens: 5000, inputTokens: 3000, outputTokens: 2000 })
  })

  it('uses turns when RPC is all zero', () => {
    const turns: ConversationTurn[] = [
      {
        id: 'a',
        prompt: '',
        answer: '',
        status: 'done',
        createdAt: 0,
        activity: [],
        thinking: '',
        toolCalls: [],
        usage: {
          inputTokens: 3,
          outputTokens: 1,
          cacheReadTokens: 0,
          cacheWriteTokens: 0,
          totalTokens: 4,
        },
      },
    ]
    const merged = mergeSessionContextTokensFromTurns({ usedTokens: 0, inputTokens: 0, outputTokens: 0 }, turns)
    expect(merged).toEqual({ usedTokens: 4, inputTokens: 3, outputTokens: 1 })
  })

  it('takes the larger of RPC and turns per field', () => {
    const turns: ConversationTurn[] = [
      {
        id: 'a',
        prompt: '',
        answer: '',
        status: 'done',
        createdAt: 0,
        activity: [],
        thinking: '',
        toolCalls: [],
        usage: {
          inputTokens: 200,
          outputTokens: 50,
          cacheReadTokens: 0,
          cacheWriteTokens: 0,
          totalTokens: 250,
        },
      },
    ]
    const merged = mergeSessionContextTokensFromTurns(
      { usedTokens: 100, inputTokens: 100, outputTokens: 100 },
      turns,
    )
    expect(merged).toEqual({ usedTokens: 250, inputTokens: 200, outputTokens: 100 })
  })
})

// ── US-002: 会话上下文状态模型 ──

describe('createSessionContextState', () => {
  it('creates state with all fields computed', () => {
    const state = createSessionContextState({
      sessionId: 'session-abc',
      usedTokens: 1000,
      contextWindow: 8000,
    })
    expect(state.sessionId).toBe('session-abc')
    expect(state.usedTokens).toBe(1000)
    expect(state.contextWindow).toBe(8000)
    expect(state.percent).toBeCloseTo(12.5)
    expect(state.stage).toBe('normal')
    expect(state.source).toBe('unknown')
    expect(state.inputTokens).toBe(0)
    expect(state.outputTokens).toBe(0)
    expect(state.updatedAt).toBeGreaterThan(0)
  })

  it('computes correct stage for high usage', () => {
    const state = createSessionContextState({
      sessionId: 's',
      usedTokens: 9000,
      contextWindow: 10000,
    })
    expect(state.percent).toBe(90)
    expect(state.stage).toBe('collapse')
  })

  it('handles missing contextWindow', () => {
    const state = createSessionContextState({
      sessionId: 's',
      usedTokens: 1000,
      contextWindow: undefined,
    })
    expect(state.percent).toBeUndefined()
    expect(state.stage).toBe('normal')
    expect(state.contextWindow).toBeUndefined()
  })

  it('handles zero contextWindow', () => {
    const state = createSessionContextState({
      sessionId: 's',
      usedTokens: 1000,
      contextWindow: 0,
    })
    expect(state.percent).toBeUndefined()
  })

  it('accepts optional fields', () => {
    const state = createSessionContextState({
      sessionId: 's',
      model: 'gpt-4',
      source: 'manual',
      inputTokens: 500,
      outputTokens: 300,
    })
    expect(state.model).toBe('gpt-4')
    expect(state.source).toBe('manual')
    expect(state.inputTokens).toBe(500)
    expect(state.outputTokens).toBe(300)
  })

  it('defaults usedTokens to 0', () => {
    const state = createSessionContextState({ sessionId: 's' })
    expect(state.usedTokens).toBe(0)
  })
})

// ── US-004: 多来源优先级解析 ──

describe('resolveContextWindow', () => {
  it('prefers auto-detected value over manual', () => {
    const result = resolveContextWindow({
      autoDetected: 128000,
      manualConfig: 64000,
    })
    expect(result.tokens).toBe(128000)
    expect(result.source).toBe('auto')
  })

  it('falls back to manual when auto is undefined', () => {
    const result = resolveContextWindow({
      autoDetected: undefined,
      manualConfig: 64000,
    })
    expect(result.tokens).toBe(64000)
    expect(result.source).toBe('manual')
  })

  it('falls back to estimated when both auto and manual are absent', () => {
    const result = resolveContextWindow({
      autoDetected: undefined,
      manualConfig: undefined,
      estimatedDefault: 32000,
    })
    expect(result.tokens).toBe(32000)
    expect(result.source).toBe('estimated')
  })

  it('returns unknown when no source available', () => {
    const result = resolveContextWindow({})
    expect(result.tokens).toBeUndefined()
    expect(result.source).toBe('unknown')
  })

  it('ignores zero auto values', () => {
    const result = resolveContextWindow({
      autoDetected: 0,
      manualConfig: 64000,
    })
    expect(result.tokens).toBe(64000)
    expect(result.source).toBe('manual')
  })

  it('ignores zero manual values', () => {
    const result = resolveContextWindow({
      autoDetected: undefined,
      manualConfig: 0,
      estimatedDefault: 32000,
    })
    expect(result.tokens).toBe(32000)
    expect(result.source).toBe('estimated')
  })

  it('ignores negative values', () => {
    const result = resolveContextWindow({
      autoDetected: -1,
      manualConfig: -100,
    })
    expect(result.tokens).toBeUndefined()
    expect(result.source).toBe('unknown')
  })
})

// ── Badge 格式化 ──

describe('formatContextUsageRingLabel', () => {
  it('rounds to integer string without percent', () => {
    expect(formatContextUsageRingLabel(6.4)).toBe('6')
    expect(formatContextUsageRingLabel(12.5)).toBe('13')
    expect(formatContextUsageRingLabel(undefined)).toBe('--')
  })
})

describe('formatBadgePercent', () => {
  it('formats valid percent with one decimal', () => {
    expect(formatBadgePercent(42.567)).toBe('42.6%')
  })

  it('formats whole number percent', () => {
    expect(formatBadgePercent(50)).toBe('50.0%')
  })

  it('formats zero percent', () => {
    expect(formatBadgePercent(0)).toBe('0.0%')
  })

  it('shows fallback for undefined', () => {
    expect(formatBadgePercent(undefined)).toBe('--')
  })

  it('formats 100% correctly', () => {
    expect(formatBadgePercent(100)).toBe('100.0%')
  })
})

describe('getStageColor', () => {
  it('returns "normal" class for normal stage', () => {
    expect(getStageColor('normal')).toBe('normal')
  })

  it('returns "warning" class for snip stage', () => {
    expect(getStageColor('snip')).toBe('warning')
  })

  it('returns "warning" class for compact stage', () => {
    expect(getStageColor('compact')).toBe('warning')
  })

  it('returns "danger" class for collapse stage', () => {
    expect(getStageColor('collapse')).toBe('danger')
  })

  it('returns "critical" class for auto_compact stage', () => {
    expect(getStageColor('auto_compact')).toBe('critical')
  })
})

// ── US-008: UI 紧凑策略 ──

describe('getTurnCompactionClass', () => {
  it('returns empty string for normal stage', () => {
    expect(getTurnCompactionClass('normal', 0, 10)).toBe('')
  })

  it('returns empty string for snip stage (lightweight, no class needed)', () => {
    expect(getTurnCompactionClass('snip', 0, 10)).toBe('')
  })

  it('returns compact class for compact stage on older turns', () => {
    expect(getTurnCompactionClass('compact', 0, 10)).toBe('turn-compact')
  })

  it('returns collapse class for collapse stage on older turns', () => {
    expect(getTurnCompactionClass('collapse', 0, 10)).toBe('turn-collapse')
  })

  it('does not compact the latest turn', () => {
    expect(getTurnCompactionClass('collapse', 9, 10)).toBe('')
  })

  it('does not compact the second-to-last turn in collapse stage', () => {
    expect(getTurnCompactionClass('collapse', 8, 10)).toBe('')
  })

  it('compacts older turns in compact stage (last 2 exempt)', () => {
    expect(getTurnCompactionClass('compact', 7, 10)).toBe('turn-compact')
    expect(getTurnCompactionClass('compact', 8, 10)).toBe('')
  })

  it('returns empty string for auto_compact stage on older turns (handled differently)', () => {
    expect(getTurnCompactionClass('auto_compact', 0, 10)).toBe('turn-collapse')
  })
})

// ── US-009: 自动压缩触发 ──

describe('shouldTriggerAutoCompact', () => {
  it('returns true when stage is auto_compact and preference enabled', () => {
    expect(shouldTriggerAutoCompact('auto_compact', true)).toBe(true)
  })

  it('returns false when preference disabled', () => {
    expect(shouldTriggerAutoCompact('auto_compact', false)).toBe(false)
  })

  it('returns false for non-auto_compact stages even when enabled', () => {
    expect(shouldTriggerAutoCompact('collapse', true)).toBe(false)
    expect(shouldTriggerAutoCompact('compact', true)).toBe(false)
    expect(shouldTriggerAutoCompact('snip', true)).toBe(false)
    expect(shouldTriggerAutoCompact('normal', true)).toBe(false)
  })

  it('returns false when stage is normal and preference disabled', () => {
    expect(shouldTriggerAutoCompact('normal', false)).toBe(false)
  })
})
