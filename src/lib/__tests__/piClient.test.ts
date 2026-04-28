import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import { getSessionContextStats, streamPiPrompt, syncRuntimeParameters } from '../piClient'

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

describe('streamPiPrompt / syncRuntimeParameters', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('passes runtimeParameters to stream_pi_prompt', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)
    const rp = {
      maxAgentToolRoundsPerDialogue: 42,
      streamDisconnectMaxRetries: 2,
      llmOuterMaxAttempts: 6,
    }
    await streamPiPrompt('hi', {
      sessionId: 's1',
      runtimeParameters: rp,
    })
    expect(invoke).toHaveBeenCalledWith(
      'stream_pi_prompt',
      expect.objectContaining({
        prompt: 'hi',
        sessionId: 's1',
        runtimeParameters: rp,
      }),
    )
  })

  it('calls sync_runtime_parameters with nested payload', async () => {
    const returned = {
      maxAgentToolRoundsPerDialogue: 80,
      streamDisconnectMaxRetries: 3,
      llmOuterMaxAttempts: 8,
    }
    vi.mocked(invoke).mockResolvedValue(returned)

    const rp = {
      maxAgentToolRoundsPerDialogue: 10,
      streamDisconnectMaxRetries: 1,
      llmOuterMaxAttempts: 5,
    }
    const result = await syncRuntimeParameters(rp)

    expect(invoke).toHaveBeenCalledWith('sync_runtime_parameters', { payload: rp })
    expect(result).toEqual(returned)
  })
})
