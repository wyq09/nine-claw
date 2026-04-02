import { lazy, Suspense, startTransition, useCallback, useDeferredValue, useEffect, useMemo, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { Check, Copy } from 'lucide-react'
import './App.css'
import {
  botDefinitions,
  createInitialBotConfigs,
  createInitialProviderConfigs,
  emptyProviderConfig,
  defaultAppearanceSettings,
  defaultGeneralSettings,
  providerDefinitions,
  resourceSeed,
} from './mockData'
import { usePiAgent } from './hooks/usePiAgent'
import type {
  AgentExecutionMode,
  AgentBuilderDraft,
  AgentInput,
  AgentRecord,
  AgentWorkspaceBundle,
  AgentWorkspaceFile,
  AppearanceSettings,
  BotChannelId,
  BotConfig,
  BotDefinition,
  ConversationAgentSnapshot,
  ConversationTurn,
  GeneralSettings,
  HistoryItem,
  HistoryStatus,
  ProviderConfig,
  ProviderDefinition,
  ProviderId,
  ProviderRuntimeConfig,
  CustomProviderMeta,
  ResourceItem,
  SettingsTab,
  InstalledSkillItem,
  SkillLibraryTab,
  SubmitShortcut,
  SystemSkillCatalog,
  TokenUsage,
  ToolCallEntry,
  ViewKey,
} from './types'
import type { KeyboardEvent, MouseEvent } from 'react'
import {
  archiveAgent,
  botLoginWechat,
  botStartWechat,
  botStopWechat,
  createAgent,
  getDefaultAgent,
  installSystemSkill,
  listInstalledSkills,
  listAgents,
  listSystemSkillCatalog,
  readAgentWorkspaceBundle,
  setDefaultAgent,
  subscribeQrCode,
  subscribeBotStatus,
  testLlmProviderConnection,
  updateAgent,
  writeAgentWorkspaceFile,
} from './lib/piClient'
import type { QrCodeEvent, BotStatusEvent } from './lib/piClient'

const ABSOLUTE_TIME_FORMATTER = new Intl.DateTimeFormat('zh-CN', {
  year: 'numeric',
  month: 'numeric',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
})

const STARTER_CHIPS = ['定时会话', '法律检索', '类案检索', '案件分析', '文书起草', '合同审查', '法律意见'] as const
const GENERAL_SETTINGS_STORAGE_KEY = 'nineclaw.general-settings.v1'
const APPEARANCE_SETTINGS_STORAGE_KEY = 'nineclaw.appearance-settings.v1'
const PROVIDER_CONFIGS_STORAGE_KEY = 'nineclaw.provider-configs.v1'
const CUSTOM_PROVIDERS_META_KEY = 'nineclaw.custom-providers-meta.v1'
const BOT_CONFIGS_STORAGE_KEY = 'nineclaw.bot-configs.v1'
const LEGACY_GENERAL_SETTINGS_STORAGE_KEYS = ['yqagent.general-settings.v1']
const LEGACY_APPEARANCE_SETTINGS_STORAGE_KEYS = ['yqagent.appearance-settings.v1']
const LEGACY_PROVIDER_CONFIGS_STORAGE_KEYS = ['yqagent.provider-configs.v1']
const LEGACY_CUSTOM_PROVIDERS_META_KEYS = ['yqagent.custom-providers-meta.v1']
const LEGACY_BOT_CONFIGS_STORAGE_KEYS = ['yqagent.bot-configs.v1']
const MarkdownRenderer = lazy(() => import('./components/MarkdownRenderer'))

function readStoredStorageValue(storageKey: string, legacyKeys: string[] = []): string | null {
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

function persistStoredStorageValue(storageKey: string, value: string, legacyKeys: string[] = []) {
  localStorage.setItem(storageKey, value)
  for (const key of legacyKeys) {
    if (key !== storageKey) {
      localStorage.removeItem(key)
    }
  }
}

function loadStoredState<T extends object>(storageKey: string, defaults: T, legacyKeys: string[] = []): T {
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

function loadStoredRecord<T extends Record<string, object>>(storageKey: string, defaults: T, legacyKeys: string[] = []): T {
  try {
    const raw = readStoredStorageValue(storageKey, legacyKeys)
    if (!raw) {
      return defaults
    }

    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      return defaults
    }

    const result = { ...defaults } as T

    for (const key of Object.keys(defaults) as Array<keyof T>) {
      const defaultValue = defaults[key]
      const storedValue = (parsed as Record<string, unknown>)[String(key)]

      result[key] =
        typeof storedValue === 'object' && storedValue !== null && !Array.isArray(storedValue)
          ? { ...defaultValue, ...storedValue }
          : defaultValue
    }

    return result
  } catch {
    return defaults
  }
}

function createInitialGeneralSettings() {
  return loadStoredState(GENERAL_SETTINGS_STORAGE_KEY, defaultGeneralSettings, LEGACY_GENERAL_SETTINGS_STORAGE_KEYS)
}

function createInitialAppearanceState() {
  return {
    ...loadStoredState(
      APPEARANCE_SETTINGS_STORAGE_KEY,
      defaultAppearanceSettings,
      LEGACY_APPEARANCE_SETTINGS_STORAGE_KEYS,
    ),
    compactSidebar: false,
  }
}

function loadProviderConfigs(): Record<string, ProviderConfig> {
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

function loadCustomProviderMeta(): CustomProviderMeta[] {
  try {
    const raw = readStoredStorageValue(CUSTOM_PROVIDERS_META_KEY, LEGACY_CUSTOM_PROVIDERS_META_KEYS)
    if (!raw) {
      return []
    }
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) {
      return []
    }
    return parsed.filter(
      (x): x is CustomProviderMeta =>
        typeof x === 'object' &&
        x !== null &&
        typeof (x as CustomProviderMeta).id === 'string' &&
        typeof (x as CustomProviderMeta).name === 'string',
    )
  } catch {
    return []
  }
}

function createInitialProviderState() {
  return loadProviderConfigs()
}

function createInitialBotState() {
  return loadStoredRecord(BOT_CONFIGS_STORAGE_KEY, createInitialBotConfigs(), LEGACY_BOT_CONFIGS_STORAGE_KEYS)
}

function summarizePrompt(prompt: string, maxLength = 26): string {
  const compact = prompt.replace(/\s+/g, ' ').trim()
  if (!compact) {
    return '新会话'
  }

  return compact.length > maxLength ? `${compact.slice(0, maxLength)}…` : compact
}

function formatAbsoluteTime(createdAt: number): string {
  return ABSOLUTE_TIME_FORMATTER.format(new Date(createdAt))
}

function formatOptionalAbsoluteTime(createdAt?: number | null): string {
  return typeof createdAt === 'number' && createdAt > 0 ? formatAbsoluteTime(createdAt) : '时间未知'
}

function createEmptySystemSkillCatalog(): SystemSkillCatalog {
  return {
    available: false,
    updatedAt: null,
    message: '系统技能库暂未开放，后续会由系统统一提供可安装技能。',
    skills: [],
  }
}

function getAgentColor(agent: { accentColor?: string; id: string; name: string }): string {
  if (agent.accentColor?.trim()) {
    return agent.accentColor.trim()
  }

  const palette = ['#7C5CFA', '#F59E0B', '#10B981', '#EC4899', '#6366F1', '#F97316', '#0EA5E9']
  const seed = `${agent.id}:${agent.name}`
  let hash = 0
  for (let index = 0; index < seed.length; index += 1) {
    hash = (hash * 31 + seed.charCodeAt(index)) >>> 0
  }
  return palette[hash % palette.length] ?? palette[0]
}

function buildConversationAgentSnapshot(agent: AgentRecord): ConversationAgentSnapshot {
  return {
    id: agent.id,
    name: agent.name,
    summary: agent.summary,
    description: agent.description,
    systemPrompt: agent.systemPrompt,
    skillIds: [...agent.skillIds],
    defaultProviderId: agent.defaultProviderId,
    defaultModel: agent.defaultModel,
    executionMode: agent.executionMode,
    ...(agent.collaborationConfig ? { collaborationConfig: agent.collaborationConfig } : {}),
    ...(agent.accentColor ? { accentColor: agent.accentColor } : {}),
  }
}

function createEmptyAgentDraft(
  providerId: ProviderId,
  model: string,
  accentColor?: string,
): AgentInput {
  return {
    name: '',
    summary: '',
    description: '',
    systemPrompt: '',
    skillIds: [],
    defaultProviderId: providerId,
    defaultModel: model,
    executionMode: 'single',
    accentColor,
  }
}

function createAgentDraftFromRecord(agent: AgentRecord): AgentInput {
  return {
    name: agent.name,
    summary: agent.summary,
    description: agent.description,
    systemPrompt: agent.systemPrompt,
    skillIds: [...agent.skillIds],
    defaultProviderId: agent.defaultProviderId,
    defaultModel: agent.defaultModel,
    executionMode: agent.executionMode,
    ...(agent.collaborationConfig ? { collaborationConfig: agent.collaborationConfig } : {}),
    ...(agent.accentColor ? { accentColor: agent.accentColor } : {}),
  }
}

function normalizeAgentDraft(input: AgentInput): AgentInput {
  return {
    ...input,
    name: input.name.trim(),
    summary: input.summary.trim(),
    description: input.description.trim(),
    systemPrompt: input.systemPrompt.trim(),
    defaultProviderId: input.defaultProviderId.trim(),
    defaultModel: input.defaultModel.trim(),
    skillIds: Array.from(new Set(input.skillIds.map((item) => item.trim()).filter(Boolean))),
  }
}

function validateAgentDraft(input: AgentInput): string | null {
  if (!input.name.trim()) {
    return '请输入智能体名称。'
  }
  if (!input.summary.trim()) {
    return '请输入智能体简介。'
  }
  if (!input.description.trim()) {
    return '请输入智能体介绍。'
  }
  if (!input.defaultProviderId.trim() || !input.defaultModel.trim()) {
    return '请为智能体配置默认模型。'
  }
  return null
}

function formatAgentExecutionModeLabel(mode: AgentExecutionMode): string {
  if (mode === 'supervisor') {
    return '协调者'
  }
  if (mode === 'worker') {
    return '执行者'
  }
  return '单智能体'
}

function formatInstalledSkillScopeLabel(scope: InstalledSkillItem['scope']): string {
  return scope === 'workspace' ? '工作区已安装' : '系统已安装'
}

function formatInstalledSkillSource(skill: InstalledSkillItem): string {
  if (skill.source && skill.sourceType) {
    return `${skill.sourceType} · ${skill.source}`
  }
  if (skill.source) {
    return skill.source
  }
  if (skill.sourceType) {
    return skill.sourceType
  }
  return skill.installType === 'symlink' ? '符号链接' : '本地目录'
}

function formatWorkspaceFileSectionLabel(section: AgentWorkspaceFile['section']): string {
  if (section === 'private') {
    return '私有文件'
  }
  if (section === 'dailyLog') {
    return '最近日志'
  }
  return '共享文件'
}

function pickDefaultWorkspaceFileKey(bundle: AgentWorkspaceBundle): string {
  const preferredNames = ['IDENTITY.md', 'ROLE.md', 'MEMORY.md', 'WORKING.md', 'AGENTS.md']
  for (const name of preferredNames) {
    const match = bundle.files.find((file) => file.name === name && file.exists)
    if (match) {
      return match.key
    }
  }
  return bundle.files[0]?.key ?? ''
}

function validateSkillInstallLink(value: string): string | null {
  const trimmed = value.trim()
  if (!trimmed) {
    return '请输入技能链接。'
  }

  try {
    const parsed = new URL(trimmed)
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
      return '目前只支持 http 或 https 链接。'
    }
    return null
  } catch {
    return '链接格式不正确，请输入完整的 http(s) 地址。'
  }
}

function buildSkillInstallPrompt(link: string): string {
  return [
    `请帮助我安装一个技能，技能链接是：${link}`,
    '',
    '请按下面流程执行：',
    '1. 先识别这个链接对应的技能来源、安装方式和风险点。',
    '2. 明确告诉我预计会安装到哪个技能目录，会新增或修改哪些文件。',
    '3. 如果安装前需要我确认、登录或补充信息，先在会话里说明再继续。',
    '4. 使用当前环境中合适的技能安装方式完成安装，不要跳过校验步骤。',
    '5. 安装完成后验证技能是否可被识别，并提醒我回到技能页刷新列表。',
  ].join('\n')
}

const AGENT_BUILDER_BLOCK_PATTERN = /```nineclaw-agent\s*([\s\S]*?)```/i

function parseAgentBuilderDraft(content: string): AgentBuilderDraft | null {
  const match = content.match(AGENT_BUILDER_BLOCK_PATTERN)
  if (!match?.[1]) {
    return null
  }

  try {
    const parsed = JSON.parse(match[1]) as Partial<AgentBuilderDraft>
    if (!parsed || typeof parsed !== 'object') {
      return null
    }

    const name = typeof parsed.name === 'string' ? parsed.name.trim() : ''
    const summary = typeof parsed.summary === 'string' ? parsed.summary.trim() : ''
    const description = typeof parsed.description === 'string' ? parsed.description.trim() : ''
    const systemPrompt = typeof parsed.systemPrompt === 'string' ? parsed.systemPrompt.trim() : ''
    const defaultProviderId =
      typeof parsed.defaultProviderId === 'string' ? parsed.defaultProviderId.trim() : ''
    const defaultModel = typeof parsed.defaultModel === 'string' ? parsed.defaultModel.trim() : ''
    const executionMode =
      parsed.executionMode === 'supervisor' || parsed.executionMode === 'worker'
        ? parsed.executionMode
        : 'single'
    const skillIds = Array.isArray(parsed.skillIds)
      ? Array.from(new Set(parsed.skillIds.filter((item): item is string => typeof item === 'string').map((item) => item.trim()).filter(Boolean)))
      : []

    if (!name || !summary || !description) {
      return null
    }

    return {
      name,
      summary,
      description,
      systemPrompt,
      skillIds,
      defaultProviderId,
      defaultModel,
      executionMode,
      collaborationConfig: parsed.collaborationConfig,
      accentColor: typeof parsed.accentColor === 'string' ? parsed.accentColor.trim() : undefined,
      workspaceNotes: typeof parsed.workspaceNotes === 'string' ? parsed.workspaceNotes.trim() : undefined,
    }
  } catch {
    return null
  }
}

function stripAgentBuilderBlock(content: string): string {
  return content.replace(AGENT_BUILDER_BLOCK_PATTERN, '').trim()
}

function formatHistoryAgeLabel(status: HistoryStatus, updatedAt: number): string {
  if (status === 'running') {
    return '思考中'
  }

  const diffMs = Math.max(0, Date.now() - updatedAt)
  const minute = 60 * 1000
  const hour = 60 * minute
  const day = 24 * hour
  const week = 7 * day

  if (diffMs < hour) {
    const minutes = Math.max(1, Math.floor(diffMs / minute))
    return `${minutes} 分`
  }

  if (diffMs < day) {
    const hours = Math.max(1, Math.floor(diffMs / hour))
    return `${hours} 小时`
  }

  if (diffMs < week) {
    const days = Math.max(1, Math.floor(diffMs / day))
    return `${days} 天`
  }

  const weeks = Math.max(1, Math.floor(diffMs / week))
  return `${weeks} 周`
}

function getStatusTone(status: HistoryStatus): string {
  if (status === 'running') return 'running'
  if (status === 'done') return 'done'
  return 'error'
}

function formatDurationLabel(durationMs: number): string {
  if (durationMs < 1000) {
    return `${durationMs} ms`
  }

  if (durationMs < 60_000) {
    const seconds = durationMs / 1000
    return `${seconds >= 10 ? seconds.toFixed(0) : seconds.toFixed(1)} 秒`
  }

  const minutes = Math.floor(durationMs / 60_000)
  const seconds = Math.floor((durationMs % 60_000) / 1000)
  return `${minutes} 分 ${seconds} 秒`
}

function getElapsedMs(startedAt: number, completedAt?: number, isStreaming = false): number | undefined {
  const endAt = completedAt ?? (isStreaming ? Date.now() : undefined)
  if (!endAt) {
    return undefined
  }

  return Math.max(0, endAt - startedAt)
}

function formatTokenCount(value: number | undefined): string {
  if (typeof value !== 'number') {
    return '--'
  }

  return value.toLocaleString('zh-CN')
}

function hasUsageMetrics(usage?: TokenUsage): boolean {
  return Boolean(
    usage &&
      (usage.totalTokens > 0 ||
        usage.inputTokens > 0 ||
        usage.outputTokens > 0 ||
        usage.cacheReadTokens > 0 ||
        usage.cacheWriteTokens > 0),
  )
}

function getProviderStatus(
  config: ProviderConfig,
  preserveVerifiedStatus = true,
): ProviderConfig['status'] {
  if (!config.baseUrl.trim() || !config.model.trim()) {
    return '未配置'
  }

  return preserveVerifiedStatus && config.status === '测试通过' ? '测试通过' : '已配置'
}

function getSubmitShortcutLabel(shortcut: SubmitShortcut): string {
  return shortcut === 'enter' ? 'Enter' : 'Ctrl / Cmd + Enter'
}

function isFnLikeKeyboardEvent(event: Pick<globalThis.KeyboardEvent, 'code' | 'key' | 'location' | 'getModifierState'>): boolean {
  if (event.key === 'Fn' || event.key === 'Function' || event.code === 'Fn') {
    return true
  }

  if (event.getModifierState?.('Fn')) {
    return true
  }

  return event.code === 'NumpadEnter' || event.location === globalThis.KeyboardEvent.DOM_KEY_LOCATION_NUMPAD
}

function providerDisplayName(definition: ProviderDefinition, config: ProviderConfig | undefined): string {
  const label = config?.displayName?.trim()
  if (label) {
    return label
  }
  return definition.name
}

function shouldSubmitWithShortcut(
  event: KeyboardEvent<HTMLTextAreaElement>,
  submitShortcut: SubmitShortcut,
): boolean {
  if (event.nativeEvent.isComposing || event.key !== 'Enter') {
    return false
  }

  // On macOS, Fn/Globe combinations can surface as keypad-style Enter events.
  // Keep submission bound to the standard Return key so those system behaviors stay untouched.
  if (isFnLikeKeyboardEvent(event.nativeEvent)) {
    return false
  }

  if (submitShortcut === 'enter') {
    return !event.shiftKey && !event.ctrlKey && !event.metaKey && !event.altKey
  }

  return !event.shiftKey && !event.altKey && (event.ctrlKey || event.metaKey)
}

function resolveActiveProviderConfig(
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
    if (!config?.enabled || !config.model.trim()) {
      continue
    }

    return {
      providerId,
      baseUrl: config.baseUrl.trim(),
      apiKey: config.apiKey.trim(),
      model: config.model.trim(),
    }
  }

  return null
}

function resolveBotRuntimeConfig(
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
    if (!isProviderConfigComplete(config)) {
      continue
    }

    return {
      providerId,
      baseUrl: config.baseUrl.trim(),
      apiKey: config.apiKey.trim(),
      model: config.model.trim(),
    }
  }

  return null
}

/** 已填写 Base URL、API Key、模型名（与会话选择器一致，不要求「启用」开关） */
function isProviderConfigComplete(cfg: ProviderConfig | undefined): boolean {
  return Boolean(cfg?.baseUrl.trim() && cfg.apiKey.trim() && cfg.model.trim())
}

function sessionLlmEncode(providerId: ProviderId, model: string): string {
  return encodeURIComponent(JSON.stringify({ p: providerId, m: model.trim() }))
}

