import { useEffect, useMemo, useState } from 'react'
import type {
  DelegationRunSegment,
  DelegationToolCallEntry,
  DelegationTurnEntry,
} from '../../../types'
import { AppIcon } from '../../../components/AppIcon'
import { AgentAvatar } from '../../../components/AgentAvatar'
import {
  subscribeWorkspaceDelegateChunk,
  subscribeWorkspaceDelegateTerminal,
  subscribeWorkspaceDelegateTool,
  subscribeWorkspaceDelegateTurn,
  workspaceAbortDelegate,
  workspaceAugmentDelegate,
} from '../../../lib/piClient'

export type DelegationCardProps = {
  workspaceId: string | null
  run: DelegationRunSegment
  onReassign?: (run: DelegationRunSegment) => void
  resolveSpeaker?: (agentId: string) => {
    name: string
    role?: 'supervisor' | 'member'
    accentColor?: string | null
    avatarUri?: string | null
    avatarEmoji?: string | null
  } | null
}

const STATUS_META: Record<
  DelegationRunSegment['status'],
  { label: string; className: string }
> = {
  pending: { label: '排队中', className: 'is-pending' },
  running: { label: '工作中', className: 'is-running' },
  done: { label: '已完成', className: 'is-done' },
  aborted: { label: '已中止', className: 'is-aborted' },
  error: { label: '出错', className: 'is-error' },
}

type LiveState = {
  turns: DelegationTurnEntry[]
  toolCalls: DelegationToolCallEntry[]
  output: string
  status: DelegationRunSegment['status']
  error: string | null
  elapsedMs?: number
}

function mergeToolCall(
  prev: DelegationToolCallEntry[],
  next: DelegationToolCallEntry,
): DelegationToolCallEntry[] {
  if (next.toolCallId) {
    const idx = prev.findIndex((t) => t.toolCallId === next.toolCallId)
    if (idx >= 0) {
      const merged = [...prev]
      merged[idx] = { ...merged[idx], ...next }
      return merged
    }
  }
  return [...prev, next]
}

