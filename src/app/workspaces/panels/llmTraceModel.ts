import type {
  LlmTraceEntry,
  LlmTraceEvent,
  LlmTraceMessageBlock,
  LlmTraceResponseBlock,
} from '../../../lib/llmTraceClient'

export function getTraceType(entry: LlmTraceEntry): string {
  if (entry.traceType?.trim()) {
    return entry.traceType
  }
  if (entry.kind === 'delegate') {
    return 'agent_agent'
  }
  if (entry.kind === 'action_llm') {
    return 'action_llm'
  }
  return 'agent_llm'
}

export function getTraceKindLabel(entry: LlmTraceEntry): string {
  switch (getTraceType(entry)) {
    case 'action_llm':
      return '动作→大模型'
    case 'agent_agent':
      return '智能体→智能体'
    default:
      return '智能体→大模型'
  }
}

export function matchesTraceScope(
  entry: LlmTraceEntry,
  options: { workspaceId?: string | null; sessionId?: string | null },
): boolean {
  const desiredWorkspace = options.workspaceId?.trim() ?? ''
  const desiredSession = options.sessionId?.trim() ?? ''
  const entryWorkspace = entry.workspaceId?.trim() ?? ''
  const entrySession = entry.sessionId?.trim() ?? ''

  if (desiredWorkspace) {
    if (entryWorkspace !== desiredWorkspace) {
      return false
    }
  } else if (entryWorkspace) {
    return false
  }

  if (desiredSession && entrySession !== desiredSession) {
    return false
  }

  return true
}

export function getTraceMessageBlocks(entry: LlmTraceEntry): LlmTraceMessageBlock[] {
  if (entry.messageBlocks && entry.messageBlocks.length > 0) {
    return entry.messageBlocks
  }

  const systemPrompts = entry.systemPrompts ?? []
  const blocks = systemPrompts.map<LlmTraceMessageBlock>((prompt, index) => ({
    id: `system-${index}`,
    role: 'system',
    label: prompt.label,
    content: prompt.content,
  }))
  blocks.push({
    id: 'user-0',
    role: 'user',
    label: 'user_prompt',
    content: entry.userMessage ?? '',
  })
  return blocks
}

export function getTraceResponseBlocks(entry: LlmTraceEntry): LlmTraceResponseBlock[] {
  if (entry.responseBlocks && entry.responseBlocks.length > 0) {
    return entry.responseBlocks
  }

  return [
    {
      id: 'thinking-0',
      kind: 'thinking',
      label: 'thinking',
      content: entry.thinkingText ?? '',
    },
    {
      id: 'output-0',
      kind: 'output',
      label: 'assistant_reply',
      content: entry.responseText ?? '',
    },
  ]
}

function appendResponseBlockDelta(
  blocks: LlmTraceResponseBlock[] | undefined,
  kind: 'response' | 'thinking' | string,
  text: string,
): LlmTraceResponseBlock[] | undefined {
  if (!blocks || blocks.length === 0) return blocks
  const blockKind = kind === 'thinking' ? 'thinking' : 'output'
  let patched = false
  const next = blocks.map((block) => {
    if (block.kind !== blockKind) return block
    patched = true
    return { ...block, content: `${block.content ?? ''}${text}` }
  })
  return patched ? next : blocks
}

export function applyLlmTraceEvent(entries: LlmTraceEntry[], event: LlmTraceEvent): LlmTraceEntry[] {
  const deltaText = event.delta?.text ?? ''
  const deltaKind = event.delta?.kind ?? ''
  const idx = entries.findIndex((entry) => entry.id === event.entry.id)

  if (idx < 0) {
    return [event.entry, ...entries]
  }

  const current = entries[idx]
  const next = entries.slice()
  if (!deltaText) {
    next[idx] = event.entry
    return next
  }

  next[idx] = {
    ...current,
    ...event.entry,
    responseText:
      deltaKind === 'response' ? `${current.responseText ?? ''}${deltaText}` : current.responseText,
    thinkingText:
      deltaKind === 'thinking' ? `${current.thinkingText ?? ''}${deltaText}` : current.thinkingText,
    messageBlocks: current.messageBlocks ?? event.entry.messageBlocks,
    responseBlocks: appendResponseBlockDelta(current.responseBlocks, deltaKind, deltaText),
  }
  return next
}
