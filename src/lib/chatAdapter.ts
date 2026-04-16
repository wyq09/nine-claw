/**
 * Adapter functions to convert structured SQLite chat API responses
 * back to the frontend's existing HistoryItem / ConversationTurn types.
 *
 * This allows a gradual migration: the UI components continue to work
 * with the same types, while the data source shifts from history_v1 blob
 * to structured SQLite queries.
 */

import type {
  ChatSessionDetail,
  ChatTurnRow,
  ConversationTurn,
  HistoryItem,
  HistoryStatus,
  ActivityEntry,
  ToolCallEntry,
  ResponseSegment,
  TokenUsage,
} from '../types'

/** Convert a ChatSessionDetail + its turns into a HistoryItem. */
export function sessionDetailToHistoryItem(detail: ChatSessionDetail): HistoryItem {
  return {
    id: detail.id,
    title: detail.title,
    status: detail.status as HistoryStatus,
    createdAt: detail.created_at,
    updatedAt: detail.updated_at,
    turns: detail.turns.map(turnRowToConversationTurn),
    agent: parseJsonField<HistoryItem['agent']>(detail.agent_snapshot_json),
    botTarget: parseJsonField<HistoryItem['botTarget']>(detail.bot_target_json),
    sessionLlmProviderId: detail.session_llm_provider_id ?? undefined,
    sessionLlmModel: detail.session_llm_model ?? undefined,
  }
}

/** Convert a ChatTurnRow into a ConversationTurn. */
export function turnRowToConversationTurn(row: ChatTurnRow): ConversationTurn {
  return {
    id: row.id,
    prompt: row.prompt,
    answer: row.answer,
    status: row.status as HistoryStatus,
    createdAt: row.created_at,
    completedAt: row.completed_at ?? undefined,
    usage: parseJsonField<TokenUsage>(row.usage_json) ?? undefined,
    activity: parseJsonField<ActivityEntry[]>(row.activity_json) ?? [],
    thinking: row.thinking,
    toolCalls: parseJsonField<ToolCallEntry[]>(row.tool_calls_json) ?? [],
    responseSegments: parseJsonField<ResponseSegment[]>(row.response_segments_json) ?? undefined,
  }
}

/** Safely parse a JSON string field. */
function parseJsonField<T>(json: string | null | undefined): T | undefined {
  if (!json) return undefined
  try {
    return JSON.parse(json) as T
  } catch {
    return undefined
  }
}
