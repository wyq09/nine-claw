import type { ConversationAgentSnapshot, HistoryItem, HistoryStatus } from '../types'
import type { HistorySidebarGroup } from './historySidebarMeta'

export type HistorySidebarItem = {
  id: string
  title: string
  status: HistoryStatus
  createdAt: number
  updatedAt: number
  agent?: ConversationAgentSnapshot
  pinned?: boolean
  groupId?: string | null
  searchText: string
}

export type HistorySidebarBucket<TItem extends HistorySidebarBucketItem = HistorySidebarItem> = {
  key: string
  label: string
  items: TItem[]
  kind?: 'pinned' | 'group' | 'time'
  groupId?: string
}

export type HistorySidebarBucketItem = {
  id: string
  createdAt: number
  updatedAt: number
  pinned?: boolean
  groupId?: string | null
}

const WEEKDAY_ZH = ['日', '一', '二', '三', '四', '五', '六'] as const

/** `05/13(三) 11:48` — 与侧栏历史会话参考样式一致 */
export function formatHistorySessionListTime(ts: number): string {
  const d = new Date(ts)
  const mm = String(d.getMonth() + 1).padStart(2, '0')
  const dd = String(d.getDate()).padStart(2, '0')
  const wd = WEEKDAY_ZH[d.getDay()] ?? '日'
  const hh = String(d.getHours()).padStart(2, '0')
  const min = String(d.getMinutes()).padStart(2, '0')
  return `${mm}/${dd}(${wd}) ${hh}:${min}`
}

function sortByRecent<TItem extends HistorySidebarBucketItem>(items: TItem[]): TItem[] {
  return [...items].sort((a, b) => {
    const tb = b.updatedAt || b.createdAt
    const ta = a.updatedAt || a.createdAt
    return tb - ta
  })
}

/**
 * 今天 / 昨天 / 过去 7 天（不含今、昨）/ 更早。空桶不返回。
 */
export function groupHistoryIntoSidebarBuckets(
  items: HistorySidebarItem[],
  nowMs?: number,
  groups?: HistorySidebarGroup[],
): HistorySidebarBucket<HistorySidebarItem>[]
export function groupHistoryIntoSidebarBuckets<TItem extends HistorySidebarBucketItem>(
  items: TItem[],
  nowMs?: number,
  groups?: HistorySidebarGroup[],
): HistorySidebarBucket<TItem>[]
export function groupHistoryIntoSidebarBuckets<TItem extends HistorySidebarBucketItem>(
  items: TItem[],
  nowMs: number = Date.now(),
  groups: HistorySidebarGroup[] = [],
): HistorySidebarBucket<TItem>[] {
  const pinnedItems = sortByRecent(items.filter((item) => item.pinned))
  const remainingItems = items.filter((item) => !item.pinned)
  const groupedItemIds = new Set<string>()
  const groupedBuckets: HistorySidebarBucket<TItem>[] = []

  for (const group of groups) {
    const groupItems = sortByRecent(remainingItems.filter((item) => item.groupId === group.id))
    if (groupItems.length === 0) {
      continue
    }
    groupItems.forEach((item) => groupedItemIds.add(item.id))
    groupedBuckets.push({
      key: `group:${group.id}`,
      label: group.name,
      items: groupItems,
      kind: 'group',
      groupId: group.id,
    })
  }

  const timeItems = remainingItems.filter((item) => !groupedItemIds.has(item.id))
  const now = new Date(nowMs)
  const startOfDay = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  const startOfYesterday = startOfDay - 24 * 3600 * 1000
  const startOfRollingWeek = startOfDay - 7 * 24 * 3600 * 1000

  const today: TItem[] = []
  const yesterday: TItem[] = []
  const week: TItem[] = []
  const earlier: TItem[] = []

  for (const item of timeItems) {
    const ts = item.updatedAt || item.createdAt
    if (ts >= startOfDay) today.push(item)
    else if (ts >= startOfYesterday) yesterday.push(item)
    else if (ts >= startOfRollingWeek) week.push(item)
    else earlier.push(item)
  }

  const out: HistorySidebarBucket<TItem>[] = []
  if (pinnedItems.length) out.push({ key: 'pinned', label: '置顶', items: pinnedItems, kind: 'pinned' })
  out.push(...groupedBuckets)
  if (today.length) out.push({ key: 'today', label: '今天', items: sortByRecent(today), kind: 'time' })
  if (yesterday.length) out.push({ key: 'yesterday', label: '昨天', items: sortByRecent(yesterday), kind: 'time' })
  if (week.length) out.push({ key: 'week', label: '过去 7 天', items: sortByRecent(week), kind: 'time' })
  if (earlier.length) out.push({ key: 'earlier', label: '更早', items: sortByRecent(earlier), kind: 'time' })
  return out
}

export function toHistorySidebarItem(item: HistoryItem): HistorySidebarItem {
  return {
    id: item.id,
    title: item.title,
    status: item.status,
    createdAt: item.createdAt,
    updatedAt: item.updatedAt,
    agent: item.agent,
    pinned: item.pinned,
    groupId: item.groupId ?? null,
    searchText: [
      item.title,
      item.agent?.name,
      ...item.turns.flatMap((turn) => [turn.prompt, turn.answer]),
    ]
      .filter((part): part is string => Boolean(part))
      .join(' ')
      .toLowerCase(),
  }
}
