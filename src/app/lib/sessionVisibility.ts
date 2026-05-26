import type { HistoryItem } from '../../types'

export function isWorkspaceHistoryItem(item: Pick<HistoryItem, 'workspaceId'> | null | undefined): boolean {
  return Boolean(item?.workspaceId?.trim())
}

export function filterStandaloneHistory(items: HistoryItem[]): HistoryItem[] {
  return items
}

export function resolveStandaloneActiveHistoryItem(activeItem: HistoryItem | null): HistoryItem | null {
  return activeItem
}

export function resolveStandaloneActiveHistoryId(_activeItem: HistoryItem | null, activeId: string | null): string {
  const trimmedId = activeId?.trim() ?? ''
  if (!trimmedId) {
    return ''
  }
  return trimmedId
}
