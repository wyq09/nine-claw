import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { RefObject } from 'react'
import type { ConversationTurn } from '../../types'

/** 进入会话时先只挂载最近若干条，避免长 history 一次性参与协调与虚拟列表测量 */
const INITIAL_VISIBLE = 4
const WARM_VISIBLE = 10
const LOAD_MORE_BATCH = 35

export function useChatTurnWindow(
  fullTurns: ConversationTurn[],
  activeHistoryId: string,
  scrollParentRef: RefObject<HTMLDivElement | null>,
) {
  const total = fullTurns.length
  const [loadedCount, setLoadedCount] = useState(() => Math.min(INITIAL_VISIBLE, total))
  const pendingScrollRestoreRef = useRef<{ scrollHeight: number; scrollTop: number } | null>(null)
  const pagingRef = useRef(false)
  const warmupRafRef = useRef<number | null>(null)

  /** 用 layout 阶段重置，避免切换会话首帧仍沿用上一会话的 loadedCount，导致虚拟列表滚底错位 */
  useLayoutEffect(() => {
    pendingScrollRestoreRef.current = null
    pagingRef.current = false
    if (warmupRafRef.current !== null) {
      cancelAnimationFrame(warmupRafRef.current)
      warmupRafRef.current = null
    }
    // Session switches must reset the virtualization window before the first paint.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setLoadedCount(Math.min(INITIAL_VISIBLE, fullTurns.length))
  }, [activeHistoryId, fullTurns.length])

  useEffect(() => {
    if (fullTurns.length <= INITIAL_VISIBLE) {
      return
    }
    warmupRafRef.current = requestAnimationFrame(() => {
      warmupRafRef.current = null
      setLoadedCount((current) => Math.min(fullTurns.length, Math.max(current, WARM_VISIBLE)))
    })
    return () => {
      if (warmupRafRef.current !== null) {
        cancelAnimationFrame(warmupRafRef.current)
        warmupRafRef.current = null
      }
    }
  }, [activeHistoryId, fullTurns.length])

  useEffect(() => {
    const t = fullTurns.length
    // Keep the window from staying at 0 while turns stream into the same session.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setLoadedCount((c) => {
      if (t === 0) {
        return 0
      }
      /* 避免 loadedCount 卡在 0：同一会话从 0 条变为 ≥1 条时 activeHistoryId 不变，旧逻辑 min(0,t) 永远为 0，虚拟列表无行可滚 */
      const floor = Math.min(WARM_VISIBLE, t)
      return Math.min(Math.max(c, floor), t)
    })
  }, [fullTurns.length])

  const visibleRangeStart = Math.max(0, total - loadedCount)
  const visibleTurns = useMemo(() => fullTurns.slice(visibleRangeStart), [fullTurns, visibleRangeStart])
  const hasMoreAbove = visibleRangeStart > 0

  const loadMoreAbove = useCallback(() => {
    if (visibleRangeStart <= 0 || pagingRef.current) {
      return
    }
    const el = scrollParentRef.current
    if (el) {
      pendingScrollRestoreRef.current = {
        scrollHeight: el.scrollHeight,
        scrollTop: el.scrollTop,
      }
    }
    pagingRef.current = true
    setLoadedCount((c) => Math.min(total, c + LOAD_MORE_BATCH))
  }, [visibleRangeStart, total, scrollParentRef])

  useLayoutEffect(() => {
    const pending = pendingScrollRestoreRef.current
    if (!pending) {
      return
    }
    const el = scrollParentRef.current
    if (!el) {
      pendingScrollRestoreRef.current = null
      pagingRef.current = false
      return
    }
    const delta = el.scrollHeight - pending.scrollHeight
    if (delta !== 0) {
      el.scrollTop = pending.scrollTop + delta
    }
    pendingScrollRestoreRef.current = null
    pagingRef.current = false
  }, [visibleTurns.length, loadedCount, scrollParentRef])

  return { visibleTurns, visibleRangeStart, hasMoreAbove, loadMoreAbove }
}
