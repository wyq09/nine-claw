import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export type LlmTraceSystemPromptSection = { label: string; content: string }

export type LlmTraceMessageBlock = {
  id: string
  role: 'system' | 'user' | string
  label: string
  content: string
}

export type LlmTraceResponseBlock = {
  id: string
  kind: 'thinking' | 'output' | string
  label: string
  content: string
}

export type LlmTraceToolCall = {
  toolCallId: string
  toolName: string
  argsJson: string
  resultText: string
  status: string
  isError?: boolean | null
  startedAt: number
  finishedAt?: number | null
}

export type LlmTraceUsage = {
  inputTokens?: number | null
  outputTokens?: number | null
  cacheReadTokens?: number | null
  cacheWriteTokens?: number | null
  totalTokens?: number | null
}

export type LlmTraceEntry = {
  id: string
  workspaceId?: string | null
  kind: 'main_pi' | 'delegate' | 'action_llm' | string
  traceType?: 'agent_llm' | 'action_llm' | 'agent_agent' | string
  callerKind?: 'agent' | 'action' | string
  targetKind?: 'model' | 'agent' | string
  callerAgentId: string
  callerAgentName: string
  targetAgentId?: string | null
  targetAgentName?: string | null
  sessionId?: string | null
  parentTraceId?: string | null
  provider?: string | null
  model?: string | null
  responseId?: string | null
  status: 'running' | 'done' | 'error' | 'aborted' | string
  error?: string | null
  startedAt: number
  finishedAt?: number | null
  durationMs?: number | null
  usage?: LlmTraceUsage | null
  systemPrompts?: LlmTraceSystemPromptSection[]
  userMessage?: string
  responseText?: string
  thinkingText?: string
  messageBlocks?: LlmTraceMessageBlock[]
  responseBlocks?: LlmTraceResponseBlock[]
  toolCalls: LlmTraceToolCall[]
}

export async function workspaceLlmTraceStatus(workspaceId: string): Promise<boolean> {
  return invoke<boolean>('workspace_llm_trace_status', { workspaceId })
}

export async function workspaceLlmTraceSetEnabled(workspaceId: string, enabled: boolean) {
  return invoke('workspace_llm_trace_set_enabled', { workspaceId, enabled })
}

export async function llmTraceList(options: {
  workspaceId?: string | null
  sessionId?: string | null
  days?: number
  limit?: number
}): Promise<LlmTraceEntry[]> {
  return invoke<LlmTraceEntry[]>('llm_trace_list', {
    workspaceId: options.workspaceId ?? null,
    sessionId: options.sessionId ?? null,
    days: options.days ?? null,
    limit: options.limit ?? null,
  })
}

export async function llmTraceClear(options: {
  workspaceId?: string | null
  sessionId?: string | null
}): Promise<void> {
  await invoke('llm_trace_clear', {
    workspaceId: options.workspaceId ?? null,
    sessionId: options.sessionId ?? null,
  })
}

export type LlmTraceEvent = {
  phase: 'started' | 'updated' | 'finalized'
  entry: LlmTraceEntry
  delta?: {
    kind: 'response' | 'thinking' | string
    text: string
  } | null
}

export async function onLlmTraceEvent(
  handler: (payload: LlmTraceEvent) => void,
): Promise<() => void> {
  const unlisten = await listen<LlmTraceEvent>('workspace.llm_trace', (event) => {
    handler(event.payload)
  })
  return () => unlisten()
}
