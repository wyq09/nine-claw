import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { AgentRecord } from '../../types'
import {
  workspaceKvMemoryUiForget,
  workspaceKvMemoryUiList,
  workspaceKvMemoryUiReorganize,
  workspaceKvMemoryUiStore,
  type WorkspaceKvMemoryUiEntry,
} from '../../lib/workspaceKvMemoryClient'
import {
  USER_MEMORY_TABS,
  USER_MEMORY_GLOBAL_WORKSPACE_ID,
  USER_MEMORY_DROPDOWN_GLOBAL,
  tabKeyFromKvKey,
  buildUserManualMemoryKey,
  formatKvMemoryBody,
  memoryRowTag,
  type MemoryRowTagVariant,
  type UserMemoryTabKey,
} from './userMemoryKinds'

type UserMemorySettingsPanelProps = {
  agents: AgentRecord[]
  defaultAgentId: string
}

function IconPencil() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.85" aria-hidden>
      <path d="M12 20h9" strokeLinecap="round" />
      <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4 11.5-11.5Z" strokeLinejoin="round" />
    </svg>
  )
}

function IconTrash() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.85" aria-hidden>
      <path d="M4 7h16" strokeLinecap="round" />
      <path d="M10 11v7M14 11v7" strokeLinecap="round" />
      <path d="M6 7l1 14h10l1-14" strokeLinejoin="round" />
      <path d="M9 7V5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2" strokeLinejoin="round" />
    </svg>
  )
}

function IconCheck() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.1" aria-hidden>
      <path d="M5 13l5 5L21 7" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function tagToneClassVariant(variant: MemoryRowTagVariant | 'edit'): string {
  if (variant === 'edit' || variant === 'auto') return 'user-memory-tag-toned accent'
  if (variant === 'migrated') return 'user-memory-tag-toned muted'
  return 'user-memory-tag-toned plain'
}

