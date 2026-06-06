import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import { loadMcpSettings, saveMcpSettings } from '../mcpClient'

describe('mcpClient', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('loads mcp settings through tauri invoke', async () => {
    vi.mocked(invoke).mockResolvedValue({ servers: [] })

    const result = await loadMcpSettings()

    expect(invoke).toHaveBeenCalledWith('load_mcp_settings')
    expect(result).toEqual({ servers: [] })
  })

  it('saves mcp settings through tauri invoke', async () => {
    const settings = {
      servers: [
        {
          id: 'miview',
          name: '',
          transport: 'streamable_http',
          enabled: true,
          command: '',
          args: [],
          env: {},
          cwd: '',
          url: 'http://127.0.0.1:25424/mcp',
          headers: { Authorization: 'Bearer token' },
        },
      ],
    }
    vi.mocked(invoke).mockResolvedValue(settings)

    const result = await saveMcpSettings(settings)

    expect(invoke).toHaveBeenCalledWith('save_mcp_settings', { settings })
    expect(result).toEqual(settings)
  })
})
