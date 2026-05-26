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
  ChatSessionListItem,
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
  return buildHistoryItemFromSessionFields({
    id: detail.id,
    title: detail.title,
    status: detail.status,
    createdAt: detail.created_at,
    updatedAt: detail.updated_at,
    agentSnapshotJson: detail.agent_snapshot_json,
    botTargetJson: detail.bot_target_json,
    sessionLlmProviderId: detail.session_llm_provider_id,
    sessionLlmModel: detail.session_llm_model,
    workspaceId: detail.workspace_id,
    topicWorkspaceDir: detail.topic_workspace_dir ?? null,
    currentWorkspaceDir: detail.current_workspace_dir ?? null,
    turns: detail.turns.map(turnRowToConversationTurn),
  })
}

/** Convert a ChatSessionListItem into a HistoryItem without turns. */
export function sessionListItemToHistoryItem(item: ChatSessionListItem): HistoryItem {
  return buildHistoryItemFromSessionFields({
    id: item.id,
    title: item.title,
    status: item.status,
    createdAt: item.created_at,
    updatedAt: item.updated_at,
    agentSnapshotJson: item.agent_snapshot_json,
    botTargetJson: item.bot_target_json,
    sessionLlmProviderId: item.session_llm_provider_id,
    sessionLlmModel: item.session_llm_model,
    workspaceId: item.workspace_id,
    topicWorkspaceDir: item.topic_workspace_dir ?? null,
    currentWorkspaceDir: item.current_workspace_dir ?? null,
    turns: [],
  })
}

function buildHistoryItemFromSessionFields(input: {
  id: string
  title: string
  status: HistoryStatus
  createdAt: number
  updatedAt: number
  agentSnapshotJson: string | null
  botTargetJson: string | null
  sessionLlmProviderId: string | null
  sessionLlmModel: string | null
  workspaceId: string | null
  topicWorkspaceDir: string | null
  currentWorkspaceDir: string | null
  turns: ConversationTurn[]
}): HistoryItem {
  return {
    id: input.id,
    title: input.title,
    status: input.status,
    createdAt: input.createdAt,
    updatedAt: input.updatedAt,
    turns: input.turns,
    agent: parseJsonField<HistoryItem['agent']>(input.agentSnapshotJson),
    botTarget: parseJsonField<HistoryItem['botTarget']>(input.botTargetJson),
    sessionLlmProviderId: input.sessionLlmProviderId ?? undefined,
    sessionLlmModel: input.sessionLlmModel ?? undefined,
    workspaceId: input.workspaceId ?? undefined,
    topicWorkspaceDir: input.topicWorkspaceDir ?? undefined,
    currentWorkspaceDir: input.currentWorkspaceDir ?? undefined,
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
    speakerAgentId: row.speaker_agent_id ?? undefined,
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
