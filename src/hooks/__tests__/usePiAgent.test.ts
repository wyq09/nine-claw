import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const piStreamHarness = vi.hoisted(() => ({
  listeners: [] as Array<(payload: Record<string, unknown>) => void>,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

vi.mock('../../lib/piClient', () => ({
  abortPiStream: vi.fn(),
  clearHistoryState: vi.fn(),
  clearPiSession: vi.fn(),
  clearPiSessionForId: vi.fn(),
  ensureRuntimeDependencies: vi.fn().mockResolvedValue({
    piAvailable: true,
    messages: [],
  }),
  listAgentTaskDeliveries: vi.fn().mockResolvedValue([]),
  loadHistoryState: vi.fn().mockResolvedValue('[]'),
  saveHistoryState: vi.fn().mockResolvedValue(undefined),
  streamPiPrompt: vi.fn().mockResolvedValue(undefined),
  subscribeAgentLoopAborted: vi.fn().mockResolvedValue(() => {}),
  subscribeAgentLoopCompleted: vi.fn().mockResolvedValue(() => {}),
  subscribeAgentLoopStarted: vi.fn().mockResolvedValue(() => {}),
  subscribeBotMessage: vi.fn().mockResolvedValue(() => {}),
  subscribePiStream: vi.fn((listener: (payload: Record<string, unknown>) => void) => {
    piStreamHarness.listeners.push(listener)
    return Promise.resolve(() => {
      const index = piStreamHarness.listeners.indexOf(listener)
      if (index >= 0) {
        piStreamHarness.listeners.splice(index, 1)
      }
    })
  }),
  widgetCancelResponse: vi.fn().mockResolvedValue(undefined),
  widgetSubmitResponse: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('../../lib/sessionTitleClient', () => ({
  generateSessionConversationTitle: vi.fn().mockResolvedValue('咖啡店开业海报'),
}))

vi.mock('../useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
  }),
}))

vi.mock('../../lib/taskDeliveryNotification', () => ({
  listenTaskDeliveryNotificationActions: vi.fn().mockResolvedValue(() => {}),
  showTaskDeliveryDesktopNotification: vi.fn(),
}))

import { streamPiPrompt } from '../../lib/piClient'
import { generateSessionConversationTitle } from '../../lib/sessionTitleClient'
import { usePiAgent } from '../usePiAgent'

const mockStreamPiPrompt = vi.mocked(streamPiPrompt)
const mockGenerateSessionConversationTitle = vi.mocked(generateSessionConversationTitle)

const sampleAgent = {
  id: 'agent-1',
  name: '九节虾',
  summary: '总结与执行',
  description: '负责总结与执行',
  systemPrompt: '',
  capabilityPolicy: {
    strategy: 'static' as const,
    requiredSkillIds: [],
    forbiddenSkillIds: [],
    maxDynamicSkills: 0,
  },
  skillIds: [],
  allowedToolIds: [],
  defaultProviderId: 'openai' as const,
  defaultModel: 'gpt-4o-mini',
  executionMode: 'single' as const,
}

describe('usePiAgent', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    piStreamHarness.listeners.length = 0
    mockStreamPiPrompt.mockResolvedValue(undefined)
    mockGenerateSessionConversationTitle.mockResolvedValue('咖啡店开业海报')
    vi.stubGlobal('localStorage', {
      getItem: vi.fn().mockReturnValue(null),
      setItem: vi.fn(),
      removeItem: vi.fn(),
    })
  })

  it('creates an empty session immediately in history', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))

    act(() => {
      result.current.createEmptySession({
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
        workspaceId: 'workspace-1',
      })
    })

    expect(result.current.history).toHaveLength(1)
    expect(result.current.activeHistoryId).toBe(result.current.history[0]?.id)
    expect(result.current.history[0]).toMatchObject({
      title: '新会话',
      workspaceId: 'workspace-1',
      sessionLlmProviderId: 'openai',
      sessionLlmModel: 'gpt-4o-mini',
    })
    expect(result.current.history[0]?.turns).toEqual([])
  })

  it('reuses the empty session when the first message is sent', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))

    let createdSessionId = ''
    act(() => {
      createdSessionId = result.current.createEmptySession({
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await act(async () => {
      await result.current.submitPrompt('帮我写一版咖啡店开业海报文案', {
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await waitFor(() => {
      expect(result.current.history[0]?.turns).toHaveLength(1)
    })

    expect(result.current.history).toHaveLength(1)
    expect(result.current.activeHistoryId).toBe(createdSessionId)
    expect(result.current.history[0]?.turns[0]?.prompt).toBe('帮我写一版咖啡店开业海报文案')
    expect(mockStreamPiPrompt).toHaveBeenCalledWith(
      '帮我写一版咖啡店开业海报文案',
      expect.objectContaining({
        sessionId: createdSessionId,
      }),
    )
  })

  it('requests an llm title after first-turn completion even without a done stream event', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))

    await act(async () => {
      await result.current.submitPrompt('帮我写一版咖啡店开业海报文案', {
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await waitFor(() => {
      expect(mockGenerateSessionConversationTitle).toHaveBeenCalledWith(
        'agent-1',
        expect.any(String),
        '帮我写一版咖啡店开业海报文案',
      )
    })
  })

  it('requests an llm title after first turn when reusing an empty session', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))
    /** 避免 SQLite 初始加载晚于空会话创建，把列表覆盖成 [] 后 submit 走错分支 */
    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    act(() => {
      result.current.createEmptySession({
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await act(async () => {
      await result.current.submitPrompt('空会话首条：咖啡店海报', {
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await waitFor(() => {
      expect(mockGenerateSessionConversationTitle).toHaveBeenCalledWith(
        'agent-1',
        expect.any(String),
        '空会话首条：咖啡店海报',
      )
    })
  })

  it('allows the next prompt after done while title generation and old invoke are still pending', async () => {
    const streamSettlers: Array<{ resolve: () => void; reject: (error: Error) => void }> = []
    mockStreamPiPrompt.mockImplementation(
      () =>
        new Promise<void>((resolve, reject) => {
          streamSettlers.push({ resolve, reject })
        }),
    )
    mockGenerateSessionConversationTitle.mockImplementation(() => new Promise<string>(() => {}))

    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))
    await waitFor(() => expect(piStreamHarness.listeners.length).toBeGreaterThan(0))

    act(() => {
      void result.current.submitPrompt('第一条：整理用户画像', {
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await waitFor(() => expect(mockStreamPiPrompt).toHaveBeenCalledTimes(1))
    const firstSessionId = mockStreamPiPrompt.mock.calls[0]?.[1]?.sessionId
    expect(firstSessionId).toEqual(expect.any(String))

    act(() => {
      for (const listener of piStreamHarness.listeners) {
        listener({ event: 'done', sessionId: firstSessionId })
      }
    })

    await waitFor(() => expect(result.current.loading).toBe(false))

    act(() => {
      void result.current.submitPrompt('第二条：继续生成标题', {
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await waitFor(() => expect(mockStreamPiPrompt).toHaveBeenCalledTimes(2))
    expect(mockStreamPiPrompt.mock.calls[1]?.[0]).toBe('第二条：继续生成标题')
    expect(streamSettlers).toHaveLength(2)

    act(() => {
      streamSettlers[0]?.reject(new Error('late background cleanup failed'))
      streamSettlers[1]?.resolve()
    })

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.error).toBe('')
  })
})