function sessionLlmDecode(value: string): { providerId: ProviderId; model: string } | null {
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

function buildSessionLlmSelectOptions(
  mergedProviderDefinitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
): { value: string; label: string }[] {
  const out: { value: string; label: string }[] = []
  for (const def of mergedProviderDefinitions) {
    const cfg = providerConfigs[def.id]
    if (!isProviderConfigComplete(cfg)) {
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

function buildSessionLlmOptionsWithFallback(
  options: { value: string; label: string }[],
  mergedProviderDefinitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
  current: { providerId: ProviderId; model: string },
): { value: string; label: string }[] {
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

function resolveRuntimeFromSessionFields(
  providerId: ProviderId,
  model: string,
  providerConfigs: Record<string, ProviderConfig>,
): ProviderRuntimeConfig | null {
  const cfg = providerConfigs[providerId]
  if (!isProviderConfigComplete(cfg) || !model.trim()) {
    return null
  }
  return {
    providerId,
    baseUrl: cfg!.baseUrl.trim(),
    apiKey: cfg!.apiKey.trim(),
    model: model.trim(),
  }
}

/** 当前聊天输入/本会话实际调用 pi 时使用的模型配置（含每会话覆盖） */
function resolveEffectiveChatRuntime(
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
    const resolved = resolveRuntimeFromSessionFields(
      fallbackAgent.defaultProviderId,
      fallbackAgent.defaultModel,
      providerConfigs,
    )
    if (resolved) {
      return resolved
    }
  }

  return resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
}

function isAbsoluteLocalPath(value: string): boolean {
  return value.startsWith('/') || /^[A-Za-z]:[\\/]/.test(value)
}

function decodeLocalPathSource(source: string): string {
  try {
    return decodeURIComponent(source)
  } catch {
    return source
  }
}

function normalizeMarkdownImageSource(source: string): string {
  const trimmed = source.trim()
  if (!trimmed) {
    return source
  }

  if (/^(https?:|data:|asset:)/i.test(trimmed)) {
    return trimmed
  }

  if (/^file:\/\//i.test(trimmed)) {
    const filePath = decodeLocalPathSource(trimmed.replace(/^file:\/\//i, ''))
    return convertFileSrc(filePath)
  }

  if (isAbsoluteLocalPath(trimmed)) {
    return convertFileSrc(decodeLocalPathSource(trimmed))
  }

  return trimmed
}

function normalizeMarkdownImageSources(content: string): string {
  return content
    .replace(/!\[([^\]]*)\]\(([^)\s]+)(\s+"[^"]*")?\)/g, (_match, alt: string, src: string, title = '') => {
      return `![${alt}](${normalizeMarkdownImageSource(src)}${title})`
    })
    .replace(/<img([^>]*?)src=(['"])(.*?)\2([^>]*)>/gi, (_match, before: string, quote: string, src: string, after: string) => {
      return `<img${before}src=${quote}${normalizeMarkdownImageSource(src)}${quote}${after}>`
    })
}

function App() {
  const {
    draft,
    setDraft,
    error,
    loading,
    runningHistoryIds,
    history,
    activeHistoryId,
    activeHistoryItem,
    submitPrompt,
    submitPromptInNewSession,
    abortPrompt,
    resetSessionDraft,
    selectHistoryItem,
    clearHistory,
    deleteHistoryItem,
    updateSessionLlm,
  } = usePiAgent()

  const [view, setView] = useState<ViewKey>('chat')
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [settingsTab, setSettingsTab] = useState<SettingsTab>('general')
  const [sidebarOverlayOpen, setSidebarOverlayOpen] = useState(false)
  const [viewportWidth, setViewportWidth] = useState(() => window.innerWidth)
  const [skillLibraryTab, setSkillLibraryTab] = useState<SkillLibraryTab>('installed')
  const [skillSearch, setSkillSearch] = useState('')
  const [installedSkills, setInstalledSkills] = useState<InstalledSkillItem[]>([])
  const [systemSkillCatalog, setSystemSkillCatalog] = useState<SystemSkillCatalog>(createEmptySystemSkillCatalog)
  const [skillsLoading, setSkillsLoading] = useState(false)
  const [skillsError, setSkillsError] = useState('')
  const [skillInstallDialogOpen, setSkillInstallDialogOpen] = useState(false)
  const [skillInstallLink, setSkillInstallLink] = useState('')
  const [skillInstallError, setSkillInstallError] = useState('')
  const [skillInstallLaunching, setSkillInstallLaunching] = useState(false)
  const [systemSkillInstallId, setSystemSkillInstallId] = useState('')
  const [resourceSearch, setResourceSearch] = useState('')
  const [agentSearch, setAgentSearch] = useState('')
  const [historySearch, setHistorySearch] = useState('')
  const [agents, setAgents] = useState<AgentRecord[]>([])
  const [agentsLoading, setAgentsLoading] = useState(false)
  const [agentsError, setAgentsError] = useState('')
  const [defaultAgentId, setDefaultAgentId] = useState('')
  const [managedAgentId, setManagedAgentId] = useState('')
  const [agentEditorMode, setAgentEditorMode] = useState<'create' | 'edit'>('edit')
  const [agentEditorDraft, setAgentEditorDraft] = useState<AgentInput | null>(null)
  const [agentEditorOpen, setAgentEditorOpen] = useState(false)
  const [agentSaving, setAgentSaving] = useState(false)
  const [agentFormError, setAgentFormError] = useState('')
  const [agentFormNotice, setAgentFormNotice] = useState('')
  const [agentBuilderActionBusyId, setAgentBuilderActionBusyId] = useState('')
  const [agentBuilderActionTargetId, setAgentBuilderActionTargetId] = useState('')
  const [agentBuilderActionNotice, setAgentBuilderActionNotice] = useState('')
  const [agentBuilderActionError, setAgentBuilderActionError] = useState('')
  const [agentSkillPickerOpen, setAgentSkillPickerOpen] = useState(false)
  const [agentSkillSearch, setAgentSkillSearch] = useState('')
  const [agentWorkspaceDialogOpen, setAgentWorkspaceDialogOpen] = useState(false)
  const [agentWorkspaceDialogLoading, setAgentWorkspaceDialogLoading] = useState(false)
  const [agentWorkspaceDialogError, setAgentWorkspaceDialogError] = useState('')
  const [agentWorkspaceBundle, setAgentWorkspaceBundle] = useState<AgentWorkspaceBundle | null>(null)
  const [agentWorkspaceSelectedKey, setAgentWorkspaceSelectedKey] = useState('')
  const [agentWorkspaceDraftContent, setAgentWorkspaceDraftContent] = useState('')
  const [agentWorkspaceSaving, setAgentWorkspaceSaving] = useState(false)
  const [agentWorkspaceSaveError, setAgentWorkspaceSaveError] = useState('')
  const [agentWorkspaceSaveNotice, setAgentWorkspaceSaveNotice] = useState('')
  const [newSessionDialogOpen, setNewSessionDialogOpen] = useState(false)
  const [newSessionAgentId, setNewSessionAgentId] = useState('')
  const [newSessionLlm, setNewSessionLlm] = useState<{ providerId: ProviderId; model: string } | null>(null)
  const [historyContextMenu, setHistoryContextMenu] = useState<{
    sessionId: string
    title: string
    x: number
    y: number
    canDelete: boolean
  } | null>(null)
  const [historyDeleteTarget, setHistoryDeleteTarget] = useState<{ sessionId: string; title: string } | null>(null)
  const [historyDeleteBusy, setHistoryDeleteBusy] = useState(false)
  const [generalSettings, setGeneralSettings] = useState<GeneralSettings>(createInitialGeneralSettings)
  const [appearanceSettings, setAppearanceSettings] = useState<AppearanceSettings>(createInitialAppearanceState)
  const [selectedProviderId, setSelectedProviderId] = useState<ProviderId>('openai')
  const [selectedBotId, setSelectedBotId] = useState<BotChannelId>('dingtalk')
  const [providerConfigs, setProviderConfigs] = useState<Record<string, ProviderConfig>>(createInitialProviderState)
  const [customProviderMeta, setCustomProviderMeta] = useState<CustomProviderMeta[]>(() => loadCustomProviderMeta())
  const [botConfigs, setBotConfigs] = useState<Record<string, BotConfig>>(createInitialBotState)
  const [qrDialogOpen, setQrDialogOpen] = useState(false)
  const [qrCodeUrl, setQrCodeUrl] = useState<string>('')
  const [qrStatus, setQrStatus] = useState<'waiting' | 'scanned' | 'confirmed' | 'error'>('waiting')
  const [botLoading, setBotLoading] = useState(false)
  const [botStatusLog, setBotStatusLog] = useState<BotStatusEvent[]>([])
  /** 无选中会话时，输入区上方选择的模型（首条消息写入该会话） */
  const [composerSessionLlm, setComposerSessionLlm] = useState<{ providerId: ProviderId; model: string } | null>(null)
  const [composerAgent, setComposerAgent] = useState<ConversationAgentSnapshot | null>(null)
  const [chatGateError, setChatGateError] = useState('')

  const deferredSkillSearch = useDeferredValue(skillSearch.trim().toLowerCase())
  const deferredResourceSearch = useDeferredValue(resourceSearch.trim().toLowerCase())
  const deferredAgentSearch = useDeferredValue(agentSearch.trim().toLowerCase())
  const deferredHistorySearch = useDeferredValue(historySearch.trim().toLowerCase())

  const mergedProviderDefinitions = useMemo((): ProviderDefinition[] => {
    const custom = customProviderMeta.map((meta) => ({
      id: meta.id,
      name: meta.name,
      defaultBaseUrl: 'https://api.openai.com/v1',
      suggestedModel: 'gpt-4o-mini',
      description: meta.description || '自定义 OpenAI 兼容接口。',
      isCustom: true,
    }))
    return [...providerDefinitions, ...custom]
  }, [customProviderMeta])
  const allProviderIds = useMemo(() => mergedProviderDefinitions.map((p) => p.id), [mergedProviderDefinitions])
  const selectedProviderDefinition =
    mergedProviderDefinitions.find((item) => item.id === selectedProviderId) ?? providerDefinitions[0]
  const selectedProviderConfig = providerConfigs[selectedProviderId] ?? emptyProviderConfig()
  const activeProviderConfig = resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
  const activeProviderDefinition = activeProviderConfig
    ? mergedProviderDefinitions.find((item) => item.id === activeProviderConfig.providerId) ?? null
    : null
  const selectedBotDefinition = botDefinitions.find((item) => item.id === selectedBotId) ?? botDefinitions[0]
  const selectedBotConfig = botConfigs[selectedBotId]
  const defaultAgent = useMemo(
    () => agents.find((item) => item.id === defaultAgentId) ?? agents[0] ?? null,
    [agents, defaultAgentId],
  )
  const editableAgents = useMemo(() => agents.filter((item) => !item.isBuiltin), [agents])
  const selectedManagedAgent = useMemo(
    () => editableAgents.find((item) => item.id === managedAgentId) ?? null,
    [editableAgents, managedAgentId],
  )
  const preferredComposerAgent = useMemo(
    () => composerAgent ?? (defaultAgent ? buildConversationAgentSnapshot(defaultAgent) : null),
    [composerAgent, defaultAgent],
  )
  const activeChatAgent = activeHistoryItem ? activeHistoryItem.agent ?? null : preferredComposerAgent
  const visibleInstalledSkills = installedSkills.filter((skill) => {
    if (!deferredSkillSearch) return true
    return `${skill.name} ${skill.description} ${skill.path}`.toLowerCase().includes(deferredSkillSearch)
  })
  const visibleSystemSkills = systemSkillCatalog.skills.filter((skill) => {
    if (!deferredSkillSearch) return true
    return `${skill.name} ${skill.description}`.toLowerCase().includes(deferredSkillSearch)
  })
  const visibleResources = resourceSeed.filter((resource) => {
    if (!deferredResourceSearch) return true
    return `${resource.title} ${resource.description} ${resource.tag}`.toLowerCase().includes(deferredResourceSearch)
  })
  const visibleAgents = editableAgents.filter((agent) => {
    if (!deferredAgentSearch) return true
    return `${agent.name} ${agent.summary} ${agent.description}`.toLowerCase().includes(deferredAgentSearch)
  })
  const visibleAgentSkillOptions = useMemo(() => {
    const searchNeedle = agentSkillSearch.trim().toLowerCase()
    const activeSkillIds = new Set(agentEditorDraft?.skillIds ?? [])

    return [...installedSkills]
      .filter((skill) => {
        if (!searchNeedle) {
          return true
        }

        return `${skill.name} ${skill.description} ${skill.path} ${skill.source ?? ''}`
          .toLowerCase()
          .includes(searchNeedle)
      })
      .sort((left, right) => {
        const leftSelected = activeSkillIds.has(left.id) ? 1 : 0
        const rightSelected = activeSkillIds.has(right.id) ? 1 : 0
        if (leftSelected !== rightSelected) {
          return rightSelected - leftSelected
        }
        return left.name.localeCompare(right.name, 'zh-CN')
      })
  }, [agentEditorDraft?.skillIds, agentSkillSearch, installedSkills])
  const visibleHistory = history.filter((item) => {
    if (!deferredHistorySearch) return true

    const searchableText = [
      item.title,
      ...item.turns.flatMap((turn) => [turn.prompt, turn.answer]),
    ]
      .join(' ')
      .toLowerCase()

    return searchableText.includes(deferredHistorySearch)
  })
  const workspaceTitle = activeHistoryItem?.title ?? '新会话'
  const shouldHideSidebar = viewportWidth < 1180
  const effectiveSidebarCollapsed = false
  const isSidebarVisible = !shouldHideSidebar || sidebarOverlayOpen
  const effectiveChatRuntime = useMemo(
    () =>
      resolveEffectiveChatRuntime(
        activeHistoryItem,
        preferredComposerAgent,
        composerSessionLlm,
        selectedProviderId,
        providerConfigs,
        allProviderIds,
      ),
    [
      activeHistoryItem,
      preferredComposerAgent,
      composerSessionLlm,
      selectedProviderId,
      providerConfigs,
      allProviderIds,
    ],
  )

  const sessionLlmDisplay = useMemo(() => {
    const globalRuntime = resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
    const fallbackPid = globalRuntime?.providerId ?? selectedProviderId
    const fallbackModel =
      globalRuntime?.model ?? providerConfigs[selectedProviderId]?.model?.trim() ?? ''

    if (activeHistoryItem?.sessionLlmProviderId && activeHistoryItem.sessionLlmModel !== undefined) {
      return {
        providerId: activeHistoryItem.sessionLlmProviderId,
        model: activeHistoryItem.sessionLlmModel,
      }
    }
    if (activeHistoryItem) {
      const global = resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
      if (global) {
        return { providerId: global.providerId, model: global.model }
      }
      const firstComplete = mergedProviderDefinitions.find((d) => isProviderConfigComplete(providerConfigs[d.id]))
      if (firstComplete) {
        const cfg = providerConfigs[firstComplete.id]
        return { providerId: firstComplete.id, model: cfg!.model.trim() }
      }
      return { providerId: fallbackPid, model: fallbackModel }
    }
    if (composerSessionLlm) {
      return composerSessionLlm
    }
    if (preferredComposerAgent?.defaultProviderId && preferredComposerAgent.defaultModel.trim()) {
      return {
        providerId: preferredComposerAgent.defaultProviderId,
        model: preferredComposerAgent.defaultModel,
      }
    }
    const globalForComposer = resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
    if (globalForComposer) {
      return { providerId: globalForComposer.providerId, model: globalForComposer.model }
    }
    const firstCompleteComposer = mergedProviderDefinitions.find((d) =>
      isProviderConfigComplete(providerConfigs[d.id]),
    )
    if (firstCompleteComposer) {
      const cfg = providerConfigs[firstCompleteComposer.id]
      return { providerId: firstCompleteComposer.id, model: cfg!.model.trim() }
    }
    return { providerId: fallbackPid, model: fallbackModel }
  }, [
    activeHistoryItem,
    preferredComposerAgent,
    composerSessionLlm,
    selectedProviderId,
    providerConfigs,
    allProviderIds,
    mergedProviderDefinitions,
  ])

  const sessionLlmSelectOptions = useMemo(
    () => buildSessionLlmSelectOptions(mergedProviderDefinitions, providerConfigs),
    [mergedProviderDefinitions, providerConfigs],
  )

  const sessionLlmEncodedCurrent = useMemo(
    () => sessionLlmEncode(sessionLlmDisplay.providerId, sessionLlmDisplay.model.trim()),
    [sessionLlmDisplay],
  )

  const sessionLlmSelectOptionsWithFallback = useMemo(() => {
    return buildSessionLlmOptionsWithFallback(
      sessionLlmSelectOptions,
      mergedProviderDefinitions,
      providerConfigs,
      sessionLlmDisplay,
    )
  }, [
    sessionLlmSelectOptions,
    sessionLlmDisplay,
    mergedProviderDefinitions,
    providerConfigs,
  ])

  const chatProviderDefinition = effectiveChatRuntime
    ? mergedProviderDefinitions.find((item) => item.id === effectiveChatRuntime.providerId) ?? null
    : null
  const requiresConfiguredSessionModel = activeHistoryItem
    ? Boolean(activeHistoryItem.sessionLlmProviderId && activeHistoryItem.sessionLlmModel?.trim())
    : Boolean(composerSessionLlm || (preferredComposerAgent?.defaultProviderId && preferredComposerAgent.defaultModel.trim()))
  const runtimeResolutionError =
    !effectiveChatRuntime && requiresConfiguredSessionModel
      ? '当前会话绑定的模型尚未在设置里完成配置，请先补全对应供应商的 Base URL、API Key 和模型，或切换到已配置模型。'
      : ''
  const chatProviderLabel =
    chatProviderDefinition && effectiveChatRuntime
      ? `${providerDisplayName(
          chatProviderDefinition,
          providerConfigs[effectiveChatRuntime.providerId],
        )} · ${effectiveChatRuntime.model}`
      : 'pi 主流程'

  const activeProviderBadge =
    activeProviderDefinition && activeProviderConfig
      ? `当前使用 · ${providerDisplayName(
          activeProviderDefinition,
          providerConfigs[activeProviderConfig.providerId],
        )}`
      : '当前使用 · pi 默认'

  useEffect(() => {
    const onResize = () => {
      const nextWidth = window.innerWidth
      setViewportWidth(nextWidth)
      if (nextWidth >= 1180) {
        setSidebarOverlayOpen(false)
      }
    }

    window.addEventListener('resize', onResize, { passive: true })
    return () => window.removeEventListener('resize', onResize)
  }, [])

  useEffect(() => {
    persistStoredStorageValue(
      GENERAL_SETTINGS_STORAGE_KEY,
      JSON.stringify(generalSettings),
      LEGACY_GENERAL_SETTINGS_STORAGE_KEYS,
    )
  }, [generalSettings])

  useEffect(() => {
    persistStoredStorageValue(
      APPEARANCE_SETTINGS_STORAGE_KEY,
      JSON.stringify(appearanceSettings),
      LEGACY_APPEARANCE_SETTINGS_STORAGE_KEYS,
    )
  }, [appearanceSettings])

  useEffect(() => {
    persistStoredStorageValue(
      PROVIDER_CONFIGS_STORAGE_KEY,
      JSON.stringify(providerConfigs),
      LEGACY_PROVIDER_CONFIGS_STORAGE_KEYS,
    )
  }, [providerConfigs])

  useEffect(() => {
    persistStoredStorageValue(
      CUSTOM_PROVIDERS_META_KEY,
      JSON.stringify(customProviderMeta),
      LEGACY_CUSTOM_PROVIDERS_META_KEYS,
    )
  }, [customProviderMeta])

  useEffect(() => {
    persistStoredStorageValue(
      BOT_CONFIGS_STORAGE_KEY,
      JSON.stringify(botConfigs),
      LEGACY_BOT_CONFIGS_STORAGE_KEYS,
    )
  }, [botConfigs])

  // ── Bot status log (diagnostics) ──
  useEffect(() => {
    let unsub: (() => void) | undefined
    void subscribeBotStatus((event) => {
      setBotStatusLog((prev) => [event, ...prev].slice(0, 20))
    }).then((unlisten) => {
      unsub = unlisten
    })
    return () => unsub?.()
  }, [])

  const refreshSkillLibrary = async () => {
    setSkillsLoading(true)
    setSkillsError('')

    try {
      const [nextInstalledSkills, nextSystemSkillCatalog] = await Promise.all([
        listInstalledSkills(),
        listSystemSkillCatalog(),
      ])
      setInstalledSkills(nextInstalledSkills)
      setSystemSkillCatalog(nextSystemSkillCatalog)
    } catch (loadError) {
      const message = loadError instanceof Error ? loadError.message : String(loadError)
      setSkillsError(message)
    } finally {
      setSkillsLoading(false)
    }
  }

  useEffect(() => {
    void refreshSkillLibrary()
  }, [])

  const handleInstallSystemSkill = async (skillId: string) => {
    const trimmedId = skillId.trim()
    if (!trimmedId) {
      return
    }

    setSystemSkillInstallId(trimmedId)
    setSkillsError('')
    try {
      await installSystemSkill(trimmedId)
      await refreshSkillLibrary()
    } catch (installError) {
      const message = installError instanceof Error ? installError.message : String(installError)
      setSkillsError(message)
    } finally {
      setSystemSkillInstallId('')
    }
  }

  const refreshAgents = useCallback(async (preferredAgentId?: string) => {
    setAgentsLoading(true)
    setAgentsError('')

    try {
      const [nextAgents, nextDefaultAgent] = await Promise.all([listAgents(), getDefaultAgent()])
      const nextEditableAgents = nextAgents.filter((item) => !item.isBuiltin)
      setAgents(nextAgents)
      setDefaultAgentId(nextDefaultAgent?.id ?? '')

      let nextManagedId = preferredAgentId ?? ''
      setManagedAgentId((current) => {
        nextManagedId =
          (preferredAgentId &&
            nextEditableAgents.some((item) => item.id === preferredAgentId) &&
            preferredAgentId) ||
          (current && nextEditableAgents.some((item) => item.id === current) && current) ||
          (nextDefaultAgent &&
            !nextDefaultAgent.isBuiltin &&
            nextEditableAgents.some((item) => item.id === nextDefaultAgent.id) &&
            nextDefaultAgent.id) ||
          nextEditableAgents[0]?.id ||
          ''
        return nextManagedId
      })

      if (nextManagedId) {
        const targetAgent = nextEditableAgents.find((item) => item.id === nextManagedId) ?? null
        if (targetAgent) {
          setAgentEditorMode('edit')
          setAgentEditorDraft(createAgentDraftFromRecord(targetAgent))
        }
      } else {
        setAgentEditorDraft(null)
      }
    } catch (loadError) {
      const message = loadError instanceof Error ? loadError.message : String(loadError)
      setAgentsError(message)
    } finally {
      setAgentsLoading(false)
    }
  }, [])

  useEffect(() => {
    void refreshAgents()
  }, [refreshAgents])

  const handleCreateAgentFromDraft = async (draft: AgentBuilderDraft, actionId: string) => {
    const fallbackProviderId = draft.defaultProviderId.trim() || effectiveChatRuntime?.providerId?.trim() || ''
    const fallbackModel = draft.defaultModel.trim() || effectiveChatRuntime?.model?.trim() || ''
    const availableSkillIds = new Set(installedSkills.map((skill) => skill.id))
    const normalizedSkillIds = draft.skillIds.filter((skillId) => availableSkillIds.has(skillId))
    const omittedSkillIds = draft.skillIds.filter((skillId) => !availableSkillIds.has(skillId))

    const payload: AgentInput = normalizeAgentDraft({
      name: draft.name,
      summary: draft.summary,
      description: draft.description,
      systemPrompt: draft.systemPrompt,
      skillIds: normalizedSkillIds,
      defaultProviderId: fallbackProviderId,
      defaultModel: fallbackModel,
      executionMode: draft.executionMode,
      ...(draft.collaborationConfig ? { collaborationConfig: draft.collaborationConfig } : {}),
      ...(draft.accentColor ? { accentColor: draft.accentColor } : {}),
    })

    const validationError = validateAgentDraft(payload)
    if (validationError) {
      setAgentBuilderActionTargetId(actionId)
      setAgentBuilderActionError(validationError)
      setAgentBuilderActionNotice('')
      return
    }

    setAgentBuilderActionBusyId(actionId)
    setAgentBuilderActionTargetId(actionId)
    setAgentBuilderActionError('')
    setAgentBuilderActionNotice('')

    try {
      const created = await createAgent(payload)
      await refreshAgents(created.id)
      setView('agents')
      setManagedAgentId(created.id)
      setAgentEditorMode('edit')
      setAgentEditorDraft(createAgentDraftFromRecord(created))
      setAgentBuilderActionNotice(
        omittedSkillIds.length > 0
          ? `已创建智能体，未挂载未安装技能：${omittedSkillIds.join('、')}`
          : '已创建智能体，并同步到智能体管理列表。',
      )
    } catch (createError) {
      const message = createError instanceof Error ? createError.message : String(createError)
      setAgentBuilderActionError(message)
    } finally {
      setAgentBuilderActionBusyId('')
    }
  }

  const handleViewChange = (nextView: ViewKey) => {
    startTransition(() => {
      setView(nextView)
      setSidebarOverlayOpen(false)
      setSkillInstallDialogOpen(false)
      setAgentSkillPickerOpen(false)
      setAgentSkillSearch('')
      setHistoryContextMenu(null)
      if (nextView !== 'agents') {
        setAgentEditorOpen(false)
      }
      if (nextView !== 'chat') {
        setNewSessionDialogOpen(false)
      }
    })
  }

  const openSettings = (tab: SettingsTab) => {
    startTransition(() => {
      setSettingsOpen(true)
      setSettingsTab(tab)
    })
  }

  const openNewSessionDialog = () => {
    const seedAgent =
      (activeChatAgent && agents.find((item) => item.id === activeChatAgent.id)) ||
      defaultAgent ||
      agents[0] ||
      null

    if (seedAgent) {
      setNewSessionAgentId(seedAgent.id)
      setNewSessionLlm({
        providerId: seedAgent.defaultProviderId,
        model: seedAgent.defaultModel,
      })
    } else {
      setNewSessionAgentId('')
      setNewSessionLlm(null)
    }

    setNewSessionDialogOpen(true)
    setSidebarOverlayOpen(false)
  }

  const handleNewSession = () => {
    openNewSessionDialog()
  }

  const handleSessionLlmChange = (nextProviderId: ProviderId, nextModel: string) => {
    if (chatGateError) {
      setChatGateError('')
    }
    if (activeHistoryId) {
      updateSessionLlm(activeHistoryId, nextProviderId, nextModel)
    } else {
      setComposerSessionLlm({ providerId: nextProviderId, model: nextModel })
    }
  }

  const handleSessionLlmSelectChange = (value: string) => {
    const parsed = sessionLlmDecode(value)
    if (!parsed) {
      return
    }
    handleSessionLlmChange(parsed.providerId, parsed.model)
  }

  const handleSubmit = async () => {
    if (runtimeResolutionError) {
      setChatGateError(runtimeResolutionError)
      return
    }
    if (chatGateError) {
      setChatGateError('')
    }
    await submitPrompt(draft, {
      providerConfig: effectiveChatRuntime,
      agent: activeHistoryItem ? activeHistoryItem.agent ?? null : preferredComposerAgent,
      sessionLlm: sessionLlmDisplay,
    })
  }

  const handleSkillInstallConversation = async () => {
    const trimmedLink = skillInstallLink.trim()
    const validationError = validateSkillInstallLink(trimmedLink)
    if (validationError) {
      setSkillInstallError(validationError)
      return
    }

    setSkillInstallLaunching(true)
    setSkillInstallError('')
    setSkillInstallDialogOpen(false)
    handleViewChange('chat')

    try {
      await submitPromptInNewSession(buildSkillInstallPrompt(trimmedLink), {
        providerConfig: effectiveChatRuntime,
        sessionLlm: sessionLlmDisplay,
      })
      setSkillInstallLink('')
    } finally {
      setSkillInstallLaunching(false)
    }
  }

  const handleHistorySelect = (id: string) => {
    if (chatGateError) {
      setChatGateError('')
    }
    setHistoryContextMenu(null)
    handleViewChange('chat')
    selectHistoryItem(id)
  }

  const handleHistoryContextMenu = (event: MouseEvent<HTMLButtonElement>, item: HistoryItem) => {
    event.preventDefault()

    const menuWidth = 196
    const menuHeight = 56
    const maxX = Math.max(12, window.innerWidth - menuWidth - 12)
    const maxY = Math.max(12, window.innerHeight - menuHeight - 12)
    const canDelete = !runningHistoryIds.includes(item.id)

    setHistoryContextMenu({
      sessionId: item.id,
      title: item.title,
      x: Math.min(event.clientX, maxX),
      y: Math.min(event.clientY, maxY),
      canDelete,
    })
  }

  const handleRequestDeleteHistoryItem = (sessionId: string) => {
    const target = history.find((item) => item.id === sessionId)
    setHistoryContextMenu(null)
    if (!target || runningHistoryIds.includes(sessionId)) {
      return
    }

    setHistoryDeleteTarget({
      sessionId: target.id,
      title: target.title,
    })
  }

  const handleConfirmDeleteHistoryItem = async () => {
    if (!historyDeleteTarget || historyDeleteBusy) {
      return
    }

    setHistoryDeleteBusy(true)
    try {
      await deleteHistoryItem(historyDeleteTarget.sessionId)
      setHistoryDeleteTarget(null)
    } finally {
      setHistoryDeleteBusy(false)
    }
  }

  const handleManagedAgentSelect = (id: string) => {
    const targetAgent = editableAgents.find((item) => item.id === id)
    if (!targetAgent) {
      return
    }

    setAgentWorkspaceDialogOpen(false)
    setAgentWorkspaceDialogError('')
    setAgentWorkspaceBundle(null)
    setAgentWorkspaceSelectedKey('')
    setAgentWorkspaceDraftContent('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')
    setManagedAgentId(id)
    setAgentEditorMode('edit')
    setAgentEditorDraft(createAgentDraftFromRecord(targetAgent))
    setAgentEditorOpen(true)
    setAgentFormError('')
    setAgentFormNotice('')
    handleCloseAgentSkillPicker()
  }

  const handleCreateAgentDraft = () => {
    setAgentWorkspaceDialogOpen(false)
    setAgentWorkspaceDialogError('')
    setAgentWorkspaceBundle(null)
    setAgentWorkspaceSelectedKey('')
    setAgentWorkspaceDraftContent('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')
    setManagedAgentId('')
    setAgentEditorMode('create')
    setAgentFormError('')
    setAgentFormNotice('')
    setAgentSkillSearch('')
    setAgentSkillPickerOpen(false)
    setAgentEditorOpen(true)
    setAgentEditorDraft(
      createEmptyAgentDraft(
        defaultAgent?.defaultProviderId ?? sessionLlmDisplay.providerId,
        defaultAgent?.defaultModel || sessionLlmDisplay.model,
        defaultAgent?.accentColor,
      ),
    )
    handleViewChange('agents')
  }

  const handleAgentDraftChange = (updates: Partial<AgentInput>) => {
    setAgentEditorDraft((current) => (current ? { ...current, ...updates } : current))
    if (agentFormError) {
      setAgentFormError('')
    }
    if (agentFormNotice) {
      setAgentFormNotice('')
    }
  }

  const handleAgentSkillToggle = (skillId: string) => {
    setAgentEditorDraft((current) => {
      if (!current) {
        return current
      }

      const nextSkillIds = current.skillIds.includes(skillId)
        ? current.skillIds.filter((item) => item !== skillId)
        : [...current.skillIds, skillId]

      return { ...current, skillIds: nextSkillIds }
    })
  }

  const handleOpenAgentSkillPicker = () => {
    if (!agentEditorDraft) {
      return
    }
    setAgentSkillSearch('')
    setAgentSkillPickerOpen(true)
  }

  const handleCloseAgentSkillPicker = () => {
    setAgentSkillPickerOpen(false)
    setAgentSkillSearch('')
  }

  const handleCloseAgentEditor = () => {
    setAgentEditorOpen(false)
    setAgentWorkspaceDialogOpen(false)
    setAgentWorkspaceDraftContent('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')
    handleCloseAgentSkillPicker()
    setAgentFormError('')
    setAgentFormNotice('')
  }

  const loadAgentWorkspaceBundle = useCallback(async (agent: AgentRecord) => {
    setAgentWorkspaceDialogLoading(true)
    setAgentWorkspaceDialogError('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')

    try {
      const bundle = await readAgentWorkspaceBundle(agent.id)
      const nextKey =
        agentWorkspaceSelectedKey && bundle.files.some((file) => file.key === agentWorkspaceSelectedKey)
          ? agentWorkspaceSelectedKey
          : pickDefaultWorkspaceFileKey(bundle)
      const nextFile =
        bundle.files.find((file) => file.key === nextKey) ?? bundle.files.find((file) => file.exists) ?? bundle.files[0] ?? null
      setAgentWorkspaceBundle(bundle)
      setAgentWorkspaceSelectedKey(nextKey)
      setAgentWorkspaceDraftContent(nextFile?.content ?? '')
      setAgentWorkspaceDialogOpen(true)
    } catch (loadError) {
      const message = loadError instanceof Error ? loadError.message : String(loadError)
      setAgentWorkspaceDialogError(message)
      setAgentWorkspaceDraftContent('')
      setAgentWorkspaceDialogOpen(true)
    } finally {
      setAgentWorkspaceDialogLoading(false)
    }
  }, [agentWorkspaceSelectedKey])

  const handleOpenAgentWorkspace = async () => {
    if (!selectedManagedAgent) {
      return
    }
    await loadAgentWorkspaceBundle(selectedManagedAgent)
  }

  const handleRefreshAgentWorkspace = async () => {
    if (!selectedManagedAgent) {
      return
    }
    await loadAgentWorkspaceBundle(selectedManagedAgent)
  }

  const handleCloseAgentWorkspaceDialog = () => {
    setAgentWorkspaceDialogOpen(false)
    setAgentWorkspaceDialogError('')
    setAgentWorkspaceDraftContent('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')
  }

  const handleSelectAgentWorkspaceFile = (key: string) => {
    const nextFile = agentWorkspaceBundle?.files.find((file) => file.key === key) ?? null
    setAgentWorkspaceSelectedKey(key)
    setAgentWorkspaceDraftContent(nextFile?.content ?? '')
    if (agentWorkspaceSaveError) {
      setAgentWorkspaceSaveError('')
    }
    if (agentWorkspaceSaveNotice) {
      setAgentWorkspaceSaveNotice('')
    }
  }

  const handleSaveAgentWorkspaceFile = async (file: AgentWorkspaceFile, content: string) => {
    if (!selectedManagedAgent || agentWorkspaceSaving) {
      return
    }

    setAgentWorkspaceSaving(true)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')

    try {
      const bundle = await writeAgentWorkspaceFile({
        agentId: selectedManagedAgent.id,
        relativePath: file.relativePath,
        content,
      })
      const nextKey =
        agentWorkspaceSelectedKey && bundle.files.some((bundleFile) => bundleFile.key === agentWorkspaceSelectedKey)
          ? agentWorkspaceSelectedKey
          : pickDefaultWorkspaceFileKey(bundle)
      const nextFile =
        bundle.files.find((bundleFile) => bundleFile.key === nextKey) ??
        bundle.files.find((bundleFile) => bundleFile.exists) ??
        bundle.files[0] ??
        null
      setAgentWorkspaceBundle(bundle)
      setAgentWorkspaceSelectedKey(nextKey)
      setAgentWorkspaceDraftContent(nextFile?.content ?? content)
      setAgentWorkspaceSaveNotice(`${file.name} 已保存`)
    } catch (saveError) {
      const message = saveError instanceof Error ? saveError.message : String(saveError)
      setAgentWorkspaceSaveError(message)
    } finally {
      setAgentWorkspaceSaving(false)
    }
  }

  const handleSaveAgent = async () => {
    if (!agentEditorDraft || agentSaving) {
      return
    }

    const normalizedDraft = normalizeAgentDraft(agentEditorDraft)
    const validationError = validateAgentDraft(normalizedDraft)
    if (validationError) {
      setAgentFormError(validationError)
      return
    }

    setAgentSaving(true)
    setAgentFormError('')
    setAgentFormNotice('')

    try {
      const savedAgent =
        agentEditorMode === 'create' || !managedAgentId
          ? await createAgent(normalizedDraft)
          : await updateAgent(managedAgentId, normalizedDraft)

      setManagedAgentId(savedAgent.id)
      setAgentEditorMode('edit')
      setAgentEditorDraft(createAgentDraftFromRecord(savedAgent))
      setAgentFormNotice(agentEditorMode === 'create' ? '智能体已创建。' : '智能体已保存。')
      handleCloseAgentSkillPicker()
      await refreshAgents(savedAgent.id)
    } catch (saveError) {
      const message = saveError instanceof Error ? saveError.message : String(saveError)
      setAgentFormError(message)
    } finally {
      setAgentSaving(false)
    }
  }

  const handleArchiveCurrentAgent = async () => {
    if (!selectedManagedAgent || agentSaving) {
      return
    }

    const shouldArchive = window.confirm(`确认归档智能体「${selectedManagedAgent.name}」吗？`)
    if (!shouldArchive) {
      return
    }

    setAgentSaving(true)
    setAgentFormError('')
    setAgentFormNotice('')

    try {
      await archiveAgent(selectedManagedAgent.id)
      setAgentEditorMode('edit')
      setAgentEditorDraft(null)
      setAgentEditorOpen(false)
      handleCloseAgentSkillPicker()
      await refreshAgents()
    } catch (archiveError) {
      const message = archiveError instanceof Error ? archiveError.message : String(archiveError)
      setAgentFormError(message)
    } finally {
      setAgentSaving(false)
    }
  }

  const handleSetCurrentDefaultAgent = async () => {
    if (!selectedManagedAgent || agentSaving) {
      return
    }

    setAgentSaving(true)
    setAgentFormError('')
    setAgentFormNotice('')

    try {
      const nextDefaultAgent = await setDefaultAgent(selectedManagedAgent.id)
      setDefaultAgentId(nextDefaultAgent?.id ?? '')
      setAgentFormNotice(`已将「${selectedManagedAgent.name}」设为默认智能体。`)
    } catch (updateError) {
      const message = updateError instanceof Error ? updateError.message : String(updateError)
      setAgentFormError(message)
    } finally {
      setAgentSaving(false)
    }
  }

  const handleNewSessionAgentChange = (agentId: string) => {
    setNewSessionAgentId(agentId)
    const targetAgent = agents.find((item) => item.id === agentId)
    if (!targetAgent) {
      return
    }
    setNewSessionLlm({
      providerId: targetAgent.defaultProviderId,
      model: targetAgent.defaultModel,
    })
  }

  const handleConfirmNewSession = () => {
    const targetAgent = agents.find((item) => item.id === newSessionAgentId) ?? defaultAgent
    if (!targetAgent) {
      return
    }

    if (chatGateError) {
      setChatGateError('')
    }
    resetSessionDraft()
    setComposerAgent(buildConversationAgentSnapshot(targetAgent))
    setComposerSessionLlm(
      newSessionLlm ?? {
        providerId: targetAgent.defaultProviderId,
        model: targetAgent.defaultModel,
      },
    )
    setNewSessionDialogOpen(false)
    handleViewChange('chat')
  }

  const updateBotConfig = (channelId: BotChannelId, updates: Partial<BotConfig>) => {
    setBotConfigs((previous) => ({
      ...previous,
      [channelId]: {
        ...previous[channelId],
        ...updates,
      },
    }))
  }

  const updateProviderConfig = (providerId: ProviderId, updates: Partial<ProviderConfig>) => {
    setProviderConfigs((previous) => {
      const base = previous[providerId] ?? emptyProviderConfig()
      return {
        ...previous,
        [providerId]: {
          ...base,
          ...updates,
          status:
            updates.status ??
            getProviderStatus(
              {
                ...base,
                ...updates,
              },
              false,
            ),
        },
      }
    })
  }

  const addCustomProvider = (name: string, description: string) => {
    const id = `custom_${crypto.randomUUID().replace(/-/g, '')}`
    setCustomProviderMeta((previous) => [...previous, { id, name, description }])
    setProviderConfigs((previous) => ({
      ...previous,
      [id]: {
        ...emptyProviderConfig(),
        added: true,
        displayName: name.trim() || name,
        status: '未配置',
      },
    }))
    setSelectedProviderId(id)
  }

  const removeCustomProvider = (providerId: ProviderId) => {
    if (!providerId.startsWith('custom_')) {
      return
    }
    setCustomProviderMeta((previous) => previous.filter((item) => item.id !== providerId))
    setProviderConfigs((previous) => {
      const next = { ...previous }
      delete next[providerId]
      return next
    })
  }

  // ── Bot Channel Actions ──

  const handleWechatLogin = async () => {
    setBotLoading(true)
    setQrStatus('waiting')
    setQrCodeUrl('')
    setQrDialogOpen(true)

    try {
      // Subscribe to QR events before calling login
      const unsub = await subscribeQrCode((event: QrCodeEvent) => {
        if (event.qrcodeUrl) {
          setQrCodeUrl(event.qrcodeUrl)
        }
        if (event.status === 'scanned') {
          setQrStatus('scanned')
        }
        if (event.status === 'confirmed') {
          setQrStatus('confirmed')
          setQrDialogOpen(false)
        }
      })

      const result = await botLoginWechat()
      unsub()

      if (result.connected) {
        const loginToken = result.bot_token ?? ''
        const loginBaseUrl = result.base_url ?? 'https://ilinkai.weixin.qq.com'

        updateBotConfig('wechat', {
          status: '已连接',
          clientId: result.account_id ?? '',
          clientSecret: loginBaseUrl,
          token: loginToken,
          errorMessage: undefined,
        })
        setQrDialogOpen(false)

        // Auto-start the bot polling immediately after login
        try {
          const botRuntime = resolveBotRuntimeConfig(selectedProviderId, providerConfigs, allProviderIds)
          if (!botRuntime) {
            throw new Error('请先在 Provider 设置中补全 Base URL、API Key 和模型，再启动微信 Bot。')
          }
          await botStartWechat(loginToken, {
            baseUrl: loginBaseUrl || undefined,
            providerId: botRuntime.providerId,
            model: botRuntime.model,
            apiKey: botRuntime.apiKey,
            providerBaseUrl: botRuntime.baseUrl,
          })
          updateBotConfig('wechat', { enabled: true })
        } catch (startError) {
          updateBotConfig('wechat', {
            status: '错误',
            errorMessage: `登录成功但启动失败: ${String(startError)}`,
          })
        }
      } else {
        updateBotConfig('wechat', {
          status: '错误',
          errorMessage: result.message,
        })
        setQrDialogOpen(false)
      }
    } catch (error) {
      updateBotConfig('wechat', {
        status: '错误',
        errorMessage: String(error),
      })
      setQrDialogOpen(false)
    } finally {
      setBotLoading(false)
    }
  }

  const handleWechatStart = async () => {
    const config = botConfigs.wechat
    if (!config.token) {
      updateBotConfig('wechat', { status: '错误', errorMessage: '请先扫码登录获取 token' })
      return
    }
    setBotLoading(true)
    try {
      const botRuntime = resolveBotRuntimeConfig(selectedProviderId, providerConfigs, allProviderIds)
      if (!botRuntime) {
        throw new Error('请先在 Provider 设置中补全 Base URL、API Key 和模型，再启动微信 Bot。')
      }
      await botStartWechat(config.token, {
        baseUrl: config.clientSecret || undefined,
        routeTag: config.routeTag || undefined,
        providerId: botRuntime.providerId,
        model: botRuntime.model,
        apiKey: botRuntime.apiKey,
        providerBaseUrl: botRuntime.baseUrl,
      })
      updateBotConfig('wechat', { status: '已连接', enabled: true, errorMessage: undefined })
    } catch (error) {
      updateBotConfig('wechat', { status: '错误', errorMessage: String(error) })
    } finally {
      setBotLoading(false)
    }
  }

  const handleWechatStop = async () => {
    setBotLoading(true)
    try {
      await botStopWechat()
      updateBotConfig('wechat', { status: '未连接', enabled: false })
    } catch (error) {
      updateBotConfig('wechat', { status: '错误', errorMessage: String(error) })
    } finally {
      setBotLoading(false)
    }
  }

  const renderContent = () => {
    if (view === 'chat') {
      return (
        <ChatView
          key={activeHistoryItem?.id ?? 'empty-chat'}
          activeProviderLabel={chatProviderLabel}
          agentBuilderActionBusyId={agentBuilderActionBusyId}
          agentBuilderActionError={agentBuilderActionError}
          agentBuilderActionNotice={agentBuilderActionNotice}
          agentBuilderActionTargetId={agentBuilderActionTargetId}
          draft={draft}
          error={chatGateError || error}
          showExecutionRail={appearanceSettings.showExecutionRail}
          globalBusy={loading}
          runningHistoryIds={runningHistoryIds}
          activeHistoryId={activeHistoryId}
          onAbort={abortPrompt}
          onCreateAgentDraft={handleCreateAgentFromDraft}
          onSubmit={handleSubmit}
          selectedAgent={activeChatAgent}
          setDraft={setDraft}
          submitShortcut={generalSettings.submitShortcut}
          activeHistoryItem={activeHistoryItem}
          workspaceTitle={workspaceTitle}
          sessionLlmSelectOptions={sessionLlmSelectOptionsWithFallback}
          sessionLlmSelectValue={sessionLlmEncodedCurrent}
          onSessionLlmSelectChange={handleSessionLlmSelectChange}
        />
      )
    }

    if (view === 'skills') {
      return (
        <SkillsView
          installedSkillCount={installedSkills.length}
          installedSkills={visibleInstalledSkills}
          onChangeTab={setSkillLibraryTab}
          onInstallByLink={() => {
            setSkillInstallDialogOpen(true)
            setSkillInstallError('')
          }}
          onInstallSystemSkill={handleInstallSystemSkill}
          onRefresh={refreshSkillLibrary}
          sessionBusy={loading || skillInstallLaunching}
          systemSkillInstallId={systemSkillInstallId}
          setSearch={setSkillSearch}
          skillsError={skillsError}
          skillsLoading={skillsLoading}
          systemSkillCount={systemSkillCatalog.skills.length}
          systemSkillCatalog={systemSkillCatalog}
          tab={skillLibraryTab}
          skillSearch={skillSearch}
          visibleSystemSkills={visibleSystemSkills}
        />
      )
    }

    if (view === 'resources') {
      return (
        <ResourcesView
          onSearch={setResourceSearch}
          resourceSearch={resourceSearch}
          visibleResources={visibleResources}
        />
      )
    }

    return (
      <AgentsView
        agentDraft={agentEditorDraft}
        agentEditorOpen={agentEditorOpen}
        agentFormError={agentFormError}
        agentFormNotice={agentFormNotice}
        agentSaving={agentSaving}
        agentWorkspaceBundle={agentWorkspaceBundle}
        agentWorkspaceDialogError={agentWorkspaceDialogError}
        agentWorkspaceDialogLoading={agentWorkspaceDialogLoading}
        agentWorkspaceDialogOpen={agentWorkspaceDialogOpen}
        agentWorkspaceDraftContent={agentWorkspaceDraftContent}
        agentWorkspaceSaveError={agentWorkspaceSaveError}
        agentWorkspaceSaveNotice={agentWorkspaceSaveNotice}
        agentWorkspaceSaving={agentWorkspaceSaving}
        agentWorkspaceSelectedKey={agentWorkspaceSelectedKey}
        agents={visibleAgents}
        allSkills={installedSkills}
        defaultAgentId={defaultAgentId}
        onCreateAgent={handleCreateAgentDraft}
        onArchiveAgent={handleArchiveCurrentAgent}
        onCloseEditor={handleCloseAgentEditor}
        onCloseWorkspaceDialog={handleCloseAgentWorkspaceDialog}
        onDraftWorkspaceContentChange={setAgentWorkspaceDraftContent}
        onDraftChange={handleAgentDraftChange}
        onOpenWorkspace={handleOpenAgentWorkspace}
        onOpenSkillPicker={handleOpenAgentSkillPicker}
        onRefreshWorkspace={handleRefreshAgentWorkspace}
        onSaveAgent={handleSaveAgent}
        onSaveWorkspaceFile={handleSaveAgentWorkspaceFile}
        onSearch={setAgentSearch}
        onSelectAgent={handleManagedAgentSelect}
        onSelectWorkspaceFile={handleSelectAgentWorkspaceFile}
        onSetDefaultAgent={handleSetCurrentDefaultAgent}
        onToggleSkill={handleAgentSkillToggle}
        searchValue={agentSearch}
        selectedAgent={selectedManagedAgent}
        loading={agentsLoading}
        error={agentsError}
        mode={agentEditorMode}
        modelOptions={sessionLlmSelectOptionsWithFallback}
      />
    )
  }

  return (
    <>
      <main
        className={[
          'app-shell',
          appearanceSettings.compactSidebar ? 'compact-sidebar' : '',
          shouldHideSidebar ? 'sidebar-hidden-mode' : '',
          sidebarOverlayOpen ? 'sidebar-overlay-open' : '',
          appearanceSettings.preferReducedMotion ? 'reduce-motion' : '',
        ]
          .filter(Boolean)
          .join(' ')}
      >
        {shouldHideSidebar && sidebarOverlayOpen ? (
          <button
            type="button"
            className="sidebar-overlay-backdrop"
            aria-label="关闭侧栏"
            onClick={() => setSidebarOverlayOpen(false)}
          />
        ) : null}

        <aside className={`sidebar ${effectiveSidebarCollapsed ? 'collapsed' : ''} ${isSidebarVisible ? 'visible' : ''}`}>
          <div className="sidebar-section">
            <div className="sidebar-section-title">工作台</div>
            <div className="primary-nav">
              <SidebarButton
                active={view === 'chat'}
                icon="plus"
                label="新会话"
                onClick={handleNewSession}
              />
              <SidebarButton
                active={view === 'agents'}
                icon="bot"
                label="智能体管理"
                onClick={() => handleViewChange('agents')}
              />
              <SidebarButton
                active={view === 'skills'}
                icon="spark"
                label="探索技能"
                onClick={() => handleViewChange('skills')}
              />
              <SidebarButton
                active={view === 'resources'}
                icon="book"
                label="资源库"
                onClick={() => handleViewChange('resources')}
              />
            </div>
          </div>

          {history.length > 0 ? (
            <div className="task-history">
              <div className="task-header">
                <h2>历史会话</h2>
                <button type="button" className="link-button" onClick={clearHistory} disabled={loading}>
                  清空
                </button>
              </div>

              <label className="history-search-field">
                <AppIcon name="search" size={16} />
                <input
                  type="search"
                  value={historySearch}
                  onChange={(event) => setHistorySearch(event.target.value)}
                  placeholder="搜索历史会话"
                  aria-label="搜索历史会话"
                />
              </label>

              <div className="history-list">
                {visibleHistory.length > 0 ? (
                  visibleHistory.map((item) => (
                    <button
                      key={item.id}
                      type="button"
                      className={`history-card ${item.id === activeHistoryId ? 'active' : ''}`}
                      onClick={() => handleHistorySelect(item.id)}
                      onContextMenu={(event) => handleHistoryContextMenu(event, item)}
                      title={item.title}
                    >
                      <span className="history-card-title">{summarizePrompt(item.title, 24)}</span>
                      <span className={`history-card-time ${getStatusTone(item.status)}`}>
                        {formatHistoryAgeLabel(item.status, item.updatedAt)}
                      </span>
                    </button>
                  ))
                ) : (
                  <div className="empty-history-card compact">
                    <div className="empty-history-title">没有匹配的会话</div>
                    <span>试试搜索会话标题、提问内容或回答里的关键词。</span>
                  </div>
                )}
              </div>
            </div>
          ) : null}

          <div className="sidebar-footer">
            <button type="button" className="avatar-badge" aria-label="用户">
              U
            </button>
            <button type="button" className="footer-icon-button" onClick={() => handleViewChange('chat')} aria-label="最近会话">
              <AppIcon name="clock" size={18} />
            </button>
            <button type="button" className="footer-icon-button" onClick={() => handleViewChange('agents')} aria-label="AI 组织管理">
              <AppIcon name="network" size={18} />
            </button>
            <button type="button" className="footer-icon-button" onClick={() => openSettings('general')} aria-label="设置">
              <AppIcon name="settings" size={18} />
            </button>
          </div>
        </aside>

        <section className={`content-panel ${shouldHideSidebar ? 'has-sidebar-drawer-toggle' : ''}`}>
          {shouldHideSidebar ? (
            <button
              type="button"
              className="sidebar-drawer-toggle"
              aria-label={sidebarOverlayOpen ? '关闭导航' : '打开导航'}
              onClick={() => setSidebarOverlayOpen((current) => !current)}
            >
              <AppIcon name="panel" size={18} />
            </button>
          ) : null}
          {renderContent()}
        </section>
      </main>

      {skillInstallDialogOpen ? (
        <SkillInstallDialog
          error={skillInstallError}
          link={skillInstallLink}
          loading={skillInstallLaunching}
          onChangeLink={(value) => {
            setSkillInstallLink(value)
            if (skillInstallError) {
              setSkillInstallError('')
            }
          }}
          onClose={() => {
            if (!skillInstallLaunching) {
              setSkillInstallDialogOpen(false)
            }
          }}
          onConfirm={handleSkillInstallConversation}
        />
      ) : null}

      {historyContextMenu ? (
        <>
          <button
            type="button"
            className="context-menu-backdrop"
            aria-label="关闭会话菜单"
            onClick={() => setHistoryContextMenu(null)}
          />
          <div
            className="history-context-menu"
            role="menu"
            style={{ left: historyContextMenu.x, top: historyContextMenu.y }}
          >
            <button
              type="button"
              className="history-context-menu-item danger"
              role="menuitem"
              onClick={() => handleRequestDeleteHistoryItem(historyContextMenu.sessionId)}
              disabled={!historyContextMenu.canDelete}
              title={historyContextMenu.canDelete ? `删除「${historyContextMenu.title}」` : '当前会话仍在生成，暂时不能删除'}
            >
              <AppIcon name="trash" size={16} />
              <span>{historyContextMenu.canDelete ? '删除会话' : '会话生成中，暂不可删'}</span>
            </button>
          </div>
        </>
      ) : null}

      {historyDeleteTarget ? (
        <div
          className="confirm-dialog-overlay"
          role="presentation"
          onClick={() => {
            if (!historyDeleteBusy) {
              setHistoryDeleteTarget(null)
            }
          }}
        >
          <div
            className="confirm-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="history-delete-confirm-title"
            onClick={(event) => event.stopPropagation()}
          >
            <h3 id="history-delete-confirm-title">删除历史会话</h3>
            <p>确定要删除「{summarizePrompt(historyDeleteTarget.title, 32)}」吗？删除后无法恢复。</p>
            <div className="confirm-dialog-actions">
              <button
                type="button"
                className="outline-button"
                onClick={() => setHistoryDeleteTarget(null)}
                disabled={historyDeleteBusy}
              >
                取消
              </button>
              <button
                type="button"
                className="outline-button confirm-dialog-delete"
                onClick={handleConfirmDeleteHistoryItem}
                disabled={historyDeleteBusy}
              >
                {historyDeleteBusy ? '删除中…' : '删除'}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {newSessionDialogOpen ? (
        <NewSessionDialog
          agents={agents}
          loading={agentsLoading}
          modelOptions={sessionLlmSelectOptionsWithFallback}
          selectedAgentId={newSessionAgentId}
          selectedModelValue={
            newSessionLlm
              ? sessionLlmEncode(newSessionLlm.providerId, newSessionLlm.model)
              : sessionLlmEncodedCurrent
          }
          onChangeAgent={handleNewSessionAgentChange}
          onChangeModel={(value) => {
            const parsed = sessionLlmDecode(value)
            if (parsed) {
              setNewSessionLlm(parsed)
            }
          }}
          onClose={() => setNewSessionDialogOpen(false)}
          onConfirm={handleConfirmNewSession}
        />
      ) : null}

      {agentSkillPickerOpen && agentEditorOpen && agentEditorDraft ? (
        <AgentSkillPickerDialog
          allSkillCount={installedSkills.length}
          searchValue={agentSkillSearch}
          selectedSkillIds={agentEditorDraft.skillIds}
          skills={visibleAgentSkillOptions}
          onClose={handleCloseAgentSkillPicker}
          onSearch={setAgentSkillSearch}
          onToggleSkill={handleAgentSkillToggle}
        />
      ) : null}

      {settingsOpen ? (
        <SettingsModal
          activeProviderBadge={activeProviderBadge}
          allProviderDefinitions={mergedProviderDefinitions}
          appearanceSettings={appearanceSettings}
          botConfigs={botConfigs}
          botLoading={botLoading}
          generalSettings={generalSettings}
          onAddCustomProvider={addCustomProvider}
          onProviderConfigChange={updateProviderConfig}
          onBotConfigChange={updateBotConfig}
          onClose={() => setSettingsOpen(false)}
          onRemoveCustomProvider={removeCustomProvider}
          botStatusLog={botStatusLog}
          onWechatLogin={handleWechatLogin}
          onWechatStart={handleWechatStart}
          onWechatStop={handleWechatStop}
          onSelectProvider={setSelectedProviderId}
          onSelectBot={setSelectedBotId}
          onSelectTab={setSettingsTab}
          providerConfigs={providerConfigs}
          qrCodeUrl={qrCodeUrl}
          qrDialogOpen={qrDialogOpen}
          qrStatus={qrStatus}
          setBotLoading={setBotLoading}
          selectedProviderConfig={selectedProviderConfig}
          selectedProviderDefinition={selectedProviderDefinition}
          selectedProviderId={selectedProviderId}
          selectedBotConfig={selectedBotConfig}
          selectedBotDefinition={selectedBotDefinition}
          selectedBotId={selectedBotId}
          setAppearanceSettings={setAppearanceSettings}
          setGeneralSettings={setGeneralSettings}
          setQrDialogOpen={setQrDialogOpen}
          tab={settingsTab}
        />
      ) : null}
    </>
  )
}

type SidebarButtonProps = {
  active: boolean
  icon: IconName
  label: string
  onClick: () => void
}

function SidebarButton({ active, icon, label, onClick }: SidebarButtonProps) {
  return (
    <button type="button" className={`sidebar-button ${active ? 'active' : ''}`} onClick={onClick}>
      <AppIcon name={icon} size={20} />
      <span>{label}</span>
    </button>
  )
}

type ChatViewProps = {
  activeProviderLabel: string
  activeHistoryId: string
  activeHistoryItem: HistoryItem | null
  agentBuilderActionBusyId: string
  agentBuilderActionError: string
  agentBuilderActionNotice: string
  agentBuilderActionTargetId: string
  draft: string
  error: string
  globalBusy: boolean
  runningHistoryIds: string[]
  onAbort: () => void
  onCreateAgentDraft: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
  onSubmit: () => Promise<void>
  selectedAgent: ConversationAgentSnapshot | null
  setDraft: (value: string) => void
  showExecutionRail: boolean
  submitShortcut: SubmitShortcut
  workspaceTitle: string
  sessionLlmSelectOptions: { value: string; label: string }[]
  sessionLlmSelectValue: string
  onSessionLlmSelectChange: (value: string) => void
}

function ChatView({
  activeProviderLabel,
  activeHistoryId,
  activeHistoryItem,
  agentBuilderActionBusyId,
  agentBuilderActionError,
  agentBuilderActionNotice,
  agentBuilderActionTargetId,
  draft,
  error,
  globalBusy,
  runningHistoryIds,
  onAbort,
  onCreateAgentDraft,
  onSubmit,
  selectedAgent,
  setDraft,
  showExecutionRail,
  submitShortcut,
  workspaceTitle,
  sessionLlmSelectOptions,
  sessionLlmSelectValue,
  onSessionLlmSelectChange,
}: ChatViewProps) {
  const [copiedTurnId, setCopiedTurnId] = useState('')
  const [previewImage, setPreviewImage] = useState<{ src: string; alt: string } | null>(null)
  const turns = activeHistoryItem?.turns ?? []
  const activeTurnId = turns.at(-1)?.id ?? ''
  const sessionRunning = Boolean(activeHistoryItem && runningHistoryIds.includes(activeHistoryItem.id))
  const sessionStreaming = sessionRunning
  const workspaceStatusNote = activeHistoryItem
    ? sessionRunning
      ? 'pi 正在处理当前会话。'
      : activeHistoryItem.status === 'done'
        ? '当前回答已完成，可以继续发送。'
        : '当前会话已停止或失败。'
    : '输入内容后将由 pi 开始执行。'

  useEffect(() => {
    if (!activeTurnId) {
      return
    }

    document.getElementById(`chat-turn-${activeTurnId}`)?.scrollIntoView({
      block: 'end',
      behavior: 'smooth',
    })
  }, [activeHistoryId, activeTurnId, sessionRunning, turns.length])

  const handleCopyAnswer = async (turnId: string, answer: string) => {
    if (!answer) {
      return
    }

    await navigator.clipboard.writeText(answer)
    setCopiedTurnId(turnId)
    window.setTimeout(() => setCopiedTurnId((current) => (current === turnId ? '' : current)), 1600)
  }

  const handleComposerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (!shouldSubmitWithShortcut(event, submitShortcut) || sessionStreaming) {
      return
    }

    event.preventDefault()
    void onSubmit()
  }

  const handleMarkdownImageClick = (src: string, alt: string) => {
    setPreviewImage({ src, alt })
  }

  const composerPlaceholder = activeHistoryItem ? '继续对话…' : "描述您的法律需求，或输入 '/' 唤起技能…"

  return (
    <div className="workspace">
      <header className="workspace-topbar">
        <div className="workspace-topbar-left">
          <div className="session-llm-toolbar" title={activeProviderLabel}>
            <label className="visually-hidden" htmlFor="session-llm-combined">
              本会话使用的模型
            </label>
            <select
              id="session-llm-combined"
              className="session-llm-select"
              value={
                sessionLlmSelectOptions.some((o) => o.value === sessionLlmSelectValue)
                  ? sessionLlmSelectValue
                  : sessionLlmSelectOptions[0]?.value ?? ''
              }
              disabled={sessionStreaming || sessionLlmSelectOptions.length === 0}
              onChange={(event) => onSessionLlmSelectChange(event.target.value)}
              aria-label="本会话使用的供应商与模型"
            >
              {sessionLlmSelectOptions.length === 0 ? (
                <option value="">暂无已配置的模型，请先在设置中填写供应商</option>
              ) : (
                sessionLlmSelectOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))
              )}
            </select>
          </div>
          {activeHistoryItem ? <h1 title={workspaceTitle}>{workspaceTitle}</h1> : null}
        </div>
      </header>

      <section className="workspace-scroll">
        {activeHistoryItem ? (
          <>
            <div className="status-strip">
              <div className="status-strip-head">
                <span className="status-strip-note">{workspaceStatusNote}</span>
              </div>
            </div>

            <div className="chat-message-list">
              {turns.map((item) => (
                <article
                  key={item.id}
                  id={`chat-turn-${item.id}`}
                  className={`chat-turn ${item.id === activeTurnId ? 'active' : ''}`}
                >
                  <div className="prompt-block">
                    <div className="prompt-bubble">{item.prompt}</div>
                    <div className="prompt-timestamp">{formatAbsoluteTime(item.createdAt)}</div>
                  </div>

                  <div className="chat-response">
                    <div className="chat-response-head">
                      <div className="answer-panel-title">
                        <AppIcon name="bot" size={18} />
                        <span>回复内容</span>
                      </div>
                    </div>
                    <div className="answer-result-card">
                      <div className="chat-response-body">
                        <TurnResponseBody
                          turn={item}
                          agentBuilderActionBusyId={agentBuilderActionBusyId}
                          agentBuilderActionError={agentBuilderActionError}
                          agentBuilderActionNotice={agentBuilderActionNotice}
                          agentBuilderActionTargetId={agentBuilderActionTargetId}
                          loading={sessionRunning}
                          activeTurnId={activeTurnId}
                          onCreateAgentDraft={onCreateAgentDraft}
                          showExecutionRail={showExecutionRail}
                          onImageClick={handleMarkdownImageClick}
                        />
                      </div>
                      {item.answer || getElapsedMs(item.createdAt, item.completedAt, sessionRunning && item.id === activeTurnId) || hasUsageMetrics(item.usage) ? (
                        <div className="answer-result-actions">
                          <TurnExecutionDetails turn={item} isStreaming={sessionRunning && item.id === activeTurnId} />
                          <button
                            type="button"
                            className={`answer-copy-icon-button ${copiedTurnId === item.id ? 'copied' : ''}`}
                            onClick={() => void handleCopyAnswer(item.id, item.answer)}
                            aria-label={copiedTurnId === item.id ? '已复制结果' : '复制结果'}
                            title={copiedTurnId === item.id ? '已复制结果' : '复制结果'}
                            disabled={!item.answer}
                          >
                            {copiedTurnId === item.id ? <Check size={18} /> : <Copy size={18} />}
                          </button>
                        </div>
                      ) : null}
                    </div>
                  </div>
                </article>
              ))}
            </div>
          </>
        ) : (
          <section className="new-task-home">
            <div className="home-brand-block">
              <div className="home-brand-mark">NineClaw</div>
            </div>

            <div className="home-composer-shell">
              <div className="starter-chip-row">
                {STARTER_CHIPS.map((chip) => (
                  <button key={chip} type="button" className="starter-chip" onClick={() => setDraft(chip)}>
                    {chip}
                  </button>
                ))}
              </div>
            </div>
          </section>
        )}
      </section>

      {error ? <div className="error-banner">{error}</div> : null}

      <div className="workspace-composer-shell">
        <form
          className="composer-card"
          onSubmit={(event) => {
            event.preventDefault()
            void onSubmit()
          }}
        >
          <textarea
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={handleComposerKeyDown}
            placeholder={composerPlaceholder}
            rows={4}
            disabled={sessionStreaming}
          />
          <div className="composer-toolbar">
            <div className="composer-toolbar-left">
              <button type="button" className="ghost-icon-button" aria-label="附件">
                <AppIcon name="attachment" size={18} />
              </button>
              <button type="button" className="ghost-icon-button" aria-label="技能">
                <AppIcon name="spark" size={18} />
              </button>
              {selectedAgent ? (
                <div className="agent-chip">
                  <span className="agent-chip-dot" style={{ backgroundColor: getAgentColor(selectedAgent) }} />
                  <span>{selectedAgent.name}</span>
                </div>
              ) : null}
            </div>
            <div className="composer-toolbar-right">
              {sessionStreaming ? (
                <button type="button" className="stop-button" onClick={onAbort}>
                  <AppIcon name="stop" size={18} />
                </button>
              ) : (
                <button type="submit" className="submit-button" disabled={sessionStreaming}>
                  <AppIcon name="send" size={18} />
                </button>
              )}
            </div>
          </div>
        </form>
        <div className="composer-footnote">
          {globalBusy && !sessionStreaming
            ? '其他会话也在执行中；当前会话仍可继续发送。'
            : `发送快捷键：${getSubmitShortcutLabel(submitShortcut)}。Shift + Enter 可换行。`}
        </div>
      </div>

      {previewImage ? (
        <ImagePreviewModal
          alt={previewImage.alt}
          src={previewImage.src}
          onClose={() => setPreviewImage(null)}
        />
      ) : null}
    </div>
  )
}

function TurnResponseBody({
  turn,
  agentBuilderActionBusyId,
  agentBuilderActionError,
  agentBuilderActionNotice,
  agentBuilderActionTargetId,
  loading,
  activeTurnId,
  onCreateAgentDraft,
  showExecutionRail,
  onImageClick,
}: {
  turn: ConversationTurn
  agentBuilderActionBusyId: string
  agentBuilderActionError: string
  agentBuilderActionNotice: string
  agentBuilderActionTargetId: string
  loading: boolean
  activeTurnId: string
  onCreateAgentDraft: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
  showExecutionRail: boolean
  onImageClick: (src: string, alt: string) => void
}) {
  const toolById = new Map(turn.toolCalls.map((t) => [t.toolCallId, t]))
  const segments = turn.responseSegments

  if (segments && segments.length > 0) {
    let lastTextSegmentIndex = -1
    for (let i = segments.length - 1; i >= 0; i -= 1) {
      if (segments[i]?.type === 'text') {
        lastTextSegmentIndex = i
        break
      }
    }

    return (
      <div className="turn-response-blocks">
        {segments.map((seg, index) => {
          if (seg.type === 'text') {
            const isStreaming = Boolean(
              loading && turn.id === activeTurnId && index === lastTextSegmentIndex,
            )
            if (!seg.text.trim() && !isStreaming) {
              return null
            }
            return (
              <MarkdownBlock
                key={`${turn.id}-t-${index}`}
                actionId={`${turn.id}-t-${index}`}
                actionBusyId={agentBuilderActionBusyId}
                actionError={agentBuilderActionError}
                actionNotice={agentBuilderActionNotice}
                actionTargetId={agentBuilderActionTargetId}
                content={seg.text}
                isStreaming={isStreaming}
                onCreateAgentDraft={onCreateAgentDraft}
                onImageClick={onImageClick}
              />
            )
          }
          if (!showExecutionRail) {
            return null
          }
          const toolCall = toolById.get(seg.toolCallId)
          if (!toolCall) {
            return null
          }
          return <ToolCallCard key={toolCall.id} toolCall={toolCall} />
        })}
      </div>
    )
  }

  const legacyTools = [...turn.toolCalls].sort((a, b) => a.createdAt - b.createdAt)
  const hasLegacyTools = showExecutionRail && legacyTools.length > 0

  return (
    <>
      {turn.answer ? (
        <MarkdownBlock
          actionId={`${turn.id}-legacy`}
          actionBusyId={agentBuilderActionBusyId}
          actionError={agentBuilderActionError}
          actionNotice={agentBuilderActionNotice}
          actionTargetId={agentBuilderActionTargetId}
          content={turn.answer}
          isStreaming={loading && turn.id === activeTurnId}
          onCreateAgentDraft={onCreateAgentDraft}
          onImageClick={onImageClick}
        />
      ) : null}
      {hasLegacyTools ? <ToolCallList toolCalls={legacyTools} /> : null}
      {!turn.answer && !hasLegacyTools ? (
        <p className="placeholder-copy">
          {loading && turn.id === activeTurnId
            ? '正在等待 pi 返回首段内容…'
            : legacyTools.length > 0
              ? '本轮主要产出了工具调用结果。'
              : '当前轮次还没有输出内容。'}
        </p>
      ) : null}
    </>
  )
}

function MarkdownBlock({
  actionId,
  actionBusyId,
  actionError,
  actionNotice,
  actionTargetId,
  content,
  isStreaming,
  onCreateAgentDraft,
  onImageClick,
}: {
  actionId: string
  actionBusyId: string
  actionError: string
  actionNotice: string
  actionTargetId: string
  content: string
  isStreaming: boolean
  onCreateAgentDraft: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
  onImageClick?: (src: string, alt: string) => void
}) {
  const agentDraft = parseAgentBuilderDraft(content)
  const cleanedContent = stripAgentBuilderBlock(content)
  const normalizedContent = normalizeMarkdownImageSources(cleanedContent)

  const handleClick = (event: MouseEvent<HTMLDivElement>) => {
    if (!onImageClick) {
      return
    }

    const target = event.target
    if (!(target instanceof HTMLElement)) {
      return
    }

    const image = target.closest('img')
    if (!(image instanceof HTMLImageElement) || !image.src) {
      return
    }

    event.preventDefault()
    onImageClick(image.src, image.alt)
  }

  return (
    <>
      {normalizedContent ? (
        <div className="markdown-content" onClick={handleClick}>
          <Suspense fallback={<MarkdownFallback content={normalizedContent} />}>
            <MarkdownRenderer content={normalizedContent} isStreaming={isStreaming} />
          </Suspense>
        </div>
      ) : null}
      {agentDraft ? (
        <AgentBuilderDraftCard
          actionError={actionTargetId === actionId && actionBusyId !== actionId ? actionError : ''}
          actionId={actionId}
          actionNotice={actionTargetId === actionId && actionBusyId !== actionId ? actionNotice : ''}
          busy={actionBusyId === actionId}
          draft={agentDraft}
          onCreate={onCreateAgentDraft}
        />
      ) : null}
    </>
  )
}

function MarkdownFallback({ content }: { content: string }) {
  return <div className="markdown-content-fallback">{content || ' '}</div>
}

function AgentBuilderDraftCard({
  actionError,
  actionId,
  actionNotice,
  busy,
  draft,
  onCreate,
}: {
  actionError: string
  actionId: string
  actionNotice: string
  busy: boolean
  draft: AgentBuilderDraft
  onCreate: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
}) {
  return (
    <div className="agent-builder-card">
      <div className="agent-builder-card-head">
        <div>
          <div className="agent-builder-card-kicker">智能体创建草案</div>
          <strong>{draft.name}</strong>
        </div>
        <span className="skill-pill subtle">{formatAgentExecutionModeLabel(draft.executionMode)}</span>
      </div>
      <p className="agent-builder-card-summary">{draft.summary}</p>
      <div className="agent-builder-card-grid">
        <span>默认模型：{draft.defaultProviderId && draft.defaultModel ? `${draft.defaultProviderId} / ${draft.defaultModel}` : '将使用当前聊天模型'}</span>
        <span>挂载技能：{draft.skillIds.length > 0 ? draft.skillIds.join('、') : '无'}</span>
      </div>
      {draft.workspaceNotes ? <div className="agent-builder-card-notes">{draft.workspaceNotes}</div> : null}
      {actionError ? <div className="skills-feedback error">{actionError}</div> : null}
      {actionNotice ? <div className="skills-feedback success">{actionNotice}</div> : null}
      <div className="agent-builder-card-actions">
        <button type="button" className="outline-button primary" onClick={() => void onCreate(draft, actionId)} disabled={busy}>
          {busy ? '创建中…' : '创建到智能体管理'}
        </button>
      </div>
    </div>
  )
}

function getToolCallStateLabel(state: ToolCallEntry['state']): string {
  if (state === 'running') return '执行中'
  if (state === 'done') return '已完成'
  return '失败'
}

function formatToolCallText(text: string, fallback: string, pretty = true): string {
  const trimmed = text.trim()
  if (!trimmed) {
    return fallback
  }
  if (!pretty) {
    return text
  }
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2)
  } catch {
    return trimmed
  }
}

function ToolCallStreamBlock({
  content,
  isStreaming,
}: {
  content: string
  isStreaming: boolean
}) {
  return (
    <div className="tool-call-code tool-call-code-stream">
      <div className="markdown-content">
        <Suspense fallback={<MarkdownFallback content={content || ' '} />}>
          <MarkdownRenderer content={content || ' '} isStreaming={isStreaming} />
        </Suspense>
      </div>
    </div>
  )
}

function ToolCallCard({ toolCall }: { toolCall: ToolCallEntry }) {
  const isStreaming = toolCall.state === 'running'
  const [detailsOpen, setDetailsOpen] = useState(() => isStreaming)

  const argsLive = formatToolCallText(toolCall.argsText, '无参数', false)
  const resultLive = formatToolCallText(toolCall.resultText, '暂无输出', false)
  const argsPretty = formatToolCallText(toolCall.argsText, '无参数')
  const resultPretty = formatToolCallText(toolCall.resultText, '暂无输出')

  return (
    <details
      className={`tool-call-card ${toolCall.state}`}
      open={detailsOpen}
      onToggle={(event) => setDetailsOpen(event.currentTarget.open)}
    >
      <summary className="tool-call-summary">
        <div className="tool-call-head">
          <div className="tool-call-title-group">
            <span className="tool-call-kicker">工具调用</span>
            <strong>{toolCall.toolName}</strong>
            <span className="tool-call-hint">{isStreaming ? '流式输出中…' : '点开看详情'}</span>
          </div>
          <div className="tool-call-summary-right">
            {isStreaming ? (
              <span className={`status-pill ${toolCall.state}`}>{getToolCallStateLabel(toolCall.state)}</span>
            ) : null}
            <AppIcon name="chevron-down" size={16} />
          </div>
        </div>
      </summary>

      <div className="tool-call-grid">
        <div className="tool-call-panel tool-call-panel-args">
          <span className="tool-call-panel-label">调用 / 参数</span>
          {isStreaming ? (
            <ToolCallStreamBlock content={argsLive} isStreaming />
          ) : (
            <pre className="tool-call-code">{argsPretty}</pre>
          )}
        </div>
        <div className="tool-call-panel tool-call-panel-result">
          <span className="tool-call-panel-label">输出结果</span>
          {isStreaming ? (
            <ToolCallStreamBlock content={resultLive} isStreaming />
          ) : (
            <pre className="tool-call-code">{resultPretty}</pre>
          )}
        </div>
      </div>
    </details>
  )
}

function ToolCallList({ toolCalls }: { toolCalls: ToolCallEntry[] }) {
  return (
    <div className="tool-call-list">
      {toolCalls.map((toolCall) => (
        <ToolCallCard key={toolCall.id} toolCall={toolCall} />
      ))}
    </div>
  )
}

function TurnExecutionDetails({ turn, isStreaming }: { turn: ConversationTurn; isStreaming: boolean }) {
  const totalDuration = getElapsedMs(turn.createdAt, turn.completedAt, isStreaming)
  const usage = turn.usage

  if (typeof totalDuration !== 'number' && !hasUsageMetrics(usage)) {
    return null
  }

  return (
    <div className="answer-result-meta">
      {typeof totalDuration === 'number' ? (
        <span className="answer-result-meta-item">总耗时：{formatDurationLabel(totalDuration)}</span>
      ) : null}
      {hasUsageMetrics(usage) ? (
        <span className="answer-result-meta-item">总 Token：{formatTokenCount(usage?.totalTokens)}</span>
      ) : null}
    </div>
  )
}

function ImagePreviewModal({ alt, src, onClose }: { alt: string; src: string; onClose: () => void }) {
  return (
    <div className="image-preview-backdrop" onClick={onClose} role="presentation">
      <button type="button" className="image-preview-close" onClick={onClose} aria-label="关闭图片预览">
        <AppIcon name="close" size={18} />
      </button>
      <div className="image-preview-dialog" onClick={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-label={alt || '图片预览'}>
        <img src={src} alt={alt} className="image-preview-image" />
        {alt ? <div className="image-preview-caption">{alt}</div> : null}
      </div>
    </div>
  )
}

type SkillsViewProps = {
  installedSkillCount: number
  installedSkills: InstalledSkillItem[]
  onChangeTab: (tab: SkillLibraryTab) => void
  onInstallByLink: () => void
  onInstallSystemSkill: (skillId: string) => Promise<void> | void
  onRefresh: () => Promise<void> | void
  sessionBusy: boolean
  setSearch: (value: string) => void
  skillsError: string
  skillsLoading: boolean
  systemSkillCount: number
  systemSkillCatalog: SystemSkillCatalog
  systemSkillInstallId: string
  tab: SkillLibraryTab
  skillSearch: string
  visibleSystemSkills: SystemSkillCatalog['skills']
}

function SkillsView({
  installedSkillCount,
  installedSkills,
  onChangeTab,
  onInstallByLink,
  onInstallSystemSkill,
  onRefresh,
  sessionBusy,
  setSearch,
  skillsError,
  skillsLoading,
  systemSkillCount,
  systemSkillCatalog,
  systemSkillInstallId,
  tab,
  skillSearch,
  visibleSystemSkills,
}: SkillsViewProps) {
  const isInstalledTab = tab === 'installed'

  return (
    <div className="page-shell">
      <header className="page-header">
        <h1>技能</h1>
      </header>

      <div className="page-toolbar">
        <div className="tab-row">
          <button
            type="button"
            className={`tab-button ${isInstalledTab ? 'active' : ''}`}
            onClick={() => onChangeTab('installed')}
          >
            已安装技能
          </button>
          <button
            type="button"
            className={`tab-button ${!isInstalledTab ? 'active' : ''}`}
            onClick={() => onChangeTab('system')}
          >
            系统技能库
          </button>
        </div>
        <button type="button" className="toolbar-link" onClick={() => void onRefresh()} disabled={skillsLoading}>
          <AppIcon name="refresh" size={18} />
          <span>{skillsLoading ? '刷新中…' : '刷新'}</span>
        </button>
      </div>

      <div className="search-row">
        <label className="search-field">
          <AppIcon name="search" size={18} />
          <input
            value={skillSearch}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={isInstalledTab ? '搜索已安装技能、路径或来源…' : '搜索系统技能库…'}
          />
        </label>
        <button
          type="button"
          className="primary-cta"
          onClick={onInstallByLink}
          disabled={skillsLoading || sessionBusy}
        >
          <AppIcon name="plus-circle" size={18} />
          <span>通过链接安装</span>
        </button>
      </div>

      {skillsError ? (
        <div className="skills-feedback error">
          <strong>技能读取失败</strong>
          <span>{skillsError}</span>
        </div>
      ) : null}

      {isInstalledTab ? (
        installedSkills.length > 0 ? (
          <div className="card-grid">
            {installedSkills.map((skill) => (
              <article key={skill.id} className="skill-card">
                <div className="skill-card-head">
                  <div className="skill-title-group">
                    <div className="skill-icon">
                      <AppIcon name="puzzle" size={18} />
                    </div>
                    <div>
                      <h3>{skill.name}</h3>
                      <p>{skill.description}</p>
                    </div>
                  </div>
                </div>

                <div className="skill-pill-row">
                  <span className={`skill-pill ${skill.scope}`}>{formatInstalledSkillScopeLabel(skill.scope)}</span>
                  <span className="skill-pill subtle">{formatInstalledSkillSource(skill)}</span>
                </div>
                <div className="skill-card-foot skill-card-foot-grid">
                  <span className="skill-path-text" title={skill.path}>
                    {skill.path}
                  </span>
                  <span>{formatOptionalAbsoluteTime(skill.updatedAt)}</span>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <div className="skill-library-empty">
            <div className="skill-library-empty-icon">
              <AppIcon name="puzzle" size={28} />
            </div>
            <strong>
              {skillsLoading
                ? '正在扫描技能目录…'
                : installedSkillCount > 0
                  ? '没有匹配的技能'
                  : '还没有读取到已安装技能'}
            </strong>
            <p>
              {installedSkillCount > 0
                ? '换个关键词试试，支持按技能名、描述、路径和来源搜索。'
                : '将自动扫描工作区和系统目录中的技能。你也可以先通过链接安装，系统会为你打开安装引导会话。'}
            </p>
            {installedSkillCount === 0 ? (
              <button
                type="button"
                className="outline-button"
                onClick={onInstallByLink}
                disabled={skillsLoading || sessionBusy}
              >
                通过链接安装
              </button>
            ) : null}
          </div>
        )
      ) : systemSkillCatalog.available && visibleSystemSkills.length > 0 ? (
        <div className="card-grid">
          {visibleSystemSkills.map((skill) => (
            <article key={skill.id} className="skill-card">
              <div className="skill-card-head">
                <div className="skill-title-group">
                  <div className="skill-icon">
                    <AppIcon name="bag" size={18} />
                  </div>
                  <div>
                    <h3>{skill.name}</h3>
                    <p>{skill.description}</p>
                  </div>
                </div>
              </div>
              <div className="skill-card-foot skill-card-foot-grid">
                <span>{skill.installUrl ?? '系统内置技能'}</span>
                <button
                  type="button"
                  className="outline-button"
                  onClick={() => void onInstallSystemSkill(skill.id)}
                  disabled={skill.installed || sessionBusy || !!systemSkillInstallId}
                >
                  {skill.installed
                    ? '已安装'
                    : systemSkillInstallId === skill.id
                      ? '安装中…'
                      : '安装到全局技能'}
                </button>
              </div>
            </article>
          ))}
        </div>
      ) : (
        <div className="skill-library-empty">
          <div className="skill-library-empty-icon">
            <AppIcon name="bag" size={28} />
          </div>
          <strong>
            {systemSkillCatalog.available && systemSkillCount > 0 ? '没有匹配的系统技能' : '系统技能库预留中'}
          </strong>
          <p>
            {systemSkillCatalog.available && systemSkillCount > 0
              ? '换个关键词试试，系统技能库上线后会支持按名称和描述检索。'
              : systemSkillCatalog.message}
          </p>
        </div>
      )}
    </div>
  )
}

type SkillInstallDialogProps = {
  error: string
  link: string
  loading: boolean
  onChangeLink: (value: string) => void
  onClose: () => void
  onConfirm: () => Promise<void>
}

function SkillInstallDialog({
  error,
  link,
  loading,
  onChangeLink,
  onClose,
  onConfirm,
}: SkillInstallDialogProps) {
  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="confirm-dialog skill-install-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="skill-install-title"
        onClick={(event) => event.stopPropagation()}
      >
        <h3 id="skill-install-title">通过链接安装技能</h3>
        <p>输入技能链接后，会自动打开一个新会话，由助手引导你完成安装与验证。</p>

        <label className="input-field skill-install-field">
          <span>技能链接</span>
          <input
            autoFocus
            value={link}
            onChange={(event) => onChangeLink(event.target.value)}
            placeholder="https://github.com/org/repo 或技能发布页地址"
          />
        </label>

        {error ? <p className="skill-install-error">{error}</p> : null}

        <div className="confirm-dialog-actions">
          <button type="button" className="outline-button" onClick={onClose} disabled={loading}>
            取消
          </button>
          <button type="button" className="outline-button primary" onClick={() => void onConfirm()} disabled={loading}>
            {loading ? '会话启动中…' : '打开安装会话'}
          </button>
        </div>
      </div>
    </div>
  )
}

type NewSessionDialogProps = {
  agents: AgentRecord[]
  loading: boolean
  modelOptions: { value: string; label: string }[]
  selectedAgentId: string
  selectedModelValue: string
  onChangeAgent: (agentId: string) => void
  onChangeModel: (value: string) => void
  onClose: () => void
  onConfirm: () => void
}

function NewSessionDialog({
  agents,
  loading,
  modelOptions,
  selectedAgentId,
  selectedModelValue,
  onChangeAgent,
  onChangeModel,
  onClose,
  onConfirm,
}: NewSessionDialogProps) {
  const selectedAgent = agents.find((item) => item.id === selectedAgentId) ?? null
  const selectedModel = sessionLlmDecode(selectedModelValue)
  const modelOptionsWithFallback =
    selectedModelValue && !modelOptions.some((item) => item.value === selectedModelValue)
      ? [
          {
            value: selectedModelValue,
            label: `${selectedModel?.providerId ?? selectedAgent?.defaultProviderId ?? '当前'} · ${
              selectedModel?.model ?? selectedAgent?.defaultModel ?? '模型'
            }（当前）`,
          },
          ...modelOptions,
        ]
      : modelOptions

  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="confirm-dialog new-session-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="new-session-title"
        onClick={(event) => event.stopPropagation()}
      >
        <h3 id="new-session-title">新会话</h3>
        <p>先选择一个智能体。会话默认使用该智能体挂载的模型，进入聊天后你仍然可以切换模型。</p>

        <div className="new-session-agent-list">
          {agents.length > 0 ? (
            agents.map((agent) => (
              <button
                key={agent.id}
                type="button"
                className={`new-session-agent-card ${agent.id === selectedAgentId ? 'active' : ''}`}
                onClick={() => onChangeAgent(agent.id)}
              >
                <span className="new-session-agent-badge" style={{ backgroundColor: getAgentColor(agent) }}>
                  <AppIcon name="bot" size={18} />
                </span>
                <span className="new-session-agent-copy">
                  <strong>{agent.name}</strong>
                  <span>{agent.summary}</span>
                </span>
              </button>
            ))
          ) : (
            <div className="skill-library-empty compact">
              <strong>{loading ? '智能体加载中…' : '还没有可用智能体'}</strong>
              <p>先去左上角智能体管理里创建或调整一个智能体。</p>
            </div>
          )}
        </div>

        <label className="input-field skill-install-field">
          <span>本会话模型</span>
          <select
            value={selectedModelValue}
            onChange={(event) => onChangeModel(event.target.value)}
            disabled={modelOptionsWithFallback.length === 0}
          >
            {modelOptionsWithFallback.length === 0 ? (
              <option value="">暂无已配置模型</option>
            ) : (
              modelOptionsWithFallback.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))
            )}
          </select>
        </label>

        {selectedAgent ? (
          <div className="new-session-tip">
            <strong>当前智能体</strong>
            <span>{selectedAgent.description}</span>
          </div>
        ) : null}

        <div className="confirm-dialog-actions">
          <button type="button" className="outline-button" onClick={onClose}>
            取消
          </button>
          <button
            type="button"
            className="outline-button primary"
            onClick={onConfirm}
            disabled={!selectedAgent}
          >
            进入会话
          </button>
        </div>
      </div>
    </div>
  )
}

type ResourcesViewProps = {
  onSearch: (value: string) => void
  resourceSearch: string
  visibleResources: ResourceItem[]
}

function ResourcesView({ onSearch, resourceSearch, visibleResources }: ResourcesViewProps) {
  return (
    <div className="page-shell">
      <header className="page-header">
        <h1>资源库</h1>
        <p>把模板、知识沉淀和可复用的交付资产放在同一个工作台里。</p>
      </header>

      <div className="resource-hero">
        <div>
          <strong>资源编排</strong>
          <span>先沉淀知识，再让 pi 从统一入口调度。</span>
        </div>
        <button type="button" className="primary-cta">
          <AppIcon name="plus-circle" size={18} />
          <span>新增资源</span>
        </button>
      </div>

      <label className="search-field wide">
        <AppIcon name="search" size={18} />
        <input value={resourceSearch} onChange={(event) => onSearch(event.target.value)} placeholder="搜索模板、规范、知识沉淀…" />
      </label>

      <div className="resource-grid">
        {visibleResources.map((resource) => (
          <article key={resource.id} className="resource-card">
            <div className="resource-tag">{resource.tag}</div>
            <h3>{resource.title}</h3>
            <p>{resource.description}</p>
            <div className="resource-meta">{resource.updatedAt}</div>
          </article>
        ))}
      </div>
    </div>
  )
}

type AgentSkillPickerDialogProps = {
  allSkillCount: number
  searchValue: string
  selectedSkillIds: string[]
  skills: InstalledSkillItem[]
  onClose: () => void
  onSearch: (value: string) => void
  onToggleSkill: (skillId: string) => void
}

function AgentSkillPickerDialog({
  allSkillCount,
  searchValue,
  selectedSkillIds,
  skills,
  onClose,
  onSearch,
  onToggleSkill,
}: AgentSkillPickerDialogProps) {
  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="confirm-dialog agent-skill-picker-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-skill-picker-title"
        onClick={(event) => event.stopPropagation()}
      >
        <h3 id="agent-skill-picker-title">添加挂载技能</h3>
        <p>从本地已安装技能里搜索并选择。选中的技能会作为当前智能体的运行时能力注入。</p>

        <label className="input-field skill-install-field">
          <span>搜索技能</span>
          <input
            autoFocus
            value={searchValue}
            onChange={(event) => onSearch(event.target.value)}
            placeholder="搜索技能名称、说明、来源或路径"
          />
        </label>

        <div className="agent-skill-picker-meta">
          <span>已安装 {allSkillCount} 个</span>
          <span>已选中 {selectedSkillIds.length} 个</span>
        </div>

        <div className="agent-skill-picker-list">
          {skills.length > 0 ? (
            skills.map((skill) => {
              const active = selectedSkillIds.includes(skill.id)
              return (
                <button
                  key={skill.id}
                  type="button"
                  className={`agent-skill-option ${active ? 'active' : ''}`}
                  onClick={() => onToggleSkill(skill.id)}
                >
                  <span className="agent-skill-option-copy">
                    <strong>{skill.name}</strong>
                    <span>{skill.description || '暂无技能说明。'}</span>
                    <small>
                      {formatInstalledSkillScopeLabel(skill.scope)} · {formatInstalledSkillSource(skill)}
                    </small>
                  </span>
                  <span className="agent-skill-option-action">{active ? '已添加' : '添加技能'}</span>
                </button>
              )
            })
          ) : (
            <div className="agent-skill-picker-empty">
              <strong>{allSkillCount > 0 ? '没有匹配的技能' : '暂无已安装技能'}</strong>
              <span>
                {allSkillCount > 0 ? '换个关键词继续搜索，或直接关闭弹窗。' : '先去技能库安装技能，再回到这里挂载。'}
              </span>
            </div>
          )}
        </div>

        <div className="confirm-dialog-actions">
          <button type="button" className="outline-button" onClick={onClose}>
            完成
          </button>
        </div>
      </div>
    </div>
  )
}

type AgentEditorDialogProps = {
  agentDraft: AgentInput | null
  agentFormError: string
  agentFormNotice: string
  agentSaving: boolean
  allSkills: InstalledSkillItem[]
  defaultAgentId: string
  mode: 'create' | 'edit'
  modelOptions: { value: string; label: string }[]
  onArchiveAgent: () => void
  onClose: () => void
  onCreateAgent: () => void
  onDraftChange: (updates: Partial<AgentInput>) => void
  onOpenWorkspace: () => void
  onOpenSkillPicker: () => void
  onSaveAgent: () => void
  onSetDefaultAgent: () => void
  onToggleSkill: (skillId: string) => void
  selectedAgent: AgentRecord | null
}

function AgentEditorDialog({
  agentDraft,
  agentFormError,
  agentFormNotice,
  agentSaving,
  allSkills,
  defaultAgentId,
  mode,
  modelOptions,
  onArchiveAgent,
  onClose,
  onCreateAgent,
  onDraftChange,
  onOpenWorkspace,
  onOpenSkillPicker,
  onSaveAgent,
  onSetDefaultAgent,
  onToggleSkill,
  selectedAgent,
}: AgentEditorDialogProps) {
  if (!agentDraft) {
    return null
  }

  const selectedModelValue =
    agentDraft.defaultProviderId.trim() && agentDraft.defaultModel.trim()
      ? sessionLlmEncode(agentDraft.defaultProviderId, agentDraft.defaultModel)
      : ''
  const modelOptionsWithFallback =
    selectedModelValue && !modelOptions.some((item) => item.value === selectedModelValue)
      ? [
          {
            value: selectedModelValue,
            label: `${agentDraft.defaultProviderId} · ${agentDraft.defaultModel}（当前）`,
          },
          ...modelOptions,
        ]
      : modelOptions
  const missingSkillIds = agentDraft.skillIds.filter((skillId) => !allSkills.some((skill) => skill.id === skillId))
  const mountedSkills = allSkills.filter((skill) => agentDraft.skillIds.includes(skill.id))
  const editorAccent = getAgentColor(
    selectedAgent ?? { id: 'draft', name: agentDraft.name || '智能体', accentColor: agentDraft.accentColor },
  )

  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="agent-editor-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-editor-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="agent-editor-dialog-header">
          <div className="agent-editor-header-copy">
            <span className="agent-page-kicker">{mode === 'create' ? 'Create Agent' : 'Agent Editor'}</span>
            <strong id="agent-editor-title">{mode === 'create' ? '新建智能体' : selectedAgent?.name ?? '编辑智能体'}</strong>
            <span>
              {mode === 'create'
                ? ''
                : selectedAgent?.summary ?? '修改角色定位、运行模型和挂载技能。'}
            </span>
          </div>

          <button type="button" className="icon-button subtle" onClick={onClose} aria-label="关闭智能体编辑器">
            <AppIcon name="close" size={18} />
          </button>
        </div>

        <div className="agent-editor-dialog-scroll">
          <div className="agent-detail-card agent-editor-card">
            <div className="agent-detail-hero" style={{ borderColor: `${editorAccent}1f` }}>
              <div className="agent-detail-hero-main">
                <span className="agent-badge large" style={{ backgroundColor: editorAccent }}>
                  <AppIcon name="bot" size={24} />
                </span>
                <div className="agent-detail-copy">
                  <span className="agent-page-kicker">{mode === 'create' ? 'Create Agent' : 'Agent Editor'}</span>
                  <h2>{mode === 'create' ? '新建智能体' : selectedAgent?.name ?? '编辑智能体'}</h2>
                  <p>
                    {mode === 'create'
                      ? '填写名字、简介、介绍和默认模型，构建一个可直接在会话里使用的智能体。保存后会自动生成它自己的 markdown 工作区。'
                      : selectedAgent?.summary ?? '修改这个智能体的角色说明、挂载技能和默认运行配置。NineClaw 会补齐它自己的 markdown 工作区文件。'}
                  </p>
                </div>
              </div>

              <div className="agent-hero-pills">
                <span className="agent-hero-pill">{mode === 'create' ? '未保存' : '用户智能体'}</span>
                <span className="agent-hero-pill">{formatAgentExecutionModeLabel(agentDraft.executionMode)}</span>
                <span className="agent-hero-pill">{agentDraft.skillIds.length} 个挂载技能</span>
                {selectedAgent?.id === defaultAgentId ? <span className="agent-hero-pill accent">当前默认</span> : null}
              </div>
            </div>

            {agentFormError ? (
              <div className="skills-feedback error agent-feedback inline">
                <strong>保存失败</strong>
                <span>{agentFormError}</span>
              </div>
            ) : null}

            {agentFormNotice ? (
              <div className="skills-feedback success agent-feedback inline">
                <strong>已更新</strong>
                <span>{agentFormNotice}</span>
              </div>
            ) : null}

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>基础信息</strong>
                  <p>名字用于识别，简介用于列表扫读，介绍用于补足能力边界和擅长场景。</p>
                </div>
              </div>

              <div className="agent-form-grid">
                <label className="input-field">
                  <span>名字</span>
                  <input
                    value={agentDraft.name}
                    onChange={(event) => onDraftChange({ name: event.target.value })}
                    placeholder="例如：诉讼项目助理"
                  />
                </label>

                <label className="input-field">
                  <span>简介</span>
                  <input
                    value={agentDraft.summary}
                    onChange={(event) => onDraftChange({ summary: event.target.value })}
                    placeholder="列表里展示的一句话能力简介"
                  />
                </label>
              </div>

              <label className="input-field agent-field-full">
                <span>介绍</span>
                <textarea
                  value={agentDraft.description}
                  onChange={(event) => onDraftChange({ description: event.target.value })}
                  rows={5}
                  placeholder="详细说明这个智能体负责什么、擅长什么、回答风格和约束是什么"
                />
              </label>
            </div>

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>运行配置</strong>
                  <p>新会话会默认带出这里的模型和模式；进入聊天后，模型仍允许按会话单独切换。</p>
                </div>
              </div>

              <div className="agent-form-grid">
                <label className="input-field">
                  <span>默认模型</span>
                  <select
                    value={selectedModelValue}
                    onChange={(event) => {
                      const parsed = sessionLlmDecode(event.target.value)
                      if (parsed) {
                        onDraftChange({
                          defaultProviderId: parsed.providerId,
                          defaultModel: parsed.model,
                        })
                      }
                    }}
                  >
                    {modelOptionsWithFallback.length === 0 ? (
                      <option value="">暂无已配置模型</option>
                    ) : (
                      modelOptionsWithFallback.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))
                    )}
                  </select>
                </label>

                <label className="input-field">
                  <span>执行模式</span>
                  <select
                    value={agentDraft.executionMode}
                    onChange={(event) =>
                      onDraftChange({
                        executionMode: (event.target.value as AgentExecutionMode) || 'single',
                      })
                    }
                  >
                    <option value="single">单智能体</option>
                    <option value="supervisor">协调者（预留）</option>
                    <option value="worker">执行者（预留）</option>
                  </select>
                </label>
              </div>

              <label className="input-field agent-field-full">
                <span>高级指令（可选）</span>
                <textarea
                  value={agentDraft.systemPrompt}
                  onChange={(event) => onDraftChange({ systemPrompt: event.target.value })}
                  rows={6}
                  placeholder="可补充这个智能体的额外执行约束、回答方式或边界要求。留空时会根据名字、简介和介绍自动生成角色上下文。"
                />
              </label>
            </div>

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>挂载技能</strong>
                  <p>通过搜索弹窗选择已安装技能。挂载后会通过 pi 的 `--skill` 注入到当前智能体运行时。</p>
                </div>

                <button
                  type="button"
                  className="outline-button"
                  onClick={onOpenSkillPicker}
                  disabled={allSkills.length === 0}
                >
                  <AppIcon name="plus" size={16} />
                  <span>添加技能</span>
                </button>
              </div>

              {mountedSkills.length > 0 || missingSkillIds.length > 0 ? (
                <div className="agent-mounted-skill-list">
                  {mountedSkills.map((skill) => (
                    <button
                      key={skill.id}
                      type="button"
                      className="agent-mounted-skill"
                      onClick={() => onToggleSkill(skill.id)}
                    >
                      <span>
                        <strong>{skill.name}</strong>
                        <small>
                          {formatInstalledSkillScopeLabel(skill.scope)} · {formatInstalledSkillSource(skill)}
                        </small>
                      </span>
                      <AppIcon name="close" size={16} />
                    </button>
                  ))}

                  {missingSkillIds.map((skillId) => (
                    <button
                      key={skillId}
                      type="button"
                      className="agent-mounted-skill missing"
                      onClick={() => onToggleSkill(skillId)}
                    >
                      <span>
                        <strong>{skillId}</strong>
                        <small>本地未找到，点击即可移除</small>
                      </span>
                      <AppIcon name="close" size={16} />
                    </button>
                  ))}
                </div>
              ) : (
                <div className="agent-empty-block">
                  <strong>{allSkills.length > 0 ? '还没有挂载技能' : '暂无已安装技能'}</strong>
                  <span>
                    {allSkills.length > 0
                      ? '点击右上角“添加技能”，从已安装技能里搜索并挂载。'
                      : '先去技能库安装技能，再回到这里进行挂载。'}
                  </span>
                </div>
              )}

              {missingSkillIds.length > 0 ? (
                <div className="agent-missing-skills">
                  <strong>有技能记录当前未在本地找到：</strong>
                  <span>{missingSkillIds.join('、')}</span>
                </div>
              ) : null}
            </div>

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>会话行为</strong>
                  <p>默认模型来自智能体配置，但聊天窗口里仍支持用户按当前会话临时切换，不会回写智能体默认值。</p>
                </div>
              </div>
            </div>

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>工作区 Markdown</strong>
                  <p>查看这个智能体对应的 `IDENTITY.md`、`ROLE.md`、`MEMORY.md`、`WORKING.md` 等文件内容。</p>
                </div>

                <button
                  type="button"
                  className="outline-button"
                  onClick={onOpenWorkspace}
                  disabled={!selectedAgent || mode !== 'edit'}
                >
                  <AppIcon name="book" size={16} />
                  <span>{selectedAgent && mode === 'edit' ? '查看 md 文件' : '保存后可查看'}</span>
                </button>
              </div>

              <div className="agent-workspace-hint">
                <span>私有目录：{selectedAgent ? `agents/${selectedAgent.id}/` : '尚未创建'}</span>
                <span>共享文件：`AGENTS.md`、`SOUL.md`、`USER.md`、`MEMORY.md`、`TOOLS.md`</span>
              </div>
            </div>
          </div>
        </div>

        <div className="agent-editor-dialog-footer">
          <button type="button" className="outline-button" onClick={onCreateAgent} disabled={agentSaving}>
            重新新建
          </button>
          {selectedAgent && mode === 'edit' ? (
            <button
              type="button"
              className="outline-button"
              onClick={onSetDefaultAgent}
              disabled={agentSaving || selectedAgent.id === defaultAgentId}
            >
              {selectedAgent.id === defaultAgentId ? '当前默认' : '设为默认'}
            </button>
          ) : null}
          {selectedAgent && mode === 'edit' ? (
            <button type="button" className="outline-button danger" onClick={onArchiveAgent} disabled={agentSaving}>
              归档
            </button>
          ) : null}
          <button type="button" className="outline-button primary" onClick={onSaveAgent} disabled={agentSaving}>
            {agentSaving ? '保存中…' : mode === 'create' ? '创建智能体' : '保存修改'}
          </button>
        </div>
      </div>
    </div>
  )
}

