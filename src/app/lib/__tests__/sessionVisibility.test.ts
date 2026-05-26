import { describe, it, expect } from 'vitest'
import type { HistoryItem } from '../../../types'
import {
  filterStandaloneHistory,
  isWorkspaceHistoryItem,
  resolveStandaloneActiveHistoryId,
  resolveStandaloneActiveHistoryItem,
} from '../sessionVisibility'

function makeHistoryItem(id: string, workspaceId?: string | null): HistoryItem {
  return {
    id,
    title: id,
    status: 'done',
    createdAt: 1,
    updatedAt: 1,
    turns: [],
    workspaceId: workspaceId ?? undefined,
  }
}

describe('sessionVisibility', () => {
  it('detects workspace sessions by workspaceId', () => {
    expect(isWorkspaceHistoryItem(makeHistoryItem('global'))).toBe(false)
    expect(isWorkspaceHistoryItem(makeHistoryItem('workspace', 'ws-1'))).toBe(true)
    expect(isWorkspaceHistoryItem(makeHistoryItem('blank', '   '))).toBe(false)
    expect(isWorkspaceHistoryItem(null)).toBe(false)
  })

  it('keeps workspace sessions visible in global history', () => {
    const items = [
      makeHistoryItem('chat-1'),
      makeHistoryItem('workspace-1', 'ws-1'),
      makeHistoryItem('chat-2'),
    ]

    expect(filterStandaloneHistory(items).map((item) => item.id)).toEqual(['chat-1', 'workspace-1', 'chat-2'])
  })

  it('keeps workspace session in standalone active selection', () => {
    const workspaceItem = makeHistoryItem('workspace-1', 'ws-1')
    const standaloneItem = makeHistoryItem('chat-1')

    expect(resolveStandaloneActiveHistoryItem(workspaceItem)).toBe(workspaceItem)
    expect(resolveStandaloneActiveHistoryId(workspaceItem, 'workspace-1')).toBe('workspace-1')

    expect(resolveStandaloneActiveHistoryItem(standaloneItem)).toBe(standaloneItem)
    expect(resolveStandaloneActiveHistoryId(standaloneItem, 'chat-1')).toBe('chat-1')
    expect(resolveStandaloneActiveHistoryId(standaloneItem, '   ')).toBe('')
  })
})
