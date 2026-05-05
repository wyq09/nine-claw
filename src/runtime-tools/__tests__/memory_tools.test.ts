import { describe, expect, it, vi } from 'vitest'

import { createChatSearchTool } from '../chat_search_tool.mjs'
import { createMemoryForgetTool } from '../memory_forget_tool.mjs'
import { createMemoryGetTool } from '../memory_get_tool.mjs'
import { createMemoryListTool } from '../memory_list_tool.mjs'
import { createMemoryReadTool } from '../memory_read_tool.mjs'
import { createMemorySearchTool } from '../memory_search_tool.mjs'
import { createMemoryStoreTool } from '../memory_store_tool.mjs'
import { createMemoryUpdateTool } from '../memory_update_tool.mjs'

function createTypeStub() {
  return {
    String: (value: unknown) => value,
    Optional: (value: unknown) => value,
    Integer: (value: unknown) => value,
    Number: (value: unknown) => value,
    Array: (value: unknown) => value,
    Object: (value: unknown) => value,
    Union: (value: unknown) => value,
    Literal: (value: unknown) => value,
    Any: (value: unknown) => value,
  }
}

function createEnv() {
  return {
    NINECLAW_PROXY_BASE_URL: 'http://127.0.0.1:8123',
    NINECLAW_PROXY_SESSION_TOKEN: 'token-1',
  }
}

describe('memory and chat tools', () => {
  it('memory_update posts markdown content to the memory update endpoint', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      json: async () => ({ ok: true, mode: 'replace' }),
    })
    vi.stubGlobal('fetch', fetchMock)
    const tool = createMemoryUpdateTool({ Type: createTypeStub() } as never)

    const result = await tool.execute('call-1', { content: '# MEMORY', mode: 'replace' })

    expect(fetchMock).toHaveBeenCalledWith(
      'http://127.0.0.1:8123/memory/token-1/update',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ content: '# MEMORY', mode: 'replace' }),
      }),
    )
    expect(result.details).toMatchObject({ ok: true })
  })

  it('memory_read reads the current agent memory file', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      json: async () => ({ ok: true, content: '# MEMORY' }),
    })
    vi.stubGlobal('fetch', fetchMock)
    const tool = createMemoryReadTool({ Type: createTypeStub() } as never)

    const result = await tool.execute('call-2', {})

    expect(fetchMock).toHaveBeenCalledWith(
      'http://127.0.0.1:8123/memory/token-1/read',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({}),
      }),
    )
    expect(result.details).toMatchObject({ ok: true, content: '# MEMORY' })
  })

  it('memory_store, memory_get, memory_list, memory_forget, and chat_search hit their proxy endpoints', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      json: async () => ({ ok: true }),
    })
    vi.stubGlobal('fetch', fetchMock)
    const Type = createTypeStub()

    const storeTool = createMemoryStoreTool({ Type } as never)
    const getTool = createMemoryGetTool({ Type } as never)
    const listTool = createMemoryListTool({ Type } as never)
    const forgetTool = createMemoryForgetTool({ Type } as never)
    const chatSearchTool = createChatSearchTool({ Type } as never)

    await storeTool.execute('call-3', { key: 'user.profile', value: { name: 'Ada' } })
    await getTool.execute('call-4', { key: 'user.profile' })
    await listTool.execute('call-5', { limit: 25 })
    await forgetTool.execute('call-6', { key: 'user.profile' })
    await chatSearchTool.execute('call-7', { query: 'memory', limit: 5 })

    expect(fetchMock.mock.calls).toEqual([
      [
        'http://127.0.0.1:8123/memory/token-1/store',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ key: 'user.profile', value: { name: 'Ada' } }),
        }),
      ],
      [
        'http://127.0.0.1:8123/memory/token-1/get',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ key: 'user.profile' }),
        }),
      ],
      [
        'http://127.0.0.1:8123/memory/token-1/list',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ limit: 25 }),
        }),
      ],
      [
        'http://127.0.0.1:8123/memory/token-1/forget',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ key: 'user.profile' }),
        }),
      ],
      [
        'http://127.0.0.1:8123/chat/token-1/search',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ query: 'memory', limit: 5 }),
        }),
      ],
    ])
  })

  it('memory_search handles plain-text backend errors without throwing JSON parse errors', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: false,
      status: 503,
      text: async () => '无可用的嵌入提供者',
    })
    vi.stubGlobal('fetch', fetchMock)
    const tool = createMemorySearchTool({ Type: createTypeStub() } as never)

    const result = await tool.execute('call-8', { query: '卡兹克', limit: 20 }, undefined, undefined, {})

    expect(result.details).toMatchObject({
      ok: false,
      status: 503,
      error: '无可用的嵌入提供者',
    })
    expect(result.content[0].text).toContain('无可用的嵌入提供者')
  })
})

Object.assign(process.env, createEnv())
