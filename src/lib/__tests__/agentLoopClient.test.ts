import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import { agentLoopRespondReview, agentLoopAbort } from '../piClient'

const mockedInvoke = vi.mocked(invoke)

describe('agentLoop API', () => {
  beforeEach(() => {
    mockedInvoke.mockReset()
  })

  it('agentLoopRespondReview sends correct params', async () => {
    mockedInvoke.mockResolvedValue(undefined)
    await agentLoopRespondReview('loop-1', true, 70)
    expect(mockedInvoke).toHaveBeenCalledWith('agent_loop_respond_review', {
      loopId: 'loop-1',
      approved: true,
      extendTo: 70,
    })
  })

  it('agentLoopRespondReview defaults extendTo to null', async () => {
    mockedInvoke.mockResolvedValue(undefined)
    await agentLoopRespondReview('loop-1', false)
    expect(mockedInvoke).toHaveBeenCalledWith('agent_loop_respond_review', {
      loopId: 'loop-1',
      approved: false,
      extendTo: null,
    })
  })

  it('agentLoopAbort sends loopId', async () => {
    mockedInvoke.mockResolvedValue(undefined)
    await agentLoopAbort('loop-1')
    expect(mockedInvoke).toHaveBeenCalledWith('agent_loop_abort', {
      loopId: 'loop-1',
    })
  })
})
