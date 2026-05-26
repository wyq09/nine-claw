import { createPortal } from 'react-dom'
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { AppIcon } from '../../components/AppIcon'
import { botDefinitions, createInitialBotConfigs } from '../../mockData'
import type { BotStatusEvent } from '../../lib/piClient'
import {
  createDefaultAgentCapabilityPolicy,
  createStaticAgentCapabilityPolicy,
  normalizeAgentCapabilityPolicy,
} from './agentCapabilities'
import type {
  AgentBuilderDraft,
  AgentExecutionMode,
  AgentHeartbeatConfig,
  AgentInput,
  AgentRecord,
  AgentScenarioLlmConfig,
  AgentScenarioLlmSlot,
  AgentTaskListItem,
  AgentToolId,
  AgentWorkspaceBundle,
  AgentWorkspaceFile,
  BotChannelId,
  BotConfig,
  ConversationAgentSnapshot,
  HistoryStatus,
  InstalledSkillItem,
  ProviderId,
  ProviderRuntimeConfig,
  SystemSkillCatalog,
  TokenUsage,
} from '../../types'

export const AGENT_TOOL_OPTIONS: { id: AgentToolId; label: string; description: string }[] = [
  { id: 'bash', label: 'bash', description: '执行命令行命令' },
  { id: 'read_file', label: 'read_file', description: '读取文件内容' },
  { id: 'write_file', label: 'write_file', description: '写入或创建文件' },
  { id: 'edit_file', label: 'edit_file', description: '按差异编辑文件' },
  { id: 'grep', label: 'grep', description: '按文本模式搜索文件内容' },
  { id: 'list_dir', label: 'list_dir', description: '列出目录内容' },
  { id: 'glob', label: 'glob', description: '按路径模式查找文件' },
  { id: 'web_search', label: 'web_search', description: '联网搜索信息' },
  { id: 'web_fetch', label: 'web_fetch', description: '抓取网页内容' },
  { id: 'image_generate', label: 'image_generate', description: '生成图片' },
  { id: 'image_task_query', label: 'image_task_query', description: '查询图片任务' },
  { id: 'ask_user', label: 'ask_user', description: '向用户发起澄清提问卡片' },
  { id: 'agent_spawn', label: 'agent_spawn', description: '委派子智能体' },
  { id: 'external_api', label: 'external_api', description: '调用外部 API 扩展' },
  { id: 'memory_update', label: 'memory_update', description: '更新当前智能体的 MEMORY.md 记忆文件' },
  { id: 'memory_search', label: 'memory_search', description: '语义搜索共享 / K/V 记忆库' },
  { id: 'memory_read', label: 'memory_read', description: '读取当前智能体的 MEMORY.md 记忆文件' },
  { id: 'memory_delete', label: 'memory_delete', description: '删除共享向量记忆条目' },
  { id: 'memory_store', label: 'memory_store', description: '把结构化信息写入 K/V 记忆库' },
  { id: 'memory_get', label: 'memory_get', description: '按 key 读取 K/V 记忆' },
  { id: 'memory_forget', label: 'memory_forget', description: '按 key 删除 K/V 记忆' },
  { id: 'memory_list', label: 'memory_list', description: '列出 K/V 记忆条目' },
  { id: 'chat_search', label: 'chat_search', description: '跨会话关键词搜索历史消息' },
  { id: 'create_scheduled_task', label: 'create_scheduled_task', description: '创建定时任务到任务中心' },
  { id: 'query_scheduled_task', label: 'query_scheduled_task', description: '查询当前智能体的定时任务列表' },
  { id: 'query_scheduled_task_info', label: 'query_scheduled_task_info', description: '查询单个定时任务的详情' },
]

const AGENT_TOOL_ID_SET = new Set<AgentToolId>(AGENT_TOOL_OPTIONS.map((tool) => tool.id))

const AGENT_TOOL_ALIASES: Record<string, AgentToolId> = {
  read: 'read_file',
  write: 'write_file',
  edit: 'edit_file',
  ls: 'list_dir',
  find: 'glob',
  agent_delegate: 'agent_spawn',
  nineclaw_external_api: 'external_api',
}

export function createDefaultAgentAllowedToolIds(): AgentToolId[] {
  return AGENT_TOOL_OPTIONS.map((tool) => tool.id)
}

