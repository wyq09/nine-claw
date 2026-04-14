import type { CustomProviderMeta, PersistedChatAttachment, ProviderApiFormat, ProviderConfig } from '../../types'
import {
  createInitialProviderConfigs,
  defaultAppearanceSettings,
  defaultGeneralSettings,
  emptyProviderConfig,
} from '../../mockData'
import {
  APPEARANCE_SETTINGS_STORAGE_KEY,
  CUSTOM_PROVIDERS_META_KEY,
  GENERAL_SETTINGS_STORAGE_KEY,
  LEGACY_APPEARANCE_SETTINGS_STORAGE_KEYS,
  LEGACY_CUSTOM_PROVIDERS_META_KEYS,
  LEGACY_GENERAL_SETTINGS_STORAGE_KEYS,
  LEGACY_PROVIDER_CONFIGS_STORAGE_KEYS,
  PROVIDER_CONFIGS_STORAGE_KEY,
} from './appConstants'

export function readStoredStorageValue(storageKey: string, legacyKeys: string[] = []): string | null {
  const keys = [storageKey, ...legacyKeys.filter((item) => item !== storageKey)]

  for (const key of keys) {
    const raw = localStorage.getItem(key)
    if (raw === null) {
      continue
    }
    if (key !== storageKey) {
      localStorage.setItem(storageKey, raw)
    }
    return raw
  }

  return null
}

export function persistStoredStorageValue(storageKey: string, value: string, legacyKeys: string[] = []) {
  localStorage.setItem(storageKey, value)
  for (const key of legacyKeys) {
    if (key !== storageKey) {
      localStorage.removeItem(key)
    }
  }
}

export function clampNumber(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max)
}

export function toBotSendMediaType(kind: PersistedChatAttachment['kind']): 'image' | 'file' | 'video' {
  if (kind === 'image') {
    return 'image'
  }
  if (kind === 'video') {
    return 'video'
  }
  return 'file'
}

export function loadStoredState<T extends object>(storageKey: string, defaults: T, legacyKeys: string[] = []): T {
  try {
    const raw = readStoredStorageValue(storageKey, legacyKeys)
    if (!raw) {
      return defaults
    }

    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      return defaults
    }

    return { ...defaults, ...parsed }
  } catch {
    return defaults
  }
}

export function createInitialGeneralSettings() {
  return loadStoredState(GENERAL_SETTINGS_STORAGE_KEY, defaultGeneralSettings, LEGACY_GENERAL_SETTINGS_STORAGE_KEYS)
}

export function createInitialAppearanceState() {
  return loadStoredState(
    APPEARANCE_SETTINGS_STORAGE_KEY,
    defaultAppearanceSettings,
    LEGACY_APPEARANCE_SETTINGS_STORAGE_KEYS,
  )
}

export function loadProviderConfigs(): Record<string, ProviderConfig> {
  const defaults = createInitialProviderConfigs()
  try {
    const raw = readStoredStorageValue(PROVIDER_CONFIGS_STORAGE_KEY, LEGACY_PROVIDER_CONFIGS_STORAGE_KEYS)
    if (!raw) {
      return defaults
    }
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      return defaults
    }
    const result: Record<string, ProviderConfig> = { ...defaults }
    for (const key of Object.keys(parsed)) {
      const v = (parsed as Record<string, unknown>)[key]
      if (typeof v !== 'object' || v === null || Array.isArray(v)) {
        continue
      }
      const base = defaults[key] ?? emptyProviderConfig()
      result[key] = { ...base, ...(v as Partial<ProviderConfig>) }
    }
    return result
  } catch {
    return defaults
  }
}

export function loadCustomProviderMeta(): CustomProviderMeta[] {
  try {
    const raw = readStoredStorageValue(CUSTOM_PROVIDERS_META_KEY, LEGACY_CUSTOM_PROVIDERS_META_KEYS)
    if (!raw) {
      return []
    }
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) {
      return []
    }
    return parsed.flatMap((x) => {
      if (
        typeof x !== 'object' ||
        x === null ||
        typeof (x as CustomProviderMeta).id !== 'string' ||
        typeof (x as CustomProviderMeta).name !== 'string'
      ) {
        return []
      }

      return [
        {
          ...(x as CustomProviderMeta),
          apiFormat: normalizeProviderApiFormat((x as Partial<CustomProviderMeta>).apiFormat),
        },
      ]
    })
  } catch {
    return []
  }
}

export function normalizeProviderApiFormat(value: string | undefined, fallback: ProviderApiFormat = 'openai'): ProviderApiFormat {
  return value === 'anthropic' ? 'anthropic' : fallback
}

export function createInitialProviderState() {
  return loadProviderConfigs()
}

export function parseStoredProviderConfigs(raw: string | null | undefined): Record<string, ProviderConfig> | null {
  if (!raw) {
    return null
  }
  try {
    const defaults = createInitialProviderConfigs()
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      return null
    }
    const result: Record<string, ProviderConfig> = { ...defaults }
    for (const key of Object.keys(parsed)) {
      const value = (parsed as Record<string, unknown>)[key]
      if (typeof value !== 'object' || value === null || Array.isArray(value)) {
        continue
      }
      const base = defaults[key] ?? emptyProviderConfig()
      result[key] = { ...base, ...(value as Partial<ProviderConfig>) }
    }
    return result
  } catch {
    return null
  }
}

export function parseStoredCustomProviderMeta(raw: string | null | undefined): CustomProviderMeta[] | null {
  if (!raw) {
    return null
  }
  try {
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) {
      return null
    }
    return parsed.flatMap((x) => {
      if (
        typeof x !== 'object' ||
        x === null ||
        typeof (x as CustomProviderMeta).id !== 'string' ||
        typeof (x as CustomProviderMeta).name !== 'string'
      ) {
        return []
      }

      return [
        {
          ...(x as CustomProviderMeta),
          apiFormat: normalizeProviderApiFormat((x as Partial<CustomProviderMeta>).apiFormat),
        },
      ]
    })
  } catch {
    return null
  }
}
