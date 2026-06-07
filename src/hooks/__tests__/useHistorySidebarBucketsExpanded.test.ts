import { describe, expect, it } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import type { HistorySidebarBucket } from '../../lib/historySidebarBuckets'
import { useHistorySidebarBucketsExpanded } from '../useHistorySidebarBucketsExpanded'
import type { HistoryItem } from '../../types'

function makeItem(id: string, updatedAt: number): HistoryItem {
  return {
    id,
    title: '',
    status: 'done',
    createdAt: updatedAt,
    updatedAt,
    turns: [],
  }
}

describe('useHistorySidebarBucketsExpanded', () => {
  it('defaults open for today bucket and expands bucket that contains selected session', () => {
    const buckets: HistorySidebarBucket<HistoryItem>[] = [
      {
        key: 'today',
        label: '今天',
        items: [makeItem('a', 3)],
      },
      {
        key: 'week',
        label: '过去 7 天',
        items: [makeItem('w', 1)],
      },
    ]

    const { result, rerender } = renderHook(
      ({
        buckets: nextBuckets,
        activeId,
      }: {
        buckets: HistorySidebarBucket<HistoryItem>[]
        activeId: string | null
      }) => useHistorySidebarBucketsExpanded(nextBuckets, activeId),
      {
        initialProps: { buckets, activeId: null as string | null },
      },
    )

    expect(result.current.mergedBucketOpen.today).toBe(true)
    expect(result.current.mergedBucketOpen.week).toBe(false)

    rerender({ buckets, activeId: 'w' })
    expect(result.current.mergedBucketOpen.week).toBe(true)

    rerender({ buckets, activeId: 'a' })
    expect(result.current.mergedBucketOpen.today).toBe(true)
    expect(result.current.mergedBucketOpen.week).toBe(false)
  })

  it('toggleBucket persists user expand/collapse', () => {
    const buckets: HistorySidebarBucket<HistoryItem>[] = [
      {
        key: 'today',
        label: '今天',
        items: [makeItem('a', 1)],
      },
    ]

    const { result } = renderHook(() => useHistorySidebarBucketsExpanded(buckets, null))

    expect(result.current.mergedBucketOpen.today).toBe(true)
    act(() => result.current.toggleBucket('today'))
    expect(result.current.mergedBucketOpen.today).toBe(false)
    act(() => result.current.toggleBucket('today'))
    expect(result.current.mergedBucketOpen.today).toBe(true)
  })
})
