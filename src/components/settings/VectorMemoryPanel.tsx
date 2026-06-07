import { useCallback, useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  getEmbeddingStatus,
  loadEmbeddingSettings,
  saveEmbeddingSettings,
  triggerEmbeddingReindex,
} from '../../lib/piClient'
import type {
  EmbeddingProviderStatus,
  EmbeddingSettings,
  WorkspaceMemoryRecord,
  WorkspaceRecord,
} from '../../types'

type MemoryStats = {
  systemCount: number
  workspaceCount: number
  agentCount: number
}

type ScopeTab = 'all' | 'system' | 'workspace' | 'agent'

const SCOPE_TABS: { key: ScopeTab; label: string }[] = [
  { key: 'all', label: '全部' },
  { key: 'system', label: '系统级' },
  { key: 'workspace', label: '团队级' },
  { key: 'agent', label: '智能体级' },
]

const SCOPE_LABELS: Record<string, string> = {
  system: '系统级',
  workspace: '团队级',
  agent: '智能体级',
}

const SCOPE_COLORS: Record<string, string> = {
  system: '#6366f1',
  workspace: '#3b82f6',
  agent: '#f59e0b',
}

export function VectorMemoryPanel() {
  const [embeddingSettings, setEmbeddingSettings] = useState<EmbeddingSettings | null>(null)
  const [embeddingStatus, setEmbeddingStatus] = useState<EmbeddingProviderStatus | null>(null)
  const [embeddingBusy, setEmbeddingBusy] = useState(false)
  const [embeddingNotice, setEmbeddingNotice] = useState('')
  const [workspaces, setWorkspaces] = useState<WorkspaceRecord[]>([])
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState('')
  const [memories, setMemories] = useState<WorkspaceMemoryRecord[]>([])
  const [stats, setStats] = useState<MemoryStats | null>(null)
  const [scopeTab, setScopeTab] = useState<ScopeTab>('all')
  const [search, setSearch] = useState('')
  const [expandedId, setExpandedId] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')

  // Load workspace list on mount
  useEffect(() => {
    let cancelled = false
    async function load() {
      try {
        const [ws, settings, status] = await Promise.all([
          invoke<WorkspaceRecord[]>('workspace_list', { includeArchived: false }),
          loadEmbeddingSettings(),
          getEmbeddingStatus(),
        ])
        if (cancelled) return
        const active = ws.filter((w) => !w.archived)
        setWorkspaces(active)
        setEmbeddingSettings(settings)
        setEmbeddingStatus(status)
        if (active.length > 0 && !selectedWorkspaceId) {
          setSelectedWorkspaceId(active[0].id)
        }
      } catch {
        // ignore — panel simply stays empty
      }
    }
    void load()
    return () => { cancelled = true }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Fetch memories + stats whenever workspaceId / scopeTab changes
  const fetchMemories = useCallback(async () => {
    if (!selectedWorkspaceId) return
    setLoading(true)
    setError('')
    try {
      const scopeParam = scopeTab === 'all' ? undefined : scopeTab
      let list: WorkspaceMemoryRecord[]
      if (search.trim()) {
        list = await invoke<WorkspaceMemoryRecord[]>('memory_search_text', {
          workspaceId: selectedWorkspaceId,
          query: search.trim(),
          scope: scopeParam ?? null,
          limit: 100,
        })
      } else {
        list = await invoke<WorkspaceMemoryRecord[]>('memory_list', {
          workspaceId: selectedWorkspaceId,
          scope: scopeParam ?? null,
          limit: 50,
          page: 1,
        })
      }
      setMemories(list)

      const s = await invoke<MemoryStats>('memory_stats', { workspaceId: selectedWorkspaceId })
      setStats(s)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
      setMemories([])
    } finally {
      setLoading(false)
    }
  }, [selectedWorkspaceId, scopeTab, search])

  useEffect(() => {
    void fetchMemories()
  }, [fetchMemories])

  const totalCount = useMemo(() => {
    if (!stats) return memories.length
    return stats.systemCount + stats.workspaceCount + stats.agentCount
  }, [stats, memories])

  const onChangeScope = async (memoryId: string, newScope: string) => {
    try {
      await invoke('memory_update_scope', { memoryId, scope: newScope, scopeAgentId: null })
      await fetchMemories()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  const onDelete = async (memoryId: string) => {
    try {
      await invoke('workspace_delete_memory', { workspaceId: selectedWorkspaceId, memoryId })
      await fetchMemories()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  const formatTime = (ts: number) => {
    if (!ts) return '—'
    const d = new Date(ts * 1000)
    return d.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
  }

  const parseTags = (tagsJson: string): string[] => {
    if (!tagsJson) return []
    try {
      const parsed = JSON.parse(tagsJson)
      return Array.isArray(parsed) ? parsed : []
    } catch {
      return []
    }
  }

  const refreshEmbeddingStatus = useCallback(async () => {
    try {
      const status = await getEmbeddingStatus()
      setEmbeddingStatus(status)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  const updateEmbeddingDraft = (updates: Partial<EmbeddingSettings>) => {
    setEmbeddingSettings((previous) => {
      const base: EmbeddingSettings = previous ?? {
        mode: 'local',
        remoteEndpoint: '',
        remoteModelName: '',
        remoteApiKey: '',
        remoteDimension: 512,
      }
      return { ...base, ...updates }
    })
  }

  const persistEmbeddingSettings = async () => {
    if (!embeddingSettings) return
    setEmbeddingBusy(true)
    setEmbeddingNotice('正在保存配置并初始化 provider…')
    setError('')
    try {
      const status = await saveEmbeddingSettings(embeddingSettings)
      setEmbeddingStatus(status)
      if (status.activeProviderId) {
        setEmbeddingNotice(`配置已保存。Provider: ${status.activeProviderId}，向量 ${status.vectorCount} 条。`)
      } else {
        setError(`配置已保存，但 provider 未就绪：${status.message || '未知原因'}。如果是本地模式，请确认模型文件已下载。`)
        setEmbeddingNotice('')
      }
    } catch (e) {
      setError(`保存配置失败：${e instanceof Error ? e.message : String(e)}`)
      setEmbeddingNotice('')
    } finally {
      setEmbeddingBusy(false)
    }
  }

  const runReindex = async () => {
    setEmbeddingBusy(true)
    setEmbeddingNotice('正在重建向量索引…')
    setError('')
    try {
      const result = await triggerEmbeddingReindex()
      await refreshEmbeddingStatus()
      if (result.searchModeReady) {
        setEmbeddingNotice(`重建完成：已索引 ${result.indexed} 条，跳过 ${result.skipped} 条。Provider: ${result.providerId ?? '未知'}`)
      } else {
        setError(result.message || '重建失败：没有可用的 embedding provider。请先确认本地模型已下载或远程 API 已正确配置。')
        setEmbeddingNotice('')
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      setError(`重建索引失败：${msg}`)
      setEmbeddingNotice('')
    } finally {
      setEmbeddingBusy(false)
    }
  }

  return (
    <div className="user-memory-panel" style={{ marginTop: 24, borderTop: '1px solid var(--color-border, #333)', paddingTop: 16 }}>
      <h4 style={{ marginBottom: 8, color: 'var(--color-text, #e0e0e0)', fontSize: 14, fontWeight: 600 }}>
        向量记忆（按作用域分组）
      </h4>

      {embeddingSettings ? (
        <div
          style={{
            border: '1px solid var(--color-border, #333)',
            borderRadius: 12,
            padding: 12,
            marginBottom: 12,
            background: 'var(--color-surface, rgba(255,255,255,0.02))',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap' }}>
            <strong style={{ fontSize: 13 }}>语义检索 Provider</strong>
            <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
              <label style={{ fontSize: 12 }}>
                模式：
                <select
                  value={embeddingSettings.mode}
                  onChange={(e) => updateEmbeddingDraft({ mode: e.target.value as EmbeddingSettings['mode'] })}
                  style={{ marginLeft: 4, fontSize: 12 }}
                >
                  <option value="local">本地模型</option>
                  <option value="remote">远程 API</option>
                </select>
              </label>
              <button type="button" className="ghost-link" disabled={embeddingBusy} onClick={() => void persistEmbeddingSettings()}>
                {embeddingBusy ? '处理中…' : '保存配置'}
              </button>
              <button type="button" className="ghost-link" disabled={embeddingBusy} onClick={() => void runReindex()}>
                {embeddingBusy ? '处理中…' : '重建索引'}
              </button>
            </div>
          </div>

          {embeddingSettings.mode === 'remote' ? (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: 8, marginTop: 10 }}>
              <label className="input-field">
                <span>Endpoint</span>
                <input
                  value={embeddingSettings.remoteEndpoint}
                  onChange={(e) => updateEmbeddingDraft({ remoteEndpoint: e.target.value })}
                  placeholder="https://api.openai.com/v1/embeddings"
                />
              </label>
              <label className="input-field">
                <span>Model</span>
                <input
                  value={embeddingSettings.remoteModelName}
                  onChange={(e) => updateEmbeddingDraft({ remoteModelName: e.target.value })}
                  placeholder="text-embedding-3-small"
                />
              </label>
              <label className="input-field">
                <span>API Key</span>
                <input
                  type="password"
                  value={embeddingSettings.remoteApiKey}
                  onChange={(e) => updateEmbeddingDraft({ remoteApiKey: e.target.value })}
                  placeholder="sk-..."
                />
              </label>
              <label className="input-field">
                <span>Dimension</span>
                <input
                  type="number"
                  min={1}
                  value={embeddingSettings.remoteDimension}
                  onChange={(e) => updateEmbeddingDraft({ remoteDimension: Number(e.target.value) || 512 })}
                />
              </label>
            </div>
          ) : null}

          {embeddingStatus ? (
            <div style={{ marginTop: 10 }}>
              <p className="settings-note" style={{ marginBottom: 4 }}>
                当前 provider：<code>{embeddingStatus.activeProviderId || '未启用'}</code> · 向量 {embeddingStatus.vectorCount} 条 · 配置 {embeddingStatus.providerCount} 个
              </p>
              <p className="settings-note" style={{ marginBottom: 4 }}>
                本地模型：{embeddingStatus.localModelReady ? '已就绪' : embeddingStatus.localDownloadState} {embeddingStatus.localModelPath ? <code>{embeddingStatus.localModelPath}</code> : null}
              </p>
              {embeddingStatus.message ? <p className="settings-note">{embeddingStatus.message}</p> : null}
            </div>
          ) : null}

          {embeddingNotice ? <p className="settings-note">{embeddingNotice}</p> : null}
        </div>
      ) : null}

      {/* Workspace selector */}
      <div className="user-memory-toolbar" style={{ marginBottom: 8 }}>
        <div className="user-memory-agent-field">
          <span className="user-memory-agent-label">工作空间</span>
          <label className="select-field user-memory-agent-select">
            <select
              value={selectedWorkspaceId}
              onChange={(e) => setSelectedWorkspaceId(e.target.value)}
              aria-label="选择工作空间"
            >
              {workspaces.length === 0 ? (
                <option value="">暂无工作空间</option>
              ) : (
                workspaces.map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.name}
                  </option>
                ))
              )}
            </select>
          </label>
        </div>
        <button
          type="button"
          className="user-memory-toolbar-refresh ghost-link"
          disabled={loading}
          onClick={() => void fetchMemories()}
        >
          刷新
        </button>
      </div>

      {/* Stats */}
      {stats ? (
        <p className="settings-note" style={{ marginBottom: 8 }}>
          共 {totalCount} 条记忆：系统级 {stats.systemCount} · 团队级 {stats.workspaceCount} · 智能体级 {stats.agentCount}
        </p>
      ) : null}

      {/* Search */}
      <div style={{ marginBottom: 10 }}>
        <input
          type="text"
          className="user-memory-compose-input"
          placeholder="搜索记忆标题或内容…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') void fetchMemories() }}
          style={{ width: '100%', boxSizing: 'border-box' }}
        />
      </div>

      {error ? <p className="settings-note error">{error}</p> : null}

      {/* Scope tabs */}
      <div className="user-memory-tabs" role="tablist">
        {SCOPE_TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            role="tab"
            aria-selected={scopeTab === t.key}
            className={`user-memory-tab ${scopeTab === t.key ? 'active' : ''}`}
            onClick={() => setScopeTab(t.key)}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Memory list */}
      <div className="user-memory-list-wrap" role="tabpanel">
        {loading ? (
          <p className="settings-note">载入中…</p>
        ) : memories.length === 0 ? (
          <p className="settings-note muted">暂无记忆条目。</p>
        ) : (
          <ul className="user-memory-list">
            {memories.map((m) => {
              const tags = parseTags(m.tagsJson)
              const isExpanded = expandedId === m.id
              const scopeLabel = SCOPE_LABELS[m.scope] ?? m.scope
              const scopeColor = SCOPE_COLORS[m.scope] ?? '#888'

              return (
                <li key={m.id} className="user-memory-row" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: 6 }}>
                  {/* Top line: title + scope badge */}
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%' }}>
                    <strong
                      style={{ cursor: 'pointer', flex: 1, fontSize: 13, color: 'var(--color-text, #e0e0e0)' }}
                      onClick={() => setExpandedId(isExpanded ? null : m.id)}
                    >
                      {m.title || '（无标题）'}
                    </strong>
                    <span
                      style={{
                        display: 'inline-block',
                        padding: '1px 8px',
                        borderRadius: 10,
                        fontSize: 11,
                        color: '#fff',
                        background: scopeColor,
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {scopeLabel}
                    </span>
                    <span className="user-memory-row-text" style={{ fontSize: 11, color: 'var(--color-text-muted, #999)', whiteSpace: 'nowrap' }}>
                      {formatTime(m.updatedAt)}
                    </span>
                  </div>

                  {/* Content preview (or full) */}
                  <div
                    className="user-memory-row-text"
                    style={{
                      maxHeight: isExpanded ? undefined : 40,
                      overflow: isExpanded ? undefined : 'hidden',
                      textOverflow: 'ellipsis',
                      cursor: 'pointer',
                      width: '100%',
                    }}
                    onClick={() => setExpandedId(isExpanded ? null : m.id)}
                  >
                    {m.content}
                  </div>

                  {/* Tags */}
                  {tags.length > 0 ? (
                    <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
                      {tags.map((tag) => (
                        <span
                          key={tag}
                          style={{
                            padding: '0 6px',
                            borderRadius: 8,
                            fontSize: 11,
                            background: 'var(--color-surface, #222)',
                            color: 'var(--color-text-muted, #aaa)',
                            border: '1px solid var(--color-border, #333)',
                          }}
                        >
                          {tag}
                        </span>
                      ))}
                    </div>
                  ) : null}

                  {/* Expanded actions */}
                  {isExpanded ? (
                    <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap', marginTop: 4 }}>
                      <label style={{ fontSize: 12, color: 'var(--color-text-muted, #999)' }}>
                        作用域：
                        <select
                          value={m.scope}
                          onChange={(e) => void onChangeScope(m.id, e.target.value)}
                          style={{ marginLeft: 4, fontSize: 12 }}
                        >
                          <option value="system">系统级</option>
                          <option value="workspace">团队级</option>
                          <option value="agent">智能体级</option>
                        </select>
                      </label>
                      {m.authorAgentId ? (
                        <span style={{ fontSize: 11, color: 'var(--color-text-muted, #999)' }}>
                          作者: {m.authorAgentId}
                        </span>
                      ) : null}
                      <button
                        type="button"
                        className="outline-button danger"
                        style={{ fontSize: 12, padding: '2px 10px' }}
                        onClick={() => {
                          if (window.confirm('确认删除此条记忆？')) {
                            void onDelete(m.id)
                          }
                        }}
                      >
                        删除
                      </button>
                    </div>
                  ) : null}
                </li>
              )
            })}
          </ul>
        )}
      </div>

      {/* Inline meta */}
      <div className="user-memory-inline-meta" style={{ marginTop: 8 }}>
        工作空间 <code>{selectedWorkspaceId || '—'}</code> · {memories.length} 条可见
      </div>
    </div>
  )
}
