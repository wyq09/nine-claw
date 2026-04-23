import type { HistoryItem } from '../../types'

export function isWorkspaceHistoryItem(item: Pick<HistoryItem, 'workspaceId'> | null | undefined): boolean {
  return Boolean(item?.workspaceId?.trim())
}

export function filterStandaloneHistory(items: HistoryItem[]): HistoryItem[] {
  return items.filter((item) => !isWorkspaceHistoryItem(item))
}

export function resolveStandaloneActiveHistoryItem(activeItem: HistoryItem | null): HistoryItem | null {
  return isWorkspaceHistoryItem(activeItem) ? null : activeItem
}

export function resolveStandaloneActiveHistoryId(activeItem: HistoryItem | null, activeId: string | null): string {
  const trimmedId = activeId?.trim() ?? ''
  if (!trimmedId || isWorkspaceHistoryItem(activeItem)) {
    return ''
  }
  return trimmedId
}
