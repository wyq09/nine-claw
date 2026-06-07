import { afterEach, describe, expect, it, vi } from 'vitest'
import type { HistorySidebarItem } from '../historySidebarBuckets'
import {
  applyHistorySidebarMeta,
  assignHistoryItemToGroup,
  createEmptyHistorySidebarMeta,
  createHistorySidebarGroup,
  dissolveHistorySidebarGroup,
  normalizeHistorySidebarMeta,
  renameHistorySidebarGroup,
  setHistoryItemPinned,
} from '../historySidebarMeta'

function makeSidebarItem(id: string): HistorySidebarItem {
  return {
    id,
    title: `会话 ${id}`,
    status: 'done',
    createdAt: 1,
    updatedAt: 2,
    searchText: id,
  }
}

describe('historySidebarMeta', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('normalizes invalid stored payloads to a safe empty state', () => {
    expect(normalizeHistorySidebarMeta(null)).toEqual(createEmptyHistorySidebarMeta())
    expect(normalizeHistorySidebarMeta({ items: { a: { pinned: true, groupId: 'missing' } } })).toEqual({
      version: 1,
      items: { a: { pinned: true, groupId: null } },
      groups: [],
    })
  })

  it('applies pinned and group metadata to sidebar items', () => {
    const base = createEmptyHistorySidebarMeta()
    const withPinned = setHistoryItemPinned(base, 'a', true)
    const { meta, group } = createHistorySidebarGroup(withPinned, '项目组', 10)
    const assigned = assignHistoryItemToGroup(meta, 'b', group.id)

    expect(applyHistorySidebarMeta([makeSidebarItem('a'), makeSidebarItem('b')], assigned)).toEqual([
      expect.objectContaining({ id: 'a', pinned: true, groupId: null }),
      expect.objectContaining({ id: 'b', pinned: false, groupId: group.id }),
    ])
  })

  it('renames and dissolves groups without removing conversations', () => {
    vi.spyOn(Date, 'now').mockReturnValue(20)
    const { meta, group } = createHistorySidebarGroup(createEmptyHistorySidebarMeta(), '旧名称', 10)
    const assigned = assignHistoryItemToGroup(meta, 'a', group.id)
    const renamed = renameHistorySidebarGroup(assigned, group.id, '新名称')
    const dissolved = dissolveHistorySidebarGroup(renamed, group.id)

    expect(renamed.groups[0]).toMatchObject({ name: '新名称', updatedAt: 20 })
    expect(dissolved.groups).toEqual([])
    expect(dissolved.items).toEqual({})
  })
})
