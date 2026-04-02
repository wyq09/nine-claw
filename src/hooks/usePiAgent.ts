import { useEffect, useEffectEvent, useRef, useState } from 'react'
import {
  abortPiStream,
  clearHistoryState,
  clearPiSession,
  clearPiSessionForId,
  loadHistoryState,
  saveHistoryState,
  streamPiPrompt,
  subscribeBotMessage,
  subscribePiStream,
} from '../lib/piClient'
import type { BotMessageEvent } from '../lib/piClient'
import type {
  ActivityEntry,
  ActivityState,
  AgentCollaborationConfig,
  AgentExecutionMode,
  AgentSharedContextPolicy,
  ConversationAgentSnapshot,
  ConversationTurn,
  HistoryItem,
  HistoryStatus,
  PiStreamPayload,
  ProviderId,
  ProviderRuntimeConfig,
  TokenUsage,
  ResponseSegment,
  ToolCallEntry,
} from '../types'

const HISTORY_STORAGE_KEY = 'nineclaw.history.v4'
const LEGACY_HISTORY_STORAGE_KEYS = ['yqagent.history.v4']
const MAX_HISTORY_ITEMS = 30
const TITLE_MAX_LENGTH = 24

function createId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`
}

function createActivity(label: string, detail: string, state: ActivityState): ActivityEntry {
  return {
    id: createId(),
    label,
    detail,
    state,
    createdAt: Date.now(),
  }
}

function updateLatestActivityState(
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

function isHistoryStatus(value: unknown): value is HistoryStatus {
  return (
    value === 'running' ||
    value === 'done' ||
    value === 'error' ||
    value === 'aborted_user' ||
    value === 'aborted_model'
  )
}

function isActivityState(value: unknown): value is ActivityState {
  return value === 'running' || value === 'done' || value === 'error'
}

function isAgentExecutionMode(value: unknown): value is AgentExecutionMode {
  return value === 'single' || value === 'supervisor' || value === 'worker'
}

function isSharedContextPolicy(value: unknown): value is AgentSharedContextPolicy {
  return value === 'session' || value === 'summary' || value === 'none'
}

function parseStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return []
  }

  return value.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
}

function parseAgentCollaborationConfig(value: unknown): AgentCollaborationConfig | undefined {
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

function parseConversationAgentSnapshot(value: unknown): ConversationAgentSnapshot | undefined {
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

  return {
    id: candidate.id,
    name: candidate.name,
    summary: candidate.summary,
    description: candidate.description,
    systemPrompt: typeof candidate.systemPrompt === 'string' ? candidate.systemPrompt : '',
    skillIds: parseStringArray(candidate.skillIds),
    defaultProviderId: typeof candidate.defaultProviderId === 'string' ? candidate.defaultProviderId : '',
    defaultModel: typeof candidate.defaultModel === 'string' ? candidate.defaultModel : '',
    executionMode: isAgentExecutionMode(candidate.executionMode) ? candidate.executionMode : 'single',
    collaborationConfig: parseAgentCollaborationConfig(candidate.collaborationConfig),
    ...(typeof candidate.accentColor === 'string' && candidate.accentColor
      ? { accentColor: candidate.accentColor }
      : {}),
  }
}

function truncateTitle(value: string, maxLength = TITLE_MAX_LENGTH): string {
  const chars = Array.from(value.trim())
  if (chars.length <= maxLength) {
    return chars.join('')
  }

  return `${chars.slice(0, maxLength).join('')}…`
}

function deriveConversationTitle(prompt: string, existingTitle?: string): string {
  const promptText = prompt.replace(/\s+/g, ' ').trim()
  const titleText = existingTitle?.replace(/\s+/g, ' ').trim() ?? ''

  if (titleText && titleText !== promptText && Array.from(titleText).length <= TITLE_MAX_LENGTH + 2) {
    return titleText
  }

  const withoutMarkdown = promptText
    .replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '$1')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1')

  const firstClause =
    withoutMarkdown
      .split(/[\n。！？!?；;]+/)
      .map((part) => part.trim())
      .find(Boolean) ?? withoutMarkdown

  const normalized = firstClause
    .replace(
      /^(请你?|麻烦你?|帮我(?:把|将)?|帮忙|我想请你|我想让你|我需要你|需要你|请帮我|可以帮我|能否帮我|能不能帮我)\s*/u,
      '',
    )
    .replace(
      /^(please|help me(?: to)?|can you|could you|would you|i need you to|i need to|i want to)\s+/i,
      '',
    )
    .replace(/^(做下|做个|做一下|处理下|处理一下|看下|看一下|看看|检查下|检查一下|分析下|分析一下)\s*/u, '')
    .replace(/(?:可以吗|行吗|谢谢|thanks)[。！？!? ]*$/iu, '')
    .replace(/^[：:;,.，。！？!?、"'“”‘’()（）【】[\]-_\s]+|[：:;,.，。！？!?、"'“”‘’()（）【】[\]-_\s]+$/gu, '')

  const title = normalized || titleText || promptText
  return truncateTitle(title || '新会话')
}

function parseOptionalNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined
}

function extractTokenUsage(raw: Record<string, unknown> | undefined): TokenUsage | undefined {
  if (!raw) {
    return undefined
  }

  const inputTokens = parseOptionalNumber(raw.inputTokens ?? raw.input_tokens) ?? 0
  const outputTokens = parseOptionalNumber(raw.outputTokens ?? raw.output_tokens) ?? 0
  const cacheReadTokens = parseOptionalNumber(raw.cacheReadTokens ?? raw.cache_read_tokens) ?? 0
  const cacheWriteTokens = parseOptionalNumber(raw.cacheWriteTokens ?? raw.cache_write_tokens) ?? 0
  const totalTokens =
    parseOptionalNumber(raw.totalTokens ?? raw.total_tokens) ??
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

  return {
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheWriteTokens,
    totalTokens,
  }
}

function parseResponseSegments(raw: unknown): ResponseSegment[] | undefined {
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
    return undefined
  }
  return out
}

function parseHistorySnapshot(raw: string | null): HistoryItem[] {
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
            const usage = extractTokenUsage(
              typeof current.usage === 'object' && current.usage !== null
                ? (current.usage as Record<string, unknown>)
                : undefined,
            )
            const parsedResponseSegments = parseResponseSegments(current.responseSegments)

            return {
              id: current.id,
              prompt: current.prompt,
              answer: current.answer,
              status: current.status,
              createdAt: current.createdAt,
              ...(typeof completedAt === 'number' ? { completedAt } : {}),
              ...(usage ? { usage } : {}),
              activity: parsedActivity,
              thinking: current.thinking,
              toolCalls: parsedToolCalls,
              ...(parsedResponseSegments ? { responseSegments: parsedResponseSegments } : {}),
            } satisfies ConversationTurn
          })
          .filter((turn): turn is ConversationTurn => turn !== null)

        const sessionLlmProviderId = candidate.sessionLlmProviderId
        const sessionLlmModel = candidate.sessionLlmModel
        const agent = parseConversationAgentSnapshot(candidate.agent)

        return {
          id,
          title: deriveConversationTitle(parsedTurns[0]?.prompt ?? title, title),
          status,
          createdAt,
          updatedAt,
          turns: parsedTurns,
          ...(agent ? { agent } : {}),
          ...(typeof sessionLlmProviderId === 'string' && sessionLlmProviderId
            ? { sessionLlmProviderId }
            : {}),
          ...(typeof sessionLlmModel === 'string' ? { sessionLlmModel } : {}),
        } satisfies HistoryItem
      })
      .filter((item): item is HistoryItem => item !== null)
      .slice(0, MAX_HISTORY_ITEMS)
  } catch {
    return []
  }
}

function readHistoryStorageValue(): string | null {
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

function clearLegacyHistoryStorage() {
  for (const key of LEGACY_HISTORY_STORAGE_KEYS) {
    if (key !== HISTORY_STORAGE_KEY) {
      localStorage.removeItem(key)
    }
  }
}

function loadLegacyHistoryFromStorage(): HistoryItem[] {
  return parseHistorySnapshot(readHistoryStorageValue())
}

function appendTextToSegments(segments: ResponseSegment[] | undefined, chunk: string): ResponseSegment[] {
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

function buildNewTurn(prompt: string): ConversationTurn {
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
  }
}

function buildToolCallEntry(payload: PiStreamPayload): ToolCallEntry {
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

function parseUsageFromPayload(payload: PiStreamPayload): TokenUsage | undefined {
  const inputTokens = payload.inputTokens ?? payload.input_tokens ?? 0
  const outputTokens = payload.outputTokens ?? payload.output_tokens ?? 0
  const cacheReadTokens = payload.cacheReadTokens ?? payload.cache_read_tokens ?? 0
  const cacheWriteTokens = payload.cacheWriteTokens ?? payload.cache_write_tokens ?? 0
  const totalTokens =
    payload.totalTokens ??
    payload.total_tokens ??
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

  return {
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheWriteTokens,
    totalTokens,
  }
}

export function getHistoryStatusLabel(status: HistoryStatus): string {
  if (status === 'running') return '思考中'
  if (status === 'done') return '已完成'
  if (status === 'error') return '错误'
  if (status === 'aborted_user') return '已停止'
  return '模型中断'
}

export function usePiAgent() {
  const [draft, setDraft] = useState('')
  const [error, setError] = useState('')
  const [runningHistoryIds, setRunningHistoryIds] = useState<string[]>([])
  const [history, setHistory] = useState<HistoryItem[]>([])
  const [activeHistoryId, setActiveHistoryId] = useState<string>('')
  const [historyHydrated, setHistoryHydrated] = useState(false)
  const currentTurnIdsRef = useRef<Map<string, string>>(new Map())
  const receivedFirstDeltaRef = useRef<Map<string, boolean>>(new Map())

  const updateHistoryItem = (id: string, updater: (item: HistoryItem) => HistoryItem) => {
    setHistory((previous) => previous.map((item) => (item.id === id ? updater(item) : item)))
  }

  const updateTurn = (
    historyId: string,
    turnId: string,
    updater: (turn: ConversationTurn) => ConversationTurn,
  ) => {
    updateHistoryItem(historyId, (item) => ({
      ...item,
      updatedAt: Date.now(),
      turns: item.turns.map((turn) => (turn.id === turnId ? updater(turn) : turn)),
    }))
  }

  const appendActivity = (historyId: string, turnId: string, label: string, detail: string, state: ActivityState) => {
    updateTurn(historyId, turnId, (turn) => ({
      ...turn,
      activity: [...turn.activity, createActivity(label, detail, state)],
    }))
  }

  const setLatestActivityState = (historyId: string, turnId: string, label: string, state: ActivityState) => {
    updateTurn(historyId, turnId, (turn) => ({
      ...turn,
      activity: updateLatestActivityState(turn.activity, label, state),
    }))
  }

  const appendThinking = (historyId: string, turnId: string, chunk: string) => {
    updateTurn(historyId, turnId, (turn) => ({
      ...turn,
      thinking: turn.thinking + chunk,
    }))
  }

  const updateHistoryToolCall = (
    historyId: string,
    turnId: string,
    toolCallId: string,
    updater: (toolCall: ToolCallEntry) => ToolCallEntry,
  ) => {
    updateTurn(historyId, turnId, (turn) => ({
      ...turn,
      toolCalls: turn.toolCalls.map((toolCall) =>
        toolCall.toolCallId === toolCallId ? updater(toolCall) : toolCall,
      ),
    }))
  }

  const updateSessionStatus = (historyId: string, status: HistoryStatus) => {
    updateHistoryItem(historyId, (item) => ({
      ...item,
      status,
      updatedAt: Date.now(),
    }))
  }

  const markSessionRunning = (historyId: string) => {
    setRunningHistoryIds((previous) =>
      previous.includes(historyId) ? previous : [...previous, historyId],
    )
  }

  const markSessionSettled = (historyId: string) => {
    setRunningHistoryIds((previous) => previous.filter((item) => item !== historyId))
    currentTurnIdsRef.current.delete(historyId)
    receivedFirstDeltaRef.current.delete(historyId)
  }

  useEffect(() => {
    let isMounted = true

    void (async () => {
      try {
        const sqlitePayload = await loadHistoryState()
        let nextHistory = parseHistorySnapshot(sqlitePayload)

        if (nextHistory.length === 0) {
          const legacyHistory = loadLegacyHistoryFromStorage()
          if (legacyHistory.length > 0) {
            nextHistory = legacyHistory
            await saveHistoryState(JSON.stringify(legacyHistory))
            clearLegacyHistoryStorage()
          }
        }

        if (!isMounted) {
          return
        }

        setHistory(nextHistory)
        setActiveHistoryId((current) => (current && nextHistory.some((item) => item.id === current) ? current : (nextHistory[0]?.id ?? '')))
      } catch (loadError) {
        if (!isMounted) {
          return
        }

        const message = loadError instanceof Error ? loadError.message : String(loadError)
        setError((current) => current || `读取历史会话失败：${message}`)
      } finally {
        if (isMounted) {
          setHistoryHydrated(true)
        }
      }
    })()

    return () => {
      isMounted = false
    }
  }, [])

  useEffect(() => {
    if (!historyHydrated) {
      return
    }

    clearLegacyHistoryStorage()
    void saveHistoryState(JSON.stringify(history))
  }, [history, historyHydrated])

  const handleStreamPayload = useEffectEvent((payload: PiStreamPayload) => {
    const currentHistoryId = payload.sessionId ?? payload.session_id ?? ''
    const currentTurnId = currentTurnIdsRef.current.get(currentHistoryId) ?? ''
    if (!currentHistoryId || !currentTurnId) {
      return
    }

    if (payload.event === 'start') {
      setError('')
      appendActivity(
        currentHistoryId,
        currentTurnId,
        '连接 pi 主脑',
        '正在建立本次 RPC 会话，并保持当前 session 以支持后续连续对话。',
        'running',
      )
      return
    }

    if (payload.event === 'thinking_start') {
      appendActivity(
        currentHistoryId,
        currentTurnId,
        '深度思考中',
        'pi 正在输出 thinking 流，界面会按流式内容实时更新。',
        'running',
      )
      return
    }

    if (payload.event === 'thinking_delta' && payload.text) {
      appendThinking(currentHistoryId, currentTurnId, payload.text)
      return
    }

    if (payload.event === 'thinking_end') {
      setLatestActivityState(currentHistoryId, currentTurnId, '深度思考中', 'done')
      return
    }

    if (payload.event === 'delta' && payload.text) {
      const deltaText = payload.text
      updateTurn(currentHistoryId, currentTurnId, (turn) => ({
        ...turn,
        answer: turn.answer + deltaText,
        responseSegments: appendTextToSegments(turn.responseSegments, deltaText),
        status: 'running',
      }))
      updateSessionStatus(currentHistoryId, 'running')
      markSessionRunning(currentHistoryId)

      if (!receivedFirstDeltaRef.current.get(currentHistoryId)) {
        receivedFirstDeltaRef.current.set(currentHistoryId, true)
        setLatestActivityState(currentHistoryId, currentTurnId, '连接 pi 主脑', 'done')
        appendActivity(
          currentHistoryId,
          currentTurnId,
          '流式输出中',
          '已收到首段内容，正在持续接收并拼接响应。',
          'running',
        )
      }
      return
    }

    if (payload.event === 'tool_execution_start') {
      const entry = buildToolCallEntry(payload)
      updateTurn(currentHistoryId, currentTurnId, (turn) => {
        const index = turn.toolCalls.findIndex((toolCall) => toolCall.toolCallId === entry.toolCallId)
        if (index === -1) {
          return {
            ...turn,
            toolCalls: [...turn.toolCalls, entry],
            responseSegments: [...(turn.responseSegments ?? []), { type: 'tool', toolCallId: entry.toolCallId }],
          }
        }
        return {
          ...turn,
          toolCalls: turn.toolCalls.map((toolCall, currentIndex) =>
            currentIndex === index ? { ...toolCall, ...entry } : toolCall,
          ),
        }
      })
      return
    }

    if (payload.event === 'tool_execution_update') {
      const toolCallId = payload.toolCallId ?? payload.tool_call_id
      if (!toolCallId) {
        return
      }

      const argsDelta = payload.argsDelta ?? payload.args_delta
      const resultDelta = payload.resultDelta ?? payload.result_delta

      updateHistoryToolCall(currentHistoryId, currentTurnId, toolCallId, (toolCall) => {
        const nextArgs =
          typeof argsDelta === 'string' && argsDelta.length > 0
            ? toolCall.argsText + argsDelta
            : (payload.argsText ?? payload.args_text ?? toolCall.argsText)
        const nextResult =
          typeof resultDelta === 'string' && resultDelta.length > 0
            ? toolCall.resultText + resultDelta
            : (payload.resultText ?? payload.result_text ?? toolCall.resultText)
        return {
          ...toolCall,
          argsText: nextArgs,
          resultText: nextResult,
          state: 'running',
        }
      })
      return
    }

    if (payload.event === 'tool_execution_end') {
      const toolCallId = payload.toolCallId ?? payload.tool_call_id
      if (!toolCallId) {
        return
      }

      updateTurn(currentHistoryId, currentTurnId, (turn) => ({
        ...turn,
        toolCalls: turn.toolCalls.map((toolCall) =>
          toolCall.toolCallId === toolCallId
            ? {
                ...toolCall,
                argsText: payload.argsText ?? payload.args_text ?? toolCall.argsText,
                resultText: payload.resultText ?? payload.result_text ?? toolCall.resultText,
                state: payload.isError ?? payload.is_error ? 'error' : 'done',
                completedAt: toolCall.completedAt ?? Date.now(),
              }
            : toolCall,
        ),
      }))
      return
    }

    if (payload.event === 'aborted') {
      const source = payload.abortedBy ?? payload.aborted_by ?? 'user'
      setError(source === 'model' ? '模型中断了当前生成' : '已中止当前生成')
      setLatestActivityState(currentHistoryId, currentTurnId, '连接 pi 主脑', 'done')
      setLatestActivityState(currentHistoryId, currentTurnId, '流式输出中', 'error')
      setLatestActivityState(currentHistoryId, currentTurnId, '深度思考中', 'error')
      appendActivity(
        currentHistoryId,
        currentTurnId,
        source === 'model' ? '模型中断' : '人工停止',
        source === 'model'
          ? 'pi 返回了 aborted 事件，当前会话未完整结束。'
          : '已向 pi 发送 abort 指令，中止当前生成。',
        'error',
      )
      updateTurn(currentHistoryId, currentTurnId, (turn) => ({
        ...turn,
        status: source === 'model' ? 'aborted_model' : 'aborted_user',
        completedAt: turn.completedAt ?? Date.now(),
      }))
      updateSessionStatus(currentHistoryId, source === 'model' ? 'aborted_model' : 'aborted_user')
      markSessionSettled(currentHistoryId)
      return
    }

    if (payload.event === 'done') {
      const usage = parseUsageFromPayload(payload)
      setLatestActivityState(currentHistoryId, currentTurnId, '连接 pi 主脑', 'done')
      setLatestActivityState(currentHistoryId, currentTurnId, '流式输出中', 'done')
      setLatestActivityState(currentHistoryId, currentTurnId, '深度思考中', 'done')
      appendActivity(currentHistoryId, currentTurnId, '回复完成', 'pi 已返回完整结果，本轮对话结束。', 'done')
      updateTurn(currentHistoryId, currentTurnId, (turn) => ({
        ...turn,
        status: 'done',
        completedAt: turn.completedAt ?? Date.now(),
        usage: usage ?? turn.usage,
      }))
      updateSessionStatus(currentHistoryId, 'done')
      markSessionSettled(currentHistoryId)
      return
    }

    if (payload.event === 'error' && payload.error) {
      const errorText = payload.error
      setError(errorText)
      setLatestActivityState(currentHistoryId, currentTurnId, '连接 pi 主脑', 'error')
      setLatestActivityState(currentHistoryId, currentTurnId, '流式输出中', 'error')
      setLatestActivityState(currentHistoryId, currentTurnId, '深度思考中', 'error')
      appendActivity(currentHistoryId, currentTurnId, '执行失败', errorText, 'error')
      updateTurn(currentHistoryId, currentTurnId, (turn) => ({
        ...turn,
        status: 'error',
        answer: turn.answer || errorText,
        responseSegments: turn.answer
          ? turn.responseSegments
          : appendTextToSegments(turn.responseSegments, errorText),
        completedAt: turn.completedAt ?? Date.now(),
      }))
      updateSessionStatus(currentHistoryId, 'error')
      markSessionSettled(currentHistoryId)
    }
  })

  useEffect(() => {
    let isMounted = true
    let unsubscribe: (() => void) | undefined

    void subscribePiStream((payload) => {
      if (!isMounted) {
        return
      }
      handleStreamPayload(payload)
    }).then((unlisten) => {
      unsubscribe = unlisten
    })

    return () => {
      isMounted = false
      if (unsubscribe) {
        unsubscribe()
      }
    }
  }, [])

  // ── Bot channel message history integration ──

  /** Map `channel_id:user_id` → { historyId, turnId } for tracking active bot sessions. */
  const botSessionMapRef = useRef<Map<string, { historyId: string; turnId: string }>>(new Map())

  const handleBotMessage = useEffectEvent((msg: BotMessageEvent) => {
    const sessionKey = `${msg.channel_id}:${msg.user_id}`

    if (msg.direction === 'inbound') {
      // User sent a message to the bot → create new turn in a dedicated session
      const existing = botSessionMapRef.current.get(sessionKey)
      const turn = buildNewTurn(msg.content)
      const now = msg.timestamp || Date.now()

      if (existing) {
        // Append a new turn to existing session
        const { historyId } = existing
        botSessionMapRef.current.set(sessionKey, { historyId, turnId: turn.id })
        setHistory((prev) => {
          const updated = prev.map((item): HistoryItem =>
            item.id === historyId
              ? {
                  ...item,
                  status: 'running',
                  updatedAt: now,
                  turns: [...item.turns, turn],
                }
              : item,
          )
          const current = updated.find((item) => item.id === historyId)
          const others = updated.filter((item) => item.id !== historyId)
          return current ? [current, ...others].slice(0, MAX_HISTORY_ITEMS) : updated.slice(0, MAX_HISTORY_ITEMS)
        })
      } else {
        // First message from this channel:user — create new history item
        const historyId = createId()
        botSessionMapRef.current.set(sessionKey, { historyId, turnId: turn.id })
        const channelLabel = msg.channel_id === 'wechat' ? '微信' : msg.channel_id
        const newSession: HistoryItem = {
          id: historyId,
          title: `[${channelLabel}] ${deriveConversationTitle(msg.content)}`,
          status: 'running',
          createdAt: now,
          updatedAt: now,
          turns: [turn],
        }
        setHistory((prev) => [newSession, ...prev].slice(0, MAX_HISTORY_ITEMS))
      }
      return
    }

    const session = botSessionMapRef.current.get(sessionKey)
    if (!session) {
      return
    }
    const { historyId, turnId } = session

    if (msg.direction === 'outbound_chunk') {
      // Streaming chunk from AI → append to answer
      updateTurn(historyId, turnId, (turn) => ({
        ...turn,
        answer: turn.answer + msg.content,
        responseSegments: appendTextToSegments(turn.responseSegments, msg.content),
        status: 'running',
      }))
      updateSessionStatus(historyId, 'running')
      return
    }

    if (msg.direction === 'outbound_done') {
      // Final complete reply → set answer to full text, mark done
      updateTurn(historyId, turnId, (turn) => ({
        ...turn,
        answer: msg.content,
        responseSegments: [{ type: 'text', text: msg.content }],
        status: 'done',
        completedAt: turn.completedAt ?? Date.now(),
      }))
      updateSessionStatus(historyId, 'done')
      return
    }

    if (msg.direction === 'error') {
      updateTurn(historyId, turnId, (turn) => ({
        ...turn,
        answer: turn.answer || msg.content,
        responseSegments: turn.answer
          ? turn.responseSegments
          : appendTextToSegments(turn.responseSegments, msg.content),
        status: 'error',
        completedAt: turn.completedAt ?? Date.now(),
      }))
      updateSessionStatus(historyId, 'error')
    }
  })

  useEffect(() => {
    let isMounted = true
    let unsubBotMsg: (() => void) | undefined

    void subscribeBotMessage((payload) => {
      if (!isMounted) {
        return
      }
      handleBotMessage(payload)
    }).then((unlisten) => {
      unsubBotMsg = unlisten
    })

    return () => {
      isMounted = false
      if (unsubBotMsg) {
        unsubBotMsg()
      }
    }
  }, [])

  const submitPromptInternal = async (
    rawPrompt: string,
    context?: {
      providerConfig?: ProviderRuntimeConfig | null
      agent?: ConversationAgentSnapshot | null
      sessionLlm?: { providerId: ProviderId; model: string } | null
    },
    options?: { forceNewSession?: boolean },
  ) => {
    const trimmedPrompt = rawPrompt.trim()
    if (!trimmedPrompt) {
      setError('请输入内容')
      return false
    }

    const turn = buildNewTurn(trimmedPrompt)
    const nextHistoryId = options?.forceNewSession ? createId() : (activeHistoryId || createId())
    const hasActiveConversation =
      !options?.forceNewSession && Boolean(activeHistoryId && history.some((item) => item.id === activeHistoryId))

    currentTurnIdsRef.current.set(nextHistoryId, turn.id)
    receivedFirstDeltaRef.current.set(nextHistoryId, false)
    markSessionRunning(nextHistoryId)
    setError('')
    setDraft('')
    setActiveHistoryId(nextHistoryId)
    setHistory((previous) => {
      if (!hasActiveConversation) {
        const nextConversation: HistoryItem = {
          id: nextHistoryId,
          title: deriveConversationTitle(trimmedPrompt),
          status: 'running',
          createdAt: turn.createdAt,
          updatedAt: turn.createdAt,
          turns: [turn],
          ...(context?.agent ? { agent: context.agent } : {}),
          ...(context?.sessionLlm
            ? {
                sessionLlmProviderId: context.sessionLlm.providerId,
                sessionLlmModel: context.sessionLlm.model,
              }
            : context?.providerConfig
            ? {
                sessionLlmProviderId: context.providerConfig.providerId,
                sessionLlmModel: context.providerConfig.model,
              }
            : {}),
        }

        return [nextConversation, ...previous].slice(0, MAX_HISTORY_ITEMS)
      }

      const updated = previous.map((item): HistoryItem =>
        item.id === nextHistoryId
          ? {
              ...item,
              status: 'running',
              updatedAt: turn.createdAt,
              turns: [...item.turns, turn],
            }
          : item,
      )
      const current = updated.find((item) => item.id === nextHistoryId)
      const others = updated.filter((item) => item.id !== nextHistoryId)
      return current ? [current, ...others].slice(0, MAX_HISTORY_ITEMS) : updated.slice(0, MAX_HISTORY_ITEMS)
    })

    try {
      await streamPiPrompt(trimmedPrompt, {
        sessionId: nextHistoryId,
        providerConfig: context?.providerConfig,
        agentConfig: context?.agent,
      })

      if (currentTurnIdsRef.current.get(nextHistoryId) === turn.id) {
        setLatestActivityState(nextHistoryId, turn.id, '连接 pi 主脑', 'done')
        setLatestActivityState(nextHistoryId, turn.id, '流式输出中', 'done')
        setLatestActivityState(nextHistoryId, turn.id, '深度思考中', 'done')
        appendActivity(nextHistoryId, turn.id, '回复完成', 'pi 已返回完整结果，本轮对话结束。', 'done')
        updateTurn(nextHistoryId, turn.id, (current) => ({
          ...current,
          status: current.status === 'running' ? 'done' : current.status,
          completedAt: current.completedAt ?? Date.now(),
        }))
        updateSessionStatus(nextHistoryId, 'done')
        markSessionSettled(nextHistoryId)
      }

      return true
    } catch (invokeError) {
      const message = invokeError instanceof Error ? invokeError.message : String(invokeError)
      setError(message)
      appendActivity(nextHistoryId, turn.id, '启动失败', message, 'error')
      updateTurn(nextHistoryId, turn.id, (current) => ({
        ...current,
        status: 'error',
        answer: current.answer || message,
        completedAt: current.completedAt ?? Date.now(),
      }))
      updateSessionStatus(nextHistoryId, 'error')
      markSessionSettled(nextHistoryId)
      return false
    }
  }

  const submitPrompt = async (
    rawPrompt: string,
    context?: {
      providerConfig?: ProviderRuntimeConfig | null
      agent?: ConversationAgentSnapshot | null
      sessionLlm?: { providerId: ProviderId; model: string } | null
    },
  ) => submitPromptInternal(rawPrompt, context)

  const submitPromptInNewSession = async (
    rawPrompt: string,
    context?: {
      providerConfig?: ProviderRuntimeConfig | null
      agent?: ConversationAgentSnapshot | null
      sessionLlm?: { providerId: ProviderId; model: string } | null
    },
  ) => submitPromptInternal(rawPrompt, context, { forceNewSession: true })

  const abortPrompt = async () => {
    const targetHistoryId = activeHistoryId.trim()
    if (!targetHistoryId) {
      return
    }
    try {
      await abortPiStream(targetHistoryId)
    } catch (invokeError) {
      const message = invokeError instanceof Error ? invokeError.message : String(invokeError)
      setError(message)
    }
  }

  const resetSessionDraft = () => {
    setDraft('')
    setError('')
    setActiveHistoryId('')
  }

  const updateSessionLlm = (historyId: string, providerId: ProviderId, model: string) => {
    updateHistoryItem(historyId, (item) => ({
      ...item,
      sessionLlmProviderId: providerId,
      sessionLlmModel: model.trim(),
      updatedAt: Date.now(),
    }))
  }

  const selectHistoryItem = (id: string) => {
    setActiveHistoryId(id)
    setError('')
  }

  const clearHistory = () => {
    setHistory([])
    setActiveHistoryId('')
    localStorage.removeItem(HISTORY_STORAGE_KEY)
    clearLegacyHistoryStorage()
    void clearHistoryState()
    void clearPiSession()
  }

  const deleteHistoryItem = async (id: string) => {
    const trimmedId = id.trim()
    if (!trimmedId) {
      return
    }

    const targetHistoryItem = history.find((item) => item.id === trimmedId)
    if (!targetHistoryItem) {
      return
    }

    if (runningHistoryIds.includes(trimmedId)) {
      setError('当前会话仍在生成，暂时不能删除。')
      return
    }

    const nextHistory = history.filter((item) => item.id !== trimmedId)
    setHistory(nextHistory)
    setActiveHistoryId((current) => (current === trimmedId ? (nextHistory[0]?.id ?? '') : current))
    setError('')

    try {
      await clearPiSessionForId(trimmedId)
    } catch (invokeError) {
      const message = invokeError instanceof Error ? invokeError.message : String(invokeError)
      setError(`会话已删除，但清理运行时 session 失败：${message}`)
    }
  }

  const clearSession = async () => {
    try {
      await clearPiSession()
      setError('')
    } catch (invokeError) {
      const message = invokeError instanceof Error ? invokeError.message : String(invokeError)
      setError(message)
    }
  }

  const activeHistoryItem = history.find((item) => item.id === activeHistoryId) ?? null
  const loading = runningHistoryIds.length > 0

  return {
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
    clearSession,
    updateSessionLlm,
  }
}
