import type { BotMessageEvent } from '../../lib/piClient'
import type { ConversationTurn, HistoryItem } from '../../types'
import { buildBotConversationTitle, withBotAgentMetadata } from './piAgentPure'

export function buildPersistedBotConversationForInbound(payload: {
  existingItem?: HistoryItem
  historyId: string
  message: BotMessageEvent
  now: number
  turn: ConversationTurn
}): { item: HistoryItem; forceCreateSession: boolean } {
  const { existingItem, historyId, message, now, turn } = payload

  if (existingItem) {
    return {
      item: withBotAgentMetadata(
        {
          ...existingItem,
          status: 'running',
          updatedAt: now,
          turns: [...existingItem.turns, turn],
        },
        message,
      ),
      forceCreateSession: false,
    }
  }

  return {
    item: {
      id: historyId,
      title: buildBotConversationTitle(message),
      status: 'running',
      createdAt: now,
      updatedAt: now,
      turns: [turn],
      botTarget: {
        channelId: message.channel_id,
        userId: message.user_id,
      },
      ...(message.agent ? { agent: message.agent } : {}),
    },
    forceCreateSession: true,
  }
}

export function serializeConversationTurnForStructuredUpdate(turn: ConversationTurn) {
  return {
    id: turn.id,
    answer: turn.answer,
    thinking: turn.thinking,
    status: turn.status,
    completedAt: turn.completedAt ?? null,
    usageJson: turn.usage ? JSON.stringify(turn.usage) : null,
    responseSegmentsJson: turn.responseSegments ? JSON.stringify(turn.responseSegments) : null,
    toolCallsJson: turn.toolCalls.length > 0 ? JSON.stringify(turn.toolCalls) : null,
    activityJson: turn.activity.length > 0 ? JSON.stringify(turn.activity) : null,
  }
}
