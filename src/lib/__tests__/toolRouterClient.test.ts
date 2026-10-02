import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import {
  DEFAULT_TOOL_ROUTER_SETTINGS,
  clampToolRouterTimeoutMs,
  loadToolRouterSettings,
  normalizeToolRouterSettings,
  saveToolRouterSettings,
  type ToolRouterSettings,
} from '../toolRouterClient'

describe('toolRouterClient', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('loads settings through the backend command', async () => {
    vi.mocked(invoke).mockResolvedValue({
      mode: 'jev',
      apiKey: 'sk-jev',
      model: 'jev-1.13.0',
      timeoutMs: 3000,
    })

    const result = await loadToolRouterSettings()

    expect(invoke).toHaveBeenCalledWith('load_tool_router_settings_command')
    expect(result).toEqual({
      mode: 'jev',
      apiKey: 'sk-jev',
      model: 'jev-1.13.0',
      timeoutMs: 3000,
    })
  })

  it('normalizes malformed backend payloads back to defaults', async () => {
    vi.mocked(invoke).mockResolvedValue({ mode: 'weird', timeoutMs: 'fast' })

    const result = await loadToolRouterSettings()

    expect(result).toEqual(DEFAULT_TOOL_ROUTER_SETTINGS)
  })

  it('falls back to the pinned model when model is missing', () => {
    expect(normalizeToolRouterSettings({ mode: 'jev' }).model).toBe('jev-1.13.0')
    expect(normalizeToolRouterSettings({ model: 'jev-latest' }).model).toBe('jev-latest')
  })

  it('saves settings and echoes the normalized payload', async () => {
    vi.mocked(invoke).mockResolvedValue({
      mode: 'jev',
      apiKey: 'sk-jev',
      model: 'jev-1.13.0',
      timeoutMs: 2000,
    })
    const settings: ToolRouterSettings = { ...DEFAULT_TOOL_ROUTER_SETTINGS, mode: 'jev', apiKey: 'sk-jev' }

    const result = await saveToolRouterSettings(settings)

    expect(invoke).toHaveBeenCalledWith('save_tool_router_settings_command', { settings })
    expect(result).toEqual({ ...settings, apiKey: 'sk-jev' })
  })

  it('clamps timeout into the 500–10000 window', () => {
    expect(clampToolRouterTimeoutMs(100)).toBe(500)
    expect(clampToolRouterTimeoutMs(999999)).toBe(10000)
    expect(clampToolRouterTimeoutMs(2000)).toBe(2000)
    expect(clampToolRouterTimeoutMs(Number.NaN)).toBe(2000)
  })
})
