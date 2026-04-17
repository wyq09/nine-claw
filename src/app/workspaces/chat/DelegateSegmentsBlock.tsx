import { createContext, useContext } from 'react'
import type { DelegatePlanItem, DelegationRunSegment, ResponseSegment } from '../../../types'
import { DelegatePlanCard } from './DelegatePlanCard'
import { DelegationCard } from './DelegationCard'

export type DelegateSegmentsContextValue = {
  workspaceId: string | null
  /** 从计划卡"全部下发"→ 真正执行某一项；父级在此调用 `workspace_run_delegate_task`。 */
  onDispatchPlan?: (payload: {
    planId: string
    items: Array<{ assignee: string; task: string }>
  }) => Promise<void> | void
  /** 运行卡"换人"→ 父级决定如何重启一次委派。 */
  onReassignRun?: (run: DelegationRunSegment) => void
}

/** 没有父级提供上下文时的降级实现：控制台提示 + 不崩溃。 */
const DEFAULT_CONTEXT: DelegateSegmentsContextValue = {
  workspaceId: null,
  onDispatchPlan: async () => {
    console.warn('[DelegateSegmentsBlock] 未接入 onDispatchPlan；计划下发被忽略')
  },
  onReassignRun: () => {
    console.warn('[DelegateSegmentsBlock] 未接入 onReassignRun；换人被忽略')
  },
}

export const DelegateSegmentsContext = createContext<DelegateSegmentsContextValue>(DEFAULT_CONTEXT)

export type DelegateSegmentsBlockProps = {
  /** 仅包含 `delegate_plan` / `delegation_run` 类型 */
  segments: Extract<ResponseSegment, { type: 'delegate_plan' | 'delegation_run' }>[]
  turnId: string
  resolveSpeaker?: (agentId: string) => {
    name: string
    role?: 'supervisor' | 'member'
    accentColor?: string | null
    avatarEmoji?: string | null
  } | null
}

export function DelegateSegmentsBlock({ segments, turnId, resolveSpeaker }: DelegateSegmentsBlockProps) {
  const ctx = useContext(DelegateSegmentsContext)
  if (segments.length === 0) return null
  return (
    <div className="delegate-segments-block" data-turn-id={turnId}>
      {segments.map((segment, index) => {
        if (segment.type === 'delegate_plan') {
          return (
            <DelegatePlanCard
              key={`plan-${segment.planId}-${index}`}
              planId={segment.planId}
              items={segment.items as DelegatePlanItem[]}
              workspaceId={ctx.workspaceId}
              onDispatch={async (p) => {
                await ctx.onDispatchPlan?.(p)
              }}
            />
          )
        }
        return (
          <DelegationCard
            key={`run-${segment.run.runId}-${index}`}
            workspaceId={ctx.workspaceId}
            run={segment.run}
            onReassign={ctx.onReassignRun}
            resolveSpeaker={resolveSpeaker}
          />
        )
      })}
    </div>
  )
}
