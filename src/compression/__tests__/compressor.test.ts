import { describe, expect, it, vi } from 'vitest'
import {
  buildCompressedSummaryMessage,
  buildCompressionMessage,
  estimateMessagesTokens,
  parseTopics,
  prepareCompressionContext,
  rebuildHistoryWithCompression,
  runInsertThenCompress,
} from '../compressor'
import { hasBrokenToolPairs, selectRecentMessagesWithToolPairs } from '../message-pair-utils'
import type { CompressionState, ConversationMessage } from '../types'

function user(content: string): ConversationMessage {
  return { role: 'user', content }
}

function assistant(content: string): ConversationMessage {
  return { role: 'assistant', content }
}

function toolPair(id: string): ConversationMessage[] {
  return [
    {
      role: 'assistant',
      content: 'using a tool',
      tool_calls: [{ id, function: { name: 'read_file', arguments: '{"path":"a.ts"}' } }],
    },
    { role: 'tool', tool_call_id: id, name: 'read_file', content: 'file content' },
  ]
}

function baseState(messages: ConversationMessage[], usedTokens = 999): CompressionState {
  return {
    messages: [{ role: 'system', content: 'stable system prompt' }, ...messages],
    usedTokens,
    compressionLevel: 0,
    nextChunk: 1,
  }
}

describe('insert-then-compress core', () => {
  it('A1 builds a system-injected user compression message when thresholds are reached', () => {
    const context = prepareCompressionContext({
      state: baseState(Array.from({ length: 5 }, (_, index) => user(`message ${index}`))),
      config: { messageCountThreshold: 5, tokenThreshold: 10_000, maxRecentMessages: 1 },
    })

    expect(context?.compressionMessage.role).toBe('user')
    expect(context?.compressionMessage.system_injected).toBe(true)
    expect(String(context?.compressionMessage.content)).toContain('<topics>')
    expect(context?.reason).toBe('message_count_threshold')
  })

  it('A2 rebuilds history as system prompt, compressed summary, and recent messages', () => {
    const recent = [user('recent')]
    const rebuilt = rebuildHistoryWithCompression({
      originalMessages: [{ role: 'system', content: 'sys' }, user('old'), ...recent],
      compressedContent: '<topics>重构,测试</topics><summary>## 对话摘要</summary>',
      recentMessages: recent,
      chunkPath: '/tmp/session-chunk-0001.md',
      topics: '重构,测试',
    })

    expect(rebuilt.map((message) => message.role)).toEqual(['system', 'user', 'user'])
    expect(rebuilt[1]?.compressed_summary).toBe(true)
    expect(rebuilt[1]?.system_injected).toBe(true)
    expect(String(rebuilt[1]?.content)).toContain('/tmp/session-chunk-0001.md')
  })

  it('A3 keeps assistant tool calls with matching tool results', () => {
    const messages = [user('old'), ...toolPair('call-1'), user('new')]
    const recent = selectRecentMessagesWithToolPairs(messages, 2)

    expect(recent.map((message) => message.role)).toEqual(['assistant', 'tool', 'user'])
    expect(hasBrokenToolPairs(recent)).toBe(false)
  })

  it('A4 changes compression prompt guidance by level', () => {
    expect(String(buildCompressionMessage(1).content)).toContain('Level 1 要求：详细摘要')
    expect(String(buildCompressionMessage(2).content)).toContain('Level 2 要求：简洁摘要')
    expect(String(buildCompressionMessage(3).content)).toContain('Level 3 要求：最小摘要')
    expect(String(buildCompressionMessage(4).content)).toContain('Level 4+ 要求：超精简一行')
  })

  it('A7 rolls back by leaving caller state untouched when LLM compression fails', async () => {
    const state = baseState([user('old'), assistant('reply')], 999)
    const before = JSON.stringify(state)
    await expect(
      runInsertThenCompress({
        state,
        sessionId: 's1',
        config: { tokenThreshold: 1, maxRecentMessages: 1 },
        callLlm: async () => {
          throw new Error('network failed')
        },
      }),
    ).rejects.toThrow('network failed')
    expect(JSON.stringify(state)).toBe(before)
  })

  it('A8 resets token estimate after successful compression to avoid immediate loops', async () => {
    const state = baseState(Array.from({ length: 30 }, (_, index) => user(`old ${index}`)), 500_000)
    const result = await runInsertThenCompress({
      state,
      sessionId: 's1',
      archiveRootDir: '/tmp',
      config: { tokenThreshold: 1, maxRecentMessages: 2 },
      archiveWriter: vi.fn(),
      callLlm: async () => '<topics>缓存,压缩</topics><summary>## 对话摘要\n- done</summary>',
    })

    expect(result.status).toBe('compressed')
    if (result.status === 'compressed') {
      expect(result.state.usedTokens).toBe(estimateMessagesTokens(result.state.messages))
      expect(result.state.usedTokens).toBeLessThan(state.usedTokens)
      expect(result.state.compressionLevel).toBe(1)
    }
  })

  it('A10 parses topics and strips them from compressed summary message body', () => {
    const raw = '<topics>文件编辑, bug修复</topics>\n<summary>body</summary>'
    const message = buildCompressedSummaryMessage(raw, null, parseTopics(raw))

    expect(parseTopics(raw)).toBe('文件编辑, bug修复')
    expect(String(message.content)).not.toContain('<topics>')
    expect(message.topics).toBe('文件编辑, bug修复')
  })
})
