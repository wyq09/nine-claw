import { invoke } from '@tauri-apps/api/core'
import type { ImageGenerationPreferencesPayload } from '../types/imageGeneration'

export async function loadImageGenerationPreferences(): Promise<ImageGenerationPreferencesPayload> {
  return invoke<ImageGenerationPreferencesPayload>('load_image_generation_preferences')
}

export async function saveImageGenerationPreferences(payload: {
  imageProviderConfigsPayload: string
  imageGenerationSystemPayload: string
}): Promise<void> {
  await invoke('save_image_generation_preferences', payload)
}