type AgentWorkspaceDialogProps = {
  agentName: string
  bundle: AgentWorkspaceBundle | null
  draftContent: string
  error: string
  loading: boolean
  onClose: () => void
  onDraftChange: (value: string) => void
  onRefresh: () => void
  onSaveFile: (file: AgentWorkspaceFile, content: string) => void | Promise<void>
  onSelectFile: (key: string) => void
  saveError: string
  saveLoading: boolean
  saveNotice: string
  selectedFileKey: string
}

function AgentWorkspaceDialog({
  agentName,
  bundle,
  draftContent,
  error,
  loading,
  onClose,
  onDraftChange,
  onRefresh,
  onSaveFile,
  onSelectFile,
  saveError,
  saveLoading,
  saveNotice,
  selectedFileKey,
}: AgentWorkspaceDialogProps) {
  const files = bundle?.files ?? []
  const selectedFile =
    files.find((file) => file.key === selectedFileKey) ??
    files.find((file) => file.exists) ??
    files[0] ??
    null

  const sectionOrder: AgentWorkspaceFile['section'][] = ['private', 'dailyLog', 'shared']
  const selectedFileContent = selectedFile?.content ?? ''
  const selectedFileEditable = Boolean(selectedFile && !selectedFile.readOnly)
  const isDirty = Boolean(selectedFile && draftContent !== selectedFileContent)

  const handleResetDraft = () => {
    onDraftChange(selectedFileContent)
  }

  const handleSaveDraft = () => {
    if (!selectedFile || !selectedFileEditable || !isDirty || saveLoading) {
      return
    }

    void onSaveFile(selectedFile, draftContent)
  }

  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="agent-workspace-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-workspace-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="agent-editor-dialog-header">
          <div className="agent-editor-header-copy">
            <span className="agent-page-kicker">Workspace Markdown</span>
            <strong id="agent-workspace-title">{agentName} 的 md 工作区</strong>
            <span>{bundle ? `${bundle.workspaceRoot} · ${bundle.agentHome}` : '读取这个智能体的共享与私有 markdown 文件。'}</span>
          </div>

          <div className="agent-workspace-toolbar">
            <button type="button" className="outline-button" onClick={onRefresh} disabled={loading}>
              <AppIcon name="refresh" size={16} />
              <span>{loading ? '刷新中…' : '刷新'}</span>
            </button>
            <button type="button" className="icon-button subtle" onClick={onClose} aria-label="关闭工作区 Markdown">
              <AppIcon name="close" size={18} />
            </button>
          </div>
        </div>

        <div className="agent-workspace-layout">
          <aside className="agent-workspace-sidebar">
            {sectionOrder.map((section) => {
              const sectionFiles = files.filter((file) => file.section === section)
              if (sectionFiles.length === 0) {
                return null
              }

              return (
                <div key={section} className="agent-workspace-group">
                  <div className="agent-workspace-group-title">{formatWorkspaceFileSectionLabel(section)}</div>
                  <div className="agent-workspace-file-list">
                    {sectionFiles.map((file) => (
                      <button
                        key={file.key}
                        type="button"
                        className={`agent-workspace-file ${selectedFile?.key === file.key ? 'active' : ''}`}
                        onClick={() => onSelectFile(file.key)}
                      >
                        <strong>{file.name}</strong>
                        <span>{file.relativePath}</span>
                        <small>{file.exists ? '可读取' : '文件不存在'}</small>
                      </button>
                    ))}
                  </div>
                </div>
              )
            })}
          </aside>

          <section className="agent-workspace-content-panel">
            {loading ? (
              <div className="agent-empty-block">
                <strong>正在读取工作区文件…</strong>
                <span>稍等，NineClaw 正在从对应的 agent home 拉取 markdown 内容。</span>
              </div>
            ) : null}

            {!loading && error ? (
              <div className="skills-feedback error agent-feedback inline">
                <strong>读取失败</strong>
                <span>{error}</span>
              </div>
            ) : null}

            {!loading && !error && selectedFile ? (
              <div className="agent-workspace-file-preview">
                <div className="agent-workspace-file-meta">
                  <div>
                    <strong>{selectedFile.name}</strong>
                    <span>{selectedFile.relativePath}</span>
                  </div>
                  <span
                    className={`agent-workspace-status ${
                      selectedFile.readOnly ? 'readonly' : selectedFile.exists ? 'ok' : 'missing'
                    }`}
                  >
                    {selectedFile.readOnly ? '只读' : selectedFile.exists ? '可编辑' : '保存后创建'}
                  </span>
                </div>

                {selectedFile.readOnly ? (
                  <div className="agent-workspace-hint">
                    <strong>这个文件由系统运行时维护。</strong>
                    <span>当前只提供查看，不支持从配置页直接覆盖写入。</span>
                  </div>
                ) : null}

                <div className="agent-workspace-editor-actions">
                  <span className="agent-workspace-editor-hint">
                    {selectedFile.readOnly
                      ? '只读文件'
                      : selectedFile.scope === 'shared'
                        ? '共享文件，保存后会影响所有智能体'
                      : selectedFile.exists
                        ? '可直接编辑并保存回 workspace'
                        : '这个文件当前不存在，保存后会自动创建'}
                  </span>
                  <div className="confirm-dialog-actions agent-workspace-editor-buttons">
                    <button
                      type="button"
                      className="outline-button"
                      onClick={handleResetDraft}
                      disabled={!selectedFileEditable || !isDirty || saveLoading}
                    >
                      重置
                    </button>
                    <button
                      type="button"
                      className="primary-button"
                      onClick={handleSaveDraft}
                      disabled={!selectedFileEditable || !isDirty || saveLoading}
                    >
                      {saveLoading ? '保存中…' : '保存'}
                    </button>
                  </div>
                </div>

                {saveError ? (
                  <div className="skills-feedback error agent-feedback inline">
                    <strong>保存失败</strong>
                    <span>{saveError}</span>
                  </div>
                ) : null}

                {saveNotice ? (
                  <div className="skills-feedback success agent-feedback inline">
                    <strong>保存成功</strong>
                    <span>{saveNotice}</span>
                  </div>
                ) : null}

                <textarea
                  className="agent-workspace-content agent-workspace-editor"
                  value={draftContent}
                  onChange={(event) => onDraftChange(event.target.value)}
                  placeholder={selectedFile.exists ? '' : '# 新文件\n\n在这里输入要保存的 markdown 内容。'}
                  readOnly={!selectedFileEditable}
                  spellCheck={false}
                />
              </div>
            ) : null}

            {!loading && !error && !selectedFile ? (
              <div className="agent-empty-block">
                <strong>没有可展示的文件</strong>
                <span>这个智能体还没有生成任何 markdown 文件。</span>
              </div>
            ) : null}
          </section>
        </div>
      </div>
    </div>
  )
}

