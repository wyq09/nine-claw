import { act, fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { McpSettingsPanel } from './McpSettingsPanel'

vi.mock('../../lib/mcpClient', () => ({
  loadMcpSettings: vi.fn().mockResolvedValue({ servers: [] }),
  saveMcpSettings: vi.fn(async (settings) => settings),
}))

import { loadMcpSettings, saveMcpSettings } from '../../lib/mcpClient'

describe('McpSettingsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(loadMcpSettings).mockResolvedValue({ servers: [] })
    vi.mocked(saveMcpSettings).mockImplementation(async (settings) => settings)
  })

  it('loads existing settings on mount', async () => {
    vi.mocked(loadMcpSettings).mockResolvedValue({
      servers: [
        {
          id: 'miview',
          name: 'MiView',
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
    })

    render(<McpSettingsPanel />)

    expect(await screen.findByDisplayValue('miview')).toBeInTheDocument()
    expect(loadMcpSettings).toHaveBeenCalledTimes(1)
  })

  it('imports pasted json and saves normalized settings', async () => {
    render(<McpSettingsPanel />)
    await screen.findByText('MCP 接入')

    await act(async () => {
      fireEvent.change(screen.getByPlaceholderText(/"mcpServers"/), {
        target: {
          value: `{
            "mcpServers": {
              "miview": {
                "url": "http://127.0.0.1:25424/mcp",
                "headers": {
                  "Authorization": "Bearer miview_3c2f7b0e74c54d23bc45f869cbf4b575"
                }
              }
            }
          }`,
        },
      })
    })

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '导入 JSON' }))
    })

    expect(await screen.findByDisplayValue('miview')).toBeInTheDocument()

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '保存 MCP 设置' }))
    })

    expect(saveMcpSettings).toHaveBeenCalledWith({
      servers: [
        expect.objectContaining({
          id: 'miview',
          transport: 'streamable_http',
          url: 'http://127.0.0.1:25424/mcp',
          headers: {
            Authorization: 'Bearer miview_3c2f7b0e74c54d23bc45f869cbf4b575',
          },
        }),
      ],
    })
  })
})
