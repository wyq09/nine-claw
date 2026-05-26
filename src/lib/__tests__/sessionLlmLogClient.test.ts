import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  onSessionLlmLogEvent,
  sessionLlmLogClear,
  sessionLlmLogGet,
  sessionLlmLogList,
} from '../sessionLlmLogClient'

describe('sessionLlmLogClient', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('reads a session log by workspace and session id', async () => {
    const detail = { info: { sessionId: 's1', path: '/tmp/s1.md', size: 1, modifiedAt: 1, preview: '' }, content: 'log' }
    vi.mocked(invoke).mockResolvedValue(detail)

    const result = await sessionLlmLogGet({ workspaceId: 'ws-1', sessionId: 's1' })

    expect(invoke).toHaveBeenCalledWith('session_llm_log_get', { workspaceId: 'ws-1', sessionId: 's1' })
    expect(result).toBe(detail)
  })

  it('lists logs with nullable workspace scope', async () => {
    vi.mocked(invoke).mockResolvedValue([])

    await sessionLlmLogList({ workspaceId: null, limit: 50 })

    expect(invoke).toHaveBeenCalledWith('session_llm_log_list', { workspaceId: null, limit: 50 })
  })

  it('clears a specific session log', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)

    await sessionLlmLogClear({ workspaceId: 'ws-1', sessionId: 's1' })

    expect(invoke).toHaveBeenCalledWith('session_llm_log_clear', { workspaceId: 'ws-1', sessionId: 's1' })
  })

  it('subscribes to the session log update event', async () => {
    const unlisten = vi.fn()
    vi.mocked(listen).mockImplementation(async (_eventName, handler) => {
      handler({ payload: { sessionId: 's1', path: '/tmp/s1.md' } } as Parameters<typeof handler>[0])
      return unlisten
    })
    const handler = vi.fn()

    const dispose = await onSessionLlmLogEvent(handler)
    dispose()

    expect(listen).toHaveBeenCalledWith('session.llm_log.updated', expect.any(Function))
    expect(handler).toHaveBeenCalledWith({ sessionId: 's1', path: '/tmp/s1.md' })
    expect(unlisten).toHaveBeenCalled()
  })
})
