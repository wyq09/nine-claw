export type ConversationRole = 'system' | 'user' | 'assistant' | 'tool' | 'toolResult' | 'custom'

export type ToolCallLike = {
  id?: string
  name?: string
  arguments?: unknown
  function?: {
    name?: string
    arguments?: unknown
  }
}

export type ContentBlockLike = {
  type?: string
  text?: string
  id?: string
  name?: string
  toolCallId?: string
  tool_call_id?: string
  toolUseId?: string
  tool_use_id?: string
  content?: unknown
  arguments?: unknown
}

export type ConversationMessage = {
  role: ConversationRole | string
  content?: string | ContentBlockLike[] | unknown
  tool_calls?: ToolCallLike[]
  tool_call_id?: string
  toolCallId?: string
  name?: string
  timestamp?: number
  system_injected?: boolean
  compressed_summary?: boolean
  chunk_path?: string | null
  topics?: string | null
  metadata?: Record<string, unknown>
}

export type CompressionConfig = {
  tokenThreshold: number
  messageCountThreshold: number
  targetCompressedTokens: number
  maxRecentMessages: number
  idleCompressionEnabled: boolean
  idleCompressionDelayMs: number
  idleTokenThreshold: number
}

export type CompressionState = {
  messages: ConversationMessage[]
  usedTokens: number
  compressionLevel: number
  nextChunk: number
}

export type CompressionTriggerReason = 'token_threshold' | 'message_count_threshold' | 'idle'

export type CompressionContext = {
  compressionMessage: ConversationMessage
  insertedMessages: ConversationMessage[]
  recentMessages: ConversationMessage[]
  archivedMessages: ConversationMessage[]
  compressionLevel: number
  originalTokenCount: number
  originalMessageCount: number
  reason: CompressionTriggerReason
}

export type CompressionArchive = {
  path: string
  content: string
  topics: string | null
  messageCount: number
}

export type CompressionResult =
  | {
      status: 'skipped'
      reason: 'disabled' | 'below_threshold' | 'nothing_to_archive'
      state: CompressionState
    }
  | {
      status: 'compressed'
      state: CompressionState
      context: CompressionContext
      archive: CompressionArchive | null
      summary: string
      topics: string | null
    }

export type CompressionLlmCall = (
  messages: ConversationMessage[],
  context: CompressionContext,
  signal?: AbortSignal,
) => Promise<string>

export type ChunkArchiveWriter = (path: string, content: string) => Promise<void> | void
