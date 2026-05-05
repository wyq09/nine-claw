/** 与 `commands_workspace_kv_memory.rs` 中常量一致（勿改前缀以免与库内数据脱节）。 */
export const USER_MEMORY_GLOBAL_WORKSPACE_ID = '__nc_user_kv_global__'

/** 下拉第一项「全局」，与任一真实 agent.id 区分开。 */
export const USER_MEMORY_DROPDOWN_GLOBAL = 'global'

/** 与 memory_store 键前缀一致，便于与对话内记忆库工具对齐。 */
export const USER_MEMORY_KEY_PREFIX = 'nc_um.'

export type UserMemoryTabKey = 'identity' | 'work' | 'writing' | 'directive' | 'other'

export type UserMemoryTabDef = {
  key: Exclude<UserMemoryTabKey, 'other'>
  label: string
}

export const USER_MEMORY_TABS: UserMemoryTabDef[] = [
  { key: 'identity', label: '身份记忆' },
  { key: 'work', label: '工作方式' },
  { key: 'writing', label: '写作风格' },
  { key: 'directive', label: '用户指令' },
]

export function tabKeyFromKvKey(kvKey: string): UserMemoryTabKey {
  if (kvKey.startsWith(`${USER_MEMORY_KEY_PREFIX}identity/`)) return 'identity'
  if (kvKey.startsWith(`${USER_MEMORY_KEY_PREFIX}work/`)) return 'work'
  if (kvKey.startsWith(`${USER_MEMORY_KEY_PREFIX}writing/`)) return 'writing'
  if (kvKey.startsWith(`${USER_MEMORY_KEY_PREFIX}directive/`)) return 'directive'
  return 'other'
}

export function buildUserManualMemoryKey(kind: Exclude<UserMemoryTabKey, 'other'>): string {
  const id =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(16).slice(2)}`
  return `${USER_MEMORY_KEY_PREFIX}${kind}/${id}`
}

/** 把存库的 value 编成给人看的单行/短文本，尽量不出现裸 JSON。 */
export function formatKvMemoryBody(value: unknown): string {
  if (value === null || value === undefined) return ''
  if (typeof value === 'string') return value
  if (typeof value === 'number' || typeof value === 'boolean') return String(value)
  if (Array.isArray(value)) {
    const parts = value.map((item) => formatKvMemoryBody(item)).filter((s) => s.length > 0)
    return parts.length ? parts.join(' · ') : ''
  }
  if (typeof value !== 'object') return String(value)

  const o = value as Record<string, unknown>
  for (const k of ['text', 'summary', 'content', 'body', 'note', 'memory', 'fact'] as const) {
    const raw = o[k]
    if (typeof raw === 'string' && raw.trim()) return raw
  }

  const skipMeta = new Set(['source', 'via', 'kind', 'updatedAt', 'createdAt'])
  const primitives = Object.entries(o)
    .filter(([k, v]) => !skipMeta.has(k) && v !== null && v !== undefined && typeof v !== 'object')
    .map(([k, v]) => `${k}：${String(v)}`)
  if (primitives.length) return primitives.join(' · ')

  const nestedBrief = Object.entries(o)
    .filter(([k]) => !skipMeta.has(k))
    .slice(0, 4)
    .map(([k, v]) => {
      if (v === null || v === undefined) return ''
      if (typeof v === 'string') return `${k}：${v}`
      if (typeof v === 'number' || typeof v === 'boolean') return `${k}：${String(v)}`
      return `${k}：(子项)`
    })
    .filter(Boolean)
    .join(' · ')
  if (nestedBrief) return nestedBrief

  return '（结构化条目，可点编辑改写为一段话后保存）'
}

function textCarrierKeys(value: unknown): boolean {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false
  const o = value as Record<string, unknown>
  const substantive = Object.keys(o).filter((key) => !['source', 'via', 'kind', 'updatedAt', 'createdAt'].includes(key))
  return (
    substantive.length === 1 &&
    substantive[0] === 'text' &&
    typeof o.text === 'string'
  )
}

export type MemoryRowTagVariant = 'migrated' | 'auto' | 'generic'

export function memoryRowTag(
  kvKey: string,
  value: unknown,
  originKind?: string | null,
): { label: string; variant: MemoryRowTagVariant } {
  if (originKind === 'manual') {
    return { label: '手动', variant: 'generic' }
  }
  if (originKind === 'auto') {
    return { label: '自动', variant: 'auto' }
  }
  if (originKind === 'migration') {
    return { label: '迁移', variant: 'migrated' }
  }
  const structuredButNotPlain = typeof value === 'object' && value !== null && !Array.isArray(value) && !textCarrierKeys(value)
  if (Array.isArray(value) || structuredButNotPlain) {
    return { label: '自动', variant: 'auto' }
  }
  if (kvKey.startsWith(`${USER_MEMORY_KEY_PREFIX}`)) {
    return { label: '迁移', variant: 'migrated' }
  }
  return { label: '记忆库', variant: 'generic' }
}
