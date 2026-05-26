import type { HistoryItem } from '../../types'
import { parseHistorySnapshot } from './piAgentPure'

export type StructuredHistoryLoadResult = {
  history: HistoryItem[]
  sessionIds: Set<string>
}

type ResolveHydratedHistorySourcesInput = {
  snapshotPayloadResult: PromiseSettledResult<string | null>
  structuredResult: PromiseSettledResult<StructuredHistoryLoadResult>
  legacyHistory: HistoryItem[]
}

type ResolveHydratedHistorySourcesResult = {
  history: HistoryItem[]
  loaded: boolean
  shouldPersist: boolean
  usedLegacy: boolean
  warnings: string[]
}

function formatHydrationError(label: string, reason: unknown) {
  const message = reason instanceof Error ? reason.message : String(reason)
  return `${label} failed: ${message}`
}

export function resolveHydratedHistorySources(
  input: ResolveHydratedHistorySourcesInput,
): ResolveHydratedHistorySourcesResult {
  const warnings: string[] = []

  const snapshotHistory =
    input.snapshotPayloadResult.status === 'fulfilled'
      ? parseHistorySnapshot(input.snapshotPayloadResult.value)
      : (() => {
          warnings.push(formatHydrationError('history snapshot load', input.snapshotPayloadResult.reason))
          return []
        })()

  const structured =
    input.structuredResult.status === 'fulfilled'
      ? input.structuredResult.value
      : (() => {
          warnings.push(formatHydrationError('structured history load', input.structuredResult.reason))
          return { history: [], sessionIds: new Set<string>() }
        })()

  let nextHistory = structured.history
  let shouldPersist = false
  let usedLegacy = false

  if (structured.history.length > 0) {
    const missingSnapshotItems = snapshotHistory.filter((item) => !structured.sessionIds.has(item.id))
    if (missingSnapshotItems.length > 0) {
      nextHistory = [...structured.history, ...missingSnapshotItems]
        .sort((left, right) => (right.updatedAt || right.createdAt) - (left.updatedAt || left.createdAt))
    }
    if (
      input.snapshotPayloadResult.status !== 'fulfilled' ||
      snapshotHistory.length === 0 ||
      missingSnapshotItems.length > 0
    ) {
      shouldPersist = true
    }
  } else if (snapshotHistory.length > 0) {
    nextHistory = snapshotHistory
    shouldPersist = true
  } else if (input.legacyHistory.length > 0) {
    nextHistory = input.legacyHistory
    shouldPersist = true
    usedLegacy = true
  }

  const loaded =
    input.snapshotPayloadResult.status === 'fulfilled' ||
    input.structuredResult.status === 'fulfilled' ||
    input.legacyHistory.length > 0

  return {
    history: nextHistory,
    loaded,
    shouldPersist: shouldPersist && nextHistory.length > 0,
    usedLegacy,
    warnings,
  }
}
