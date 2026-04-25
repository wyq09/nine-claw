import { useEffect, useState, useCallback } from 'react'
import type { AgentLoopSegment, AgentLoopIteration } from '../../types'
import {
  subscribeAgentLoopIterationStart,
  subscribeAgentLoopIterationEnd,
  subscribeAgentLoopReviewRequest,
  agentLoopAbort,
  agentLoopRespondReview,
} from '../../lib/piClient'
import type {
  AgentLoopIterationStartEvent,
  AgentLoopIterationEndEvent,
  AgentLoopReviewRequestEvent,
} from '../../lib/piClient'
import { AgentResultCard } from './AgentResultCard'
import { ReviewCard } from './ReviewCard'

type Props = {
  segment: AgentLoopSegment
}

type PendingReview = {
  reviewType: AgentLoopReviewRequestEvent['reviewType']
  info: Record<string, unknown>
}

export function AgentLoopBlock({ segment }: Props) {
  const [iterations, setIterations] = useState<AgentLoopIteration[]>(segment.iterations)
  const [detailExpanded, setDetailExpanded] = useState(false)
  const [pendingReview, setPendingReview] = useState<PendingReview | null>(null)

  const loopId = segment.loopId
  const isRunning = segment.status === 'running'
  const isCompleted =
    segment.status === 'completed' || segment.status === 'aborted' || segment.status === 'error'

  // Subscribe to live iteration events when the loop is running
  useEffect(() => {
    if (!isRunning) return

    const unsubs: (() => void)[] = []

    async function setup() {
      unsubs.push(
        await subscribeAgentLoopIterationStart((payload: AgentLoopIterationStartEvent) => {
          if (payload.loopId !== loopId) return
          setIterations((prev) => {
            // Avoid duplicate entries
            if (prev.some((it) => it.iteration === payload.iteration)) return prev
            const newIteration: AgentLoopIteration = {
              iteration: payload.iteration,
              markerType: payload.type === 'batch' ? 'batch' : 'call',
              status: 'running',
              ...(payload.type === 'call'
                ? {
                    delegate: {
                      agentId: payload.agentId ?? '',
                      agentName: payload.agentId ?? 'Agent',
                      task: payload.task ?? '',
                    },
                  }
                : {}),
              ...(payload.type === 'batch'
                ? {
                    batch: {
                      delegates: [],
                    },
                  }
                : {}),
            }
            return [...prev, newIteration]
          })
        }),
      )

      unsubs.push(
        await subscribeAgentLoopIterationEnd((payload: AgentLoopIterationEndEvent) => {
          if (payload.loopId !== loopId) return
          setIterations((prev) =>
            prev.map((it) => {
              if (it.iteration !== payload.iteration) return it
              return {
                ...it,
                status: 'completed',
                delegate: it.delegate
                  ? {
                      ...it.delegate,
                      durationMs: payload.durationMs,
                    }
                  : undefined,
              }
            }),
          )
        }),
      )

      unsubs.push(
        await subscribeAgentLoopReviewRequest((payload: AgentLoopReviewRequestEvent) => {
          if (payload.loopId !== loopId) return
          setPendingReview({
            reviewType: payload.reviewType,
            info: payload.info,
          })
        }),
      )
    }

    void setup()

    return () => {
      for (const un of unsubs) un()
    }
  }, [loopId, isRunning])

  // Sync iterations from prop when the parent updates them (completed/aborted segments)
  useEffect(() => {
    if (!isRunning) {
      setIterations(segment.iterations)
    }
  }, [segment.iterations, isRunning])

  const handleAbort = useCallback(() => {
    void agentLoopAbort(loopId)
  }, [loopId])

  const handleReviewRespond = useCallback(
    (approved: boolean) => {
      void agentLoopRespondReview(loopId, approved)
      setPendingReview(null)
    },
    [loopId],
  )

  const totalDurationMs = iterations.reduce((sum, it) => {
    const ms = it.delegate?.durationMs ?? it.batch?.delegates[0]?.durationMs
    return sum + (ms ?? 0)
  }, 0)

  const completedCount = iterations.filter((it) => it.status === 'completed').length

  return (
    <div className="agent-loop-block">
      <div className="agent-loop-header">
        <span className="agent-loop-status">
          {isRunning
            ? `Agent Loop 运行中 (${iterations.length})`
            : isCompleted
              ? `Agent Loop 完成 — ${completedCount}/${iterations.length} 次委派`
              : `Agent Loop`}
        </span>
        {isRunning ? (
          <button className="agent-loop-abort" onClick={handleAbort}>
            终止
          </button>
        ) : null}
        <button
          className="agent-loop-toggle"
          onClick={() => setDetailExpanded((prev) => !prev)}
        >
          {detailExpanded ? '收起详情' : '展开详情'}
        </button>
      </div>

      <div className="agent-loop-iterations">
        {iterations.map((it) => (
          <AgentResultCard
            key={`iteration-${it.iteration}`}
            iteration={it}
            expanded={detailExpanded}
          />
        ))}
      </div>

      {pendingReview ? (
        <ReviewCard
          loopId={loopId}
          reviewType={pendingReview.reviewType}
          info={pendingReview.info}
          onRespond={handleReviewRespond}
        />
      ) : null}

      {detailExpanded && isCompleted ? (
        <div className="agent-loop-stats">
          {`内部统计：${iterations.length} 次委派，总耗时 ${Math.round(totalDurationMs / 1000)}s`}
        </div>
      ) : null}
    </div>
  )
}
