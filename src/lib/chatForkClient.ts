import { invoke } from '@tauri-apps/api/core'
import type { ChatSessionDetail } from '../types'

/** 会话分支的工作区模式：share 共享原会话目录；copy 独立拷贝文件。 */
export type SessionForkWorkspaceMode = 'share' | 'copy'

/** 从源会话的某一轮（含）分叉出新会话，返回新会话详情（含复制的轮次）。 */
export async function chatForkSession(payload: {
  sourceSessionId: string
  forkTurnId: string
  newSessionId: string
  workspaceMode: SessionForkWorkspaceMode
}): Promise<ChatSessionDetail> {
  return invoke<ChatSessionDetail>('chat_fork_session', {
    sourceSessionId: payload.sourceSessionId,
    forkTurnId: payload.forkTurnId,
    newSessionId: payload.newSessionId,
    workspaceMode: payload.workspaceMode,
  })
}
