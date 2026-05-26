import type { HistoryItem } from '../types'

export type HistorySidebarBucket = {
  key: 'today' | 'yesterday' | 'week' | 'earlier'
  label: string
  items: HistoryItem[]
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

function sortByRecent(items: HistoryItem[]): HistoryItem[] {
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
  items: HistoryItem[],
  nowMs: number = Date.now(),
): HistorySidebarBucket[] {
  const now = new Date(nowMs)
  const startOfDay = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  const startOfYesterday = startOfDay - 24 * 3600 * 1000
  const startOfRollingWeek = startOfDay - 7 * 24 * 3600 * 1000

  const today: HistoryItem[] = []
  const yesterday: HistoryItem[] = []
  const week: HistoryItem[] = []
  const earlier: HistoryItem[] = []

  for (const item of items) {
    const ts = item.updatedAt || item.createdAt
    if (ts >= startOfDay) today.push(item)
    else if (ts >= startOfYesterday) yesterday.push(item)
    else if (ts >= startOfRollingWeek) week.push(item)
    else earlier.push(item)
  }

  const out: HistorySidebarBucket[] = []
  if (today.length) out.push({ key: 'today', label: '今天', items: sortByRecent(today) })
  if (yesterday.length) out.push({ key: 'yesterday', label: '昨天', items: sortByRecent(yesterday) })
  if (week.length) out.push({ key: 'week', label: '过去 7 天', items: sortByRecent(week) })
  if (earlier.length) out.push({ key: 'earlier', label: '更早', items: sortByRecent(earlier) })
  return out
}
