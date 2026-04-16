import type { ConversationTurn } from '../../types'

// ── US-007: 阈值常量与状态机 ──

export const CONTEXT_THRESHOLDS = {
  snip: 60,
  compact: 75,
  collapse: 85,
  auto_compact: 95,
} as const

export type ContextStage = 'normal' | 'snip' | 'compact' | 'collapse' | 'auto_compact'

/** 与 Popover / 状态徽标配套的中文短标签 */
export function getContextStageLabel(stage: ContextStage): string {
  switch (stage) {
    case 'snip':
      return '裁剪'
    case 'compact':
      return '紧凑'
    case 'collapse':
      return '折叠'
    case 'auto_compact':
      return '自动压缩'
    default:
      return '正常'
  }
}

export function classifyContextStage(percent: number | undefined): ContextStage {
  if (percent === undefined || percent === null || percent < CONTEXT_THRESHOLDS.snip) {
    return 'normal'
  }
  if (percent < CONTEXT_THRESHOLDS.compact) return 'snip'
  if (percent < CONTEXT_THRESHOLDS.collapse) return 'compact'
  if (percent < CONTEXT_THRESHOLDS.auto_compact) return 'collapse'
  return 'auto_compact'
}

// ── US-002: 会话上下文状态模型 ──

export type ContextWindowSource = 'auto' | 'manual' | 'estimated' | 'unknown'

export type SessionContextState = {
  sessionId: string
  model: string
  usedTokens: number
  contextWindow: number | undefined
  percent: number | undefined
  stage: ContextStage
  source: ContextWindowSource
  inputTokens: number
  outputTokens: number
  updatedAt: number
}

/** 从会话 turns 累计用量（与 SQLite 历史快照同源；用于与后端结果取 max） */
export function aggregateSessionUsageFromTurns(turns: ConversationTurn[]): {
  inputTokens: number
  outputTokens: number
  usedTokensEstimate: number
} {
  let inputTokens = 0
  let outputTokens = 0
  let cacheRead = 0
  let cacheWrite = 0
  let totalFromUsage = 0
  for (const turn of turns) {
    const u = turn.usage
    if (!u) continue
    inputTokens += u.inputTokens ?? 0
    outputTokens += u.outputTokens ?? 0
    cacheRead += u.cacheReadTokens ?? 0
    cacheWrite += u.cacheWriteTokens ?? 0
    totalFromUsage += u.totalTokens ?? 0
  }
  const componentSum = inputTokens + outputTokens + cacheRead + cacheWrite
  const usedTokensEstimate = totalFromUsage > 0 ? totalFromUsage : componentSum
  return { inputTokens, outputTokens, usedTokensEstimate }
}

/** 合并 SQLite/PI 与内存 turns；按字段取较大值，避免「库已更新但 UI 未落盘」或反之漏计 */
export function mergeSessionContextTokensFromTurns(
  stats: {
    usedTokens: number
    inputTokens: number
    outputTokens: number
  },
  turns: ConversationTurn[],
): { usedTokens: number; inputTokens: number; outputTokens: number } {
  const agg = aggregateSessionUsageFromTurns(turns)
  const rpcHasIo = stats.inputTokens > 0 || stats.outputTokens > 0
  const usedFromRpc = stats.usedTokens > 0
  let usedTokens = usedFromRpc ? stats.usedTokens : agg.usedTokensEstimate
  let inputTokens = rpcHasIo ? stats.inputTokens : agg.inputTokens
  let outputTokens = rpcHasIo ? stats.outputTokens : agg.outputTokens
  usedTokens = Math.max(usedTokens, agg.usedTokensEstimate)
  inputTokens = Math.max(inputTokens, agg.inputTokens)
  outputTokens = Math.max(outputTokens, agg.outputTokens)
  return { usedTokens, inputTokens, outputTokens }
}

export function createSessionContextState(input: {
  sessionId: string
  usedTokens?: number
  contextWindow?: number
  model?: string
  source?: ContextWindowSource
  inputTokens?: number
  outputTokens?: number
}): SessionContextState {
  const usedTokens = input.usedTokens ?? 0
  const contextWindow = input.contextWindow
  const percent =
    contextWindow !== undefined && contextWindow > 0
      ? (usedTokens / contextWindow) * 100
      : undefined

  return {
    sessionId: input.sessionId,
    model: input.model ?? '',
    usedTokens,
    contextWindow,
    percent,
    stage: classifyContextStage(percent),
    source: input.source ?? 'unknown',
    inputTokens: input.inputTokens ?? 0,
    outputTokens: input.outputTokens ?? 0,
    updatedAt: Date.now(),
  }
}

// ── US-004: 多来源优先级解析 ──

export type ResolvedContextWindow = {
  tokens: number | undefined
  source: ContextWindowSource
}

export function resolveContextWindow(sources: {
  autoDetected?: number
  manualConfig?: number
  estimatedDefault?: number
}): ResolvedContextWindow {
  if (sources.autoDetected && sources.autoDetected > 0) {
    return { tokens: sources.autoDetected, source: 'auto' }
  }
  if (sources.manualConfig && sources.manualConfig > 0) {
    return { tokens: sources.manualConfig, source: 'manual' }
  }
  if (sources.estimatedDefault && sources.estimatedDefault > 0) {
    return { tokens: sources.estimatedDefault, source: 'estimated' }
  }
  return { tokens: undefined, source: 'unknown' }
}

// ── Badge 格式化工具 ──

export function formatBadgePercent(percent: number | undefined): string {
  if (percent === undefined) return '--'
  return `${percent.toFixed(1)}%`
}

/** 输入区内环：整数 0–100，无百分号 */
export function formatContextUsageRingLabel(percent: number | undefined): string {
  if (percent === undefined) return '--'
  return String(Math.round(Math.min(Math.max(percent, 0), 100)))
}

export function getStageColor(stage: ContextStage): string {
  switch (stage) {
    case 'snip':
      return 'warning'
    case 'compact':
      return 'warning'
    case 'collapse':
      return 'danger'
    case 'auto_compact':
      return 'critical'
    default:
      return 'normal'
  }
}

// ── US-008: UI 紧凑策略 ──

export function getTurnCompactionClass(
  stage: ContextStage,
  turnIndex: number,
  totalTurns: number,
): string {
  // Always exempt the last 2 turns from compaction
  const exemptThreshold = totalTurns - 2
  if (turnIndex >= exemptThreshold) {
    return ''
  }

  switch (stage) {
    case 'compact':
      return 'turn-compact'
    case 'collapse':
    case 'auto_compact':
      return 'turn-collapse'
    default:
      return ''
  }
}

// ── US-009: 自动压缩触发 ──

export function shouldTriggerAutoCompact(
  stage: ContextStage,
  preferenceEnabled: boolean,
): boolean {
  return stage === 'auto_compact' && preferenceEnabled
}
