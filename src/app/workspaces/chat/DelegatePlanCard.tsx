import { useEffect, useMemo, useState } from 'react'
import type { DelegatePlanItem, WorkspaceMemberView } from '../../../types'
import { workspaceListMembers } from '../../../lib/piClient'

export type DelegatePlanCardProps = {
  planId: string
  items: DelegatePlanItem[]
  /** 当前团队 id，用于取候选成员；空时禁用"换人"下拉 */
  workspaceId: string | null
  /** 回调：点"全部下发"时以已启用项逐条调用 */
  onDispatch: (plan: {
    planId: string
    items: Array<Required<Pick<DelegatePlanItem, 'assignee' | 'task'>>>
  }) => Promise<void> | void
  /** 回调：取消，前端可选择把卡片标记为已处理 */
  onCancel?: (planId: string) => void
}

/** 团队空间：主智能体计划卡，用户可编辑文案 / 换人 / 勾选后一键下发。 */
export function DelegatePlanCard({
  planId,
  items,
  workspaceId,
  onDispatch,
  onCancel,
}: DelegatePlanCardProps) {
  const [drafts, setDrafts] = useState<DelegatePlanItem[]>(() =>
    items.map((i) => ({
      assignee: i.assignee,
      task: i.task,
      reason: i.reason ?? '',
      enabled: i.enabled !== false,
    })),
  )
  const [dispatching, setDispatching] = useState(false)
  const [members, setMembers] = useState<WorkspaceMemberView[] | null>(null)
  const [dispatchError, setDispatchError] = useState<string | null>(null)

  // 为了让"非团队成员"项可以被高亮阻止，组件挂载后就自动拉一次成员名单
  useEffect(() => {
    if (!workspaceId) return
    let cancelled = false
    workspaceListMembers(workspaceId)
      .then((list) => {
        if (!cancelled) setMembers(list)
      })
      .catch(() => {
        if (!cancelled) setMembers([])
      })
    return () => {
      cancelled = true
    }
  }, [workspaceId])

  const memberIdSet = useMemo(() => {
    if (!members) return null
    return new Set(members.map((m) => m.agentId))
  }, [members])

  const invalidAssignees = useMemo(() => {
    if (!memberIdSet) return new Set<number>()
    const out = new Set<number>()
    drafts.forEach((d, idx) => {
      if (d.enabled === false) return
      const aid = d.assignee.trim()
      if (!aid || !memberIdSet.has(aid)) out.add(idx)
    })
    return out
  }, [drafts, memberIdSet])

  const enabledCount = useMemo(() => drafts.filter((d) => d.enabled !== false).length, [drafts])

  const ensureMembers = async () => {
    if (!workspaceId || members) return
    try {
      const list = await workspaceListMembers(workspaceId)
      setMembers(list)
    } catch {
      setMembers([])
    }
  }

  const updateDraft = (index: number, patch: Partial<DelegatePlanItem>) => {
    setDrafts((prev) => prev.map((item, i) => (i === index ? { ...item, ...patch } : item)))
    if (dispatchError) setDispatchError(null)
  }

  const handleDispatch = async () => {
    if (dispatching) return
    if (invalidAssignees.size > 0) {
      setDispatchError('存在不属于本团队的成员，请先修正后再下发。')
      return
    }
    setDispatching(true)
    setDispatchError(null)
    try {
      const actionable = drafts
        .filter((d) => d.enabled !== false && d.task.trim() && d.assignee.trim())
        .map((d) => ({ assignee: d.assignee.trim(), task: d.task.trim() }))
      await onDispatch({ planId, items: actionable })
    } finally {
      setDispatching(false)
    }
  }

  return (
    <div className="delegate-plan-card" data-plan-id={planId}>
      <header className="delegate-plan-card-header">
        <div className="delegate-plan-card-title">
          <span className="delegate-plan-card-icon" aria-hidden>
            📋
          </span>
          <span>委派计划</span>
          <span className="delegate-plan-card-count">{drafts.length} 项</span>
        </div>
        <p className="delegate-plan-card-hint">
          已启用 {enabledCount} 项，下发后每项会出现一张独立进度卡。
        </p>
      </header>

      <ul className="delegate-plan-items">
        {drafts.map((item, index) => (
          <li
            key={`${planId}-${index}`}
            className={`delegate-plan-item${item.enabled === false ? ' is-disabled' : ''}${
              invalidAssignees.has(index) ? ' is-invalid-assignee' : ''
            }`}
          >
            <label className="delegate-plan-item-toggle">
              <input
                type="checkbox"
                checked={item.enabled !== false}
                onChange={(e) => updateDraft(index, { enabled: e.target.checked })}
              />
              <span className="visually-hidden">启用此项</span>
            </label>
            <div className="delegate-plan-item-body">
              <div className="delegate-plan-item-row">
                <span className="delegate-plan-item-label">任务</span>
                <textarea
                  className="delegate-plan-item-task"
                  value={item.task}
                  rows={2}
                  onChange={(e) => updateDraft(index, { task: e.target.value })}
                  placeholder="描述要让该成员完成的具体任务"
                />
              </div>
              <div className="delegate-plan-item-row delegate-plan-item-row-meta">
                <label className="delegate-plan-item-assignee">
                  <span className="delegate-plan-item-label">执行成员</span>
                  <select
                    value={item.assignee}
                    onFocus={ensureMembers}
                    onChange={(e) => updateDraft(index, { assignee: e.target.value })}
                  >
                    <option value={item.assignee}>{item.assignee}</option>
                    {members
                      ?.filter((m) => m.agentId !== item.assignee)
                      .map((m) => (
                        <option key={m.agentId} value={m.agentId}>
                          {m.name} ({m.role === 'supervisor' ? '主' : '成员'})
                        </option>
                      ))}
                  </select>
                </label>
                {item.reason ? (
                  <span className="delegate-plan-item-reason" title={item.reason}>
                    理由：{item.reason}
                  </span>
                ) : null}
                {invalidAssignees.has(index) ? (
                  <span className="delegate-plan-item-invalid" title="该 agentId 不是本团队成员">
                    ⚠ 非团队成员
                  </span>
                ) : null}
              </div>
            </div>
          </li>
        ))}
      </ul>

      {dispatchError ? <div className="delegate-plan-card-error">{dispatchError}</div> : null}

      <footer className="delegate-plan-card-actions">
        {onCancel ? (
          <button
            type="button"
            className="delegate-plan-btn-secondary"
            onClick={() => onCancel(planId)}
            disabled={dispatching}
          >
            取消
          </button>
        ) : null}
        <button
          type="button"
          className="delegate-plan-btn-primary"
          onClick={() => void handleDispatch()}
          disabled={dispatching || enabledCount === 0 || invalidAssignees.size > 0}
        >
          {dispatching ? '下发中…' : `全部下发 (${enabledCount})`}
        </button>
      </footer>
    </div>
  )
}
