import { invoke } from '@tauri-apps/api/core'
import type { McpSettings } from '../types/mcp'

export async function loadMcpSettings(): Promise<McpSettings> {
  return invoke<McpSettings>('load_mcp_settings')
}

export async function saveMcpSettings(settings: McpSettings): Promise<McpSettings> {
  return invoke<McpSettings>('save_mcp_settings', { settings })
}