export function normalizeAgentAllowedToolIds(value?: readonly unknown[] | null): AgentToolId[] {
  if (!Array.isArray(value)) {
    return createDefaultAgentAllowedToolIds()
  }
  const ids: AgentToolId[] = []
  for (const item of value) {
    if (typeof item !== 'string') {
      continue
    }
    const raw = item.trim()
    const id = (AGENT_TOOL_ALIASES[raw] ?? raw) as AgentToolId
    if (AGENT_TOOL_ID_SET.has(id) && !ids.includes(id)) {
      ids.push(id)
    }
  }
  return ids
}

const ABSOLUTE_TIME_FORMATTER = new Intl.DateTimeFormat('zh-CN', {
  year: 'numeric',
  month: 'numeric',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
})

export function summarizePrompt(prompt: string, maxLength = 20): string {
  const compact = prompt.replace(/\s+/g, ' ').trim()
  if (!compact) {
    return '新会话'
  }

  return compact.length > maxLength ? `${compact.slice(0, maxLength)}…` : compact
}

export function formatAbsoluteTime(createdAt: number): string {
  return ABSOLUTE_TIME_FORMATTER.format(new Date(createdAt))
}

const CHAT_WEEKDAY_LABELS = ['日', '一', '二', '三', '四', '五', '六'] as const

/** 会话内时间分隔：今天/昨天/日期+星期 + 时刻，贴近常见 IM 顶栏样式 */
export function formatChatTurnTimeLabel(createdAt: number): string {
  const d = new Date(createdAt)
  if (!Number.isFinite(d.getTime())) {
    return '时间未知'
  }
  const pad = (n: number) => String(n).padStart(2, '0')
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`
  const now = new Date()
  const startOfDay = (t: Date) => new Date(t.getFullYear(), t.getMonth(), t.getDate()).getTime()
  const diffDays = Math.round((startOfDay(now) - startOfDay(d)) / 86400000)
  if (diffDays === 0) {
    return `今天 ${hm}`
  }
  if (diffDays === 1) {
    return `昨天 ${hm}`
  }
  if (diffDays === 2) {
    return `前天 ${hm}`
  }
  const w = CHAT_WEEKDAY_LABELS[d.getDay()] ?? '日'
  const y = d.getFullYear()
  const ny = now.getFullYear()
  if (y === ny) {
    return `${d.getMonth() + 1}月${d.getDate()}日 星期${w} ${hm}`
  }
  return `${y}年${d.getMonth() + 1}月${d.getDate()}日 星期${w} ${hm}`
}

/** 时间 pill 点击展开：如「4 月 13 日 星期一 23:25」（与常见 IM 一致，不含年份） */
export function formatChatTurnTimeFull(createdAt: number): string {
  const d = new Date(createdAt)
  if (!Number.isFinite(d.getTime())) {
    return '时间未知'
  }
  const pad = (n: number) => String(n).padStart(2, '0')
  const w = CHAT_WEEKDAY_LABELS[d.getDay()] ?? '日'
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`
  return `${d.getMonth() + 1} 月 ${d.getDate()} 日 星期${w} ${hm}`
}

export function formatOptionalAbsoluteTime(createdAt?: number | null): string {
  return typeof createdAt === 'number' && createdAt > 0 ? formatAbsoluteTime(createdAt) : '时间未知'
}

