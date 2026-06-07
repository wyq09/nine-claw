import type { HistoryItem } from '../../types'
import { appendTextToSegments } from './piAgentPure'

export type PendingToolDelta = {
  toolCallId: string
  argsDelta?: string
  resultDelta?: string
}

/**
 * Applies buffered streaming deltas to history in a single pass.
 *
 * During streaming, each token/delta arrives as a separate event. Calling
 * `setHistory` on every event causes one full React reconciliation per
 * character. This function merges all buffered chunks into a single
 * history update, so callers can use `requestAnimationFrame` to batch
 * all deltas that arrived within one frame into a single `setHistory` call.
 *
 * Returns the same `history` reference (no allocation) when there is
 * nothing to apply.
 */
export function applyBatchedStreamDeltas({
  history,
  historyId,
  turnId,
  textChunks,
  thinkingChunks,
  toolDeltas,
}: {
  history: HistoryItem[]
  historyId: string
  turnId: string
  textChunks: string[]
  thinkingChunks: string[]
  toolDeltas: PendingToolDelta[]
}): HistoryItem[] {
  const combinedText = textChunks.length > 0 ? textChunks.join('') : ''
  const combinedThinking = thinkingChunks.length > 0 ? thinkingChunks.join('') : ''
  const hasToolDeltas = toolDeltas.length > 0

  if (!combinedText && !combinedThinking && !hasToolDeltas) {
    return history
  }

  return history.map((item) => {
    if (item.id !== historyId) return item

    let turnChanged = false
    const updatedTurns = item.turns.map((turn) => {
      if (turn.id !== turnId) return turn
      turnChanged = true

      let next = turn

      if (combinedText) {
        next = {
          ...next,
          answer: next.answer + combinedText,
          responseSegments: appendTextToSegments(next.responseSegments, combinedText),
          status: 'running' as const,
        }
      }

      if (combinedThinking) {
        next = {
          ...next,
          thinking: next.thinking + combinedThinking,
        }
      }

      if (hasToolDeltas) {
        // Merge all deltas for the same tool call into a single accumulated value
        const mergedByToolId = new Map<string, { args: string; result: string }>()
        for (const d of toolDeltas) {
          const acc = mergedByToolId.get(d.toolCallId)
          if (acc) {
            if (d.argsDelta) acc.args += d.argsDelta
            if (d.resultDelta) acc.result += d.resultDelta
          } else {
            mergedByToolId.set(d.toolCallId, {
              args: d.argsDelta ?? '',
              result: d.resultDelta ?? '',
            })
          }
        }

        next = {
          ...next,
          toolCalls: next.toolCalls.map((toolCall) => {
            const merged = mergedByToolId.get(toolCall.toolCallId)
            if (!merged) return toolCall
            return {
              ...toolCall,
              argsText: merged.args ? toolCall.argsText + merged.args : toolCall.argsText,
              resultText: merged.result ? toolCall.resultText + merged.result : toolCall.resultText,
              state: 'running' as const,
            }
          }),
        }
      }

      return next
    })

    if (!turnChanged) return item

    return {
      ...item,
      updatedAt: Date.now(),
      turns: updatedTurns,
    }
  })
}
