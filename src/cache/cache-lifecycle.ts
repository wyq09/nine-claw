import type { CompressionConfig, ConversationMessage } from '../compression/types'
import { estimateMessagesTokens } from '../compression/compressor'

export type CacheUsage = {
  totalInputTokens: number
  cachedInputTokens: number
  actualCost?: number
  estimatedCostWithoutCache?: number
}

export type CacheMetrics = {
  totalApiCalls: number
  totalCacheHits: number
  totalCacheMisses: number
  totalInputTokens: number
  cachedInputTokens: number
  averageCacheHitRate: number
  actualCost: number
  estimatedCostWithoutCache: number
  cacheSavings: number
}

export function emptyCacheMetrics(): CacheMetrics {
  return {
    totalApiCalls: 0,
    totalCacheHits: 0,
    totalCacheMisses: 0,
    totalInputTokens: 0,
    cachedInputTokens: 0,
    averageCacheHitRate: 0,
    actualCost: 0,
    estimatedCostWithoutCache: 0,
    cacheSavings: 0,
  }
}

export function updateCacheMetrics(previous: CacheMetrics, usage: CacheUsage): CacheMetrics {
  const totalApiCalls = previous.totalApiCalls + 1
  const cacheHit = usage.cachedInputTokens > 0
  const totalInputTokens = previous.totalInputTokens + Math.max(0, usage.totalInputTokens)
  const cachedInputTokens = previous.cachedInputTokens + Math.max(0, usage.cachedInputTokens)
  const actualCost = previous.actualCost + Math.max(0, usage.actualCost ?? 0)
  const estimatedCostWithoutCache =
    previous.estimatedCostWithoutCache + Math.max(0, usage.estimatedCostWithoutCache ?? usage.actualCost ?? 0)
  return {
    totalApiCalls,
    totalCacheHits: previous.totalCacheHits + (cacheHit ? 1 : 0),
    totalCacheMisses: previous.totalCacheMisses + (cacheHit ? 0 : 1),
    totalInputTokens,
    cachedInputTokens,
    averageCacheHitRate: totalInputTokens > 0 ? cachedInputTokens / totalInputTokens : 0,
    actualCost,
    estimatedCostWithoutCache,
    cacheSavings: Math.max(0, estimatedCostWithoutCache - actualCost),
  }
}

export function shouldCompressBeforeModelSwitch(options: {
  currentModel: string
  nextModel: string
  messages: ConversationMessage[]
  config: Pick<CompressionConfig, 'targetCompressedTokens'>
}): boolean {
  if (options.currentModel.trim() === options.nextModel.trim()) return false
  return estimateMessagesTokens(options.messages) > options.config.targetCompressedTokens
}

export function buildSessionContextInjection(input: {
  date: string
  model: string
  cwd: string
  previousSummary?: string | null
}): ConversationMessage {
  const summary = input.previousSummary?.trim()
  const lines = [
    '[session_context]',
    `date: ${input.date}`,
    `model: ${input.model}`,
    `cwd: ${input.cwd}`,
  ]
  if (summary) {
    lines.push('', '[previous_session_summary]', summary, '[/previous_session_summary]')
  }
  lines.push('[/session_context]')
  return {
    role: 'user',
    content: lines.join('\n'),
    system_injected: true,
    metadata: { system_injected: true, purpose: 'session_context' },
  }
}
