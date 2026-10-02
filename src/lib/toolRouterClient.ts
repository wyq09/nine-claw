import { invoke } from '@tauri-apps/api/core'

/** 工具路由配置：legacy 走原有白名单；jev 每轮由 Jev 模型决定暴露哪些工具。 */
export interface ToolRouterSettings {
  mode: 'legacy' | 'jev'
  apiKey: string
  /** 固定版本号（默认 jev-1.13.0），避免模型漂移；不要使用 jev-latest。 */
  model: string
  timeoutMs: number
}

export const DEFAULT_TOOL_ROUTER_SETTINGS: ToolRouterSettings = {
  mode: 'legacy',
  apiKey: '',
  model: 'jev-1.13.0',
  timeoutMs: 2000,
}

export const TOOL_ROUTER_TIMEOUT_MIN_MS = 500
export const TOOL_ROUTER_TIMEOUT_MAX_MS = 10000

/** 超时毫秒数收敛到 [500, 10000]；非法值回退默认 2000。 */
export function clampToolRouterTimeoutMs(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_TOOL_ROUTER_SETTINGS.timeoutMs
  }
  return Math.min(
    TOOL_ROUTER_TIMEOUT_MAX_MS,
    Math.max(TOOL_ROUTER_TIMEOUT_MIN_MS, Math.round(value)),
  )
}

/** 归一化后端返回值：字段缺失/类型不符时回退默认，旧数据不会把 UI 打穿。 */
export function normalizeToolRouterSettings(raw: unknown): ToolRouterSettings {
  const source = (raw ?? {}) as Partial<ToolRouterSettings>
  const mode = source.mode === 'jev' ? 'jev' : 'legacy'
  const apiKey = typeof source.apiKey === 'string' ? source.apiKey : ''
  const model =
    typeof source.model === 'string' && source.model.trim() !== ''
      ? source.model
      : DEFAULT_TOOL_ROUTER_SETTINGS.model
  const timeoutMs =
    typeof source.timeoutMs === 'number'
      ? clampToolRouterTimeoutMs(source.timeoutMs)
      : DEFAULT_TOOL_ROUTER_SETTINGS.timeoutMs
  return { mode, apiKey, model, timeoutMs }
}

export async function loadToolRouterSettings(): Promise<ToolRouterSettings> {
  return invoke<ToolRouterSettings>('load_tool_router_settings_command').then(
    normalizeToolRouterSettings,
  )
}

export async function saveToolRouterSettings(
  settings: ToolRouterSettings,
): Promise<ToolRouterSettings> {
  return invoke<ToolRouterSettings>('save_tool_router_settings_command', {
    settings,
  }).then(normalizeToolRouterSettings)
}
