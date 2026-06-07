import { useMemo, useRef } from 'react'
import { toHistorySidebarItem, type HistorySidebarItem } from '../lib/historySidebarBuckets'
import type { HistoryItem } from '../types'

export function useHistorySidebarItems(history: HistoryItem[]): HistorySidebarItem[] {
  const cacheRef = useRef<Map<string, HistorySidebarItem>>(new Map())

  return useMemo(() => {
    const cache = cacheRef.current
    const nextIds = new Set<string>()
    const nextItems = history.map((item) => {
      nextIds.add(item.id)
      const next = toHistorySidebarItem(item)
      const cached = cache.get(item.id)
      if (
        cached &&
        cached.title === next.title &&
        cached.status === next.status &&
        cached.createdAt === next.createdAt &&
        cached.updatedAt === next.updatedAt &&
        cached.agent === next.agent &&
        cached.pinned === next.pinned &&
        cached.groupId === next.groupId &&
        cached.searchText === next.searchText
      ) {
        return cached
      }
      cache.set(item.id, next)
      return next
    })
    for (const id of cache.keys()) {
      if (!nextIds.has(id)) {
        cache.delete(id)
      }
    }
    return nextItems
  }, [history])
}
