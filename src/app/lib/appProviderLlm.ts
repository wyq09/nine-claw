import type { KeyboardEvent } from 'react'
import type {
  AgentInput,
  AgentScenarioLlmConfig,
  AgentScenarioLlmSlot,
  ConversationAgentSnapshot,
  HistoryItem,
  ProviderConfig,
  ProviderDefinition,
  ProviderId,
  ProviderRuntimeConfig,
  SubmitShortcut,
} from '../../types'

export function getProviderStatus(
  config: ProviderConfig,
  preserveVerifiedStatus = true,
): ProviderConfig['status'] {
  if (!config.baseUrl.trim() || !config.model.trim()) {
    return '未配置'
  }

  return preserveVerifiedStatus && config.status === '测试通过' ? '测试通过' : '已配置'
}

export function getSubmitShortcutLabel(shortcut: SubmitShortcut): string {
  return shortcut === 'enter' ? 'Enter' : 'Ctrl / Cmd + Enter'
}

/** Fn 键本身或系统标记的 Fn 修饰（用于 Escape 关闭弹窗等场景避免与系统快捷键冲突） */
export function isFnLikeKeyboardEvent(event: Pick<globalThis.KeyboardEvent, 'code' | 'key' | 'location' | 'getModifierState'>): boolean {
  if (event.key === 'Fn' || event.key === 'Function' || event.code === 'Fn') {
    return true
  }

  if (event.getModifierState?.('Fn')) {
    return true
  }

  return false
}

/**
 * macOS 上 Fn/地球键组合可能把 Return 报成「小键盘 Enter」；仅应对 Enter，避免把其它 NUMPAD 区按键一律当成 Fn 场景。
 */
export function isMacOSFnStyleEnterKey(event: Pick<globalThis.KeyboardEvent, 'code' | 'key' | 'location'>): boolean {
  if (event.key !== 'Enter') {
    return false
  }
  return event.code === 'NumpadEnter' || event.location === globalThis.KeyboardEvent.DOM_KEY_LOCATION_NUMPAD
}

export function providerDisplayName(definition: ProviderDefinition, config: ProviderConfig | undefined): string {
  const label = config?.displayName?.trim()
  if (label) {
    return label
  }
  return definition.name
}

export function hasProviderDefinition(providerId: ProviderId, definitions: ProviderDefinition[]): boolean {
  return definitions.some((item) => item.id === providerId)
}

export function isProviderAvailable(
  providerId: ProviderId,
  definitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
): boolean {
  return hasProviderDefinition(providerId, definitions) && providerConfigs[providerId]?.added === true
}

export function isValidConfiguredModelReference(
  providerId: ProviderId,
  model: string,
  definitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
): boolean {
  if (!providerId.trim() || !model.trim() || !isProviderAvailable(providerId, definitions, providerConfigs)) {
    return false
  }
  const config = providerConfigs[providerId]
  return isProviderConfigComplete(config) && config.model.trim() === model.trim()
}

export function pickFallbackSessionLlm(
  definitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
  preferredProviderId?: ProviderId | null,
): { providerId: ProviderId; model: string } | null {
  const orderedDefinitions = preferredProviderId
    ? [
        ...definitions.filter((item) => item.id === preferredProviderId),
        ...definitions.filter((item) => item.id !== preferredProviderId),
      ]
    : definitions

  for (const definition of orderedDefinitions) {
    const config = providerConfigs[definition.id]
    if (config?.added !== true || !isProviderConfigComplete(config)) {
      continue
    }
    return {
      providerId: definition.id,
      model: config.model.trim(),
    }
  }

  return null
}

export function pickFallbackAgentModel(
  definitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
  preferredProviderId?: ProviderId | null,
): { providerId: ProviderId; model: string } | null {
  const runtimeFallback = pickFallbackSessionLlm(definitions, providerConfigs, preferredProviderId)
  if (runtimeFallback) {
    return runtimeFallback
  }

  const preferredDefinition =
    (preferredProviderId &&
      definitions.find((item) => item.id === preferredProviderId && providerConfigs[item.id]?.added === true)) ??
    definitions.find((item) => providerConfigs[item.id]?.added === true) ??
    definitions[0] ??
    null
  if (!preferredDefinition) {
    return null
  }

  const configuredModel = providerConfigs[preferredDefinition.id]?.model?.trim()
  return {
    providerId: preferredDefinition.id,
    model: configuredModel || preferredDefinition.suggestedModel,
  }
}

