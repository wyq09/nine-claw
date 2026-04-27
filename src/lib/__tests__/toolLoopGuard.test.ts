import { describe, expect, it } from 'vitest'
import { TOOL_LOOP_GUARD_REASON_PREFIX, isToolLoopGuardBlockResult } from '../toolLoopGuard'

describe('tool loop guard notification helper', () => {
  it('detects runtime loop guard block results', () => {
    expect(
      isToolLoopGuardBlockResult(`${TOOL_LOOP_GUARD_REASON_PREFIX} 检测到连续 3 次调用相同工具且参数一致。`),
    ).toBe(true)
  })

  it('ignores unrelated tool errors', () => {
    expect(isToolLoopGuardBlockResult('command failed')).toBe(false)
    expect(isToolLoopGuardBlockResult(null)).toBe(false)
  })
})