export function formatAgentTaskStatus(status: string): string {
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

export function runAtMsToDatetimeLocalValue(ms: number): string {
  const d = new Date(ms)
  if (!Number.isFinite(d.getTime())) {
    return ''
  }
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function formatAgentTaskScheduleShort(task: AgentTaskListItem): string {
  if (task.scheduleType === 'interval') {
    const m = task.intervalMinutes
    if (typeof m === 'number' && m > 0) {
      return `每 ${m} 分钟`
    }
    return '间隔执行'
  }
  if (task.scheduleType === 'once_at') {
    return '一次性'
  }
  if (task.scheduleType === 'weekly_time') {
    const days = formatWeeklyDays(task.weeklyDays)
    return task.dailyTimes.length > 0 ? `每周 ${days} ${task.dailyTimes.join('、')}` : `每周 ${days}`
  }
  if (task.scheduleType === 'monthly_time') {
    const days = formatMonthlyDays(task.monthlyDays)
    return task.dailyTimes.length > 0 ? `每月 ${days} ${task.dailyTimes.join('、')}` : `每月 ${days}`
  }
  if (task.dailyTimes.length > 0) {
    return `每日 ${task.dailyTimes.join('、')}`
  }
  return '每日定时'
}

const TASK_STATUS_GROUP_ORDER = ['active', 'paused', 'draft', 'deleted'] as const

const TASK_STATUS_GROUP_SET = new Set<string>(TASK_STATUS_GROUP_ORDER)

export function groupAgentTasksByStatus(taskItems: AgentTaskListItem[]) {
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

export function parseDailyTimesInput(value: string): string[] {
  return value
    .split(/[\s,，、]+/)
    .map((item) => item.trim())
    .filter(Boolean)
}

export function parseNumericDaysInput(value: string, min: number, max: number): number[] {
  const digits = value.match(/\d+/g) ?? []
  return Array.from(
    new Set(
      digits
        .map((item) => Number.parseInt(item, 10))
        .filter((item) => Number.isFinite(item) && item >= min && item <= max),
    ),
  ).sort((left, right) => left - right)
}

export function formatWeeklyDays(days: number[]): string {
  const labels = days
    .map((day) => {
      switch (day) {
        case 1:
          return '周一'
        case 2:
          return '周二'
        case 3:
          return '周三'
        case 4:
          return '周四'
        case 5:
          return '周五'
        case 6:
          return '周六'
        case 7:
          return '周日'
        default:
          return ''
      }
    })
    .filter(Boolean)
  return labels.length > 0 ? labels.join('、') : '未设置'
}

export function formatMonthlyDays(days: number[]): string {
  return days.length > 0 ? days.map((day) => `${day}号`).join('、') : '未设置'
}

export function createEmptySystemSkillCatalog(): SystemSkillCatalog {
  return {
    available: false,
    updatedAt: null,
    message: '系统技能库暂未开放，后续会由系统统一提供可安装技能。',
    skills: [],
  }
}

export function getAgentColor(agent: { accentColor?: string; id: string; name: string }): string {
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

export function createAgentBotConfigState(configs?: Record<string, BotConfig>): Record<string, BotConfig> {
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

export function getBotChannelRuntimeId(agentId: string, channelId: BotChannelId): string {
  return `${channelId}:${agentId}`
}

export function resolveBotChannelFromRuntimeId(agentId: string, runtimeChannelId: string): BotChannelId | null {
  for (const channel of botDefinitions) {
    if (runtimeChannelId === getBotChannelRuntimeId(agentId, channel.id)) {
      return channel.id
    }
  }
  return null
}

export function buildBotConfigStatusPatch(event: BotStatusEvent): Partial<BotConfig> | null {
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

export function buildBotRuntimeBindingConfig(runtime: ProviderRuntimeConfig): Partial<BotConfig> {
  return {
    aiProviderId: runtime.providerId,
    aiApiFormat: runtime.apiFormat,
    aiBaseUrl: runtime.baseUrl,
    aiApiKey: runtime.apiKey,
    aiModel: runtime.model,
  }
}

export function buildConversationAgentSnapshot(agent: AgentRecord): ConversationAgentSnapshot {
  return {
    id: agent.id,
    name: agent.name,
    summary: agent.summary,
    description: agent.description,
    ...(agent.avatarUri ? { avatarUri: agent.avatarUri } : {}),
    triggerCondition: agent.triggerCondition,
    manualTriggerOnly: agent.manualTriggerOnly,
    systemPrompt: agent.systemPrompt,
    capabilityPolicy: normalizeAgentCapabilityPolicy(agent.capabilityPolicy, createStaticAgentCapabilityPolicy()),
    skillIds: [...agent.skillIds],
    allowedToolIds: normalizeAgentAllowedToolIds(agent.allowedToolIds),
    defaultProviderId: agent.defaultProviderId,
    defaultModel: agent.defaultModel,
    executionMode: agent.executionMode,
    ...(agent.collaborationConfig ? { collaborationConfig: agent.collaborationConfig } : {}),
    ...(agent.accentColor ? { accentColor: agent.accentColor } : {}),
    ...(agent.scenarioLlmConfig ? { scenarioLlmConfig: agent.scenarioLlmConfig } : {}),
  }
}

export function generateDraftItemId(prefix: string): string {
  const uuid =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID().replace(/-/g, '')
      : `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`
  return `${prefix}_${uuid}`
}

export function createEmptyHeartbeatConfig(): AgentHeartbeatConfig {
  return {
    timezone: 'Asia/Shanghai',
    tasks: [],
    schedules: [],
  }
}

export function normalizeHeartbeatTimes(times: string[]): string[] {
  return Array.from(
    new Set(
      times
        .flatMap((value) => value.split(','))
        .map((value) => value.trim())
        .filter(Boolean),
    ),
  )
}

export function normalizeHeartbeatConfig(config?: AgentHeartbeatConfig | null): AgentHeartbeatConfig {
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

export function createEmptyAgentDraft(
  providerId: ProviderId,
  model: string,
  accentColor?: string,
): AgentInput {
  return {
    id: '',
    name: '',
    summary: '',
    description: '',
    avatarUri: '',
    triggerCondition: '',
    manualTriggerOnly: false,
    systemPrompt: '',
    capabilityPolicy: createStaticAgentCapabilityPolicy(),
    skillIds: [],
    allowedToolIds: createDefaultAgentAllowedToolIds(),
    defaultProviderId: providerId,
    defaultModel: model,
    executionMode: 'single',
    accentColor,
    botConfigs: createAgentBotConfigState(),
    heartbeatConfig: createEmptyHeartbeatConfig(),
  }
}

export function normalizeAgentScenarioLlmConfigInDraft(
  config?: AgentScenarioLlmConfig | null,
): AgentScenarioLlmConfig | undefined {
  if (!config) {
    return undefined
  }
  const pick = (slot?: AgentScenarioLlmSlot | null) => {
    const providerId = slot?.providerId?.trim() ?? ''
    const model = slot?.model?.trim() ?? ''
    if (!providerId || !model) {
      return undefined
    }
    return { providerId, model }
  }
  const titleGeneration = pick(config.titleGeneration)
  const memoryExtraction = pick(config.memoryExtraction)
  const taskPushNotificationCopy = pick(config.taskPushNotificationCopy)
  if (!titleGeneration && !memoryExtraction && !taskPushNotificationCopy) {
    return undefined
  }
  return { titleGeneration, memoryExtraction, taskPushNotificationCopy }
}

export function createAgentDraftFromRecord(agent: AgentRecord): AgentInput {
  return {
    id: agent.id,
    name: agent.name,
    summary: agent.summary,
    description: agent.description,
    ...(agent.avatarUri ? { avatarUri: agent.avatarUri } : {}),
    triggerCondition: agent.triggerCondition,
    manualTriggerOnly: agent.manualTriggerOnly,
    systemPrompt: agent.systemPrompt,
    capabilityPolicy: normalizeAgentCapabilityPolicy(agent.capabilityPolicy, createStaticAgentCapabilityPolicy()),
    skillIds: [...agent.skillIds],
    allowedToolIds: normalizeAgentAllowedToolIds(agent.allowedToolIds),
    defaultProviderId: agent.defaultProviderId,
    defaultModel: agent.defaultModel,
    executionMode: agent.executionMode,
    botConfigs: createAgentBotConfigState(agent.botConfigs),
    heartbeatConfig: normalizeHeartbeatConfig(agent.heartbeatConfig),
    ...(agent.collaborationConfig ? { collaborationConfig: agent.collaborationConfig } : {}),
    ...(agent.accentColor ? { accentColor: agent.accentColor } : {}),
    ...(agent.scenarioLlmConfig ? { scenarioLlmConfig: agent.scenarioLlmConfig } : {}),
    ...(agent.agentLoopConfig ? { agentLoopConfig: agent.agentLoopConfig } : {}),
  }
}

export function normalizeInlineText(value: string): string {
  return value.replace(/\s+/g, ' ').trim()
}

const AUTO_AGENT_SUMMARY_MAX_CHARS = 36

export function buildAutoAgentSummary(description: string, fallbackName = ''): string {
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

export function normalizeAgentDraft(input: AgentInput): AgentInput {
  const id = input.id?.trim() ?? ''
  const name = input.name.trim()
  const explicitSummary = input.summary.trim()
  const description = input.description.trim() || explicitSummary
  const summary = explicitSummary || buildAutoAgentSummary(description, name)
  const avatarUri = input.avatarUri?.trim() ?? ''
  return {
    ...input,
    id,
    name,
    summary,
    description,
    avatarUri,
    triggerCondition: input.triggerCondition.trim(),
    manualTriggerOnly: input.manualTriggerOnly === true,
    systemPrompt: input.systemPrompt.trim(),
    capabilityPolicy: createStaticAgentCapabilityPolicy(),
    defaultProviderId: input.defaultProviderId.trim(),
    defaultModel: input.defaultModel.trim(),
    skillIds: Array.from(new Set(input.skillIds.map((item) => item.trim()).filter(Boolean))),
    allowedToolIds: normalizeAgentAllowedToolIds(input.allowedToolIds),
    botConfigs: createAgentBotConfigState(input.botConfigs),
    heartbeatConfig: normalizeHeartbeatConfig(input.heartbeatConfig),
    scenarioLlmConfig: normalizeAgentScenarioLlmConfigInDraft(input.scenarioLlmConfig),
  }
}

/** 根据智能体名称生成推荐的 Agent_ID（仅保留英文字母/数字/下划线/连字符） */
export function suggestAgentIdFromName(name: string): string {
  const base = name
    .trim()
    .toLowerCase()
    .replace(/[\s]+/g, '-')
    .replace(/[^a-z0-9_-]/g, '')
  if (!base) return ''
  // 去掉首尾的连字符
  return base.replace(/^-+|-+$/g, '')
}

export function validateAgentDraft(input: AgentInput): string | null {
  const id = input.id?.trim() ?? ''
  if (id && !/^[A-Za-z0-9_-]+$/.test(id)) {
    return 'Agent_ID 只能包含英文字母、数字、下划线和连字符。'
  }
  if (!input.name.trim()) {
    return '请输入展示名称。'
  }
  if (!input.description.trim() && !input.summary.trim()) {
    return '请输入智能体角色说明。'
  }
  if (!input.defaultProviderId.trim() || !input.defaultModel.trim()) {
    return '请为智能体配置默认模型。'
  }
  return null
}

export function formatAgentExecutionModeLabel(mode: AgentExecutionMode): string {
  if (mode === 'supervisor') {
    return '协调者'
  }
  if (mode === 'worker') {
    return '执行者'
  }
  return '单智能体'
}

export function formatInstalledSkillScopeLabel(scope: InstalledSkillItem['scope']): string {
  return scope === 'workspace' ? '工作区已安装' : '系统已安装'
}

export function formatInstalledSkillSource(skill: InstalledSkillItem): string {
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

export function normalizeSkillDescription(description: string | null | undefined): string {
  const trimmed = description?.trim()
  return trimmed ? trimmed : '暂无技能说明。'
}

export function shouldCollapseSkillDescription(description: string): boolean {
  return description.length > 96 || description.includes('\n')
}

type SkillDescriptionDisclosureProps = {
  description: string | null | undefined
  collapsedLines?: number
  className?: string
}

export function SkillDescriptionDisclosure({
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

export function formatWorkspaceFileSectionLabel(section: AgentWorkspaceFile['section']): string {
  if (section === 'private') {
    return '私有文件'
  }
  if (section === 'memoryIndex') {
    return '记忆索引'
  }
  if (section === 'wiki') {
    return '知识 Wiki'
  }
  if (section === 'dailyLog') {
    return '最近日志'
  }
  return '共享文件'
}

export function pickDefaultWorkspaceFileKey(bundle: AgentWorkspaceBundle): string {
  const preferredNames = ['IDENTITY.md', 'ROLE.md', 'MEMORY.md', 'WORKING.md', 'AGENTS.md']
  for (const name of preferredNames) {
    const match = bundle.files.find((file) => file.name === name && file.exists)
    if (match) {
      return match.key
    }
  }
  return bundle.files[0]?.key ?? ''
}

export function validateSkillInstallLink(value: string): string | null {
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

export function buildSkillInstallPrompt(link: string): string {
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

export function parseAgentBuilderDraft(content: string): AgentBuilderDraft | null {
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
    const id = typeof parsed.id === 'string' ? parsed.id.trim() : ''
    const summary = typeof parsed.summary === 'string' ? parsed.summary.trim() : ''
    const description = typeof parsed.description === 'string' ? parsed.description.trim() : ''
    const avatarUri = typeof parsed.avatarUri === 'string' ? parsed.avatarUri.trim() : ''
    const triggerCondition =
      typeof parsed.triggerCondition === 'string' ? parsed.triggerCondition.trim() : ''
    const manualTriggerOnly = parsed.manualTriggerOnly === true
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
    const allowedToolIds = normalizeAgentAllowedToolIds(parsed.allowedToolIds)
    const capabilityPolicy = normalizeAgentCapabilityPolicy(
      parsed.capabilityPolicy,
      createDefaultAgentCapabilityPolicy(),
    )

    if (!name || !normalizedDescription) {
      return null
    }

    return {
      ...(id ? { id } : {}),
      name,
      summary: normalizedSummary,
      description: normalizedDescription,
      ...(avatarUri ? { avatarUri } : {}),
      triggerCondition,
      manualTriggerOnly,
      systemPrompt,
      capabilityPolicy,
      skillIds,
      allowedToolIds,
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

export function stripAgentBuilderBlock(content: string): string {
  return content.replace(AGENT_BUILDER_BLOCK_PATTERN, '').trim()
}

export function formatHistoryAgeLabel(status: HistoryStatus, updatedAt: number): string {
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

export function getStatusTone(status: HistoryStatus): string {
  if (status === 'running') return 'running'
  if (status === 'done') return 'done'
  return 'error'
}

export function formatDurationLabel(durationMs: number): string {
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

export function getElapsedMs(startedAt: number, completedAt?: number, isStreaming = false): number | undefined {
  const endAt = completedAt ?? (isStreaming ? Date.now() : undefined)
  if (!endAt) {
    return undefined
  }

  return Math.max(0, endAt - startedAt)
}

export function formatTokenCount(value: number | undefined): string {
  if (typeof value !== 'number') {
    return '--'
  }

  return value.toLocaleString('zh-CN')
}

export function useLiveNow(enabled: boolean, intervalMs = 1000): number {
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

export function hasUsageMetrics(usage?: TokenUsage): boolean {
  return Boolean(
    usage &&
      (usage.totalTokens > 0 ||
        usage.inputTokens > 0 ||
        usage.outputTokens > 0 ||
        usage.cacheReadTokens > 0 ||
        usage.cacheWriteTokens > 0),
  )
}

export function TokenUsageDetailPill({ usage }: { usage: TokenUsage }) {
  const [open, setOpen] = useState(false)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const buttonRef = useRef<HTMLButtonElement | null>(null)
  const tooltipRef = useRef<HTMLDivElement | null>(null)
  const [tipPlacement, setTipPlacement] = useState<{ top: number; left: number } | null>(null)

  const repositionTooltip = useCallback(() => {
    const button = buttonRef.current
    const tip = tooltipRef.current
    if (!button || !tip) {
      return
    }
    const anchor = button.getBoundingClientRect()
    const margin = 12
    const gap = 8
    const tw = tip.offsetWidth
    const th = tip.offsetHeight
    let left = anchor.left
    let top = anchor.bottom + gap
    if (top + th > window.innerHeight - margin) {
      top = Math.max(margin, anchor.top - th - gap)
    }
    if (left + tw > window.innerWidth - margin) {
      left = window.innerWidth - margin - tw
    }
    if (left < margin) {
      left = margin
    }
    setTipPlacement({ top, left })
  }, [])

  useLayoutEffect(() => {
    if (!open) {
      setTipPlacement(null)
      return
    }
    repositionTooltip()
    const raf = window.requestAnimationFrame(() => repositionTooltip())
    window.addEventListener('resize', repositionTooltip)
    window.addEventListener('scroll', repositionTooltip, true)
    return () => {
      window.cancelAnimationFrame(raf)
      window.removeEventListener('resize', repositionTooltip)
      window.removeEventListener('scroll', repositionTooltip, true)
    }
  }, [open, repositionTooltip, usage.totalTokens])

  useEffect(() => {
    if (!open) {
      return
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target
      if (!(target instanceof Node)) {
        return
      }
      const inTrigger = containerRef.current?.contains(target)
      const inTooltip = tooltipRef.current?.contains(target)
      if (!inTrigger && !inTooltip) {
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

  const tooltipNode =
    open &&
    createPortal(
      <div
        ref={tooltipRef}
        className="answer-result-tooltip"
        role="dialog"
        aria-label="Token 详情"
        style={{
          position: 'fixed',
          top: tipPlacement?.top ?? -9999,
          left: tipPlacement?.left ?? 0,
          zIndex: 20050,
          visibility: tipPlacement ? 'visible' : 'hidden',
          pointerEvents: tipPlacement ? 'auto' : 'none',
        }}
      >
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
          <span>cacheWrite</span>
          <span>{formatTokenCount(usage.cacheWriteTokens)}</span>
          <span>totalTokens</span>
          <span>{formatTokenCount(usage.totalTokens)}</span>
        </div>
      </div>,
      document.body,
    )

  return (
    <div className="answer-result-meta-popover" ref={containerRef}>
      <button
        ref={buttonRef}
        type="button"
        className="answer-result-meta-item answer-result-meta-button"
        onClick={() => setOpen((current) => !current)}
        aria-expanded={open}
      >
        总 Token：{formatTokenCount(usage.totalTokens)}
      </button>
      {tooltipNode}
    </div>
  )
}
