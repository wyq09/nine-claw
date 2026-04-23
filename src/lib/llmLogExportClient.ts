import { invoke } from '@tauri-apps/api/core'

export type LlmLogPreview = {
  file: string | null
  tail: string
}

export async function llmLogExportGet(): Promise<string | null> {
  return invoke<string | null>('llm_log_export_get')
}

export async function llmLogExportSet(path: string | null): Promise<void> {
  return invoke('llm_log_export_set', { path })
}

export async function llmLogExportPreview(): Promise<LlmLogPreview> {
  return invoke<LlmLogPreview>('llm_log_export_preview')
}
