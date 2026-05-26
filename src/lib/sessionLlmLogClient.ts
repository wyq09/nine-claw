import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export type SessionLlmLogInfo = {
  workspaceId?: string | null
  sessionId: string
  path: string
  size: number
  modifiedAt: number
  preview: string
}

export type SessionLlmLogDetail = {
  info: SessionLlmLogInfo
  content: string
}

export type SessionLlmLogEvent = {
  workspaceId?: string | null
  sessionId: string
  path: string
}

export async function sessionLlmLogGet(options: {
  workspaceId?: string | null
  sessionId: string
}): Promise<SessionLlmLogDetail> {
  return invoke<SessionLlmLogDetail>('session_llm_log_get', {
    workspaceId: options.workspaceId ?? null,
    sessionId: options.sessionId,
  })
}

export async function sessionLlmLogList(options: {
  workspaceId?: string | null
  limit?: number
}): Promise<SessionLlmLogInfo[]> {
  return invoke<SessionLlmLogInfo[]>('session_llm_log_list', {
    workspaceId: options.workspaceId ?? null,
    limit: options.limit ?? null,
  })
}

export async function sessionLlmLogClear(options: {
  workspaceId?: string | null
  sessionId: string
}): Promise<void> {
  await invoke('session_llm_log_clear', {
    workspaceId: options.workspaceId ?? null,
    sessionId: options.sessionId,
  })
}

export async function onSessionLlmLogEvent(
  handler: (payload: SessionLlmLogEvent) => void,
): Promise<() => void> {
  const unlisten = await listen<SessionLlmLogEvent>('session.llm_log.updated', (event) => {
    handler(event.payload)
  })
  return () => unlisten()
}
