import { invoke } from '@tauri-apps/api/core'

export async function loadImageVisionPreferences(): Promise<string | null> {
  return invoke<string | null>('load_image_vision_preferences')
}

export async function saveImageVisionPreferences(payload: string): Promise<void> {
  await invoke('save_image_vision_preferences', { imageVisionSystemPayload: payload })
}