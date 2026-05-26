import type { ContentBlockLike, ConversationMessage, ToolCallLike } from './types'

const TOOL_RESULT_TYPES = new Set(['tool_result', 'toolResult'])
const TOOL_CALL_TYPES = new Set(['tool_call', 'toolCall'])

export function getToolCallIds(message: ConversationMessage): string[] {
  const ids = new Set<string>()
  for (const call of message.tool_calls ?? []) {
    addIfPresent(ids, call.id)
  }
  for (const block of contentBlocks(message)) {
    if (TOOL_CALL_TYPES.has(block.type ?? '')) {
      addIfPresent(ids, block.id ?? block.toolCallId ?? block.tool_call_id)
    }
  }
  return [...ids]
}

export function getToolResultIds(message: ConversationMessage): string[] {
  const ids = new Set<string>()
  addIfPresent(ids, message.tool_call_id ?? message.toolCallId)
  for (const block of contentBlocks(message)) {
    if (TOOL_RESULT_TYPES.has(block.type ?? '')) {
      addIfPresent(ids, block.tool_use_id ?? block.toolUseId ?? block.tool_call_id ?? block.toolCallId ?? block.id)
    }
  }
  return [...ids]
}

export function isToolResultMessage(message: ConversationMessage): boolean {
  if (message.role === 'tool' || message.role === 'toolResult') return true
  return getToolResultIds(message).length > 0
}

export function selectRecentMessagesWithToolPairs(
  messages: ConversationMessage[],
  maxRecentMessages: number,
): ConversationMessage[] {
  if (!messages.length || maxRecentMessages <= 0) return []

  const included = new Set<number>()
  let collected = 0
  for (let index = messages.length - 1; index >= 0 && collected < maxRecentMessages; index -= 1) {
    const message = messages[index]
    if (!message || message.role === 'system') continue
    if (!included.has(index)) {
      included.add(index)
      collected += 1
    }

    const callIds = getToolCallIds(message)
    if (callIds.length) {
      pullToolResultsAfter(messages, index, callIds, included)
    }

    const resultIds = getToolResultIds(message)
    if (resultIds.length) {
      const assistantAdded = pullAssistantBefore(messages, index, resultIds, included)
      if (assistantAdded) collected += 1
    }
  }

  return [...included]
    .sort((a, b) => a - b)
    .map((index) => truncateToolResult(messages[index]))
}

export function hasBrokenToolPairs(messages: ConversationMessage[]): boolean {
  const callIds = new Set<string>()
  const resultIds = new Set<string>()
  for (const message of messages) {
    for (const id of getToolCallIds(message)) callIds.add(id)
    for (const id of getToolResultIds(message)) resultIds.add(id)
  }
  for (const id of callIds) {
    if (!resultIds.has(id)) return true
  }
  for (const id of resultIds) {
    if (!callIds.has(id)) return true
  }
  return false
}

export function getToolName(call: ToolCallLike): string {
  return call.function?.name ?? call.name ?? ''
}

function pullToolResultsAfter(
  messages: ConversationMessage[],
  assistantIndex: number,
  callIds: string[],
  included: Set<number>,
) {
  for (let index = assistantIndex + 1; index < messages.length; index += 1) {
    const candidate = messages[index]
    if (!candidate) continue
    if (!isToolResultMessage(candidate)) break
    if (intersects(getToolResultIds(candidate), callIds)) {
      included.add(index)
    }
  }
}

function pullAssistantBefore(
  messages: ConversationMessage[],
  toolResultIndex: number,
  resultIds: string[],
  included: Set<number>,
): boolean {
  for (let index = toolResultIndex - 1; index >= 0; index -= 1) {
    const candidate = messages[index]
    if (!candidate) continue
    const callIds = getToolCallIds(candidate)
    if (candidate.role === 'assistant' && intersects(callIds, resultIds)) {
      const wasNew = !included.has(index)
      included.add(index)
      pullToolResultsAfter(messages, index, callIds, included)
      return wasNew
    }
  }
  return false
}

function truncateToolResult(message: ConversationMessage | undefined): ConversationMessage {
  if (!message) return { role: 'custom' }
  if (!isToolResultMessage(message) || typeof message.content !== 'string' || message.content.length <= 2_000) {
    return message
  }
  return {
    ...message,
    content: `${message.content.slice(0, 2_000)}...\n[Content truncated - exceeded 2000 characters]`,
  }
}

function contentBlocks(message: ConversationMessage): ContentBlockLike[] {
  return Array.isArray(message.content) ? (message.content as ContentBlockLike[]) : []
}

function addIfPresent(ids: Set<string>, value: string | undefined) {
  const trimmed = value?.trim()
  if (trimmed) ids.add(trimmed)
}

function intersects(left: string[], right: string[]): boolean {
  const rightSet = new Set(right)
  return left.some((item) => rightSet.has(item))
}
