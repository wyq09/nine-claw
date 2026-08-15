import type { Mock } from 'vitest'
import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'

const invokeMock = invoke as unknown as Mock

import { loadImageVisionPreferences, saveImageVisionPreferences } from '../imageVisionClient'
import {
  DEFAULT_IMAGE_VISION_SYSTEM_CONFIG,
  isEmptyImageVisionConfig,
  isImageVisionConfigured,
  parseStoredImageVisionSystemConfig,
} from '../../types/imageVision'

describe('imageVisionClient', () => {
  it('invokes the backend load/save commands', async () => {
    invokeMock.mockReset()
    invokeMock.mockResolvedValue('{"model":"m"}')

    await expect(loadImageVisionPreferences()).resolves.toBe('{"model":"m"}')
    expect(invoke).toHaveBeenCalledWith('load_image_vision_preferences')

    await saveImageVisionPreferences('{"model":"m"}')
    expect(invoke).toHaveBeenCalledWith('save_image_vision_preferences', {
      imageVisionSystemPayload: '{"model":"m"}',
    })
  })
})

describe('imageVision config helpers', () => {
  it('parses and normalizes stored config payloads', () => {
    const parsed = parseStoredImageVisionSystemConfig(
      JSON.stringify({
        apiFormat: 'anthropic',
        baseUrl: ' https://api.anthropic.com ',
        apiKey: ' key ',
        model: ' claude-sonnet-4-5 ',
        maxOutputTokens: 999999,
        defaultPrompt: '看图说话',
      }),
    )
    expect(parsed).toEqual({
      apiFormat: 'anthropic',
      baseUrl: 'https://api.anthropic.com',
      apiKey: 'key',
      model: 'claude-sonnet-4-5',
      maxOutputTokens: 16384,
      defaultPrompt: '看图说话',
    })
  })

  it('falls back safely for invalid payloads', () => {
    expect(parseStoredImageVisionSystemConfig(null)).toBeNull()
    expect(parseStoredImageVisionSystemConfig('')).toBeNull()
    expect(parseStoredImageVisionSystemConfig('not-json')).toBeNull()
    expect(parseStoredImageVisionSystemConfig(JSON.stringify({ maxOutputTokens: 'oops' }))).toEqual(
      expect.objectContaining({ maxOutputTokens: DEFAULT_IMAGE_VISION_SYSTEM_CONFIG.maxOutputTokens }),
    )
  })

  it('detects configured and empty states', () => {
    const blank = { ...DEFAULT_IMAGE_VISION_SYSTEM_CONFIG, baseUrl: '', model: '' }
    expect(isEmptyImageVisionConfig(blank)).toBe(true)
    expect(isImageVisionConfigured(blank)).toBe(false)
    expect(isImageVisionConfigured(DEFAULT_IMAGE_VISION_SYSTEM_CONFIG)).toBe(false)
    expect(
      isImageVisionConfigured({
        ...DEFAULT_IMAGE_VISION_SYSTEM_CONFIG,
        apiKey: 'k',
        model: 'm',
      }),
    ).toBe(true)
  })
})