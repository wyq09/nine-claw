import { useCallback, useEffect, useState } from 'react'
import type { SessionForkWorkspaceMode } from '../../lib/chatForkClient'
import type { ConversationTurn, HistoryItem } from '../../types'

export type ForkSessionTurnHandler = (
  historyId: string,
  turnId: string,
  workspaceMode: SessionForkWorkspaceMode,
) => Promise<HistoryItem | null | void>

/** 分支弹窗状态机：目标轮次、busy 与确认/取消处理（从 ChatWorkspace 拆出）。 */
export function useCreateBranchFlow(input: {
  activeHistoryId: string
  turns: ConversationTurn[]
  onForkSessionTurn?: ForkSessionTurnHandler
  onError: (message: string) => void
}) {
  const { activeHistoryId, turns, onForkSessionTurn, onError } = input
  const [forkTarget, setForkTarget] = useState<{ turnId: string; prompt: string } | null>(null)
  const [forkBusy, setForkBusy] = useState(false)

  useEffect(() => {
    setForkTarget(null)
    setForkBusy(false)
  }, [activeHistoryId])

  const handleForkTurn = useCallback(
    (turnId: string) => {
      const turn = turns.find((item) => item.id === turnId)
      if (!turn) {
        return
      }
      setForkTarget({ turnId, prompt: turn.prompt })
    },
    [turns],
  )

  const handleForkConfirm = useCallback(
    async (mode: SessionForkWorkspaceMode) => {
      if (!forkTarget || !onForkSessionTurn || !activeHistoryId) {
        return
      }
      setForkBusy(true)
      try {
        await onForkSessionTurn(activeHistoryId, forkTarget.turnId, mode)
        setForkTarget(null)
      } catch (forkError) {
        onError(forkError instanceof Error ? forkError.message : String(forkError))
      } finally {
        setForkBusy(false)
      }
    },
    [forkTarget, onForkSessionTurn, activeHistoryId, onError],
  )

  const cancelFork = useCallback(() => {
    setForkTarget((current) => (current && !forkBusy ? null : current))
  }, [forkBusy])

  return { forkTarget, forkBusy, handleForkTurn, handleForkConfirm, cancelFork }
}