type AgentsViewProps = {
  agentDraft: AgentInput | null
  agentEditorOpen: boolean
  agentFormError: string
  agentFormNotice: string
  agentSaving: boolean
  agentWorkspaceBundle: AgentWorkspaceBundle | null
  agentWorkspaceDialogError: string
  agentWorkspaceDialogLoading: boolean
  agentWorkspaceDialogOpen: boolean
  agentWorkspaceDraftContent: string
  agentWorkspaceSaveError: string
  agentWorkspaceSaveNotice: string
  agentWorkspaceSaving: boolean
  agentWorkspaceSelectedKey: string
  agents: AgentRecord[]
  allSkills: InstalledSkillItem[]
  defaultAgentId: string
  error: string
  loading: boolean
  mode: 'create' | 'edit'
  modelOptions: { value: string; label: string }[]
  onArchiveAgent: () => void
  onCloseEditor: () => void
  onCloseWorkspaceDialog: () => void
  onCreateAgent: () => void
  onDraftChange: (updates: Partial<AgentInput>) => void
  onDraftWorkspaceContentChange: (value: string) => void
  onOpenWorkspace: () => void
  onOpenSkillPicker: () => void
  onRefreshWorkspace: () => void
  onSaveAgent: () => void
  onSaveWorkspaceFile: (file: AgentWorkspaceFile, content: string) => void | Promise<void>
  onSearch: (value: string) => void
  onSelectAgent: (id: string) => void
  onSelectWorkspaceFile: (key: string) => void
  onSetDefaultAgent: () => void
  onToggleSkill: (skillId: string) => void
  searchValue: string
  selectedAgent: AgentRecord | null
}

