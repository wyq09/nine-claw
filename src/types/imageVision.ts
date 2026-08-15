export type ImageVisionApiFormat = 'openai' | 'anthropic'

export type ImageVisionSystemConfig = {
  apiFormat: ImageVisionApiFormat
  baseUrl: string
  apiKey: string
  model: string
  maxOutputTokens: number
  defaultPrompt: string
}

export const DEFAULT_IMAGE_VISION_SYSTEM_CONFIG: ImageVisionSystemConfig = {
  apiFormat: 'openai',
  baseUrl: 'https://api.openai.com/v1',
  apiKey: '',
  model: 'gpt-4o-mini',
  maxOutputTokens: 2048,
  defaultPrompt: '',
}

export function isEmptyImageVisionConfig(config: ImageVisionSystemConfig): boolean {
  return !config.baseUrl.trim() && !config.apiKey.trim() && !config.model.trim()
}

export function isImageVisionConfigured(config: ImageVisionSystemConfig): boolean {
  return Boolean(config.baseUrl.trim() && config.apiKey.trim() && config.model.trim())
}

function normalizeApiFormat(value: unknown): ImageVisionApiFormat {
  return value === 'anthropic' ? 'anthropic' : 'openai'
}

function normalizeTokenCount(value: unknown): number {
  const parsed = typeof value === 'number' ? value : Number.parseInt(String(value ?? ''), 10)
  if (!Number.isFinite(parsed)) {
    return DEFAULT_IMAGE_VISION_SYSTEM_CONFIG.maxOutputTokens
  }
  return Math.min(16384, Math.max(256, Math.trunc(parsed)))
}

function normalizeString(value: unknown): string {
  return typeof value === 'string' ? value.trim() : ''
}

export function parseStoredImageVisionSystemConfig(
  raw: string | null | undefined,
): ImageVisionSystemConfig | null {
  if (!raw || !raw.trim()) {
    return null
  }
  try {
    const parsed = JSON.parse(raw) as Record<string, unknown>
    return {
      apiFormat: normalizeApiFormat(parsed.apiFormat),
      baseUrl: normalizeString(parsed.baseUrl),
      apiKey: normalizeString(parsed.apiKey),
      model: normalizeString(parsed.model),
      maxOutputTokens: normalizeTokenCount(parsed.maxOutputTokens),
      defaultPrompt: typeof parsed.defaultPrompt === 'string' ? parsed.defaultPrompt : '',
    }
  } catch {
    return null
  }
}