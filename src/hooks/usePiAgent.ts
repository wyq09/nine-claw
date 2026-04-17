import type { MutableRefObject } from 'react'
import { startTransition, useCallback, useEffect, useEffectEvent, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import {
  abortPiStream,
  clearHistoryState,
  clearPiSession,
  clearPiSessionForId,
  ensureRuntimeDependencies,
  generateSessionConversationTitle,
  listAgentTaskDeliveries,
  loadHistoryState,
  saveHistoryState,
  streamPiPrompt,
  subscribeBotMessage,
  subscribePiStream,
} from '../lib/piClient'
import type { BotMessageEvent } from '../lib/piClient'
import {
  listenTaskDeliveryNotificationActions,
  showTaskDeliveryDesktopNotification,
} from '../lib/taskDeliveryNotification'
import type {
  ActivityState,
  AgentTaskDeliveryRecord,
  ConversationAgentSnapshot,
  ConversationTurn,
  HistoryItem,
  HistoryStatus,
  PiStreamPayload,
  PersistedChatAttachment,
  ProviderId,
  ProviderRuntimeConfig,
  ToolCallEntry,
} from '../types'
import {
  appendAgentTaskDeliveriesToHistory,
  appendTextToSegments,
  buildBotConversationTitle,
  buildNewTurn,
  buildToolCallEntry,
  cleanLlmSessionTitle,
  clearLegacyHistoryStorage,
  createActivity,
  createId,
  deriveConversationTitle,
  extractTokenUsage,
  HISTORY_STORAGE_KEY,
  loadLegacyHistoryFromStorage,
  MAX_HISTORY_ITEMS,
  parseHistorySnapshot,
  parseUsageFromPayload,
  truncateTitle,
  updateLatestActivityState,
  withBotAgentMetadata,
} from './piAgent/piAgentPure'

export { getHistoryStatusLabel } from './piAgent/piAgentPure'

/** 让出主线程，便于浏览器先完成上一轮 paint，再进入长时间 `invoke` */
function yieldToNextPaint(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve())
    })
  })
}

