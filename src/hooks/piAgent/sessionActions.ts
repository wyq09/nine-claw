/**
 * 会话级动作（从 usePiAgent 拆出以控制主文件行数）：
 * - forkHistoryItem：从某一轮（含）派生分支会话并切换过去；
 * - clearSession：清空当前 PI 运行时会话。
 */

import { startTransition, useCallback, type Dispatch, type SetStateAction } from 'react'
import { chatForkSession, type SessionForkWorkspaceMode } from '../../lib/chatForkClient'
import { clearPiSession } from '../../lib/piClient'
import { sessionDetailToHistoryItem } from '../../lib/chatAdapter'
import type { HistoryItem } from '../../types'
import { createId } from './piAgentPure'

export type SessionActionsInput = {
  runningHistoryIds: string[]
  setError: Dispatch<SetStateAction<string>>
  setHistory: Dispatch<SetStateAction<HistoryItem[]>>
  setActiveHistoryId: Dispatch<SetStateAction<string>>
}

export function useSessionActions({
  runningHistoryIds,
  setError,
  setHistory,
  setActiveHistoryId,
}: SessionActionsInput) {
  /** 从源会话的某一轮（含）创建分支会话，成功后切换到新会话。 */
  const forkHistoryItem = useCallback(
    async (
      historyId: string,
      turnId: string,
      workspaceMode: SessionForkWorkspaceMode,
    ): Promise<HistoryItem | null> => {
      const trimmedId = historyId.trim()
      if (!trimmedId || !turnId.trim()) {
        return null
      }
      if (runningHistoryIds.includes(trimmedId)) {
        setError('当前会话仍在生成，暂时不能创建分支。')
        return null
      }
      const newSessionId = createId()
      try {
        const detail = await chatForkSession({
          sourceSessionId: trimmedId,
          forkTurnId: turnId,
          newSessionId,
          workspaceMode,
        })
        const forkedItem = sessionDetailToHistoryItem(detail)
        setHistory((previous) => [
          forkedItem,
          ...previous.filter((item) => item.id !== forkedItem.id),
        ])
        startTransition(() => {
          setActiveHistoryId(forkedItem.id)
          setError('')
        })
        return forkedItem
      } catch (invokeError) {
        const message = invokeError instanceof Error ? invokeError.message : String(invokeError)
        setError(`创建分支失败：${message}`)
        return null
      }
    },
    [runningHistoryIds, setError, setHistory, setActiveHistoryId],
  )

  const clearSession = useCallback(async () => {
    try {
      await clearPiSession()
      setError('')
    } catch (invokeError) {
      const message = invokeError instanceof Error ? invokeError.message : String(invokeError)
      setError(message)
    }
  }, [setError])

  return { forkHistoryItem, clearSession }
}
