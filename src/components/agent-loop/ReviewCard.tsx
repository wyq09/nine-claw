type Props = {
  loopId: string
  reviewType: 'pause_for_review' | 'extend'
  info: Record<string, unknown>
  onRespond: (approved: boolean) => void
}

export function ReviewCard({ loopId, reviewType, info, onRespond }: Props) {
  if (reviewType === 'extend') {
    const reason = typeof info.reason === 'string' ? info.reason : ''
    return (
      <div className="review-card" data-loop-id={loopId}>
        <div className="review-card-title">{'⏸ 任务还没完成'}</div>
        {reason ? (
          <div className="review-card-body">{reason}</div>
        ) : null}
        <div className="review-card-body">{'是否继续？'}</div>
        <div className="review-card-actions">
          <button className="review-btn-reject" onClick={() => onRespond(false)}>
            {'到此为止'}
          </button>
          <button className="review-btn-approve" onClick={() => onRespond(true)}>
            {'继续执行'}
          </button>
        </div>
      </div>
    )
  }

  // pause_for_review
  return (
    <div className="review-card" data-loop-id={loopId}>
      <div className="review-card-title">{'⏸ 确认执行'}</div>
      <div className="review-card-actions">
        <button className="review-btn-reject" onClick={() => onRespond(false)}>
          {'取消'}
        </button>
        <button className="review-btn-approve" onClick={() => onRespond(true)}>
          {'确认'}
        </button>
      </div>
    </div>
  )
}
