import { describe, expect, it, vi } from 'vitest'
import { IdleCompressionController } from '../idle-compression'
import type { CompressionState } from '../types'

const state: CompressionState = {
  messages: [{ role: 'user', content: 'hello' }],
  usedTokens: 100,
  compressionLevel: 0,
  nextChunk: 1,
}

describe('idle compression controller', () => {
  it('A5 schedules idle compression after user stops interacting', () => {
    const callbacks: Array<() => void> = []
    const onResult = vi.fn()
    const controller = new IdleCompressionController({
      delayMs: 90_000,
      getState: () => state,
      setTimer: (callback) => {
        callbacks.push(callback)
        return callback
      },
      clearTimer: vi.fn(),
      runCompression: async (snapshot) => ({ status: 'skipped', reason: 'below_threshold', state: snapshot }),
      onResult,
    })

    controller.notifyUserMessage()
    expect(callbacks).toHaveLength(1)
    callbacks[0]?.()

    return Promise.resolve().then(() => {
      expect(onResult).toHaveBeenCalledWith({ status: 'skipped', reason: 'below_threshold', state })
    })
  })

  it('A6 aborts an in-flight idle compression when a new user message arrives', async () => {
    let capturedSignal: AbortSignal | undefined
    const controller = new IdleCompressionController({
      delayMs: 1,
      getState: () => state,
      runCompression: async (_snapshot, signal) => {
        capturedSignal = signal
        await new Promise(() => undefined)
        return { status: 'skipped', reason: 'below_threshold', state }
      },
    })

    void controller.fire()
    await Promise.resolve()
    controller.notifyUserMessage()

    expect(capturedSignal && capturedSignal.aborted).toBe(true)
  })
})
