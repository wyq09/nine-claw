export type ImageProviderId = string

export type ImageProviderAdapterType = 'openai_images' | 'openai_compatible' | 'apimart_gpt_image_2'

export type ImageProviderDefinition = {
  id: ImageProviderId
  name: string
  defaultBaseUrl: string
  suggestedModel: string
  description: string
  adapterType: ImageProviderAdapterType
}

export type ImageProviderConfig = {
  adapterType: ImageProviderAdapterType
  baseUrl: string
  apiKey: string
  model: string
  note: string
  displayName: string
  status: '未配置' | '已配置' | '测试通过'
}

export type ImageGenerationBackground = 'auto' | 'transparent' | 'opaque'

export type ImageGenerationOutputFormat = 'png' | 'jpeg' | 'webp'

export type ImageGenerationQuality = 'auto' | 'low' | 'medium' | 'high'

export type ImageGenerationResolution = '1k' | '2k' | '4k'

export type ImageGenerationSystemConfig = {
  defaultProviderId: ImageProviderId
  size: string
  resolution: ImageGenerationResolution
  background: ImageGenerationBackground
  outputFormat: ImageGenerationOutputFormat
  quality: ImageGenerationQuality
  count: number
}

export type ImageGenerationPreferencesPayload = {
  imageProviderConfigs?: string | null
  imageGenerationSystem?: string | null
}
