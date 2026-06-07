import { describe, expect, it } from 'vitest'
import { applyBatchedStreamDeltas, type PendingToolDelta } from '../piAgent/streamDeltaBatcher'
import type { ConversationTurn, HistoryItem, ToolCallEntry } from '../../types'

function makeTurn(override?: Partial<ConversationTurn>): ConversationTurn {
  return {
    id: 't1',
    prompt: 'hello',
    answer: '',
    status: 'running',
    createdAt: 100,
    thinking: '',
    activity: [],
    toolCalls: [],
    responseSegments: [],
    ...override,
  }
}

function makeHistory(override?: Partial<HistoryItem>): HistoryItem {
  return {
    id: 'h1',
    title: 'test',
    status: 'running',
    createdAt: 100,
    updatedAt: 100,
    turns: [makeTurn()],
    ...override,
  }
}

function makeToolCall(override?: Partial<ToolCallEntry>): ToolCallEntry {
  return {
    id: 'tc-id',
    toolCallId: 'tool-1',
    toolName: 'my_tool',
    argsText: '',
    resultText: '',
    state: 'running',
    createdAt: 100,
    ...override,
  }
}

describe('applyBatchedStreamDeltas', () => {
  describe('empty buffers', () => {
    it('returns the same array reference when all buffers are empty', () => {
      const history = [makeHistory()]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: [],
        thinkingChunks: [],
        toolDeltas: [],
      })
      expect(result).toBe(history)
    })
  })

  describe('text chunks', () => {
    it('concatenates text chunks and appends to answer', () => {
      const history = [makeHistory()]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: ['Hello', ', ', 'World'],
        thinkingChunks: [],
        toolDeltas: [],
      })
      expect(result[0]!.turns[0]!.answer).toBe('Hello, World')
    })

    it('appends to existing answer', () => {
      const history = [makeHistory({ turns: [makeTurn({ answer: 'prior ' })] })]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: ['new text'],
        thinkingChunks: [],
        toolDeltas: [],
      })
      expect(result[0]!.turns[0]!.answer).toBe('prior new text')
    })

    it('updates responseSegments via appendTextToSegments', () => {
      const history = [makeHistory()]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: ['chunk1', 'chunk2'],
        thinkingChunks: [],
        toolDeltas: [],
      })
      expect(result[0]!.turns[0]!.responseSegments).toEqual([
        { type: 'text', text: 'chunk1chunk2' },
      ])
    })

    it('sets turn status to running', () => {
      const history = [makeHistory({ turns: [makeTurn({ status: 'done' })] })]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: ['x'],
        thinkingChunks: [],
        toolDeltas: [],
      })
      expect(result[0]!.turns[0]!.status).toBe('running')
    })
  })

  describe('thinking chunks', () => {
    it('concatenates thinking chunks and appends to thinking field', () => {
      const history = [makeHistory({ turns: [makeTurn({ thinking: 'base ' })] })]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: [],
        thinkingChunks: ['more', ' thought'],
        toolDeltas: [],
      })
      expect(result[0]!.turns[0]!.thinking).toBe('base more thought')
    })

    it('does not affect text fields when only thinking chunks present', () => {
      const history = [makeHistory({ turns: [makeTurn({ answer: 'existing' })] })]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: [],
        thinkingChunks: ['think'],
        toolDeltas: [],
      })
      expect(result[0]!.turns[0]!.answer).toBe('existing')
    })
  })

  describe('tool deltas', () => {
    it('appends argsDelta to matching tool call argsText', () => {
      const history = [makeHistory({ turns: [makeTurn({ toolCalls: [makeToolCall({ argsText: '{"a":' })] })] })]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: [],
        thinkingChunks: [],
        toolDeltas: [{ toolCallId: 'tool-1', argsDelta: '"b"}' }],
      })
      expect(result[0]!.turns[0]!.toolCalls[0]!.argsText).toBe('{"a":"b"}')
    })

    it('appends resultDelta to matching tool call resultText', () => {
      const history = [makeHistory({ turns: [makeTurn({ toolCalls: [makeToolCall()] })] })]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: [],
        thinkingChunks: [],
        toolDeltas: [
          { toolCallId: 'tool-1', resultDelta: 'result-' },
          { toolCallId: 'tool-1', resultDelta: 'data' },
        ],
      })
      expect(result[0]!.turns[0]!.toolCalls[0]!.resultText).toBe('result-data')
    })

    it('merges multiple deltas for the same toolCallId', () => {
      const history = [makeHistory({ turns: [makeTurn({ toolCalls: [makeToolCall()] })] })]
      const deltas: PendingToolDelta[] = [
        { toolCallId: 'tool-1', argsDelta: 'a' },
        { toolCallId: 'tool-1', argsDelta: 'b' },
        { toolCallId: 'tool-1', resultDelta: 'r1' },
        { toolCallId: 'tool-1', argsDelta: 'c' },
        { toolCallId: 'tool-1', resultDelta: 'r2' },
      ]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: [],
        thinkingChunks: [],
        toolDeltas: deltas,
      })
      const tc = result[0]!.turns[0]!.toolCalls[0]!
      expect(tc.argsText).toBe('abc')
      expect(tc.resultText).toBe('r1r2')
    })

    it('handles multiple different tool calls independently', () => {
      const tool1 = makeToolCall({ toolCallId: 'tool-1' })
      const tool2 = makeToolCall({ id: 'tc-id-2', toolCallId: 'tool-2', argsText: 'x' })
      const history = [makeHistory({ turns: [makeTurn({ toolCalls: [tool1, tool2] })] })]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: [],
        thinkingChunks: [],
        toolDeltas: [
          { toolCallId: 'tool-1', argsDelta: 'for-1' },
          { toolCallId: 'tool-2', argsDelta: 'for-2' },
        ],
      })
      const turns = result[0]!.turns[0]!.toolCalls
      expect(turns[0]!.argsText).toBe('for-1')
      expect(turns[1]!.argsText).toBe('xfor-2')
    })

    it('sets tool call state to running', () => {
      const history = [makeHistory({
        turns: [makeTurn({ toolCalls: [makeToolCall({ state: 'done' })] })],
      })]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: [],
        thinkingChunks: [],
        toolDeltas: [{ toolCallId: 'tool-1', argsDelta: 'x' }],
      })
      expect(result[0]!.turns[0]!.toolCalls[0]!.state).toBe('running')
    })
  })

  describe('mixed updates', () => {
    it('applies text, thinking, and tool deltas in a single call', () => {
      const history = [makeHistory({ turns: [makeTurn({ toolCalls: [makeToolCall()] })] })]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: ['text chunk'],
        thinkingChunks: ['think'],
        toolDeltas: [{ toolCallId: 'tool-1', argsDelta: 'args' }],
      })
      const turn = result[0]!.turns[0]!
      expect(turn.answer).toBe('text chunk')
      expect(turn.thinking).toBe('think')
      expect(turn.toolCalls[0]!.argsText).toBe('args')
    })
  })

  describe('non-matching identifiers', () => {
    it('returns the same item reference when historyId does not match', () => {
      const item = makeHistory()
      const history = [item]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'wrong-id',
        turnId: 't1',
        textChunks: ['hi'],
        thinkingChunks: [],
        toolDeltas: [],
      })
      expect(result[0]).toBe(item)
    })

    it('returns the same item reference when turnId does not match any turn', () => {
      const item = makeHistory({ turns: [makeTurn({ id: 'existing-turn' })] })
      const history = [item]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 'non-existent-turn',
        textChunks: ['hi'],
        thinkingChunks: [],
        toolDeltas: [],
      })
      expect(result[0]).toBe(item)
    })

    it('does not modify non-matching items in a multi-item history', () => {
      const other = { ...makeHistory(), id: 'other' }
      const history = [makeHistory(), other]
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: ['hi'],
        thinkingChunks: [],
        toolDeltas: [],
      })
      expect(result[1]).toBe(other)
    })
  })

  describe('item metadata', () => {
    it('updates item updatedAt when a turn changes', () => {
      const history = [makeHistory({ updatedAt: 0 })]
      const before = Date.now()
      const result = applyBatchedStreamDeltas({
        history,
        historyId: 'h1',
        turnId: 't1',
        textChunks: ['x'],
        thinkingChunks: [],
        toolDeltas: [],
      })
      expect(result[0]!.updatedAt).toBeGreaterThanOrEqual(before)
    })
  })
})
