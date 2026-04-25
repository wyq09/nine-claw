import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import {
  loadImageGenerationPreferences,
  saveImageGenerationPreferences,
} from '../imageGenerationClient'

describe('imageGenerationClient', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('loads image generation preferences via invoke', async () => {
    const payload = {
      imageProviderConfigs: '{"openai":{}}',
      imageGenerationSystem: '{"defaultProviderId":"openai_image"}',
    }
    vi.mocked(invoke).mockResolvedValue(payload)

    await expect(loadImageGenerationPreferences()).resolves.toEqual(payload)
    expect(invoke).toHaveBeenCalledWith('load_image_generation_preferences')
  })

  it('saves image generation preferences via invoke', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)

    await saveImageGenerationPreferences({
      imageProviderConfigsPayload: '{"openai_image":{}}',
      imageGenerationSystemPayload: '{"defaultProviderId":"openai_image"}',
    })

    expect(invoke).toHaveBeenCalledWith('save_image_generation_preferences', {
      imageProviderConfigsPayload: '{"openai_image":{}}',
      imageGenerationSystemPayload: '{"defaultProviderId":"openai_image"}',
    })
  })
})
