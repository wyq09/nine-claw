import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock @tauri-apps/api/core before importing the module under test.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import {
  chatListSessions,
  chatGetSessionDetail,
  chatCreateSession,
  chatAppendTurn,
  chatUpdateTurn,
  chatDeleteSession,
  chatClearAllSessions,
  chatMigrateHistoryV1,
} from '../../lib/piClient'

const mockedInvoke = vi.mocked(invoke)

beforeEach(() => {
  vi.clearAllMocks()
})

describe('chatListSessions', () => {
  it('calls chat_list_sessions and returns session list', async () => {
    const mockSessions = [
      { id: 's1', title: 'Test', status: 'done', turn_count: 2 },
    ]
    mockedInvoke.mockResolvedValueOnce(mockSessions)

    const result = await chatListSessions()

    expect(mockedInvoke).toHaveBeenCalledWith('chat_list_sessions')
    expect(result).toEqual(mockSessions)
  })

  it('returns empty array when no sessions', async () => {
    mockedInvoke.mockResolvedValueOnce([])

    const result = await chatListSessions()

    expect(result).toEqual([])
  })
})

describe('chatGetSessionDetail', () => {
  it('calls chat_get_session_detail with correct params', async () => {
    const mockDetail = {
      id: 's1',
      title: 'Test',
      turns: [],
    }
    mockedInvoke.mockResolvedValueOnce(mockDetail)

    const result = await chatGetSessionDetail('s1')

    expect(mockedInvoke).toHaveBeenCalledWith('chat_get_session_detail', {
      sessionId: 's1',
    })
    expect(result).toEqual(mockDetail)
  })

  it('returns null for nonexistent session', async () => {
    mockedInvoke.mockResolvedValueOnce(null)

    const result = await chatGetSessionDetail('nonexistent')

    expect(result).toBeNull()
  })
})

describe('chatCreateSession', () => {
  it('calls chat_create_session with all fields', async () => {
    const mockResult = { id: 's1', title: 'New', turns: [] }
    mockedInvoke.mockResolvedValueOnce(mockResult)

    const result = await chatCreateSession({
      id: 's1',
      title: 'New Session',
      status: 'running',
      agentId: 'agent-1',
      sessionLlmModel: 'gpt-4',
    })

    expect(mockedInvoke).toHaveBeenCalledWith('chat_create_session', {
      id: 's1',
      title: 'New Session',
      status: 'running',
      agentId: 'agent-1',
      agentSnapshotJson: null,
      botTargetJson: null,
      sessionLlmProviderId: null,
      sessionLlmModel: 'gpt-4',
    })
    expect(result).toEqual(mockResult)
  })

  it('passes null for unspecified optional fields', async () => {
    mockedInvoke.mockResolvedValueOnce({ id: 's1', turns: [] })

    await chatCreateSession({
      id: 's1',
      title: 'Minimal',
      status: 'running',
    })

    expect(mockedInvoke).toHaveBeenCalledWith('chat_create_session', {
      id: 's1',
      title: 'Minimal',
      status: 'running',
      agentId: null,
      agentSnapshotJson: null,
      botTargetJson: null,
      sessionLlmProviderId: null,
      sessionLlmModel: null,
    })
  })
})

describe('chatAppendTurn', () => {
  it('calls chat_append_turn with correct params', async () => {
    const mockTurn = {
      id: 't1',
      session_id: 's1',
      turn_index: 0,
      prompt: 'Hello',
      answer: '',
      status: 'running',
    }
    mockedInvoke.mockResolvedValueOnce(mockTurn)

    const result = await chatAppendTurn({
      id: 't1',
      sessionId: 's1',
      turnIndex: 0,
      prompt: 'Hello',
      status: 'running',
    })

    expect(mockedInvoke).toHaveBeenCalledWith('chat_append_turn', {
      id: 't1',
      sessionId: 's1',
      turnIndex: 0,
      prompt: 'Hello',
      answer: '',
      thinking: '',
      status: 'running',
      usageJson: null,
      responseSegmentsJson: null,
      toolCallsJson: null,
      activityJson: null,
    })
    expect(result).toEqual(mockTurn)
  })

  it('passes optional fields when provided', async () => {
    mockedInvoke.mockResolvedValueOnce({ id: 't1' })

    await chatAppendTurn({
      id: 't1',
      sessionId: 's1',
      turnIndex: 0,
      prompt: 'Hi',
      answer: 'Hello',
      thinking: 'hmm',
      status: 'done',
      usageJson: '{"inputTokens":10}',
      activityJson: '[]',
    })

    expect(mockedInvoke).toHaveBeenCalledWith('chat_append_turn', {
      id: 't1',
      sessionId: 's1',
      turnIndex: 0,
      prompt: 'Hi',
      answer: 'Hello',
      thinking: 'hmm',
      status: 'done',
      usageJson: '{"inputTokens":10}',
      responseSegmentsJson: null,
      toolCallsJson: null,
      activityJson: '[]',
    })
  })
})

describe('chatUpdateTurn', () => {
  it('calls chat_update_turn with partial updates', async () => {
    const mockTurn = { id: 't1', answer: 'Updated', status: 'done' }
    mockedInvoke.mockResolvedValueOnce(mockTurn)

    const result = await chatUpdateTurn({
      id: 't1',
      answer: 'Updated',
      status: 'done',
    })

    expect(mockedInvoke).toHaveBeenCalledWith('chat_update_turn', {
      id: 't1',
      answer: 'Updated',
      thinking: null,
      status: 'done',
      completedAt: null,
      usageJson: null,
      responseSegmentsJson: null,
      toolCallsJson: null,
      activityJson: null,
    })
    expect(result).toEqual(mockTurn)
  })

  it('passes completedAt when provided', async () => {
    mockedInvoke.mockResolvedValueOnce({ id: 't1' })

    await chatUpdateTurn({
      id: 't1',
      status: 'done',
      completedAt: 1700000000500,
    })

    expect(mockedInvoke).toHaveBeenCalledWith('chat_update_turn', {
      id: 't1',
      answer: null,
      thinking: null,
      status: 'done',
      completedAt: 1700000000500,
      usageJson: null,
      responseSegmentsJson: null,
      toolCallsJson: null,
      activityJson: null,
    })
  })
})

describe('chatDeleteSession', () => {
  it('calls chat_delete_session with sessionId', async () => {
    mockedInvoke.mockResolvedValueOnce(undefined)

    await chatDeleteSession('s1')

    expect(mockedInvoke).toHaveBeenCalledWith('chat_delete_session', {
      sessionId: 's1',
    })
  })
})

describe('chatClearAllSessions', () => {
  it('calls chat_clear_all_sessions', async () => {
    mockedInvoke.mockResolvedValueOnce(undefined)

    await chatClearAllSessions()

    expect(mockedInvoke).toHaveBeenCalledWith('chat_clear_all_sessions')
  })
})

describe('chatMigrateHistoryV1', () => {
  it('calls chat_migrate_history_v1', async () => {
    mockedInvoke.mockResolvedValueOnce('Migrated { sessions: 1, turns: 2 }')

    const result = await chatMigrateHistoryV1()

    expect(mockedInvoke).toHaveBeenCalledWith('chat_migrate_history_v1')
    expect(result).toContain('Migrated')
  })
})
