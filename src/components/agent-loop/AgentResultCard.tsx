import { useState, useEffect } from 'react'
import type { AgentLoopIteration } from '../../types'

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

export function AgentResultCard({ iteration, expanded }: Props) {
  const [detailOpen, setDetailOpen] = useState(false)

  // Sync with parent's detail toggle
  useEffect(() => {
    setDetailOpen(expanded)
  }, [expanded])
  const { iteration: idx, status, delegate, batch } = iteration

  // Batch mode
  if (batch) {
    return (
      <div className="agent-result-card" data-status={status}>
        <div className="agent-result-header">
          <span className="agent-result-number">#{idx}</span>
          <span className="agent-result-name">
            [{batch.delegates.length} 个 Agent 并发]
          </span>
          <span className="agent-result-status">
            {status === 'running' ? '⏳' : status === 'completed' ? '✅' : status === 'reviewing' ? '⏳' : '⏳'}
          </span>
          {status === 'completed' && batch.delegates[0]?.durationMs != null && (
            <span className="agent-result-duration">
              {formatDuration(batch.delegates[0].durationMs)}
            </span>
          )}
        </div>
        <div className="agent-result-detail">
          {batch.delegates.map((d, i) => (
            <div key={i} className="agent-result-batch-item">
              <span className="agent-result-name">{d.agentName}</span>
              <span className="agent-result-status">
                {d.status === 'running'
                  ? '⏳'
                  : d.status === 'completed'
                    ? '✅'
                    : d.status === 'error'
                      ? '❌'
                      : '⏳'}
              </span>
            </div>
          ))}
        </div>
      </div>
    )
  }

  // Running mode
  if (status === 'running') {
    const task = delegate?.task ?? ''
    return (
      <div className="agent-result-card" data-status="running">
        <div className="agent-result-header">
          <span className="agent-result-number">#{idx}</span>
          <span className="agent-result-name">{delegate?.agentName ?? 'Agent'}</span>
          <span className="agent-result-status">{'⏳'}</span>
        </div>
        <div className="agent-result-streaming">
          <span>{task}</span>
          <span className="agent-result-cursor">{'▌'}</span>
        </div>
      </div>
    )
  }

  // Completed mode
  const output = delegate?.output ?? ''
  const summary = output.length > 100 ? output.slice(0, 100) + '...' : output
  const showExpandToggle = output.length > 100

  return (
    <div className="agent-result-card" data-status={status}>
      <div className="agent-result-header">
        <span className="agent-result-number">#{idx}</span>
        <span className="agent-result-name">{delegate?.agentName ?? 'Agent'}</span>
        <span className="agent-result-status">
          {status === 'completed' ? '✅' : status === 'reviewing' ? '⏳' : '⏳'}
        </span>
        {delegate?.durationMs != null && (
          <span className="agent-result-duration">
            {formatDuration(delegate.durationMs)}
          </span>
        )}
      </div>
      {output ? (
        <div className="agent-result-summary">
          {detailOpen ? (
            <pre className="agent-result-detail">{output}</pre>
          ) : (
            <span>{summary}</span>
          )}
        </div>
      ) : null}
      {showExpandToggle ? (
        <button
          className="agent-result-expand"
          onClick={() => setDetailOpen((prev) => !prev)}
        >
          {detailOpen ? '收起' : '展开'}
        </button>
      ) : null}
    </div>
  )
}
