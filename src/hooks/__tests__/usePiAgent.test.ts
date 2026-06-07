import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const piStreamHarness = vi.hoisted(() => ({
  listeners: [] as Array<(payload: Record<string, unknown>) => void>,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

vi.mock('../../lib/piClient', () => ({
  abortPiStream: vi.fn(),
  chatAppendTurn: vi.fn(),
  chatClearAllSessions: vi.fn().mockResolvedValue(undefined),
  chatCreateSession: vi.fn(),
  chatDeleteSession: vi.fn().mockResolvedValue(undefined),
  chatGetSessionDetail: vi.fn().mockResolvedValue(null),
  chatListSessions: vi.fn().mockResolvedValue([]),
  chatUpdateSessionTitle: vi.fn().mockResolvedValue({}),
  clearHistoryState: vi.fn(),
  clearPiSession: vi.fn(),
  clearPiSessionForId: vi.fn(),
  ensureRuntimeDependencies: vi.fn().mockResolvedValue({
    piAvailable: true,
    messages: [],
  }),
  listAgentTaskDeliveries: vi.fn().mockResolvedValue([]),
  loadHistoryState: vi.fn().mockResolvedValue('[]'),
  persistChatAttachments: vi.fn().mockResolvedValue([]),
  saveHistoryState: vi.fn().mockResolvedValue(undefined),
  streamPiPrompt: vi.fn().mockResolvedValue(undefined),
  syncHistoryBackupFromStructured: vi.fn().mockResolvedValue(undefined),
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

import {
  chatAppendTurn,
  chatCreateSession,
  chatGetSessionDetail,
  chatListSessions,
  chatUpdateSessionTitle,
  loadHistoryState,
  persistChatAttachments,
  saveHistoryState,
  streamPiPrompt,
} from '../../lib/piClient'
import { generateSessionConversationTitle } from '../../lib/sessionTitleClient'
import { usePiAgent } from '../usePiAgent'

const mockChatGetSessionDetail = vi.mocked(chatGetSessionDetail)
const mockChatListSessions = vi.mocked(chatListSessions)
const mockLoadHistoryState = vi.mocked(loadHistoryState)
const mockPersistChatAttachments = vi.mocked(persistChatAttachments)
const mockStreamPiPrompt = vi.mocked(streamPiPrompt)
const mockSaveHistoryState = vi.mocked(saveHistoryState)
const mockGenerateSessionConversationTitle = vi.mocked(generateSessionConversationTitle)
const mockChatCreateSession = vi.mocked(chatCreateSession)
const mockChatAppendTurn = vi.mocked(chatAppendTurn)
const mockChatUpdateSessionTitle = vi.mocked(chatUpdateSessionTitle)

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
    mockLoadHistoryState.mockResolvedValue('[]')
    mockChatListSessions.mockResolvedValue([])
    mockChatGetSessionDetail.mockResolvedValue(null)
    mockPersistChatAttachments.mockImplementation(async (payload) =>
      payload.attachments.map((attachment, index) => ({
        id: `session-attachment-${index}`,
        fileName: attachment.fileName,
        filePath: `/session-workspace/${payload.sessionId}/${attachment.fileName}`,
        mimeType: attachment.mimeType ?? '',
        size: 12,
        kind: 'file' as const,
      })),
    )
    mockStreamPiPrompt.mockResolvedValue(undefined)
    mockSaveHistoryState.mockResolvedValue(undefined)
    mockGenerateSessionConversationTitle.mockResolvedValue('咖啡店开业海报')
    mockChatUpdateSessionTitle.mockResolvedValue({} as Awaited<ReturnType<typeof chatUpdateSessionTitle>>)
    vi.stubGlobal('localStorage', {
      getItem: vi.fn().mockReturnValue(null),
      setItem: vi.fn(),
      removeItem: vi.fn(),
    })
  })

  afterEach(() => {
    vi.useRealTimers()
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

  it('renames a history item and persists the structured session title', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))

    act(() => {
      result.current.createEmptySession({
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    const sessionId = result.current.history[0]?.id ?? ''
    act(() => {
      result.current.renameHistoryItem(sessionId, '  新会话标题  ')
    })

    expect(result.current.history[0]?.title).toBe('新会话标题')
    expect(mockChatUpdateSessionTitle).toHaveBeenCalledWith({
      sessionId,
      title: '新会话标题',
    })
  })

  it('rehydrates history from structured sqlite sessions when snapshot payload is empty', async () => {
    mockChatListSessions.mockResolvedValue([
      {
        id: 'recovered-1',
        title: 'Recovered Session',
        status: 'done',
        created_at: 1700000000000,
        updated_at: 1700000001000,
        agent_id: null,
        agent_snapshot_json: null,
        bot_target_json: null,
        session_llm_provider_id: null,
        session_llm_model: null,
        workspace_id: null,
        turn_count: 1,
      },
    ])
    mockChatGetSessionDetail.mockResolvedValue({
      id: 'recovered-1',
      title: 'Recovered Session',
      status: 'done',
      created_at: 1700000000000,
      updated_at: 1700000001000,
      agent_id: null,
      agent_snapshot_json: null,
      bot_target_json: null,
      session_llm_provider_id: null,
      session_llm_model: null,
      workspace_id: null,
      turns: [
        {
          id: 'turn-1',
          session_id: 'recovered-1',
          turn_index: 0,
          prompt: 'hello',
          answer: 'world',
          thinking: '',
          status: 'done',
          created_at: 1700000000001,
          completed_at: 1700000000002,
          usage_json: null,
          response_segments_json: null,
          tool_calls_json: null,
          activity_json: null,
          speaker_agent_id: null,
        },
      ],
    })

    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    expect(result.current.history).toHaveLength(1)
    expect(result.current.history[0]).toMatchObject({
      id: 'recovered-1',
      title: 'Recovered Session',
    })
    expect(result.current.history[0]?.turns[0]?.prompt).toBe('hello')
    expect(mockSaveHistoryState).toHaveBeenCalledWith(
      expect.stringContaining('"recovered-1"'),
    )
  })

  it('prefers structured sqlite history over a stale snapshot payload on startup', async () => {
    mockLoadHistoryState.mockResolvedValueOnce(
      JSON.stringify([
        {
          id: 'recovered-1',
          title: '旧标题',
          status: 'done',
          createdAt: 1700000000000,
          updatedAt: 1700000000500,
          turns: [
            {
              id: 'turn-old',
              prompt: 'old prompt',
              answer: 'old answer',
              thinking: '',
              status: 'done',
              createdAt: 1700000000001,
              completedAt: 1700000000002,
              activity: [],
              toolCalls: [],
            },
          ],
        },
      ]),
    )
    mockChatListSessions.mockResolvedValueOnce([
      {
        id: 'recovered-1',
        title: 'Recovered Session',
        status: 'done',
        created_at: 1700000000000,
        updated_at: 1700000001000,
        agent_id: null,
        agent_snapshot_json: null,
        bot_target_json: null,
        session_llm_provider_id: null,
        session_llm_model: null,
        workspace_id: null,
        turn_count: 1,
      },
    ])
    mockChatGetSessionDetail.mockResolvedValueOnce({
      id: 'recovered-1',
      title: 'Recovered Session',
      status: 'done',
      created_at: 1700000000000,
      updated_at: 1700000001000,
      agent_id: null,
      agent_snapshot_json: null,
      bot_target_json: null,
      session_llm_provider_id: null,
      session_llm_model: null,
      workspace_id: null,
      turns: [
        {
          id: 'turn-1',
          session_id: 'recovered-1',
          turn_index: 0,
          prompt: 'hello',
          answer: 'world',
          thinking: '',
          status: 'done',
          created_at: 1700000000001,
          completed_at: 1700000000002,
          usage_json: null,
          response_segments_json: null,
          tool_calls_json: null,
          activity_json: null,
          speaker_agent_id: null,
        },
      ],
    })

    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    expect(result.current.history).toHaveLength(1)
    expect(result.current.history[0]).toMatchObject({
      id: 'recovered-1',
      title: 'Recovered Session',
    })
    expect(result.current.history[0]?.turns[0]?.prompt).toBe('hello')
    await waitFor(() =>
      expect(mockSaveHistoryState).toHaveBeenCalledWith(
        expect.stringContaining('"Recovered Session"'),
      ),
    )
  })

  it('keeps history visible when snapshot hydration fails but structured sqlite succeeds', async () => {
    mockLoadHistoryState.mockRejectedValueOnce(new Error('snapshot blew up'))
    mockChatListSessions.mockResolvedValueOnce([
      {
        id: 'recovered-1',
        title: 'Recovered Session',
        status: 'done',
        created_at: 1700000000000,
        updated_at: 1700000001000,
        agent_id: null,
        agent_snapshot_json: null,
        bot_target_json: null,
        session_llm_provider_id: null,
        session_llm_model: null,
        workspace_id: null,
        turn_count: 1,
      },
    ])
    mockChatGetSessionDetail.mockResolvedValueOnce({
      id: 'recovered-1',
      title: 'Recovered Session',
      status: 'done',
      created_at: 1700000000000,
      updated_at: 1700000001000,
      agent_id: null,
      agent_snapshot_json: null,
      bot_target_json: null,
      session_llm_provider_id: null,
      session_llm_model: null,
      workspace_id: null,
      turns: [
        {
          id: 'turn-1',
          session_id: 'recovered-1',
          turn_index: 0,
          prompt: 'hello',
          answer: 'world',
          thinking: '',
          status: 'done',
          created_at: 1700000000001,
          completed_at: 1700000000002,
          usage_json: null,
          response_segments_json: null,
          tool_calls_json: null,
          activity_json: null,
          speaker_agent_id: null,
        },
      ],
    })

    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    expect(result.current.error).toBe('')
    expect(result.current.history).toHaveLength(1)
    expect(result.current.history[0]?.id).toBe('recovered-1')
    expect(mockSaveHistoryState).toHaveBeenCalledWith(
      expect.stringContaining('"Recovered Session"'),
    )
  })

  it('keeps history visible when structured sqlite hydration fails but snapshot succeeds', async () => {
    mockLoadHistoryState.mockResolvedValueOnce(
      JSON.stringify([
        {
          id: 'snapshot-1',
          title: 'Snapshot Session',
          status: 'done',
          createdAt: 1700000000000,
          updatedAt: 1700000001000,
          turns: [
            {
              id: 'turn-1',
              prompt: 'hello',
              answer: 'world',
              thinking: '',
              status: 'done',
              createdAt: 1700000000001,
              completedAt: 1700000000002,
              activity: [],
              toolCalls: [],
            },
          ],
        },
      ]),
    )
    mockChatListSessions.mockRejectedValueOnce(new Error('sqlite list failed'))

    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    expect(result.current.error).toBe('')
    expect(result.current.history).toHaveLength(1)
    expect(result.current.history[0]?.id).toBe('snapshot-1')
    expect(result.current.history[0]?.turns[0]?.prompt).toBe('hello')
    expect(mockSaveHistoryState).toHaveBeenCalledWith(
      expect.stringContaining('"Snapshot Session"'),
    )
  })

  it('does not truncate hydrated structured history when more than 30 sessions exist', async () => {
    mockLoadHistoryState.mockResolvedValueOnce('[]')
    mockChatListSessions.mockResolvedValueOnce(
      Array.from({ length: 33 }, (_, index) => ({
        id: `session-${index + 1}`,
        title: `Session ${index + 1}`,
        status: 'done',
        created_at: 1700000000000 + index,
        updated_at: 1700000001000 + index,
        agent_id: null,
        agent_snapshot_json: null,
        bot_target_json: null,
        session_llm_provider_id: null,
        session_llm_model: null,
        workspace_id: null,
        turn_count: 0,
      })),
    )
    mockChatGetSessionDetail.mockImplementation(async (sessionId) => ({
      id: sessionId,
      title: `Detail ${sessionId}`,
      status: 'done',
      created_at: 1700000000000,
      updated_at: 1700000001000,
      agent_id: null,
      agent_snapshot_json: null,
      bot_target_json: null,
      session_llm_provider_id: null,
      session_llm_model: null,
      workspace_id: null,
      turns: [],
    }))

    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    expect(result.current.history).toHaveLength(33)
    expect(result.current.history.map((item) => item.id)).toContain('session-1')
    expect(result.current.history.map((item) => item.id)).toContain('session-33')
  })

  it('persists an empty session quickly after it is created', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    act(() => {
      result.current.createEmptySession({
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await waitFor(() => expect(mockSaveHistoryState).toHaveBeenCalled())
  })

  it('does not postpone snapshot persistence while history keeps changing', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    let sessionId = ''
    act(() => {
      sessionId = result.current.createEmptySession({
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await waitFor(() => expect(mockSaveHistoryState).toHaveBeenCalled())
    mockSaveHistoryState.mockClear()

    vi.useFakeTimers()

    act(() => {
      result.current.updateSessionLlm(sessionId, 'openai', 'model-1')
    })

    await act(async () => {
      vi.advanceTimersByTime(30)
    })

    act(() => {
      result.current.updateSessionLlm(sessionId, 'openai', 'model-2')
    })

    await act(async () => {
      vi.advanceTimersByTime(30)
    })

    act(() => {
      result.current.updateSessionLlm(sessionId, 'openai', 'model-3')
    })

    expect(mockSaveHistoryState).not.toHaveBeenCalled()

    await act(async () => {
      vi.advanceTimersByTime(40)
    })

    expect(mockSaveHistoryState).toHaveBeenCalledTimes(1)
  })

  it('throttles full snapshot persistence while a stream is running', async () => {
    let resolveStream: (() => void) | undefined
    mockStreamPiPrompt.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveStream = resolve
        }),
    )
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))
    await waitFor(() => expect(piStreamHarness.listeners.length).toBeGreaterThan(0))

    act(() => {
      void result.current.submitPrompt('运行中不要高频保存完整快照', {
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await waitFor(() => expect(mockStreamPiPrompt).toHaveBeenCalledTimes(1))
    const sessionId = mockStreamPiPrompt.mock.calls[0]?.[1]?.sessionId
    mockSaveHistoryState.mockClear()
    vi.useFakeTimers()

    act(() => {
      for (const listener of piStreamHarness.listeners) {
        listener({ event: 'delta', sessionId, text: 'a' })
      }
    })

    await act(async () => {
      vi.advanceTimersByTime(16)
    })

    await act(async () => {
      vi.advanceTimersByTime(1000)
    })

    expect(mockSaveHistoryState).not.toHaveBeenCalled()

    await act(async () => {
      vi.advanceTimersByTime(1500)
    })

    expect(mockSaveHistoryState).toHaveBeenCalledTimes(1)

    await act(async () => {
      resolveStream?.()
    })
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

  it('re-persists first-turn attachments into the generated session workspace before streaming', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))
    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    await act(async () => {
      await result.current.submitPrompt('请读取附件内容', {
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
        workspaceId: 'workspace-1',
        attachments: [
          {
            id: 'pre-uploaded-attachment',
            fileName: 'source.txt',
            filePath: '/tmp/agent-inbox/source.txt',
            mimeType: 'text/plain',
            size: 12,
            kind: 'file',
          },
        ],
      })
    })

    await waitFor(() => expect(mockStreamPiPrompt).toHaveBeenCalledTimes(1))

    const streamOptions = mockStreamPiPrompt.mock.calls[0]?.[1]
    const generatedSessionId = streamOptions?.sessionId
    expect(generatedSessionId).toEqual(expect.any(String))
    expect(mockPersistChatAttachments).toHaveBeenCalledWith({
      agentId: 'agent-1',
      sessionId: generatedSessionId,
      workspaceId: 'workspace-1',
      attachments: [
        {
          fileName: 'source.txt',
          mimeType: 'text/plain',
          sourcePath: '/tmp/agent-inbox/source.txt',
        },
      ],
    })
    expect(streamOptions?.attachments).toEqual([
      expect.objectContaining({
        fileName: 'source.txt',
        filePath: `/session-workspace/${generatedSessionId}/source.txt`,
      }),
    ])
    expect(mockChatAppendTurn).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: generatedSessionId,
        prompt: expect.stringContaining(`/session-workspace/${generatedSessionId}/source.txt`),
      }),
    )
  })

  it('flushes history while a stream is still in progress', async () => {
    let resolveStream: (() => void) | null = null
    mockStreamPiPrompt.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveStream = resolve
        }),
    )

    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    act(() => {
      void result.current.submitPrompt('流式过程中也要快落盘', {
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await waitFor(() => expect(mockSaveHistoryState).toHaveBeenCalled())

    await act(async () => {
      resolveStream?.()
    })
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

  it('persists only one session shell and one first turn when reusing an empty session', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))
    await waitFor(() => expect(result.current.historyHydrated).toBe(true))

    let createdSessionId = ''
    act(() => {
      createdSessionId = result.current.createEmptySession({
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await act(async () => {
      await result.current.submitPrompt('空会话首条：只存一次', {
        agent: sampleAgent,
        sessionLlm: { providerId: 'openai', model: 'gpt-4o-mini' },
      })
    })

    await waitFor(() => expect(mockChatAppendTurn).toHaveBeenCalledTimes(1))
    expect(mockChatCreateSession).toHaveBeenCalledTimes(1)
    expect(mockChatCreateSession).toHaveBeenCalledWith(
      expect.objectContaining({
        id: createdSessionId,
      }),
    )
    expect(mockChatAppendTurn).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: createdSessionId,
        turnIndex: 0,
      }),
    )
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
