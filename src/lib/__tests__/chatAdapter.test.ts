import { describe, it, expect } from 'vitest'
import {
  sessionDetailToHistoryItem,
  sessionListItemToHistoryItem,
  turnRowToConversationTurn,
} from '../../lib/chatAdapter'
import type { ChatSessionDetail, ChatSessionListItem, ChatTurnRow } from '../../types'

describe('turnRowToConversationTurn', () => {
  it('converts a basic turn row', () => {
    const row: ChatTurnRow = {
      id: 't1',
      session_id: 's1',
      turn_index: 0,
      prompt: 'Hello',
      answer: 'Hi there!',
      thinking: 'Let me think...',
      status: 'done',
      created_at: 1700000000100,
      completed_at: 1700000000500,
      usage_json: '{"inputTokens":10,"outputTokens":20,"totalTokens":30}',
      response_segments_json: '[{"type":"text","text":"Hi there!"}]',
      tool_calls_json: null,
      activity_json: '[]',
      speaker_agent_id: null,
    }

    const turn = turnRowToConversationTurn(row)

    expect(turn.id).toBe('t1')
    expect(turn.prompt).toBe('Hello')
    expect(turn.answer).toBe('Hi there!')
    expect(turn.thinking).toBe('Let me think...')
    expect(turn.status).toBe('done')
    expect(turn.createdAt).toBe(1700000000100)
    expect(turn.completedAt).toBe(1700000000500)
    expect(turn.usage).toEqual({
      inputTokens: 10,
      outputTokens: 20,
      totalTokens: 30,
    })
    expect(turn.responseSegments).toEqual([{ type: 'text', text: 'Hi there!' }])
    expect(turn.toolCalls).toEqual([])
    expect(turn.activity).toEqual([])
  })

  it('handles null optional fields', () => {
    const row: ChatTurnRow = {
      id: 't2',
      session_id: 's1',
      turn_index: 1,
      prompt: 'Test',
      answer: '',
      thinking: '',
      status: 'running',
      created_at: 1700000001000,
      completed_at: null,
      usage_json: null,
      response_segments_json: null,
      tool_calls_json: null,
      activity_json: null,
      speaker_agent_id: null,
    }

    const turn = turnRowToConversationTurn(row)

    expect(turn.completedAt).toBeUndefined()
    expect(turn.usage).toBeUndefined()
    expect(turn.responseSegments).toBeUndefined()
    expect(turn.toolCalls).toEqual([])
    expect(turn.activity).toEqual([])
  })

  it('handles invalid JSON gracefully', () => {
    const row: ChatTurnRow = {
      id: 't3',
      session_id: 's1',
      turn_index: 0,
      prompt: 'Test',
      answer: '',
      thinking: '',
      status: 'done',
      created_at: 1700000000000,
      completed_at: null,
      usage_json: 'not valid json',
      response_segments_json: 'broken',
      tool_calls_json: '{invalid}',
      activity_json: null,
      speaker_agent_id: null,
    }

    const turn = turnRowToConversationTurn(row)

    expect(turn.usage).toBeUndefined()
    expect(turn.responseSegments).toBeUndefined()
    expect(turn.toolCalls).toEqual([])
    expect(turn.activity).toEqual([])
  })

  it('handles tool calls and activity entries', () => {
    const row: ChatTurnRow = {
      id: 't4',
      session_id: 's1',
      turn_index: 0,
      prompt: 'Read a file',
      answer: 'Here is the content',
      thinking: '',
      status: 'done',
      created_at: 1700000000000,
      completed_at: 1700000001000,
      usage_json: null,
      response_segments_json:
        '[{"type":"text","text":"Here is "},{"type":"tool","toolCallId":"tc1"},{"type":"text","text":" the rest"}]',
      tool_calls_json:
        '[{"id":"tc0","toolCallId":"tc1","toolName":"read_file","argsText":"{\\"path\\":\\"/test\\"}","resultText":"content","state":"done","createdAt":1700000000050,"completedAt":1700000000080}]',
      activity_json:
        '[{"id":"a1","label":"Reading file","detail":"/test","state":"done","createdAt":1700000000050,"completedAt":1700000000080}]',
      speaker_agent_id: null,
    }

    const turn = turnRowToConversationTurn(row)

    expect(turn.toolCalls).toHaveLength(1)
    expect(turn.toolCalls[0].toolCallId).toBe('tc1')
    expect(turn.toolCalls[0].toolName).toBe('read_file')
    expect(turn.activity).toHaveLength(1)
    expect(turn.activity[0].label).toBe('Reading file')
    expect(turn.responseSegments).toHaveLength(3)
    expect(turn.responseSegments![0]).toEqual({ type: 'text', text: 'Here is ' })
    expect(turn.responseSegments![1]).toEqual({ type: 'tool', toolCallId: 'tc1' })
  })
})

