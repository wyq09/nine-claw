// @vitest-environment node
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  createMcpTool,
  formatMcpCallResult,
  formatMcpToolList,
  normalizeRuntimeMcpSettings,
  resolveMcpServer,
} from './mcp_tool.mjs'

function createTypeStub() {
  return {
    Any: vi.fn(() => ({ kind: 'any' })),
    Integer: vi.fn((options) => ({ kind: 'integer', options })),
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

function writeConfigFile(payload: unknown) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nineclaw-mcp-tool-'))
  const filePath = path.join(dir, 'mcp.json')
  fs.writeFileSync(filePath, JSON.stringify(payload, null, 2))
  tempPaths.push(dir)
  return filePath
}

describe('mcp_tool helpers', () => {
  it('normalizes standard mcpServers config', () => {
    const settings = normalizeRuntimeMcpSettings({
      mcpServers: {
        miview: {
          url: 'http://127.0.0.1:25424/mcp',
          headers: {
            Authorization: 'Bearer token',
          },
        },
      },
    })

    expect(settings.servers).toEqual([
      expect.objectContaining({
        id: 'miview',
        transport: 'streamable_http',
        url: 'http://127.0.0.1:25424/mcp',
        headers: { Authorization: 'Bearer token' },
      }),
    ])
  })

  it('auto-selects the only enabled server', () => {
    const server = resolveMcpServer(
      {
        servers: [
          {
            id: 'only-server',
            enabled: true,
            transport: 'streamable_http',
            url: 'http://127.0.0.1:25424/mcp',
            command: '',
            args: [],
            env: {},
            cwd: '',
            headers: {},
            name: '',
          },
        ],
      },
      '',
    )

    expect(server.id).toBe('only-server')
  })

  it('renders tool list and tool call result text', () => {
    const server = {
      id: 'miview',
      enabled: true,
      transport: 'streamable_http',
      url: 'http://127.0.0.1:25424/mcp',
      command: '',
      args: [],
      env: {},
      cwd: '',
      headers: {},
      name: '',
    }

    expect(
      formatMcpToolList(server, [
        {
          name: 'search_announcements',
          description: 'Search latest announcements',
          inputSchema: { properties: { query: {} } },
        },
      ]),
    ).toContain('search_announcements')

    expect(
      formatMcpCallResult(server, 'search_announcements', {
        isError: false,
        content: [{ type: 'text', text: 'ok' }],
      }),
    ).toContain('ok')
  })
})

describe('createMcpTool', () => {
  it('lists remote tools using the configured server', async () => {
    const configPath = writeConfigFile({
      mcpServers: {
        miview: {
          url: 'http://127.0.0.1:25424/mcp',
          headers: {
            Authorization: 'Bearer token',
          },
        },
      },
    })

    const connect = vi.fn(async () => {})
    const listTools = vi.fn(async () => ({
      tools: [
        {
          name: 'miview_search',
          description: 'Search MiView records',
          inputSchema: { properties: { query: {} } },
        },
      ],
    }))
    const close = vi.fn(async () => {})

    const tool = createMcpTool({
      Type: createTypeStub(),
      fsSync: fs,
      processApi: { env: { NINECLAW_MCP_CONFIG_FILE: configPath } },
      transportFactory: vi.fn(() => ({ close: vi.fn(async () => {}) })),
      clientFactory: vi.fn(() => ({ connect, listTools, close })),
    } as never)

    const result = await tool.execute('call-1', { operation: 'list_tools', server_id: 'miview' })

    expect(connect).toHaveBeenCalledTimes(1)
    expect(listTools).toHaveBeenCalledTimes(1)
    expect(close).toHaveBeenCalledTimes(1)
    expect(result.content[0].text).toContain('miview_search')
  })

  it('calls a remote tool with JSON arguments', async () => {
    const configPath = writeConfigFile({
      servers: [
        {
          id: 'miview',
          transport: 'streamable_http',
          url: 'http://127.0.0.1:25424/mcp',
          headers: {
            Authorization: 'Bearer token',
          },
          enabled: true,
        },
      ],
    })

    const callTool = vi.fn(async () => ({
      isError: false,
      content: [{ type: 'text', text: 'search result' }],
    }))

    const tool = createMcpTool({
      Type: createTypeStub(),
      fsSync: fs,
      processApi: { env: { NINECLAW_MCP_CONFIG_FILE: configPath } },
      transportFactory: vi.fn(() => ({ close: vi.fn(async () => {}) })),
      clientFactory: vi.fn(() => ({
        connect: vi.fn(async () => {}),
        callTool,
        close: vi.fn(async () => {}),
      })),
    } as never)

    const result = await tool.execute('call-2', {
      operation: 'call_tool',
      server_id: 'miview',
      tool_name: 'miview_search',
      arguments: { query: 'earnings' },
    })

    expect(callTool).toHaveBeenCalledWith({
      name: 'miview_search',
      arguments: { query: 'earnings' },
    })
    expect(result.content[0].text).toContain('search result')
  })
})