export function sanitizeAgentScenarioSlot(
  slot: AgentScenarioLlmSlot | undefined,
  definitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
): AgentScenarioLlmSlot | undefined {
  if (!slot) {
    return undefined
  }
  if (isValidConfiguredModelReference(slot.providerId, slot.model, definitions, providerConfigs)) {
    return slot
  }
  return undefined
}

export function sanitizeAgentScenarioLlmConfig(
  config: AgentScenarioLlmConfig | undefined,
  definitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
): AgentScenarioLlmConfig | undefined {
  if (!config) {
    return undefined
  }
  const titleGeneration = sanitizeAgentScenarioSlot(config.titleGeneration, definitions, providerConfigs)
  const memoryExtraction = sanitizeAgentScenarioSlot(config.memoryExtraction, definitions, providerConfigs)
  const taskPushNotificationCopy = sanitizeAgentScenarioSlot(
    config.taskPushNotificationCopy,
    definitions,
    providerConfigs,
  )
  if (!titleGeneration && !memoryExtraction && !taskPushNotificationCopy) {
    return undefined
  }
  return { titleGeneration, memoryExtraction, taskPushNotificationCopy }
}

export function sanitizeAgentInputModelReference(
  draft: AgentInput,
  definitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
): AgentInput {
  let next = draft

  if (
    !isValidConfiguredModelReference(
      draft.defaultProviderId,
      draft.defaultModel,
      definitions,
      providerConfigs,
    )
  ) {
    const fallback = pickFallbackAgentModel(definitions, providerConfigs, draft.defaultProviderId)
    if (fallback) {
      next = {
        ...draft,
        defaultProviderId: fallback.providerId,
        defaultModel: fallback.model,
      }
    }
  }

  const scenarioLlmConfig = sanitizeAgentScenarioLlmConfig(next.scenarioLlmConfig, definitions, providerConfigs)
  if (scenarioLlmConfig === next.scenarioLlmConfig) {
    return next
  }
  return { ...next, scenarioLlmConfig }
}

export function shouldSubmitWithShortcut(
  event: KeyboardEvent<HTMLTextAreaElement>,
  submitShortcut: SubmitShortcut,
): boolean {
  if (event.nativeEvent.isComposing || event.key !== 'Enter') {
    return false
  }

  // On macOS, Fn/Globe combinations can surface as keypad-style Enter events.
  // Keep submission bound to the standard Return key so those system behaviors stay untouched.
  if (isFnLikeKeyboardEvent(event.nativeEvent) || isMacOSFnStyleEnterKey(event.nativeEvent)) {
    return false
  }

  if (submitShortcut === 'enter') {
    return !event.shiftKey && !event.ctrlKey && !event.metaKey && !event.altKey
  }

  return !event.shiftKey && !event.altKey && (event.ctrlKey || event.metaKey)
}

export function resolveActiveProviderConfig(
  selectedProviderId: ProviderId,
  providerConfigs: Record<string, ProviderConfig>,
  allProviderIds: string[],
): ProviderRuntimeConfig | null {
  const orderedProviderIds = [
    selectedProviderId,
    ...allProviderIds.filter((providerId) => providerId !== selectedProviderId),
  ]

  for (const providerId of orderedProviderIds) {
    const config = providerConfigs[providerId]
    if (!config?.added || !config.enabled || !config.model.trim()) {
      continue
    }

    return {
      providerId,
      apiFormat: config.apiFormat,
      baseUrl: config.baseUrl.trim(),
      apiKey: config.apiKey.trim(),
      model: config.model.trim(),
    }
  }

  return null
}

/** 已填写 Base URL、API Key、模型名（与会话选择器一致，不要求「启用」开关） */
export function isProviderConfigComplete(cfg: ProviderConfig | undefined): boolean {
  return Boolean(cfg?.baseUrl.trim() && cfg.apiKey.trim() && cfg.model.trim())
}

export function sessionLlmEncode(providerId: ProviderId, model: string): string {
  return encodeURIComponent(JSON.stringify({ p: providerId, m: model.trim() }))
}

