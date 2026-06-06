import { useEffect, useState } from 'react'

const PREPARING_INDICATOR_DELAY_MS = 280

/** 前置逻辑（如任务编排 / 上下文整理）进行中：延迟展示，避免每次发送都闪出提示。 */
export function TurnPreparingIndicator() {
  const [visible, setVisible] = useState(false)

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setVisible(true)
    }, PREPARING_INDICATOR_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [])

  if (!visible) {
    return null
  }

  return (
    <div className="turn-preparing-indicator" role="status" aria-live="polite">
      <span className="turn-waiting-dots" aria-hidden="true">
        <span />
        <span />
        <span />
      </span>
      <span className="turn-waiting-label">准备本轮回复</span>
      <span className="turn-preparing-hint">正在整理上下文并启动本轮任务…</span>
    </div>
  )
}
