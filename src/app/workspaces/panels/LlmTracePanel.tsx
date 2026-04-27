import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  llmTraceClear,
  llmTraceList,
  onLlmTraceEvent,
  workspaceLlmTraceSetEnabled,
  workspaceLlmTraceStatus,
  type LlmTraceEntry,
  type LlmTraceEvent,
} from '../../../lib/llmTraceClient'
import { AppIcon } from '../../../components/AppIcon'
import {
  applyLlmTraceEvent,
  getTraceKindLabel,
  getTraceMessageBlocks,
  getTraceResponseBlocks,
  matchesTraceScope,
} from './llmTraceModel'
import { openLlmTracePopout } from '../../lib/llmTracePopout'

export type LlmTracePanelProps = {
  workspaceId?: string | null
  /** 当前会话 id：提供则只显示该会话产生的调试记录。 */
  sessionId?: string | null
  open: boolean
  onClose: () => void
  /** 独立窗口模式：填满视口，不可拖动，不显示弹出按钮。 */
  standalone?: boolean
}

type DragState = { startX: number; startY: number; offsetX: number; offsetY: number } | null

const INITIAL_POSITION = { x: 120, y: 80 }
const INITIAL_SIZE = { width: 520, height: 640 }

const statusLabel = (status: string): { label: string; tone: 'running' | 'done' | 'error' | 'muted' } => {
  switch (status) {
    case 'running':
      return { label: '运行中', tone: 'running' }
    case 'done':
      return { label: '成功', tone: 'done' }
    case 'error':
      return { label: '失败', tone: 'error' }
    case 'aborted':
      return { label: '已中止', tone: 'error' }
    default:
      return { label: status || '未知', tone: 'muted' }
  }
}

const formatDuration = (ms?: number | null) => {
  if (!ms && ms !== 0) return '—'
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(1)}s`
}

const formatTokens = (n?: number | null) => {
  if (!n && n !== 0) return '—'
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`
  return String(n)
}

const formatTime = (ts: number) => {
  const d = new Date(ts)
  const pad = (n: number) => n.toString().padStart(2, '0')
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

function useDraggable(initial: { x: number; y: number }) {
  const [pos, setPos] = useState(initial)
  const dragRef = useRef<DragState>(null)
  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      dragRef.current = {
        startX: e.clientX,
        startY: e.clientY,
        offsetX: pos.x,
        offsetY: pos.y,
      }
      const onMove = (ev: MouseEvent) => {
        if (!dragRef.current) return
        const nx = dragRef.current.offsetX + (ev.clientX - dragRef.current.startX)
        const ny = dragRef.current.offsetY + (ev.clientY - dragRef.current.startY)
        setPos({ x: Math.max(0, nx), y: Math.max(0, ny) })
      }
      const onUp = () => {
        dragRef.current = null
        window.removeEventListener('mousemove', onMove)
        window.removeEventListener('mouseup', onUp)
      }
      window.addEventListener('mousemove', onMove)
      window.addEventListener('mouseup', onUp)
    },
    [pos],
  )
  return { pos, onMouseDown }
}

type Expanded = { [id: string]: boolean }
type Section = 'messages' | 'responses' | 'tools' | 'raw'
type ViewMode = 'tree' | 'flat'

