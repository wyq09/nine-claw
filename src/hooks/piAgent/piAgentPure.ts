import type { BotMessageEvent } from '../../lib/piClient'
import { createStaticAgentCapabilityPolicy, normalizeAgentCapabilityPolicy } from '../../app/lib/agentCapabilities'
import { normalizeAgentAllowedToolIds } from '../../app/lib/appFormatting'
import type {
  ActivityEntry,
  ActivityState,
  AgentCapabilityPolicy,
  AgentCollaborationConfig,
  AgentExecutionMode,
  AgentScenarioLlmConfig,
  AgentScenarioLlmSlot,
  AgentSharedContextPolicy,
  AgentTaskDeliveryRecord,
  BotConversationTarget,
  ConversationAgentSnapshot,
  ConversationTurn,
  HistoryItem,
  HistoryStatus,
  PiStreamPayload,
  ResponseSegment,
  ToolCallEntry,
  TokenUsage,
} from '../../types'

export const HISTORY_STORAGE_KEY = 'nineclaw.history.v4'
const LEGACY_HISTORY_STORAGE_KEYS = ['yqagent.history.v4']
export const MAX_HISTORY_ITEMS = 30
const TITLE_MIN_LENGTH = 10
const TITLE_MAX_LENGTH = 20

export function createId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`
}

export function createActivity(label: string, detail: string, state: ActivityState): ActivityEntry {
  return {
    id: createId(),
    label,
    detail,
    state,
    createdAt: Date.now(),
  }
}

export function appendAgentTaskDeliveriesToHistory(
  previous: HistoryItem[],
  deliveries: AgentTaskDeliveryRecord[],
  onEachNewDelivery?: (delivery: AgentTaskDeliveryRecord) => void,
): HistoryItem[] {
  if (deliveries.length === 0) {
    return previous
  }

  const existingIds = new Set(previous.map((item) => item.id))
  const newSessionIds = new Set<string>()
  for (const delivery of deliveries) {
    if (!existingIds.has(delivery.sessionId)) {
      newSessionIds.add(delivery.sessionId)
    }
  }

  const now = Date.now()
  const prepended: HistoryItem[] = []
  for (const sessionId of newSessionIds) {
    const group = deliveries.filter((d) => d.sessionId === sessionId)
    const sorted = [...group].sort((a, b) => a.createdAt - b.createdAt)
    const first = sorted[0]!
    prepended.push({
      id: sessionId,
      title: first.title.trim() || '定时任务',
      status: 'done',
      createdAt: Math.min(...sorted.map((d) => d.createdAt)),
      updatedAt: now,
      turns: [],
      ...(first.agent ? { agent: first.agent } : {}),
    })
    existingIds.add(sessionId)
  }

  const next = [...prepended, ...previous]

  return next.map((item) => {
    const matches = deliveries.filter((delivery) => delivery.sessionId === item.id)
    if (matches.length === 0) {
      return item
    }

    const newDeliveries = matches.filter(
      (delivery) => !item.turns.some((turn) => turn.id === delivery.id),
    )
    for (const delivery of newDeliveries) {
      onEachNewDelivery?.(delivery)
    }

    const appendedTurns = newDeliveries.map((delivery) => ({
      id: delivery.id,
      prompt: `[系统定时任务] ${delivery.title}`,
      answer: delivery.content,
      status: 'done' as const,
      createdAt: delivery.createdAt,
      completedAt: delivery.createdAt,
      activity: [],
      thinking: '',
      toolCalls: [],
    }))

    if (appendedTurns.length === 0) {
      return item
    }

    return {
      ...item,
      updatedAt: now,
      turns: [...item.turns, ...appendedTurns],
    }
  })
}

export function updateLatestActivityState(
  activity: ActivityEntry[],
  label: string,
  nextState: ActivityState,
): ActivityEntry[] {
  for (let index = activity.length - 1; index >= 0; index -= 1) {
    if (activity[index]?.label === label) {
      return activity.map((item, currentIndex) =>
        currentIndex === index
          ? {
              ...item,
              state: nextState,
              completedAt: nextState === 'running' ? undefined : (item.completedAt ?? Date.now()),
            }
          : item,
      )
    }
  }

  return activity
}

export function isHistoryStatus(value: unknown): value is HistoryStatus {
  return (
    value === 'running' ||
    value === 'done' ||
    value === 'error' ||
    value === 'aborted_user' ||
    value === 'aborted_model'
  )
}

export function isActivityState(value: unknown): value is ActivityState {
  return value === 'running' || value === 'done' || value === 'error'
}

export function isAgentExecutionMode(value: unknown): value is AgentExecutionMode {
  return value === 'single' || value === 'supervisor' || value === 'worker'
}

export function isSharedContextPolicy(value: unknown): value is AgentSharedContextPolicy {
  return value === 'session' || value === 'summary' || value === 'none'
}

export function parseStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return []
  }

  return value.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
}

export function parseScenarioLlmSlot(value: unknown): AgentScenarioLlmSlot | undefined {
  if (typeof value !== 'object' || value === null) {
    return undefined
  }
  const o = value as Record<string, unknown>
  const providerId = typeof o.providerId === 'string' ? o.providerId.trim() : ''
  const model = typeof o.model === 'string' ? o.model.trim() : ''
  if (!providerId || !model) {
    return undefined
  }
  return { providerId, model }
}

export function parseScenarioLlmConfig(value: unknown): AgentScenarioLlmConfig | undefined {
  if (typeof value !== 'object' || value === null) {
    return undefined
  }
  const o = value as Record<string, unknown>
  const titleGeneration = parseScenarioLlmSlot(o.titleGeneration)
  const memoryExtraction = parseScenarioLlmSlot(o.memoryExtraction)
  const taskPushNotificationCopy = parseScenarioLlmSlot(o.taskPushNotificationCopy)
  if (!titleGeneration && !memoryExtraction && !taskPushNotificationCopy) {
    return undefined
  }
  return { titleGeneration, memoryExtraction, taskPushNotificationCopy }
}

export function parseAgentCollaborationConfig(value: unknown): AgentCollaborationConfig | undefined {
  if (typeof value !== 'object' || value === null) {
    return undefined
  }

  const candidate = value as Record<string, unknown>
  const allowedDelegateAgentIds = parseStringArray(candidate.allowedDelegateAgentIds)
  const handoffPrompt = typeof candidate.handoffPrompt === 'string' ? candidate.handoffPrompt : ''
  const sharedContextPolicy = isSharedContextPolicy(candidate.sharedContextPolicy)
    ? candidate.sharedContextPolicy
    : 'session'

  return {
    allowedDelegateAgentIds,
    handoffPrompt,
    sharedContextPolicy,
  }
}

export function parseConversationAgentSnapshot(value: unknown): ConversationAgentSnapshot | undefined {
  if (typeof value !== 'object' || value === null) {
    return undefined
  }

  const candidate = value as Record<string, unknown>
  if (
    typeof candidate.id !== 'string' ||
    typeof candidate.name !== 'string' ||
    typeof candidate.summary !== 'string' ||
    typeof candidate.description !== 'string'
  ) {
    return undefined
  }

  const scenarioLlmConfig = parseScenarioLlmConfig(candidate.scenarioLlmConfig)

  return {
    id: candidate.id,
    name: candidate.name,
    summary: candidate.summary,
    description: candidate.description,
    ...(typeof candidate.avatarUri === 'string' && candidate.avatarUri.trim()
      ? { avatarUri: candidate.avatarUri.trim() }
      : {}),
    systemPrompt: typeof candidate.systemPrompt === 'string' ? candidate.systemPrompt : '',
    capabilityPolicy: normalizeAgentCapabilityPolicy(
      candidate.capabilityPolicy as Partial<AgentCapabilityPolicy> | undefined,
      createStaticAgentCapabilityPolicy(),
    ),
    skillIds: parseStringArray(candidate.skillIds),
    allowedToolIds: normalizeAgentAllowedToolIds(
      Array.isArray(candidate.allowedToolIds) ? parseStringArray(candidate.allowedToolIds) : undefined,
    ),
    defaultProviderId: typeof candidate.defaultProviderId === 'string' ? candidate.defaultProviderId : '',
    defaultModel: typeof candidate.defaultModel === 'string' ? candidate.defaultModel : '',
    executionMode: isAgentExecutionMode(candidate.executionMode) ? candidate.executionMode : 'single',
    collaborationConfig: parseAgentCollaborationConfig(candidate.collaborationConfig),
    ...(typeof candidate.accentColor === 'string' && candidate.accentColor
      ? { accentColor: candidate.accentColor }
      : {}),
    ...(scenarioLlmConfig ? { scenarioLlmConfig } : {}),
  }
}

export function truncateTitle(value: string, maxLength = TITLE_MAX_LENGTH): string {
  const chars = Array.from(value.trim())
  if (chars.length <= maxLength) {
    return chars.join('')
  }

  if (maxLength <= 1) {
    return '…'
  }

  return `${chars.slice(0, maxLength - 1).join('')}…`
}

export function extractTitlePrefix(title: string | undefined): string {
  const normalized = title?.trim() ?? ''
  return normalized.match(/^\[[^\]]+\]\s*/)?.[0] ?? ''
}

export function normalizeTitleSource(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '$1')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1')
    .replace(/https?:\/\/\S+/g, ' ')
    .replace(/[#>*_~]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
}

export function cleanupTitleClause(text: string): string {
  return text
    .replace(
      /^(请你?|麻烦你?|帮我(?:把|将)?|帮忙|我想请你|我想让你|我需要你|需要你|请帮我|可以帮我|能否帮我|能不能帮我|请协助|协助我)\s*/u,
      '',
    )
    .replace(
      /^(please|help me(?: to)?|can you|could you|would you|i need you to|i need to|i want to)\s+/i,
      '',
    )
    .replace(/^(做下|做个|做一下|处理下|处理一下|看下|看一下|看看|检查下|检查一下|分析下|分析一下|优化下|优化一下)\s*/u, '')
    .replace(/^(关于|有关|针对|对于|围绕|这个|这个问题|这个需求|这里|目前|现在)\s*/u, '')
    .replace(/(?:可以吗|行吗|谢谢|thanks|thank you)[。！？!? ]*$/iu, '')
    .replace(/^[：:;,.，。！？!?、"'“”‘’()（）【】[\]-_\s]+|[：:;,.，。！？!?、"'“”‘’()（）【】[\]-_\s]+$/gu, '')
    .replace(/\s+/g, ' ')
    .trim()
}

export function isWeakTitleClause(text: string): boolean {
  return /^(一下|看看|分析|处理|优化|检查|修复|问题|需求|内容|结果|情况|消息|对话|回复|记录)$/u.test(text)
}

export function collectTitleClauses(text: string): string[] {
  return normalizeTitleSource(text)
    .split(/[\n。！？!?；;：:]+/)
    .flatMap((segment) => segment.split(/[，,、]/))
    .map((segment) => cleanupTitleClause(segment))
    .filter((segment) => segment.length > 1 && !isWeakTitleClause(segment))
}

export function cleanLlmSessionTitle(raw: string): string {
  return raw.replace(/^(标题|Title)[：:]\s*/iu, '').trim()
}

export function joinTitleParts(parts: string[]): string {
  if (parts.length === 0) {
    return ''
  }

  return parts.reduce((title, part) => {
    if (!title) {
      return part
    }

    const next = `${title}，${part}`
    return Array.from(next).length <= TITLE_MAX_LENGTH ? next : title
  }, '')
}

export function deriveConversationTitle(prompt: string, answer = '', existingTitle?: string): string {
  const prefix = extractTitlePrefix(existingTitle)
  const candidates = [...collectTitleClauses(prompt), ...collectTitleClauses(answer)]

  let baseTitle = ''
  const selectedParts: string[] = []

  for (const candidate of candidates) {
    const nextParts = [...selectedParts, candidate]
    const nextTitle = joinTitleParts(nextParts)
    if (!nextTitle) {
      continue
    }

    baseTitle = nextTitle
    selectedParts.push(candidate)
    if (Array.from(baseTitle).length >= TITLE_MIN_LENGTH) {
      break
    }
  }

  if (!baseTitle) {
    const fallback = cleanupTitleClause(normalizeTitleSource(prompt || answer || existingTitle || ''))
    baseTitle = fallback || '新会话'
  }

  const compactTitle = truncateTitle(baseTitle)
  return `${prefix}${compactTitle}`.trim()
}

export function deriveBotChannelLabel(channelId: string): string {
  const baseChannelId = channelId.split(':')[0] ?? channelId
  if (baseChannelId === 'wechat') {
    return '微信'
  }
  if (baseChannelId === 'lark') {
    return '飞书'
  }
  if (baseChannelId === 'peer') {
    return '外部'
  }
  return baseChannelId
}

export function buildBotConversationTitle(message: BotMessageEvent): string {
  const channelLabel = deriveBotChannelLabel(message.channel_id)
  const title = deriveConversationTitle(message.content)
  if (message.agent?.name) {
    return `[${channelLabel} · ${message.agent.name}] ${title}`
  }
  return `[${channelLabel}] ${title}`
}

export function withBotAgentMetadata(item: HistoryItem, message: BotMessageEvent): HistoryItem {
  const botTarget = {
    channelId: message.channel_id,
    userId: message.user_id,
  } satisfies BotConversationTarget

  if (!message.agent) {
    if (
      item.botTarget?.channelId === botTarget.channelId &&
      item.botTarget?.userId === botTarget.userId
    ) {
      return item
    }

    return {
      ...item,
      botTarget,
    }
  }

  const channelLabel = deriveBotChannelLabel(message.channel_id)
  const nextTitle =
    item.title.startsWith(`[${channelLabel}] `) && !item.title.startsWith(`[${channelLabel} · `)
      ? `[${channelLabel} · ${message.agent.name}] ${item.title.slice(`[${channelLabel}] `.length)}`
      : item.title

  return {
    ...item,
    title: nextTitle,
    agent: message.agent,
    botTarget,
  }
}

export function parseOptionalNumber(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value
  }
  if (typeof value === 'string' && value.trim() !== '') {
    const parsed = Number(value)
    if (Number.isFinite(parsed)) {
      return parsed
    }
  }
  return undefined
}

export function extractTokenUsage(raw: Record<string, unknown> | undefined): TokenUsage | undefined {
  if (!raw) {
    return undefined
  }

  // raw.usage covers the Tauri event path where PiTokenUsagePayload is a nested field.
  // raw.inputTokens etc. covers the flattened Tauri event path via #[serde(flatten)].
  // The short-name forms (input, output, cacheRead) cover direct PI JSON passthrough.
  const usage = raw.usage as Record<string, unknown> | undefined

  const inputTokens =
    parseOptionalNumber(
      usage?.inputTokens ?? usage?.input ?? raw.inputTokens ?? raw.input_tokens ?? raw.input,
    ) ?? 0
  const outputTokens =
    parseOptionalNumber(
      usage?.outputTokens ?? usage?.output ?? raw.outputTokens ?? raw.output_tokens ?? raw.output,
    ) ?? 0
  const cacheReadTokens =
    parseOptionalNumber(
      usage?.cacheReadTokens ?? usage?.cacheRead ?? raw.cacheReadTokens ?? raw.cache_read_tokens ?? raw.cacheRead,
    ) ?? 0
  const cacheWriteTokens =
    parseOptionalNumber(
      usage?.cacheWriteTokens ?? usage?.cacheWrite ?? raw.cacheWriteTokens ?? raw.cache_write_tokens ?? raw.cacheWrite,
    ) ?? 0
  const totalTokens =
    parseOptionalNumber(
      usage?.totalTokens ?? usage?.total ?? raw.totalTokens ?? raw.total_tokens,
    ) ??
    inputTokens + outputTokens + cacheReadTokens + cacheWriteTokens

  if (
    inputTokens === 0 &&
    outputTokens === 0 &&
    cacheReadTokens === 0 &&
    cacheWriteTokens === 0 &&
    totalTokens === 0
  ) {
    return undefined
  }

  // Prefer metadata from raw (flattened Tauri path), fall back to usage sub-object
  const api: string | undefined =
    (typeof raw.api === 'string' ? raw.api : undefined) ??
    (typeof usage?.api === 'string' ? (usage.api as string) : undefined)
  const provider: string | undefined =
    (typeof raw.provider === 'string' ? raw.provider : undefined) ??
    (typeof usage?.provider === 'string' ? (usage.provider as string) : undefined)
  const model: string | undefined =
    (typeof raw.model === 'string' ? raw.model : undefined) ??
    (typeof usage?.model === 'string' ? (usage.model as string) : undefined)
  const responseId: string | undefined =
    (typeof raw.responseId === 'string' ? raw.responseId : undefined) ??
    (typeof raw.response_id === 'string' ? raw.response_id : undefined) ??
    (typeof usage?.responseId === 'string' ? (usage.responseId as string) : undefined) ??
    (typeof usage?.response_id === 'string' ? (usage.response_id as string) : undefined)
  const timestamp: number | undefined =
    parseOptionalNumber(raw.timestamp) ??
    parseOptionalNumber(usage?.timestamp as number)

  return {
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheWriteTokens,
    totalTokens,
    api,
    provider,
    model,
    responseId,
    timestamp,
  }
}

export function parseResponseSegments(raw: unknown): ResponseSegment[] | undefined {
  if (raw === undefined) {
    return undefined
  }
  if (!Array.isArray(raw)) {
    return undefined
  }
  const out: ResponseSegment[] = []
  for (const item of raw) {
    if (typeof item !== 'object' || item === null) {
      return undefined
    }
    const seg = item as Record<string, unknown>
    if (seg.type === 'text' && typeof seg.text === 'string') {
      out.push({ type: 'text', text: seg.text })
      continue
    }
    if (seg.type === 'tool' && typeof seg.toolCallId === 'string') {
      out.push({ type: 'tool', toolCallId: seg.toolCallId })
      continue
    }
    if (
      seg.type === 'delegate_plan' &&
      typeof seg.planId === 'string' &&
      Array.isArray(seg.items)
    ) {
      out.push({
        type: 'delegate_plan',
        planId: seg.planId,
        items: seg.items as ResponseSegment extends { type: 'delegate_plan'; items: infer I }
          ? I
          : never,
      })
      continue
    }
    if (seg.type === 'delegation_run' && typeof seg.run === 'object' && seg.run !== null) {
      const run = seg.run as Record<string, unknown>
      if (typeof run.runId === 'string' && typeof run.assignee === 'string') {
        const rawTurns = Array.isArray(run.turns) ? (run.turns as unknown[]) : []
        const turns = rawTurns
          .map((item) => {
            if (typeof item !== 'object' || item === null) return null
            const t = item as Record<string, unknown>
            if (typeof t.index !== 'number') return null
            const kind = t.kind === 'agent' ? 'agent' : 'thinking'
            return {
              index: t.index,
              kind: kind as 'thinking' | 'agent',
              summary: typeof t.summary === 'string' ? t.summary : undefined,
            }
          })
          .filter((v): v is NonNullable<typeof v> => v !== null)

        const rawCalls = Array.isArray(run.toolCalls) ? (run.toolCalls as unknown[]) : []
        const toolCalls = rawCalls
          .map((item) => {
            if (typeof item !== 'object' || item === null) return null
            const t = item as Record<string, unknown>
            if (typeof t.index !== 'number' || typeof t.toolName !== 'string') return null
            const status =
              t.status === 'running' || t.status === 'error' ? t.status : 'done'
            return {
              index: t.index,
              toolCallId: typeof t.toolCallId === 'string' ? t.toolCallId : '',
              toolName: t.toolName,
              argsDigest: typeof t.argsDigest === 'string' ? t.argsDigest : undefined,
              status: status as 'running' | 'done' | 'error',
              isError: typeof t.isError === 'boolean' ? t.isError : undefined,
            }
          })
          .filter((v): v is NonNullable<typeof v> => v !== null)

        out.push({
          type: 'delegation_run',
          run: {
            runId: run.runId,
            assignee: run.assignee,
            task: typeof run.task === 'string' ? run.task : '',
            status:
              (run.status as 'pending' | 'running' | 'done' | 'aborted' | 'error') ?? 'pending',
            output: typeof run.output === 'string' ? run.output : '',
            startedAt: typeof run.startedAt === 'number' ? run.startedAt : undefined,
            elapsedMs: typeof run.elapsedMs === 'number' ? run.elapsedMs : undefined,
            error: typeof run.error === 'string' ? run.error : null,
            ...(turns.length > 0 ? { turns } : {}),
            ...(toolCalls.length > 0 ? { toolCalls } : {}),
          },
        })
        continue
      }
    }
    return undefined
  }
  return out
}

/** 从 `turn.answer` 里解析 `<!--NC_DELEGATE_PLAN:{...}-->` 占位为 delegate_plan 片段。 */
export function extractDelegatePlanSegmentsFromAnswer(answer: string): {
  cleaned: string
  segments: ResponseSegment[]
} {
  const re = /<!--NC_DELEGATE_PLAN:(\{[\s\S]*?\})-->/g
  const segments: ResponseSegment[] = []
  const cleaned = answer.replace(re, (_, jsonPart: string) => {
    try {
      const parsed = JSON.parse(jsonPart) as {
        planId?: string
        items?: unknown
      }
      if (parsed.planId && Array.isArray(parsed.items)) {
        segments.push({
          type: 'delegate_plan',
          planId: parsed.planId,
          items: parsed.items as ResponseSegment extends { type: 'delegate_plan'; items: infer I }
            ? I
            : never,
        })
      }
    } catch {
      // 忽略无效 JSON，原样落到正文
      return jsonPart
    }
    return ''
  })
  return { cleaned, segments }
}

export function parseBotConversationTarget(value: unknown): BotConversationTarget | undefined {
  if (typeof value !== 'object' || value === null) {
    return undefined
  }

  const candidate = value as Record<string, unknown>
  const channelId = typeof candidate.channelId === 'string' ? candidate.channelId.trim() : ''
  const userId = typeof candidate.userId === 'string' ? candidate.userId.trim() : ''

  if (!channelId || !userId) {
    return undefined
  }

  return {
    channelId,
    userId,
  }
}

export function parseHistorySnapshot(raw: string | null): HistoryItem[] {
  try {
    if (!raw) {
      return []
    }

    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) {
      return []
    }

    return parsed
      .map((item) => {
        if (typeof item !== 'object' || item === null) {
          return null
        }

        const candidate = item as Record<string, unknown>
        const id = candidate.id
        const title = candidate.title
        const status = candidate.status
        const createdAt = candidate.createdAt
        const updatedAt = candidate.updatedAt
        const turns = candidate.turns

        if (
          typeof id !== 'string' ||
          typeof title !== 'string' ||
          typeof createdAt !== 'number' ||
          typeof updatedAt !== 'number' ||
          !isHistoryStatus(status) ||
          !Array.isArray(turns)
        ) {
          return null
        }

        const parsedTurns = turns
          .map((turn) => {
            if (typeof turn !== 'object' || turn === null) {
              return null
            }

            const current = turn as Record<string, unknown>
            const activity = current.activity
            const toolCalls = current.toolCalls

            if (
              typeof current.id !== 'string' ||
              typeof current.prompt !== 'string' ||
              typeof current.answer !== 'string' ||
              typeof current.createdAt !== 'number' ||
              typeof current.thinking !== 'string' ||
              !isHistoryStatus(current.status) ||
              !Array.isArray(activity) ||
              !Array.isArray(toolCalls)
            ) {
              return null
            }

            const parsedActivity = activity
              .map((entry) => {
                if (typeof entry !== 'object' || entry === null) {
                  return null
                }

                const log = entry as Record<string, unknown>
                if (
                  typeof log.id !== 'string' ||
                  typeof log.label !== 'string' ||
                  typeof log.detail !== 'string' ||
                  typeof log.createdAt !== 'number' ||
                  !isActivityState(log.state)
                ) {
                  return null
                }

                const completedAt = parseOptionalNumber(log.completedAt)

                return {
                  id: log.id,
                  label: log.label,
                  detail: log.detail,
                  state: log.state,
                  createdAt: log.createdAt,
                  ...(typeof completedAt === 'number' ? { completedAt } : {}),
                } satisfies ActivityEntry
              })
              .filter((entry): entry is ActivityEntry => entry !== null)

            const parsedToolCalls = toolCalls
              .map((entry) => {
                if (typeof entry !== 'object' || entry === null) {
                  return null
                }

                const tool = entry as Record<string, unknown>
                if (
                  typeof tool.id !== 'string' ||
                  typeof tool.toolCallId !== 'string' ||
                  typeof tool.toolName !== 'string' ||
                  typeof tool.argsText !== 'string' ||
                  typeof tool.resultText !== 'string' ||
                  typeof tool.createdAt !== 'number' ||
                  !isActivityState(tool.state)
                ) {
                  return null
                }

                const completedAt = parseOptionalNumber(tool.completedAt)

                return {
                  id: tool.id,
                  toolCallId: tool.toolCallId,
                  toolName: tool.toolName,
                  argsText: tool.argsText,
                  resultText: tool.resultText,
                  state: tool.state,
                  createdAt: tool.createdAt,
                  ...(typeof completedAt === 'number' ? { completedAt } : {}),
                } satisfies ToolCallEntry
              })
              .filter((entry): entry is ToolCallEntry => entry !== null)

            const completedAt = parseOptionalNumber(current.completedAt)
            // 必须传整段 turn：用量可能在 `usage` 内，也可能与 pi 流式事件一样摊平在 turn 根上；
            // 仅传 current.usage 会在 usage 为 null/缺失时丢光 token。
            const usage = extractTokenUsage(current)
            const parsedResponseSegmentsStored = parseResponseSegments(current.responseSegments)

            const rawAnswer = typeof current.answer === 'string' ? current.answer : ''
            const { cleaned: cleanedAnswer, segments: planSegmentsFromAnswer } =
              extractDelegatePlanSegmentsFromAnswer(rawAnswer)
            const parsedResponseSegments =
              planSegmentsFromAnswer.length > 0
                ? [...(parsedResponseSegmentsStored ?? []), ...planSegmentsFromAnswer]
                : parsedResponseSegmentsStored

            const speakerAgentId = typeof current.speakerAgentId === 'string' ? current.speakerAgentId : undefined

            return {
              id: current.id,
              prompt: current.prompt,
              answer: cleanedAnswer,
              status: current.status,
              createdAt: current.createdAt,
              ...(typeof completedAt === 'number' ? { completedAt } : {}),
              ...(usage ? { usage } : {}),
              activity: parsedActivity,
              thinking: current.thinking,
              toolCalls: parsedToolCalls,
              ...(parsedResponseSegments ? { responseSegments: parsedResponseSegments } : {}),
              ...(speakerAgentId ? { speakerAgentId } : {}),
            } satisfies ConversationTurn
          })
          .filter((turn): turn is ConversationTurn => turn !== null)

        const sessionLlmProviderId = candidate.sessionLlmProviderId
        const sessionLlmModel = candidate.sessionLlmModel
        const workspaceId = candidate.workspaceId
        const agent = parseConversationAgentSnapshot(candidate.agent)
        const botTarget = parseBotConversationTarget(candidate.botTarget)

        return {
          id,
          title: deriveConversationTitle(parsedTurns[0]?.prompt ?? title, parsedTurns[0]?.answer ?? '', title),
          status,
          createdAt,
          updatedAt,
          turns: parsedTurns,
          ...(agent ? { agent } : {}),
          ...(botTarget ? { botTarget } : {}),
          ...(typeof sessionLlmProviderId === 'string' && sessionLlmProviderId
            ? { sessionLlmProviderId }
            : {}),
          ...(typeof sessionLlmModel === 'string' ? { sessionLlmModel } : {}),
          ...(typeof workspaceId === 'string' && workspaceId ? { workspaceId } : {}),
        } satisfies HistoryItem
      })
      .filter((item): item is HistoryItem => item !== null)
      .slice(0, MAX_HISTORY_ITEMS)
  } catch {
    return []
  }
}

export function readHistoryStorageValue(): string | null {
  const keys = [HISTORY_STORAGE_KEY, ...LEGACY_HISTORY_STORAGE_KEYS]
  for (const key of keys) {
    const raw = localStorage.getItem(key)
    if (raw === null) {
      continue
    }
    if (key !== HISTORY_STORAGE_KEY) {
      localStorage.setItem(HISTORY_STORAGE_KEY, raw)
    }
    return raw
  }
  return null
}

export function clearLegacyHistoryStorage() {
  for (const key of LEGACY_HISTORY_STORAGE_KEYS) {
    if (key !== HISTORY_STORAGE_KEY) {
      localStorage.removeItem(key)
    }
  }
}

export function loadLegacyHistoryFromStorage(): HistoryItem[] {
  return parseHistorySnapshot(readHistoryStorageValue())
}

export function appendTextToSegments(segments: ResponseSegment[] | undefined, chunk: string): ResponseSegment[] {
  if (!chunk) {
    return segments ?? []
  }
  const base = segments ?? []
  if (base.length === 0) {
    return [{ type: 'text', text: chunk }]
  }
  const last = base[base.length - 1]
  if (last.type === 'text') {
    return [...base.slice(0, -1), { type: 'text', text: last.text + chunk }]
  }
  return [...base, { type: 'text', text: chunk }]
}

export function buildNewTurn(prompt: string, speakerAgentId?: string | null): ConversationTurn {
  return {
    id: createId(),
    prompt,
    answer: '',
    status: 'running',
    createdAt: Date.now(),
    completedAt: undefined,
    usage: undefined,
    activity: [],
    thinking: '',
    toolCalls: [],
    responseSegments: [],
    ...(speakerAgentId ? { speakerAgentId } : {}),
  }
}

export function buildToolCallEntry(payload: PiStreamPayload): ToolCallEntry {
  return {
    id: createId(),
    toolCallId: payload.toolCallId ?? payload.tool_call_id ?? createId(),
    toolName: payload.toolName ?? payload.tool_name ?? 'tool',
    argsText: payload.argsText ?? payload.args_text ?? '',
    resultText: payload.resultText ?? payload.result_text ?? '',
    state: payload.isError ?? payload.is_error ? 'error' : 'running',
    createdAt: Date.now(),
    completedAt: undefined,
  }
}

export function parseUsageFromPayload(payload: PiStreamPayload): TokenUsage | undefined {
  return extractTokenUsage(payload as unknown as Record<string, unknown>)
}

export function getHistoryStatusLabel(status: HistoryStatus): string {
  if (status === 'running') return '思考中'
  if (status === 'done') return '已完成'
  if (status === 'error') return '错误'
  if (status === 'aborted_user') return '已停止'
  return '模型中断'
}