export function sessionLlmDecode(value: string): { providerId: ProviderId; model: string } | null {
  try {
    const raw: unknown = JSON.parse(decodeURIComponent(value))
    if (typeof raw !== 'object' || raw === null) {
      return null
    }
    const o = raw as Record<string, unknown>
    if (typeof o.p !== 'string' || typeof o.m !== 'string') {
      return null
    }
    return { providerId: o.p, model: o.m }
  } catch {
    return null
  }
}

export function buildSessionLlmSelectOptions(
  mergedProviderDefinitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
): { value: string; label: string }[] {
  const out: { value: string; label: string }[] = []
  for (const def of mergedProviderDefinitions) {
    const cfg = providerConfigs[def.id]
    if (!cfg?.added || !isProviderConfigComplete(cfg)) {
      continue
    }
    const m = cfg.model.trim()
    out.push({
      value: sessionLlmEncode(def.id, m),
      label: `${providerDisplayName(def, cfg)} · ${m}`,
    })
  }
  return out
}

export function buildSessionLlmOptionsWithFallback(
  options: { value: string; label: string }[],
  mergedProviderDefinitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
  current: { providerId: ProviderId; model: string },
): { value: string; label: string }[] {
  if (
    !isValidConfiguredModelReference(
      current.providerId,
      current.model,
      mergedProviderDefinitions,
      providerConfigs,
    )
  ) {
    return options
  }

  const encodedCurrent = sessionLlmEncode(current.providerId, current.model.trim())
  if (options.some((option) => option.value === encodedCurrent)) {
    return options
  }

  const definition = mergedProviderDefinitions.find((item) => item.id === current.providerId)
  const config = providerConfigs[current.providerId]
  const label =
    definition && config
      ? `${providerDisplayName(definition, config)} · ${current.model.trim()}`
      : `${current.providerId} · ${current.model.trim()}`

  return [{ value: encodedCurrent, label: `${label}（当前）` }, ...options]
}

export function resolveRuntimeFromSessionFields(
  providerId: ProviderId,
  model: string,
  providerConfigs: Record<string, ProviderConfig>,
): ProviderRuntimeConfig | null {
  const cfg = providerConfigs[providerId]
  if (!cfg?.added || !isProviderConfigComplete(cfg) || !model.trim()) {
    return null
  }
  return {
    providerId,
    apiFormat: cfg!.apiFormat,
    baseUrl: cfg!.baseUrl.trim(),
    apiKey: cfg!.apiKey.trim(),
    model: model.trim(),
  }
}

export function resolveRuntimeFromAgentSnapshot(
  agent: ConversationAgentSnapshot | null,
  providerConfigs: Record<string, ProviderConfig>,
): ProviderRuntimeConfig | null {
  if (!agent) {
    return null
  }

  return resolveRuntimeFromSessionFields(agent.defaultProviderId, agent.defaultModel, providerConfigs)
}

/** 当前聊天输入/本会话实际调用 pi 时使用的模型配置（含每会话覆盖） */
export function resolveEffectiveChatRuntime(
  activeHistoryItem: HistoryItem | null,
  fallbackAgent: ConversationAgentSnapshot | null,
  composerSessionLlm: { providerId: ProviderId; model: string } | null,
  selectedProviderId: ProviderId,
  providerConfigs: Record<string, ProviderConfig>,
  allProviderIds: string[],
): ProviderRuntimeConfig | null {
  if (activeHistoryItem?.sessionLlmProviderId && activeHistoryItem.sessionLlmModel?.trim()) {
    const resolved = resolveRuntimeFromSessionFields(
      activeHistoryItem.sessionLlmProviderId,
      activeHistoryItem.sessionLlmModel,
      providerConfigs,
    )
    if (resolved) {
      return resolved
    }
  }

  if (activeHistoryItem) {
    const resolvedFromAgent = resolveRuntimeFromAgentSnapshot(activeHistoryItem.agent ?? null, providerConfigs)
    if (resolvedFromAgent) {
      return resolvedFromAgent
    }
    return resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
  }

  if (composerSessionLlm) {
    const resolved = resolveRuntimeFromSessionFields(
      composerSessionLlm.providerId,
      composerSessionLlm.model,
      providerConfigs,
    )
    if (resolved) {
      return resolved
    }
  }

  if (!activeHistoryItem && fallbackAgent) {
    const resolved = resolveRuntimeFromAgentSnapshot(fallbackAgent, providerConfigs)
    if (resolved) {
      return resolved
    }
  }

  return resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
}