function AgentsView({
  agentDraft,
  agentEditorOpen,
  agentFormError,
  agentFormNotice,
  agentSaving,
  agentWorkspaceBundle,
  agentWorkspaceDialogError,
  agentWorkspaceDialogLoading,
  agentWorkspaceDialogOpen,
  agentWorkspaceDraftContent,
  agentWorkspaceSaveError,
  agentWorkspaceSaveNotice,
  agentWorkspaceSaving,
  agentWorkspaceSelectedKey,
  agents,
  allSkills,
  defaultAgentId,
  error,
  loading,
  mode,
  modelOptions,
  onArchiveAgent,
  onCloseEditor,
  onCloseWorkspaceDialog,
  onCreateAgent,
  onDraftChange,
  onDraftWorkspaceContentChange,
  onOpenWorkspace,
  onOpenSkillPicker,
  onRefreshWorkspace,
  onSaveAgent,
  onSaveWorkspaceFile,
  onSearch,
  onSelectAgent,
  onSelectWorkspaceFile,
  onSetDefaultAgent,
  onToggleSkill,
  searchValue,
  selectedAgent,
}: AgentsViewProps) {
  const studioCountLabel = loading ? '正在同步智能体…' : '已保存智能体'

  return (
    <div className="agent-layout">
      <div className="agent-studio-shell">
        <header className="agent-page-header agent-page-header-inline">
          <div>
            <span className="agent-page-kicker">Agent Studio</span>
            <h1>智能体管理</h1>
            <p></p>
          </div>

          <button type="button" className="create-agent-button agent-create-inline" onClick={onCreateAgent}>
            <AppIcon name="plus" size={20} />
            <span>新建智能体</span>
          </button>
        </header>

        <section className="agent-list-panel">
          <div className="agent-sidebar-toolbar agent-toolbar-inline">
            <label className="search-field wide agent-search-field">
              <AppIcon name="search" size={18} />
              <input value={searchValue} onChange={(event) => onSearch(event.target.value)} placeholder="搜索名字、简介或介绍…" />
            </label>

            <div className="agent-sidebar-summary">
              <strong>{agents.length}</strong>
              <span>{studioCountLabel}</span>
            </div>
          </div>

          {error ? (
            <div className="skills-feedback error agent-feedback inline">
              <strong>智能体读取失败</strong>
              <span>{error}</span>
            </div>
          ) : null}

          <div className="agent-list-shell">
            <div className="agent-list-caption">我的智能体</div>
            <div className="agent-list">
              {agents.length > 0 ? (
                agents.map((agent) => (
                  <button
                    key={agent.id}
                    type="button"
                    className={`agent-row list ${selectedAgent?.id === agent.id ? 'active' : ''}`}
                    onClick={() => onSelectAgent(agent.id)}
                  >
                    <span className="agent-row-tone" style={{ backgroundColor: getAgentColor(agent) }} />
                    <span className="agent-badge" style={{ backgroundColor: getAgentColor(agent) }}>
                      <AppIcon name="bot" size={18} />
                    </span>
                    <span className="agent-copy">
                      <strong>
                        {agent.name}
                        {agent.id === defaultAgentId ? <span className="agent-inline-tag">默认</span> : null}
                      </strong>
                      <span>{agent.summary}</span>
                      <small>{agent.description || '点击进入弹窗，补充介绍、模型和挂载技能。'}</small>
                    </span>
                    <span className="agent-list-meta">
                      <span className="agent-list-meta-pill">
                        {agent.defaultProviderId} · {agent.defaultModel}
                      </span>
                      <span className="agent-list-meta-pill">{agent.skillIds.length} 个技能</span>
                      <span className="agent-row-action">编辑配置</span>
                    </span>
                  </button>
                ))
              ) : (
                <div className="empty-history-card compact agent-list-empty">
                  <div className="empty-history-title">{loading ? '正在读取智能体…' : '还没有已保存智能体'}</div>
                  <span>先新建一个智能体，再为它配置简介、模型和挂载技能。</span>
                </div>
              )}
            </div>
          </div>
        </section>
      </div>

      {agentEditorOpen && agentDraft ? (
        <AgentEditorDialog
          agentDraft={agentDraft}
          agentFormError={agentFormError}
          agentFormNotice={agentFormNotice}
          agentSaving={agentSaving}
          allSkills={allSkills}
          defaultAgentId={defaultAgentId}
          mode={mode}
          modelOptions={modelOptions}
          onArchiveAgent={onArchiveAgent}
          onClose={onCloseEditor}
          onCreateAgent={onCreateAgent}
          onDraftChange={onDraftChange}
          onOpenWorkspace={onOpenWorkspace}
          onOpenSkillPicker={onOpenSkillPicker}
          onSaveAgent={onSaveAgent}
          onSetDefaultAgent={onSetDefaultAgent}
          onToggleSkill={onToggleSkill}
          selectedAgent={selectedAgent}
        />
      ) : null}

      {agentWorkspaceDialogOpen ? (
        <AgentWorkspaceDialog
          agentName={selectedAgent?.name ?? '智能体'}
          bundle={agentWorkspaceBundle}
          draftContent={agentWorkspaceDraftContent}
          error={agentWorkspaceDialogError}
          loading={agentWorkspaceDialogLoading}
          onClose={onCloseWorkspaceDialog}
          onDraftChange={onDraftWorkspaceContentChange}
          onRefresh={onRefreshWorkspace}
          onSaveFile={onSaveWorkspaceFile}
          onSelectFile={onSelectWorkspaceFile}
          saveError={agentWorkspaceSaveError}
          saveLoading={agentWorkspaceSaving}
          saveNotice={agentWorkspaceSaveNotice}
          selectedFileKey={agentWorkspaceSelectedKey}
        />
      ) : null}
    </div>
  )
}

