import type { MutableRefObject } from 'react'
import { startTransition, useCallback, useEffect, useEffectEvent, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import {
  abortPiStream,
  chatAppendTurn,
  chatClearAllSessions,
  chatCreateSession,
  chatDeleteSession,
  chatGetSessionDetail,
  chatListSessions,
  chatUpdateTurn,
  clearPiSession,
  clearPiSessionForId,
  ensureRuntimeDependencies,
  listAgentTaskDeliveries,
  loadHistoryState,
  saveHistoryState,
  streamPiPrompt,
  subscribeAgentLoopAborted,
  subscribeAgentLoopCompleted,
  subscribeAgentLoopStarted,
  subscribeBotMessage,
  subscribePiStream,
  widgetCancelResponse,
  widgetSubmitResponse,
} from '../lib/piClient'
import type { BotMessageEvent } from '../lib/piClient'
import { sessionDetailToHistoryItem, sessionListItemToHistoryItem } from '../lib/chatAdapter'
import { buildPromptWithAttachments, stripAttachmentDirectivesFromPrompt } from '../lib/composerAttachments'
import { generateSessionConversationTitle } from '../lib/sessionTitleClient'
import { isToolLoopGuardBlockResult } from '../lib/toolLoopGuard'
import {
  listenTaskDeliveryNotificationActions,
  showAgentReplyNotification,
  showTaskDeliveryDesktopNotification,
} from '../lib/taskDeliveryNotification'
import { useToast } from './useToast'
import type {
  ActivityState,
  AgentLoopIteration,
  AgentLoopSegment,
  AgentTaskDeliveryRecord,
  ConversationAgentSnapshot,
  ConversationTurn,
  HistoryItem,
  HistoryStatus,
  PiStreamPayload,
  PersistedChatAttachment,
  ProviderId,
  ProviderRuntimeConfig,
  RuntimeParameters,
  ToolCallEntry,
} from '../types'
import {
  appendOrReplaceWidgetSegment,
} from './piAgent/piAgentWidgets'
import {
  buildPersistedBotConversationForInbound,
  serializeConversationTurnForStructuredUpdate,
} from './piAgent/botHistoryPersistence'
import {
  resolveHydratedHistorySources,
  type StructuredHistoryLoadResult,
} from './piAgent/historyHydration'
import { persistAttachmentsForSessionWorkspace } from './piAgent/sessionWorkspaceAttachments'
import {
  appendAgentTaskDeliveriesToHistory,
  appendTextToSegments,
  buildBotConversationTitle,
  buildNewTurn,
  buildToolCallEntry,
  cleanLlmSessionTitle,
  clearLegacyHistoryStorage,
  createEmptyHistoryItem,
  createActivity,
  createId,
  deriveFirstUserTurnConversationTitle,
  extractTokenUsage,
  HISTORY_STORAGE_KEY,
  loadLegacyHistoryFromStorage,
  parseUsageFromPayload,
  truncateTitle,
  updateLatestActivityState,
  withBotAgentMetadata,
} from './piAgent/piAgentPure'
import { parseWidgetSegment } from '../widgetTypes'

export { getHistoryStatusLabel } from './piAgent/piAgentPure'

const HISTORY_PERSIST_DELAY_IDLE_MS = 10
const HISTORY_PERSIST_DELAY_RUNNING_MS = 20

/** 让出主线程，便于浏览器先完成上一轮 paint，再进入长时间 `invoke` */
function yieldToNextPaint(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve())
    })
  })
}