export function UserMemorySettingsPanel({ agents, defaultAgentId }: UserMemorySettingsPanelProps) {
  void defaultAgentId // 与其它设置入口 props 对齐；默认列表首项仍为「全局」
  const uniqueAgents = useMemo(() => {
    const byName = new Map<string, AgentRecord>()
    for (const agent of agents) {
      const id = agent.id?.trim()
      const name = agent.name?.trim()
      if (!id || !name) continue
      if (!byName.has(name)) byName.set(name, agent)
    }
    return Array.from(byName.values())
  }, [agents])

  const [selection, setSelection] = useState<string>(() => USER_MEMORY_DROPDOWN_GLOBAL)

  const [activeTab, setActiveTab] = useState<UserMemoryTabKey>('identity')
  const [entries, setEntries] = useState<WorkspaceKvMemoryUiEntry[]>([])
  const [workspaceIdResolved, setWorkspaceIdResolved] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [draftText, setDraftText] = useState('')
  const [editingKey, setEditingKey] = useState<string | null>(null)
  const [editDraft, setEditDraft] = useState('')
  const editInputRef = useRef<HTMLInputElement | null>(null)

  const isGlobalSelection = selection === USER_MEMORY_DROPDOWN_GLOBAL
  const selectedAgentName =
    selection === USER_MEMORY_DROPDOWN_GLOBAL
      ? null
      : uniqueAgents.find((a) => a.id === selection)?.name ?? null

  const kvForget = useCallback(
    async (key: string) => {
      if (isGlobalSelection) {
        await workspaceKvMemoryUiForget({ workspaceId: USER_MEMORY_GLOBAL_WORKSPACE_ID, key })
      } else {
        await workspaceKvMemoryUiForget({ agentId: selection.trim(), key })
      }
    },
    [isGlobalSelection, selection],
  )

  const kvStore = useCallback(
    async (key: string, value: unknown) => {
      if (isGlobalSelection) {
        await workspaceKvMemoryUiStore({ workspaceId: USER_MEMORY_GLOBAL_WORKSPACE_ID, key, value })
      } else {
        await workspaceKvMemoryUiStore({ agentId: selection.trim(), key, value })
      }
    },
    [isGlobalSelection, selection],
  )

  const refresh = useCallback(async () => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const payload = isGlobalSelection
        ? await workspaceKvMemoryUiList({ workspaceId: USER_MEMORY_GLOBAL_WORKSPACE_ID, limit: 500 })
        : await workspaceKvMemoryUiList({ agentId: selection.trim(), limit: 500 })
      setWorkspaceIdResolved(payload.workspaceId)
      setEntries(Array.isArray(payload.entries) ? payload.entries : [])
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
      setEntries([])
    } finally {
      setBusy(false)
    }
  }, [isGlobalSelection, selection])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const reorganizeThenRefresh = useCallback(async () => {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      const res = await workspaceKvMemoryUiReorganize(
        isGlobalSelection
          ? { workspaceId: USER_MEMORY_GLOBAL_WORKSPACE_ID }
          : { agentId: selection.trim() },
      )
      const payload = isGlobalSelection
        ? await workspaceKvMemoryUiList({ workspaceId: USER_MEMORY_GLOBAL_WORKSPACE_ID, limit: 500 })
        : await workspaceKvMemoryUiList({ agentId: selection.trim(), limit: 500 })
      setWorkspaceIdResolved(payload.workspaceId)
      setEntries(Array.isArray(payload.entries) ? payload.entries : [])
      const backendMsg =
        typeof res.message === 'string' && res.message.trim().length ? res.message.trim() : ''
      setNotice(backendMsg || '整理完成并已刷新列表（重复项不会在列表中再次出现）。')
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
      setEntries([])
    } finally {
      setBusy(false)
    }
  }, [isGlobalSelection, selection])

  useEffect(() => {
    setEditingKey(null)
    setEditDraft('')
  }, [selection])

  useEffect(() => {
    if (selection === USER_MEMORY_DROPDOWN_GLOBAL) return
    if (!uniqueAgents.some((a) => a.id === selection)) {
      setSelection(USER_MEMORY_DROPDOWN_GLOBAL)
    }
  }, [uniqueAgents, selection])

  useEffect(() => {
    if (!editingKey || !editInputRef.current) return
    editInputRef.current.focus()
    editInputRef.current.select()
  }, [editingKey])

  useEffect(() => {
    if (!editingKey) return
    const esc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setEditingKey(null)
        setEditDraft('')
      }
    }
    window.addEventListener('keydown', esc)
    return () => window.removeEventListener('keydown', esc)
  }, [editingKey])

  useEffect(() => {
    setEditingKey(null)
    setEditDraft('')
  }, [activeTab])

  const buckets = useMemo(() => {
    const next: Record<UserMemoryTabKey, WorkspaceKvMemoryUiEntry[]> = {
      identity: [],
      work: [],
      writing: [],
      directive: [],
      other: [],
    }
    for (const entry of entries) {
      next[tabKeyFromKvKey(entry.key)].push(entry)
    }
    return next
  }, [entries])

  const listForTab =
    activeTab === 'identity' ||
    activeTab === 'work' ||
    activeTab === 'writing' ||
    activeTab === 'directive'
      ? buckets[activeTab]
      : buckets.other

  const otherCount = buckets.other.length

  const cancelEdit = useCallback(() => {
    setEditingKey(null)
    setEditDraft('')
  }, [])

  const submitEdit = useCallback(async () => {
    const key = editingKey
    if (!key?.trim()) return
    const next = editDraft.trim()
    if (!next) {
      setError('记忆内容不能为空。')
      return
    }
    setBusy(true)
    setError('')
    try {
      await kvStore(key, next)
      cancelEdit()
      await refresh()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [cancelEdit, editDraft, editingKey, kvStore, refresh])

  const onRemove = useCallback(
    async (key: string) => {
      if (busy || editingKey === key) return
      setBusy(true)
      setError('')
      try {
        await kvForget(key)
        await refresh()
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e))
      } finally {
        setBusy(false)
      }
    },
    [busy, editingKey, kvForget, refresh],
  )

  const onAdd = async () => {
    const text = draftText.trim()
    if (!text) return
    if (activeTab === 'other') {
      setError('请在上方选择一个分类后再添加记忆。')
      return
    }
    setBusy(true)
    setError('')
    try {
      const key = buildUserManualMemoryKey(activeTab)
      await kvStore(key, text)
      setDraftText('')
      await refresh()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  const tabButtons: { key: UserMemoryTabKey; label: string; count: number }[] = [
    ...USER_MEMORY_TABS.map((t) => ({ key: t.key as UserMemoryTabKey, label: t.label, count: buckets[t.key].length })),
    ...(otherCount > 0 ? [{ key: 'other' as const, label: '其他键', count: otherCount }] : []),
  ]

  const ledeSubject =
    selection === USER_MEMORY_DROPDOWN_GLOBAL
      ? '全局共享这一路'
      : `${selectedAgentName ?? '该智能体'} 在独立命名空间下的`

  return (
    <div className="user-memory-panel">
      <div className="user-memory-toolbar">
        <label className="user-memory-agent-field">
          <span className="user-memory-agent-label">记忆归属</span>
          <select
            value={selection}
            onChange={(e) => setSelection(e.target.value)}
            aria-label="选择全局或某一智能体的记忆命名空间"
          >
            <option value={USER_MEMORY_DROPDOWN_GLOBAL}>全局用户记忆</option>
            {uniqueAgents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </label>
        <button type="button" className="user-memory-toolbar-refresh ghost-link" disabled={busy} onClick={() => void reorganizeThenRefresh()}>
          重新整理
        </button>
      </div>
      <p className="settings-note user-memory-lede">
        {ledeSubject}
        K/V 记忆（与对话中的 <code>memory_store</code> 同源）。以下为可读文本视图，编辑后以纯字符串写回列表，不再铺满 JSON。
      </p>

      {error ? <p className="settings-note error">{error}</p> : null}
      {notice ? <p className="settings-note muted">{notice}</p> : null}

      <div className="user-memory-tabs" role="tablist">
        {tabButtons.map((t) => (
          <button
            key={t.key}
            type="button"
            role="tab"
            aria-selected={activeTab === t.key}
            className={`user-memory-tab ${activeTab === t.key ? 'active' : ''}`}
            onClick={() => setActiveTab(t.key)}
          >
            {t.label}{' '}
            <span className="user-memory-tab-count">({t.count})</span>
          </button>
        ))}
      </div>

      <div className="user-memory-list-wrap" role="tabpanel">
        {busy && !entries.length && editingKey === null ? (
          <p className="settings-note">载入中…</p>
        ) : listForTab.length === 0 ? (
          <p className="settings-note muted">此分类暂无条目。</p>
        ) : (
          <ul className="user-memory-list">
            {listForTab.map((row) => {
              const editing = editingKey === row.key
              const pres = memoryRowTag(row.key, row.value, row.originKind)
              const display = formatKvMemoryBody(row.value)

              const tagLabel = editing ? '自动' : pres.label
              const toneVariant: MemoryRowTagVariant | 'edit' = editing ? 'edit' : pres.variant

              return (
                <li key={row.key} className={`user-memory-row ${editing ? 'user-memory-row-editing' : ''}`}>
                  <span className={`user-memory-tag ${tagToneClassVariant(toneVariant)}`}>{tagLabel}</span>
                  <div className="user-memory-row-main">
                    {editing ? (
                      <input
                        ref={editInputRef}
                        className="user-memory-edit-input"
                        type="text"
                        value={editDraft}
                        onChange={(e) => setEditDraft(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') void submitEdit()
                          if (e.key === 'Escape') cancelEdit()
                        }}
                        aria-label="编辑记忆内容"
                      />
                    ) : (
                      <span className="user-memory-row-text">{display || '—'}</span>
                    )}
                  </div>
                  <div className={`user-memory-row-actions ${editing ? 'editing' : ''}`}>
                    {editing ? (
                      <>
                        <button
                          type="button"
                          className="user-memory-icon-btn affirm"
                          disabled={busy}
                          aria-label="保存修改"
                          onClick={() => void submitEdit()}
                        >
                          <IconCheck />
                        </button>
                      </>
                    ) : (
                      <>
                        <button
                          type="button"
                          className="user-memory-icon-btn"
                          disabled={busy}
                          aria-label="编辑这条记忆"
                          onClick={() => {
                            setEditingKey(row.key)
                            setEditDraft(display)
                          }}
                        >
                          <IconPencil />
                        </button>
                        <button
                          type="button"
                          className="user-memory-icon-btn danger"
                          disabled={busy}
                          aria-label="删除这条记忆"
                          onClick={() => void onRemove(row.key)}
                        >
                          <IconTrash />
                        </button>
                      </>
                    )}
                  </div>
                </li>
              )
            })}
          </ul>
        )}
      </div>

      <div className="user-memory-inline-meta">
        命名空间 <code>{workspaceIdResolved || '—'}</code>
      </div>

      <div className="user-memory-compose">
        <input
          className="user-memory-compose-input"
          type="text"
          placeholder="添加一条新记忆…"
          value={draftText}
          disabled={busy || activeTab === 'other' || editingKey !== null}
          onChange={(e) => setDraftText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void onAdd()
          }}
        />
        <button type="button" className="outline-button primary user-memory-compose-submit" disabled={busy} onClick={() => void onAdd()}>
          添加
        </button>
      </div>
    </div>
  )
}
