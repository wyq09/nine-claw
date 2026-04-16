import {
  CONTEXT_THRESHOLDS,
  formatBadgePercent,
  getContextStageLabel,
  type SessionContextState,
} from '../app/lib/sessionContext'

type SessionContextPopoverProps = {
  state: SessionContextState | null
}

const THRESHOLD_ROWS = [
  {
    value: CONTEXT_THRESHOLDS.snip,
    name: 'Snip',
    dotClass: 'context-popover-dot--snip',
    lineClass: 'context-popover-threshold-line--snip',
  },
  {
    value: CONTEXT_THRESHOLDS.compact,
    name: 'Compact',
    dotClass: 'context-popover-dot--compact',
    lineClass: 'context-popover-threshold-line--compact',
  },
  {
    value: CONTEXT_THRESHOLDS.collapse,
    name: 'Collapse',
    dotClass: 'context-popover-dot--collapse',
    lineClass: 'context-popover-threshold-line--collapse',
  },
  {
    value: CONTEXT_THRESHOLDS.auto_compact,
    name: 'Auto Compact',
    dotClass: 'context-popover-dot--auto',
    lineClass: 'context-popover-threshold-line--auto',
  },
] as const

function formatTokenInt(n: number): string {
  return n.toLocaleString()
}

export function SessionContextPopover({ state }: SessionContextPopoverProps) {
  if (!state) {
    return (
      <div className="context-popover" role="tooltip">
        <div className="context-popover-surface">
          <div className="context-popover-title">上下文占用</div>
          <div className="context-popover-unknown">未知</div>
        </div>
        <span className="context-popover-caret" aria-hidden />
      </div>
    )
  }

  const percentDisplay = state.percent !== undefined ? formatBadgePercent(state.percent) : '--'
  const stageLabel = getContextStageLabel(state.stage)

  return (
    <div className="context-popover" role="tooltip">
      <div className="context-popover-surface">
        <div className="context-popover-title">上下文占用</div>

        <div className="context-popover-progress">
          <div
            className="context-popover-progress-bar"
            style={{ width: `${Math.min(state.percent ?? 0, 100)}%` }}
          />
        </div>

        <div className="context-popover-status-row">
          <span className="context-popover-percent">{percentDisplay}</span>
          <span className={`context-popover-stage context-popover-stage--${state.stage}`}>{stageLabel}</span>
        </div>

        <div className="context-popover-thresholds" aria-label="上下文阈值">
          {THRESHOLD_ROWS.map((item) => (
            <div key={item.name} className="context-popover-threshold-item">
              <span className={`context-popover-dot ${item.dotClass}`} aria-hidden />
              <span className={`context-popover-threshold-line ${item.lineClass}`}>
                <span className="context-popover-threshold-pct">{item.value}%</span>
                <span className="context-popover-threshold-name">{item.name}</span>
              </span>
            </div>
          ))}
        </div>

        <div className="context-popover-token-divider" role="separator" />

        <div className="context-popover-tokens">
          <div className="context-popover-token-row">
            <span className="context-popover-token-label">输入</span>
            <span className="context-popover-token-value">{formatTokenInt(state.inputTokens)} tokens</span>
          </div>
          <div className="context-popover-token-row">
            <span className="context-popover-token-label">输出</span>
            <span className="context-popover-token-value">{formatTokenInt(state.outputTokens)} tokens</span>
          </div>
        </div>
      </div>
      <span className="context-popover-caret" aria-hidden />
    </div>
  )
}
