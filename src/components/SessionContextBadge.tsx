import { useState } from 'react'
import {
  formatContextUsageRingLabel,
  getStageColor,
  type SessionContextState,
} from '../app/lib/sessionContext'
import { SessionContextPopover } from './SessionContextPopover'

type SessionContextBadgeProps = {
  state: SessionContextState | null
  loading: boolean
}

/** 原 36px，整体缩小约 30% */
const RING_SIZE = 25
const STROKE = 2
const R = (RING_SIZE - STROKE) / 2
const CX = RING_SIZE / 2
const CY = RING_SIZE / 2
const CIRC = 2 * Math.PI * R

function ContextUsageRing({
  percent,
  colorClass,
  label,
}: {
  percent: number
  colorClass: string
  label: string
}) {
  const p = Math.min(Math.max(percent, 0), 100)
  const dash = (p / 100) * CIRC
  return (
    <svg
      className={`context-badge-ring context-badge-ring--${colorClass}`}
      width={RING_SIZE}
      height={RING_SIZE}
      viewBox={`0 0 ${RING_SIZE} ${RING_SIZE}`}
      aria-hidden
    >
      <circle
        className="context-badge-ring-track"
        cx={CX}
        cy={CY}
        r={R}
        fill="none"
        strokeWidth={STROKE}
      />
      <circle
        className="context-badge-ring-progress"
        cx={CX}
        cy={CY}
        r={R}
        fill="none"
        strokeWidth={STROKE}
        strokeLinecap="round"
        strokeDasharray={`${dash} ${CIRC}`}
        transform={`rotate(-90 ${CX} ${CY})`}
      />
      <text
        className="context-badge-ring-label"
        x={CX}
        y={CY}
        textAnchor="middle"
        dominantBaseline="central"
      >
        {label}
      </text>
    </svg>
  )
}

export function SessionContextBadge({ state, loading }: SessionContextBadgeProps) {
  const [showPopover, setShowPopover] = useState(false)

  if (loading && !state) {
    return (
      <span className="context-badge-wrap" aria-busy="true" aria-label="正在读取上下文占用">
        <ContextUsageRing percent={0} colorClass="loading" label="..." />
      </span>
    )
  }

  if (!state || state.percent === undefined) {
    return (
      <span className="context-badge-wrap" aria-label="上下文占用未知">
        <ContextUsageRing percent={0} colorClass="unknown" label="--" />
      </span>
    )
  }

  const colorClass = getStageColor(state.stage)
  const label = formatContextUsageRingLabel(state.percent)

  return (
    <span
      className="context-badge-wrap"
      aria-label={`上下文占用约 ${label}`}
      onMouseEnter={() => setShowPopover(true)}
      onMouseLeave={() => setShowPopover(false)}
    >
      <ContextUsageRing percent={state.percent} colorClass={colorClass} label={label} />
      {showPopover && <SessionContextPopover state={state} />}
    </span>
  )
}
