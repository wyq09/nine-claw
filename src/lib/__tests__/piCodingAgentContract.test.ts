// @vitest-environment node

import { describe, expect, it } from 'vitest'

import {
  formatSize,
  truncateHead,
  withFileMutationQueue,
} from '@earendil-works/pi-coding-agent'

// 九爪的托管运行时扩展（src-tauri/src/managed_runtime_extension.rs 的
// build_managed_runtime_extension_source）从 @earendil-works/pi-coding-agent
// 具名导入 formatSize / truncateHead / withFileMutationQueue。
// 此测试锁定 pi-coding-agent 1.1 对这些导出的公开契约（存在且行为正确），
// 防止上游升级时静默破坏打包出来的扩展。
describe('pi-coding-agent managed extension contract', () => {
  it('exports formatSize / truncateHead / withFileMutationQueue as functions', () => {
    expect(typeof formatSize).toBe('function')
    expect(typeof truncateHead).toBe('function')
    expect(typeof withFileMutationQueue).toBe('function')
  })

  it('formatSize renders byte sizes', () => {
    expect(formatSize(0)).toContain('0')
    expect(formatSize(2048)).toMatch(/2(\.0)?\s?[kK]/)
  })

  it('truncateHead keeps content within the byte limit', () => {
    const result = truncateHead('short line\n'.repeat(20), { maxBytes: 64 })
    expect(result.content.length).toBeLessThanOrEqual(64)
    expect(result.firstLineExceedsLimit).toBe(false)
  })

  it('truncateHead flags a first line that exceeds the byte limit', () => {
    const result = truncateHead('a'.repeat(100), { maxBytes: 64 })
    expect(result.firstLineExceedsLimit).toBe(true)
  })

  it('withFileMutationQueue serializes mutations for the same path', async () => {
    const events: string[] = []
    const first = withFileMutationQueue('/tmp/nineclaw-contract-a', async () => {
      events.push('first-start')
      await new Promise((resolve) => setTimeout(resolve, 10))
      events.push('first-end')
    })
    const second = withFileMutationQueue('/tmp/nineclaw-contract-a', async () => {
      events.push('second-start')
    })

    await Promise.all([first, second])
    expect(events).toEqual(['first-start', 'first-end', 'second-start'])
  })
})