function TraceRow({
  entry,
  expanded,
  onToggle,
  depth = 0,
  childCount = 0,
}: {
  entry: LlmTraceEntry
  expanded: boolean
  onToggle: () => void
  depth?: number
  childCount?: number
}) {
  const [section, setSection] = useState<Section>('messages')
  const st = statusLabel(entry.status)
  const caller = entry.callerAgentName || entry.callerAgentId
  const target =
    entry.targetAgentName ||
    entry.targetAgentId ||
    (entry.targetKind === 'model' || entry.kind === 'main_pi' || entry.kind === 'action_llm' ? 'LLM' : '—')
  const input = entry.usage?.inputTokens ?? null
  const output = entry.usage?.outputTokens ?? null
  const messageBlocks = useMemo(() => getTraceMessageBlocks(entry), [entry])
  const responseBlocks = useMemo(() => getTraceResponseBlocks(entry), [entry])

  const rawJson = useMemo(() => JSON.stringify(entry, null, 2), [entry])

  const copyJson = useCallback(() => {
    void navigator.clipboard.writeText(rawJson).catch(() => {})
  }, [rawJson])

  const downloadJson = useCallback(() => {
    const blob = new Blob([rawJson], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `trace-${entry.id.slice(0, 8)}.json`
    a.click()
    setTimeout(() => URL.revokeObjectURL(url), 1000)
  }, [rawJson, entry.id])

  return (
    <div
      className={`llm-trace-row trace-${st.tone}${expanded ? ' is-open' : ''}${depth > 0 ? ' is-child' : ''}`}
      style={depth > 0 ? { marginLeft: depth * 18 } : undefined}
    >
      <button type="button" className="llm-trace-row-head" onClick={onToggle}>
        {depth > 0 ? <span className="llm-trace-branch" aria-hidden>└</span> : null}
        <span className={`llm-trace-dot tone-${st.tone}`} />
        <span className="llm-trace-row-kind">{getTraceKindLabel(entry)}</span>
        <span className="llm-trace-row-agents">
          <span className="llm-trace-caller">{caller}</span>
          <span className="llm-trace-arrow">→</span>
          <span className="llm-trace-target">{target}</span>
        </span>
        {childCount > 0 ? (
          <span className="llm-trace-children-badge" title={`派出 ${childCount} 个子 Agent`}>
            派 {childCount}
          </span>
        ) : null}
        <span className="llm-trace-row-model">{entry.model || '—'}</span>
        <span className="llm-trace-row-tokens">
          {formatTokens(input)}↑ {formatTokens(output)}↓
        </span>
        <span className="llm-trace-row-duration">{formatDuration(entry.durationMs ?? null)}</span>
        <span className="llm-trace-row-time">{formatTime(entry.startedAt)}</span>
      </button>
      {expanded ? (
        <div className="llm-trace-row-body">
          <dl className="llm-trace-meta">
            <dt>状态</dt>
            <dd>
              <span className={`llm-trace-pill tone-${st.tone}`}>{st.label}</span>
              {entry.error ? <span className="llm-trace-error-msg">{entry.error}</span> : null}
            </dd>
            <dt>提供商</dt>
            <dd>{entry.provider || '—'}</dd>
            <dt>调用者</dt>
            <dd>
              {caller} <span className="llm-trace-muted">({entry.callerAgentId})</span>
            </dd>
            <dt>目标</dt>
            <dd>
              {target}
              {entry.targetAgentId ? (
                <span className="llm-trace-muted"> ({entry.targetAgentId})</span>
              ) : null}
            </dd>
            <dt>Session</dt>
            <dd className="mono">{entry.sessionId || '—'}</dd>
            <dt>Response ID</dt>
            <dd className="mono">{entry.responseId || '—'}</dd>
            <dt>Token</dt>
            <dd>
              输入 {formatTokens(input)} · 输出 {formatTokens(output)}
              {entry.usage?.cacheReadTokens ? ` · 缓存读 ${formatTokens(entry.usage.cacheReadTokens)}` : ''}
              {entry.usage?.totalTokens ? ` · 合计 ${formatTokens(entry.usage.totalTokens)}` : ''}
            </dd>
          </dl>

          <div className="llm-trace-actions">
            <button type="button" onClick={copyJson}>
              复制 JSON
            </button>
            <button type="button" onClick={downloadJson}>
              下载 .json
            </button>
          </div>

          <nav className="llm-trace-tabs" role="tablist">
            <button
              type="button"
              className={section === 'messages' ? 'active' : ''}
              onClick={() => setSection('messages')}
            >
              消息 ({messageBlocks.length})
            </button>
            <button
              type="button"
              className={section === 'responses' ? 'active' : ''}
              onClick={() => setSection('responses')}
            >
              响应 ({responseBlocks.length})
            </button>
            <button
              type="button"
              className={section === 'tools' ? 'active' : ''}
              onClick={() => setSection('tools')}
            >
              工具 ({entry.toolCalls.length})
            </button>
            <button
              type="button"
              className={section === 'raw' ? 'active' : ''}
              onClick={() => setSection('raw')}
            >
              原始 JSON
            </button>
          </nav>

          {section === 'messages' ? (
            <div className="llm-trace-section">
              {messageBlocks.length === 0 ? (
                <div className="llm-trace-empty">（未记录消息块）</div>
              ) : (
                messageBlocks.map((block, index) => (
                  <details
                    key={block.id || `${block.role}-${index}`}
                    className="llm-trace-prompt-part"
                    open={index < 2}
                  >
                    <summary>
                      <span>{block.role.toUpperCase()}</span>
                      <span className="llm-trace-muted"> · {block.label}</span>
                    </summary>
                    <pre className="llm-trace-pre">{block.content || '—'}</pre>
                  </details>
                ))
              )}
            </div>
          ) : null}

          {section === 'responses' ? (
            <div className="llm-trace-section">
              {responseBlocks.length === 0 ? (
                <div className="llm-trace-empty">（未记录响应块）</div>
              ) : (
                responseBlocks.map((block, index) => (
                  <details
                    key={block.id || `${block.kind}-${index}`}
                    className="llm-trace-prompt-part"
                    open={index < 2}
                  >
                    <summary>
                      <span>{block.kind === 'thinking' ? '思考过程' : '回复内容'}</span>
                      <span className="llm-trace-muted"> · {block.label}</span>
                    </summary>
                    <pre
                      className={`llm-trace-pre${block.kind === 'thinking' ? ' llm-trace-thinking' : ''}`}
                    >
                      {block.content || '—'}
                    </pre>
                  </details>
                ))
              )}
            </div>
          ) : null}

          {section === 'tools' ? (
            <div className="llm-trace-section">
              {entry.toolCalls.length === 0 ? (
                <div className="llm-trace-empty">（本次调用未使用工具）</div>
              ) : (
                entry.toolCalls.map((t, i) => {
                  const toolStatus = statusLabel(t.status)
                  return (
                    <details key={`${t.toolCallId}-${i}`} className="llm-trace-tool">
                      <summary>
                        <span className={`llm-trace-pill tone-${toolStatus.tone}`}>
                          {toolStatus.label}
                        </span>
                        <span className="llm-trace-tool-name">#{i + 1} {t.toolName || '(unnamed)'}</span>
                        {t.finishedAt ? (
                          <span className="llm-trace-muted">
                            {' '}
                            · {formatDuration(t.finishedAt - t.startedAt)}
                          </span>
                        ) : null}
                      </summary>
                      <div className="llm-trace-tool-body">
                        <h5>参数</h5>
                        <pre className="llm-trace-pre mono">{t.argsJson || '—'}</pre>
                        <h5>结果</h5>
                        <pre className="llm-trace-pre">{t.resultText || '—'}</pre>
                      </div>
                    </details>
                  )
                })
              )}
            </div>
          ) : null}

          {section === 'raw' ? (
            <div className="llm-trace-section">
              <pre className="llm-trace-pre mono">{rawJson}</pre>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

export function LlmTracePanel({
  workspaceId,
  sessionId,
  open,
  onClose,
  standalone = false,
}: LlmTracePanelProps) {
  const { pos, onMouseDown } = useDraggable(INITIAL_POSITION)
  const [enabled, setEnabled] = useState(false)
  const [entries, setEntries] = useState<LlmTraceEntry[]>([])
  const [expanded, setExpanded] = useState<Expanded>({})
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [sessionOnly, setSessionOnly] = useState<boolean>(!!sessionId)
  const [viewMode, setViewMode] = useState<ViewMode>('tree')
  const hasWorkspaceScope = Boolean(workspaceId?.trim())
  const hasSessionScope = Boolean(sessionId?.trim())

  useEffect(() => {
    if (!sessionId) setSessionOnly(false)
  }, [sessionId])

  const matchesCurrentSession = useCallback(
    (e: LlmTraceEntry) => {
      if (!sessionOnly || !sessionId) return true
      return e.sessionId === sessionId
    },
    [sessionOnly, sessionId],
  )

  const refresh = useCallback(async () => {
    if (!workspaceId && !sessionId) return
    setLoading(true)
    try {
      const list = await llmTraceList({
        workspaceId: workspaceId ?? null,
        sessionId: sessionId ?? null,
        days: 3,
        limit: 200,
      })
      setEntries(list)
      setError(null)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [sessionId, workspaceId])

  useEffect(() => {
    if (!open || (!workspaceId && !sessionId)) return
    let mounted = true
    void (async () => {
      if (!hasWorkspaceScope) {
        if (mounted) setEnabled(true)
        return
      }
      try {
        const status = await workspaceLlmTraceStatus(workspaceId!)
        if (mounted) setEnabled(status)
      } catch (e) {
        if (mounted) setError(String(e))
      }
    })()
    void refresh()
    return () => {
      mounted = false
    }
  }, [hasWorkspaceScope, open, refresh, sessionId, workspaceId])

  useEffect(() => {
    if (!open || (!workspaceId && !sessionId)) {
      return
    }
    const intervalId = window.setInterval(() => {
      void refresh()
    }, 2500)
    const handleVisibility = () => {
      if (document.visibilityState === 'visible') {
        void refresh()
      }
    }
    document.addEventListener('visibilitychange', handleVisibility)
    return () => {
      window.clearInterval(intervalId)
      document.removeEventListener('visibilitychange', handleVisibility)
    }
  }, [open, refresh, sessionId, workspaceId])

  useEffect(() => {
    if (!open || (!workspaceId && !sessionId)) return
    let unsubscribe: (() => void) | null = null
    void onLlmTraceEvent((payload: LlmTraceEvent) => {
      if (!matchesTraceScope(payload.entry, { workspaceId, sessionId })) return
      setEntries((prev) => applyLlmTraceEvent(prev, payload))
    }).then((un) => {
      unsubscribe = un
    })
    return () => {
      unsubscribe?.()
    }
  }, [open, sessionId, workspaceId])

  const toggle = useCallback(async () => {
    if (!workspaceId) return
    try {
      const next = !enabled
      await workspaceLlmTraceSetEnabled(workspaceId, next)
      setEnabled(next)
    } catch (e) {
      setError(String(e))
    }
  }, [enabled, workspaceId])

  const clear = useCallback(async () => {
    const confirmText = workspaceId
      ? '清空当前作用域下的调试记录（仅影响本地 .debug 目录）？'
      : '清空当前 session 的调试记录（仅影响本地 .debug 目录）？'
    if (!window.confirm(confirmText)) return
    try {
      await llmTraceClear({ workspaceId: workspaceId ?? null, sessionId: sessionId ?? null })
      setEntries([])
    } catch (e) {
      setError(String(e))
    }
  }, [sessionId, workspaceId])

  const visibleEntries = useMemo(
    () => entries.filter(matchesCurrentSession),
    [entries, matchesCurrentSession],
  )
  const runningCount = useMemo(
    () => visibleEntries.filter((e) => e.status === 'running').length,
    [visibleEntries],
  )
  const hiddenBySessionFilter = entries.length - visibleEntries.length

  /**
   * 按 parentTraceId 构造树：
   * - 主→Pi 通常是根；主→子 如果父节点在可见集合里就挂到父节点下
   * - 若父节点不在可见集合（例如已被会话过滤掉），子节点提升为根
   */
  const treeOrdered = useMemo(() => {
    const visibleIds = new Set(visibleEntries.map((e) => e.id))
    const childrenByParent = new Map<string, LlmTraceEntry[]>()
    const roots: LlmTraceEntry[] = []
    for (const e of visibleEntries) {
      const parent = e.parentTraceId && visibleIds.has(e.parentTraceId) ? e.parentTraceId : null
      if (parent) {
        const arr = childrenByParent.get(parent) ?? []
        arr.push(e)
        childrenByParent.set(parent, arr)
      } else {
        roots.push(e)
      }
    }
    childrenByParent.forEach((arr) =>
      arr.sort((a, b) => a.startedAt - b.startedAt),
    )
    const out: Array<{ entry: LlmTraceEntry; depth: number; childCount: number }> = []
    const walk = (node: LlmTraceEntry, depth: number) => {
      const kids = childrenByParent.get(node.id) ?? []
      out.push({ entry: node, depth, childCount: kids.length })
      for (const kid of kids) walk(kid, depth + 1)
    }
    for (const root of roots) walk(root, 0)
    return out
  }, [visibleEntries])

  if (!open) return null

  const panelStyle: React.CSSProperties = standalone
    ? { inset: 0, width: '100%', height: '100%', borderRadius: 0, border: 0 }
    : {
        left: pos.x,
        top: pos.y,
        width: INITIAL_SIZE.width,
        height: INITIAL_SIZE.height,
      }

  return (
    <div
      className={`llm-trace-panel${standalone ? ' standalone' : ''}`}
      style={panelStyle}
      role="dialog"
      aria-label="LLM 调用链"
    >
      <header
        className="llm-trace-head"
        onMouseDown={standalone ? undefined : onMouseDown}
        style={standalone ? { cursor: 'default' } : undefined}
      >
        <div className="llm-trace-head-title">
          <AppIcon name="wrench" size={14} />
          <span>LLM 调用链</span>
          {runningCount > 0 ? (
            <span className="llm-trace-head-badge">{runningCount} 运行中</span>
          ) : null}
        </div>
        <div className="llm-trace-head-actions">
          {hasWorkspaceScope ? (
            <label className="llm-trace-switch" title="是否写入调试追踪">
              <input type="checkbox" checked={enabled} onChange={() => void toggle()} />
              <span>{enabled ? '记录中' : '未开启'}</span>
            </label>
          ) : hasSessionScope ? (
            <span className="llm-trace-switch llm-trace-switch-static">当前 session</span>
          ) : null}
          <button type="button" onClick={() => void refresh()} disabled={loading} title="刷新">
            <AppIcon name="refresh" size={13} />
          </button>
          <button type="button" onClick={() => void clear()} title="清空记录">
            <AppIcon name="trash" size={13} />
          </button>
          {!standalone ? (
            <button
              type="button"
              onClick={() => void openLlmTracePopout(workspaceId ?? null, sessionId ?? null)}
              title="在独立窗口打开"
            >
              <AppIcon name="panel" size={13} />
            </button>
          ) : null}
          {!standalone ? (
            <button type="button" onClick={onClose} title="关闭" className="llm-trace-close">
              <AppIcon name="close" size={13} />
            </button>
          ) : null}
        </div>
      </header>

      {!enabled && hasWorkspaceScope ? (
        <div className="llm-trace-hint">
          调试模式未开启：智能体→大模型、动作→大模型、智能体→智能体 的结构化调用链不会被记录。点击右上「未开启」开启即可。
        </div>
      ) : null}
      {!hasWorkspaceScope && hasSessionScope ? (
        <div className="llm-trace-hint">
          当前单独 session 的结构化调试记录会按 session 作用域实时刷新。
        </div>
      ) : null}
      {error ? <div className="llm-trace-error">{error}</div> : null}

      <div className="llm-trace-filterbar">
        {hasWorkspaceScope && sessionId ? (
          <label className="llm-trace-switch">
            <input
              type="checkbox"
              checked={sessionOnly}
              onChange={(e) => setSessionOnly(e.target.checked)}
            />
            <span>仅当前会话</span>
          </label>
        ) : (
          <span />
        )}
        <div className="llm-trace-view-toggle" role="tablist" aria-label="视图模式">
          <button
            type="button"
            className={viewMode === 'tree' ? 'active' : ''}
            onClick={() => setViewMode('tree')}
            title="按调用链聚合：主→Pi 作为根，派出的子 Agent 缩进为子节点"
          >
            树形
          </button>
          <button
            type="button"
            className={viewMode === 'flat' ? 'active' : ''}
            onClick={() => setViewMode('flat')}
            title="按时间倒序扁平展示"
          >
            扁平
          </button>
        </div>
        <span className="llm-trace-muted llm-trace-filter-meta">
          {sessionOnly && sessionId
            ? `${visibleEntries.length} 条 · 已隐藏 ${hiddenBySessionFilter}`
            : `全部 ${entries.length} 条`}
        </span>
      </div>

      <div className="llm-trace-list">
        {visibleEntries.length === 0 ? (
          <div className="llm-trace-empty-big">
            {loading
              ? '加载中…'
              : !enabled
                ? '尚未开启调试记录'
                : sessionOnly && sessionId
                  ? entries.length > 0
                    ? '当前会话暂无调用记录（总共 ' +
                      entries.length +
                      ' 条来自其他会话，可关闭「仅当前会话」查看）'
                    : '暂无调用记录。发一条消息试试？'
                  : '暂无调用记录。发一条消息试试？'}
          </div>
        ) : viewMode === 'tree' ? (
          treeOrdered.map(({ entry, depth, childCount }) => (
            <TraceRow
              key={entry.id}
              entry={entry}
              depth={depth}
              childCount={childCount}
              expanded={!!expanded[entry.id]}
              onToggle={() =>
                setExpanded((prev) => ({ ...prev, [entry.id]: !prev[entry.id] }))
              }
            />
          ))
        ) : (
          visibleEntries.map((entry) => (
            <TraceRow
              key={entry.id}
              entry={entry}
              expanded={!!expanded[entry.id]}
              onToggle={() =>
                setExpanded((prev) => ({ ...prev, [entry.id]: !prev[entry.id] }))
              }
            />
          ))
        )}
      </div>
    </div>
  )
}

export default LlmTracePanel
