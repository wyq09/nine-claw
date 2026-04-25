import { describe, expect, it } from 'vitest'

import {
  parseStoredImageGenerationSystemConfig,
  parseStoredImageProviderConfigs,
} from '../appStorage'

describe('image generation storage helpers', () => {
  it('parses provider configs and keeps defaults for missing fields', () => {
    const parsed = parseStoredImageProviderConfigs(
      JSON.stringify({
        custom_image: {
          baseUrl: 'https://example.com/v1',
          apiKey: 'secret',
          model: 'flux-dev',
          displayName: 'Custom Flux',
        },
      }),
    )

    expect(parsed?.custom_image).toMatchObject({
      baseUrl: 'https://example.com/v1',
      apiKey: 'secret',
      model: 'flux-dev',
      displayName: 'Custom Flux',
      adapterType: 'openai_compatible',
    })
  })

  it('parses system config and clamps count to supported range', () => {
    const parsed = parseStoredImageGenerationSystemConfig(
      JSON.stringify({
        defaultProviderId: 'custom_image',
        size: '1536x1024',
        count: 9,
      }),
    )

    expect(parsed).toEqual({
      defaultProviderId: 'custom_image',
      size: '1536x1024',
      resolution: '1k',
      background: 'auto',
      outputFormat: 'png',
      quality: 'auto',
      count: 4,
    })
  })
})
