import { useCallback, useEffect, useRef, useState } from 'react'
import type { ApprovalRequest } from '../../lib/piClient'
import { agentLoopRespondApproval } from '../../lib/piClient'

export function AgentApprovalDialog({
  request,
  onDone,
}: {
  request: ApprovalRequest
  onDone: () => void
}) {
  const [remaining, setRemaining] = useState(request.timeoutMs)
  const respondedRef = useRef(false)
  const startTimeRef = useRef(Date.now())

  useEffect(() => {
    startTimeRef.current = Date.now()
    setRemaining(request.timeoutMs)
    respondedRef.current = false
  }, [request])

  useEffect(() => {
    const interval = setInterval(() => {
      const elapsed = Date.now() - startTimeRef.current
      const left = Math.max(0, request.timeoutMs - elapsed)
      setRemaining(left)
      if (left <= 0 && !respondedRef.current) {
        respondedRef.current = true
        void agentLoopRespondApproval(request.loopId, false).catch(() => {})
        onDone()
      }
    }, 200)
    return () => clearInterval(interval)
  }, [request, onDone])

  const handleRespond = useCallback(
    (approved: boolean) => {
      if (respondedRef.current) return
      respondedRef.current = true
      void agentLoopRespondApproval(request.loopId, approved).catch(() => {})
      onDone()
    },
    [request.loopId, onDone],
  )

  const secondsLeft = Math.ceil(remaining / 1000)
  const progress = remaining / request.timeoutMs

  const riskLabel = (() => {
    switch (request.action.riskLevel) {
      case 'high':
        return '高风险'
      case 'medium':
        return '中风险'
      case 'low':
        return '低风险'
      default:
        return request.action.riskLevel
    }
  })()

  return (
    <div className="approval-dialog-overlay">
      <div className="approval-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="approval-dialog-header">
          <strong>操作审批请求</strong>
          <span className="approval-dialog-countdown">{secondsLeft}s</span>
        </div>

        <div className="approval-dialog-body">
          <div className="approval-dialog-info">
            <div className="approval-dialog-field">
              <span className="approval-dialog-label">智能体</span>
              <span className="approval-dialog-value">{request.action.agentId}</span>
            </div>
            <div className="approval-dialog-field">
              <span className="approval-dialog-label">操作类型</span>
              <span className="approval-dialog-value">
                {request.action.type === 'batch' ? '批量委派' : '单次委派'}
              </span>
            </div>
            <div className="approval-dialog-field">
              <span className="approval-dialog-label">任务描述</span>
              <span className="approval-dialog-value approval-dialog-task">
                {request.action.task}
              </span>
            </div>
            <div className="approval-dialog-field">
              <span className="approval-dialog-label">风险等级</span>
              <span
                className={`approval-dialog-risk approval-dialog-risk--${request.action.riskLevel}`}
              >
                {riskLabel}
              </span>
            </div>
            {request.action.reason && (
              <div className="approval-dialog-field">
                <span className="approval-dialog-label">原因</span>
                <span className="approval-dialog-value approval-dialog-reason">
                  {request.action.reason}
                </span>
              </div>
            )}
          </div>

          <div className="approval-dialog-progress">
            <div
              className="approval-dialog-progress-bar"
              style={{ width: `${progress * 100}%` }}
            />
          </div>

          <div className="approval-dialog-actions">
            <button
              type="button"
              className="outline-button danger"
              onClick={() => handleRespond(false)}
            >
              拒绝
            </button>
            <button
              type="button"
              className="outline-button"
              onClick={() => handleRespond(true)}
            >
              仅本次允许
            </button>
            <button
              type="button"
              className="outline-button primary"
              onClick={() => handleRespond(true)}
            >
              允许
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
