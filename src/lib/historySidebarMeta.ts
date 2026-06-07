import type { HistorySidebarItem } from './historySidebarBuckets'

export const HISTORY_SIDEBAR_META_STORAGE_KEY = 'nineclaw_history_sidebar_meta_v1'

export type HistorySidebarGroup = {
  id: string
  name: string
  createdAt: number
  updatedAt: number
}

export type HistorySidebarItemMeta = {
  pinned?: boolean
  groupId?: string | null
}

export type HistorySidebarMetaState = {
  version: 1
  items: Record<string, HistorySidebarItemMeta>
  groups: HistorySidebarGroup[]
}

export function createEmptyHistorySidebarMeta(): HistorySidebarMetaState {
  return {
    version: 1,
    items: {},
    groups: [],
  }
}

function createId(prefix: string): string {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

export function normalizeHistorySidebarMeta(value: unknown): HistorySidebarMetaState {
  if (!isObject(value)) {
    return createEmptyHistorySidebarMeta()
  }

  const rawItems = isObject(value.items) ? value.items : {}
  const items: Record<string, HistorySidebarItemMeta> = {}
  for (const [sessionId, rawMeta] of Object.entries(rawItems)) {
    if (!sessionId.trim() || !isObject(rawMeta)) {
      continue
    }
    const pinned = rawMeta.pinned === true
    const groupId = typeof rawMeta.groupId === 'string' && rawMeta.groupId.trim() ? rawMeta.groupId.trim() : null
    if (pinned || groupId) {
      items[sessionId] = { pinned, groupId }
    }
  }

  const groups = Array.isArray(value.groups)
    ? value.groups
        .filter(isObject)
        .map((group): HistorySidebarGroup | null => {
          const id = typeof group.id === 'string' ? group.id.trim() : ''
          const name = typeof group.name === 'string' ? group.name.trim() : ''
          if (!id || !name) {
            return null
          }
          return {
            id,
            name,
            createdAt: typeof group.createdAt === 'number' ? group.createdAt : Date.now(),
            updatedAt: typeof group.updatedAt === 'number' ? group.updatedAt : Date.now(),
          }
        })
        .filter((group): group is HistorySidebarGroup => Boolean(group))
    : []

  const validGroupIds = new Set(groups.map((group) => group.id))
  for (const [sessionId, item] of Object.entries(items)) {
    if (item.groupId && !validGroupIds.has(item.groupId)) {
      items[sessionId] = { ...item, groupId: null }
    }
  }

  return { version: 1, items, groups }
}

export function loadHistorySidebarMeta(storage: Storage | null = typeof window !== 'undefined' ? window.localStorage : null): HistorySidebarMetaState {
  if (!storage) {
    return createEmptyHistorySidebarMeta()
  }
  try {
    return normalizeHistorySidebarMeta(JSON.parse(storage.getItem(HISTORY_SIDEBAR_META_STORAGE_KEY) || 'null'))
  } catch {
    return createEmptyHistorySidebarMeta()
  }
}

export function saveHistorySidebarMeta(meta: HistorySidebarMetaState, storage: Storage | null = typeof window !== 'undefined' ? window.localStorage : null): void {
  if (!storage) {
    return
  }
  storage.setItem(HISTORY_SIDEBAR_META_STORAGE_KEY, JSON.stringify(normalizeHistorySidebarMeta(meta)))
}

export function applyHistorySidebarMeta(
  items: HistorySidebarItem[],
  meta: HistorySidebarMetaState,
): HistorySidebarItem[] {
  return items.map((item) => {
    const itemMeta = meta.items[item.id]
    if (!itemMeta?.pinned && !itemMeta?.groupId) {
      return item
    }
    return {
      ...item,
      pinned: itemMeta.pinned === true,
      groupId: itemMeta.groupId ?? null,
    }
  })
}

export function pruneHistorySidebarMeta(
  meta: HistorySidebarMetaState,
  sessionIds: Iterable<string>,
): HistorySidebarMetaState {
  const validIds = new Set(sessionIds)
  const items: Record<string, HistorySidebarItemMeta> = {}
  for (const [sessionId, item] of Object.entries(meta.items)) {
    if (validIds.has(sessionId)) {
      items[sessionId] = item
    }
  }
  return { ...meta, items }
}

function updateItemMeta(
  meta: HistorySidebarMetaState,
  sessionId: string,
  updater: (current: HistorySidebarItemMeta) => HistorySidebarItemMeta,
): HistorySidebarMetaState {
  const current = meta.items[sessionId] ?? {}
  const next = updater(current)
  const items = { ...meta.items }
  if (next.pinned || next.groupId) {
    items[sessionId] = next
  } else {
    delete items[sessionId]
  }
  return { ...meta, items }
}

export function setHistoryItemPinned(
  meta: HistorySidebarMetaState,
  sessionId: string,
  pinned: boolean,
): HistorySidebarMetaState {
  return updateItemMeta(meta, sessionId, (current) => ({ ...current, pinned }))
}

export function assignHistoryItemToGroup(
  meta: HistorySidebarMetaState,
  sessionId: string,
  groupId: string | null,
): HistorySidebarMetaState {
  const normalizedGroupId = groupId?.trim() || null
  return updateItemMeta(meta, sessionId, (current) => ({ ...current, groupId: normalizedGroupId }))
}

export function createHistorySidebarGroup(
  meta: HistorySidebarMetaState,
  name: string,
  now: number = Date.now(),
): { meta: HistorySidebarMetaState; group: HistorySidebarGroup } {
  const group: HistorySidebarGroup = {
    id: createId('history_group'),
    name: name.trim() || '新分组',
    createdAt: now,
    updatedAt: now,
  }
  return {
    meta: {
      ...meta,
      groups: [...meta.groups, group],
    },
    group,
  }
}

export function renameHistorySidebarGroup(
  meta: HistorySidebarMetaState,
  groupId: string,
  name: string,
  now: number = Date.now(),
): HistorySidebarMetaState {
  const nextName = name.trim()
  if (!nextName) {
    return meta
  }
  return {
    ...meta,
    groups: meta.groups.map((group) =>
      group.id === groupId ? { ...group, name: nextName, updatedAt: now } : group,
    ),
  }
}

export function dissolveHistorySidebarGroup(
  meta: HistorySidebarMetaState,
  groupId: string,
): HistorySidebarMetaState {
  const groups = meta.groups.filter((group) => group.id !== groupId)
  const items: Record<string, HistorySidebarItemMeta> = {}
  for (const [sessionId, item] of Object.entries(meta.items)) {
    const nextGroupId = item.groupId === groupId ? null : item.groupId
    if (item.pinned || nextGroupId) {
      items[sessionId] = { ...item, groupId: nextGroupId }
    }
  }
  return { ...meta, groups, items }
}

export function deriveHistorySidebarGroupName(items: HistorySidebarItem[]): string {
  const agentName = items.find((item) => item.agent?.name)?.agent?.name?.trim()
  if (agentName) {
    return `${agentName} 相关`
  }
  const firstTitle = items.find((item) => item.title.trim())?.title.trim()
  return firstTitle ? firstTitle.slice(0, 12) : '会话分组'
}
