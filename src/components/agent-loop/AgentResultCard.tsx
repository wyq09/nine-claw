import { useEffect, useState } from 'react'
import type { AgentLoopIteration } from '../../types'
import { AgentAvatar } from '../AgentAvatar'

type Props = {
  iteration: AgentLoopIteration
  expanded: boolean
}

function formatDuration(ms?: number): string {
  if (ms == null) return ''
  const seconds = Math.round(ms / 1000)
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  const remainder = seconds % 60
  return `${minutes}m${remainder}s`
}

function statusLabel(status: string): string {
  switch (status) {
    case 'completed':
      return '已完成'
    case 'error':
      return '出错'
    case 'cancelled':
      return '取消'
    case 'reviewing':
      return '处理中'
    case 'running':
      return '进行中'
    default:
      return '等待中'
  }
}

export function AgentResultCard({ iteration, expanded }: Props) {
  const [detailOpen, setDetailOpen] = useState(false)

  useEffect(() => {
    setDetailOpen(expanded)
  }, [expanded])

  const { iteration: idx, status, delegate, batch } = iteration

  if (batch) {
    return (
      <div className="agent-result-card is-batch" data-status={status}>
        <div className="agent-result-shell">
          <div className="agent-result-header">
            <div className="agent-result-identity">
              <span className="agent-result-number">#{idx}</span>
              <div className="agent-result-identity-text">
                <div className="agent-result-name-row">
                  <span className="agent-result-name">并发子智能体</span>
                  <span className={`agent-result-status-pill is-${status}`}>
                    {statusLabel(status)}
                  </span>
                </div>
                <div className="agent-result-meta">
                  {batch.delegates.length} 个 Agent 并发
                  {status === 'completed' && batch.delegates[0]?.durationMs != null ? (
                    <span className="agent-result-duration">
                      · {formatDuration(batch.delegates[0].durationMs)}
                    </span>
                  ) : null}
                </div>
              </div>
            </div>
          </div>
          <div className="agent-result-batch-list">
            {batch.delegates.map((d, i) => (
              <div key={`${d.agentId}-${i}`} className={`agent-result-batch-item is-${d.status}`}>
                <AgentAvatar name={d.agentName} className="agent-result-avatar" size={16} />
                <div className="agent-result-batch-copy">
                  <div className="agent-result-batch-head">
                    <span className="agent-result-batch-name">{d.agentName}</span>
                    <span className={`agent-result-mini-pill is-${d.status}`}>
                      {statusLabel(d.status)}
                    </span>
                  </div>
                  {d.task ? <div className="agent-result-batch-task">{d.task}</div> : null}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    )
  }

  if (status === 'running') {
    const task = delegate?.task ?? ''
    return (
      <div className="agent-result-card" data-status="running">
        <div className="agent-result-shell">
          <div className="agent-result-header">
            <div className="agent-result-identity">
              <AgentAvatar name={delegate?.agentName ?? 'Agent'} className="agent-result-avatar" size={18} />
              <div className="agent-result-identity-text">
                <div className="agent-result-name-row">
                  <span className="agent-result-name">{delegate?.agentName ?? 'Agent'}</span>
                  <span className="agent-result-status-pill is-running">进行中</span>
                </div>
                <div className="agent-result-meta">
                  <span className="agent-result-number">#{idx}</span>
                  {delegate?.agentId ? (
                    <span className="agent-result-agent-id">{delegate.agentId}</span>
                  ) : null}
                </div>
              </div>
            </div>
          </div>
          <div className="agent-result-task-block">
            <div className="agent-result-task-label">任务</div>
            <div className="agent-result-streaming">
              <span>{task}</span>
              <span className="agent-result-cursor">▌</span>
            </div>
          </div>
        </div>
      </div>
    )
  }

  const output = delegate?.output ?? ''
  const summary = output.length > 160 ? output.slice(0, 160) + '...' : output
  const showExpandToggle = output.length > 160

  return (
    <div className="agent-result-card" data-status={status}>
      <div className="agent-result-shell">
        <div className="agent-result-header">
          <div className="agent-result-identity">
            <AgentAvatar name={delegate?.agentName ?? 'Agent'} className="agent-result-avatar" size={18} />
            <div className="agent-result-identity-text">
              <div className="agent-result-name-row">
                <span className="agent-result-name">{delegate?.agentName ?? 'Agent'}</span>
                <span className={`agent-result-status-pill is-${status}`}>
                  {statusLabel(status)}
                </span>
              </div>
              <div className="agent-result-meta">
                <span className="agent-result-number">#{idx}</span>
                {delegate?.agentId ? (
                  <span className="agent-result-agent-id">{delegate.agentId}</span>
                ) : null}
                {delegate?.durationMs != null ? (
                  <span className="agent-result-duration">
                    · {formatDuration(delegate.durationMs)}
                  </span>
                ) : null}
              </div>
            </div>
          </div>
          {showExpandToggle ? (
            <button
              type="button"
              className="agent-result-expand"
              onClick={() => setDetailOpen((prev) => !prev)}
            >
              {detailOpen ? '收起' : '结果'}
            </button>
          ) : null}
        </div>
        {delegate?.task ? (
          <div className="agent-result-task-block">
            <div className="agent-result-task-label">任务</div>
            <div className="agent-result-task-copy">{delegate.task}</div>
          </div>
        ) : null}
        {output ? (
          <div className="agent-result-summary">
            <div className="agent-result-task-label">结果</div>
            {detailOpen ? (
              <pre className="agent-result-detail">{output}</pre>
            ) : (
              <span>{summary}</span>
            )}
          </div>
        ) : null}
      </div>
      {!showExpandToggle && output ? (
        <div className="agent-result-footer-hint">结果已完整展示</div>
      ) : null}
      {showExpandToggle && !detailOpen ? (
        <div className="agent-result-footer-hint">点击“结果”查看完整输出</div>
      ) : null}
    </div>
  )
}