export function usePiAgent(composerClearRef?: MutableRefObject<(() => void) | null>, options?: { notificationEnabled?: boolean }) {
  const toast = useToast()
  const [error, setError] = useState('')
  const [runningHistoryIds, setRunningHistoryIds] = useState<string[]>([])
  /** 已调用 `streamPiPrompt`（主对话流已挂起） */
  const [streamingHistoryIds, setStreamingHistoryIds] = useState<string[]>([])
  const [history, setHistory] = useState<HistoryItem[]>([])
  const [activeHistoryId, setActiveHistoryId] = useState<string>('')
  const [historyHydrated, setHistoryHydrated] = useState(false)
  const [runtimeReady, setRuntimeReady] = useState(false)
  /** `null`：仍在检测或已就绪；非空：PI 不可用时的说明（避免一直显示「正在初始化」） */
  const [runtimeBlockingReason, setRuntimeBlockingReason] = useState<string | null>(null)
  const currentTurnIdsRef = useRef<Map<string, string>>(new Map())
  const receivedFirstDeltaRef = useRef<Map<string, boolean>>(new Map())
  /** 同步防连点：同 session 在 `streamPiPrompt` resolve 前禁止第二段逻辑抢跑（与 fly 配合）。 */
  const desktopStreamHoldRef = useRef<Set<string>>(new Set())
  /** 当前进行中的 `streamPiPrompt` Promise，用于连续发送时 abort 后 await 释放 Rust 侧会话互斥锁。 */
  const desktopStreamFlyRef = useRef<Map<string, Promise<void>>>(new Map())
  const openTaskSessionRef = useRef<(sessionId: string) => void>(() => {})
  const notifyNewTaskDeliveryRef = useRef<(delivery: AgentTaskDeliveryRecord) => void>(() => {})
  const notifiedTaskDeliveryIdsRef = useRef<Set<string>>(new Set())
  const notifiedToolLoopGuardIdsRef = useRef<Set<string>>(new Set())
  /** 防止同一会话重复并发「标题 LLM」请求 */
  const sessionTitleLlmInflightRef = useRef<Set<string>>(new Set())
  /** 首轮用户文案（submit 时同步写入）；避免 `done` 早于 startTransition 提交时读到 turns 仍为空而跳过 LLM 标题 */
  const sessionFirstUserPromptRef = useRef<Map<string, string>>(new Map())
  const latestHistoryRef = useRef<HistoryItem[]>([])
  const latestHistorySerializedRef = useRef<string>('')
  const historyHydratedRef = useRef(false)
  /** 仅在为 true 时允许把内存写回 SQLite：初始加载失败时必须为 false，否则会误用 [] 覆盖库内数据 */
  const historyPersistAllowedRef = useRef(false)
  const saveHistoryTimerRef = useRef<number | null>(null)
  const saveHistoryDueAtRef = useRef<number | null>(null)
  const saveHistoryInFlightRef = useRef<Promise<void> | null>(null)
  const saveHistoryAfterFlightRef = useRef(false)

  const updateHistoryItem = useCallback((id: string, updater: (item: HistoryItem) => HistoryItem) => {
    setHistory((previous) => previous.map((item) => (item.id === id ? updater(item) : item)))
  }, [])

  const updateTurn = useCallback(
    (
      historyId: string,
      turnId: string,
      updater: (turn: ConversationTurn) => ConversationTurn,
    ) => {
      updateHistoryItem(historyId, (item) => ({
        ...item,
        updatedAt: Date.now(),
        turns: item.turns.map((turn) => (turn.id === turnId ? updater(turn) : turn)),
      }))
    },
    [updateHistoryItem],
  )

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

  /** 连续发新消息时：上一轮若仍在 running，在流已结束后补一条说明（若已被 aborted 事件收尾则跳过）。 */
  const finalizeSupersededTurn = (historyId: string, turnId: string) => {
    updateTurn(historyId, turnId, (turn) => {
      if (turn.status !== 'running') {
        return turn
      }
      return {
        ...turn,
        status: 'aborted_user',
        completedAt: turn.completedAt ?? Date.now(),
        activity: [
          ...turn.activity,
          createActivity(
            '已中断',
            '已停止上一轮回复（连续发送新消息）。',
            'done',
          ),
        ],
      }
    })
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
    setStreamingHistoryIds((previous) => previous.filter((item) => item !== historyId))
    currentTurnIdsRef.current.delete(historyId)
    receivedFirstDeltaRef.current.delete(historyId)
  }

  const markSessionStreaming = (historyId: string) => {
    setStreamingHistoryIds((previous) => (previous.includes(historyId) ? previous : [...previous, historyId]))
  }

  const persistHistoryItemToStructuredStorage = useCallback(
    async (item: HistoryItem, turn?: ConversationTurn, options?: { forceCreateSession?: boolean }) => {
      const shouldCreateSession = options?.forceCreateSession || (!turn && item.turns.length === 0)

      if (shouldCreateSession) {
        try {
          await chatCreateSession({
            id: item.id,
            title: item.title,
            status: item.status,
            agentId: item.agent?.id ?? null,
            agentSnapshotJson: item.agent ? JSON.stringify(item.agent) : null,
            botTargetJson: item.botTarget ? JSON.stringify(item.botTarget) : null,
            sessionLlmProviderId: item.sessionLlmProviderId ?? null,
            sessionLlmModel: item.sessionLlmModel ?? null,
            workspaceId: item.workspaceId ?? null,
          })
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error)
          if (!/UNIQUE|already exists|constraint/i.test(message)) {
            throw error
          }
        }
      }

      if (turn) {
        await chatAppendTurn({
          id: turn.id,
          sessionId: item.id,
          turnIndex: item.turns.length - 1,
          prompt: turn.prompt,
          answer: turn.answer,
          thinking: turn.thinking,
          status: turn.status,
          usageJson: turn.usage ? JSON.stringify(turn.usage) : null,
          responseSegmentsJson: turn.responseSegments ? JSON.stringify(turn.responseSegments) : null,
          toolCallsJson: turn.toolCalls.length > 0 ? JSON.stringify(turn.toolCalls) : null,
          activityJson: turn.activity.length > 0 ? JSON.stringify(turn.activity) : null,
          speakerAgentId: turn.speakerAgentId ?? null,
        })
      }
    },
    [],
  )

  const flushHistoryToStorage = useCallback(async () => {
    if (!historyHydratedRef.current) {
      return
    }
    if (!historyPersistAllowedRef.current && latestHistoryRef.current.length === 0) {
      return
    }

    const payload = JSON.stringify(latestHistoryRef.current)
    latestHistorySerializedRef.current = payload

    if (saveHistoryInFlightRef.current) {
      saveHistoryAfterFlightRef.current = true
      return
    }

    const run = async () => {
      try {
        await saveHistoryState(payload)
      } catch (error) {
        console.error('[NineClaw] async history save failed:', error)
      } finally {
        saveHistoryInFlightRef.current = null
        if (saveHistoryAfterFlightRef.current) {
          saveHistoryAfterFlightRef.current = false
          void flushHistoryToStorage()
        }
      }
    }

    const promise = run()
    saveHistoryInFlightRef.current = promise
    await promise
  }, [])

  const scheduleHistoryPersist = useCallback((delayMs: number) => {
    if (!historyHydratedRef.current) {
      return
    }
    if (!historyPersistAllowedRef.current && latestHistoryRef.current.length === 0) {
      return
    }

    const now = Date.now()
    const requestedDelay = Math.max(0, delayMs)
    const requestedDueAt = now + requestedDelay
    const currentDueAt = saveHistoryDueAtRef.current

    if (saveHistoryTimerRef.current !== null && currentDueAt !== null && currentDueAt <= requestedDueAt) {
      return
    }

    if (saveHistoryTimerRef.current !== null) {
      window.clearTimeout(saveHistoryTimerRef.current)
      saveHistoryTimerRef.current = null
    }

    saveHistoryDueAtRef.current = requestedDueAt
    saveHistoryTimerRef.current = window.setTimeout(() => {
      saveHistoryTimerRef.current = null
      saveHistoryDueAtRef.current = null
      void flushHistoryToStorage()
    }, requestedDelay)
  }, [flushHistoryToStorage])

  const loadHistoryFromStructuredStorage = useCallback(async (): Promise<StructuredHistoryLoadResult> => {
    const sessions = await chatListSessions()
    if (sessions.length === 0) {
      return { history: [], sessionIds: new Set() }
    }

    const details = await Promise.all(
      sessions.map(async (session) => {
        try {
          const detail = await chatGetSessionDetail(session.id)
          return detail ? sessionDetailToHistoryItem(detail) : sessionListItemToHistoryItem(session)
        } catch (error) {
          console.warn('[NineClaw] load structured session detail failed:', session.id, error)
          return sessionListItemToHistoryItem(session)
        }
      }),
    )

    const history = details
      .sort((left, right) => (right.updatedAt || right.createdAt) - (left.updatedAt || left.createdAt))
    return {
      history,
      sessionIds: new Set(history.map((item) => item.id)),
    }
  }, [])

  const requestLlmSessionTitle = (payload: {
    historyId: string
    agentId: string
    userMsg: string
    heuristicTitle: string
  }) => {
    if (sessionTitleLlmInflightRef.current.has(payload.historyId)) {
      return
    }
    sessionTitleLlmInflightRef.current.add(payload.historyId)

    queueMicrotask(() => {
      void (async () => {
        try {
          const raw = await generateSessionConversationTitle(
            payload.agentId,
            payload.historyId,
            payload.userMsg,
          )
          const cleaned = cleanLlmSessionTitle(raw).trim()
          const nextTitle = cleaned.length > 0 ? truncateTitle(cleaned) : payload.heuristicTitle
          setHistory((p) => p.map((h) => (h.id === payload.historyId ? { ...h, title: nextTitle } : h)))
        } catch {
          setHistory((p) =>
            p.map((h) =>
              h.id === payload.historyId ? { ...h, title: payload.heuristicTitle } : h,
            ),
          )
        } finally {
          sessionTitleLlmInflightRef.current.delete(payload.historyId)
        }
      })()
    })
  }

  const persistBotTurnUpdateToStructuredStorage = useCallback(
    async (turn: ConversationTurn) => {
      await chatUpdateTurn(serializeConversationTurnForStructuredUpdate(turn))
    },
    [],
  )

  /** 首轮完成后用智能体「标题生成」模型起名；多轮仅按首轮文案做本地摘要；无智能体则始终本地摘要。 */
  const finalizeHistoryTitleAfterTurn = (historyId: string) => {
    queueMicrotask(() => {
      queueMicrotask(() => {
        let asyncTitleJob:
          | {
              agentId: string
              userMsg: string
              heuristicTitle: string
            }
          | null = null

        setHistory((prev) => {
          const item = prev.find((h) => h.id === historyId)
          if (!item) {
            return prev
          }

          const storedFirst = sessionFirstUserPromptRef.current.get(historyId)?.trim() ?? ''
          const firstTurn = item.turns[0]
          const userMsg = (storedFirst || firstTurn?.prompt || '').trim()

          const applyHeuristic = () =>
            truncateTitle(
              deriveFirstUserTurnConversationTitle(userMsg || item.title, item.title),
            )

          const turnCount = item.turns.length
          if (turnCount >= 2) {
            const nextTitle = applyHeuristic()
            if (nextTitle === item.title) {
              return prev
            }
            return prev.map((h) => (h.id === historyId ? { ...h, title: nextTitle } : h))
          }

          if (turnCount === 0 && !storedFirst) {
            return prev
          }

          const agentId = item.agent?.id?.trim()
          if (!agentId || !userMsg) {
            const nextTitle = applyHeuristic()
            if (nextTitle === item.title) {
              return prev
            }
            return prev.map((h) => (h.id === historyId ? { ...h, title: nextTitle } : h))
          }

          const heuristicTitle = applyHeuristic()
          asyncTitleJob = { agentId, userMsg, heuristicTitle }
          return prev
        })

        if (!asyncTitleJob) {
          return
        }
        const titleJob = asyncTitleJob as {
          agentId: string
          userMsg: string
          heuristicTitle: string
        }
        requestLlmSessionTitle({
          historyId,
          agentId: titleJob.agentId,
          userMsg: titleJob.userMsg,
          heuristicTitle: titleJob.heuristicTitle,
        })
      })
    })
  }

  // 同步 PI 就绪状态：后台线程可能在 WebView 订阅前就 emit，仅靠事件会永远等不到。
  useEffect(() => {
    let cancelled = false

    const applyStatus = (piAvailable: boolean, messages: string[]) => {
      if (cancelled) {
        return
      }
      if (piAvailable) {
        setRuntimeReady(true)
        setRuntimeBlockingReason(null)
      } else {
        setRuntimeReady(false)
        const detail = messages.filter(Boolean).join(' | ').trim()
        setRuntimeBlockingReason(detail || '未检测到可用的 PI 运行时')
      }
    }

    void ensureRuntimeDependencies()
      .then((status) => applyStatus(status.piAvailable, status.messages))
      .catch((invokeError) => {
        if (cancelled) {
          return
        }
        setRuntimeReady(false)
        const message = invokeError instanceof Error ? invokeError.message : String(invokeError)
        setRuntimeBlockingReason(`无法校验 PI 运行时：${message}`)
      })

    const unlisten = listen<boolean>('pi://runtime-ready', (event) => {
      if (cancelled || !event.payload) {
        return
      }
      setRuntimeReady(true)
      setRuntimeBlockingReason(null)
    })

    return () => {
      cancelled = true
      void unlisten.then((fn) => fn())
    }
  }, [])

  useEffect(() => {
    let isMounted = true

    void (async () => {
      const legacyHistory = loadLegacyHistoryFromStorage()
      try {
        const [snapshotPayloadResult, structuredResult] = await Promise.allSettled([
          loadHistoryState(),
          loadHistoryFromStructuredStorage(),
        ])
        const resolution = resolveHydratedHistorySources({
          snapshotPayloadResult,
          structuredResult,
          legacyHistory,
        })

        for (const warning of resolution.warnings) {
          console.warn('[NineClaw] history hydration warning:', warning)
        }

        if (!isMounted) {
          return
        }

        setHistory(resolution.history)
        setActiveHistoryId((current) =>
          current && resolution.history.some((item) => item.id === current)
            ? current
            : (resolution.history[0]?.id ?? ''),
        )
        historyPersistAllowedRef.current = true

        if (resolution.usedLegacy) {
          clearLegacyHistoryStorage()
        }
        if (resolution.shouldPersist) {
          void saveHistoryState(JSON.stringify(resolution.history)).catch((persistError) => {
            console.warn('[NineClaw] history hydration repair save failed:', persistError)
          })
        }
        if (resolution.history.length === 0 && resolution.warnings.length > 0 && !resolution.loaded) {
          setError((current) => current || `读取历史会话失败：${resolution.warnings.join(' | ')}`)
        }
      } catch (loadError) {
        if (!isMounted) {
          return
        }

        const message = loadError instanceof Error ? loadError.message : String(loadError)
        setError((current) => current || `读取历史会话失败：${message}`)
      } finally {
        if (isMounted) {
          historyHydratedRef.current = true
          setHistoryHydrated(true)
        }
      }
    })()

    return () => {
      isMounted = false
    }
  }, [loadHistoryFromStructuredStorage])

  useEffect(() => {
    if (!historyHydrated) {
      return
    }

    latestHistoryRef.current = history
    clearLegacyHistoryStorage()
    if (!historyPersistAllowedRef.current && history.length === 0) {
      return
    }
    const delay = runningHistoryIds.length > 0
      ? HISTORY_PERSIST_DELAY_RUNNING_MS
      : HISTORY_PERSIST_DELAY_IDLE_MS
    scheduleHistoryPersist(delay)
  }, [history, historyHydrated, runningHistoryIds.length, scheduleHistoryPersist])

  useEffect(() => {
    return () => {
      if (saveHistoryTimerRef.current !== null) {
        window.clearTimeout(saveHistoryTimerRef.current)
        saveHistoryTimerRef.current = null
      }
      saveHistoryDueAtRef.current = null
      if (!historyHydratedRef.current) {
        return
      }
      if (!historyPersistAllowedRef.current && latestHistoryRef.current.length === 0) {
        return
      }
      void flushHistoryToStorage()
    }
  }, [flushHistoryToStorage])

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

    if (payload.event === 'skill_selection') {
      const strategy = payload.strategy?.trim() || 'static'
      const mounted = (payload.mountedSkillIds ?? payload.mounted_skill_ids ?? []).filter(Boolean)
      const reasons = (payload.reasons ?? []).filter(Boolean)
      appendActivity(
        currentHistoryId,
        currentTurnId,
        '本轮能力装配',
        `策略：${strategy}；技能：${mounted.length > 0 ? mounted.join('、') : '无'}${reasons.length > 0 ? `；理由：${reasons.join(' | ')}` : ''}`,
        'done',
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

    if (payload.event === 'final_text' && typeof payload.text === 'string') {
      updateTurn(currentHistoryId, currentTurnId, (turn) => ({
        ...turn,
        answer: payload.text ?? turn.answer,
        responseSegments: payload.text
          ? [...(turn.responseSegments ?? []).filter((segment) => segment.type === 'tool'), { type: 'text', text: payload.text }]
          : turn.responseSegments,
      }))
      return
    }

    if (payload.event === 'widget_request' || payload.event === 'widget_resolved') {
      const parsedWidget = parseWidgetSegment({
        type: 'widget',
        widget: payload.widget,
      })
      if (!parsedWidget) {
        return
      }
      updateTurn(currentHistoryId, currentTurnId, (turn) => ({
        ...turn,
        responseSegments: appendOrReplaceWidgetSegment(turn.responseSegments, parsedWidget),
      }))
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
      const rawResultText = payload.resultText ?? payload.result_text
      const resultText = rawResultText ?? ''
      const isError = payload.isError ?? payload.is_error

      if (isError && isToolLoopGuardBlockResult(resultText) && !notifiedToolLoopGuardIdsRef.current.has(toolCallId)) {
        notifiedToolLoopGuardIdsRef.current.add(toolCallId)
        toast.error('检测到连续 3 次相同工具调用，已拦截以防止死循环。')
      }

      updateTurn(currentHistoryId, currentTurnId, (turn) => ({
        ...turn,
        toolCalls: turn.toolCalls.map((toolCall) =>
          toolCall.toolCallId === toolCallId
            ? {
                ...toolCall,
                argsText: payload.argsText ?? payload.args_text ?? toolCall.argsText,
                resultText: rawResultText ?? toolCall.resultText,
                state: isError ? 'error' : 'done',
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
      finalizeHistoryTitleAfterTurn(currentHistoryId)
      markSessionSettled(currentHistoryId)

      // 当用户不在该会话窗口时推送系统通知
      const notificationOn = options?.notificationEnabled !== false
      const notViewingThisSession = document.visibilityState !== 'visible' || activeHistoryId !== currentHistoryId
      if (notificationOn && notViewingThisSession) {
        const item = latestHistoryRef.current.find((h) => h.id === currentHistoryId)
        const title = item?.title ?? ''
        void showAgentReplyNotification(currentHistoryId, title)
      }

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
      finalizeHistoryTitleAfterTurn(currentHistoryId)
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

  // ── Agent Loop event subscriptions ──

  /** Map loopId → { historyId, turnId } for tracking which turn owns each agent loop. */
  const agentLoopTurnMapRef = useRef<Map<string, { historyId: string; turnId: string }>>(new Map())

  useEffect(() => {
    let isMounted = true
    const unsubs: (() => void)[] = []

    async function setup() {
      unsubs.push(
        await subscribeAgentLoopStarted((payload) => {
          if (!isMounted) return

          // Resolve the current active turn — agent loops run within the active stream session
          const historyId = activeHistoryId
          const turnId = currentTurnIdsRef.current.get(historyId) ?? ''
          if (!historyId || !turnId) return

          agentLoopTurnMapRef.current.set(payload.loopId, { historyId, turnId })

          const segment: AgentLoopSegment = {
            type: 'agent_loop',
            loopId: payload.loopId,
            status: 'running',
            totalIterations: 0,
            currentDepth: payload.depth,
            iterations: [] as AgentLoopIteration[],
            startedAt: Date.now(),
          }

          updateTurn(historyId, turnId, (turn) => ({
            ...turn,
            responseSegments: [...(turn.responseSegments ?? []), { type: 'agent_loop' as const, segment }],
          }))
        }),
      )

      unsubs.push(
        await subscribeAgentLoopCompleted((payload) => {
          if (!isMounted) return

          const mapping = agentLoopTurnMapRef.current.get(payload.loopId)
          if (!mapping) return
          const { historyId, turnId } = mapping

          updateTurn(historyId, turnId, (turn) => ({
            ...turn,
            responseSegments: (turn.responseSegments ?? []).map((seg) =>
              seg.type === 'agent_loop' && seg.segment.loopId === payload.loopId
                ? {
                    ...seg,
                    segment: {
                      ...seg.segment,
                      status: 'completed' as const,
                      reason: payload.reason as AgentLoopSegment['reason'],
                      totalIterations: payload.totalIterations,
                      completedAt: Date.now(),
                    },
                  }
                : seg,
            ),
          }))

          agentLoopTurnMapRef.current.delete(payload.loopId)
        }),
      )

      unsubs.push(
        await subscribeAgentLoopAborted((payload) => {
          if (!isMounted) return

          const mapping = agentLoopTurnMapRef.current.get(payload.loopId)
          if (!mapping) return
          const { historyId, turnId } = mapping

          updateTurn(historyId, turnId, (turn) => ({
            ...turn,
            responseSegments: (turn.responseSegments ?? []).map((seg) =>
              seg.type === 'agent_loop' && seg.segment.loopId === payload.loopId
                ? {
                    ...seg,
                    segment: {
                      ...seg.segment,
                      status: 'aborted' as const,
                      totalIterations: payload.iterationsCompleted,
                      completedAt: Date.now(),
                    },
                  }
                : seg,
            ),
          }))

          agentLoopTurnMapRef.current.delete(payload.loopId)
        }),
      )
    }

    void setup()

    return () => {
      isMounted = false
      for (const un of unsubs) un()
    }
  }, [activeHistoryId, updateTurn])

  // ── Bot channel message history integration ──

  /** Map `channel_id:user_id` → { historyId, turnId } for tracking active bot sessions. */
  const botSessionMapRef = useRef<Map<string, { historyId: string; turnId: string }>>(new Map())

  useEffect(() => {
    const nextMap = new Map<string, { historyId: string; turnId: string }>()

    for (const item of history) {
      const channelId = item.botTarget?.channelId?.trim()
      const userId = item.botTarget?.userId?.trim()
      const latestTurnId = item.turns[item.turns.length - 1]?.id
      if (!channelId || !userId || !latestTurnId) {
        continue
      }

      const sessionKey = `${channelId}:${userId}`
      if (nextMap.has(sessionKey)) {
        continue
      }

      nextMap.set(sessionKey, {
        historyId: item.id,
        turnId: latestTurnId,
      })
    }

    botSessionMapRef.current = nextMap
  }, [history])

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
              ? withBotAgentMetadata(
                  {
                    ...item,
                    status: 'running',
                    updatedAt: now,
                    turns: [...item.turns, turn],
                  },
                  msg,
                )
              : item,
          )
          const current = updated.find((item) => item.id === historyId)
          const others = updated.filter((item) => item.id !== historyId)
          return current ? [current, ...others] : updated
        })
        const existingItem = latestHistoryRef.current.find((item) => item.id === historyId)
        const persisted = buildPersistedBotConversationForInbound({
          existingItem,
          historyId,
          message: msg,
          now,
          turn,
        })
        void persistHistoryItemToStructuredStorage(persisted.item, turn, {
          forceCreateSession: persisted.forceCreateSession,
        }).catch((error) => {
          console.error('[NineClaw] async bot inbound save failed:', error)
        })
      } else {
        // First message from this channel:user — create new history item
        const historyId = createId()
        botSessionMapRef.current.set(sessionKey, { historyId, turnId: turn.id })
        const newSession: HistoryItem = {
          id: historyId,
          title: buildBotConversationTitle(msg),
          status: 'running',
          createdAt: now,
          updatedAt: now,
          turns: [turn],
          botTarget: {
            channelId: msg.channel_id,
            userId: msg.user_id,
          },
          ...(msg.agent ? { agent: msg.agent } : {}),
        }
        setHistory((prev) => [newSession, ...prev])
        const persisted = buildPersistedBotConversationForInbound({
          historyId,
          message: msg,
          now,
          turn,
        })
        void persistHistoryItemToStructuredStorage(persisted.item, turn, {
          forceCreateSession: persisted.forceCreateSession,
        }).catch((error) => {
          console.error('[NineClaw] async bot inbound save failed:', error)
        })
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
      updateHistoryItem(historyId, (item) => withBotAgentMetadata(item, msg))
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
      const usage = extractTokenUsage(msg as unknown as Record<string, unknown>)
      const currentTurn = latestHistoryRef.current
        .find((item) => item.id === historyId)
        ?.turns.find((item) => item.id === turnId)
      // Final complete reply → set answer to full text, mark done
      updateHistoryItem(historyId, (item) => withBotAgentMetadata(item, msg))
      updateTurn(historyId, turnId, (turn) => ({
        ...turn,
        answer: msg.content,
        responseSegments: [{ type: 'text', text: msg.content }],
        status: 'done',
        completedAt: turn.completedAt ?? Date.now(),
        usage: usage ?? turn.usage,
      }))
      updateSessionStatus(historyId, 'done')
      finalizeHistoryTitleAfterTurn(historyId)
      if (currentTurn) {
        const updatedTurn: ConversationTurn = {
          ...currentTurn,
          answer: msg.content,
          responseSegments: [{ type: 'text', text: msg.content }],
          status: 'done',
          completedAt: currentTurn.completedAt ?? Date.now(),
          usage: usage ?? currentTurn.usage,
        }
        void persistBotTurnUpdateToStructuredStorage(updatedTurn).catch((error) => {
          console.error('[NineClaw] async bot outbound save failed:', error)
        })
      }
      return
    }

    if (msg.direction === 'error') {
      const currentTurn = latestHistoryRef.current
        .find((item) => item.id === historyId)
        ?.turns.find((item) => item.id === turnId)
      updateHistoryItem(historyId, (item) => withBotAgentMetadata(item, msg))
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
      finalizeHistoryTitleAfterTurn(historyId)
      if (currentTurn) {
        const failedTurn: ConversationTurn = {
          ...currentTurn,
          answer: currentTurn.answer || msg.content,
          responseSegments: currentTurn.answer
            ? currentTurn.responseSegments
            : appendTextToSegments(currentTurn.responseSegments, msg.content),
          status: 'error',
          completedAt: currentTurn.completedAt ?? Date.now(),
        }
        void persistBotTurnUpdateToStructuredStorage(failedTurn).catch((error) => {
          console.error('[NineClaw] async bot outbound save failed:', error)
        })
      }
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

  useEffect(() => {
    openTaskSessionRef.current = (sessionId: string) => {
      const trimmed = sessionId.trim()
      if (!trimmed) {
        return
      }
      setActiveHistoryId(trimmed)
      setError('')
    }
    notifyNewTaskDeliveryRef.current = (delivery: AgentTaskDeliveryRecord) => {
      const seen = notifiedTaskDeliveryIdsRef.current
      if (seen.has(delivery.id)) {
        return
      }
      seen.add(delivery.id)
      if (seen.size > 200) {
        notifiedTaskDeliveryIdsRef.current = new Set([...seen].slice(-100))
      }
      void showTaskDeliveryDesktopNotification(delivery, () => openTaskSessionRef.current(delivery.sessionId))
    }
  }, [])

  useEffect(() => {
    let dispose: (() => void) | undefined
    void listenTaskDeliveryNotificationActions((sessionId) => {
      openTaskSessionRef.current(sessionId)
    }).then((unlisten) => {
      dispose = unlisten
    })
    return () => {
      dispose?.()
    }
  }, [])

  // Web Notification 点击跳转（通过自定义事件桥接）
  useEffect(() => {
    const handler = (e: Event) => {
      const sessionId = (e as CustomEvent<{ sessionId: string }>).detail?.sessionId
      if (sessionId) {
        openTaskSessionRef.current(sessionId)
      }
    }
    window.addEventListener('nineclaw-navigate-session', handler)
    return () => window.removeEventListener('nineclaw-navigate-session', handler)
  }, [])

  useEffect(() => {
    let unlisten: (() => void) | undefined
    void listen<{ kind?: string; message?: string }>('nineclaw-runtime-notification', (event) => {
      const message = typeof event.payload.message === 'string' ? event.payload.message.trim() : ''
      if (!message) {
        return
      }
      toast.success(message, 9000)
    }).then((fn) => {
      unlisten = fn
    })
    return () => unlisten?.()
  }, [toast])

  useEffect(() => {
    let cancelled = false

    const pollTaskDeliveries = async () => {
      const sessionIds = history.map((item) => item.id).filter((item) => item.trim().length > 0)
      if (sessionIds.length === 0) {
        return
      }

      try {
        const deliveries = await listAgentTaskDeliveries(sessionIds)
        if (cancelled || deliveries.length === 0) {
          return
        }

        setHistory((previous) =>
          appendAgentTaskDeliveriesToHistory(previous, deliveries, (d) => notifyNewTaskDeliveryRef.current(d)),
        )
      } catch {
        // ignore polling errors
      }
    }

    void pollTaskDeliveries()
    const timer = window.setInterval(() => {
      void pollTaskDeliveries()
    }, 15000)

    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [history])

  useEffect(() => {
    let active = true
    let unlisten: (() => void) | undefined

    void listen<AgentTaskDeliveryRecord>('agent-task-delivery', (event) => {
      if (!active) {
        return
      }
      const payload = event.payload
      if (
        !payload ||
        typeof payload.id !== 'string' ||
        typeof payload.sessionId !== 'string' ||
        !payload.id.trim()
      ) {
        return
      }
      setHistory((previous) =>
        appendAgentTaskDeliveriesToHistory(previous, [payload], (d) => notifyNewTaskDeliveryRef.current(d)),
      )
    }).then((fn) => {
      if (active) {
        unlisten = fn
      } else {
        fn()
      }
    })

    return () => {
      active = false
      unlisten?.()
    }
  }, [])

  const submitPromptInternal = async (
    rawPrompt: string,
    context?: {
      providerConfig?: ProviderRuntimeConfig | null
      agent?: ConversationAgentSnapshot | null
      sessionLlm?: { providerId: ProviderId; model: string } | null
      attachments?: PersistedChatAttachment[]
      workspaceId?: string | null
      overrideAgentId?: string | null
      runtimeParameters?: RuntimeParameters | null
    },
    options?: { forceNewSession?: boolean },
  ) => {
    const contextAttachments = context?.attachments ?? []
    const userPromptText = stripAttachmentDirectivesFromPrompt(rawPrompt).trim()
    if (!userPromptText && contextAttachments.length === 0) {
      setError('请输入内容')
      return false
    }
    const trimmedPrompt =
      contextAttachments.length > 0
        ? buildPromptWithAttachments(userPromptText, contextAttachments)
        : userPromptText
    const titlePrompt =
      userPromptText ||
      contextAttachments
        .map((attachment) => attachment.fileName.trim())
        .filter(Boolean)
        .join('、') ||
      '附件'

    const speakerAgentIdForTurn =
      context?.overrideAgentId?.trim() || context?.agent?.id?.trim() || null
    const nextHistoryId = options?.forceNewSession ? createId() : (activeHistoryId || createId())

    const prevFly = desktopStreamFlyRef.current.get(nextHistoryId)
    const supersededTurnId = prevFly ? currentTurnIdsRef.current.get(nextHistoryId) : undefined

    const hasActiveConversation =
      !options?.forceNewSession && Boolean(activeHistoryId && history.some((item) => item.id === activeHistoryId))
    const needsSessionWorkspaceAttachmentPersist = contextAttachments.length > 0 && !hasActiveConversation
    const initialTurnPrompt =
      contextAttachments.length > 0 && !needsSessionWorkspaceAttachmentPersist
        ? buildPromptWithAttachments(userPromptText, contextAttachments)
        : userPromptText
    const turn = buildNewTurn(initialTurnPrompt, speakerAgentIdForTurn)

    const prevTurnCountForTitle =
      history.find((item) => item.id === nextHistoryId)?.turns.length ?? 0
    if (!hasActiveConversation || prevTurnCountForTitle === 0) {
      sessionFirstUserPromptRef.current.set(nextHistoryId, titlePrompt)
    }

    /** 先清输入（同步）；仅将 `setHistory` 放入 transition，减轻长会话下列表 diff / 虚拟列表的同步阻塞 */
    composerClearRef?.current?.()
    markSessionRunning(nextHistoryId)
    setError('')
    setActiveHistoryId(nextHistoryId)
    startTransition(() => {
      setHistory((previous) => {
        if (!hasActiveConversation) {
          const nextConversation: HistoryItem = {
            id: nextHistoryId,
            title: '新会话',
            status: 'running',
            createdAt: turn.createdAt,
            updatedAt: turn.createdAt,
            turns: [turn],
            ...(context?.agent ? { agent: context.agent } : {}),
            ...(context?.workspaceId ? { workspaceId: context.workspaceId } : {}),
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

          return [nextConversation, ...previous]
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
        return current ? [current, ...others] : updated
      })
    })

    const persistedConversation: HistoryItem | null = hasActiveConversation
      ? (() => {
          const current = history.find((item) => item.id === nextHistoryId)
          if (!current) {
            return null
          }
          return {
            ...current,
            status: 'running',
            updatedAt: turn.createdAt,
            turns: [...current.turns, turn],
          }
        })()
      : {
          id: nextHistoryId,
          title: '新会话',
          status: 'running',
          createdAt: turn.createdAt,
          updatedAt: turn.createdAt,
          turns: [turn],
          ...(context?.agent ? { agent: context.agent } : {}),
          ...(context?.workspaceId ? { workspaceId: context.workspaceId } : {}),
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

    if (persistedConversation) {
      try {
        if (needsSessionWorkspaceAttachmentPersist) {
          await persistHistoryItemToStructuredStorage(
            { ...persistedConversation, turns: [] },
            undefined,
            { forceCreateSession: true },
          )
        } else {
          await persistHistoryItemToStructuredStorage(persistedConversation, turn, {
            forceCreateSession: !hasActiveConversation,
          })
        }
      } catch (invokeError) {
        const message = invokeError instanceof Error ? invokeError.message : String(invokeError)
        setError(`保存会话失败：${message}`)
        markSessionSettled(nextHistoryId)
        return false
      }
    }

    await yieldToNextPaint()
    scheduleHistoryPersist(0)

    desktopStreamHoldRef.current.add(nextHistoryId)
    let streamFly: Promise<void> | undefined
    try {
      if (prevFly && supersededTurnId) {
        try {
          await abortPiStream(nextHistoryId)
        } catch {
          /* abort 失败不阻塞连续发送 */
        }
        await prevFly.catch(() => {})
        if (desktopStreamFlyRef.current.get(nextHistoryId) === prevFly) {
          desktopStreamFlyRef.current.delete(nextHistoryId)
        }
        if (supersededTurnId) {
          finalizeSupersededTurn(nextHistoryId, supersededTurnId)
        }
        setError('')
      } else if (prevFly) {
        // 旧主回复已通过 done/error/abort 收尾，只剩 Rust 侧后台清理 promise；
        // 继续等待会把标题生成、记忆提取、压缩等后台任务误算成输入框 busy。
        if (desktopStreamFlyRef.current.get(nextHistoryId) === prevFly) {
          desktopStreamFlyRef.current.delete(nextHistoryId)
        }
      }

      currentTurnIdsRef.current.set(nextHistoryId, turn.id)
      receivedFirstDeltaRef.current.set(nextHistoryId, false)

      try {
        const attachmentsForStream = await persistAttachmentsForSessionWorkspace({
          agentId: speakerAgentIdForTurn,
          sessionId: nextHistoryId,
          workspaceId: context?.workspaceId ?? null,
          attachments: contextAttachments,
          enabled: contextAttachments.length > 0 && !hasActiveConversation,
        })
        const promptForStream =
          attachmentsForStream.length > 0
            ? buildPromptWithAttachments(userPromptText, attachmentsForStream)
            : userPromptText
        if (promptForStream && promptForStream !== turn.prompt) {
          updateTurn(nextHistoryId, turn.id, (current) => ({
            ...current,
            prompt: promptForStream,
          }))
        }
        if (needsSessionWorkspaceAttachmentPersist && persistedConversation) {
          await persistHistoryItemToStructuredStorage(
            { ...persistedConversation, turns: [{ ...turn, prompt: promptForStream || turn.prompt }] },
            { ...turn, prompt: promptForStream || turn.prompt },
            { forceCreateSession: false },
          )
        }
        await yieldToNextPaint()
        markSessionStreaming(nextHistoryId)
        streamFly = streamPiPrompt(promptForStream || trimmedPrompt, {
          sessionId: nextHistoryId,
          providerConfig: context?.providerConfig,
          agentConfig: context?.agent,
          attachments: attachmentsForStream,
          workspaceId: context?.workspaceId ?? null,
          overrideAgentId: context?.overrideAgentId ?? null,
          runtimeParameters: context?.runtimeParameters ?? null,
        })
        desktopStreamFlyRef.current.set(nextHistoryId, streamFly)
        await streamFly

        const latestTurnIdAfterFly = currentTurnIdsRef.current.get(nextHistoryId)
        if (latestTurnIdAfterFly === turn.id) {
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
          const agentIdForTitle = context?.agent?.id?.trim()
          if (agentIdForTitle && prevTurnCountForTitle === 0) {
            requestLlmSessionTitle({
              historyId: nextHistoryId,
              agentId: agentIdForTitle,
              userMsg: titlePrompt,
              heuristicTitle: truncateTitle(
                deriveFirstUserTurnConversationTitle(titlePrompt, '新会话'),
              ),
            })
          } else {
            queueMicrotask(() => {
              finalizeHistoryTitleAfterTurn(nextHistoryId)
            })
          }
          markSessionSettled(nextHistoryId)
        } else if (latestTurnIdAfterFly === undefined) {
          /** `done` 已收尾并清空 turn ref；补充首轮标题，避免 fly resolve 略晚时整块跳过 */
          queueMicrotask(() => {
            finalizeHistoryTitleAfterTurn(nextHistoryId)
          })
        }

        return true
      } catch (invokeError) {
        const latestTurnIdAfterError = currentTurnIdsRef.current.get(nextHistoryId)
        if (latestTurnIdAfterError !== turn.id) {
          return true
        }
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
    } finally {
      if (streamFly && desktopStreamFlyRef.current.get(nextHistoryId) === streamFly) {
        desktopStreamFlyRef.current.delete(nextHistoryId)
      }
      desktopStreamHoldRef.current.delete(nextHistoryId)
    }
  }

  const submitPrompt = async (
    rawPrompt: string,
    context?: {
      providerConfig?: ProviderRuntimeConfig | null
      agent?: ConversationAgentSnapshot | null
      sessionLlm?: { providerId: ProviderId; model: string } | null
      attachments?: PersistedChatAttachment[]
      workspaceId?: string | null
      overrideAgentId?: string | null
      runtimeParameters?: RuntimeParameters | null
    },
  ) => submitPromptInternal(rawPrompt, context)

  const submitPromptInNewSession = async (
    rawPrompt: string,
    context?: {
      providerConfig?: ProviderRuntimeConfig | null
      agent?: ConversationAgentSnapshot | null
      sessionLlm?: { providerId: ProviderId; model: string } | null
      attachments?: PersistedChatAttachment[]
      workspaceId?: string | null
      overrideAgentId?: string | null
      runtimeParameters?: RuntimeParameters | null
    },
  ) => submitPromptInternal(rawPrompt, context, { forceNewSession: true })

  const submitWidgetResponse = async (payload: {
    widgetId: string
    kind: 'ask_user'
    answers: Array<{ questionId: string; value: string | string[]; customValue?: string }>
  }) => {
    await widgetSubmitResponse({
      widgetId: payload.widgetId,
      kind: payload.kind,
      answers: payload.answers,
    })
  }

  const cancelWidgetResponse = async (payload: {
    widgetId: string
    kind: 'ask_user'
  }) => {
    await widgetCancelResponse(payload)
  }

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
    composerClearRef?.current?.()
    setError('')
    setActiveHistoryId('')
  }

  const createEmptySession = (context?: {
    agent?: ConversationAgentSnapshot | null
    sessionLlm?: { providerId: ProviderId; model: string } | null
    workspaceId?: string | null
  }) => {
    const nextConversation = createEmptyHistoryItem(context)
    setError('')
    setActiveHistoryId(nextConversation.id)
    setHistory((previous) => [nextConversation, ...previous])
    void persistHistoryItemToStructuredStorage(nextConversation).catch((error) => {
      console.error('[NineClaw] async empty-session save failed:', error)
    })
    queueMicrotask(() => scheduleHistoryPersist(0))
    return nextConversation.id
  }

  const updateSessionLlm = (historyId: string, providerId: ProviderId, model: string) => {
    updateHistoryItem(historyId, (item) => ({
      ...item,
      sessionLlmProviderId: providerId,
      sessionLlmModel: model.trim(),
      updatedAt: Date.now(),
    }))
  }

  const sanitizeSessionLlmReferences = useCallback((isValidRef: (providerId: ProviderId, model: string) => boolean) => {
    setHistory((previous) => {
      let changed = false
      const next = previous.map((item) => {
        const providerId = item.sessionLlmProviderId?.trim()
        const model = item.sessionLlmModel?.trim() ?? ''
        if (!providerId || !model) {
          return item
        }
        if (isValidRef(providerId, model)) {
          return item
        }
        changed = true
        return {
          ...item,
          updatedAt: Date.now(),
          sessionLlmProviderId: undefined,
          sessionLlmModel: undefined,
        }
      })
      return changed ? next : previous
    })
  }, [])

  const selectHistoryItem = (id: string) => {
    startTransition(() => {
      setActiveHistoryId(id)
      setError('')
    })
  }

  const clearHistory = () => {
    setHistory([])
    setActiveHistoryId('')
    localStorage.removeItem(HISTORY_STORAGE_KEY)
    clearLegacyHistoryStorage()
    void chatClearAllSessions()
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

    const previousHistory = history
    const previousActiveHistoryId = activeHistoryId
    const nextHistory = previousHistory.filter((item) => item.id !== trimmedId)
    setHistory(nextHistory)
    setActiveHistoryId((current) => (current === trimmedId ? (nextHistory[0]?.id ?? '') : current))
    setError('')

    try {
      await chatDeleteSession(trimmedId)
    } catch (invokeError) {
      const message = invokeError instanceof Error ? invokeError.message : String(invokeError)
      setError(`删除会话失败：${message}`)
      setHistory(previousHistory)
      setActiveHistoryId(previousActiveHistoryId)
      return
    }

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
    error,
    loading,
    historyHydrated,
    runtimeReady,
    runtimeBlockingReason,
    runningHistoryIds,
    streamingHistoryIds,
    history,
    activeHistoryId,
    activeHistoryItem,
    submitPrompt,
    submitPromptInNewSession,
    submitWidgetResponse,
    cancelWidgetResponse,
    abortPrompt,
    resetSessionDraft,
    createEmptySession,
    selectHistoryItem,
    clearHistory,
    deleteHistoryItem,
    clearSession,
    updateSessionLlm,
    sanitizeSessionLlmReferences,
  }
}
