import { useCallback, useEffect, useMemo, useState } from 'react'
import { AppIcon } from '../../../components/AppIcon'
import './SessionLlmLogPanel.css'
import {
  onSessionLlmLogEvent,
  sessionLlmLogClear,
  sessionLlmLogGet,
  sessionLlmLogList,
  type SessionLlmLogDetail,
  type SessionLlmLogInfo,
} from '../../../lib/sessionLlmLogClient'
import {
  filterSessionLogs,
  formatLogSize,
  matchesSessionLogScope,
  parseSessionLlmLogSections,
} from './sessionLlmLogModel'

export type SessionLlmLogPanelProps = {
  workspaceId?: string | null
  sessionId?: string | null
  open: boolean
  onClose: () => void
  standalone?: boolean
}

function formatTime(ts: number) {
  if (!ts) return '—'
  const d = new Date(ts)
  const pad = (n: number) => n.toString().padStart(2, '0')
  return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

function downloadMarkdown(detail: SessionLlmLogDetail | null) {
  if (!detail) return
  const blob = new Blob([detail.content], { type: 'text/markdown;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `session-llm-log-${detail.info.sessionId}.md`
  a.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export function SessionLlmLogPanel({
  workspaceId,
  sessionId,
  open,
  onClose,
  standalone = false,
}: SessionLlmLogPanelProps) {
  const [logs, setLogs] = useState<SessionLlmLogInfo[]>([])
  const [selectedSessionId, setSelectedSessionId] = useState(sessionId?.trim() ?? '')
  const [detail, setDetail] = useState<SessionLlmLogDetail | null>(null)
  const [query, setQuery] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    const sid = sessionId?.trim() ?? ''
    if (sid) setSelectedSessionId(sid)
  }, [sessionId])

  const refreshList = useCallback(async () => {
    if (!open) return
    const list = await sessionLlmLogList({ workspaceId: workspaceId ?? null, limit: 200 })
    setLogs(list)
  }, [open, workspaceId])

  const refreshDetail = useCallback(
    async (sid: string) => {
      const trimmed = sid.trim()
      if (!trimmed || !open) return
      setLoading(true)
      setError('')
      try {
        const next = await sessionLlmLogGet({ workspaceId: workspaceId ?? null, sessionId: trimmed })
        setDetail(next)
      } catch (err) {
        setDetail(null)
        setError(err instanceof Error ? err.message : String(err))
      } finally {
        setLoading(false)
      }
    },
    [open, workspaceId],
  )

  useEffect(() => {
    if (!open) return
    void refreshList().catch(() => {})
  }, [open, refreshList])

  useEffect(() => {
    if (!open || !selectedSessionId) return
    void refreshDetail(selectedSessionId)
  }, [open, refreshDetail, selectedSessionId])

  useEffect(() => {
    if (!open) return
    let unsubscribe: (() => void) | null = null
    void onSessionLlmLogEvent((payload) => {
      if (!matchesSessionLogScope(payload, { workspaceId: workspaceId ?? null, sessionId: selectedSessionId || null })) {
        if (workspaceId && payload.workspaceId !== workspaceId) return
      }
      void refreshList().catch(() => {})
      if (payload.sessionId === selectedSessionId) {
        void refreshDetail(selectedSessionId).catch(() => {})
      }
    }).then((unlisten) => {
      unsubscribe = unlisten
    })
    return () => unsubscribe?.()
  }, [open, refreshDetail, refreshList, selectedSessionId, workspaceId])

  const filteredLogs = useMemo(() => filterSessionLogs(logs, query), [logs, query])
  const sections = useMemo(() => parseSessionLlmLogSections(detail?.content ?? ''), [detail])

  const handleClear = useCallback(async () => {
    if (!selectedSessionId) return
    const ok = window.confirm('清空当前 session 的文本日志？')
    if (!ok) return
    await sessionLlmLogClear({ workspaceId: workspaceId ?? null, sessionId: selectedSessionId })
    setDetail(null)
    await refreshList()
  }, [refreshList, selectedSessionId, workspaceId])

  if (!open) return null

  return (
    <section className={`session-log-panel${standalone ? ' standalone' : ''}`}>
      <header className="session-log-head">
        <div className="session-log-title">
          <AppIcon name="folder" size={16} />
          <span>Session 文本日志</span>
          {selectedSessionId ? <code>{selectedSessionId}</code> : null}
        </div>
        <div className="session-log-actions">
          <button type="button" title="刷新" onClick={() => void Promise.all([refreshList(), refreshDetail(selectedSessionId)])}>
            <AppIcon name="refresh" size={13} />
          </button>
          <button type="button" title="复制 Markdown" onClick={() => void navigator.clipboard.writeText(detail?.content ?? '')}>
            <AppIcon name="message" size={13} />
          </button>
          <button type="button" title="下载 Markdown" onClick={() => downloadMarkdown(detail)}>
            <AppIcon name="download" size={13} />
          </button>
          <button type="button" title="清空当前日志" onClick={() => void handleClear()}>
            <AppIcon name="trash" size={13} />
          </button>
          {!standalone ? (
            <button type="button" title="关闭" onClick={onClose}>
              <AppIcon name="close" size={13} />
            </button>
          ) : null}
        </div>
      </header>

      <div className="session-log-body">
        <aside className="session-log-sidebar">
          <label className="session-log-search">
            <AppIcon name="search" size={14} />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索 session / 内容" />
          </label>
          <div className="session-log-list">
            {filteredLogs.length === 0 ? (
              <div className="session-log-empty">还没有 session 文本日志。</div>
            ) : (
              filteredLogs.map((item) => (
                <button
                  type="button"
                  key={item.path}
                  className={`session-log-item${item.sessionId === selectedSessionId ? ' active' : ''}`}
                  onClick={() => setSelectedSessionId(item.sessionId)}
                >
                  <span>{item.sessionId}</span>
                  <small>{formatTime(item.modifiedAt)} · {formatLogSize(item.size)}</small>
                  {item.preview ? <p>{item.preview}</p> : null}
                </button>
              ))
            )}
          </div>
        </aside>

        <main className="session-log-detail">
          {loading ? <div className="session-log-empty">正在读取日志...</div> : null}
          {!loading && error ? <div className="session-log-error">{error}</div> : null}
          {!loading && !error && detail ? (
            <>
              <div className="session-log-meta">
                <span>{formatTime(detail.info.modifiedAt)}</span>
                <span>{formatLogSize(detail.info.size)}</span>
                <code>{detail.info.path}</code>
              </div>
              <div className="session-log-sections">
                {sections.map((section) => (
                  <article key={section.id} className={`session-log-section tone-${section.tone}`}>
                    <h3>{section.title}</h3>
                    <pre>{section.body || '—'}</pre>
                  </article>
                ))}
              </div>
            </>
          ) : null}
          {!loading && !error && !detail ? <div className="session-log-empty">选择一个 session 查看文本日志。</div> : null}
        </main>
      </div>
    </section>
  )
}

export default SessionLlmLogPanel
