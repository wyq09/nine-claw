// @vitest-environment node

import { describe, expect, it } from 'vitest'

import {
  getCurrentSystemPrompt,
  getCurrentTools,
  normalizeContext,
  type Context,
  type Tool,
} from '@earendil-works/pi-ai'

function createEchoTool(name: string): Tool {
  return {
    name,
    description: `echo ${name}`,
    parameters: { type: 'object', properties: {} },
  }
}

function createContextWithSystemMessages(): Context {
  return {
    systemPrompt: 'nineclaw 基础提示词',
    tools: [createEchoTool('web_search'), createEchoTool('memory_store')],
    messages: [
      { role: 'user', content: '你好', timestamp: 1 },
      {
        role: 'assistant',
        content: [{ type: 'text', text: '你好！' }],
        api: 'anthropic-messages',
        provider: 'test-provider',
        model: 'test-model',
        stopReason: 'stop',
        usage: {
          input: 1,
          output: 1,
          cacheRead: 0,
          cacheWrite: 0,
          totalTokens: 2,
          cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
        },
        timestamp: 2,
      },
    ],
  }
}

// 九爪的 Anthropic 兼容扩展（src-tauri/src/lib.rs 的
// build_desktop_anthropic_compat_extension_source）在 pi >= 0.86 的
// TranscriptContext 上必须通过重放 system message 获取 systemPrompt 与 tools。
// 此测试锁定 pi-ai 1.0 对该语义的公开契约，防止上游 API 漂移时静默失效。
describe('pi-ai transcript system message contract', () => {
  it('normalizeContext folds systemPrompt and tools into a leading system message', () => {
    const transcript = normalizeContext(createContextWithSystemMessages())

    expect(transcript.messages).toHaveLength(3)
    const [systemMessage] = transcript.messages
    if (systemMessage.role !== 'system') {
      throw new Error('expected leading system message')
    }
    expect(systemMessage.content).toBe('nineclaw 基础提示词')
    expect(systemMessage.toolsAdded?.map((tool) => tool.name)).toEqual([
      'web_search',
      'memory_store',
    ])
  })

  it('getCurrentSystemPrompt and getCurrentTools replay system messages', () => {
    const transcript = normalizeContext(createContextWithSystemMessages())

    expect(getCurrentSystemPrompt(transcript.messages)).toBe('nineclaw 基础提示词')
    expect(getCurrentTools(transcript.messages).map((tool) => tool.name)).toEqual([
      'web_search',
      'memory_store',
    ])
  })

  it('getCurrentTools honors toolsRemoved deltas from later system messages', () => {
    const transcript = normalizeContext(createContextWithSystemMessages())
    transcript.messages.push({
      role: 'system',
      content: '',
      toolsRemoved: [{ name: 'web_search' }],
      timestamp: 3,
    })

    expect(getCurrentSystemPrompt(transcript.messages)).toBe('nineclaw 基础提示词')
    expect(getCurrentTools(transcript.messages).map((tool) => tool.name)).toEqual([
      'memory_store',
    ])
  })

  it('transcript without system messages yields empty prompt and tools', () => {
    const messages = [{ role: 'user', content: '纯文本', timestamp: 1 } as const]

    expect(getCurrentSystemPrompt(messages)).toBe('')
    expect(getCurrentTools(messages)).toEqual([])
  })
})
