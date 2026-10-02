import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

vi.mock('../../lib/piClient', () => ({
  abortPiStream: vi.fn(),
  chatAppendTurn: vi.fn(),
  chatClearAllSessions: vi.fn().mockResolvedValue(undefined),
  chatCreateSession: vi.fn().mockResolvedValue({}),
  chatDeleteSession: vi.fn().mockResolvedValue(undefined),
  chatGetSessionDetail: vi.fn().mockResolvedValue(null),
  chatListSessions: vi.fn().mockResolvedValue([]),
  chatUpdateSessionTitle: vi.fn().mockResolvedValue({}),
  chatUpdateTurn: vi.fn().mockResolvedValue({}),
  clearHistoryState: vi.fn(),
  clearPiSession: vi.fn().mockResolvedValue(undefined),
  clearPiSessionForId: vi.fn().mockResolvedValue(undefined),
  ensureRuntimeDependencies: vi.fn().mockResolvedValue({
    piAvailable: true,
    messages: [],
  }),
  listAgentTaskDeliveries: vi.fn().mockResolvedValue([]),
  loadHistoryState: vi.fn().mockResolvedValue('[]'),
  persistChatAttachments: vi.fn().mockResolvedValue([]),
  saveHistoryState: vi.fn().mockResolvedValue(undefined),
  streamPiPrompt: vi.fn().mockResolvedValue(undefined),
  subscribeAgentLoopAborted: vi.fn().mockResolvedValue(() => {}),
  subscribeAgentLoopCompleted: vi.fn().mockResolvedValue(() => {}),
  subscribeAgentLoopStarted: vi.fn().mockResolvedValue(() => {}),
  subscribeBotMessage: vi.fn().mockResolvedValue(() => {}),
  subscribePiStream: vi.fn().mockResolvedValue(() => {}),
  widgetCancelResponse: vi.fn().mockResolvedValue(undefined),
  widgetSubmitResponse: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('../../lib/chatForkClient', () => ({
  chatForkSession: vi.fn(),
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
  showAgentReplyNotification: vi.fn(),
  showTaskDeliveryDesktopNotification: vi.fn(),
}))

import { chatForkSession } from '../../lib/chatForkClient'
import { usePiAgent } from '../usePiAgent'
import type { ChatSessionDetail } from '../../types'

const mockChatForkSession = vi.mocked(chatForkSession)

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

const branchDetail: ChatSessionDetail = {
  id: 'branch-1',
  title: '分支 · 新会话',
  status: 'done',
  created_at: 1700000002000,
  updated_at: 1700000002000,
  agent_id: 'agent-1',
  agent_snapshot_json: null,
  bot_target_json: null,
  session_llm_provider_id: null,
  session_llm_model: null,
  workspace_id: null,
  topic_workspace_dir: '/tmp/branch-1',
  current_workspace_dir: '/tmp/branch-1',
  turns: [
    {
      id: 'turn-a',
      session_id: 'branch-1',
      turn_index: 0,
      prompt: '第一问',
      answer: '第一答',
      thinking: '',
      status: 'done',
      created_at: 1700000001500,
      completed_at: 1700000001600,
      usage_json: null,
      response_segments_json: null,
      tool_calls_json: null,
      activity_json: null,
      speaker_agent_id: null,
    },
  ],
}

describe('usePiAgent forkHistoryItem', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('localStorage', {
      getItem: vi.fn().mockReturnValue(null),
      setItem: vi.fn(),
      removeItem: vi.fn(),
    })
  })

  it('forks a history item from a turn and switches to the new branch', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))

    act(() => {
      result.current.createEmptySession({ agent: sampleAgent })
    })
    const sourceId = result.current.history[0]?.id ?? ''

    mockChatForkSession.mockResolvedValue(branchDetail)

    let forked: unknown = null
    await act(async () => {
      forked = await result.current.forkHistoryItem(sourceId, 'turn-a', 'share')
    })

    expect(mockChatForkSession).toHaveBeenCalledWith({
      sourceSessionId: sourceId,
      forkTurnId: 'turn-a',
      newSessionId: expect.any(String),
      workspaceMode: 'share',
    })
    expect(forked).toMatchObject({ id: 'branch-1' })
    expect(result.current.history[0]?.id).toBe('branch-1')
    expect(result.current.history[0]?.turns[0]).toMatchObject({
      id: 'turn-a',
      prompt: '第一问',
      answer: '第一答',
      completedAt: 1700000001600,
    })
    expect(result.current.activeHistoryId).toBe('branch-1')
    // 源会话仍保留在历史列表中。
    expect(result.current.history.some((item) => item.id === sourceId)).toBe(true)
  })

  it('refuses to fork while the source session is generating', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))

    act(() => {
      result.current.createEmptySession({ agent: sampleAgent })
    })
    const sourceId = result.current.history[0]?.id ?? ''

    // 模拟该会话正在生成（runningHistoryIds 含 sourceId）。
    act(() => {
      void result.current.submitPrompt('第一问', { agent: sampleAgent })
    })
    expect(result.current.runningHistoryIds).toContain(sourceId)

    await act(async () => {
      const forked = await result.current.forkHistoryItem(sourceId, 'turn-x', 'copy')
      expect(forked).toBeNull()
    })
    expect(mockChatForkSession).not.toHaveBeenCalled()
  })

  it('surfaces fork errors without switching sessions', async () => {
    const { result } = renderHook(() => usePiAgent())

    await waitFor(() => expect(result.current.runtimeReady).toBe(true))

    act(() => {
      result.current.createEmptySession({ agent: sampleAgent })
    })
    const sourceId = result.current.history[0]?.id ?? ''
    mockChatForkSession.mockRejectedValue(new Error('该会话正在生成'))

    await act(async () => {
      const forked = await result.current.forkHistoryItem(sourceId, 'turn-a', 'copy')
      expect(forked).toBeNull()
    })

    expect(result.current.error).toContain('创建分支失败')
    expect(result.current.activeHistoryId).toBe(sourceId)
  })
})
