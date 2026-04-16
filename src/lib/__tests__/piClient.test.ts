import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import { getSessionContextStats } from '../piClient'

describe('getSessionContextStats', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('calls invoke with correct command name and args', async () => {
    const mockResult = {
      sessionId: 'abc',
      usedTokens: 1000,
      contextWindow: 128000,
      inputTokens: 600,
      outputTokens: 400,
      model: 'gpt-4',
      source: 'auto',
    }
    vi.mocked(invoke).mockResolvedValue(mockResult)

    const result = await getSessionContextStats('abc')
    expect(invoke).toHaveBeenCalledWith('get_session_context_stats', { sessionId: 'abc' })
    expect(result).toEqual(mockResult)
  })

  it('propagates errors from invoke', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('RPC failed'))
    await expect(getSessionContextStats('abc')).rejects.toThrow('RPC failed')
  })
})
