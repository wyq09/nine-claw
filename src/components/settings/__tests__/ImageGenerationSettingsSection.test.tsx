import { fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { describe, expect, it } from 'vitest'
import { emptyImageProviderConfig } from '../../../mockData'
import { ImageGenerationSettingsSection } from '../ImageGenerationSettingsSection'
import type {
  ImageGenerationSystemConfig,
  ImageProviderAdapterType,
  ImageProviderConfig,
  ImageProviderDefinition,
  ImageProviderId,
} from '../../../types/imageGeneration'

const imageProviderDefinitions: ImageProviderDefinition[] = [
  {
    id: 'openai_image',
    name: 'OpenAI Images',
    description: 'OpenAI 原生图片生成接口',
    adapterType: 'openai_images',
    defaultBaseUrl: 'https://api.openai.com/v1',
    suggestedModel: 'gpt-image-1',
  },
]

const initialConfigs: Record<string, ImageProviderConfig> = {
  openai_image: {
    adapterType: 'openai_images',
    displayName: '',
    baseUrl: 'https://api.openai.com/v1',
    apiKey: 'secret',
    model: 'gpt-image-1',
    note: '',
    status: '已配置',
  },
}

const initialSystem: ImageGenerationSystemConfig = {
  defaultProviderId: 'openai_image',
  size: '1024x1024',
  resolution: '1k',
  background: 'auto',
  outputFormat: 'png',
  quality: 'auto',
  count: 1,
}

function TestHarness() {
  const [imageProviderConfigs, setImageProviderConfigs] = useState(initialConfigs)
  const [imageGenerationSystem, setImageGenerationSystem] = useState(initialSystem)

  const addImageProvider = (name: string, adapterType: ImageProviderAdapterType): ImageProviderId => {
    const nextId = `custom_image_${Object.keys(imageProviderConfigs).length}`
    setImageProviderConfigs((previous) => ({
      ...previous,
      [nextId]: {
        ...emptyImageProviderConfig(adapterType),
        displayName: name,
        baseUrl: 'https://example.com/v1',
        model: `${name}-model`,
      },
    }))
    return nextId
  }

  return (
    <ImageGenerationSettingsSection
      imageGenerationSystem={imageGenerationSystem}
      imageProviderConfigs={imageProviderConfigs}
      imageProviderDefinitions={imageProviderDefinitions}
      onAddImageProvider={addImageProvider}
      onImageGenerationSystemChange={setImageGenerationSystem}
      onImageProviderConfigChange={(providerId, updates) =>
        setImageProviderConfigs((previous) => ({
          ...previous,
          [providerId]: {
            ...previous[providerId],
            ...updates,
          },
        }))
      }
      onRemoveImageProvider={(providerId) =>
        setImageProviderConfigs((previous) => {
          const next = { ...previous }
          delete next[providerId]
          return next
        })
      }
    />
  )
}

describe('ImageGenerationSettingsSection', () => {
  it('adds multiple custom providers and allows choosing one as default', () => {
    const { container } = render(<TestHarness />)

    fireEvent.click(screen.getByRole('button', { name: '添加图片 Provider' }))
    const nameInputs = screen.getAllByPlaceholderText('例如：Flux 海报专用')
    fireEvent.change(nameInputs[0], { target: { value: 'Flux One' } })
    fireEvent.click(screen.getByRole('button', { name: '创建' }))

    fireEvent.click(screen.getByRole('button', { name: '添加图片 Provider' }))
    fireEvent.change(screen.getAllByPlaceholderText('例如：Flux 海报专用')[0], {
      target: { value: 'Poster Two' },
    })
    fireEvent.click(screen.getByRole('button', { name: '创建' }))

    const selects = container.querySelectorAll('select')
    expect(selects[0]).toBeTruthy()

    fireEvent.change(selects[0], { target: { value: 'custom_image_2' } })

    expect(screen.getByLabelText('默认图片提供方')).toHaveValue('custom_image_2')
    expect(selects[0]).toHaveValue('custom_image_2')
    expect(screen.getByRole('button', { name: /Poster Two/ })).toBeInTheDocument()
  })
})
