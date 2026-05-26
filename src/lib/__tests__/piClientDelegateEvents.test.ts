import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

import { listen } from '@tauri-apps/api/event'
import {
  subscribeWorkspaceDelegateChunk,
  subscribeWorkspaceDelegateTerminal,
  subscribeWorkspaceDelegateTool,
  subscribeWorkspaceDelegateTurn,
} from '../piClient'

describe('workspace delegate event subscriptions', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('uses Tauri-safe event names for delegate live events', async () => {
    await subscribeWorkspaceDelegateTurn(() => {})
    await subscribeWorkspaceDelegateTool(() => {})
    await subscribeWorkspaceDelegateChunk(() => {})
    await subscribeWorkspaceDelegateTerminal(() => {})

    expect(listen).toHaveBeenCalledWith('workspace:delegate:turn', expect.any(Function))
    expect(listen).toHaveBeenCalledWith('workspace:delegate:tool', expect.any(Function))
    expect(listen).toHaveBeenCalledWith('workspace:delegate:chunk', expect.any(Function))
    expect(listen).toHaveBeenCalledWith('workspace:delegate:done', expect.any(Function))
    expect(listen).toHaveBeenCalledWith('workspace:delegate:error', expect.any(Function))
  })
})