type SettingsModalProps = {
  activeProviderBadge: string
  allProviderDefinitions: ProviderDefinition[]
  appearanceSettings: AppearanceSettings
  botConfigs: Record<string, BotConfig>
  botLoading: boolean
  botStatusLog: BotStatusEvent[]
  generalSettings: GeneralSettings
  onAddCustomProvider: (name: string, description: string) => void
  onProviderConfigChange: (providerId: ProviderId, updates: Partial<ProviderConfig>) => void
  onBotConfigChange: (channelId: BotChannelId, updates: Partial<BotConfig>) => void
  onClose: () => void
  onRemoveCustomProvider: (providerId: ProviderId) => void
  onWechatLogin: () => void
  onWechatStart: () => void
  onWechatStop: () => void
  onSelectProvider: (id: ProviderId) => void
  onSelectBot: (id: BotChannelId) => void
  onSelectTab: (tab: SettingsTab) => void
  providerConfigs: Record<string, ProviderConfig>
  qrCodeUrl: string
  qrDialogOpen: boolean
  qrStatus: 'waiting' | 'scanned' | 'confirmed' | 'error'
  selectedProviderConfig: ProviderConfig
  selectedProviderDefinition: ProviderDefinition
  selectedProviderId: ProviderId
  selectedBotConfig: BotConfig
  selectedBotDefinition: BotDefinition
  selectedBotId: BotChannelId
  setAppearanceSettings: (value: AppearanceSettings | ((previous: AppearanceSettings) => AppearanceSettings)) => void
  setGeneralSettings: (value: GeneralSettings | ((previous: GeneralSettings) => GeneralSettings)) => void
  setBotLoading: (loading: boolean) => void
  setQrDialogOpen: (open: boolean) => void
  tab: SettingsTab
}

