import { describe, expect, it } from 'vitest'
import type { LlmTraceEntry } from '../../../../lib/llmTraceClient'
import {
  applyLlmTraceEvent,
  getTraceKindLabel,
  getTraceMessageBlocks,
  getTraceResponseBlocks,
  matchesTraceScope,
} from '../llmTraceModel'

function makeEntry(overrides: Partial<LlmTraceEntry> = {}): LlmTraceEntry {
  return {
    id: 'trace-1',
    kind: 'main_pi',
    callerAgentId: 'agent-1',
    callerAgentName: 'Alice',
    status: 'done',
    startedAt: 1,
    systemPrompts: [{ label: 'agent_system_prompt', content: 'system' }],
    userMessage: 'user',
    responseText: 'answer',
    thinkingText: 'thinking',
    toolCalls: [],
    ...overrides,
  }
}

describe('llmTraceModel', () => {
  it('maps trace type labels for all structured categories', () => {
    expect(getTraceKindLabel(makeEntry({ traceType: 'agent_llm' }))).toBe('智能体→大模型')
    expect(getTraceKindLabel(makeEntry({ traceType: 'action_llm', kind: 'action_llm' }))).toBe('动作→大模型')
    expect(getTraceKindLabel(makeEntry({ traceType: 'agent_agent', kind: 'delegate' }))).toBe('智能体→智能体')
  })

  it('falls back to legacy message and response fields', () => {
    const entry = makeEntry()
    expect(getTraceMessageBlocks(entry)).toEqual([
      { id: 'system-0', role: 'system', label: 'agent_system_prompt', content: 'system' },
      { id: 'user-0', role: 'user', label: 'user_prompt', content: 'user' },
    ])
    expect(getTraceResponseBlocks(entry)).toEqual([
      { id: 'thinking-0', kind: 'thinking', label: 'thinking', content: 'thinking' },
      { id: 'output-0', kind: 'output', label: 'assistant_reply', content: 'answer' },
    ])
  })

  it('matches standalone session scope without leaking workspace traces', () => {
    expect(matchesTraceScope(makeEntry({ sessionId: 'session-1' }), { sessionId: 'session-1' })).toBe(true)
    expect(
      matchesTraceScope(makeEntry({ workspaceId: 'ws-1', sessionId: 'session-1' }), { sessionId: 'session-1' }),
    ).toBe(false)
  })

  it('applies response delta events without replacing the entry with partial payloads', () => {
    const current = makeEntry({
      status: 'running',
      responseText: 'hello',
      responseBlocks: [{ id: 'output-0', kind: 'output', label: 'assistant_reply', content: 'hello' }],
    })
    const [patched] = applyLlmTraceEvent([current], {
      phase: 'updated',
      entry: makeEntry({
        id: current.id,
        status: 'running',
        responseText: undefined,
        responseBlocks: undefined,
      }),
      delta: { kind: 'response', text: ' world' },
    })

    expect(patched.responseText).toBe('hello world')
    expect(patched.responseBlocks?.[0]?.content).toBe('hello world')
    expect(patched.userMessage).toBe('user')
  })

  it('applies thinking delta events to thinking fields only', () => {
    const current = makeEntry({
      status: 'running',
      thinkingText: 'plan',
      responseText: 'answer',
      responseBlocks: [
        { id: 'thinking-0', kind: 'thinking', label: 'thinking', content: 'plan' },
        { id: 'output-0', kind: 'output', label: 'assistant_reply', content: 'answer' },
      ],
    })
    const [patched] = applyLlmTraceEvent([current], {
      phase: 'updated',
      entry: makeEntry({ id: current.id, status: 'running' }),
      delta: { kind: 'thinking', text: ' next' },
    })

    expect(patched.thinkingText).toBe('plan next')
    expect(patched.responseText).toBe('answer')
    expect(patched.responseBlocks?.[0]?.content).toBe('plan next')
    expect(patched.responseBlocks?.[1]?.content).toBe('answer')
  })
})
