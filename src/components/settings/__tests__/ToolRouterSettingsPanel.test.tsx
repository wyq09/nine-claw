import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../../../lib/toolRouterClient', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/toolRouterClient')>()
  return {
    ...actual,
    loadToolRouterSettings: vi.fn(),
    saveToolRouterSettings: vi.fn(),
  }
})

import {
  DEFAULT_TOOL_ROUTER_SETTINGS,
  loadToolRouterSettings,
  saveToolRouterSettings,
  type ToolRouterSettings,
} from '../../../lib/toolRouterClient'
import { ToolRouterSettingsPanel } from '../ToolRouterSettingsPanel'

const storedSettings: ToolRouterSettings = {
  mode: 'legacy',
  apiKey: '',
  model: 'jev-1.13.0',
  timeoutMs: 2000,
}

describe('ToolRouterSettingsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(loadToolRouterSettings).mockResolvedValue({ ...storedSettings })
    vi.mocked(saveToolRouterSettings).mockImplementation(async (settings) => settings)
  })

  it('loads stored settings on mount', async () => {
    vi.mocked(loadToolRouterSettings).mockResolvedValue({
      mode: 'jev',
      apiKey: 'sk-jev',
      model: 'jev-1.13.0',
      timeoutMs: 3000,
    })

    render(<ToolRouterSettingsPanel />)

    expect(await screen.findByLabelText('Jev API Key')).toHaveValue('sk-jev')
    expect(screen.getByLabelText('Jev 模型名称')).toHaveValue('jev-1.13.0')
    expect(screen.getByText('Jev 模式', { selector: '.image-settings-summary-card strong' })).toBeInTheDocument()
    expect(loadToolRouterSettings).toHaveBeenCalledTimes(1)
  })

  it('defaults to legacy mode with the pinned model', async () => {
    render(<ToolRouterSettingsPanel />)

    await waitFor(() => {
      expect(screen.getByText('原有模式', { selector: '.image-settings-summary-card strong' })).toBeInTheDocument()
    })
    expect(screen.getByLabelText('Jev 模型名称')).toHaveValue(DEFAULT_TOOL_ROUTER_SETTINGS.model)
    expect(screen.getByText('保存工具路由配置')).toBeEnabled()
    expect(screen.queryByText(/尚未填写 API Key/)).not.toBeInTheDocument()
  })

  it('toggles to jev mode and saves the expected payload', async () => {
    render(<ToolRouterSettingsPanel />)

    await waitFor(() => {
      expect(screen.getByText('原有模式', { selector: '.image-settings-summary-card strong' })).toBeInTheDocument()
    })
    fireEvent.change(screen.getByLabelText('Jev API Key'), { target: { value: 'sk-new' } })
    fireEvent.click(screen.getByRole('tab', { name: 'Jev 模式' }))
    fireEvent.click(screen.getByText('保存工具路由配置'))

    await waitFor(() => {
      expect(saveToolRouterSettings).toHaveBeenCalledTimes(1)
    })
    expect(vi.mocked(saveToolRouterSettings).mock.calls[0][0]).toEqual({
      mode: 'jev',
      apiKey: 'sk-new',
      model: 'jev-1.13.0',
      timeoutMs: 2000,
    })
    expect(await screen.findByText('工具路由配置已保存，新会话起生效。')).toBeInTheDocument()
  })

  it('clamps the timeout back into the 500–10000 window', async () => {
    render(<ToolRouterSettingsPanel />)

    const timeoutField = await screen.findByLabelText('路由超时（毫秒）')
    fireEvent.focus(timeoutField)
    fireEvent.change(timeoutField, { target: { value: '50' } })
    fireEvent.blur(timeoutField)
    fireEvent.click(screen.getByText('保存工具路由配置'))

    await waitFor(() => {
      expect(saveToolRouterSettings).toHaveBeenCalledTimes(1)
    })
    expect(vi.mocked(saveToolRouterSettings).mock.calls[0][0].timeoutMs).toBe(500)
  })

  it('shows an inline hint when jev mode has no api key and still allows saving', async () => {
    render(<ToolRouterSettingsPanel />)

    await waitFor(() => {
      expect(screen.getByText('原有模式', { selector: '.image-settings-summary-card strong' })).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole('tab', { name: 'Jev 模式' }))

    expect(
      await screen.findByText(/在补齐密钥前，后端会继续按原有模式路由工具/),
    ).toBeInTheDocument()

    fireEvent.click(screen.getByText('保存工具路由配置'))
    await waitFor(() => {
      expect(saveToolRouterSettings).toHaveBeenCalledWith(
        expect.objectContaining({ mode: 'jev', apiKey: '' }),
      )
    })
  })

  it('surfaces backend save failures', async () => {
    vi.mocked(saveToolRouterSettings).mockRejectedValue(new Error('db locked'))

    render(<ToolRouterSettingsPanel />)
    fireEvent.click(await screen.findByText('保存工具路由配置'))

    expect(await screen.findByText('db locked')).toBeInTheDocument()
  })
})