function SettingsModal({
  activeProviderBadge,
  allProviderDefinitions,
  appearanceSettings,
  botConfigs,
  botLoading,
  botStatusLog,
  generalSettings,
  onAddCustomProvider,
  onProviderConfigChange,
  onBotConfigChange,
  onClose,
  onRemoveCustomProvider,
  onWechatLogin,
  onWechatStart,
  onWechatStop,
  onSelectProvider,
  onSelectBot,
  onSelectTab,
  providerConfigs,
  qrCodeUrl,
  qrDialogOpen,
  qrStatus,
  selectedProviderConfig,
  selectedProviderDefinition,
  selectedProviderId,
  selectedBotConfig,
  selectedBotDefinition,
  selectedBotId,
  setAppearanceSettings,
  setBotLoading,
  setGeneralSettings,
  setQrDialogOpen,
  tab,
}: SettingsModalProps) {
  const [providerAddMode, setProviderAddMode] = useState(false)
  const [customFormOpen, setCustomFormOpen] = useState(false)
  const [customName, setCustomName] = useState('')
  const [customDescription, setCustomDescription] = useState('')
  const [providerTestLoading, setProviderTestLoading] = useState(false)
  const [providerTestNote, setProviderTestNote] = useState<{ kind: 'ok' | 'err'; text: string } | null>(null)
  const [providerDeleteConfirmId, setProviderDeleteConfirmId] = useState<ProviderId | null>(null)

  const addedProviders = allProviderDefinitions.filter((p) => providerConfigs[p.id]?.added)
  const availableProviders = allProviderDefinitions.filter((p) => !providerConfigs[p.id]?.added)

  const handleProviderSelect = (providerId: ProviderId) => {
    if (providerAddMode) {
      onProviderConfigChange(providerId, { added: true })
      onSelectProvider(providerId)
      setProviderAddMode(false)
    } else {
      onSelectProvider(providerId)
    }
  }

  const handleRemoveProvider = (providerId: ProviderId) => {
    if (selectedProviderId === providerId) {
      const nextAdded = addedProviders.find((p) => p.id !== providerId)
      if (nextAdded) {
        onSelectProvider(nextAdded.id)
      }
    }
    if (providerId.startsWith('custom_')) {
      onRemoveCustomProvider(providerId)
    } else {
      onProviderConfigChange(providerId, { added: false, enabled: false })
    }
  }

  useEffect(() => {
    if (!providerDeleteConfirmId) {
      return
    }
    const onKeyDown = (event: Event) => {
      if (event instanceof KeyboardEvent && !isFnLikeKeyboardEvent(event) && event.key === 'Escape') {
        setProviderDeleteConfirmId(null)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [providerDeleteConfirmId])

  const handleSubmitCustomProvider = () => {
    const name = customName.trim()
    if (!name) {
      return
    }
    onAddCustomProvider(name, customDescription.trim())
    setCustomFormOpen(false)
    setCustomName('')
    setCustomDescription('')
    setProviderAddMode(false)
  }

  const pendingDeleteLabel =
    providerDeleteConfirmId &&
    (() => {
      const def = allProviderDefinitions.find((p) => p.id === providerDeleteConfirmId)
      const cfg = providerConfigs[providerDeleteConfirmId]
      return def && cfg ? providerDisplayName(def, cfg) : providerDeleteConfirmId
    })()

  return (
    <>
      <div className="modal-backdrop">
        <div className="settings-dialog" role="dialog" aria-modal="true" aria-label="设置">
        <div className="settings-sidebar">
          <header>
            <h2>设置</h2>
          </header>

          <div className="settings-tab-list">
            <SettingsTabButton active={tab === 'general'} icon="settings" label="通用" onClick={() => onSelectTab('general')} />
            <SettingsTabButton active={tab === 'appearance'} icon="sparkles" label="个性化" onClick={() => onSelectTab('appearance')} />
            <SettingsTabButton active={tab === 'providers'} icon="provider" label="大模型 Provider" onClick={() => onSelectTab('providers')} />
            <SettingsTabButton active={tab === 'bots'} icon="message" label="IM 机器人" onClick={() => onSelectTab('bots')} />
            <SettingsTabButton active={tab === 'shortcuts'} icon="keyboard" label="快捷键" onClick={() => onSelectTab('shortcuts')} />
          </div>
        </div>

        <div className="settings-content">
          <div className="settings-content-head">
            <h2>
              {tab === 'general'
                ? '通用'
                : tab === 'appearance'
                  ? '个性化'
                  : tab === 'providers'
                    ? '大模型 Provider'
                    : tab === 'bots'
                      ? 'IM 机器人'
                      : '快捷键'}
            </h2>
            <button type="button" className="icon-button subtle" onClick={onClose} aria-label="关闭设置">
              <AppIcon name="close" size={20} />
            </button>
          </div>

          {tab === 'general' ? (
            <div className="settings-section-stack">
              <div className="settings-row">
                <div>
                  <strong>语言</strong>
                </div>
                <label className="select-field">
                  <select
                    value={generalSettings.language}
                    onChange={(event) =>
                      setGeneralSettings((previous) => ({
                        ...previous,
                        language: event.target.value as GeneralSettings['language'],
                      }))
                    }
                  >
                    <option value="中文">中文</option>
                    <option value="English">English</option>
                  </select>
                </label>
              </div>

              <SettingSwitch
                checked={generalSettings.launchOnStartup}
                description="系统启动时自动运行应用"
                label="开机自启动"
                onChange={() =>
                  setGeneralSettings((previous) => ({
                    ...previous,
                    launchOnStartup: !previous.launchOnStartup,
                  }))
                }
              />

              <SettingSwitch
                checked={generalSettings.useSystemProxy}
                description="开启后网络请求将跟随系统代理（保存后生效）"
                label="使用系统代理"
                onChange={() =>
                  setGeneralSettings((previous) => ({
                    ...previous,
                    useSystemProxy: !previous.useSystemProxy,
                  }))
                }
              />
            </div>
          ) : null}

          {tab === 'appearance' ? (
            <div className="settings-section-stack">
              <SettingSwitch
                checked={appearanceSettings.compactSidebar}
                description="压缩左侧导航宽度，适合更小的桌面窗口。"
                label="紧凑侧栏"
                onChange={() =>
                  setAppearanceSettings((previous) => ({
                    ...previous,
                    compactSidebar: !previous.compactSidebar,
                  }))
                }
              />
              <SettingSwitch
                checked={appearanceSettings.showExecutionRail}
                description="在聊天页显示执行状态卡片，而不是直接暴露模型内部推理。"
                label="显示执行轨迹"
                onChange={() =>
                  setAppearanceSettings((previous) => ({
                    ...previous,
                    showExecutionRail: !previous.showExecutionRail,
                  }))
                }
              />
              <SettingSwitch
                checked={appearanceSettings.preferReducedMotion}
                description="减少过渡动画，提升低性能机器上的响应感。"
                label="减少动画"
                onChange={() =>
                  setAppearanceSettings((previous) => ({
                    ...previous,
                    preferReducedMotion: !previous.preferReducedMotion,
                  }))
                }
              />
            </div>
          ) : null}

          {tab === 'providers' ? (
            <div className="bot-settings-layout">
              <div className="bot-channel-list">
                {providerAddMode ? (
                  <>
                    <div className="provider-add-header">
                      <span>选择要添加的 Provider</span>
                      <button
                        type="button"
                        className="link-button"
                        onClick={() => {
                          setProviderAddMode(false)
                          setCustomFormOpen(false)
                        }}
                      >
                        取消
                      </button>
                    </div>
                    {customFormOpen ? (
                      <div className="provider-custom-form">
                        <label className="input-field">
                          <span>供应商名称</span>
                          <input
                            value={customName}
                            onChange={(event) => setCustomName(event.target.value)}
                            placeholder="例如：公司内网网关"
                          />
                        </label>
                        <label className="input-field">
                          <span>说明（可选）</span>
                          <input
                            value={customDescription}
                            onChange={(event) => setCustomDescription(event.target.value)}
                            placeholder="OpenAI 兼容接口"
                          />
                        </label>
                        <div className="provider-actions">
                          <button type="button" className="outline-button" onClick={() => setCustomFormOpen(false)}>
                            返回
                          </button>
                          <button type="button" className="outline-button primary" onClick={handleSubmitCustomProvider}>
                            创建
                          </button>
                        </div>
                      </div>
                    ) : (
                      <>
                        <button type="button" className="provider-add-button subtle" onClick={() => setCustomFormOpen(true)}>
                          <AppIcon name="plus" size={18} />
                          <span>添加自定义供应商（OpenAI 兼容）</span>
                        </button>
                        {availableProviders.map((provider) => {
                          return (
                            <button
                              key={provider.id}
                              type="button"
                              className={`bot-channel-card ${selectedProviderId === provider.id ? 'active' : ''}`}
                              onClick={() => handleProviderSelect(provider.id)}
                            >
                              <span className="bot-channel-copy">
                                <strong>{provider.name}</strong>
                                <span>{provider.description}</span>
                              </span>
                              <AppIcon name="plus" size={16} />
                            </button>
                          )
                        })}
                        {availableProviders.length === 0 && (
                          <p className="settings-note">预设已全部添加；你仍可使用上方「自定义供应商」。</p>
                        )}
                      </>
                    )}
                  </>
                ) : (
                  <>
                    <button
                      type="button"
                      className="provider-add-button"
                      onClick={() => {
                        setProviderAddMode(true)
                        setCustomFormOpen(false)
                      }}
                    >
                      <AppIcon name="plus" size={18} />
                      <span>添加 Provider</span>
                    </button>
                    {addedProviders.length === 0 ? (
                      <p className="settings-note">暂未添加任何 Provider，请点击上方按钮添加。</p>
                    ) : (
                      addedProviders.map((provider) => {
                        const config = providerConfigs[provider.id]
                        return (
                          <div
                            key={provider.id}
                            className={`bot-channel-card ${selectedProviderId === provider.id ? 'active' : ''}`}
                            role="button"
                            tabIndex={0}
                            onClick={() => handleProviderSelect(provider.id)}
                            onKeyDown={(event) => {
                              if (event.key === 'Enter' || event.key === ' ') {
                                event.preventDefault()
                                handleProviderSelect(provider.id)
                              }
                            }}
                          >
                            <span className="bot-channel-copy">
                              <strong>{providerDisplayName(provider, config)}</strong>
                              <span>{config.status}</span>
                            </span>
                            <button
                              type="button"
                              className="provider-remove-button"
                              onClick={(e) => {
                                e.stopPropagation()
                                setProviderDeleteConfirmId(provider.id)
                              }}
                              aria-label={`移除 ${providerDisplayName(provider, config)}`}
                            >
                              <AppIcon name="close" size={14} />
                            </button>
                          </div>
                        )
                      })
                    )}
                  </>
                )}
              </div>

              <div className="bot-detail-panel">
                {addedProviders.length === 0 ? (
                  <div className="provider-empty-state">
                    <AppIcon name="provider" size={48} />
                    <p>请先添加一个 Provider</p>
                  </div>
                ) : (
                  <>
                    <div className="bot-detail-head provider-detail-head">
                      <div className="bot-detail-title">
                        <AppIcon name="provider" size={18} />
                        <strong className="bot-detail-heading">
                          {providerDisplayName(selectedProviderDefinition, selectedProviderConfig)} 配置
                        </strong>
                        <span className="bot-status-tag">{selectedProviderConfig.status}</span>
                      </div>
                      <div className="provider-runtime-badge">{activeProviderBadge}</div>
                    </div>

                    <p className="settings-note provider-note">{selectedProviderDefinition.description}</p>

                    <label className="input-field">
                      <span>显示名称</span>
                      <input
                        value={selectedProviderConfig.displayName}
                        onChange={(event) => onProviderConfigChange(selectedProviderId, { displayName: event.target.value })}
                        placeholder={selectedProviderDefinition.name}
                      />
                    </label>

                    <label className="input-field">
                      <span>Base URL</span>
                      <input
                        value={selectedProviderConfig.baseUrl}
                        onChange={(event) => onProviderConfigChange(selectedProviderId, { baseUrl: event.target.value })}
                        placeholder={selectedProviderDefinition.defaultBaseUrl}
                      />
                    </label>

                    <label className="input-field">
                      <span>API Key</span>
                      <input
                        type="password"
                        value={selectedProviderConfig.apiKey}
                        onChange={(event) => onProviderConfigChange(selectedProviderId, { apiKey: event.target.value })}
                        placeholder="请输入 API Key"
                      />
                    </label>

                    <label className="input-field">
                      <span>默认模型</span>
                      <input
                        value={selectedProviderConfig.model}
                        onChange={(event) => onProviderConfigChange(selectedProviderId, { model: event.target.value })}
                        placeholder={selectedProviderDefinition.suggestedModel}
                      />
                    </label>

                    <label className="input-field">
                      <span>备注</span>
                      <input
                        value={selectedProviderConfig.note}
                        onChange={(event) => onProviderConfigChange(selectedProviderId, { note: event.target.value })}
                        placeholder="例如：用于后续替换默认模型路由"
                      />
                    </label>

                    <div className="provider-actions">
                      <button
                        type="button"
                        className="outline-button"
                        onClick={() =>
                          onProviderConfigChange(selectedProviderId, {
                            status: getProviderStatus(selectedProviderConfig, false),
                          })
                        }
                      >
                        <AppIcon name="refresh" size={18} />
                        <span>校验配置</span>
                      </button>
                      <button
                        type="button"
                        className="outline-button"
                        disabled={providerTestLoading}
                        onClick={async () => {
                          setProviderTestLoading(true)
                          setProviderTestNote(null)
                          try {
                            const message = await testLlmProviderConnection({
                              baseUrl: selectedProviderConfig.baseUrl,
                              apiKey: selectedProviderConfig.apiKey,
                              model: selectedProviderConfig.model,
                            })
                            onProviderConfigChange(selectedProviderId, { status: '测试通过' })
                            setProviderTestNote({ kind: 'ok', text: message })
                          } catch (error) {
                            onProviderConfigChange(selectedProviderId, { status: '已配置' })
                            setProviderTestNote({ kind: 'err', text: String(error) })
                          } finally {
                            setProviderTestLoading(false)
                          }
                        }}
                      >
                        <AppIcon name="broadcast" size={18} />
                        <span>{providerTestLoading ? '测试中…' : '测试连通性'}</span>
                      </button>
                    </div>

                    {providerTestNote ? (
                      <p className={`settings-note ${providerTestNote.kind === 'err' ? 'error' : ''}`}>
                        {providerTestNote.text}
                      </p>
                    ) : null}

                    <p className="settings-note">
                      启用后的 Provider 会直接参与后续对话执行。为避免冲突，界面会保持单一启用项。
                    </p>
                  </>
                )}
              </div>
            </div>
          ) : null}

          {tab === 'bots' ? (
            <div className="bot-settings-layout">
              <div className="bot-channel-list">
                {botDefinitions.map((channel) => {
                  const config = botConfigs[channel.id]
                  return (
                    <div
                      key={channel.id}
                      className={`bot-channel-card ${selectedBotId === channel.id ? 'active' : ''}`}
                      role="button"
                      tabIndex={0}
                      onClick={() => onSelectBot(channel.id)}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter' || event.key === ' ') {
                          event.preventDefault()
                          onSelectBot(channel.id)
                        }
                      }}
                    >
                      <span className="bot-channel-copy">
                        <strong>{channel.name}</strong>
                        <span className={`bot-status-text ${config.status === '已连接' ? 'connected' : config.status === '错误' ? 'error' : ''}`}>{config.status}</span>
                      </span>
                      {channel.id === 'wechat' ? null : (
                        <Toggle checked={config.enabled} onChange={() => onBotConfigChange(channel.id, { enabled: !config.enabled })} />
                      )}
                    </div>
                  )
                })}
              </div>

              <div className="bot-detail-panel">
                {/* ── WeChat: QR Login + Connect/Disconnect ── */}
                {selectedBotId === 'wechat' ? (
                  <>
                    <div className="bot-detail-head">
                      <div className="bot-detail-title">
                        <AppIcon name="message" size={18} />
                        <strong>微信 Bot 设置</strong>
                        <span className={`bot-status-tag ${selectedBotConfig.status === '已连接' ? 'connected' : selectedBotConfig.status === '错误' ? 'error' : ''}`}>
                          {selectedBotConfig.status}
                        </span>
                      </div>
                    </div>

                    {selectedBotConfig.status === '已连接' ? (
                      <div className="bot-connected-info">
                        <p>微信 Bot 正在运行，每 3 秒轮询一次新消息并自动 AI 回复。</p>
                        <button
                          type="button"
                          className="outline-button danger"
                          onClick={onWechatStop}
                          disabled={botLoading}
                        >
                          <AppIcon name="stop" size={18} />
                          <span>{botLoading ? '断开中...' : '断开连接'}</span>
                        </button>
                        {botStatusLog.length > 0 ? (
                          <div className="bot-status-log">
                            <strong>Bot 运行日志</strong>
                            <div className="bot-status-entries">
                              {botStatusLog.slice(0, 8).map((entry, i) => (
                                <div key={i} className={`bot-status-entry ${entry.level}`}>
                                  <span className="bot-status-level">{entry.level}</span>
                                  <span className="bot-status-msg">{entry.message}</span>
                                </div>
                              ))}
                            </div>
                          </div>
                        ) : null}
                      </div>
                    ) : (
                      <>
                        <label className="input-field">
                          <span>iLink 服务地址</span>
                          <input
                            value={selectedBotConfig.clientSecret}
                            onChange={(event) => onBotConfigChange(selectedBotId, { clientSecret: event.target.value })}
                            placeholder="https://ilinkai.weixin.qq.com"
                          />
                        </label>

                        <label className="input-field">
                          <span>Bot Token (扫码后自动填入)</span>
                          <input
                            value={selectedBotConfig.clientId}
                            onChange={(event) => onBotConfigChange(selectedBotId, { clientId: event.target.value })}
                            placeholder="扫码登录后自动获取"
                            readOnly
                          />
                        </label>

                        <div className="bot-action-row">
                          <button
                            type="button"
                            className="outline-button primary"
                            onClick={onWechatLogin}
                            disabled={botLoading}
                          >
                            <AppIcon name="qr" size={18} />
                            <span>{botLoading ? '请稍候...' : '扫码登录'}</span>
                          </button>

                          {selectedBotConfig.clientId ? (
                            <button
                              type="button"
                              className="outline-button"
                              onClick={onWechatStart}
                              disabled={botLoading}
                            >
                              <AppIcon name="broadcast" size={18} />
                              <span>{botLoading ? '启动中...' : '启动 Bot'}</span>
                            </button>
                          ) : null}
                        </div>

                        {selectedBotConfig.errorMessage ? (
                          <p className="settings-note error">{selectedBotConfig.errorMessage}</p>
                        ) : null}

                        <p className="settings-note">微信 Bot 使用 iLink 协议，扫码登录后即可接收消息并自动 AI 回复。</p>
                      </>
                    )}
                  </>
                ) : (
                  <>
                    {/* ── Other Channels: original UI ── */}
                    <div className="bot-detail-head">
                      <div className="bot-detail-title">
                        <AppIcon name="message" size={18} />
                        <strong>{selectedBotDefinition.name} 设置</strong>
                        <span className="bot-status-tag">{selectedBotConfig.status}</span>
                      </div>
                      <button type="button" className="outline-button">
                        <AppIcon name="book" size={18} />
                        <span>{selectedBotDefinition.guideLabel}</span>
                      </button>
                    </div>

                    <label className="input-field">
                      <span>{selectedBotDefinition.keyLabel}</span>
                      <input
                        value={selectedBotConfig.clientId}
                        onChange={(event) => onBotConfigChange(selectedBotId, { clientId: event.target.value })}
                        placeholder={selectedBotDefinition.keyPlaceholder}
                      />
                    </label>
                    <label className="input-field">
                      <span>{selectedBotDefinition.secretLabel}</span>
                      <input
                        type="password"
                        value={selectedBotConfig.clientSecret}
                        onChange={(event) => onBotConfigChange(selectedBotId, { clientSecret: event.target.value })}
                        placeholder={selectedBotDefinition.secretPlaceholder}
                      />
                    </label>

                    <button
                      type="button"
                      className="outline-button"
                      onClick={() => onBotConfigChange(selectedBotId, { status: '待接入' })}
                    >
                      <AppIcon name="broadcast" size={18} />
                      <span>测试连通性</span>
                    </button>

                    <p className="settings-note">该通道暂未接入真实后端，仅保留配置界面。</p>
                  </>
                )}
              </div>

              {/* ── QR Code Dialog ── */}
              {qrDialogOpen && selectedBotId === 'wechat' ? (
                <div className="qr-dialog-overlay" onClick={() => { setQrDialogOpen(false); setBotLoading(false); }}>
                  <div className="qr-dialog" onClick={(event) => event.stopPropagation()}>
                    <div className="qr-dialog-header">
                      <strong>微信扫码登录</strong>
                      <button type="button" className="qr-dialog-close" onClick={() => { setQrDialogOpen(false); setBotLoading(false); }}>
                        &times;
                      </button>
                    </div>
                    <div className="qr-dialog-body">
                      {qrStatus === 'waiting' && !qrCodeUrl ? (
                        <div className="qr-loading">正在获取二维码...</div>
                      ) : qrStatus === 'waiting' && qrCodeUrl ? (
                        <>
                          <img className="qr-image" src={qrCodeUrl} alt="微信登录二维码" />
                          <p className="qr-hint">请使用微信扫描二维码</p>
                        </>
                      ) : qrStatus === 'scanned' ? (
                        <div className="qr-status scanned">
                          <AppIcon name="check" size={48} />
                          <p>已扫描，请在手机上确认</p>
                        </div>
                      ) : qrStatus === 'confirmed' ? (
                        <div className="qr-status confirmed">
                          <AppIcon name="check" size={48} />
                          <p>登录成功</p>
                        </div>
                      ) : (
                        <div className="qr-status error">
                          <p>二维码获取失败，请重试</p>
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}

          {tab === 'shortcuts' ? (
            <div className="shortcut-list">
              <div className="shortcut-config-card">
                <strong>发送消息方式</strong>
                <div className="shortcut-choice-row">
                  <button
                    type="button"
                    className={`shortcut-choice ${generalSettings.submitShortcut === 'enter' ? 'active' : ''}`}
                    onClick={() =>
                      setGeneralSettings((previous) => ({
                        ...previous,
                        submitShortcut: 'enter',
                      }))
                    }
                  >
                    <span>Enter 发送</span>
                    <code>Shift + Enter 换行</code>
                  </button>
                  <button
                    type="button"
                    className={`shortcut-choice ${generalSettings.submitShortcut === 'mod_enter' ? 'active' : ''}`}
                    onClick={() =>
                      setGeneralSettings((previous) => ({
                        ...previous,
                        submitShortcut: 'mod_enter',
                      }))
                    }
                  >
                    <span>Ctrl / Cmd + Enter 发送</span>
                    <code>Enter 换行</code>
                  </button>
                </div>
              </div>
              <div className="shortcut-row">
                <span>发送消息</span>
                <code>{getSubmitShortcutLabel(generalSettings.submitShortcut)}</code>
              </div>
              <div className="shortcut-row">
                <span>打开设置</span>
                <code>Cmd + ,</code>
              </div>
              <div className="shortcut-row">
                <span>切换技能页</span>
                <code>Cmd + 2</code>
              </div>
              <div className="shortcut-row">
                <span>停止当前生成</span>
                <code>Esc</code>
              </div>
            </div>
          ) : null}

          <div className="settings-footer">
            <button type="button" className="outline-button settings-footer-button" onClick={onClose}>
              关闭
            </button>
            <button type="button" className="primary-dark-button settings-footer-button" onClick={onClose}>
              完成
            </button>
          </div>
        </div>
      </div>
    </div>

      {providerDeleteConfirmId && pendingDeleteLabel ? (
        <div
          className="confirm-dialog-overlay"
          role="presentation"
          onClick={() => setProviderDeleteConfirmId(null)}
        >
          <div
            className="confirm-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="provider-delete-confirm-title"
            onClick={(event) => event.stopPropagation()}
          >
            <h3 id="provider-delete-confirm-title">删除 Provider</h3>
            <p>
              确定要移除「{pendingDeleteLabel}」吗？移除后需重新添加才能再次使用，请确认后再操作。
            </p>
            <div className="confirm-dialog-actions">
              <button type="button" className="outline-button" onClick={() => setProviderDeleteConfirmId(null)}>
                取消
              </button>
              <button
                type="button"
                className="outline-button confirm-dialog-delete"
                onClick={() => {
                  handleRemoveProvider(providerDeleteConfirmId)
                  setProviderDeleteConfirmId(null)
                }}
              >
                删除
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  )
}

type SettingsTabButtonProps = {
  active: boolean
  icon: IconName
  label: string
  onClick: () => void
}

function SettingsTabButton({ active, icon, label, onClick }: SettingsTabButtonProps) {
  return (
    <button type="button" className={`settings-tab-button ${active ? 'active' : ''}`} onClick={onClick}>
      <AppIcon name={icon} size={20} />
      <span>{label}</span>
    </button>
  )
}

type SettingSwitchProps = {
  checked: boolean
  description: string
  label: string
  onChange: () => void
}

function SettingSwitch({ checked, description, label, onChange }: SettingSwitchProps) {
  return (
    <div className="settings-row switch">
      <div>
        <strong>{label}</strong>
        <p>{description}</p>
      </div>
      <Toggle checked={checked} onChange={onChange} />
    </div>
  )
}

type ToggleProps = {
  checked: boolean
  onChange: () => void
}

function Toggle({ checked, onChange }: ToggleProps) {
  return (
    <button type="button" className={`toggle ${checked ? 'checked' : ''}`} onClick={onChange} aria-pressed={checked}>
      <span />
    </button>
  )
}

type IconName =
  | 'attachment'
  | 'bag'
  | 'book'
  | 'bot'
  | 'broadcast'
  | 'chevron-down'
  | 'clock'
  | 'close'
  | 'folder'
  | 'keyboard'
  | 'message'
  | 'more'
  | 'network'
  | 'panel'
  | 'plus'
  | 'plus-circle'
  | 'provider'
  | 'puzzle'
  | 'refresh'
  | 'search'
  | 'send'
  | 'settings'
  | 'spark'
  | 'sparkles'
  | 'stop'
  | 'trash'
  | 'wrench'
  | 'upload'
  | 'qr'
  | 'check'

function AppIcon({ name, size = 20 }: { name: IconName; size?: number }) {
  const stroke = 1.8

  return (
    <svg
      aria-hidden="true"
      className="app-icon"
      fill="none"
      height={size}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      {name === 'plus' ? <path d="M12 5v14M5 12h14" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} /> : null}
      {name === 'spark' ? (
        <path d="m12 3 1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8L12 3Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
      {name === 'book' ? (
        <>
          <path d="M5 5.5C5 4.67 5.67 4 6.5 4H19v15H6.5A1.5 1.5 0 0 1 5 17.5v-12Z" stroke="currentColor" strokeWidth={stroke} />
          <path d="M9 4v15" stroke="currentColor" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'clock' ? (
        <>
          <circle cx="12" cy="12" r="8" stroke="currentColor" strokeWidth={stroke} />
          <path d="M12 8v4l3 2" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'network' ? (
        <>
          <circle cx="6" cy="12" r="2.2" stroke="currentColor" strokeWidth={stroke} />
          <circle cx="18" cy="7" r="2.2" stroke="currentColor" strokeWidth={stroke} />
          <circle cx="18" cy="17" r="2.2" stroke="currentColor" strokeWidth={stroke} />
          <path d="M8 11l7.6-3M8 13l7.6 3" stroke="currentColor" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'settings' ? (
        <>
          <circle cx="12" cy="12" r="3" stroke="currentColor" strokeWidth={stroke} />
          <path
            d="M19.4 15a1 1 0 0 0 .2 1.1l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1 1 0 0 0-1.1-.2 1 1 0 0 0-.6.9V20a2 2 0 1 1-4 0v-.2a1 1 0 0 0-.6-.9 1 1 0 0 0-1.1.2l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1 1 0 0 0 .2-1.1 1 1 0 0 0-.9-.6H4a2 2 0 1 1 0-4h.2a1 1 0 0 0 .9-.6 1 1 0 0 0-.2-1.1l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1 1 0 0 0 1.1.2h.1a1 1 0 0 0 .6-.9V4a2 2 0 1 1 4 0v.2a1 1 0 0 0 .6.9 1 1 0 0 0 1.1-.2l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1 1 0 0 0-.2 1.1v.1a1 1 0 0 0 .9.6H20a2 2 0 1 1 0 4h-.2a1 1 0 0 0-.9.6Z"
            stroke="currentColor"
            strokeLinejoin="round"
            strokeWidth={1.4}
          />
        </>
      ) : null}
      {name === 'panel' ? (
        <>
          <rect x="4" y="4" width="16" height="16" rx="2" stroke="currentColor" strokeWidth={stroke} />
          <path d="M11 4v16M15 10l-2 2 2 2" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'more' ? (
        <>
          <circle cx="6" cy="12" r="1.5" fill="currentColor" />
          <circle cx="12" cy="12" r="1.5" fill="currentColor" />
          <circle cx="18" cy="12" r="1.5" fill="currentColor" />
        </>
      ) : null}
      {name === 'chevron-down' ? <path d="m6 9 6 6 6-6" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} /> : null}
      {name === 'bot' ? (
        <>
          <rect x="6" y="8" width="12" height="10" rx="3" stroke="currentColor" strokeWidth={stroke} />
          <path d="M12 4v4M9.5 13h.01M14.5 13h.01M8 18v2M16 18v2" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'attachment' ? <path d="M8.5 12.5 14 7a3 3 0 1 1 4.2 4.2l-6.8 6.8a5 5 0 1 1-7.1-7.1l7.1-7.1" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} /> : null}
      {name === 'send' ? <path d="m5 12 14-7-3 14-4.2-5.2L5 12Z" fill="currentColor" /> : null}
      {name === 'stop' ? <rect x="7" y="7" width="10" height="10" rx="2.4" fill="currentColor" /> : null}
      {name === 'search' ? (
        <>
          <circle cx="11" cy="11" r="5.5" stroke="currentColor" strokeWidth={stroke} />
          <path d="m16 16 3.5 3.5" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'refresh' ? (
        <path d="M20 11a8 8 0 1 0 2 5.3M20 4v5h-5" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
      {name === 'bag' ? (
        <>
          <path d="M6 8h12l-1 11H7L6 8Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={stroke} />
          <path d="M9 8a3 3 0 1 1 6 0" stroke="currentColor" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'plus-circle' ? (
        <>
          <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth={stroke} />
          <path d="M12 8v8M8 12h8" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'upload' ? (
        <>
          <path d="M12 16V6M8 10l4-4 4 4" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
          <path d="M5 18h14" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'folder' ? (
        <>
          <path d="M4 8h5l2 2h9v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={stroke} />
          <path d="M4 10h16" stroke="currentColor" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'puzzle' ? (
        <path d="M10 4h4v3a1.5 1.5 0 1 0 3 0V4h3v4a2 2 0 0 1-2 2h-3v3a1.5 1.5 0 1 1-3 0v-3H8a2 2 0 0 1-2-2V4h3a1.5 1.5 0 1 0 1 0Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
      {name === 'trash' ? (
        <>
          <path d="M5 7h14M9 7V5h6v2M8 10v7M12 10v7M16 10v7M7 7l1 12h8l1-12" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'close' ? <path d="m6 6 12 12M18 6 6 18" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} /> : null}
      {name === 'sparkles' ? (
        <>
          <path d="M6 4 7.4 7.6 11 9l-3.6 1.4L6 14l-1.4-3.6L1 9l3.6-1.4L6 4ZM18 9l1.1 2.9L22 13l-2.9 1.1L18 17l-1.1-2.9L14 13l2.9-1.1L18 9ZM16 2l.7 1.8L18.5 4.5l-1.8.7L16 7l-.7-1.8-1.8-.7 1.8-.7L16 2Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={1.4} />
        </>
      ) : null}
      {name === 'message' ? (
        <path d="M5 6.5A2.5 2.5 0 0 1 7.5 4H18a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H11l-4.5 4v-4H7.5A2.5 2.5 0 0 1 5 12.5v-6Z" stroke="currentColor" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
      {name === 'keyboard' ? (
        <>
          <rect x="3" y="6" width="18" height="12" rx="2.5" stroke="currentColor" strokeWidth={stroke} />
          <path d="M6.5 10h.01M10 10h.01M13.5 10h.01M17 10h.01M7 14h10" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'provider' ? (
        <>
          <rect x="4" y="5" width="16" height="5" rx="1.5" stroke="currentColor" strokeWidth={stroke} />
          <rect x="4" y="14" width="16" height="5" rx="1.5" stroke="currentColor" strokeWidth={stroke} />
          <path d="M8 10v4M16 10v4" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'broadcast' ? (
        <>
          <path d="M12 18a6 6 0 0 0 0-12M12 14a2 2 0 0 0 0-4M5 12a9 9 0 0 1 3-6.7M19 12a9 9 0 0 0-3-6.7" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
          <circle cx="12" cy="12" r="1.6" fill="currentColor" />
        </>
      ) : null}
      {name === 'wrench' ? (
        <path
          d="M14.5 6.5a4 4 0 0 0 2.8 5.6l-6.9 6.9a2 2 0 1 1-2.8-2.8l6.9-6.9a4 4 0 0 1-5.6-2.8l2.4-2.4 2.6.6.6 2.6 2.4 2.4-.4.4"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={stroke}
        />
      ) : null}
      {name === 'qr' ? (
        <>
          <rect x="5" y="5" width="5" height="5" rx="1" stroke="currentColor" strokeWidth={stroke} />
          <rect x="14" y="5" width="5" height="5" rx="1" stroke="currentColor" strokeWidth={stroke} />
          <rect x="5" y="14" width="5" height="5" rx="1" stroke="currentColor" strokeWidth={stroke} />
          <rect x="14" y="14" width="3" height="3" rx="0.5" stroke="currentColor" strokeWidth={stroke} />
          <path d="M17 14v-1M17 19h-1" stroke="currentColor" strokeLinecap="round" strokeWidth={stroke} />
        </>
      ) : null}
      {name === 'check' ? (
        <path d="M5 13 9 17 19 7" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={stroke} />
      ) : null}
    </svg>
  )
}

export default App
