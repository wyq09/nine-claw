import type { CompressionResult, CompressionState } from './types'

export type IdleCompressionControllerOptions = {
  delayMs: number
  getState: () => CompressionState
  runCompression: (state: CompressionState, signal: AbortSignal) => Promise<CompressionResult>
  onResult?: (result: CompressionResult) => void
  onError?: (error: unknown) => void
  setTimer?: (callback: () => void, delayMs: number) => unknown
  clearTimer?: (timer: unknown) => void
}

export class IdleCompressionController {
  private timer: unknown = null
  private abortController: AbortController | null = null
  private readonly setTimer: (callback: () => void, delayMs: number) => unknown
  private readonly clearTimer: (timer: unknown) => void
  private readonly options: IdleCompressionControllerOptions

  constructor(options: IdleCompressionControllerOptions) {
    this.options = options
    this.setTimer = options.setTimer ?? ((callback, delayMs) => window.setTimeout(callback, delayMs))
    this.clearTimer = options.clearTimer ?? ((timer) => window.clearTimeout(timer as number))
  }

  notifyUserMessage() {
    this.cancel()
    this.schedule()
  }

  schedule() {
    this.clearPendingTimer()
    this.timer = this.setTimer(() => {
      this.timer = null
      void this.fire()
    }, this.options.delayMs)
  }

  cancel() {
    this.clearPendingTimer()
    this.abortController?.abort()
    this.abortController = null
  }

  dispose() {
    this.cancel()
  }

  async fire(): Promise<void> {
    this.abortController?.abort()
    const controller = new AbortController()
    this.abortController = controller
    const snapshot = this.options.getState()

    try {
      const result = await this.options.runCompression(snapshot, controller.signal)
      if (!controller.signal.aborted) {
        this.options.onResult?.(result)
      }
    } catch (error) {
      if (!controller.signal.aborted) {
        this.options.onError?.(error)
      }
    } finally {
      if (this.abortController === controller) {
        this.abortController = null
      }
    }
  }

  private clearPendingTimer() {
    if (this.timer == null) return
    this.clearTimer(this.timer)
    this.timer = null
  }
}