export function usePiAgent(composerClearRef?: MutableRefObject<(() => void) | null>) {
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
  /** 防止同一会话重复并发「标题 LLM」请求 */
  const sessionTitleLlmInflightRef = useRef<Set<string>>(new Set())
  const latestHistoryRef = useRef<HistoryItem[]>([])
  const latestHistorySerializedRef = useRef<string>('')
  const historyHydratedRef = useRef(false)
  /** 仅在为 true 时允许把内存写回 SQLite：初始加载失败时必须为 false，否则会误用 [] 覆盖库内数据 */
  const historyPersistAllowedRef = useRef(false)

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

  /** 首轮完成后用智能体「标题生成」模型起名；多轮仅按首轮文案做本地摘要；无智能体则始终本地摘要。 */
  const finalizeHistoryTitleAfterTurn = (historyId: string) => {
    setHistory((prev) => {
      const item = prev.find((h) => h.id === historyId)
      if (!item) {
        return prev
      }

      const firstTurn = item.turns[0]
      const applyHeuristic = () =>
        truncateTitle(
          deriveConversationTitle(firstTurn?.prompt ?? item.title, firstTurn?.answer ?? '', item.title),
        )

      if (item.turns.length !== 1) {
        const nextTitle = applyHeuristic()
        if (nextTitle === item.title) {
          return prev
        }
        return prev.map((h) => (h.id === historyId ? { ...h, title: nextTitle } : h))
      }

      const agentId = item.agent?.id?.trim()
      if (!agentId) {
        const nextTitle = applyHeuristic()
        if (nextTitle === item.title) {
          return prev
        }
        return prev.map((h) => (h.id === historyId ? { ...h, title: nextTitle } : h))
      }

      if (sessionTitleLlmInflightRef.current.has(historyId)) {
        return prev
      }
      sessionTitleLlmInflightRef.current.add(historyId)

      const userMsg = firstTurn?.prompt ?? ''
      const assistantMsg = firstTurn?.answer ?? ''

      void (async () => {
        try {
          const raw = await generateSessionConversationTitle(agentId, userMsg, assistantMsg)
          const cleaned = cleanLlmSessionTitle(raw).trim()
          const nextTitle = cleaned.length > 0 ? truncateTitle(cleaned) : applyHeuristic()
          setHistory((p) => p.map((h) => (h.id === historyId ? { ...h, title: nextTitle } : h)))
        } catch {
          const nextTitle = applyHeuristic()
          setHistory((p) => p.map((h) => (h.id === historyId ? { ...h, title: nextTitle } : h)))
        } finally {
          sessionTitleLlmInflightRef.current.delete(historyId)
        }
      })()

      return prev
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
        historyPersistAllowedRef.current = true
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
  }, [])

  useEffect(() => {
    if (!historyHydrated) {
      return
    }

    latestHistoryRef.current = history
    clearLegacyHistoryStorage()
    if (!historyPersistAllowedRef.current && history.length === 0) {
      return
    }
    const delay = runningHistoryIds.length > 0 ? 1200 : 200
    const timer = window.setTimeout(() => {
      const serialized = JSON.stringify(history)
      latestHistorySerializedRef.current = serialized
      void saveHistoryState(serialized)
    }, delay)
    return () => {
      window.clearTimeout(timer)
    }
  }, [history, historyHydrated, runningHistoryIds.length])

  useEffect(() => {
    return () => {
      if (!historyHydratedRef.current) {
        return
      }
      if (!historyPersistAllowedRef.current && latestHistoryRef.current.length === 0) {
        return
      }
      const payload =
        latestHistorySerializedRef.current || JSON.stringify(latestHistoryRef.current)
      if (payload) {
        void saveHistoryState(payload)
      }
    }
  }, [])

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
      finalizeHistoryTitleAfterTurn(currentHistoryId)
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
          return current ? [current, ...others].slice(0, MAX_HISTORY_ITEMS) : updated.slice(0, MAX_HISTORY_ITEMS)
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
      return
    }

    if (msg.direction === 'error') {
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
    },
    options?: { forceNewSession?: boolean },
  ) => {
    const trimmedPrompt = rawPrompt.trim()
    if (!trimmedPrompt) {
      setError('请输入内容')
      return false
    }

    const speakerAgentIdForTurn =
      context?.overrideAgentId?.trim() || context?.agent?.id?.trim() || null
    const turn = buildNewTurn(trimmedPrompt, speakerAgentIdForTurn)
    const nextHistoryId = options?.forceNewSession ? createId() : (activeHistoryId || createId())

    const prevFly = desktopStreamFlyRef.current.get(nextHistoryId)
    const supersededTurnId = prevFly ? currentTurnIdsRef.current.get(nextHistoryId) : undefined

    const hasActiveConversation =
      !options?.forceNewSession && Boolean(activeHistoryId && history.some((item) => item.id === activeHistoryId))

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
            title: deriveConversationTitle(trimmedPrompt),
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
    })
    await yieldToNextPaint()

    desktopStreamHoldRef.current.add(nextHistoryId)
    let streamFly: Promise<void> | undefined
    try {
      if (prevFly) {
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
      }

      currentTurnIdsRef.current.set(nextHistoryId, turn.id)
      receivedFirstDeltaRef.current.set(nextHistoryId, false)

      try {
        await yieldToNextPaint()
        markSessionStreaming(nextHistoryId)
        streamFly = streamPiPrompt(trimmedPrompt, {
          sessionId: nextHistoryId,
          providerConfig: context?.providerConfig,
          agentConfig: context?.agent,
          attachments: context?.attachments ?? [],
          workspaceId: context?.workspaceId ?? null,
          overrideAgentId: context?.overrideAgentId ?? null,
        })
        desktopStreamFlyRef.current.set(nextHistoryId, streamFly)
        await streamFly

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
    composerClearRef?.current?.()
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
    error,
    loading,
    runtimeReady,
    runtimeBlockingReason,
    runningHistoryIds,
    streamingHistoryIds,
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
    sanitizeSessionLlmReferences,
  }
}
