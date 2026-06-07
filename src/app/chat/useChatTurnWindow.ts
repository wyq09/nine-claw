import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { RefObject } from 'react'
import type { ConversationTurn } from '../../types'

/** 进入会话时先只挂载最近若干条，避免长 history 一次性参与协调与虚拟列表测量 */
const INITIAL_VISIBLE = 4
const WARM_VISIBLE = 10
const LOAD_MORE_BATCH = 35

type WindowState = {
  sessionId: string
  loadedCount: number
}

export function useChatTurnWindow(
  fullTurns: ConversationTurn[],
  activeHistoryId: string,
  scrollParentRef: RefObject<HTMLDivElement | null>,
) {
  const total = fullTurns.length
  const [windowState, setWindowState] = useState<WindowState>(() => ({
    sessionId: activeHistoryId,
    loadedCount: Math.min(INITIAL_VISIBLE, total),
  }))
  const pendingScrollRestoreRef = useRef<{ scrollHeight: number; scrollTop: number } | null>(null)
  const pagingRef = useRef(false)
  const warmupRafRef = useRef<number | null>(null)
  const previousTurnsLengthRef = useRef({ sessionId: activeHistoryId, length: total })

  const loadedCount =
    windowState.sessionId === activeHistoryId
      ? Math.min(windowState.loadedCount, total)
      : Math.min(INITIAL_VISIBLE, total)

  const setActiveLoadedCount = useCallback((updater: (current: number) => number) => {
    setWindowState((current) => {
      const currentLoaded =
        current.sessionId === activeHistoryId
          ? current.loadedCount
          : Math.min(INITIAL_VISIBLE, fullTurns.length)
      const nextLoaded = Math.min(fullTurns.length, Math.max(0, updater(currentLoaded)))
      if (current.sessionId === activeHistoryId && current.loadedCount === nextLoaded) {
        return current
      }
      return { sessionId: activeHistoryId, loadedCount: nextLoaded }
    })
  }, [activeHistoryId, fullTurns.length])

  /** 切换会话时只同步清理 refs；窗口数量用派生值立即回到首屏，避免 layout 阶段二次渲染卡点击。 */
  useLayoutEffect(() => {
    pendingScrollRestoreRef.current = null
    pagingRef.current = false
    if (warmupRafRef.current !== null) {
      cancelAnimationFrame(warmupRafRef.current)
      warmupRafRef.current = null
    }
  }, [activeHistoryId, fullTurns.length])

  useEffect(() => {
    setActiveLoadedCount(() => Math.min(INITIAL_VISIBLE, fullTurns.length))
    if (fullTurns.length <= INITIAL_VISIBLE) {
      return
    }
    warmupRafRef.current = requestAnimationFrame(() => {
      warmupRafRef.current = null
      setActiveLoadedCount((current) => Math.min(fullTurns.length, Math.max(current, WARM_VISIBLE)))
    })
    return () => {
      if (warmupRafRef.current !== null) {
        cancelAnimationFrame(warmupRafRef.current)
        warmupRafRef.current = null
      }
    }
  }, [activeHistoryId, fullTurns.length, setActiveLoadedCount])

  useEffect(() => {
    const t = fullTurns.length
    const previous = previousTurnsLengthRef.current
    previousTurnsLengthRef.current = { sessionId: activeHistoryId, length: t }
    if (previous.sessionId !== activeHistoryId || previous.length === t) {
      return
    }
    // Keep the window from staying at 0 while turns stream into the same session.
    setActiveLoadedCount((c) => {
      if (t === 0) {
        return 0
      }
      /* 避免 loadedCount 卡在 0：同一会话从 0 条变为 ≥1 条时 activeHistoryId 不变，旧逻辑 min(0,t) 永远为 0，虚拟列表无行可滚 */
      const floor = Math.min(WARM_VISIBLE, t)
      return Math.min(Math.max(c, floor), t)
    })
  }, [fullTurns.length, activeHistoryId, setActiveLoadedCount])

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
    setActiveLoadedCount((c) => Math.min(total, c + LOAD_MORE_BATCH))
  }, [visibleRangeStart, total, scrollParentRef, setActiveLoadedCount])

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
