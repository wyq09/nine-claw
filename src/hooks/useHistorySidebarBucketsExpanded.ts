import { useCallback, useMemo, useState } from 'react'
import type { HistorySidebarBucket } from '../lib/historySidebarBuckets'

export function useHistorySidebarBucketsExpanded(
  historyBuckets: HistorySidebarBucket[],
  activeHistoryId: string | null,
): {
  mergedBucketOpen: Record<string, boolean>
  toggleBucket: (key: string) => void
} {
  const [bucketExpandedOverrides, setBucketExpandedOverrides] = useState<Partial<Record<string, boolean>>>(
    {},
  )

  const mergedBucketOpen = useMemo(() => {
    const activeBucketKey = activeHistoryId
      ? historyBuckets.find((bucket) => bucket.items.some((item) => item.id === activeHistoryId))?.key
      : undefined

    const out: Record<string, boolean> = {}
    for (const bucket of historyBuckets) {
      const inferred = bucket.key === 'today' || bucket.key === activeBucketKey
      const exp = bucketExpandedOverrides[bucket.key]
      out[bucket.key] = exp !== undefined ? exp : inferred
    }
    return out
  }, [activeHistoryId, bucketExpandedOverrides, historyBuckets])

  const toggleBucket = useCallback(
    (key: string) => {
      setBucketExpandedOverrides((prevOverrides) => {
        const activeBucketKey = activeHistoryId
          ? historyBuckets.find((bucket) =>
              bucket.items.some((item) => item.id === activeHistoryId),
            )?.key
          : undefined
        const inferred = key === 'today' || key === activeBucketKey
        const currentlyOpen =
          prevOverrides[key] !== undefined ? Boolean(prevOverrides[key]) : inferred
        return {
          ...prevOverrides,
          [key]: !currentlyOpen,
        }
      })
    },
    [activeHistoryId, historyBuckets],
  )

  return { mergedBucketOpen, toggleBucket }
}