describe('sessionDetailToHistoryItem', () => {
  it('converts a full session detail with turns', () => {
    const detail: ChatSessionDetail = {
      id: 's1',
      title: 'Test Session',
      status: 'done',
      created_at: 1700000000000,
      updated_at: 1700000001000,
      agent_id: 'agent-1',
      agent_snapshot_json: JSON.stringify({
        id: 'agent-1',
        name: 'TestBot',
        summary: 'A test bot',
        description: 'Test',
        systemPrompt: '',
        skillIds: [],
        defaultProviderId: 'openai',
        defaultModel: 'gpt-4',
        executionMode: 'pooled',
      }),
      bot_target_json: null,
      session_llm_provider_id: 'openai',
      session_llm_model: 'gpt-4',
      workspace_id: null,
      turns: [
        {
          id: 't1',
          session_id: 's1',
          turn_index: 0,
          prompt: 'Hello',
          answer: 'Hi!',
          thinking: '',
          status: 'done',
          created_at: 1700000000100,
          completed_at: 1700000000500,
          usage_json: null,
          response_segments_json: null,
          tool_calls_json: null,
          activity_json: null,
          speaker_agent_id: null,
        },
      ],
    }

    const item = sessionDetailToHistoryItem(detail)

    expect(item.id).toBe('s1')
    expect(item.title).toBe('Test Session')
    expect(item.status).toBe('done')
    expect(item.createdAt).toBe(1700000000000)
    expect(item.updatedAt).toBe(1700000001000)
    expect(item.turns).toHaveLength(1)
    expect(item.turns[0].id).toBe('t1')
    expect(item.turns[0].prompt).toBe('Hello')
    expect(item.agent).toBeDefined()
    expect(item.agent!.name).toBe('TestBot')
    expect(item.sessionLlmProviderId).toBe('openai')
    expect(item.sessionLlmModel).toBe('gpt-4')
    expect(item.botTarget).toBeUndefined()
  })

  it('handles minimal session without optional fields', () => {
    const detail: ChatSessionDetail = {
      id: 's2',
      title: 'Minimal',
      status: 'running',
      created_at: 1700000000000,
      updated_at: 1700000000000,
      agent_id: null,
      agent_snapshot_json: null,
      bot_target_json: null,
      session_llm_provider_id: null,
      session_llm_model: null,
      workspace_id: null,
      turns: [],
    }

    const item = sessionDetailToHistoryItem(detail)

    expect(item.agent).toBeUndefined()
    expect(item.botTarget).toBeUndefined()
    expect(item.sessionLlmProviderId).toBeUndefined()
    expect(item.sessionLlmModel).toBeUndefined()
    expect(item.turns).toEqual([])
  })

  it('parses bot target from json', () => {
    const detail: ChatSessionDetail = {
      id: 's3',
      title: 'Bot Chat',
      status: 'done',
      created_at: 1700000000000,
      updated_at: 1700000000000,
      agent_id: null,
      agent_snapshot_json: null,
      bot_target_json: '{"channelId":"ch1","userId":"u1"}',
      session_llm_provider_id: null,
      session_llm_model: null,
      workspace_id: null,
      turns: [],
    }

    const item = sessionDetailToHistoryItem(detail)

    expect(item.botTarget).toEqual({ channelId: 'ch1', userId: 'u1' })
  })

  it('converts a session list item without turns', () => {
    const item: ChatSessionListItem = {
      id: 's4',
      title: 'Recovered Session',
      status: 'running',
      created_at: 1700000003000,
      updated_at: 1700000004000,
      agent_id: 'agent-1',
      agent_snapshot_json: JSON.stringify({
        id: 'agent-1',
        name: '九节虾',
      }),
      bot_target_json: '{"channelId":"wechat","userId":"u42"}',
      session_llm_provider_id: 'openai',
      session_llm_model: 'gpt-4o-mini',
      workspace_id: null,
      turn_count: 3,
    }

    const historyItem = sessionListItemToHistoryItem(item)

    expect(historyItem).toMatchObject({
      id: 's4',
      title: 'Recovered Session',
      status: 'running',
      createdAt: 1700000003000,
      updatedAt: 1700000004000,
      sessionLlmProviderId: 'openai',
      sessionLlmModel: 'gpt-4o-mini',
      botTarget: { channelId: 'wechat', userId: 'u42' },
    })
    expect(historyItem.turns).toEqual([])
    expect(historyItem.agent?.name).toBe('九节虾')
  })
})
