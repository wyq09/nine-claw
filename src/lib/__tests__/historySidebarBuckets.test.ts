import { describe, expect, it } from 'vitest'
import {
  formatHistorySessionListTime,
  groupHistoryIntoSidebarBuckets,
} from '../historySidebarBuckets'
import type { HistoryItem } from '../../types'

function makeItem(partial: Partial<HistoryItem> & Pick<HistoryItem, 'id' | 'updatedAt'>): HistoryItem {
  return {
    title: 't',
    status: 'done',
    createdAt: partial.updatedAt,
    turns: [],
    ...partial,
  }
}

describe('formatHistorySessionListTime', () => {
  it('formats with weekday label and zero-padded parts', () => {
    const ts = new Date(2026, 4, 13, 11, 8, 0).getTime()
    expect(formatHistorySessionListTime(ts)).toBe('05/13(三) 11:08')
  })
})

describe('groupHistoryIntoSidebarBuckets', () => {
  it('places items into today / yesterday / week / earlier', () => {
    const anchor = new Date(2026, 4, 13, 12, 0, 0).getTime()
    const startOfDay = new Date(2026, 4, 13, 0, 0, 0).getTime()

    const todayId = makeItem({ id: 'a', updatedAt: startOfDay + 3600_000 })
    const yesterdayId = makeItem({ id: 'b', updatedAt: startOfDay - 3600_000 })
    const weekId = makeItem({ id: 'c', updatedAt: startOfDay - 3 * 24 * 3600_000 })
    const earlierId = makeItem({ id: 'd', updatedAt: startOfDay - 10 * 24 * 3600_000 })

    const groups = groupHistoryIntoSidebarBuckets([earlierId, weekId, yesterdayId, todayId], anchor)
    expect(groups.map((g) => g.key)).toEqual(['today', 'yesterday', 'week', 'earlier'])
    expect(groups[0]?.items.map((i) => i.id)).toEqual(['a'])
    expect(groups[1]?.items.map((i) => i.id)).toEqual(['b'])
    expect(groups[2]?.items.map((i) => i.id)).toEqual(['c'])
    expect(groups[3]?.items.map((i) => i.id)).toEqual(['d'])
  })

  it('sorts within bucket by updatedAt descending', () => {
    const anchor = new Date(2026, 4, 13, 12, 0, 0).getTime()
    const startOfDay = new Date(2026, 4, 13, 0, 0, 0).getTime()
    const older = makeItem({ id: 'old', updatedAt: startOfDay + 1000 })
    const newer = makeItem({ id: 'new', updatedAt: startOfDay + 5000 })
    const groups = groupHistoryIntoSidebarBuckets([older, newer], anchor)
    expect(groups[0]?.items.map((i) => i.id)).toEqual(['new', 'old'])
  })

  it('omits empty buckets', () => {
    const anchor = new Date(2026, 4, 13, 12, 0, 0).getTime()
    const startOfDay = new Date(2026, 4, 13, 0, 0, 0).getTime()
    const onlyToday = makeItem({ id: 'x', updatedAt: startOfDay + 1 })
    const groups = groupHistoryIntoSidebarBuckets([onlyToday], anchor)
    expect(groups.map((g) => g.key)).toEqual(['today'])
  })
})
