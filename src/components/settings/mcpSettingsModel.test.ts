import { describe, expect, it } from 'vitest'
import {
  formatCommandArgs,
  parseJsonRecord,
  parseMcpServersImport,
  splitCommandArgs,
} from './mcpSettingsModel'

describe('mcpSettingsModel', () => {
  it('parses streamable http import json', () => {
    const parsed = parseMcpServersImport(`{
      "mcpServers": {
        "miview": {
          "url": "http://127.0.0.1:25424/mcp",
          "headers": {
            "Authorization": "Bearer miview_3c2f7b0e74c54d23bc45f869cbf4b575"
          }
        }
      }
    }`)

    expect(parsed.servers).toEqual([
      expect.objectContaining({
        id: 'miview',
        transport: 'streamable_http',
        url: 'http://127.0.0.1:25424/mcp',
        headers: {
          Authorization: 'Bearer miview_3c2f7b0e74c54d23bc45f869cbf4b575',
        },
      }),
    ])
  })

  it('infers stdio transport from command', () => {
    const parsed = parseMcpServersImport(`{
      "mcpServers": {
        "filesystem": {
          "command": "npx",
          "args": "-y @modelcontextprotocol/server-filesystem /tmp"
        }
      }
    }`)

    expect(parsed.servers[0]).toEqual(
      expect.objectContaining({
        id: 'filesystem',
        transport: 'stdio',
        command: 'npx',
        args: ['-y', '@modelcontextprotocol/server-filesystem', '/tmp'],
      }),
    )
  })

  it('splits and re-formats shell-like args', () => {
    const args = splitCommandArgs(`-y "server name" '/tmp/a b'`)
    expect(args).toEqual(['-y', 'server name', '/tmp/a b'])
    expect(formatCommandArgs(args)).toBe('-y "server name" "/tmp/a b"')
  })

  it('rejects non-string json record values', () => {
    expect(() => parseJsonRecord('{"Authorization": 1}', '请求头')).toThrow('请求头 的值必须全部是字符串')
  })
})
