export const TOOL_LOOP_GUARD_REASON_PREFIX = '[NineClaw loop guard]'

export function isToolLoopGuardBlockResult(value: unknown): boolean {
  return typeof value === 'string' && value.includes(TOOL_LOOP_GUARD_REASON_PREFIX)
}

