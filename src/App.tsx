import type { Dispatch, SetStateAction } from 'react'
import { lazy, Suspense, startTransition, useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from 'react'
import { Check, Copy } from 'lucide-react'
import './App.css'
import { AppIcon, type IconName } from './components/AppIcon'
import { ComposerAttachmentStrip } from './components/ComposerAttachmentStrip'
import { InlineMediaAttachmentList } from './components/InlineMediaAttachmentList'
import { PromptBubbleContent } from './components/PromptBubbleContent'
import { SettingsModal } from './components/SettingsModal'
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
import { useComposerAttachments } from './hooks/useComposerAttachments'
import { usePiAgent } from './hooks/usePiAgent'
import type {
  AgentExecutionMode,
  AgentBuilderDraft,
  AgentTaskListItem,
  AgentTaskUpdateInput,
  AgentHeartbeatConfig,
  AgentHeartbeatSchedule,
  AgentHeartbeatTask,
  AgentInput,
  AgentRecord,
  AgentWorkspaceBundle,
  AgentWorkspaceFile,
  AppearanceSettings,
  BotChannelId,
  BotConfig,
  ConversationAgentSnapshot,
  ConversationTurn,
  GeneralSettings,
  HistoryItem,
  HistoryStatus,
  ProviderConfig,
  ProviderDefinition,
  ProviderId,
  ProviderRuntimeConfig,
  PersistedChatAttachment,
  CustomProviderMeta,
  ProviderApiFormat,
  PeerGatewayInfo,
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
import type { CSSProperties, ChangeEvent, ClipboardEvent, KeyboardEvent, MouseEvent, PointerEvent as ReactPointerEvent, RefObject } from 'react'
import {
  deleteAgent,
  botSendMedia,
  botSendMessage,
  botLoginWechat,
  botStartLark,
  botStartWechat,
  botStopLark,
  botStopWechat,
  createAgent,
  deleteAgentTask,
  getDefaultAgent,
  getPeerGatewayInfo,
  installSystemSkill,
  listAgentTasks,
  listInstalledSkills,
  listAgents,
  loadProviderPreferences,
  listSystemSkillCatalog,
  readAgentWorkspaceBundle,
  openExternalUrl,
  rotateAgentPeerInboundSecret,
  runAgentTaskNow,
  resumeAgentTask,
  saveProviderPreferences,
  setDefaultAgent,
  pauseAgentTask,
  updateAgentTask,
  subscribeQrCode,
  subscribeBotStatus,
  updateAgent,
  writeAgentWorkspaceFile,
} from './lib/piClient'
import type { QrCodeEvent, BotStatusEvent } from './lib/piClient'
import { buildPromptWithAttachments } from './lib/composerAttachments'
import { extractInlineMediaAttachments, normalizeMarkdownImageSources } from './lib/inlineMedia'
import { resolveReplyCardItems } from './lib/replyCardFormat'
import { THEME_PRESETS, THEME_VARIABLE_KEYS } from './theme/themePresets'
import ReplyCardStack from './components/ReplyCardStack'

const ABSOLUTE_TIME_FORMATTER = new Intl.DateTimeFormat('zh-CN', {
  year: 'numeric',
  month: 'numeric',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
})

const STARTER_CHIPS = ['定时会话'] as const
const DEFAULT_COMPOSER_HEIGHT = 96
const MIN_COMPOSER_HEIGHT = 56
const MAX_COMPOSER_HEIGHT = 360
const GENERAL_SETTINGS_STORAGE_KEY = 'nineclaw.general-settings.v1'
const APPEARANCE_SETTINGS_STORAGE_KEY = 'nineclaw.appearance-settings.v1'
const PROVIDER_CONFIGS_STORAGE_KEY = 'nineclaw.provider-configs.v1'
const CUSTOM_PROVIDERS_META_KEY = 'nineclaw.custom-providers-meta.v1'
const LEGACY_GENERAL_SETTINGS_STORAGE_KEYS = ['yqagent.general-settings.v1']
const LEGACY_APPEARANCE_SETTINGS_STORAGE_KEYS = ['yqagent.appearance-settings.v1']
const LEGACY_PROVIDER_CONFIGS_STORAGE_KEYS = ['yqagent.provider-configs.v1']
const LEGACY_CUSTOM_PROVIDERS_META_KEYS = ['yqagent.custom-providers-meta.v1']
/** 与 TurnResponseBody 空闲占位一致；不应作为一条「回复」展示在列表中。 */
const TURN_PLACEHOLDER_NO_OUTPUT = '当前轮次还没有输出内容。'
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

function clampNumber(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max)
}

function toBotSendMediaType(kind: PersistedChatAttachment['kind']): 'image' | 'file' | 'video' {
  if (kind === 'image') {
    return 'image'
  }
  if (kind === 'video') {
    return 'video'
  }
  return 'file'
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

function createInitialGeneralSettings() {
  return loadStoredState(GENERAL_SETTINGS_STORAGE_KEY, defaultGeneralSettings, LEGACY_GENERAL_SETTINGS_STORAGE_KEYS)
}

function createInitialAppearanceState() {
  return loadStoredState(
    APPEARANCE_SETTINGS_STORAGE_KEY,
    defaultAppearanceSettings,
    LEGACY_APPEARANCE_SETTINGS_STORAGE_KEYS,
  )
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

function normalizeProviderApiFormat(value: string | undefined, fallback: ProviderApiFormat = 'openai'): ProviderApiFormat {
  return value === 'anthropic' ? 'anthropic' : fallback
}

function createInitialProviderState() {
  return loadProviderConfigs()
}

function parseStoredProviderConfigs(raw: string | null | undefined): Record<string, ProviderConfig> | null {
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

function parseStoredCustomProviderMeta(raw: string | null | undefined): CustomProviderMeta[] | null {
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

function summarizePrompt(prompt: string, maxLength = 20): string {
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

function formatAgentTaskStatus(status: string): string {
  switch (status) {
    case 'active':
      return '运行中'
    case 'paused':
      return '已暂停'
    case 'deleted':
      return '已删除'
    case 'draft':
      return '待补充'
    default:
      return status || '未知'
  }
}

function formatAgentTaskSchedule(task: AgentTaskListItem): string {
  if (task.scheduleType === 'interval') {
    return task.nextRunAt
      ? `间隔任务 · 下次 ${formatOptionalAbsoluteTime(task.nextRunAt)}`
      : '间隔任务'
  }
  return task.nextRunAt
    ? `每日定时 · 下次 ${formatOptionalAbsoluteTime(task.nextRunAt)}`
    : '每日定时'
}

function formatAgentTaskScheduleShort(task: AgentTaskListItem): string {
  if (task.scheduleType === 'interval') {
    const m = task.intervalMinutes
    if (typeof m === 'number' && m > 0) {
      return `每 ${m} 分钟`
    }
    return '间隔执行'
  }
  if (task.dailyTimes.length > 0) {
    return `每日 ${task.dailyTimes.join('、')}`
  }
  return '每日定时'
}

const TASK_STATUS_GROUP_ORDER = ['active', 'paused', 'draft', 'deleted'] as const

const TASK_STATUS_GROUP_SET = new Set<string>(TASK_STATUS_GROUP_ORDER)

function groupAgentTasksByStatus(taskItems: AgentTaskListItem[]) {
  const bucket = new Map<string, AgentTaskListItem[]>()
  for (const key of TASK_STATUS_GROUP_ORDER) {
    bucket.set(key, [])
  }
  bucket.set('other', [])
  for (const task of taskItems) {
    const k = TASK_STATUS_GROUP_SET.has(task.status) ? task.status : 'other'
    bucket.get(k)?.push(task)
  }
  const sections: { key: string; label: string; items: AgentTaskListItem[] }[] = []
  for (const key of TASK_STATUS_GROUP_ORDER) {
    const items = bucket.get(key) ?? []
    if (items.length > 0) {
      sections.push({
        key,
        label:
          key === 'active'
            ? '运行中'
            : key === 'paused'
              ? '已暂停'
              : key === 'draft'
                ? '待补充'
                : '已删除',
        items,
      })
    }
  }
  const other = bucket.get('other') ?? []
  if (other.length > 0) {
    sections.push({ key: 'other', label: '其他', items: other })
  }
  return sections
}

function parseDailyTimesInput(value: string): string[] {
  return value
    .split(/[\s,，、]+/)
    .map((item) => item.trim())
    .filter(Boolean)
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

function createAgentBotConfigState(configs?: Record<string, BotConfig>): Record<string, BotConfig> {
  const defaults = createInitialBotConfigs()
  if (!configs) {
    return defaults
  }

  const next = { ...defaults }
  for (const key of Object.keys(defaults)) {
    const current = configs[key]
    if (current) {
      next[key] = { ...defaults[key], ...current }
    }
  }
  return next
}

function getBotChannelRuntimeId(agentId: string, channelId: BotChannelId): string {
  return `${channelId}:${agentId}`
}

function resolveBotChannelFromRuntimeId(agentId: string, runtimeChannelId: string): BotChannelId | null {
  for (const channel of botDefinitions) {
    if (runtimeChannelId === getBotChannelRuntimeId(agentId, channel.id)) {
      return channel.id
    }
  }
  return null
}

function buildBotConfigStatusPatch(event: BotStatusEvent): Partial<BotConfig> | null {
  switch (event.level) {
    case 'processing':
      return { status: '登录中', errorMessage: undefined }
    case 'done':
      return { status: '已连接', enabled: true, errorMessage: undefined }
    case 'error':
      return { status: '错误', enabled: false, errorMessage: event.message }
    case 'warn':
      if (event.message.includes('已关闭') || event.message.includes('已停止') || event.message.includes('已退出')) {
        return { status: '未连接', enabled: false, imChannelPaused: true }
      }
      return null
    default:
      return null
  }
}

function buildBotRuntimeBindingConfig(runtime: ProviderRuntimeConfig): Partial<BotConfig> {
  return {
    aiProviderId: runtime.providerId,
    aiApiFormat: runtime.apiFormat,
    aiBaseUrl: runtime.baseUrl,
    aiApiKey: runtime.apiKey,
    aiModel: runtime.model,
  }
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

function generateDraftItemId(prefix: string): string {
  const uuid =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID().replace(/-/g, '')
      : `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`
  return `${prefix}_${uuid}`
}

function createEmptyHeartbeatConfig(): AgentHeartbeatConfig {
  return {
    timezone: 'Asia/Shanghai',
    tasks: [],
    schedules: [],
  }
}

function createEmptyHeartbeatTask(): AgentHeartbeatTask {
  return {
    id: generateDraftItemId('task'),
    name: '',
    description: '',
    taskType: 'notify',
    enabled: true,
    messageTemplate: '',
    command: '',
    workingDirectory: '',
    timeoutSec: 180,
    notifyOnSuccess: true,
    notifyOnFailure: true,
  }
}

function createEmptyHeartbeatSchedule(): AgentHeartbeatSchedule {
  return {
    id: generateDraftItemId('schedule'),
    name: '',
    enabled: true,
    taskId: '',
    scheduleType: 'daily',
    times: ['08:00'],
    channelId: 'wechat',
    targetUserId: '',
    targetLabel: '',
  }
}

function normalizeHeartbeatTimes(times: string[]): string[] {
  return Array.from(
    new Set(
      times
        .flatMap((value) => value.split(','))
        .map((value) => value.trim())
        .filter(Boolean),
    ),
  )
}

function normalizeHeartbeatConfig(config?: AgentHeartbeatConfig | null): AgentHeartbeatConfig {
  const source = config ?? createEmptyHeartbeatConfig()
  return {
    timezone: source.timezone.trim() || 'Asia/Shanghai',
    tasks: source.tasks.map((task) => ({
      id: task.id || generateDraftItemId('task'),
      name: task.name.trim(),
      description: task.description.trim(),
      taskType: task.taskType === 'shell' ? 'shell' : 'notify',
      enabled: task.enabled !== false,
      messageTemplate: task.messageTemplate.trim(),
      command: task.command.trim(),
      workingDirectory: task.workingDirectory.trim(),
      timeoutSec: Number.isFinite(task.timeoutSec) ? Math.max(10, Math.trunc(task.timeoutSec)) : 180,
      notifyOnSuccess: task.notifyOnSuccess !== false,
      notifyOnFailure: task.notifyOnFailure !== false,
    })),
    schedules: source.schedules.map((schedule) => ({
      id: schedule.id || generateDraftItemId('schedule'),
      name: schedule.name.trim(),
      enabled: schedule.enabled !== false,
      taskId: schedule.taskId.trim(),
      scheduleType: 'daily',
      times: normalizeHeartbeatTimes(schedule.times),
      channelId: schedule.channelId.trim() || 'wechat',
      targetUserId: schedule.targetUserId.trim(),
      targetLabel: schedule.targetLabel.trim(),
    })),
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
    botConfigs: createAgentBotConfigState(),
    heartbeatConfig: createEmptyHeartbeatConfig(),
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
    botConfigs: createAgentBotConfigState(agent.botConfigs),
    heartbeatConfig: normalizeHeartbeatConfig(agent.heartbeatConfig),
    ...(agent.collaborationConfig ? { collaborationConfig: agent.collaborationConfig } : {}),
    ...(agent.accentColor ? { accentColor: agent.accentColor } : {}),
  }
}

function normalizeInlineText(value: string): string {
  return value.replace(/\s+/g, ' ').trim()
}

const AUTO_AGENT_SUMMARY_MAX_CHARS = 36

function buildAutoAgentSummary(description: string, fallbackName = ''): string {
  const normalizedDescription = normalizeInlineText(description)
  const normalizedFallback = normalizeInlineText(fallbackName)
  const source = normalizedDescription || normalizedFallback
  if (!source) {
    return ''
  }
  const chars = Array.from(source)
  if (chars.length <= AUTO_AGENT_SUMMARY_MAX_CHARS) {
    return source
  }
  return `${chars.slice(0, AUTO_AGENT_SUMMARY_MAX_CHARS - 1).join('')}…`
}

function normalizeAgentDraft(input: AgentInput): AgentInput {
  const name = input.name.trim()
  const explicitSummary = input.summary.trim()
  const description = input.description.trim() || explicitSummary
  const summary = explicitSummary || buildAutoAgentSummary(description, name)
  return {
    ...input,
    name,
    summary,
    description,
    systemPrompt: input.systemPrompt.trim(),
    defaultProviderId: input.defaultProviderId.trim(),
    defaultModel: input.defaultModel.trim(),
    skillIds: Array.from(new Set(input.skillIds.map((item) => item.trim()).filter(Boolean))),
    botConfigs: createAgentBotConfigState(input.botConfigs),
    heartbeatConfig: normalizeHeartbeatConfig(input.heartbeatConfig),
  }
}

function validateAgentDraft(input: AgentInput): string | null {
  if (!input.name.trim()) {
    return '请输入智能体名称。'
  }
  if (!input.description.trim() && !input.summary.trim()) {
    return '请输入智能体角色说明。'
  }
  if (!input.defaultProviderId.trim() || !input.defaultModel.trim()) {
    return '请为智能体配置默认模型。'
  }
  const heartbeatConfig = normalizeHeartbeatConfig(input.heartbeatConfig)
  const taskIds = new Set(heartbeatConfig.tasks.map((task) => task.id))
  for (const task of heartbeatConfig.tasks) {
    if (task.enabled && task.taskType === 'shell' && !task.command.trim()) {
      return `任务「${task.name || '未命名任务'}」缺少执行命令。`
    }
  }
  for (const schedule of heartbeatConfig.schedules) {
    if (!schedule.enabled) {
      continue
    }
    if (!schedule.taskId || !taskIds.has(schedule.taskId)) {
      return `规则「${schedule.name || '未命名规则'}」需要绑定一个有效任务。`
    }
    if (schedule.times.length === 0) {
      return `规则「${schedule.name || '未命名规则'}」至少要配置一个触发时间。`
    }
    if (!schedule.targetUserId.trim()) {
      return `规则「${schedule.name || '未命名规则'}」缺少接收用户 ID。`
    }
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

function normalizeSkillDescription(description: string | null | undefined): string {
  const trimmed = description?.trim()
  return trimmed ? trimmed : '暂无技能说明。'
}

function shouldCollapseSkillDescription(description: string): boolean {
  return description.length > 96 || description.includes('\n')
}

type SkillDescriptionDisclosureProps = {
  description: string | null | undefined
  collapsedLines?: number
  className?: string
}

function SkillDescriptionDisclosure({
  description,
  collapsedLines = 2,
  className = '',
}: SkillDescriptionDisclosureProps) {
  const normalizedDescription = normalizeSkillDescription(description)
  const expandable = shouldCollapseSkillDescription(normalizedDescription)
  const [expanded, setExpanded] = useState(false)

  return (
    <div className={`skill-description-block ${expanded ? 'expanded' : ''} ${className}`.trim()}>
      <p
        className={`skill-description-text ${expanded ? 'expanded' : 'collapsed'}`}
        style={expanded ? undefined : { WebkitLineClamp: collapsedLines }}
      >
        {normalizedDescription}
      </p>
      {expandable ? (
        <button
          type="button"
          className={`skill-description-toggle ${expanded ? 'expanded' : ''}`}
          onClick={() => setExpanded((current) => !current)}
          aria-expanded={expanded}
        >
          <span>{expanded ? '收起介绍' : '展开介绍'}</span>
          <AppIcon name="chevron-down" size={14} />
        </button>
      ) : null}
    </div>
  )
}

function formatWorkspaceFileSectionLabel(section: AgentWorkspaceFile['section']): string {
  if (section === 'private') {
    return '私有文件'
  }
  if (section === 'memoryIndex') {
    return '记忆入口'
  }
  if (section === 'categoryMemory') {
    return '分类记忆'
  }
  if (section === 'wiki') {
    return '知识 Wiki'
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
    const normalizedDescription = description || summary
    const normalizedSummary = summary || buildAutoAgentSummary(normalizedDescription, name)
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

    if (!name || !normalizedDescription) {
      return null
    }

    return {
      name,
      summary: normalizedSummary,
      description: normalizedDescription,
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

function useLiveNow(enabled: boolean, intervalMs = 1000): number {
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (!enabled) {
      setNow(Date.now())
      return
    }

    setNow(Date.now())
    const timer = window.setInterval(() => {
      setNow(Date.now())
    }, intervalMs)

    return () => window.clearInterval(timer)
  }, [enabled, intervalMs])

  return now
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

function TokenUsageDetailPill({ usage, variant = 'pill' }: { usage: TokenUsage; variant?: 'pill' | 'inline' }) {
  const [open, setOpen] = useState(false)
  const containerRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!open) {
      return
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target
      if (!(target instanceof Node)) {
        return
      }
      if (!containerRef.current?.contains(target)) {
        setOpen(false)
      }
    }

    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(false)
      }
    }

    window.addEventListener('pointerdown', handlePointerDown)
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown)
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [open])

  const isInline = variant === 'inline'

  return (
    <div className={`answer-result-meta-popover${isInline ? ' answer-result-meta-popover-inline' : ''}`} ref={containerRef}>
      <button
        type="button"
        className={isInline ? 'answer-result-meta-button-inline' : 'answer-result-meta-item answer-result-meta-button'}
        onClick={() => setOpen((current) => !current)}
        aria-expanded={open}
      >
        总 Token：{formatTokenCount(usage.totalTokens)}
      </button>
      {open ? (
        <div className="answer-result-tooltip" role="dialog" aria-label="Token 详情">
          <div className="answer-result-tooltip-head">
            <strong>{usage.model?.trim() || '未标记模型'}</strong>
            {usage.provider || usage.api ? (
              <span>{[usage.provider, usage.api].filter(Boolean).join(' · ')}</span>
            ) : null}
          </div>
          <div className="answer-result-tooltip-grid">
            <span>model</span>
            <span>{usage.model?.trim() || '--'}</span>
            <span>input</span>
            <span>{formatTokenCount(usage.inputTokens)}</span>
            <span>output</span>
            <span>{formatTokenCount(usage.outputTokens)}</span>
            <span>cacheRead</span>
            <span>{formatTokenCount(usage.cacheReadTokens)}</span>
            <span>totalTokens</span>
            <span>{formatTokenCount(usage.totalTokens)}</span>
          </div>
        </div>
      ) : null}
    </div>
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

function hasProviderDefinition(providerId: ProviderId, definitions: ProviderDefinition[]): boolean {
  return definitions.some((item) => item.id === providerId)
}

function isProviderAvailable(
  providerId: ProviderId,
  definitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
): boolean {
  return hasProviderDefinition(providerId, definitions) && providerConfigs[providerId]?.added === true
}

function isValidConfiguredModelReference(
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

function pickFallbackSessionLlm(
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

function pickFallbackAgentModel(
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

function sanitizeAgentInputModelReference(
  draft: AgentInput,
  definitions: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
): AgentInput {
  if (
    isValidConfiguredModelReference(
      draft.defaultProviderId,
      draft.defaultModel,
      definitions,
      providerConfigs,
    )
  ) {
    return draft
  }

  const fallback = pickFallbackAgentModel(definitions, providerConfigs, draft.defaultProviderId)
  if (!fallback) {
    return draft
  }

  return {
    ...draft,
    defaultProviderId: fallback.providerId,
    defaultModel: fallback.model,
  }
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

function buildSessionLlmOptionsWithFallback(
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

function resolveRuntimeFromSessionFields(
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

function resolveRuntimeFromAgentSnapshot(
  agent: ConversationAgentSnapshot | null,
  providerConfigs: Record<string, ProviderConfig>,
): ProviderRuntimeConfig | null {
  if (!agent) {
    return null
  }

  return resolveRuntimeFromSessionFields(agent.defaultProviderId, agent.defaultModel, providerConfigs)
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

function App() {
  const {
    draft,
    setDraft,
    error,
    loading,
    runtimeReady,
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
    sanitizeSessionLlmReferences,
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
  const [agentRefreshing, setAgentRefreshing] = useState(false)
  const [agentFormError, setAgentFormError] = useState('')
  const [agentFormNotice, setAgentFormNotice] = useState('')
  const [agentBotBindingDialogOpen, setAgentBotBindingDialogOpen] = useState(false)
  const [agentDeleteConfirmOpen, setAgentDeleteConfirmOpen] = useState(false)
  const [agentDeleteConfirmText, setAgentDeleteConfirmText] = useState('')
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
  const [qrDialogOpen, setQrDialogOpen] = useState(false)
  const [qrCodeUrl, setQrCodeUrl] = useState<string>('')
  const [qrStatus, setQrStatus] = useState<'waiting' | 'scanned' | 'confirmed' | 'error'>('waiting')
  const [botLoading, setBotLoading] = useState(false)
  const [botStatusLog, setBotStatusLog] = useState<BotStatusEvent[]>([])
  /** 无选中会话时，输入区上方选择的模型（首条消息写入该会话） */
  const [composerSessionLlm, setComposerSessionLlm] = useState<{ providerId: ProviderId; model: string } | null>(null)
  const [composerAgent, setComposerAgent] = useState<ConversationAgentSnapshot | null>(null)
  const [chatGateError, setChatGateError] = useState('')
  const providerCleanupInFlightRef = useRef<Set<string>>(new Set())
  const providerPreferencesHydratedRef = useRef(false)

  const deferredSkillSearch = useDeferredValue(skillSearch.trim().toLowerCase())
  const deferredResourceSearch = useDeferredValue(resourceSearch.trim().toLowerCase())
  const deferredAgentSearch = useDeferredValue(agentSearch.trim().toLowerCase())
  const deferredHistorySearch = useDeferredValue(historySearch.trim().toLowerCase())

  const mergedProviderDefinitions = useMemo((): ProviderDefinition[] => {
    const custom = customProviderMeta.map((meta) => {
      const effectiveApiFormat = providerConfigs[meta.id]?.apiFormat ?? meta.apiFormat
      return {
        id: meta.id,
        name: meta.name,
        defaultBaseUrl: effectiveApiFormat === 'anthropic' ? 'https://api.anthropic.com' : 'https://api.openai.com/v1',
        suggestedModel: effectiveApiFormat === 'anthropic' ? 'claude-sonnet-4-0' : 'gpt-4o-mini',
        description: meta.description || `自定义 ${effectiveApiFormat === 'anthropic' ? 'Anthropic' : 'OpenAI'} 兼容接口。`,
        apiFormat: effectiveApiFormat,
        isCustom: true,
      }
    })
    return [...providerDefinitions, ...custom]
  }, [customProviderMeta, providerConfigs])
  const allProviderIds = useMemo(() => mergedProviderDefinitions.map((p) => p.id), [mergedProviderDefinitions])
  const firstAddedProviderId = useMemo(
    () => mergedProviderDefinitions.find((item) => providerConfigs[item.id]?.added)?.id ?? '',
    [mergedProviderDefinitions, providerConfigs],
  )
  const selectedProviderDefinition =
    mergedProviderDefinitions.find((item) => item.id === selectedProviderId) ?? providerDefinitions[0]
  const selectedProviderConfig = providerConfigs[selectedProviderId] ?? emptyProviderConfig()
  const activeProviderConfig = resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
  const activeProviderDefinition = activeProviderConfig
    ? mergedProviderDefinitions.find((item) => item.id === activeProviderConfig.providerId) ?? null
    : null
  const selectedBotDefinition = botDefinitions.find((item) => item.id === selectedBotId) ?? botDefinitions[0]
  const defaultAgent = useMemo(
    () => agents.find((item) => item.id === defaultAgentId) ?? agents[0] ?? null,
    [agents, defaultAgentId],
  )
  const editableAgents = useMemo(() => agents.filter((item) => !item.isBuiltin), [agents])
  const selectedManagedAgent = useMemo(
    () => editableAgents.find((item) => item.id === managedAgentId) ?? null,
    [editableAgents, managedAgentId],
  )
  const selectedManagedBotConfigs = useMemo(
    () => createAgentBotConfigState(agentEditorDraft?.botConfigs),
    [agentEditorDraft?.botConfigs],
  )
  const selectedManagedBotConfig = selectedManagedBotConfigs[selectedBotId] ?? createInitialBotConfigs()[selectedBotId]
  const preferredComposerAgent = useMemo(
    () => composerAgent ?? (defaultAgent ? buildConversationAgentSnapshot(defaultAgent) : null),
    [composerAgent, defaultAgent],
  )
  const activeChatAgent = activeHistoryItem ? activeHistoryItem.agent ?? null : preferredComposerAgent
  const composerAttachmentScopeKey = `${activeHistoryId || 'composer'}:${activeChatAgent?.id ?? 'no-agent'}`
  const {
    attachments: composerAttachments,
    uploading: composerAttachmentUploading,
    error: composerAttachmentError,
    fileInputRef: composerAttachmentInputRef,
    openFilePicker: openComposerAttachmentPicker,
    handleFileInputChange: handleComposerAttachmentInputChange,
    handleComposerPaste,
    removeAttachment: removeComposerAttachment,
    clearAttachments: clearComposerAttachments,
    clearError: clearComposerAttachmentError,
  } = useComposerAttachments({
    agentId: activeChatAgent?.id ?? '',
    sessionId: activeHistoryId || null,
    scopeKey: composerAttachmentScopeKey,
  })
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
  const effectiveSidebarCollapsed = !shouldHideSidebar && appearanceSettings.sidebarCollapsed
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
      const resolved = resolveRuntimeFromSessionFields(
        activeHistoryItem.sessionLlmProviderId,
        activeHistoryItem.sessionLlmModel,
        providerConfigs,
      )
      if (resolved) {
        return {
          providerId: resolved.providerId,
          model: resolved.model,
        }
      }
    }
    if (activeHistoryItem) {
      const resolvedFromAgent = resolveRuntimeFromAgentSnapshot(activeHistoryItem.agent ?? null, providerConfigs)
      if (resolvedFromAgent) {
        return {
          providerId: resolvedFromAgent.providerId,
          model: resolvedFromAgent.model,
        }
      }
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
      const resolved = resolveRuntimeFromSessionFields(composerSessionLlm.providerId, composerSessionLlm.model, providerConfigs)
      if (resolved) {
        return { providerId: resolved.providerId, model: resolved.model }
      }
    }
    if (preferredComposerAgent?.defaultProviderId && preferredComposerAgent.defaultModel.trim()) {
      const resolved = resolveRuntimeFromAgentSnapshot(preferredComposerAgent, providerConfigs)
      if (resolved) {
        return {
          providerId: resolved.providerId,
          model: resolved.model,
        }
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
    ? Boolean(
        (activeHistoryItem.sessionLlmProviderId && activeHistoryItem.sessionLlmModel?.trim()) ||
          (activeHistoryItem.agent?.defaultProviderId && activeHistoryItem.agent.defaultModel?.trim()),
      )
    : Boolean(composerSessionLlm || (preferredComposerAgent?.defaultProviderId && preferredComposerAgent.defaultModel.trim()))
  const runtimeResolutionError =
    !effectiveChatRuntime && requiresConfiguredSessionModel
      ? '当前会话绑定的模型尚未在设置里完成配置，请先补全对应供应商的 Base URL、API Key 和模型，或切换到已配置模型。'
      : !effectiveChatRuntime
      ? '当前没有已启用的 Provider。请在设置中启用至少一个 Provider，或为当前会话/智能体绑定一个已配置模型。'
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
    if (mergedProviderDefinitions.length === 0) {
      return
    }

    if (
      !isProviderAvailable(selectedProviderId, mergedProviderDefinitions, providerConfigs) &&
      firstAddedProviderId &&
      selectedProviderId !== firstAddedProviderId
    ) {
      setSelectedProviderId(firstAddedProviderId)
      return
    }

    if (!hasProviderDefinition(selectedProviderId, mergedProviderDefinitions)) {
      setSelectedProviderId(firstAddedProviderId || mergedProviderDefinitions[0]?.id || 'openai')
    }

    sanitizeSessionLlmReferences((providerId, model) =>
      isValidConfiguredModelReference(providerId, model, mergedProviderDefinitions, providerConfigs),
    )

    setComposerSessionLlm((current) => {
      if (!current) {
        return current
      }
      if (isValidConfiguredModelReference(current.providerId, current.model, mergedProviderDefinitions, providerConfigs)) {
        return current
      }
      return pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, current.providerId)
    })

    setNewSessionLlm((current) => {
      if (!current) {
        return current
      }
      if (isValidConfiguredModelReference(current.providerId, current.model, mergedProviderDefinitions, providerConfigs)) {
        return current
      }
      return pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, current.providerId)
    })

    setAgentEditorDraft((current) => {
      if (!current) {
        return current
      }
      const next = sanitizeAgentInputModelReference(current, mergedProviderDefinitions, providerConfigs)
      return next.defaultProviderId === current.defaultProviderId && next.defaultModel === current.defaultModel
        ? current
        : next
    })

    setAgents((previous) => {
      let changed = false
      const next = previous.map((agent) => {
        const sanitized = sanitizeAgentInputModelReference(
          createAgentDraftFromRecord(agent),
          mergedProviderDefinitions,
          providerConfigs,
        )
        if (
          sanitized.defaultProviderId === agent.defaultProviderId &&
          sanitized.defaultModel === agent.defaultModel
        ) {
          return agent
        }
        changed = true
        return {
          ...agent,
          defaultProviderId: sanitized.defaultProviderId,
          defaultModel: sanitized.defaultModel,
        }
      })
      return changed ? next : previous
    })

    for (const agent of agents) {
      const sanitized = sanitizeAgentInputModelReference(
        createAgentDraftFromRecord(agent),
        mergedProviderDefinitions,
        providerConfigs,
      )
      if (
        sanitized.defaultProviderId === agent.defaultProviderId &&
        sanitized.defaultModel === agent.defaultModel
      ) {
        continue
      }
      if (providerCleanupInFlightRef.current.has(agent.id)) {
        continue
      }

      providerCleanupInFlightRef.current.add(agent.id)
      void updateAgent(agent.id, sanitized)
        .then((savedAgent) => {
          setAgents((previous) => previous.map((item) => (item.id === savedAgent.id ? savedAgent : item)))
          setAgentEditorDraft((current) => {
            if (!current || managedAgentId !== savedAgent.id) {
              return current
            }
            return createAgentDraftFromRecord(savedAgent)
          })
        })
        .catch((cleanupError) => {
          const message = cleanupError instanceof Error ? cleanupError.message : String(cleanupError)
          setAgentFormError((current) =>
            current || `自动清理已删除 Provider 的智能体模型引用失败：${message}`,
          )
        })
        .finally(() => {
          providerCleanupInFlightRef.current.delete(agent.id)
        })
    }
  }, [
    agents,
    managedAgentId,
    firstAddedProviderId,
    mergedProviderDefinitions,
    providerConfigs,
    sanitizeSessionLlmReferences,
    selectedProviderId,
  ])

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
    const root = document.documentElement
    const preset = THEME_PRESETS[appearanceSettings.themeMode] ?? THEME_PRESETS.dark

    root.dataset.uiTheme = appearanceSettings.themeMode
    document.body.dataset.uiTheme = appearanceSettings.themeMode
    root.style.setProperty('color-scheme', preset.colorScheme)

    for (const variableName of THEME_VARIABLE_KEYS) {
      const variableValue = preset.variables[variableName]

      if (variableValue) {
        root.style.setProperty(variableName, variableValue)
      } else {
        root.style.removeProperty(variableName)
      }
    }
  }, [appearanceSettings.themeMode])

  useEffect(() => {
    let cancelled = false

    void loadProviderPreferences()
      .then((payload) => {
        if (cancelled) {
          return
        }

        const backendProviderConfigs = parseStoredProviderConfigs(payload.providerConfigs)
        const backendCustomProviderMeta = parseStoredCustomProviderMeta(payload.customProviderMeta)

        if (backendProviderConfigs) {
          setProviderConfigs(backendProviderConfigs)
        }
        if (backendCustomProviderMeta) {
          setCustomProviderMeta(backendCustomProviderMeta)
        }

        const hasBackendState = Boolean(payload.providerConfigs || payload.customProviderMeta)
        providerPreferencesHydratedRef.current = true

        if (!hasBackendState) {
          void saveProviderPreferences({
            providerConfigsPayload: JSON.stringify(providerConfigs),
            customProviderMetaPayload: JSON.stringify(customProviderMeta),
          }).catch((error) => {
            console.warn('NineClaw: migrate provider preferences failed', error)
          })
        }
      })
      .catch((error) => {
        providerPreferencesHydratedRef.current = true
        console.warn('NineClaw: load provider preferences failed', error)
      })

    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    persistStoredStorageValue(
      PROVIDER_CONFIGS_STORAGE_KEY,
      JSON.stringify(providerConfigs),
      LEGACY_PROVIDER_CONFIGS_STORAGE_KEYS,
    )
    if (!providerPreferencesHydratedRef.current) {
      return
    }
    void saveProviderPreferences({
      providerConfigsPayload: JSON.stringify(providerConfigs),
      customProviderMetaPayload: JSON.stringify(customProviderMeta),
    }).catch((error) => {
      console.warn('NineClaw: save provider preferences failed', error)
    })
  }, [customProviderMeta, providerConfigs])

  useEffect(() => {
    persistStoredStorageValue(
      CUSTOM_PROVIDERS_META_KEY,
      JSON.stringify(customProviderMeta),
      LEGACY_CUSTOM_PROVIDERS_META_KEYS,
    )
  }, [customProviderMeta])

  // ── Bot status log (diagnostics) ──
  useEffect(() => {
    let unsub: (() => void) | undefined
    void subscribeBotStatus((event) => {
      setBotStatusLog((prev) => [event, ...prev].slice(0, 20))
      if (!selectedManagedAgent) {
        return
      }

      const channelId = resolveBotChannelFromRuntimeId(selectedManagedAgent.id, event.channelId)
      const statusPatch = channelId ? buildBotConfigStatusPatch(event) : null
      if (!channelId || !statusPatch) {
        return
      }

      setAgentEditorDraft((current) => {
        if (!current) {
          return current
        }
        const currentConfigs = createAgentBotConfigState(current.botConfigs)
        return {
          ...current,
          botConfigs: {
            ...currentConfigs,
            [channelId]: {
              ...currentConfigs[channelId],
              ...statusPatch,
            },
          },
        }
      })
    }).then((unlisten) => {
      unsub = unlisten
    })
    return () => unsub?.()
  }, [selectedManagedAgent])

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
      return true
    } catch (loadError) {
      const message = loadError instanceof Error ? loadError.message : String(loadError)
      setAgentsError(message)
      return false
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
      botConfigs: createAgentBotConfigState(draft.botConfigs),
      heartbeatConfig: normalizeHeartbeatConfig(draft.heartbeatConfig),
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
      setAgentEditorOpen(true)
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
      setNewSessionLlm(
        isValidConfiguredModelReference(
          seedAgent.defaultProviderId,
          seedAgent.defaultModel,
          mergedProviderDefinitions,
          providerConfigs,
        )
          ? {
              providerId: seedAgent.defaultProviderId,
              model: seedAgent.defaultModel,
            }
          : pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, seedAgent.defaultProviderId),
      )
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
    if (composerAttachmentUploading) {
      setChatGateError('附件仍在导入中，请稍等片刻再发送。')
      return
    }

    const botTarget = activeHistoryItem?.botTarget ?? null
    const hasDirectBotAttachments = Boolean(botTarget && composerAttachments.length > 0)

    if (hasDirectBotAttachments && botTarget) {
      if (chatGateError) {
        setChatGateError('')
      }

      try {
        const trimmedDraft = draft.trim()
        if (trimmedDraft) {
          await botSendMessage(botTarget.channelId, botTarget.userId, trimmedDraft)
        }

        for (const attachment of composerAttachments) {
          await botSendMedia(
            botTarget.channelId,
            botTarget.userId,
            toBotSendMediaType(attachment.kind),
            attachment.filePath,
          )
        }

        setDraft('')
        clearComposerAttachments()
      } catch (sendError) {
        const message = sendError instanceof Error ? sendError.message : String(sendError)
        setChatGateError(message)
      }
      return
    }

    if (runtimeResolutionError) {
      setChatGateError(runtimeResolutionError)
      return
    }
    if (chatGateError) {
      setChatGateError('')
    }
    const promptWithAttachments = buildPromptWithAttachments(draft, composerAttachments)
    clearComposerAttachments()
    await submitPrompt(promptWithAttachments, {
      providerConfig: effectiveChatRuntime,
      agent: activeHistoryItem ? activeHistoryItem.agent ?? null : preferredComposerAgent,
      sessionLlm: sessionLlmDisplay,
      attachments: composerAttachments,
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
    setAgentFormError('')
    setAgentFormNotice('')
    setAgentBotBindingDialogOpen(false)
    setAgentDeleteConfirmOpen(false)
    setAgentDeleteConfirmText('')
    handleCloseAgentSkillPicker()
  }

  const handleOpenAgentEditor = (agentId: string) => {
    const targetAgent = editableAgents.find((item) => item.id === agentId)
    if (!targetAgent) {
      return
    }
    handleManagedAgentSelect(agentId)
    setAgentEditorOpen(true)
  }

  const handleOpenAgentBotBinding = (agentId: string) => {
    const targetAgent = editableAgents.find((item) => item.id === agentId)
    if (!targetAgent) {
      return
    }
    handleManagedAgentSelect(agentId)
    setAgentEditorOpen(false)
    setAgentBotBindingDialogOpen(true)
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
    setAgentDeleteConfirmOpen(false)
    setAgentDeleteConfirmText('')
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
    setAgentDeleteConfirmOpen(false)
    setAgentDeleteConfirmText('')
    setAgentWorkspaceDialogOpen(false)
    setAgentWorkspaceDraftContent('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')
    handleCloseAgentSkillPicker()
    setAgentFormError('')
    setAgentFormNotice('')
  }

  const handleCloseAgentBotBindingDialog = () => {
    setAgentBotBindingDialogOpen(false)
    setQrDialogOpen(false)
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

  const handleRefreshCurrentAgent = async () => {
    const preferredAgentId = selectedManagedAgent?.id || managedAgentId || undefined

    setAgentRefreshing(true)
    setAgentFormError('')
    if (preferredAgentId) {
      setAgentFormNotice('正在刷新最新配置…')
    }

    try {
      const refreshed = await refreshAgents(preferredAgentId)
      if (preferredAgentId && refreshed) {
        setAgentFormNotice('已刷新当前智能体配置。')
      } else if (!refreshed) {
        setAgentFormNotice('')
      }
    } catch (refreshError) {
      const message = refreshError instanceof Error ? refreshError.message : String(refreshError)
      setAgentFormError(message)
    } finally {
      setAgentRefreshing(false)
    }
  }

  const handleRequestDeleteCurrentAgent = () => {
    if (!selectedManagedAgent || agentSaving) {
      return
    }
    setAgentDeleteConfirmText('')
    setAgentDeleteConfirmOpen(true)
  }

  const handleCloseDeleteAgentDialog = () => {
    if (agentSaving) {
      return
    }
    setAgentDeleteConfirmOpen(false)
    setAgentDeleteConfirmText('')
  }

  const handleConfirmDeleteCurrentAgent = async () => {
    if (!selectedManagedAgent || agentSaving) {
      return
    }
    if (agentDeleteConfirmText.trim() !== '确认删除') {
      setAgentFormError('请输入“确认删除”后再删除智能体。')
      return
    }

    setAgentSaving(true)
    setAgentFormError('')
    setAgentFormNotice('')

    try {
      await deleteAgent(selectedManagedAgent.id)
      setAgentDeleteConfirmOpen(false)
      setAgentDeleteConfirmText('')
      setAgentEditorMode('edit')
      setAgentEditorDraft(null)
      setAgentEditorOpen(false)
      handleCloseAgentSkillPicker()
      await refreshAgents()
    } catch (deleteError) {
      const message = deleteError instanceof Error ? deleteError.message : String(deleteError)
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
    setNewSessionLlm(
      isValidConfiguredModelReference(
        targetAgent.defaultProviderId,
        targetAgent.defaultModel,
        mergedProviderDefinitions,
        providerConfigs,
      )
        ? {
            providerId: targetAgent.defaultProviderId,
            model: targetAgent.defaultModel,
          }
        : pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, targetAgent.defaultProviderId),
    )
  }

  const handleConfirmNewSession = () => {
    const targetAgent = agents.find((item) => item.id === newSessionAgentId) ?? defaultAgent
    if (!targetAgent) {
      return
    }

    if (chatGateError) {
      setChatGateError('')
    }
    const nextSessionLlm =
      newSessionLlm &&
      isValidConfiguredModelReference(
        newSessionLlm.providerId,
        newSessionLlm.model,
        mergedProviderDefinitions,
        providerConfigs,
      )
        ? newSessionLlm
        : pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, targetAgent.defaultProviderId)
    resetSessionDraft()
    setComposerAgent(buildConversationAgentSnapshot(targetAgent))
    setComposerSessionLlm(nextSessionLlm)
    setNewSessionDialogOpen(false)
    handleViewChange('chat')
  }

  const handleStartChatWithAgent = (agentId: string) => {
    const targetAgent = agents.find((item) => item.id === agentId)
    if (!targetAgent) {
      return
    }

    if (chatGateError) {
      setChatGateError('')
    }

    const nextSessionLlm = isValidConfiguredModelReference(
      targetAgent.defaultProviderId,
      targetAgent.defaultModel,
      mergedProviderDefinitions,
      providerConfigs,
    )
      ? {
          providerId: targetAgent.defaultProviderId,
          model: targetAgent.defaultModel,
        }
      : pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, targetAgent.defaultProviderId)

    resetSessionDraft()
    setComposerAgent(buildConversationAgentSnapshot(targetAgent))
    setComposerSessionLlm(nextSessionLlm)
    setNewSessionDialogOpen(false)
    setAgentEditorOpen(false)
    handleViewChange('chat')
  }

  const updateAgentBotConfig = (channelId: BotChannelId, updates: Partial<BotConfig>) => {
    setAgentEditorDraft((current) => {
      if (!current) {
        return current
      }
      const currentConfigs = createAgentBotConfigState(current.botConfigs)
      return {
        ...current,
        botConfigs: {
          ...currentConfigs,
          [channelId]: {
            ...currentConfigs[channelId],
            ...updates,
          },
        },
      }
    })
    if (agentFormError) {
      setAgentFormError('')
    }
    if (agentFormNotice) {
      setAgentFormNotice('')
    }
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

  const addCustomProvider = (name: string, description: string, apiFormat: ProviderApiFormat) => {
    const id = `custom_${crypto.randomUUID().replace(/-/g, '')}`
    const defaultBaseUrl = apiFormat === 'anthropic' ? 'https://api.anthropic.com' : 'https://api.openai.com/v1'
    const defaultModel = apiFormat === 'anthropic' ? 'claude-sonnet-4-0' : 'gpt-4o-mini'
    setCustomProviderMeta((previous) => [...previous, { id, name, description, apiFormat }])
    setProviderConfigs((previous) => ({
      ...previous,
      [id]: {
        ...emptyProviderConfig(),
        added: true,
        apiFormat,
        baseUrl: defaultBaseUrl,
        model: defaultModel,
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
    if (!selectedManagedAgent) {
      setAgentFormError('请先保存当前智能体，再绑定微信 Bot。')
      return
    }
    setBotLoading(true)
    setQrStatus('waiting')
    setQrCodeUrl('')
    setQrDialogOpen(true)

    const runtimeChannelId = selectedManagedAgent
      ? getBotChannelRuntimeId(selectedManagedAgent.id, 'wechat')
      : ''

    try {
      // Subscribe to QR events before calling login
      const unsub = await subscribeQrCode((event: QrCodeEvent) => {
        if (runtimeChannelId && event.channelId !== runtimeChannelId) {
          return
        }
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

      const result = await botLoginWechat(runtimeChannelId)
      unsub()

      if (result.connected) {
        const loginToken = result.bot_token ?? ''
        const loginBaseUrl = result.base_url ?? 'https://ilinkai.weixin.qq.com'

        if (!selectedManagedAgent) {
          throw new Error('请先保存当前智能体，再绑定微信 Bot。')
        }
        const botRuntime = resolveRuntimeFromAgentSnapshot(
          buildConversationAgentSnapshot(selectedManagedAgent),
          providerConfigs,
        )
        if (!botRuntime) {
          throw new Error('请先在设置中配置该智能体默认模型对应的 Provider（Base URL、API Key）。')
        }

        updateAgentBotConfig('wechat', {
          status: '已连接',
          clientId: result.account_id ?? '',
          clientSecret: loginBaseUrl,
          token: loginToken,
          imChannelPaused: false,
          ...buildBotRuntimeBindingConfig(botRuntime),
          errorMessage: undefined,
        })
        setQrDialogOpen(false)

        // Auto-start the bot polling immediately after login
        try {
          await botStartWechat(getBotChannelRuntimeId(selectedManagedAgent.id, 'wechat'), selectedManagedAgent.id, loginToken, {
            baseUrl: loginBaseUrl || undefined,
            providerId: botRuntime.providerId,
            providerApiFormat: botRuntime.apiFormat,
            model: botRuntime.model,
            apiKey: botRuntime.apiKey,
            providerBaseUrl: botRuntime.baseUrl,
          })
          updateAgentBotConfig('wechat', { enabled: true, imChannelPaused: false })
        } catch (startError) {
          updateAgentBotConfig('wechat', {
            status: '错误',
            errorMessage: `登录成功但启动失败: ${String(startError)}`,
          })
        }
      } else {
        updateAgentBotConfig('wechat', {
          status: '错误',
          errorMessage: result.message,
        })
        setQrDialogOpen(false)
      }
    } catch (error) {
      updateAgentBotConfig('wechat', {
        status: '错误',
        errorMessage: String(error),
      })
      setQrDialogOpen(false)
    } finally {
      setBotLoading(false)
    }
  }

  const handleWechatStart = async () => {
    if (!selectedManagedAgent) {
      setAgentFormError('请先保存当前智能体，再启动微信 Bot。')
      return
    }

    const config = selectedManagedBotConfigs.wechat
    if (!config.token) {
      updateAgentBotConfig('wechat', { status: '错误', errorMessage: '请先扫码登录获取 token' })
      return
    }
    setBotLoading(true)
    try {
      const botRuntime = resolveRuntimeFromAgentSnapshot(
        buildConversationAgentSnapshot(selectedManagedAgent),
        providerConfigs,
      )
      if (!botRuntime) {
        throw new Error('请先在设置中配置该智能体默认模型对应的 Provider（Base URL、API Key）。')
      }
      updateAgentBotConfig('wechat', {
        ...buildBotRuntimeBindingConfig(botRuntime),
        imChannelPaused: false,
      })
      await botStartWechat(getBotChannelRuntimeId(selectedManagedAgent.id, 'wechat'), selectedManagedAgent.id, config.token, {
        baseUrl: config.clientSecret || undefined,
        routeTag: config.routeTag || undefined,
        providerId: botRuntime.providerId,
        providerApiFormat: botRuntime.apiFormat,
        model: botRuntime.model,
        apiKey: botRuntime.apiKey,
        providerBaseUrl: botRuntime.baseUrl,
      })
      updateAgentBotConfig('wechat', {
        status: '已连接',
        enabled: true,
        imChannelPaused: false,
        errorMessage: undefined,
      })
    } catch (error) {
      updateAgentBotConfig('wechat', { status: '错误', errorMessage: String(error) })
    } finally {
      setBotLoading(false)
    }
  }

  const handleWechatStop = async () => {
    if (!selectedManagedAgent) {
      return
    }
    setBotLoading(true)
    try {
      await botStopWechat(getBotChannelRuntimeId(selectedManagedAgent.id, 'wechat'))
      updateAgentBotConfig('wechat', { status: '未连接', enabled: false, imChannelPaused: true })
    } catch (error) {
      updateAgentBotConfig('wechat', { status: '错误', errorMessage: String(error) })
    } finally {
      setBotLoading(false)
    }
  }

  const handleLarkStart = async () => {
    if (!selectedManagedAgent) {
      setAgentFormError('请先保存当前智能体，再启动飞书 Bot。')
      return
    }

    const config = selectedManagedBotConfigs.lark
    if (!config.clientId.trim() || !config.clientSecret.trim()) {
      updateAgentBotConfig('lark', {
        status: '错误',
        errorMessage: '请先填写飞书 App ID 和 App Secret',
      })
      return
    }

    setBotLoading(true)
    try {
      const botRuntime = resolveRuntimeFromAgentSnapshot(
        buildConversationAgentSnapshot(selectedManagedAgent),
        providerConfigs,
      )
      if (!botRuntime) {
        throw new Error('请先在设置中配置该智能体默认模型对应的 Provider（Base URL、API Key）。')
      }

      updateAgentBotConfig('lark', {
        ...buildBotRuntimeBindingConfig(botRuntime),
        imChannelPaused: false,
        status: '登录中',
        errorMessage: undefined,
      })
      await botStartLark(
        getBotChannelRuntimeId(selectedManagedAgent.id, 'lark'),
        selectedManagedAgent.id,
        config.clientId.trim(),
        config.clientSecret.trim(),
        {
          providerId: botRuntime.providerId,
          providerApiFormat: botRuntime.apiFormat,
          model: botRuntime.model,
          apiKey: botRuntime.apiKey,
          providerBaseUrl: botRuntime.baseUrl,
        },
      )

      updateAgentBotConfig('lark', {
        status: '已连接',
        enabled: true,
        imChannelPaused: false,
        errorMessage: undefined,
      })
    } catch (error) {
      updateAgentBotConfig('lark', { status: '错误', errorMessage: String(error) })
    } finally {
      setBotLoading(false)
    }
  }

  const handleLarkStop = async () => {
    if (!selectedManagedAgent) {
      return
    }
    setBotLoading(true)
    try {
      await botStopLark(getBotChannelRuntimeId(selectedManagedAgent.id, 'lark'))
      updateAgentBotConfig('lark', {
        status: '未连接',
        enabled: false,
        imChannelPaused: true,
        errorMessage: undefined,
      })
    } catch (error) {
      updateAgentBotConfig('lark', { status: '错误', errorMessage: String(error) })
    } finally {
      setBotLoading(false)
    }
  }

  const handleRotatePeerSecret = async () => {
    if (!selectedManagedAgent) {
      return
    }
    setBotLoading(true)
    setAgentFormError('')
    setAgentFormNotice('')
    try {
      await rotateAgentPeerInboundSecret(selectedManagedAgent.id)
      await refreshAgents(selectedManagedAgent.id)
      setAgentFormNotice('已重新生成该智能体的对等入站密钥并写入数据库。')
    } catch (error) {
      setAgentFormError(error instanceof Error ? error.message : String(error))
    } finally {
      setBotLoading(false)
    }
  }

  const renderContent = () => {
    if (view === 'chat') {
      return (
        <ChatView
          activeProviderLabel={chatProviderLabel}
          agentBuilderActionBusyId={agentBuilderActionBusyId}
          agentBuilderActionError={agentBuilderActionError}
          agentBuilderActionNotice={agentBuilderActionNotice}
          agentBuilderActionTargetId={agentBuilderActionTargetId}
          draft={draft}
          error={chatGateError || error}
          showExecutionRail={appearanceSettings.showExecutionRail}
          showThinkingProcess={appearanceSettings.showThinkingProcess}
          globalBusy={loading}
          runningHistoryIds={runningHistoryIds}
          activeHistoryId={activeHistoryId}
          onAbort={abortPrompt}
          attachmentError={composerAttachmentError}
          attachmentInputRef={composerAttachmentInputRef}
          attachmentUploading={composerAttachmentUploading}
          composerAttachments={composerAttachments}
          onCreateAgentDraft={handleCreateAgentFromDraft}
          onComposerAttachmentInputChange={handleComposerAttachmentInputChange}
          onComposerClearAttachments={clearComposerAttachments}
          onComposerPaste={handleComposerPaste}
          onComposerPickAttachment={openComposerAttachmentPicker}
          onComposerRemoveAttachment={removeComposerAttachment}
          onComposerClearAttachmentError={clearComposerAttachmentError}
          onSubmit={handleSubmit}
          selectedAgent={activeChatAgent}
          setDraft={setDraft}
          submitShortcut={generalSettings.submitShortcut}
          activeHistoryItem={activeHistoryItem}
          workspaceTitle={workspaceTitle}
          sessionLlmSelectOptions={sessionLlmSelectOptionsWithFallback}
          sessionLlmSelectValue={sessionLlmEncodedCurrent}
          onSessionLlmSelectChange={handleSessionLlmSelectChange}
          runtimeReady={runtimeReady}
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

    if (view === 'tasks') {
      return <TasksView agents={editableAgents} onOpenAgent={handleOpenAgentEditor} />
    }

    return (
      <AgentsView
        agentDraft={agentEditorDraft}
        agentBotBindingDialogOpen={agentBotBindingDialogOpen}
        agentDeleteConfirmOpen={agentDeleteConfirmOpen}
        agentDeleteConfirmText={agentDeleteConfirmText}
        agentEditorOpen={agentEditorOpen}
        agentFormError={agentFormError}
        agentFormNotice={agentFormNotice}
        agentRefreshing={agentRefreshing}
        agentSaving={agentSaving}
        botConfigs={selectedManagedBotConfigs}
        botLoading={botLoading}
        botStatusLog={botStatusLog.filter((entry) =>
          selectedManagedAgent ? entry.channelId === getBotChannelRuntimeId(selectedManagedAgent.id, selectedBotId) : false,
        )}
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
        onBotConfigChange={updateAgentBotConfig}
        onCloseEditor={handleCloseAgentEditor}
        onCloseBotBindingDialog={handleCloseAgentBotBindingDialog}
        onCloseDeleteAgentDialog={handleCloseDeleteAgentDialog}
        onCloseWorkspaceDialog={handleCloseAgentWorkspaceDialog}
        onConfirmDeleteAgent={handleConfirmDeleteCurrentAgent}
        onDraftWorkspaceContentChange={setAgentWorkspaceDraftContent}
        onDraftChange={handleAgentDraftChange}
        onDeleteConfirmTextChange={setAgentDeleteConfirmText}
        onRefreshAgents={handleRefreshCurrentAgent}
        onOpenEditor={handleOpenAgentEditor}
        onOpenBotBinding={handleOpenAgentBotBinding}
        onOpenWorkspace={handleOpenAgentWorkspace}
        onOpenSkillPicker={handleOpenAgentSkillPicker}
        onRefreshWorkspace={handleRefreshAgentWorkspace}
        onRequestDeleteAgent={handleRequestDeleteCurrentAgent}
        onSaveAgent={handleSaveAgent}
        onSaveWorkspaceFile={handleSaveAgentWorkspaceFile}
        onSearch={setAgentSearch}
        onSelectAgent={handleManagedAgentSelect}
        onSelectBot={setSelectedBotId}
        onSelectWorkspaceFile={handleSelectAgentWorkspaceFile}
        onSetDefaultAgent={handleSetCurrentDefaultAgent}
        onToggleSkill={handleAgentSkillToggle}
        onLarkStart={handleLarkStart}
        onLarkStop={handleLarkStop}
        onWechatLogin={handleWechatLogin}
        onWechatStart={handleWechatStart}
        onWechatStop={handleWechatStop}
        onRotatePeerSecret={handleRotatePeerSecret}
        onStartChatWithAgent={handleStartChatWithAgent}
        searchValue={agentSearch}
        selectedAgent={selectedManagedAgent}
        selectedBotConfig={selectedManagedBotConfig}
        selectedBotDefinition={selectedBotDefinition}
        selectedBotId={selectedBotId}
        loading={agentsLoading}
        error={agentsError}
        mode={agentEditorMode}
        modelOptions={sessionLlmSelectOptionsWithFallback}
        qrCodeUrl={qrCodeUrl}
        qrDialogOpen={qrDialogOpen}
        qrStatus={qrStatus}
        setBotLoading={setBotLoading}
        setQrDialogOpen={setQrDialogOpen}
        managedAgentId={managedAgentId}
      />
    )
  }

  return (
    <>
      <main
        className={[
          'app-shell',
          `theme-${appearanceSettings.themeMode}`,
          appearanceSettings.compactSidebar ? 'compact-sidebar' : '',
          effectiveSidebarCollapsed ? 'sidebar-is-collapsed' : '',
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
          <div className="sidebar-window">
            {!shouldHideSidebar ? (
              <button
                type="button"
                className={`collapse-button ${effectiveSidebarCollapsed ? 'collapsed' : ''}`}
                aria-label={effectiveSidebarCollapsed ? '展开侧栏' : '折叠侧栏'}
                onClick={() =>
                  setAppearanceSettings((previous) => ({
                    ...previous,
                    sidebarCollapsed: !previous.sidebarCollapsed,
                  }))
                }
              >
                <AppIcon name="panel" size={18} />
              </button>
            ) : null}
          </div>

          <div className="sidebar-brand">
            <div className="sidebar-brand-main">
              <div className="sidebar-brand-mark">9</div>
              <div className="sidebar-brand-copy">
                <strong>NineClaw</strong>
                <span>多智能体工作台</span>
              </div>
            </div>
          </div>

          <div className="sidebar-section">
            <div className="sidebar-section-title">导航</div>
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
              <SidebarButton
                active={view === 'tasks'}
                icon="clock"
                label="任务中心"
                onClick={() => handleViewChange('tasks')}
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
                      <span className="history-card-copy">
                        <span className="history-card-title">{summarizePrompt(item.title, 20)}</span>
                        {item.agent ? <span className="history-card-agent">{item.agent.name}</span> : null}
                      </span>
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
          generalSettings={generalSettings}
          onAddCustomProvider={addCustomProvider}
          onProviderConfigChange={updateProviderConfig}
          onClose={() => setSettingsOpen(false)}
          onRemoveCustomProvider={removeCustomProvider}
          onSelectProvider={setSelectedProviderId}
          onSelectTab={setSettingsTab}
          providerConfigs={providerConfigs}
          selectedProviderConfig={selectedProviderConfig}
          selectedProviderDefinition={selectedProviderDefinition}
          selectedProviderId={selectedProviderId}
          setAppearanceSettings={setAppearanceSettings}
          setGeneralSettings={setGeneralSettings}
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
  attachmentError: string
  attachmentInputRef: RefObject<HTMLInputElement | null>
  attachmentUploading: boolean
  composerAttachments: PersistedChatAttachment[]
  draft: string
  error: string
  globalBusy: boolean
  runningHistoryIds: string[]
  onAbort: () => void
  onComposerAttachmentInputChange: (event: ChangeEvent<HTMLInputElement>) => void
  onComposerClearAttachments: () => void
  onComposerPaste: (event: ClipboardEvent<HTMLTextAreaElement>) => void
  onComposerPickAttachment: () => void
  onComposerRemoveAttachment: (attachmentId: string) => void
  onComposerClearAttachmentError: () => void
  onCreateAgentDraft: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
  onSubmit: () => Promise<void>
  selectedAgent: ConversationAgentSnapshot | null
  setDraft: (value: string) => void
  showExecutionRail: boolean
  showThinkingProcess: boolean
  submitShortcut: SubmitShortcut
  workspaceTitle: string
  sessionLlmSelectOptions: { value: string; label: string }[]
  sessionLlmSelectValue: string
  onSessionLlmSelectChange: (value: string) => void
  runtimeReady: boolean
}

function ChatView({
  activeProviderLabel,
  activeHistoryId,
  activeHistoryItem,
  agentBuilderActionBusyId,
  agentBuilderActionError,
  agentBuilderActionNotice,
  agentBuilderActionTargetId,
  attachmentError,
  attachmentInputRef,
  attachmentUploading,
  composerAttachments,
  draft,
  error,
  globalBusy,
  runningHistoryIds,
  onAbort,
  onComposerAttachmentInputChange,
  onComposerClearAttachments,
  onComposerPaste,
  onComposerPickAttachment,
  onComposerRemoveAttachment,
  onComposerClearAttachmentError,
  onCreateAgentDraft,
  onSubmit,
  selectedAgent,
  setDraft,
  showExecutionRail,
  showThinkingProcess,
  submitShortcut,
  workspaceTitle,
  sessionLlmSelectOptions,
  sessionLlmSelectValue,
  onSessionLlmSelectChange,
  runtimeReady,
}: ChatViewProps) {
  const [copiedTurnId, setCopiedTurnId] = useState('')
  const [copiedPromptTurnId, setCopiedPromptTurnId] = useState('')
  const [previewImage, setPreviewImage] = useState<{ src: string; alt: string } | null>(null)
  const [composerHeight, setComposerHeight] = useState(DEFAULT_COMPOSER_HEIGHT)
  const [isComposerResizing, setIsComposerResizing] = useState(false)
  const [showScrollToLatest, setShowScrollToLatest] = useState(false)
  const resizeStateRef = useRef<{ startHeight: number; startY: number } | null>(null)
  const workspaceScrollRef = useRef<HTMLElement | null>(null)
  const turns = activeHistoryItem?.turns ?? []
  const activeTurnId = turns.at(-1)?.id ?? ''
  const isHomeState = !activeHistoryItem
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
    setCopiedTurnId('')
    setCopiedPromptTurnId('')
    setPreviewImage(null)
  }, [activeHistoryId])

  useEffect(() => {
    if (!activeTurnId) {
      return
    }

    document.getElementById(`chat-turn-${activeTurnId}`)?.scrollIntoView({
      block: 'end',
      behavior: 'smooth',
    })
  }, [activeHistoryId, activeTurnId, sessionRunning, turns.length])

  useEffect(() => {
    const handlePointerMove = (event: PointerEvent) => {
      const resizeState = resizeStateRef.current
      if (!resizeState) {
        return
      }

      const nextHeight = clampNumber(
        resizeState.startHeight + (resizeState.startY - event.clientY),
        MIN_COMPOSER_HEIGHT,
        MAX_COMPOSER_HEIGHT,
      )
      setComposerHeight(nextHeight)
    }

    const stopResize = () => {
      resizeStateRef.current = null
      setIsComposerResizing(false)
    }

    window.addEventListener('pointermove', handlePointerMove)
    window.addEventListener('pointerup', stopResize)
    window.addEventListener('pointercancel', stopResize)

    return () => {
      window.removeEventListener('pointermove', handlePointerMove)
      window.removeEventListener('pointerup', stopResize)
      window.removeEventListener('pointercancel', stopResize)
    }
  }, [])

  useEffect(() => {
    if (!isHomeState) {
      return
    }

    workspaceScrollRef.current?.scrollTo({ top: 0, behavior: 'auto' })
  }, [isHomeState])

  const syncScrollToLatestVisibility = useCallback(() => {
    const viewport = workspaceScrollRef.current
    if (!viewport || isHomeState) {
      setShowScrollToLatest(false)
      return
    }

    const distanceToBottom = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight
    setShowScrollToLatest(distanceToBottom > 180)
  }, [isHomeState])

  useEffect(() => {
    syncScrollToLatestVisibility()
  }, [syncScrollToLatestVisibility, activeHistoryId, activeTurnId, turns.length])

  const handleCopyAnswer = async (turnId: string, answer: string) => {
    if (!answer) {
      return
    }

    await navigator.clipboard.writeText(answer)
    setCopiedTurnId(turnId)
    window.setTimeout(() => setCopiedTurnId((current) => (current === turnId ? '' : current)), 1600)
  }

  const handleCopyPrompt = async (turnId: string, prompt: string) => {
    if (!prompt) {
      return
    }

    await navigator.clipboard.writeText(prompt)
    setCopiedPromptTurnId(turnId)
    window.setTimeout(() => setCopiedPromptTurnId((current) => (current === turnId ? '' : current)), 1600)
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

  const handleScrollToLatest = () => {
    const viewport = workspaceScrollRef.current
    if (!viewport) {
      return
    }

    if (activeTurnId) {
      document.getElementById(`chat-turn-${activeTurnId}`)?.scrollIntoView({
        block: 'end',
        behavior: 'smooth',
      })
    } else {
      viewport.scrollTo({ top: viewport.scrollHeight, behavior: 'smooth' })
    }
  }

  const handleComposerResizeStart = (event: ReactPointerEvent<HTMLDivElement>) => {
    resizeStateRef.current = {
      startHeight: composerHeight,
      startY: event.clientY,
    }
    setIsComposerResizing(true)
  }

  const composerPlaceholder = activeHistoryItem ? '继续对话…' : "描述您的需求，或输入 '/' 唤起技能…"
  const composerInlineStyle = {
    '--composer-textarea-height': `${composerHeight}px`,
  } as CSSProperties

  return (
    <div className={`workspace ${isHomeState ? 'workspace-home-state' : 'workspace-thread-state'}`}>
      <header className="workspace-topbar">
        <div className="workspace-topbar-left">
          {activeHistoryItem ? <h1 title={workspaceTitle}>{workspaceTitle}</h1> : null}
        </div>
      </header>

      <section ref={workspaceScrollRef} className="workspace-scroll" onScroll={syncScrollToLatestVisibility}>
        {activeHistoryItem ? (
          <>
            <div className="status-strip">
              <div className="status-strip-head">
                <span className="status-strip-note">{workspaceStatusNote}</span>
                {selectedAgent ? (
                  <div className="agent-chip" title={`当前挂载智能体：${selectedAgent.name}`}>
                    <span className="agent-chip-dot" style={{ backgroundColor: getAgentColor(selectedAgent) }} />
                    <span>{selectedAgent.name}</span>
                  </div>
                ) : null}
              </div>
            </div>

            <div className="chat-message-list">
              {turns.map((item) => (
                (() => {
                  const isStreamingTurn = sessionRunning && item.id === activeTurnId
                  const isWaitingOnly = isTurnWaitingOnly(item, isStreamingTurn, showExecutionRail, showThinkingProcess)
                  const shouldShowActions =
                    !isWaitingOnly &&
                    Boolean(
                      item.answer ||
                        getElapsedMs(item.createdAt, item.completedAt, isStreamingTurn) ||
                        hasUsageMetrics(item.usage),
                    )

                  return (
                    <article
                      key={item.id}
                      id={`chat-turn-${item.id}`}
                      className={`chat-turn ${item.id === activeTurnId ? 'active' : ''}`}
                    >
                      <div className="prompt-block">
                        <PromptBubbleContent content={item.prompt} onImageClick={handleMarkdownImageClick} />
                        <div className="prompt-meta">
                          <div className="prompt-timestamp">{formatAbsoluteTime(item.createdAt)}</div>
                          <button
                            type="button"
                            className={`prompt-copy-icon-button ${copiedPromptTurnId === item.id ? 'copied' : ''}`}
                            onClick={() => void handleCopyPrompt(item.id, item.prompt)}
                            aria-label={copiedPromptTurnId === item.id ? '已复制提问' : '复制提问'}
                            title={copiedPromptTurnId === item.id ? '已复制提问' : '复制提问'}
                          >
                            {copiedPromptTurnId === item.id ? <Check size={15} /> : <Copy size={15} />}
                          </button>
                        </div>
                      </div>

                      {shouldRenderAssistantColumn(item, showExecutionRail, showThinkingProcess, isStreamingTurn) ? (
                        <div className="chat-response">
                          <div className="assistant-message-shell">
                            <div
                              className="assistant-avatar"
                              style={
                                selectedAgent?.accentColor
                                  ? {
                                      borderColor: `${selectedAgent.accentColor}55`,
                                      background: `${selectedAgent.accentColor}22`,
                                      color: selectedAgent.accentColor,
                                    }
                                  : undefined
                              }
                              aria-hidden
                            >
                              <AppIcon name="bot" size={20} />
                            </div>
                            <div className="assistant-message-stack">
                              {shouldShowActions ? (
                                <div className="assistant-message-toolbar">
                                  <div className="assistant-message-toolbar-meta">
                                    <TurnExecutionDetails turn={item} isStreaming={isStreamingTurn} variant="inline" />
                                  </div>
                                </div>
                              ) : null}
                              <div
                                className={`answer-result-card answer-result-card-chat${
                                  isWaitingOnly ? ' answer-result-card-waiting' : ''
                                }`}
                              >
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
                                    showThinkingProcess={showThinkingProcess}
                                    onImageClick={handleMarkdownImageClick}
                                  />
                                </div>
                              </div>
                              {shouldShowActions ? (
                                <div className="assistant-message-answer-footer">
                                  <button
                                    type="button"
                                    className={`answer-copy-control ${copiedTurnId === item.id ? 'copied' : ''}`}
                                    onClick={() => void handleCopyAnswer(item.id, item.answer)}
                                    aria-label={copiedTurnId === item.id ? '已复制结果' : '复制结果'}
                                    title={copiedTurnId === item.id ? '已复制结果' : '复制结果'}
                                    disabled={!item.answer}
                                  >
                                    {copiedTurnId === item.id ? <Check size={16} /> : <Copy size={16} />}
                                    <span className="answer-copy-control-label">
                                      {copiedTurnId === item.id ? '已复制' : '复制'}
                                    </span>
                                  </button>
                                </div>
                              ) : null}
                            </div>
                          </div>
                        </div>
                      ) : null}
                    </article>
                  )
                })()
              ))}
            </div>
          </>
        ) : (
          <section className="new-task-home">
            <div className="home-brand-block">
              <span className="home-brand-kicker">NineClaw Workspace</span>
              <div className="home-brand-mark">今天想让 NineClaw 帮你处理什么？</div>
              <p>输入需求、上传素材，直接开始一条新会话。</p>
            </div>
          </section>
        )}
      </section>

      <div className="workspace-footer-stack">
        {activeHistoryItem && showScrollToLatest ? (
          <button
            type="button"
            className="scroll-to-latest-button"
            onClick={handleScrollToLatest}
            aria-label="滚动到最新消息"
            title="滚动到最新消息"
          >
            <AppIcon name="arrow-down" size={22} />
          </button>
        ) : null}

        {error && activeHistoryItem ? <div className="error-banner">{error}</div> : null}

        <div className={`workspace-composer-shell ${isComposerResizing ? 'is-resizing' : ''}`}>
        <form
          className="composer-card"
          style={composerInlineStyle}
          onSubmit={(event) => {
            event.preventDefault()
            void onSubmit()
          }}
        >
          <input
            ref={attachmentInputRef}
            type="file"
            className="composer-file-input"
            multiple
            onChange={onComposerAttachmentInputChange}
          />
          <div
            className="composer-resize-handle"
            role="separator"
            aria-label="拖动调整输入框高度"
            aria-orientation="horizontal"
            onPointerDown={handleComposerResizeStart}
          >
            <span />
          </div>
          <textarea
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={handleComposerKeyDown}
            onPaste={(event) => {
              if (attachmentError) {
                onComposerClearAttachmentError()
              }
              void onComposerPaste(event)
            }}
            placeholder={composerPlaceholder}
            rows={4}
            disabled={sessionStreaming}
          />
          <ComposerAttachmentStrip
            attachments={composerAttachments}
            uploading={attachmentUploading}
            onRemove={onComposerRemoveAttachment}
            onClear={onComposerClearAttachments}
          />
          <div className="composer-toolbar">
            <div className="composer-toolbar-left">
              <button
                type="button"
                className="ghost-icon-button"
                aria-label="附件"
                onClick={() => {
                  if (attachmentError) {
                    onComposerClearAttachmentError()
                  }
                  onComposerPickAttachment()
                }}
                disabled={sessionStreaming || attachmentUploading || !selectedAgent}
              >
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
              <div className="session-llm-toolbar composer-session-llm-toolbar" title={activeProviderLabel}>
                <label className="visually-hidden" htmlFor="session-llm-combined">
                  本会话使用的模型
                </label>
                <label className="select-field composer-model-field">
                  <AppIcon name="zap" size={13} />
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
                </label>
              </div>
            </div>
            <div className="composer-toolbar-right">
              {sessionStreaming ? (
                <button type="button" className="stop-button" onClick={onAbort}>
                  <AppIcon name="stop" size={16} />
                </button>
              ) : (
                <button type="submit" className="submit-button" disabled={sessionStreaming || !runtimeReady}>
                  {runtimeReady ? <AppIcon name="arrow-up" size={17} /> : <span className="composer-runtime-loading">⏳</span>}
                </button>
              )}
            </div>
          </div>
        </form>
        {isHomeState ? (
          <div className="starter-chip-row">
            {STARTER_CHIPS.map((chip) => (
              <button key={chip} type="button" className="starter-chip" onClick={() => setDraft(chip)}>
                {chip}
              </button>
            ))}
          </div>
        ) : null}
        {attachmentError ? <div className="composer-attachment-error">{attachmentError}</div> : null}
        <div className="composer-footnote">
          {!runtimeReady
            ? 'PI 运行时正在初始化，请稍候...'
            : globalBusy && !sessionStreaming
              ? '其他会话也在执行中；当前会话仍可继续发送。'
              : `发送快捷键：${getSubmitShortcutLabel(submitShortcut)}。Shift + Enter 可换行。`}
        </div>
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
  showThinkingProcess,
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
  showThinkingProcess: boolean
  onImageClick: (src: string, alt: string) => void
}) {
  const toolById = new Map(turn.toolCalls.map((t) => [t.toolCallId, t]))
  const segments = turn.responseSegments
  const isActiveStreamingTurn = loading && turn.id === activeTurnId
  const runningToolCount = turn.toolCalls.filter((toolCall) => toolCall.state === 'running').length

  if (segments && segments.length > 0) {
    const renderBlocks: Array<
      | { type: 'text'; text: string; index: number }
      | { type: 'tools'; toolCalls: ToolCallEntry[]; index: number }
    > = []
    let groupedToolCalls: ToolCallEntry[] = []
    let groupedToolStartIndex = -1

    const flushGroupedToolCalls = () => {
      if (groupedToolCalls.length === 0) {
        return
      }
      renderBlocks.push({
        type: 'tools',
        toolCalls: groupedToolCalls,
        index: groupedToolStartIndex,
      })
      groupedToolCalls = []
      groupedToolStartIndex = -1
    }

    segments.forEach((segment, index) => {
      if (segment.type === 'text') {
        flushGroupedToolCalls()
        renderBlocks.push({ type: 'text', text: segment.text, index })
        return
      }

      if (!showExecutionRail) {
        return
      }

      const toolCall = toolById.get(segment.toolCallId)
      if (!toolCall) {
        flushGroupedToolCalls()
        return
      }

      if (groupedToolCalls.length === 0) {
        groupedToolStartIndex = index
      }
      groupedToolCalls.push(toolCall)
    })

    flushGroupedToolCalls()

    let lastTextSegmentIndex = -1
    for (let i = renderBlocks.length - 1; i >= 0; i -= 1) {
      if (renderBlocks[i]?.type === 'text') {
        lastTextSegmentIndex = renderBlocks[i].index
        break
      }
    }

    const firstToolsBlockIndex = renderBlocks.findIndex(
      (b) => b.type === 'tools' && b.toolCalls.length > 0,
    )
    const thinkingEmbeddedInRail =
      showThinkingProcess && turn.thinking.trim() && firstToolsBlockIndex >= 0

    return (
      <div className="turn-response-blocks">
        {showThinkingProcess && turn.thinking.trim() && !thinkingEmbeddedInRail ? (
          <TurnThinkingBlock isStreaming={isActiveStreamingTurn} thinking={turn.thinking} />
        ) : null}
        {renderBlocks.map((block, blockIndex) => {
          if (block.type === 'text') {
            const isStreaming = Boolean(
              isActiveStreamingTurn && block.index === lastTextSegmentIndex,
            )
            if ((!block.text.trim() && !isStreaming) || isTurnPlaceholderNoOutputText(block.text)) {
              return null
            }
            return (
              <MarkdownBlock
                key={`${turn.id}-t-${block.index}`}
                actionId={`${turn.id}-t-${block.index}`}
                actionBusyId={agentBuilderActionBusyId}
                actionError={agentBuilderActionError}
                actionNotice={agentBuilderActionNotice}
                actionTargetId={agentBuilderActionTargetId}
                content={block.text}
                isStreaming={isStreaming}
                onCreateAgentDraft={onCreateAgentDraft}
                onImageClick={onImageClick}
              />
            )
          }

          if (block.toolCalls.length === 0) {
            return null
          }

          return (
            <TurnExecutionRail
              key={`${turn.id}-tools-${block.index}`}
              toolCalls={block.toolCalls}
              thinking={blockIndex === firstToolsBlockIndex ? turn.thinking : ''}
              showThinkingProcess={showThinkingProcess}
              runningToolCount={runningToolCount}
              onImageClick={onImageClick}
            />
          )
        })}
      </div>
    )
  }

  const legacyTools = [...turn.toolCalls].sort((a, b) => a.createdAt - b.createdAt)
  const hasLegacyTools = showExecutionRail && legacyTools.length > 0
  const legacyThinkingInRail = showThinkingProcess && turn.thinking.trim() && hasLegacyTools

  return (
    <>
      {showThinkingProcess && turn.thinking.trim() && !legacyThinkingInRail ? (
        <TurnThinkingBlock isStreaming={isActiveStreamingTurn} thinking={turn.thinking} />
      ) : null}
      {turn.answer && !isTurnPlaceholderNoOutputText(turn.answer) ? (
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
      {hasLegacyTools ? (
        <TurnExecutionRail
          toolCalls={legacyTools}
          thinking={turn.thinking}
          showThinkingProcess={showThinkingProcess}
          runningToolCount={runningToolCount}
          onImageClick={onImageClick}
        />
      ) : null}
      {!turn.answer && !hasLegacyTools ? (
        isActiveStreamingTurn ? (
          <TurnWaitingIndicator startedAt={turn.createdAt} />
        ) : turn.status === 'done' ? (
          <p className="placeholder-copy">本轮已结束，但模型没有返回任何可渲染内容。</p>
        ) : turn.status === 'error' ? (
          <p className="placeholder-copy">本轮执行失败，未产出可渲染内容。</p>
        ) : legacyTools.length > 0 ? (
          <p className="placeholder-copy">本轮主要产出了工具调用结果。</p>
        ) : null
      ) : null}
    </>
  )
}

function TurnThinkingBlock({ thinking, isStreaming }: { thinking: string; isStreaming: boolean }) {
  const [expanded, setExpanded] = useState(true)

  useEffect(() => {
    if (isStreaming) {
      setExpanded(true)
    }
  }, [isStreaming])

  return (
    <details
      className="turn-thinking-block"
      open={expanded}
      onToggle={(event) => setExpanded(event.currentTarget.open)}
      data-streaming={isStreaming ? '' : undefined}
    >
      <summary className="turn-thinking-summary">思考过程</summary>
      <pre className="turn-thinking-pre">{thinking}</pre>
    </details>
  )
}

function TurnWaitingIndicator({ startedAt }: { startedAt: number }) {
  const now = useLiveNow(true, 500)
  const elapsed = Math.max(0, now - startedAt)

  return (
    <div className="turn-waiting-indicator" role="status" aria-live="polite">
      <span className="turn-waiting-dots" aria-hidden="true">
        <span />
        <span />
        <span />
      </span>
      <span className="turn-waiting-label">处理中</span>
      <span className="turn-waiting-time">已执行 {formatDurationLabel(elapsed)}</span>
    </div>
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
  const { contentWithoutAttachments, attachments } = useMemo(
    () => extractInlineMediaAttachments(cleanedContent),
    [cleanedContent],
  )
  const normalizedContent = useMemo(
    () => normalizeMarkdownImageSources(contentWithoutAttachments),
    [contentWithoutAttachments],
  )

  const replyCardItems = useMemo(
    () => resolveReplyCardItems(normalizedContent || '', isStreaming),
    [normalizedContent, isStreaming],
  )

  return (
    <>
      {normalizedContent ? (
        <ReplyCardStack items={replyCardItems} isStreaming={isStreaming} onImageClick={onImageClick} />
      ) : null}
      {attachments.length > 0 ? <InlineMediaAttachmentList attachments={attachments} onImageClick={onImageClick} /> : null}
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

function getToolGroupState(toolCalls: ToolCallEntry[]): ToolCallEntry['state'] {
  if (toolCalls.some((toolCall) => toolCall.state === 'running')) {
    return 'running'
  }
  if (toolCalls.some((toolCall) => toolCall.state === 'error')) {
    return 'error'
  }
  return 'done'
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

/** 按空行将 thinking 粗分为每轮工具前一段 + 末尾纯思考段（与参考 UI 对齐） */
function computeExecThoughtSlices(raw: string, toolCount: number): { perRound: string[]; tailOnly: string } {
  const perRound = Array.from({ length: toolCount }, () => '')
  let tailOnly = ''
  const t = raw.trim()
  if (!t || toolCount === 0) {
    return { perRound, tailOnly }
  }

  const parts = t
    .split(/\n{2,}/)
    .map((s) => s.trim())
    .filter(Boolean)
  if (parts.length === 0) {
    return { perRound, tailOnly }
  }

  if (parts.length <= toolCount) {
    for (let i = 0; i < parts.length; i += 1) {
      perRound[i] = parts[i] ?? ''
    }
  } else {
    for (let i = 0; i < toolCount; i += 1) {
      perRound[i] = parts[i] ?? ''
    }
    tailOnly = parts.slice(toolCount).join('\n\n')
  }

  return { perRound, tailOnly }
}

function isTurnPlaceholderNoOutputText(text: string): boolean {
  return text.trim() === TURN_PLACEHOLDER_NO_OUTPUT
}

function hasRenderableTurnContent(
  turn: ConversationTurn,
  showExecutionRail: boolean,
  showThinkingProcess: boolean,
): boolean {
  if (showThinkingProcess && turn.thinking.trim()) {
    return true
  }

  if (turn.answer.trim() && !isTurnPlaceholderNoOutputText(turn.answer)) {
    return true
  }

  if (turn.responseSegments?.length) {
    return turn.responseSegments.some((segment) => {
      if (segment.type === 'text') {
        const t = segment.text.trim()
        return t.length > 0 && !isTurnPlaceholderNoOutputText(t)
      }
      return showExecutionRail && turn.toolCalls.some((toolCall) => toolCall.toolCallId === segment.toolCallId)
    })
  }

  return showExecutionRail && turn.toolCalls.length > 0
}

function shouldRenderAssistantColumn(
  turn: ConversationTurn,
  showExecutionRail: boolean,
  showThinkingProcess: boolean,
  isActiveStreamingTurn: boolean,
): boolean {
  if (isActiveStreamingTurn && !hasRenderableTurnContent(turn, showExecutionRail, showThinkingProcess)) {
    return true
  }
  if (hasRenderableTurnContent(turn, showExecutionRail, showThinkingProcess)) {
    return true
  }
  if (showExecutionRail && turn.toolCalls.length > 0) {
    return true
  }
  if (isTurnPlaceholderNoOutputText(turn.answer)) {
    return false
  }
  if (!turn.answer.trim()) {
    if (turn.status === 'done' || turn.status === 'error') {
      return true
    }
    if (turn.toolCalls.length > 0) {
      return true
    }
    return false
  }
  return true
}

function isTurnWaitingOnly(
  turn: ConversationTurn,
  isStreaming: boolean,
  showExecutionRail: boolean,
  showThinkingProcess: boolean,
): boolean {
  return isStreaming && !hasRenderableTurnContent(turn, showExecutionRail, showThinkingProcess)
}

function ToolCallContentBlock({
  content,
  isStreaming,
  onImageClick,
}: {
  content: string
  isStreaming: boolean
  onImageClick?: (src: string, alt: string) => void
}) {
  const { contentWithoutAttachments, attachments } = useMemo(
    () => extractInlineMediaAttachments(content),
    [content],
  )
  const normalizedContent = useMemo(
    () => normalizeMarkdownImageSources(contentWithoutAttachments || content),
    [content, contentWithoutAttachments],
  )

  const handleClick = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target
    if (!(target instanceof HTMLElement)) {
      return
    }

    const anchor = target.closest('a[href]')
    if (anchor instanceof HTMLAnchorElement && anchor.href) {
      event.preventDefault()
      event.stopPropagation()
      void openExternalUrl(anchor.href)
      return
    }

    if (!onImageClick) {
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
    <div className="tool-call-code tool-call-code-stream">
      {normalizedContent ? (
        <div className="markdown-content" onClick={handleClick}>
          <Suspense fallback={<MarkdownFallback content={normalizedContent || ' '} />}>
            <MarkdownRenderer content={normalizedContent || ' '} isStreaming={isStreaming} />
          </Suspense>
        </div>
      ) : null}
      {attachments.length > 0 ? <InlineMediaAttachmentList attachments={attachments} onImageClick={onImageClick} /> : null}
    </div>
  )
}

function ToolRoundIoPanels({
  toolCall,
  onImageClick,
}: {
  toolCall: ToolCallEntry
  onImageClick?: (src: string, alt: string) => void
}) {
  const isStreaming = toolCall.state === 'running'
  const argsLive = formatToolCallText(toolCall.argsText, '无参数', false)
  const resultLive = formatToolCallText(toolCall.resultText, '暂无输出', false)
  const argsPretty = formatToolCallText(toolCall.argsText, '无参数')
  const resultPretty = formatToolCallText(toolCall.resultText, '暂无输出')
  const [copiedKey, setCopiedKey] = useState<'input' | 'output' | ''>('')

  const handleCopy = async (key: 'input' | 'output', text: string) => {
    await navigator.clipboard.writeText(text)
    setCopiedKey(key)
    window.setTimeout(() => setCopiedKey((current) => (current === key ? '' : current)), 1600)
  }

  return (
    <div className="tool-exec-io-stack">
      <div className="tool-io-panel tool-io-panel-input">
        <div className="tool-io-panel-head">
          <span className="tool-io-label">INPUT</span>
          <button
            type="button"
            className={`tool-io-copy-button ${copiedKey === 'input' ? 'copied' : ''}`}
            onClick={() => void handleCopy('input', argsPretty)}
            aria-label={copiedKey === 'input' ? '已复制' : '复制 INPUT'}
            title={copiedKey === 'input' ? '已复制' : '复制'}
          >
            {copiedKey === 'input' ? <Check size={14} /> : <Copy size={14} />}
          </button>
        </div>
        <div className="tool-io-panel-body">
          <ToolCallContentBlock
            content={isStreaming ? argsLive : argsPretty}
            isStreaming={isStreaming}
            onImageClick={onImageClick}
          />
        </div>
      </div>
      <div className="tool-io-panel tool-io-panel-output">
        <div className="tool-io-panel-head">
          <span className="tool-io-label">RESULT</span>
          <button
            type="button"
            className={`tool-io-copy-button ${copiedKey === 'output' ? 'copied' : ''}`}
            onClick={() => void handleCopy('output', resultPretty)}
            aria-label={copiedKey === 'output' ? '已复制' : '复制 RESULT'}
            title={copiedKey === 'output' ? '已复制' : '复制'}
          >
            {copiedKey === 'output' ? <Check size={14} /> : <Copy size={14} />}
          </button>
        </div>
        <div className="tool-io-panel-body">
          <ToolCallContentBlock
            content={isStreaming ? resultLive : resultPretty}
            isStreaming={isStreaming}
            onImageClick={onImageClick}
          />
        </div>
      </div>
    </div>
  )
}

function TurnExecutionRail({
  toolCalls,
  thinking,
  showThinkingProcess,
  runningToolCount,
  onImageClick,
}: {
  toolCalls: ToolCallEntry[]
  thinking: string
  showThinkingProcess: boolean
  runningToolCount: number
  onImageClick?: (src: string, alt: string) => void
}) {
  const sorted = useMemo(() => [...toolCalls].sort((a, b) => a.createdAt - b.createdAt), [toolCalls])
  const n = sorted.length
  const thinkingRaw = showThinkingProcess ? thinking : ''
  const { perRound, tailOnly } = useMemo(
    () => computeExecThoughtSlices(thinkingRaw, n),
    [thinkingRaw, n],
  )
  const hasThinkingText = Boolean(thinkingRaw.trim())
  const thinkingRoundCount = n + (tailOnly.trim() ? 1 : 0)
  const groupState = getToolGroupState(sorted)
  const runningInSorted = sorted.filter((t) => t.state === 'running').length
  const [railOpen, setRailOpen] = useState(false)

  useEffect(() => {
    if (runningInSorted > 0) {
      setRailOpen(true)
    }
  }, [runningInSorted])

  if (n === 0) {
    return null
  }

  const parallelRunning = runningInSorted > 1

  return (
    <details
      className={`assistant-exec-rail tool-call-card ${groupState}${parallelRunning ? ' parallel-running' : ''}`}
      open={railOpen}
      onToggle={(event) => setRailOpen(event.currentTarget.open)}
    >
      <summary className="assistant-exec-rail-summary">
        <div className="assistant-exec-rail-summary-main">
          <span className="assistant-exec-rail-wrench" aria-hidden>
            <AppIcon name="wrench" size={15} />
          </span>
          <span className="assistant-exec-rail-title">
            {hasThinkingText ? `${n} 次工具调用 · 思考 ${thinkingRoundCount} 轮` : `${n} 次工具调用`}
          </span>
        </div>
        <span className="assistant-exec-rail-chevron" aria-hidden>
          <AppIcon name="chevron-down" size={16} />
        </span>
      </summary>
      <div className="assistant-exec-rail-body">
        <div className="assistant-exec-rounds">
          {sorted.map((tool, i) => {
            const thought = perRound[i]?.trim() ?? ''
            const isStreaming = tool.state === 'running'
            const isParallelRunning = isStreaming && runningToolCount > 1
            return (
              <details key={tool.id} className={`tool-exec-round ${tool.state}`}>
                <summary className="tool-exec-round-summary">
                  <span className="tool-exec-round-summary-text">
                    第 {i + 1} 轮
                    {hasThinkingText ? ' · 已思考' : ''}
                    {' · '}
                    <code>{tool.toolName}</code>
                  </span>
                  <span className="tool-exec-round-summary-meta">
                    {isStreaming ? (
                      <span className={`status-pill ${tool.state}`}>{getToolCallStateLabel(tool.state)}</span>
                    ) : null}
                    {isParallelRunning ? (
                      <span className="tool-parallel-pill">并行 {runningToolCount}</span>
                    ) : null}
                    <span className="tool-exec-round-chevron">
                      <AppIcon name="chevron-down" size={14} />
                    </span>
                  </span>
                </summary>
                <div className="tool-exec-round-body">
                  {thought ? <blockquote className="tool-exec-thought">{thought}</blockquote> : null}
                  <ToolRoundIoPanels toolCall={tool} onImageClick={onImageClick} />
                </div>
              </details>
            )
          })}
          {hasThinkingText && tailOnly.trim() ? (
            <details className="tool-exec-round tool-exec-round-tail done">
              <summary className="tool-exec-round-summary">
                <span className="tool-exec-round-summary-text">第 {n + 1} 轮 · 已思考</span>
                <span className="tool-exec-round-chevron">
                  <AppIcon name="chevron-down" size={14} />
                </span>
              </summary>
              <div className="tool-exec-round-body">
                <blockquote className="tool-exec-thought">{tailOnly}</blockquote>
              </div>
            </details>
          ) : null}
        </div>
      </div>
    </details>
  )
}

function TurnExecutionDetails({
  turn,
  isStreaming,
  variant = 'pills',
}: {
  turn: ConversationTurn
  isStreaming: boolean
  variant?: 'pills' | 'inline'
}) {
  useLiveNow(isStreaming, 500)
  const totalDuration = getElapsedMs(turn.createdAt, turn.completedAt, isStreaming)
  const usage = turn.usage

  if (typeof totalDuration !== 'number' && !hasUsageMetrics(usage)) {
    return null
  }

  if (variant === 'inline') {
    return (
      <div className="assistant-turn-meta answer-result-meta-inline">
        {typeof totalDuration === 'number' ? <span>已思考 {formatDurationLabel(totalDuration)}</span> : null}
        {typeof totalDuration === 'number' && hasUsageMetrics(usage) && usage ? (
          <span className="assistant-turn-meta-sep" aria-hidden>
            ·
          </span>
        ) : null}
        {hasUsageMetrics(usage) && usage ? <TokenUsageDetailPill usage={usage} variant="inline" /> : null}
      </div>
    )
  }

  return (
    <div className="answer-result-meta">
      {typeof totalDuration === 'number' ? (
        <span className="answer-result-meta-item">总耗时：{formatDurationLabel(totalDuration)}</span>
      ) : null}
      {hasUsageMetrics(usage) && usage ? <TokenUsageDetailPill usage={usage} /> : null}
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
                    <div className="skill-title-copy">
                      <h3>{skill.name}</h3>
                      <SkillDescriptionDisclosure description={skill.description} />
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
                  <div className="skill-title-copy">
                    <h3>{skill.name}</h3>
                    <SkillDescriptionDisclosure description={skill.description} />
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
  const selectedAgent = agents.find((agent) => agent.id === selectedAgentId) ?? null

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

        <div className="input-field skill-install-field new-session-model-block">
          <span>本会话模型</span>
          <label className="select-field dialog-select-field">
            <AppIcon name="zap" size={14} />
            <select
              value={selectedModelValue}
              onChange={(event) => onChangeModel(event.target.value)}
              disabled={modelOptions.length === 0}
            >
              {modelOptions.length === 0 ? (
                <option value="">暂无已配置模型</option>
              ) : (
                modelOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))
              )}
            </select>
          </label>
        </div>

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

type TaskCenterEditPageProps = {
  task: AgentTaskListItem
  editDraft: AgentTaskUpdateInput
  setEditDraft: Dispatch<SetStateAction<AgentTaskUpdateInput | null>>
  actionBusy: boolean
  onBack: () => void
  onOpenAgent: (agentId: string) => void
  onSave: () => void
  onRunNow: () => void
  onPause: () => void
  onResume: () => void
  onDelete: () => void
}

function TaskCenterEditPage({
  task,
  editDraft,
  setEditDraft,
  actionBusy,
  onBack,
  onOpenAgent,
  onSave,
  onRunNow,
  onPause,
  onResume,
  onDelete,
}: TaskCenterEditPageProps) {
  return (
    <div className="page-shell task-center-page task-edit-page">
      <header className="task-edit-page-header">
        <button type="button" className="task-edit-back" onClick={onBack} aria-label="返回任务列表">
          <AppIcon name="arrow-left" size={18} />
          <span>返回</span>
        </button>
        <div className="task-edit-page-header-main">
          <h1 className="task-edit-page-title">{task.title.trim() || '未命名任务'}</h1>
          <p className="task-edit-page-kicker">
            {formatAgentTaskStatus(task.status)} · {formatAgentTaskScheduleShort(task)}
            {task.nextRunAt ? ` · 下次 ${formatOptionalAbsoluteTime(task.nextRunAt)}` : ''}
          </p>
        </div>
      </header>

      <div className="task-center-body task-edit-body">
        <div className="task-edit-toolbar">
          <button
            type="button"
            className="outline-button task-edit-toolbar-btn"
            disabled={actionBusy || task.status === 'deleted'}
            onClick={onRunNow}
          >
            {actionBusy ? '处理中…' : '立即执行'}
          </button>
          {task.status === 'active' ? (
            <button type="button" className="outline-button task-edit-toolbar-btn" disabled={actionBusy} onClick={onPause}>
              {actionBusy ? '处理中…' : '暂停'}
            </button>
          ) : null}
          {task.status === 'paused' ? (
            <button type="button" className="outline-button task-edit-toolbar-btn" disabled={actionBusy} onClick={onResume}>
              {actionBusy ? '处理中…' : '恢复'}
            </button>
          ) : null}
          {task.status !== 'deleted' ? (
            <button type="button" className="outline-button task-edit-toolbar-btn danger" disabled={actionBusy} onClick={onDelete}>
              删除
            </button>
          ) : null}
          <button type="button" className="link-button task-edit-toolbar-link" onClick={() => onOpenAgent(task.agentId)}>
            打开智能体
          </button>
        </div>

        <section className="task-edit-section">
          <h2 className="task-edit-section-title">上下文</h2>
          <dl className="task-edit-dl">
            <div className="task-edit-dl-row">
              <dt>智能体</dt>
              <dd>{task.agentName}</dd>
            </div>
            <div className="task-edit-dl-row">
              <dt>类型</dt>
              <dd>{task.taskType === 'agent_prompt' ? 'agent_prompt（到点唤起智能体）' : 'reminder（直接提醒）'}</dd>
            </div>
            <div className="task-edit-dl-row">
              <dt>来源会话</dt>
              <dd className="task-edit-dl-mono">{task.sourceSessionId}</dd>
            </div>
            <div className="task-edit-dl-row">
              <dt>投递</dt>
              <dd>
                {task.deliveryKind} → {task.deliveryTarget}
              </dd>
            </div>
            <div className="task-edit-dl-row">
              <dt>上次执行</dt>
              <dd>{formatOptionalAbsoluteTime(task.lastRunAt)}</dd>
            </div>
          </dl>
        </section>

        <section className="task-edit-section">
          <h2 className="task-edit-section-title">调度与内容</h2>
          <div className="task-edit-form">
            <label className="task-edit-field">
              <span className="task-edit-label">任务标题</span>
              <input
                className="task-edit-control"
                value={editDraft.title}
                onChange={(event) =>
                  setEditDraft((current) => (current ? { ...current, title: event.target.value } : current))
                }
              />
            </label>
            <label className="task-edit-field">
              <span className="task-edit-label">触发方式</span>
              <select
                className="task-edit-control"
                value={editDraft.scheduleType}
                onChange={(event) =>
                  setEditDraft((current) =>
                    current
                      ? {
                          ...current,
                          scheduleType: event.target.value,
                          intervalMinutes: event.target.value === 'interval' ? current.intervalMinutes || 10 : null,
                          dailyTimes: event.target.value === 'daily_time' ? current.dailyTimes : [],
                        }
                      : current,
                  )
                }
              >
                <option value="interval">每隔若干分钟</option>
                <option value="daily_time">每天固定时间</option>
              </select>
            </label>
            {editDraft.scheduleType === 'interval' ? (
              <label className="task-edit-field">
                <span className="task-edit-label">间隔（分钟）</span>
                <input
                  className="task-edit-control"
                  type="number"
                  min={1}
                  value={editDraft.intervalMinutes ?? 10}
                  onChange={(event) =>
                    setEditDraft((current) =>
                      current
                        ? {
                            ...current,
                            intervalMinutes: Number.parseInt(event.target.value || '0', 10) || 0,
                          }
                        : current,
                    )
                  }
                />
              </label>
            ) : (
              <label className="task-edit-field">
                <span className="task-edit-label">每日时间</span>
                <input
                  className="task-edit-control"
                  value={editDraft.dailyTimes.join(', ')}
                  onChange={(event) =>
                    setEditDraft((current) =>
                      current ? { ...current, dailyTimes: parseDailyTimesInput(event.target.value) } : current,
                    )
                  }
                  placeholder="09:00, 18:30"
                />
              </label>
            )}
            <label className="task-edit-field">
              <span className="task-edit-label">时区</span>
              <input
                className="task-edit-control"
                value={editDraft.timezone}
                onChange={(event) =>
                  setEditDraft((current) => (current ? { ...current, timezone: event.target.value } : current))
                }
                placeholder="Asia/Shanghai"
              />
            </label>
            <label className="task-edit-field task-edit-field-grow">
              <span className="task-edit-label">任务内容</span>
              <textarea
                className="task-edit-control task-edit-textarea"
                rows={4}
                value={editDraft.goal}
                onChange={(event) =>
                  setEditDraft((current) => (current ? { ...current, goal: event.target.value } : current))
                }
              />
            </label>
          </div>
        </section>

        <div className="task-edit-footer">
          <button type="button" className="primary-cta task-edit-save" disabled={actionBusy} onClick={onSave}>
            {actionBusy ? '保存中…' : '保存'}
          </button>
        </div>
      </div>
    </div>
  )
}

type TasksViewProps = {
  agents: AgentRecord[]
  onOpenAgent: (agentId: string) => void
}

function TasksView({ agents, onOpenAgent }: TasksViewProps) {
  const [tasks, setTasks] = useState<AgentTaskListItem[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [actionTaskId, setActionTaskId] = useState('')
  const [search, setSearch] = useState('')
  const [agentFilter, setAgentFilter] = useState('')
  const [detailTaskId, setDetailTaskId] = useState('')
  const [editDraft, setEditDraft] = useState<AgentTaskUpdateInput | null>(null)

  const refreshTasks = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const items = await listAgentTasks(agentFilter.trim() || null)
      setTasks(items)
    } catch (fetchError) {
      setError(fetchError instanceof Error ? fetchError.message : String(fetchError))
    } finally {
      setLoading(false)
    }
  }, [agentFilter])

  useEffect(() => {
    void refreshTasks()
  }, [refreshTasks])

  const visibleTasks = useMemo(() => {
    const keyword = search.trim().toLowerCase()
    if (!keyword) {
      return tasks
    }
    return tasks.filter((task) =>
      [
        task.title,
        task.goal,
        task.intentSummary,
        task.agentName,
        task.agentId,
        task.sourceSessionId,
      ]
        .join(' ')
        .toLowerCase()
        .includes(keyword),
    )
  }, [search, tasks])

  const openTaskDetail = useCallback((task: AgentTaskListItem) => {
    setDetailTaskId(task.id)
    setEditDraft({
      title: task.title,
      goal: task.goal || task.intentSummary,
      scheduleType: task.scheduleType,
      timezone: task.timezone || 'Asia/Shanghai',
      intervalMinutes: task.intervalMinutes ?? null,
      dailyTimes: task.dailyTimes ?? [],
    })
  }, [])

  const closeTaskDetail = useCallback(() => {
    setDetailTaskId('')
    setEditDraft(null)
  }, [])

  useEffect(() => {
    if (!detailTaskId) {
      return
    }
    const onKeyDown = (event: Event) => {
      if (event instanceof KeyboardEvent && event.key === 'Escape') {
        closeTaskDetail()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [detailTaskId, closeTaskDetail])

  const detailTask = detailTaskId ? tasks.find((t) => t.id === detailTaskId) : undefined
  const taskSections = useMemo(() => groupAgentTasksByStatus(visibleTasks), [visibleTasks])

  if (detailTaskId) {
    if (!detailTask || !editDraft) {
      return (
        <div className="page-shell task-center-page task-edit-page">
          <header className="task-edit-page-header">
            <button type="button" className="task-edit-back" onClick={closeTaskDetail} aria-label="返回">
              <AppIcon name="arrow-left" size={18} />
              <span>返回</span>
            </button>
            <h1 className="task-edit-page-title">任务不可用</h1>
          </header>
          <div className="task-center-body task-edit-body">
            <p className="task-linear-empty-hint">该任务可能已被删除或不在当前筛选结果中。</p>
            <button type="button" className="outline-button" onClick={closeTaskDetail}>
              返回列表
            </button>
          </div>
        </div>
      )
    }

    const actionBusy = actionTaskId === detailTask.id

    return (
      <TaskCenterEditPage
        task={detailTask}
        editDraft={editDraft}
        setEditDraft={setEditDraft}
        actionBusy={actionBusy}
        onBack={closeTaskDetail}
        onOpenAgent={onOpenAgent}
        onSave={() => {
          setActionTaskId(detailTask.id)
          void updateAgentTask(detailTask.id, editDraft)
            .then(() => refreshTasks())
            .catch((taskError: unknown) => {
              setError(taskError instanceof Error ? taskError.message : String(taskError))
            })
            .finally(() => setActionTaskId(''))
        }}
        onRunNow={() => {
          setActionTaskId(detailTask.id)
          void runAgentTaskNow(detailTask.id)
            .then(refreshTasks)
            .catch((runError: unknown) => {
              setError(runError instanceof Error ? runError.message : String(runError))
            })
            .finally(() => setActionTaskId(''))
        }}
        onPause={() => {
          setActionTaskId(detailTask.id)
          void pauseAgentTask(detailTask.id)
            .then(refreshTasks)
            .catch((taskError: unknown) => {
              setError(taskError instanceof Error ? taskError.message : String(taskError))
            })
            .finally(() => setActionTaskId(''))
        }}
        onResume={() => {
          setActionTaskId(detailTask.id)
          void resumeAgentTask(detailTask.id)
            .then(refreshTasks)
            .catch((taskError: unknown) => {
              setError(taskError instanceof Error ? taskError.message : String(taskError))
            })
            .finally(() => setActionTaskId(''))
        }}
        onDelete={() => {
          setActionTaskId(detailTask.id)
          void deleteAgentTask(detailTask.id)
            .then(async () => {
              closeTaskDetail()
              await refreshTasks()
            })
            .catch((taskError: unknown) => {
              setError(taskError instanceof Error ? taskError.message : String(taskError))
            })
            .finally(() => setActionTaskId(''))
        }}
      />
    )
  }

  return (
    <div className="page-shell task-center-page task-linear-page">
      <header className="page-header task-linear-page-toolbar">
        <div className="task-linear-page-toolbar-text">
          <h1>任务中心</h1>
          <p className="task-linear-page-sub">由智能体在对话中创建的定时任务，按状态分组；点按一行进入编辑。</p>
          {!loading && visibleTasks.length === 0 ? <p className="task-center-header-hint">还没有定时任务</p> : null}
        </div>
        <button
          type="button"
          className="primary-cta task-linear-toolbar-cta"
          onClick={() => void refreshTasks()}
          disabled={loading}
        >
          <AppIcon name="refresh" size={16} />
          <span>{loading ? '刷新中…' : '刷新'}</span>
        </button>
      </header>

      <div className="task-center-body task-linear-body">
        <div className="task-linear-filters">
          <label className="search-field task-linear-filter-search">
            <AppIcon name="search" size={18} />
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="按标题、内容、会话、智能体筛选…"
            />
          </label>
          <div className="task-linear-filter-select-wrap">
            <select
              className="task-edit-control task-linear-filter-select"
              value={agentFilter}
              onChange={(event) => setAgentFilter(event.target.value)}
              aria-label="按智能体筛选"
            >
              <option value="">全部智能体</option>
              {agents.map((agent) => (
                <option key={agent.id} value={agent.id}>
                  {agent.name}
                </option>
              ))}
            </select>
          </div>
        </div>

        {error ? (
          <div className="skills-feedback error agent-feedback inline task-center-error">
            <span>{error}</span>
          </div>
        ) : null}

        {visibleTasks.length > 0 ? (
          <div className="task-linear-table-outer">
            {taskSections.map((section) => (
              <section key={section.key} className="task-linear-group">
                <div className="task-linear-group-bar">
                  <span className="task-linear-group-title">{section.label}</span>
                  <span className="task-linear-group-count">{section.items.length}</span>
                </div>
                <div className="task-linear-table-wrap" role="grid" aria-label={`${section.label}任务`}>
                  <div className="task-linear-thead" role="row">
                    <div className="task-linear-th task-linear-col-name" role="columnheader">
                      任务
                    </div>
                    <div className="task-linear-th task-linear-col-schedule" role="columnheader">
                      调度
                    </div>
                    <div className="task-linear-th task-linear-col-status" role="columnheader">
                      状态
                    </div>
                    <div className="task-linear-th task-linear-col-agent" role="columnheader">
                      智能体
                    </div>
                    <div className="task-linear-th task-linear-col-last" role="columnheader">
                      上次执行
                    </div>
                  </div>
                  {section.items.map((task) => {
                    const subtitle = (task.goal || task.intentSummary || '').trim()
                    const subtitleShort = subtitle.length > 72 ? `${subtitle.slice(0, 72)}…` : subtitle
                    return (
                      <button
                        key={task.id}
                        type="button"
                        className="task-linear-row"
                        onClick={() => openTaskDetail(task)}
                      >
                        <div className="task-linear-col task-linear-col-name">
                          <span className="task-linear-row-title">{task.title.trim() || '未命名任务'}</span>
                          {subtitleShort ? <span className="task-linear-row-sub">{subtitleShort}</span> : null}
                        </div>
                        <div className="task-linear-col task-linear-col-schedule">
                          <span className="task-linear-row-primary">{formatAgentTaskScheduleShort(task)}</span>
                          {task.nextRunAt ? (
                            <span className="task-linear-row-sub">下次 {formatOptionalAbsoluteTime(task.nextRunAt)}</span>
                          ) : null}
                        </div>
                        <div className="task-linear-col task-linear-col-status">
                          <span className="task-linear-status-pill">{formatAgentTaskStatus(task.status)}</span>
                        </div>
                        <div className="task-linear-col task-linear-col-agent">
                          <span className="task-linear-row-primary">{task.agentName}</span>
                        </div>
                        <div className="task-linear-col task-linear-col-last">
                          <span className="task-linear-row-primary">{formatOptionalAbsoluteTime(task.lastRunAt)}</span>
                        </div>
                      </button>
                    )
                  })}
                </div>
              </section>
            ))}
          </div>
        ) : (
          <section className="task-center-panel task-center-panel-empty task-linear-empty">
            {loading ? (
              <p className="task-center-empty-title">正在读取任务…</p>
            ) : (
              <p className="task-center-empty-desc">
                先在聊天里对智能体说「每 10 分钟…」或「每天 9 点…」，保存后这里会出现记录。
              </p>
            )}
          </section>
        )}
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
                <article
                  key={skill.id}
                  className={`agent-skill-option ${active ? 'active' : ''}`}
                >
                  <div className="agent-skill-option-copy">
                    <strong>{skill.name}</strong>
                    <SkillDescriptionDisclosure description={skill.description} className="skill-description-inset" />
                    <small>
                      {formatInstalledSkillScopeLabel(skill.scope)} · {formatInstalledSkillSource(skill)}
                    </small>
                  </div>
                  <div className="agent-skill-option-actions">
                    <button
                      type="button"
                      className="agent-skill-option-action"
                      onClick={() => onToggleSkill(skill.id)}
                    >
                      {active ? '已添加' : '添加技能'}
                    </button>
                  </div>
                </article>
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

function buildAgentPeerSnippet(
  info: PeerGatewayInfo,
  savedAgentId: string | null,
  peerSecret: string,
): string {
  const lines: string[] = []
  lines.push('── NineClaw 智能体对等接入说明 ──')
  lines.push('')
  if (info.envOverrideActive) {
    lines.push('【说明】监听地址由环境变量 NINECLAW_PEER_BIND 决定，与应用内「设置 → 通用」中的端口无关。')
    lines.push('')
  }
  if (!info.enabled) {
    lines.push(
      '【注意】当前未监听对等 HTTP。请在 NineClaw「设置 → 通用 → 对等 HTTP（虾）」中启用并配置端口（默认 1052），或设置环境变量 NINECLAW_PEER_BIND。',
    )
    lines.push('')
  }
  if (info.listenAddress) {
    lines.push(`进程监听：${info.listenAddress}`)
  }
  if (info.publicBaseUrl) {
    lines.push(`API 接口地址：${info.publicBaseUrl}`)
  }
  if (info.inboundUrl) {
    lines.push(`入站接口（POST）：${info.inboundUrl}`)
  }
  if (info.healthUrl) {
    lines.push(`健康检查（GET）：${info.healthUrl}`)
  }
  lines.push('')
  lines.push('鉴权：请求头 Authorization: Bearer <本智能体入站密钥>')
  lines.push('')
  if (savedAgentId) {
    lines.push(`本智能体 ID（JSON 字段 toAgentId）：${savedAgentId}`)
  } else {
    lines.push('本智能体 ID：请先保存智能体，保存后即可在此看到稳定 ID。')
  }
  lines.push(
    peerSecret
      ? `本智能体入站密钥：${peerSecret}`
      : '本智能体入站密钥：（保存智能体后由系统生成；或在「机器人 → 虾/对等」查看 / 重新生成）',
  )
  lines.push('')
  lines.push('请求 JSON 示例：')
  lines.push(
    JSON.stringify(
      {
        protocol: 'nineclaw-peer',
        version: 1,
        fromAgentId: '<我的名字>',
        toAgentId: savedAgentId || '<保存后替换为本智能体ID>',
        threadId: '同一会话固定字符串',
        text: '你好',
      },
      null,
      2,
    ),
  )
  lines.push('')
  lines.push('同步成功时响应示例（统一信封，字段均为 camelCase）：')
  lines.push(
    JSON.stringify(
      {
        protocol: 'nineclaw-peer',
        version: 1,
        ok: true,
        kind: 'inboundReply',
        fromAgentId: '<我的名字>',
        toAgentId: savedAgentId || '<本智能体ID>',
        threadId: '同一会话固定字符串',
        reply: '助手回复正文',
      },
      null,
      2,
    ),
  )
  return lines.join('\n')
}

type AgentEditorDialogProps = {
  agentDraft: AgentInput | null
  agentDeleteConfirmOpen: boolean
  agentDeleteConfirmText: string
  agentFormError: string
  agentFormNotice: string
  agentRefreshing: boolean
  agentSaving: boolean
  allSkills: InstalledSkillItem[]
  defaultAgentId: string
  mode: 'create' | 'edit'
  modelOptions: { value: string; label: string }[]
  onClose: () => void
  onCloseDeleteAgentDialog: () => void
  onConfirmDeleteAgent: () => void
  onCreateAgent: () => void
  onDraftChange: (updates: Partial<AgentInput>) => void
  onDeleteConfirmTextChange: (value: string) => void
  onRefreshAgent: () => void
  onOpenWorkspace: () => void
  onOpenSkillPicker: () => void
  onRequestDeleteAgent: () => void
  onSaveAgent: () => void
  onSetDefaultAgent: () => void
  onToggleSkill: (skillId: string) => void
  selectedAgent: AgentRecord | null
  /** 已保存智能体的 id；新建为空字符串 */
  managedAgentId: string
}

function AgentEditorDialog({
  agentDraft,
  agentDeleteConfirmOpen,
  agentDeleteConfirmText,
  agentFormError,
  agentFormNotice,
  agentRefreshing,
  agentSaving,
  allSkills,
  defaultAgentId,
  mode,
  modelOptions,
  onClose,
  onCloseDeleteAgentDialog,
  onConfirmDeleteAgent,
  onCreateAgent,
  onDraftChange,
  onDeleteConfirmTextChange,
  onRefreshAgent,
  onOpenWorkspace,
  onOpenSkillPicker,
  onRequestDeleteAgent,
  onSaveAgent,
  onSetDefaultAgent,
  onToggleSkill,
  selectedAgent,
  managedAgentId,
}: AgentEditorDialogProps) {
  const [peerGatewayInfo, setPeerGatewayInfo] = useState<PeerGatewayInfo | null>(null)
  const [peerGatewayLoadError, setPeerGatewayLoadError] = useState('')
  const [peerSnippetCopied, setPeerSnippetCopied] = useState(false)
  const [agentTasks, setAgentTasks] = useState<AgentTaskListItem[]>([])
  const [agentTasksLoading, setAgentTasksLoading] = useState(false)
  const [agentTasksError, setAgentTasksError] = useState('')
  const [agentTaskActionId, setAgentTaskActionId] = useState('')
  const [advancedOpen, setAdvancedOpen] = useState(mode === 'edit')
  const advancedPresetKeyRef = useRef('')

  useEffect(() => {
    let cancelled = false
    setPeerGatewayLoadError('')
    void getPeerGatewayInfo()
      .then((value) => {
        if (!cancelled) {
          setPeerGatewayInfo(value)
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setPeerGatewayLoadError(error instanceof Error ? error.message : String(error))
        }
      })
    return () => {
      cancelled = true
    }
  }, [managedAgentId])

  useEffect(() => {
    const targetAgentId = selectedAgent?.id?.trim()
    if (!targetAgentId || mode !== 'edit') {
      setAgentTasks([])
      setAgentTasksError('')
      setAgentTasksLoading(false)
      return
    }

    let cancelled = false
    setAgentTasksLoading(true)
    setAgentTasksError('')
    void listAgentTasks(targetAgentId)
      .then((items) => {
        if (!cancelled) {
          setAgentTasks(items)
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setAgentTasksError(error instanceof Error ? error.message : String(error))
        }
      })
      .finally(() => {
        if (!cancelled) {
          setAgentTasksLoading(false)
        }
      })

    return () => {
      cancelled = true
    }
  }, [mode, selectedAgent?.id])

  const refreshAgentTasks = useCallback(async () => {
    const targetAgentId = selectedAgent?.id?.trim()
    if (!targetAgentId || mode !== 'edit') {
      setAgentTasks([])
      return
    }
    setAgentTasksLoading(true)
    setAgentTasksError('')
    try {
      const items = await listAgentTasks(targetAgentId)
      setAgentTasks(items)
    } catch (error) {
      setAgentTasksError(error instanceof Error ? error.message : String(error))
    } finally {
      setAgentTasksLoading(false)
    }
  }, [mode, selectedAgent?.id])

  const peerDraftConfigs = createAgentBotConfigState(agentDraft?.botConfigs)
  const peerDraftSecret =
    peerDraftConfigs.peer?.peerSharedSecret?.trim() || peerDraftConfigs.peer?.clientSecret?.trim() || ''
  const savedAgentIdForPeer = managedAgentId.trim() || null
  const peerSnippetText =
    peerGatewayInfo !== null
      ? buildAgentPeerSnippet(peerGatewayInfo, savedAgentIdForPeer, peerDraftSecret)
      : ''

  const selectedModelValue =
    agentDraft?.defaultProviderId.trim() && agentDraft?.defaultModel.trim()
      ? sessionLlmEncode(agentDraft.defaultProviderId, agentDraft.defaultModel)
      : ''
  const generatedSummary = buildAutoAgentSummary(agentDraft?.description ?? '', agentDraft?.name ?? '')
  const summaryOverride = agentDraft?.summary.trim() ?? ''
  const hasSummaryOverride = Boolean(summaryOverride) && summaryOverride !== generatedSummary
  const missingSkillIds = (agentDraft?.skillIds ?? []).filter((skillId) => !allSkills.some((skill) => skill.id === skillId))
  const mountedSkills = allSkills.filter((skill) => agentDraft?.skillIds.includes(skill.id) ?? false)
  const editorAccent = getAgentColor(
    selectedAgent ?? { id: 'draft', name: agentDraft?.name || '智能体', accentColor: agentDraft?.accentColor },
  )
  const heartbeatConfig = normalizeHeartbeatConfig(agentDraft?.heartbeatConfig)
  const heartbeatTasks = heartbeatConfig.tasks
  const heartbeatSchedules = heartbeatConfig.schedules
  const updateHeartbeatConfig = (updater: (current: AgentHeartbeatConfig) => AgentHeartbeatConfig) => {
    onDraftChange({ heartbeatConfig: updater(normalizeHeartbeatConfig(agentDraft?.heartbeatConfig)) })
  }
  const updateHeartbeatTask = (taskId: string, updates: Partial<AgentHeartbeatTask>) => {
    updateHeartbeatConfig((current) => ({
      ...current,
      tasks: current.tasks.map((task) => (task.id === taskId ? { ...task, ...updates } : task)),
    }))
  }
  const removeHeartbeatTask = (taskId: string) => {
    updateHeartbeatConfig((current) => ({
      ...current,
      tasks: current.tasks.filter((task) => task.id !== taskId),
      schedules: current.schedules.map((schedule) =>
        schedule.taskId === taskId ? { ...schedule, taskId: '' } : schedule,
      ),
    }))
  }
  const addHeartbeatTask = () => {
    updateHeartbeatConfig((current) => ({
      ...current,
      tasks: [...current.tasks, createEmptyHeartbeatTask()],
    }))
  }
  const updateHeartbeatSchedule = (scheduleId: string, updates: Partial<AgentHeartbeatSchedule>) => {
    updateHeartbeatConfig((current) => ({
      ...current,
      schedules: current.schedules.map((schedule) => (schedule.id === scheduleId ? { ...schedule, ...updates } : schedule)),
    }))
  }
  const removeHeartbeatSchedule = (scheduleId: string) => {
    updateHeartbeatConfig((current) => ({
      ...current,
      schedules: current.schedules.filter((schedule) => schedule.id !== scheduleId),
    }))
  }
  const addHeartbeatSchedule = () => {
    updateHeartbeatConfig((current) => ({
      ...current,
      schedules: [
        ...current.schedules,
        {
          ...createEmptyHeartbeatSchedule(),
          taskId: current.tasks[0]?.id ?? '',
        },
      ],
    }))
  }
  const hasAdvancedSettings =
    hasSummaryOverride ||
    (agentDraft ? agentDraft.executionMode !== 'single' : false) ||
    Boolean(agentDraft?.systemPrompt.trim()) ||
    heartbeatTasks.length > 0 ||
    heartbeatSchedules.length > 0 ||
    Boolean(peerDraftSecret)

  useEffect(() => {
    const nextKey = `${mode}:${managedAgentId || 'draft'}`
    if (advancedPresetKeyRef.current === nextKey) {
      return
    }
    advancedPresetKeyRef.current = nextKey
    setAdvancedOpen(mode === 'edit' && hasAdvancedSettings)
  }, [hasAdvancedSettings, managedAgentId, mode])

  useEffect(() => {
    if (
      agentFormError &&
      (agentFormError.includes('任务') ||
        agentFormError.includes('规则') ||
        agentFormError.includes('接收用户') ||
        agentFormError.includes('时区'))
    ) {
      setAdvancedOpen(true)
    }
  }, [agentFormError])

  if (!agentDraft) {
    return null
  }

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

          <div className="agent-editor-header-actions">
            <button
              type="button"
              className="outline-button"
              onClick={onRefreshAgent}
              disabled={agentRefreshing || agentSaving || mode !== 'edit' || !selectedAgent}
            >
              <AppIcon name="refresh" size={16} />
              <span>{agentRefreshing ? '刷新中…' : '刷新'}</span>
            </button>

            <button type="button" className="icon-button subtle" onClick={onClose} aria-label="关闭智能体编辑器">
              <AppIcon name="close" size={18} />
            </button>
          </div>
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
	                      ? '先填名字、角色说明、默认模型和挂载技能，就能建出一个可直接使用的智能体。更细的运行参数都放在高级设置里。'
	                      : selectedAgent?.summary ?? '先改角色说明、模型和技能；需要时再展开高级设置，调整运行参数和自动化能力。'}
	                  </p>
	                </div>
	              </div>

	              <div className="agent-hero-pills">
	                <span className="agent-hero-pill">{mode === 'create' ? '未保存' : '用户智能体'}</span>
	                {agentDraft.executionMode !== 'single' ? (
	                  <span className="agent-hero-pill">{formatAgentExecutionModeLabel(agentDraft.executionMode)}</span>
	                ) : null}
	                <span className="agent-hero-pill">{agentDraft.skillIds.length} 个挂载技能</span>
	                {selectedAgent ? <span className="agent-hero-pill">ID: {selectedAgent.id}</span> : null}
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
	                  <strong>快速创建</strong>
	                  <p>常用配置只保留名字、角色说明、默认模型和技能。列表摘要会自动从角色说明提炼，避免重复填写。</p>
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
	                    {modelOptions.length === 0 ? (
	                      <option value="">暂无已配置模型</option>
	                    ) : (
	                      modelOptions.map((option) => (
	                        <option key={option.value} value={option.value}>
	                          {option.label}
	                        </option>
	                      ))
	                    )}
	                  </select>
	                </label>
	              </div>

	              <label className="input-field agent-field-full">
	                <span>角色说明</span>
	                <textarea
	                  value={agentDraft.description}
	                  onChange={(event) => onDraftChange({ description: event.target.value })}
	                  rows={5}
	                  placeholder="说明这个智能体负责什么、擅长什么、回答风格和边界。"
	                />
	              </label>

	              <div className="agent-helper-copy">
	                <strong>列表摘要将自动生成</strong>
	                <span>
	                  当前预览：{generatedSummary || '输入角色说明后会自动生成'}。如果你想手动改写，可以在高级设置里覆盖。
	                </span>
	              </div>
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
                    <article
                      key={skill.id}
                      className="agent-mounted-skill"
                    >
                      <div className="agent-mounted-skill-copy">
                        <strong>{skill.name}</strong>
                        <SkillDescriptionDisclosure description={skill.description} className="skill-description-inset" />
                        <small>
                          {formatInstalledSkillScopeLabel(skill.scope)} · {formatInstalledSkillSource(skill)}
                        </small>
                      </div>
                      <button
                        type="button"
                        className="agent-mounted-skill-action"
                        onClick={() => onToggleSkill(skill.id)}
                      >
                        <AppIcon name="close" size={16} />
                        <span>移除</span>
                      </button>
                    </article>
                  ))}

                  {missingSkillIds.map((skillId) => (
                    <article key={skillId} className="agent-mounted-skill missing">
                      <div className="agent-mounted-skill-copy">
                        <strong>{skillId}</strong>
                        <small>本地未找到，点击即可移除</small>
                      </div>
                      <button
                        type="button"
                        className="agent-mounted-skill-action"
                        onClick={() => onToggleSkill(skillId)}
                      >
                        <AppIcon name="close" size={16} />
                        <span>移除</span>
                      </button>
                    </article>
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
	              <button
	                type="button"
	                className={`agent-advanced-toggle ${advancedOpen ? 'open' : ''}`}
	                onClick={() => setAdvancedOpen((current) => !current)}
	                aria-expanded={advancedOpen}
	              >
	                <div className="agent-advanced-toggle-copy">
	                  <strong>高级设置</strong>
	                  <span>摘要覆盖、执行模式、高级指令、对等 HTTP 对接、心跳任务都收在这里。</span>
	                </div>
	                <div className="agent-advanced-toggle-meta">
	                  <span>{hasAdvancedSettings ? '已配置' : '可选'}</span>
	                  <AppIcon name="chevron-down" size={18} />
	                </div>
	              </button>

	              {advancedOpen ? (
	                <div className="agent-advanced-stack">
	                  <div className="agent-subsection">
	                    <div className="agent-subsection-header">
	                      <div>
	                        <strong>运行与路由</strong>
	                        <p>默认模型来自智能体配置，但聊天窗口里仍支持用户按当前会话临时切换，不会回写智能体默认值。</p>
	                      </div>
	                    </div>

	                    <div className="agent-form-grid">
	                      <label className="input-field">
	                        <span>列表摘要覆盖（可选）</span>
	                        <input
	                          value={agentDraft.summary}
	                          onChange={(event) => onDraftChange({ summary: event.target.value })}
	                          placeholder={generatedSummary || '留空时自动生成'}
	                        />
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

	                    <div className="agent-helper-copy subtle">
	                      <strong>自动摘要预览</strong>
	                      <span>{generatedSummary || '输入角色说明后生成摘要。'}</span>
	                    </div>

	                    <label className="input-field agent-field-full">
	                      <span>高级指令（可选）</span>
	                      <textarea
	                        value={agentDraft.systemPrompt}
	                        onChange={(event) => onDraftChange({ systemPrompt: event.target.value })}
	                        rows={6}
	                        placeholder="补充额外执行约束、回答方式或边界要求。留空时会根据名字和角色说明自动生成角色上下文。"
	                      />
	                    </label>
	                  </div>

	                  <div className="agent-peer-integration-card">
	                    <div className="agent-peer-integration-head">
	                      <strong>对等 HTTP（虾）对接</strong>
	                      <span>全应用共用一个监听端口；下列地址与一键说明会包含本智能体的密钥与 ID。</span>
	                    </div>
	                    {peerGatewayLoadError ? (
	                      <div className="skills-feedback error agent-feedback inline">
	                        <span>读取网关信息失败：{peerGatewayLoadError}</span>
	                      </div>
	                    ) : null}
	                    {peerGatewayInfo ? (
	                      <>
	                        <div className="agent-peer-api-grid">
	                          <label className="input-field">
	                            <span>监听地址（NINECLAW_PEER_BIND）</span>
	                            <input
	                              readOnly
	                              value={
	                                peerGatewayInfo.enabled && peerGatewayInfo.listenAddress
	                                  ? peerGatewayInfo.listenAddress
	                                  : '未配置（未监听）'
	                              }
	                            />
	                          </label>
	                          <label className="input-field">
	                            <span>入站 API（POST）</span>
	                            <input readOnly value={peerGatewayInfo.inboundUrl ?? '—'} />
	                          </label>
	                          <label className="input-field">
	                            <span>健康检查（GET）</span>
	                            <input readOnly value={peerGatewayInfo.healthUrl ?? '—'} />
	                          </label>
	                        </div>
	                        <div className="agent-peer-snippet-toolbar">
	                          <span className="agent-peer-snippet-label">给对方的一键说明（含密钥）</span>
	                          <button
	                            type="button"
	                            className="outline-button"
	                            onClick={() => {
	                              void navigator.clipboard.writeText(peerSnippetText).then(() => {
	                                setPeerSnippetCopied(true)
	                                window.setTimeout(() => setPeerSnippetCopied(false), 2000)
	                              })
	                            }}
	                          >
	                            {peerSnippetCopied ? (
	                              <>
	                                <Check size={16} />
	                                <span>已复制</span>
	                              </>
	                            ) : (
	                              <>
	                                <Copy size={16} />
	                                <span>复制全文</span>
	                              </>
	                            )}
	                          </button>
	                        </div>
	                        <pre className="agent-peer-snippet-pre">{peerSnippetText}</pre>
	                      </>
	                    ) : (
	                      <div className="agent-workspace-hint">
	                        <span>正在读取对等网关信息…</span>
	                      </div>
	                    )}
	                  </div>

	                  <div className="agent-subsection">
	                    <div className="agent-subsection-header">
	                      <div>
	                        <strong>自然语言定时任务</strong>
	                        <p>这里展示用户在聊天里直接对这个智能体创建的真实任务。它们会带上当前智能体 ID，并由 scheduler 定时触发。</p>
	                      </div>

	                      <div className="agent-inline-actions">
	                        <button type="button" className="outline-button" onClick={() => void refreshAgentTasks()} disabled={agentTasksLoading}>
	                          <AppIcon name="refresh" size={16} />
	                          <span>{agentTasksLoading ? '刷新中…' : '刷新列表'}</span>
	                        </button>
	                      </div>
	                    </div>

	                    {agentTasksError ? (
	                      <div className="skills-feedback error agent-feedback inline">
	                        <span>读取定时任务失败：{agentTasksError}</span>
	                      </div>
	                    ) : null}

	                    {agentTasks.length > 0 ? (
	                      <div className="agent-automation-list">
	                        {agentTasks.map((task) => {
	                          const actionBusy = agentTaskActionId === task.id
	                          return (
	                            <div key={task.id} className="agent-automation-card">
	                              <div className="agent-automation-card-header">
	                                <div>
	                                  <strong>{task.title}</strong>
	                                  <span>{formatAgentTaskSchedule(task)} · {formatAgentTaskStatus(task.status)}</span>
	                                </div>
	                                <div className="agent-inline-actions">
                                  <button
                                    type="button"
                                    className="outline-button"
                                    disabled={actionBusy || task.status === 'deleted'}
                                    onClick={() => {
                                      setAgentTaskActionId(task.id)
                                      void runAgentTaskNow(task.id)
                                        .then(refreshAgentTasks)
                                        .catch((error: unknown) => {
                                          setAgentTasksError(error instanceof Error ? error.message : String(error))
                                        })
                                        .finally(() => setAgentTaskActionId(''))
                                    }}
                                  >
                                    <span>{actionBusy ? '处理中…' : '立即执行'}</span>
                                  </button>
	                                  {task.status === 'active' ? (
	                                    <button
	                                      type="button"
	                                      className="outline-button"
	                                      disabled={actionBusy}
	                                      onClick={() => {
	                                        setAgentTaskActionId(task.id)
	                                        void pauseAgentTask(task.id)
	                                          .then(refreshAgentTasks)
	                                          .catch((error: unknown) => {
	                                            setAgentTasksError(error instanceof Error ? error.message : String(error))
	                                          })
	                                          .finally(() => setAgentTaskActionId(''))
	                                      }}
	                                    >
	                                      <span>{actionBusy ? '处理中…' : '暂停'}</span>
	                                    </button>
	                                  ) : task.status === 'paused' ? (
	                                    <button
	                                      type="button"
	                                      className="outline-button"
	                                      disabled={actionBusy}
	                                      onClick={() => {
	                                        setAgentTaskActionId(task.id)
	                                        void resumeAgentTask(task.id)
	                                          .then(refreshAgentTasks)
	                                          .catch((error: unknown) => {
	                                            setAgentTasksError(error instanceof Error ? error.message : String(error))
	                                          })
	                                          .finally(() => setAgentTaskActionId(''))
	                                      }}
	                                    >
	                                      <span>{actionBusy ? '处理中…' : '恢复'}</span>
	                                    </button>
	                                  ) : null}
	                                  {task.status !== 'deleted' ? (
	                                    <button
	                                      type="button"
	                                      className="icon-button subtle"
	                                      disabled={actionBusy}
	                                      onClick={() => {
	                                        setAgentTaskActionId(task.id)
	                                        void deleteAgentTask(task.id)
	                                          .then(refreshAgentTasks)
	                                          .catch((error: unknown) => {
	                                            setAgentTasksError(error instanceof Error ? error.message : String(error))
	                                          })
	                                          .finally(() => setAgentTaskActionId(''))
	                                      }}
	                                    >
	                                      <AppIcon name="trash" size={16} />
	                                    </button>
	                                  ) : null}
	                                </div>
	                              </div>

	                              <div className="agent-workspace-hint">
	                                <span>创建智能体：{task.agentName} ({task.agentId})</span>
	                                <span>类型：{task.taskType === 'agent_prompt' ? 'agent_prompt · 到点后再唤起智能体' : 'reminder · 直接提醒'}</span>
	                                <span>来源会话：{task.sourceSessionId}</span>
	                                <span>投递：{task.deliveryKind} → {task.deliveryTarget}</span>
	                                <span>上次执行：{formatOptionalAbsoluteTime(task.lastRunAt)}</span>
	                              </div>

	                              <label className="input-field agent-field-full">
	                                <span>任务内容</span>
	                                <textarea value={task.goal || task.intentSummary} readOnly rows={3} />
	                              </label>
	                            </div>
	                          )
	                        })}
	                      </div>
	                    ) : (
	                      <div className="agent-empty-block">
	                        <strong>{agentTasksLoading ? '正在读取任务…' : '还没有自然语言创建的任务'}</strong>
	                        <span>先在聊天里对这个智能体说“每 10 分钟…”或“每天 9 点…”，这里就会出现对应记录。</span>
	                      </div>
	                    )}
	                  </div>

	                  <div className="agent-subsection">
	                    <div className="agent-subsection-header">
	                      <div>
	                        <strong>心跳与任务</strong>
	                        <p>给这个智能体配置定时提醒和可执行任务。规则到点后会自动通过绑定的 IM 通道给目标用户发消息，shell 任务会先执行程序，再推送结果。</p>
	                      </div>

	                      <div className="agent-inline-actions">
	                        <button type="button" className="outline-button" onClick={addHeartbeatTask}>
	                          <AppIcon name="plus" size={16} />
	                          <span>添加任务</span>
	                        </button>
	                        <button type="button" className="outline-button" onClick={addHeartbeatSchedule}>
	                          <AppIcon name="clock" size={16} />
	                          <span>添加规则</span>
	                        </button>
	                      </div>
	                    </div>

	                    <div className="agent-form-grid">
	                      <label className="input-field">
	                        <span>时区</span>
	                        <input
	                          value={heartbeatConfig.timezone}
	                          onChange={(event) =>
	                            updateHeartbeatConfig((current) => ({
	                              ...current,
	                              timezone: event.target.value,
	                            }))
	                          }
	                          placeholder="Asia/Shanghai"
	                        />
	                      </label>
	                    </div>

	                    <div className="agent-subsection">
	                      <div className="agent-subsection-header">
	                        <div>
	                          <strong>任务</strong>
	                          <p>`notify` 只负责提醒，`shell` 会执行命令后把结果发给用户。</p>
	                        </div>
	                      </div>

	                      {heartbeatTasks.length > 0 ? (
	                        <div className="agent-automation-list">
	                          {heartbeatTasks.map((task, index) => (
	                            <div key={task.id} className="agent-automation-card">
	                              <div className="agent-automation-card-header">
	                                <div>
	                                  <strong>{task.name || `任务 ${index + 1}`}</strong>
	                                  <span>{task.taskType === 'shell' ? '执行程序并回推结果' : '纯文本提醒'}</span>
	                                </div>
	                                <button type="button" className="icon-button subtle" onClick={() => removeHeartbeatTask(task.id)}>
	                                  <AppIcon name="trash" size={16} />
	                                </button>
	                              </div>

	                              <div className="agent-form-grid">
	                                <label className="input-field">
	                                  <span>任务名称</span>
	                                  <input
	                                    value={task.name}
	                                    onChange={(event) => updateHeartbeatTask(task.id, { name: event.target.value })}
	                                    placeholder="例如：早间播报"
	                                  />
	                                </label>

	                                <label className="input-field">
	                                  <span>任务类型</span>
	                                  <select
	                                    value={task.taskType}
	                                    onChange={(event) =>
	                                      updateHeartbeatTask(task.id, {
	                                        taskType: event.target.value === 'shell' ? 'shell' : 'notify',
	                                      })
	                                    }
	                                  >
	                                    <option value="notify">notify · 纯提醒</option>
	                                    <option value="shell">shell · 先执行程序</option>
	                                  </select>
	                                </label>
	                              </div>

	                              <label className="input-field agent-field-full">
	                                <span>任务说明</span>
	                                <textarea
	                                  value={task.description}
	                                  onChange={(event) => updateHeartbeatTask(task.id, { description: event.target.value })}
	                                  rows={3}
	                                  placeholder="说明这个任务在做什么，例如：每天 8 点推送昨晚抓取的数据摘要"
	                                />
	                              </label>

	                              {task.taskType === 'shell' ? (
	                                <>
	                                  <label className="input-field agent-field-full">
	                                    <span>执行命令</span>
	                                    <input
	                                      value={task.command}
	                                      onChange={(event) => updateHeartbeatTask(task.id, { command: event.target.value })}
	                                      placeholder="例如：python3 scripts/fetch_daily_report.py"
	                                    />
	                                  </label>

	                                  <div className="agent-form-grid">
	                                    <label className="input-field">
	                                      <span>工作目录</span>
	                                      <input
	                                        value={task.workingDirectory}
	                                        onChange={(event) => updateHeartbeatTask(task.id, { workingDirectory: event.target.value })}
	                                        placeholder="留空时使用 agents/<agent-id>/"
	                                      />
	                                    </label>

	                                    <label className="input-field">
	                                      <span>超时秒数</span>
	                                      <input
	                                        type="number"
	                                        min={10}
	                                        step={10}
	                                        value={task.timeoutSec}
	                                        onChange={(event) =>
	                                          updateHeartbeatTask(task.id, {
	                                            timeoutSec: Number.parseInt(event.target.value || '180', 10) || 180,
	                                          })
	                                        }
	                                      />
	                                    </label>
	                                  </div>
	                                </>
	                              ) : null}

	                              <label className="input-field agent-field-full">
	                                <span>消息模板</span>
	                                <textarea
	                                  value={task.messageTemplate}
	                                  onChange={(event) => updateHeartbeatTask(task.id, { messageTemplate: event.target.value })}
	                                  rows={4}
	                                  placeholder={'留空时使用系统默认文案。可用变量：{{agent_name}} {{task_name}} {{schedule_name}} {{now}} {{stdout}} {{stderr}} {{exit_code}}'}
	                                />
	                              </label>

	                              <div className="agent-toggle-row">
	                                <label className="agent-check">
	                                  <input
	                                    type="checkbox"
	                                    checked={task.enabled}
	                                    onChange={(event) => updateHeartbeatTask(task.id, { enabled: event.target.checked })}
	                                  />
	                                  <span>启用任务</span>
	                                </label>

	                                {task.taskType === 'shell' ? (
	                                  <>
	                                    <label className="agent-check">
	                                      <input
	                                        type="checkbox"
	                                        checked={task.notifyOnSuccess}
	                                        onChange={(event) => updateHeartbeatTask(task.id, { notifyOnSuccess: event.target.checked })}
	                                      />
	                                      <span>成功后发消息</span>
	                                    </label>

	                                    <label className="agent-check">
	                                      <input
	                                        type="checkbox"
	                                        checked={task.notifyOnFailure}
	                                        onChange={(event) => updateHeartbeatTask(task.id, { notifyOnFailure: event.target.checked })}
	                                      />
	                                      <span>失败后发消息</span>
	                                    </label>
	                                  </>
	                                ) : null}
	                              </div>
	                            </div>
	                          ))}
	                        </div>
	                      ) : (
	                        <div className="agent-empty-block">
	                          <strong>还没有任务</strong>
	                          <span>先添加一个 `notify` 或 `shell` 任务，再给它配置定时规则。</span>
	                        </div>
	                      )}
	                    </div>

	                    <div className="agent-subsection">
	                      <div className="agent-subsection-header">
	                        <div>
	                          <strong>规则</strong>
	                          <p>规则决定什么时候触发、触发哪个任务，以及把结果发给谁。当前 MVP 先支持每日固定时刻。</p>
	                        </div>
	                      </div>

	                      {heartbeatSchedules.length > 0 ? (
	                        <div className="agent-automation-list">
	                          {heartbeatSchedules.map((schedule, index) => (
	                            <div key={schedule.id} className="agent-automation-card">
	                              <div className="agent-automation-card-header">
	                                <div>
	                                  <strong>{schedule.name || `规则 ${index + 1}`}</strong>
	                                  <span>{schedule.times.join('、') || '未设置时间'} · {schedule.channelId || 'wechat'}</span>
	                                </div>
	                                <button type="button" className="icon-button subtle" onClick={() => removeHeartbeatSchedule(schedule.id)}>
	                                  <AppIcon name="trash" size={16} />
	                                </button>
	                              </div>

	                              <div className="agent-form-grid">
	                                <label className="input-field">
	                                  <span>规则名称</span>
	                                  <input
	                                    value={schedule.name}
	                                    onChange={(event) => updateHeartbeatSchedule(schedule.id, { name: event.target.value })}
	                                    placeholder="例如：工作日晚间复盘"
	                                  />
	                                </label>

	                                <label className="input-field">
	                                  <span>绑定任务</span>
	                                  <select
	                                    value={schedule.taskId}
	                                    onChange={(event) => updateHeartbeatSchedule(schedule.id, { taskId: event.target.value })}
	                                  >
	                                    <option value="">请选择任务</option>
	                                    {heartbeatTasks.map((task) => (
	                                      <option key={task.id} value={task.id}>
	                                        {task.name || task.id}
	                                      </option>
	                                    ))}
	                                  </select>
	                                </label>
	                              </div>

	                              <div className="agent-form-grid">
	                                <label className="input-field">
	                                  <span>触发时间</span>
	                                  <input
	                                    value={schedule.times.join(', ')}
	                                    onChange={(event) =>
	                                      updateHeartbeatSchedule(schedule.id, {
	                                        times: event.target.value
	                                          .split(',')
	                                          .map((value) => value.trim())
	                                          .filter(Boolean),
	                                      })
	                                    }
	                                    placeholder="例如：08:00, 17:00"
	                                  />
	                                </label>

	                                <label className="input-field">
	                                  <span>通道 ID</span>
	                                  <input
	                                    value={schedule.channelId}
	                                    onChange={(event) => updateHeartbeatSchedule(schedule.id, { channelId: event.target.value })}
	                                    placeholder="wechat"
	                                  />
	                                </label>
	                              </div>

	                              <div className="agent-form-grid">
	                                <label className="input-field">
	                                  <span>接收用户 ID</span>
	                                  <input
	                                    value={schedule.targetUserId}
	                                    onChange={(event) => updateHeartbeatSchedule(schedule.id, { targetUserId: event.target.value })}
	                                    placeholder="例如：wxid_xxx"
	                                  />
	                                </label>

	                                <label className="input-field">
	                                  <span>接收人备注</span>
	                                  <input
	                                    value={schedule.targetLabel}
	                                    onChange={(event) => updateHeartbeatSchedule(schedule.id, { targetLabel: event.target.value })}
	                                    placeholder="例如：老板 / 自己 / 数据群"
	                                  />
	                                </label>
	                              </div>

	                              <div className="agent-toggle-row">
	                                <label className="agent-check">
	                                  <input
	                                    type="checkbox"
	                                    checked={schedule.enabled}
	                                    onChange={(event) => updateHeartbeatSchedule(schedule.id, { enabled: event.target.checked })}
	                                  />
	                                  <span>启用规则</span>
	                                </label>
	                              </div>
	                            </div>
	                          ))}
	                        </div>
	                      ) : (
	                        <div className="agent-empty-block">
	                          <strong>还没有规则</strong>
	                          <span>规则决定执行时机和接收对象。添加后，应用启动时会自动加载并在后台定时检查。</span>
	                        </div>
	                      )}
	                    </div>
	                  </div>
	                </div>
	              ) : null}
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
                <span>智能体 ID：{selectedAgent?.id ?? '保存后生成'}</span>
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
            <button type="button" className="outline-button danger" onClick={onRequestDeleteAgent} disabled={agentSaving}>
              删除
            </button>
          ) : null}
          <button type="button" className="outline-button primary" onClick={onSaveAgent} disabled={agentSaving}>
            {agentSaving ? '保存中…' : mode === 'create' ? '创建智能体' : '保存修改'}
          </button>
        </div>

        {agentDeleteConfirmOpen && selectedAgent ? (
          <div className="confirm-dialog-overlay" role="presentation" onClick={onCloseDeleteAgentDialog}>
            <div className="confirm-dialog" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
              <h3>删除智能体</h3>
              <p>删除后会一并清除这个智能体的数据库记录、绑定机器人数据和 md 工作区。</p>
              <p>请输入 `确认删除` 以删除「{selectedAgent.name}」。</p>
              <label className="input-field">
                <span>确认口令</span>
                <input
                  value={agentDeleteConfirmText}
                  onChange={(event) => onDeleteConfirmTextChange(event.target.value)}
                  placeholder="确认删除"
                />
              </label>
              <div className="confirm-dialog-actions">
                <button type="button" className="outline-button" onClick={onCloseDeleteAgentDialog} disabled={agentSaving}>
                  取消
                </button>
                <button
                  type="button"
                  className="outline-button confirm-dialog-delete"
                  onClick={onConfirmDeleteAgent}
                  disabled={agentSaving || agentDeleteConfirmText.trim() !== '确认删除'}
                >
                  {agentSaving ? '删除中…' : '确认删除'}
                </button>
              </div>
            </div>
          </div>
        ) : null}

      </div>
    </div>
  )
}

type AgentBotBindingDialogProps = {
  agentId: string
  agentName: string
  botConfigs: Record<string, BotConfig>
  botLoading: boolean
  botStatusLog: BotStatusEvent[]
  formError: string
  formNotice: string
  onBotConfigChange: (channelId: BotChannelId, updates: Partial<BotConfig>) => void
  onClose: () => void
  onLarkStart: () => void
  onLarkStop: () => void
  onRotatePeerSecret: () => void
  onSave: () => void
  onSelectBot: (id: BotChannelId) => void
  onWechatLogin: () => void
  onWechatStart: () => void
  onWechatStop: () => void
  qrCodeUrl: string
  qrDialogOpen: boolean
  qrStatus: 'waiting' | 'scanned' | 'confirmed' | 'error'
  selectedBotConfig: BotConfig
  selectedBotDefinition: (typeof botDefinitions)[number]
  selectedBotId: BotChannelId
  setBotLoading: (loading: boolean) => void
  setQrDialogOpen: (open: boolean) => void
  saving: boolean
}

function AgentBotBindingDialog({
  agentId,
  agentName,
  botConfigs,
  botLoading,
  botStatusLog,
  formError,
  formNotice,
  onBotConfigChange,
  onClose,
  onLarkStart,
  onLarkStop,
  onRotatePeerSecret,
  onSave,
  onSelectBot,
  onWechatLogin,
  onWechatStart,
  onWechatStop,
  qrCodeUrl,
  qrDialogOpen,
  qrStatus,
  selectedBotConfig,
  selectedBotDefinition,
  selectedBotId,
  setBotLoading,
  setQrDialogOpen,
  saving,
}: AgentBotBindingDialogProps) {
  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="agent-editor-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-bot-binding-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="agent-editor-dialog-header">
          <div className="agent-editor-header-copy">
            <span className="agent-page-kicker">IM Bot Binding</span>
            <strong id="agent-bot-binding-title">{agentName} 的 IM 机器人绑定</strong>
            <span>独立维护当前智能体的机器人渠道配置，保存后写入数据库。</span>
          </div>

          <button type="button" className="icon-button subtle" onClick={onClose} aria-label="关闭机器人绑定弹窗">
            <AppIcon name="close" size={18} />
          </button>
        </div>

        <div className="agent-editor-dialog-scroll">
          <div className="agent-detail-card agent-editor-card">
            {formError ? (
              <div className="skills-feedback error agent-feedback inline">
                <strong>保存失败</strong>
                <span>{formError}</span>
              </div>
            ) : null}

            {formNotice ? (
              <div className="skills-feedback success agent-feedback inline">
                <strong>已更新</strong>
                <span>{formNotice}</span>
              </div>
            ) : null}

            <div className="bot-settings-layout">
              <div className="bot-channel-list">
                {botDefinitions.map((channel) => {
                  const config = botConfigs[channel.id]
                  return (
                    <button
                      key={channel.id}
                      type="button"
                      className={`bot-channel-card ${selectedBotId === channel.id ? 'active' : ''}`}
                      onClick={() => onSelectBot(channel.id)}
                    >
                      <span className="bot-channel-copy">
                        <strong>{channel.name}</strong>
                        <span
                          className={`bot-status-text ${
                            config.status === '已连接' ? 'connected' : config.status === '错误' ? 'error' : ''
                          }`}
                        >
                          {config.status}
                        </span>
                      </span>
                    </button>
                  )
                })}
              </div>

              <div className="bot-detail-panel">
                <div className="bot-detail-head">
                  <div className="bot-detail-title">
                    <strong className="bot-detail-heading">{selectedBotDefinition.name}</strong>
                    <span
                      className={`bot-status-tag ${
                        selectedBotConfig.status === '已连接'
                          ? 'connected'
                          : selectedBotConfig.status === '错误'
                            ? 'error'
                            : ''
                      }`}
                    >
                      {selectedBotConfig.status}
                    </span>
                  </div>
                </div>

                {selectedBotId === 'peer' ? (
                  <div className="agent-form-grid">
                    <label className="input-field agent-field-full">
                      <span>智能体 ID（对方填 toAgentId）</span>
                      <input readOnly value={agentId} />
                    </label>
                    <label className="input-field agent-field-full">
                      <span>本智能体入站密钥（Bearer）</span>
                      <input
                        value={
                          selectedBotConfig.peerSharedSecret?.trim()
                            ? selectedBotConfig.peerSharedSecret
                            : selectedBotConfig.clientSecret
                        }
                        onChange={(event) =>
                          onBotConfigChange('peer', { peerSharedSecret: event.target.value })
                        }
                        placeholder="保存智能体后由系统自动分配，也可自填"
                        autoComplete="off"
                      />
                    </label>
                    <div className="bot-action-row agent-field-full">
                      <button
                        type="button"
                        className="outline-button"
                        onClick={onRotatePeerSecret}
                        disabled={botLoading || saving}
                      >
                        <span>{botLoading ? '处理中…' : '重新生成密钥'}</span>
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="agent-form-grid">
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
                        value={selectedBotConfig.clientSecret}
                        onChange={(event) => onBotConfigChange(selectedBotId, { clientSecret: event.target.value })}
                        placeholder={selectedBotDefinition.secretPlaceholder}
                      />
                    </label>
                  </div>
                )}

                {selectedBotId === 'wechat' ? (
                  <>
                    <label className="input-field agent-field-full">
                      <span>路由标识（可选）</span>
                      <input
                        value={selectedBotConfig.routeTag ?? ''}
                        onChange={(event) => onBotConfigChange('wechat', { routeTag: event.target.value })}
                        placeholder="例如：agent-lawyer"
                      />
                    </label>

                    <div className="bot-action-row">
                      <button type="button" className="outline-button" onClick={onWechatLogin} disabled={botLoading || saving}>
                        <span>{botLoading ? '请稍候...' : '扫码绑定微信'}</span>
                      </button>
                      <button type="button" className="outline-button" onClick={onWechatStart} disabled={botLoading || saving}>
                        <span>{botLoading ? '启动中...' : '启动 Bot'}</span>
                      </button>
                      <button type="button" className="outline-button" onClick={onWechatStop} disabled={botLoading || saving}>
                        <span>{botLoading ? '处理中...' : '断开 Bot'}</span>
                      </button>
                    </div>
                  </>
                ) : selectedBotId === 'lark' ? (
                  <>
                    <div className="agent-workspace-hint">
                      <span>请在飞书开放平台开启机器人能力、长连接事件订阅，并订阅 `im.message.receive_v1`。</span>
                    </div>
                    <div className="bot-action-row">
                      <button type="button" className="outline-button" onClick={onLarkStart} disabled={botLoading || saving}>
                        <span>{botLoading ? '启动中...' : '启动飞书 Bot'}</span>
                      </button>
                      <button type="button" className="outline-button" onClick={onLarkStop} disabled={botLoading || saving}>
                        <span>{botLoading ? '处理中...' : '断开飞书 Bot'}</span>
                      </button>
                    </div>
                  </>
                ) : selectedBotId === 'peer' ? (
                  <div className="agent-workspace-hint">
                    <span>
                      全应用只需环境变量 <code>NINECLAW_PEER_BIND</code>（如 <code>127.0.0.1:17312</code>）开启监听；<strong>每个智能体各自密钥</strong>鉴权，无全平台共用 Secret。新建或保存智能体会自动补密钥；「重新生成」立即写库。详见{' '}
                      <code>docs/AGENT_PEER_INTEROP.md</code>。
                    </span>
                  </div>
                ) : (
                  <div className="agent-workspace-hint">
                    <span>该渠道当前先支持独立保存绑定信息，运行接入稍后补齐。</span>
                  </div>
                )}

                {selectedBotConfig.errorMessage ? (
                  <div className="skills-feedback error agent-feedback inline">
                    <strong>机器人状态异常</strong>
                    <span>{selectedBotConfig.errorMessage}</span>
                  </div>
                ) : null}

                {botStatusLog.length > 0 ? (
                  <div className="bot-status-log">
                    <strong>最近运行状态</strong>
                    <div className="bot-status-entries">
                      {botStatusLog.slice(0, 6).map((entry, index) => (
                        <div key={`${entry.timestamp}-${index}`} className={`bot-status-entry ${entry.level}`}>
                          <span className="bot-status-level">{entry.level}</span>
                          <span className="bot-status-msg">{entry.message}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                ) : null}
              </div>
            </div>
          </div>
        </div>

        <div className="agent-editor-dialog-footer">
          <button type="button" className="outline-button" onClick={onClose} disabled={saving}>
            关闭
          </button>
          <button type="button" className="outline-button primary" onClick={onSave} disabled={saving}>
            {saving ? '保存中…' : '保存绑定'}
          </button>
        </div>

        {selectedBotId === 'wechat' && qrDialogOpen ? (
          <div className="qr-dialog-overlay" onClick={() => { setQrDialogOpen(false); setBotLoading(false) }}>
            <div className="qr-dialog" onClick={(event) => event.stopPropagation()}>
              <div className="qr-dialog-header">
                <strong>微信扫码绑定</strong>
                <button type="button" className="qr-dialog-close" onClick={() => { setQrDialogOpen(false); setBotLoading(false) }}>
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
                    <p>绑定成功</p>
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
  agentBotBindingDialogOpen: boolean
  agentDeleteConfirmOpen: boolean
  agentDeleteConfirmText: string
  agentEditorOpen: boolean
  agentFormError: string
  agentFormNotice: string
  agentRefreshing: boolean
  agentSaving: boolean
  botConfigs: Record<string, BotConfig>
  botLoading: boolean
  botStatusLog: BotStatusEvent[]
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
  qrCodeUrl: string
  qrDialogOpen: boolean
  qrStatus: 'waiting' | 'scanned' | 'confirmed' | 'error'
  onBotConfigChange: (channelId: BotChannelId, updates: Partial<BotConfig>) => void
  onCloseEditor: () => void
  onCloseBotBindingDialog: () => void
  onCloseDeleteAgentDialog: () => void
  onCloseWorkspaceDialog: () => void
  onConfirmDeleteAgent: () => void
  onCreateAgent: () => void
  onDraftChange: (updates: Partial<AgentInput>) => void
  onDeleteConfirmTextChange: (value: string) => void
  onDraftWorkspaceContentChange: (value: string) => void
  onRefreshAgents: () => void
  onOpenWorkspace: () => void
  onOpenSkillPicker: () => void
  onOpenBotBinding: (id: string) => void
  onOpenEditor: (id: string) => void
  onRequestDeleteAgent: () => void
  onRefreshWorkspace: () => void
  onSaveAgent: () => void
  onSaveWorkspaceFile: (file: AgentWorkspaceFile, content: string) => void | Promise<void>
  onSearch: (value: string) => void
  onSelectAgent: (id: string) => void
  onSelectBot: (id: BotChannelId) => void
  onSelectWorkspaceFile: (key: string) => void
  setBotLoading: (loading: boolean) => void
  onSetDefaultAgent: () => void
  onToggleSkill: (skillId: string) => void
  onLarkStart: () => void
  onLarkStop: () => void
  onWechatLogin: () => void
  onWechatStart: () => void
  onWechatStop: () => void
  onRotatePeerSecret: () => void
  onStartChatWithAgent: (id: string) => void
  setQrDialogOpen: (open: boolean) => void
  searchValue: string
  selectedAgent: AgentRecord | null
  selectedBotConfig: BotConfig
  selectedBotDefinition: (typeof botDefinitions)[number]
  selectedBotId: BotChannelId
  managedAgentId: string
}

function AgentsView({
  agentDraft,
  agentBotBindingDialogOpen,
  agentDeleteConfirmOpen,
  agentDeleteConfirmText,
  agentEditorOpen,
  agentFormError,
  agentFormNotice,
  agentRefreshing,
  agentSaving,
  botConfigs,
  botLoading,
  botStatusLog,
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
  qrCodeUrl,
  qrDialogOpen,
  qrStatus,
  onBotConfigChange,
  onCloseEditor,
  onCloseBotBindingDialog,
  onCloseDeleteAgentDialog,
  onCloseWorkspaceDialog,
  onConfirmDeleteAgent,
  onCreateAgent,
  onDraftChange,
  onDeleteConfirmTextChange,
  onDraftWorkspaceContentChange,
  onRefreshAgents,
  onOpenWorkspace,
  onOpenSkillPicker,
  onOpenBotBinding,
  onOpenEditor,
  onRequestDeleteAgent,
  onRefreshWorkspace,
  onSaveAgent,
  onSaveWorkspaceFile,
  onSearch,
  onSelectAgent,
  onSelectBot,
  onSelectWorkspaceFile,
  setBotLoading,
  onSetDefaultAgent,
  onToggleSkill,
  onLarkStart,
  onLarkStop,
  onWechatLogin,
  onWechatStart,
  onWechatStop,
  onRotatePeerSecret,
  onStartChatWithAgent,
  setQrDialogOpen,
  searchValue,
  selectedAgent,
  selectedBotConfig,
  selectedBotDefinition,
  selectedBotId,
  managedAgentId,
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

          <div className="agent-page-header-actions">
            <button type="button" className="outline-button" onClick={() => void onRefreshAgents()} disabled={loading || agentRefreshing}>
              <AppIcon name="refresh" size={18} />
              <span>{agentRefreshing ? '刷新中…' : '刷新列表'}</span>
            </button>

            <button type="button" className="create-agent-button agent-create-inline" onClick={onCreateAgent}>
              <AppIcon name="plus" size={20} />
              <span>新建智能体</span>
            </button>
          </div>
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
                  <div
                    key={agent.id}
                    role="button"
                    tabIndex={0}
                    className={`agent-row list ${selectedAgent?.id === agent.id ? 'active' : ''}`}
                    onClick={() => onSelectAgent(agent.id)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault()
                        onSelectAgent(agent.id)
                      }
                    }}
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
                      <span className="agent-inline-id">ID: {agent.id}</span>
                      <span>{agent.summary}</span>
                      <small>{agent.description || '点击进入弹窗，补充介绍、模型和挂载技能。'}</small>
                    </span>
                    <span className="agent-list-meta">
                      <span className="agent-list-meta-pill">
                        {agent.defaultProviderId} · {agent.defaultModel}
                      </span>
                      <span className="agent-list-meta-pill">{agent.skillIds.length} 个技能</span>
                      <button
                        type="button"
                        className="agent-row-action"
                        onClick={(event) => {
                          event.stopPropagation()
                          onStartChatWithAgent(agent.id)
                        }}
                      >
                        直接聊天
                      </button>
                      <button
                        type="button"
                        className="agent-row-action"
                        onClick={(event) => {
                          event.stopPropagation()
                          onOpenBotBinding(agent.id)
                        }}
                      >
                        绑定 IM
                      </button>
                      <button
                        type="button"
                        className="agent-row-action"
                        onClick={(event) => {
                          event.stopPropagation()
                          onOpenEditor(agent.id)
                        }}
                      >
                        编辑配置
                      </button>
                    </span>
                  </div>
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
          agentDeleteConfirmOpen={agentDeleteConfirmOpen}
          agentDeleteConfirmText={agentDeleteConfirmText}
          agentFormError={agentFormError}
          agentFormNotice={agentFormNotice}
          agentRefreshing={agentRefreshing}
          agentSaving={agentSaving}
          allSkills={allSkills}
          defaultAgentId={defaultAgentId}
          mode={mode}
          modelOptions={modelOptions}
          onClose={onCloseEditor}
          onCloseDeleteAgentDialog={onCloseDeleteAgentDialog}
          onConfirmDeleteAgent={onConfirmDeleteAgent}
          onCreateAgent={onCreateAgent}
          onDraftChange={onDraftChange}
          onDeleteConfirmTextChange={onDeleteConfirmTextChange}
          onRefreshAgent={onRefreshAgents}
          onOpenWorkspace={onOpenWorkspace}
          onOpenSkillPicker={onOpenSkillPicker}
          onRequestDeleteAgent={onRequestDeleteAgent}
          onSaveAgent={onSaveAgent}
          onSetDefaultAgent={onSetDefaultAgent}
          onToggleSkill={onToggleSkill}
          selectedAgent={selectedAgent}
          managedAgentId={managedAgentId}
        />
      ) : null}

      {agentBotBindingDialogOpen && selectedAgent ? (
        <AgentBotBindingDialog
          agentId={selectedAgent.id}
          agentName={selectedAgent.name}
          botConfigs={botConfigs}
          botLoading={botLoading}
          botStatusLog={botStatusLog}
          formError={agentFormError}
          formNotice={agentFormNotice}
          onBotConfigChange={onBotConfigChange}
          onClose={onCloseBotBindingDialog}
          onLarkStart={onLarkStart}
          onLarkStop={onLarkStop}
          onRotatePeerSecret={onRotatePeerSecret}
          onSave={onSaveAgent}
          onSelectBot={onSelectBot}
          onWechatLogin={onWechatLogin}
          onWechatStart={onWechatStart}
          onWechatStop={onWechatStop}
          qrCodeUrl={qrCodeUrl}
          qrDialogOpen={qrDialogOpen}
          qrStatus={qrStatus}
          selectedBotConfig={selectedBotConfig}
          selectedBotDefinition={selectedBotDefinition}
          selectedBotId={selectedBotId}
          setBotLoading={setBotLoading}
          setQrDialogOpen={setQrDialogOpen}
          saving={agentSaving}
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

export default App