/** 一个子任务的运行态卡片：显示状态 + 已用时 + 输出，右上三按钮 停止/补充/换人。 */
export function DelegationCard({ workspaceId, run, onReassign, resolveSpeaker }: DelegationCardProps) {
  const [live, setLive] = useState<LiveState>(() => ({
    turns: run.turns ?? [],
    toolCalls: run.toolCalls ?? [],
    output: run.output ?? '',
    status: run.status,
    error: run.error ?? null,
    elapsedMs: run.elapsedMs,
  }))

  useEffect(() => {
    setLive((prev) => ({
      turns: run.turns && run.turns.length > 0 ? run.turns : prev.turns,
      toolCalls:
        run.toolCalls && run.toolCalls.length > 0 ? run.toolCalls : prev.toolCalls,
      output: run.output || prev.output,
      status:
        // 以"更终态的状态"为准：已完成/出错/中止覆盖 running
        run.status === 'done' || run.status === 'error' || run.status === 'aborted'
          ? run.status
          : prev.status === 'done' || prev.status === 'error' || prev.status === 'aborted'
            ? prev.status
            : run.status,
      error: run.error ?? prev.error,
      elapsedMs: typeof run.elapsedMs === 'number' ? run.elapsedMs : prev.elapsedMs,
    }))
  }, [run.turns, run.toolCalls, run.output, run.status, run.error, run.elapsedMs])

  useEffect(() => {
    const runId = run.runId
    const unsubs: Array<() => void> = []

    const disposers: Array<Promise<() => void>> = [
      subscribeWorkspaceDelegateTurn((payload) => {
        if (payload.runId !== runId) return
        setLive((prev) => {
          if (prev.turns.some((t) => t.index === payload.turnIndex)) return prev
          return {
            ...prev,
            status: prev.status === 'done' || prev.status === 'error' ? prev.status : 'running',
            turns: [
              ...prev.turns,
              {
                index: payload.turnIndex,
                kind: payload.kind,
                summary: payload.summary,
              },
            ].sort((a, b) => a.index - b.index),
          }
        })
      }),
      subscribeWorkspaceDelegateTool((payload) => {
        if (payload.runId !== runId) return
        setLive((prev) => ({
          ...prev,
          status: prev.status === 'done' || prev.status === 'error' ? prev.status : 'running',
          toolCalls: mergeToolCall(prev.toolCalls, {
            index: payload.toolIndex,
            toolCallId: payload.toolCallId ?? '',
            toolName: payload.toolName,
            argsDigest: payload.argsDigest,
            status: payload.status,
            isError: payload.isError,
          }),
        }))
      }),
      subscribeWorkspaceDelegateChunk((payload) => {
        if (payload.runId !== runId) return
        setLive((prev) => ({
          ...prev,
          status: prev.status === 'done' || prev.status === 'error' ? prev.status : 'running',
          output: prev.output + payload.deltaText,
        }))
      }),
      subscribeWorkspaceDelegateTerminal((payload) => {
        if (payload.runId !== runId) return
        setLive((prev) => ({
          ...prev,
          status: payload.status,
          output: payload.output ?? prev.output,
          error: payload.error ?? prev.error,
          elapsedMs: payload.elapsedMs ?? prev.elapsedMs,
          toolCalls: prev.toolCalls.map((t) =>
            t.status === 'running' ? { ...t, status: 'done' } : t,
          ),
        }))
      }),
    ]

    let cancelled = false
    Promise.all(disposers)
      .then((fns) => {
        if (cancelled) {
          fns.forEach((fn) => fn())
          return
        }
        unsubs.push(...fns)
      })
      .catch(() => {
        // 订阅失败不中断 UI；卡片只是失去实时性
      })

    return () => {
      cancelled = true
      unsubs.forEach((fn) => {
        try {
          fn()
        } catch {
          /* noop */
        }
      })
    }
  }, [run.runId])

  const meta = STATUS_META[live.status] ?? STATUS_META.pending
  const assigneeInfo = useMemo(
    () => resolveSpeaker?.(run.assignee) ?? null,
    [resolveSpeaker, run.assignee],
  )
  const displayName = assigneeInfo?.name ?? run.assignee
  const accent = assigneeInfo?.accentColor ?? null
  const [outputExpanded, setOutputExpanded] = useState(false)
  const [augmentOpen, setAugmentOpen] = useState(false)
  const [augmentText, setAugmentText] = useState('')
  const [augmentBusy, setAugmentBusy] = useState(false)
  const [abortBusy, setAbortBusy] = useState(false)
  const [liveExpanded, setLiveExpanded] = useState(false)

  const output = live.output
  const lines = output.split('\n')
  const needsCollapse = lines.length > 12
  const visibleOutput = outputExpanded || !needsCollapse ? output : lines.slice(0, 12).join('\n')

  const handleAbort = async () => {
    if (abortBusy) return
    setAbortBusy(true)
    try {
      await workspaceAbortDelegate(run.runId)
    } finally {
      setAbortBusy(false)
    }
  }

  const handleAugment = async () => {
    const text = augmentText.trim()
    if (!text || !workspaceId || augmentBusy) return
    setAugmentBusy(true)
    try {
      await workspaceAugmentDelegate({ workspaceId, runId: run.runId, note: text })
      setAugmentText('')
      setAugmentOpen(false)
    } finally {
      setAugmentBusy(false)
    }
  }

  const canStop = live.status === 'running' || live.status === 'pending'
  const hasLiveTrace = live.toolCalls.length > 0 || live.turns.length > 0
  const actionCount = live.toolCalls.length + live.turns.length

  return (
    <div className={`delegation-card ${meta.className}`} data-run-id={run.runId}>
      <header className="delegation-card-header">
        <div className="delegation-card-identity">
          <AgentAvatar
            name={displayName}
            avatarUri={assigneeInfo?.avatarUri}
            accentColor={accent}
            className="delegation-card-avatar"
            size={18}
          />
          <div className="delegation-card-identity-text">
            <div className="delegation-card-name-row">
              <span className="delegation-card-assignee-name">{displayName}</span>
              <span className={`delegation-card-status-pill ${meta.className}`}>{meta.label}</span>
            </div>
            <div className="delegation-card-id-meta" title={run.assignee}>
              {assigneeInfo?.role === 'supervisor' ? <span className="delegation-card-role">主</span> : null}
              {assigneeInfo?.role === 'member' ? <span className="delegation-card-role">成员</span> : null}
              <code className="delegation-card-agent-id">{run.assignee}</code>
              {typeof live.elapsedMs === 'number' ? (
                <span className="delegation-card-elapsed">· {(live.elapsedMs / 1000).toFixed(1)}s</span>
              ) : null}
            </div>
          </div>
        </div>
        <div className="delegation-card-actions">
          {output ? (
            <button
              type="button"
              className="delegation-card-result-link"
              onClick={() => setOutputExpanded((v) => !v)}
              title={outputExpanded ? '收起结果' : '查看结果'}
            >
              结果 <span aria-hidden>›</span>
            </button>
          ) : null}
          <button
            type="button"
            className="delegation-card-icon-btn"
            disabled={!canStop || abortBusy}
            onClick={() => void handleAbort()}
            title="停止"
            aria-label="停止"
          >
            <AppIcon name="stop" size={16} />
          </button>
          <button
            type="button"
            className="delegation-card-icon-btn delegation-card-chevron-btn"
            onClick={() => setLiveExpanded((v) => !v)}
            title={liveExpanded ? '收起运行细节' : '展开运行细节'}
            aria-label={liveExpanded ? '收起运行细节' : '展开运行细节'}
            aria-expanded={liveExpanded}
          >
            <AppIcon name="chevron-down" size={16} />
          </button>
          <button
            type="button"
            className="delegation-card-btn"
            onClick={() => setAugmentOpen((v) => !v)}
            title="追加补充说明"
          >
            追加
          </button>
          {onReassign ? (
            <button
              type="button"
              className="delegation-card-btn"
              onClick={() => onReassign(run)}
              title="换人重跑"
            >
              换人
            </button>
          ) : null}
        </div>
      </header>

      <div className="delegation-card-task">{run.task}</div>

      {hasLiveTrace ? (
        <div className="delegation-card-live">
          <button
            type="button"
            className="delegation-card-live-toggle"
            onClick={() => setLiveExpanded((v) => !v)}
            aria-expanded={liveExpanded}
          >
            <span className="delegation-card-live-caret" aria-hidden>
              {liveExpanded ? '▾' : '▸'}
            </span>
            <span>
              {live.toolCalls.length} 次工具调用 · 思考 {live.turns.length} 轮
            </span>
            {live.status === 'running' ? (
              <span className="delegation-card-live-dot" aria-hidden />
            ) : null}
          </button>
          {liveExpanded ? (
            <ul className="delegation-card-live-list">
              {live.toolCalls.map((tc) => (
                <li
                  key={`tool-${tc.index}-${tc.toolCallId || tc.toolName}`}
                  className={`delegation-card-live-item is-${tc.status}`}
                >
                  <span className="delegation-card-live-item-label">
                    <code>{tc.toolName || 'tool'}</code>
                  </span>
                  {tc.argsDigest ? (
                    <span className="delegation-card-live-item-args" title={tc.argsDigest}>
                      {tc.argsDigest}
                    </span>
                  ) : null}
                  <span className="delegation-card-live-item-status">
                    {tc.status === 'running'
                      ? '进行中'
                      : tc.status === 'error' || tc.isError
                        ? '出错'
                        : '完成'}
                  </span>
                </li>
              ))}
              {live.turns.map((turn) => (
                <li
                  key={`turn-${turn.index}-${turn.kind}`}
                  className="delegation-card-live-item is-turn"
                >
                  <span className="delegation-card-live-item-label">
                    {turn.kind === 'agent' ? '汇总回合' : '思考回合'} #{turn.index + 1}
                  </span>
                  {turn.summary ? (
                    <span className="delegation-card-live-item-args">{turn.summary}</span>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}

      {augmentOpen ? (
        <div className="delegation-card-augment">
          <textarea
            value={augmentText}
            onChange={(e) => setAugmentText(e.target.value)}
            placeholder="追加一条补充说明（会通过 USER_NOTES 注入下一轮上下文）"
            rows={2}
          />
          <div className="delegation-card-augment-actions">
            <button
              type="button"
              className="delegate-plan-btn-secondary"
              onClick={() => setAugmentOpen(false)}
              disabled={augmentBusy}
            >
              取消
            </button>
            <button
              type="button"
              className="delegate-plan-btn-primary"
              onClick={() => void handleAugment()}
              disabled={augmentBusy || !augmentText.trim()}
            >
              {augmentBusy ? '提交中…' : '提交补充'}
            </button>
          </div>
        </div>
      ) : null}

      {output ? (
        <div className="delegation-card-output">
          <pre>{visibleOutput}</pre>
          {needsCollapse ? (
            <button
              type="button"
              className="delegation-card-expand"
              onClick={() => setOutputExpanded((v) => !v)}
            >
              {outputExpanded ? '收起' : `展开 (${lines.length} 行)`}
            </button>
          ) : null}
        </div>
      ) : null}

      {live.error ? <div className="delegation-card-error">{live.error}</div> : null}

      {live.status === 'running' || live.status === 'pending' ? (
        <div className="delegation-card-processing" role="status" aria-live="polite">
          <AgentAvatar
            name={displayName}
            avatarUri={assigneeInfo?.avatarUri}
            accentColor={accent}
            className="delegation-card-processing-avatar"
            size={16}
          />
          <strong>{displayName}</strong>
          <span>{live.status === 'pending' ? '排队中' : '正在处理'}</span>
          {actionCount > 0 ? <span className="delegation-card-processing-meta">{actionCount} 个动作</span> : null}
        </div>
      ) : null}
    </div>
  )
}
