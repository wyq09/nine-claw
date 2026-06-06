// @vitest-environment node
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  collectServersForAdd,
  createMcpConfigTool,
  deriveServerId,
  normalizeServerConfig,
  normalizeTransport,
  validateServerConfig,
} from './mcp_config.mjs'

function createTypeStub() {
  return {
    Any: vi.fn((options) => ({ kind: 'any', options })),
    Array: vi.fn((value, options) => ({ kind: 'array', value, options })),
    Boolean: vi.fn((options) => ({ kind: 'boolean', options })),
    Literal: vi.fn((value) => ({ kind: 'literal', value })),
    Object: vi.fn((shape) => ({ kind: 'object', shape })),
    Optional: vi.fn((value) => ({ kind: 'optional', value })),
    Record: vi.fn((key, value, options) => ({ kind: 'record', key, value, options })),
    String: vi.fn((options) => ({ kind: 'string', options })),
    Union: vi.fn((values, options) => ({ kind: 'union', values, options })),
  }
}

const tempPaths: string[] = []
afterEach(() => {
  for (const filePath of tempPaths.splice(0)) {
    try {
      fs.rmSync(filePath, { recursive: true, force: true })
    } catch {}
  }
})

function tempConfigFile() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nineclaw-mcp-config-'))
  tempPaths.push(dir)
  return path.join(dir, 'mcp.json')
}

function jsonResponse(payload: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(payload),
  }
}

describe('mcp_config helpers', () => {
  it('normalizes transport aliases including the three required transports', () => {
    expect(normalizeTransport('stdio')).toBe('stdio')
    expect(normalizeTransport('SSE')).toBe('sse')
    expect(normalizeTransport('streamable-http')).toBe('streamable_http')
    expect(normalizeTransport('http')).toBe('streamable_http')
    expect(normalizeTransport('nope')).toBeNull()
  })

  it('derives a server id from name, url, or command', () => {
    expect(deriveServerId({ id: 'My Server' })).toBe('my-server')
    expect(deriveServerId({ url: 'https://api.example.com/mcp' })).toBe('api-example-com')
    expect(deriveServerId({ command: '/usr/local/bin/some-mcp' })).toBe('some-mcp')
  })

  it('infers transport and validates stdio/sse/streamable_http', () => {
    expect(normalizeServerConfig({ command: 'npx' }).transport).toBe('stdio')
    expect(normalizeServerConfig({ url: 'https://x/mcp' }).transport).toBe('streamable_http')

    expect(() => validateServerConfig(normalizeServerConfig({ id: 'a', transport: 'stdio' }))).toThrow(/command/)
    expect(() => validateServerConfig(normalizeServerConfig({ id: 'a', transport: 'sse' }))).toThrow(/url/)
    expect(() =>
      validateServerConfig(normalizeServerConfig({ id: 'a', transport: 'sse', url: 'ftp://x' })),
    ).toThrow(/http/)
  })

  it('collects servers from raw mcpServers config JSON', () => {
    const servers = collectServersForAdd({
      config: JSON.stringify({
        mcpServers: {
          miview: { url: 'http://127.0.0.1:25424/mcp', headers: { Authorization: 'Bearer t' } },
        },
      }),
    })
    expect(servers).toEqual([
      expect.objectContaining({
        id: 'miview',
        transport: 'streamable_http',
        url: 'http://127.0.0.1:25424/mcp',
        headers: { Authorization: 'Bearer t' },
      }),
    ])
  })

  it('collects servers from structured fields', () => {
    const servers = collectServersForAdd({
      transport: 'sse',
      url: 'https://api.example.com/sse',
      name: 'Example',
    })
    expect(servers[0]).toEqual(
      expect.objectContaining({ id: 'example', transport: 'sse', url: 'https://api.example.com/sse' }),
    )
  })
})

describe('createMcpConfigTool', () => {
  it('adds a server, posts to the proxy, and writes the snapshot for no-restart use', async () => {
    const configPath = tempConfigFile()
    const snapshot = {
      mcpServers: {
        miview: { transport: 'streamable_http', url: 'http://127.0.0.1:25424/mcp', enabled: true },
      },
    }
    const fetchImpl = vi.fn(async () =>
      jsonResponse({ ok: true, data: { servers: [{ id: 'miview', transport: 'streamable_http', url: 'http://127.0.0.1:25424/mcp', enabled: true }] }, snapshot }),
    )

    const tool = createMcpConfigTool({
      Type: createTypeStub(),
      fsSync: fs,
      fetchImpl,
      processApi: {
        env: {
          NINECLAW_PROXY_BASE_URL: 'http://127.0.0.1:9000',
          NINECLAW_PROXY_SESSION_TOKEN: 'tok',
          NINECLAW_MCP_CONFIG_FILE: configPath,
        },
      },
    } as never)

    const result = await tool.execute('call-1', {
      operation: 'add',
      config: { mcpServers: { miview: { url: 'http://127.0.0.1:25424/mcp' } } },
    })

    expect(fetchImpl).toHaveBeenCalledTimes(1)
    const callArgs = fetchImpl.mock.calls[0] as unknown as [string, RequestInit]
    const calledUrl = callArgs[0]
    const calledInit = callArgs[1]
    expect(calledUrl).toBe('http://127.0.0.1:9000/mcp/tok/add')
    const body = JSON.parse(calledInit.body as string)
    expect(body.servers[0]).toEqual(expect.objectContaining({ id: 'miview', transport: 'streamable_http' }))

    // Snapshot must be written so mcp_tool sees it without a restart.
    const written = JSON.parse(fs.readFileSync(configPath, 'utf8'))
    expect(written.mcpServers.miview.url).toBe('http://127.0.0.1:25424/mcp')
    expect(result.content[0].text).toContain('miview')
  })

  it('requires an id for remove/enable/disable', async () => {
    const tool = createMcpConfigTool({
      Type: createTypeStub(),
      fsSync: fs,
      fetchImpl: vi.fn(),
      processApi: { env: { NINECLAW_PROXY_BASE_URL: 'http://x', NINECLAW_PROXY_SESSION_TOKEN: 't' } },
    } as never)

    await expect(tool.execute('c', { operation: 'remove' })).rejects.toThrow(/id/)
  })

  it('disables a server via the set-enabled endpoint', async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse({ ok: true, data: { servers: [] }, snapshot: { mcpServers: {} } }),
    )
    const tool = createMcpConfigTool({
      Type: createTypeStub(),
      fsSync: fs,
      fetchImpl,
      processApi: { env: { NINECLAW_PROXY_BASE_URL: 'http://h', NINECLAW_PROXY_SESSION_TOKEN: 'tk' } },
    } as never)

    await tool.execute('c', { operation: 'disable', id: 'miview' })

    const callArgs = fetchImpl.mock.calls[0] as unknown as [string, RequestInit]
    expect(callArgs[0]).toBe('http://h/mcp/tk/set-enabled')
    expect(JSON.parse(callArgs[1].body as string)).toEqual({ id: 'miview', enabled: false })
  })

  it('surfaces proxy errors', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({ ok: false, error: 'boom' }, 400))
    const tool = createMcpConfigTool({
      Type: createTypeStub(),
      fsSync: fs,
      fetchImpl,
      processApi: { env: { NINECLAW_PROXY_BASE_URL: 'http://h', NINECLAW_PROXY_SESSION_TOKEN: 'tk' } },
    } as never)

    await expect(
      tool.execute('c', { operation: 'add', url: 'https://x/mcp' }),
    ).rejects.toThrow(/boom/)
  })
})
