// @vitest-environment node

import { Type } from '@sinclair/typebox'

import {
  createAgentDelegateTool,
  fetchDelegateWithRetry,
  formatDelegateFetchError,
} from './agent_delegate_tool.mjs'

function createDeps(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    Type,
    fetchImpl: vi.fn(async () => ({
      ok: true,
      status: 200,
      json: async () => ({
        ok: true,
        output: 'delegate result',
        agentId: 'worker-1',
        agentName: 'Worker',
        durationMs: 1200,
      }),
    })),
    processApi: {
      env: {
        NINECLAW_PROXY_BASE_URL: 'http://127.0.0.1:8123',
        NINECLAW_PROXY_SESSION_TOKEN: 'token-1',
      },
    },
    retryDelaysMs: [0, 0],
    ...overrides,
  }
}

describe('agent_delegate helpers', () => {
  it('retries transient fetch failures before returning a response', async () => {
    const fetchImpl = vi
      .fn()
      .mockRejectedValueOnce(new TypeError('fetch failed'))
      .mockResolvedValueOnce({ ok: true })

    const response = await fetchDelegateWithRetry(fetchImpl, 'http://127.0.0.1:8123/delegate/token/dispatch', {}, [0])

    expect(response).toEqual({ ok: true })
    expect(fetchImpl).toHaveBeenCalledTimes(2)
  })

  it('keeps useful cause details from generic Node fetch errors', () => {
    const error = new TypeError('fetch failed', {
      cause: Object.assign(new Error('connect ECONNREFUSED 127.0.0.1:8123'), {
        code: 'ECONNREFUSED',
        address: '127.0.0.1',
        port: 8123,
      }),
    })

    expect(formatDelegateFetchError(error)).toContain('fetch failed')
    expect(formatDelegateFetchError(error)).toContain('ECONNREFUSED')
    expect(formatDelegateFetchError(error)).toContain('127.0.0.1:8123')
  })
})

describe('agent_delegate tool execute', () => {
  it('delegates through the local proxy and includes result metadata', async () => {
    const deps = createDeps()
    const tool = createAgentDelegateTool(deps as never)

    const result = await tool.execute(
      'tool-call-1',
      { role: 'worker', task: 'Summarize this', context: 'Context' },
      undefined,
      undefined,
      {},
    )

    expect(deps.fetchImpl).toHaveBeenCalledWith(
      'http://127.0.0.1:8123/delegate/token-1/dispatch',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ role: 'worker', task: 'Summarize this', context: 'Context' }),
      }),
    )
    expect(result.content[0].text).toContain('[Agent: Worker | Time: 1.2s]')
    expect(result.content[0].text).toContain('delegate result')
    expect(result.details).toMatchObject({
      ok: true,
      agentId: 'worker-1',
      agentName: 'Worker',
      durationMs: 1200,
    })
  })

  it('retries transient proxy fetch failures during execution', async () => {
    const fetchImpl = vi
      .fn()
      .mockRejectedValueOnce(new TypeError('fetch failed'))
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({ ok: true, output: 'after retry' }),
      })
    const tool = createAgentDelegateTool(createDeps({ fetchImpl, retryDelaysMs: [0] }) as never)

    const result = await tool.execute(
      'tool-call-2',
      { role: 'worker', task: 'Try again' },
      undefined,
      undefined,
      {},
    )

    expect(fetchImpl).toHaveBeenCalledTimes(2)
    expect(result.content[0].text).toContain('after retry')
  })

  it('returns actionable network diagnostics after retries are exhausted', async () => {
    const fetchError = new TypeError('fetch failed', {
      cause: Object.assign(new Error('connect ECONNREFUSED 127.0.0.1:8123'), {
        code: 'ECONNREFUSED',
        address: '127.0.0.1',
        port: 8123,
      }),
    })
    const fetchImpl = vi.fn().mockRejectedValue(fetchError)
    const tool = createAgentDelegateTool(createDeps({ fetchImpl, retryDelaysMs: [0] }) as never)

    const result = await tool.execute(
      'tool-call-3',
      { role: 'worker', task: 'Fail clearly' },
      undefined,
      undefined,
      {},
    )

    expect(fetchImpl).toHaveBeenCalledTimes(2)
    expect(result.content[0].text).toContain('Delegation error: fetch failed')
    expect(result.content[0].text).toContain('ECONNREFUSED')
    expect(result.content[0].text).toContain('Report this blocker to the user')
    expect(result.details).toMatchObject({
      ok: false,
      reason: 'fetch_error',
    })
  })

  it('tells the caller to surface delegate failures instead of silently doing the work', async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ ok: false, error: 'worker unavailable' }),
    })
    const tool = createAgentDelegateTool(createDeps({ fetchImpl }) as never)

    const result = await tool.execute(
      'tool-call-5',
      { role: 'worker', task: 'Use the named sub-agent' },
      undefined,
      undefined,
      {},
    )

    expect(result.content[0].text).toContain('worker unavailable')
    expect(result.content[0].text).toContain('Report this blocker to the user')
    expect(result.details).toMatchObject({
      ok: false,
      reason: 'delegate_error',
    })
  })

  it('does not retry cancelled delegation requests', async () => {
    const controller = new AbortController()
    controller.abort()
    const fetchImpl = vi.fn().mockRejectedValue(Object.assign(new Error('Aborted'), { name: 'AbortError' }))
    const tool = createAgentDelegateTool(createDeps({ fetchImpl, retryDelaysMs: [0, 0] }) as never)

    const result = await tool.execute(
      'tool-call-4',
      { role: 'worker', task: 'Cancel' },
      controller.signal,
      undefined,
      {},
    )

    expect(fetchImpl).toHaveBeenCalledTimes(1)
    expect(result.content[0].text).toBe('Delegation was cancelled.')
    expect(result.details).toMatchObject({
      ok: false,
      reason: 'cancelled',
    })
  })
})
