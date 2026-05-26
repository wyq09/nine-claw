import { archiveCompressedChunk } from './chunk-archiver'
import { buildCompressionPrompt, normalizeCompressionConfig } from './compression-prompts'
import { selectRecentMessagesWithToolPairs } from './message-pair-utils'
import type {
  ChunkArchiveWriter,
  CompressionArchive,
  CompressionConfig,
  CompressionContext,
  CompressionLlmCall,
  CompressionResult,
  CompressionState,
  CompressionTriggerReason,
  ConversationMessage,
} from './types'

export type PrepareCompressionOptions = {
  state: CompressionState
  config?: Partial<CompressionConfig>
  forceIdle?: boolean
}

export type RunCompressionOptions = PrepareCompressionOptions & {
  sessionId: string
  archiveRootDir?: string
  archiveWriter?: ChunkArchiveWriter
  callLlm: CompressionLlmCall
  signal?: AbortSignal
}

export function shouldTriggerCompression(options: PrepareCompressionOptions): CompressionTriggerReason | null {
  const config = normalizeCompressionConfig(options.config)
  const { state } = options
  const nonSystemMessageCount = state.messages.filter((message) => message.role !== 'system').length

  if (options.forceIdle) {
    if (!config.idleCompressionEnabled) return null
    if (nonSystemMessageCount <= config.maxRecentMessages) return null
    return state.usedTokens >= config.idleTokenThreshold ? 'idle' : null
  }

  if (state.usedTokens >= config.tokenThreshold) return 'token_threshold'
  if (nonSystemMessageCount >= config.messageCountThreshold) return 'message_count_threshold'
  return null
}

export function prepareCompressionContext(options: PrepareCompressionOptions): CompressionContext | null {
  const config = normalizeCompressionConfig(options.config)
  const reason = shouldTriggerCompression(options)
  if (!reason) return null

  const level = options.state.compressionLevel + 1
  const recentMessages = selectRecentMessagesWithToolPairs(options.state.messages, config.maxRecentMessages)
  const recentSet = new Set(recentMessages)
  const archivedMessages = options.state.messages.filter(
    (message) => message.role !== 'system' && !recentSet.has(message) && !message.system_injected,
  )
  if (!archivedMessages.length) return null

  const compressionMessage = buildCompressionMessage(level, config)
  return {
    compressionMessage,
    insertedMessages: [...options.state.messages, compressionMessage],
    recentMessages,
    archivedMessages,
    compressionLevel: level,
    originalTokenCount: options.state.usedTokens,
    originalMessageCount: options.state.messages.length,
    reason,
  }
}

export async function runInsertThenCompress(options: RunCompressionOptions): Promise<CompressionResult> {
  const context = prepareCompressionContext(options)
  if (!context) {
    return { status: 'skipped', reason: 'below_threshold', state: cloneState(options.state) }
  }

  throwIfAborted(options.signal)
  const summary = await options.callLlm(context.insertedMessages, context, options.signal)
  throwIfAborted(options.signal)

  const topics = parseTopics(summary)
  const archive = await buildArchive(options, context, topics)
  const rebuiltMessages = rebuildHistoryWithCompression({
    originalMessages: options.state.messages,
    compressedContent: summary,
    recentMessages: context.recentMessages,
    chunkPath: archive?.path ?? null,
    topics,
  })

  const nextState: CompressionState = {
    messages: rebuiltMessages,
    usedTokens: estimateMessagesTokens(rebuiltMessages),
    compressionLevel: context.compressionLevel,
    nextChunk: options.state.nextChunk + 1,
  }

  return {
    status: 'compressed',
    state: nextState,
    context,
    archive,
    summary,
    topics,
  }
}

export function buildCompressionMessage(level: number, configInput?: Partial<CompressionConfig>): ConversationMessage {
  const config = normalizeCompressionConfig(configInput)
  return {
    role: 'user',
    content: buildCompressionPrompt(level, config),
    system_injected: true,
    metadata: {
      system_injected: true,
      compression_level: level,
      purpose: 'insert_then_compress',
    },
  }
}

export function rebuildHistoryWithCompression(input: {
  originalMessages: ConversationMessage[]
  compressedContent: string
  recentMessages: ConversationMessage[]
  chunkPath?: string | null
  topics?: string | null
}): ConversationMessage[] {
  const systemMessages = input.originalMessages.filter((message) => message.role === 'system')
  const compressedSummary = buildCompressedSummaryMessage(input.compressedContent, input.chunkPath ?? null, input.topics ?? null)
  const recent = input.recentMessages.filter((message) => message.role !== 'system')
  return [...systemMessages, compressedSummary, ...recent]
}

export function buildCompressedSummaryMessage(
  compressedContent: string,
  chunkPath: string | null,
  topics: string | null,
): ConversationMessage {
  const body = stripTopics(compressedContent).trim()
  const archiveAnchor = chunkPath
    ? `\n\n---\nCurrent chunk archived at: \`${chunkPath}\`\nUse file_reader to recall exact details from this chunk.`
    : ''
  return {
    role: 'user',
    content: `[Compressed conversation summary - previous turns archived]\n\n${body}${archiveAnchor}`,
    compressed_summary: true,
    system_injected: true,
    chunk_path: chunkPath,
    topics,
    metadata: {
      compressed_summary: true,
      system_injected: true,
      chunk_path: chunkPath,
      topics,
    },
  }
}

export function parseTopics(content: string): string | null {
  const match = /<topics>([\s\S]*?)<\/topics>/i.exec(content)
  const topics = match?.[1]?.trim()
  return topics || null
}

export function stripTopics(content: string): string {
  return content.replace(/<topics>[\s\S]*?<\/topics>\s*/i, '')
}

export function estimateMessagesTokens(messages: ConversationMessage[]): number {
  return messages.reduce((total, message) => total + estimateMessageTokens(message), 0)
}

export function estimateMessageTokens(message: ConversationMessage): number {
  return Math.ceil(messageToText(message).length / 4)
}

function messageToText(message: ConversationMessage): string {
  const parts = [message.role]
  if (typeof message.content === 'string') {
    parts.push(message.content)
  } else if (Array.isArray(message.content)) {
    parts.push(JSON.stringify(message.content))
  } else if (message.content != null) {
    parts.push(String(message.content))
  }
  if (message.tool_calls?.length) {
    parts.push(JSON.stringify(message.tool_calls))
  }
  return parts.join('\n')
}

async function buildArchive(
  options: RunCompressionOptions,
  context: CompressionContext,
  topics: string | null,
): Promise<CompressionArchive | null> {
  const archive = await archiveCompressedChunk({
    sessionId: options.sessionId,
    chunk: options.state.nextChunk,
    compressionLevel: context.compressionLevel,
    topics,
    messages: context.archivedMessages,
    rootDir: options.archiveRootDir,
    writer: options.archiveWriter,
  })
  if (!archive) return null
  return {
    path: archive.path,
    content: archive.content,
    topics,
    messageCount: context.archivedMessages.length,
  }
}

function cloneState(state: CompressionState): CompressionState {
  return {
    messages: [...state.messages],
    usedTokens: state.usedTokens,
    compressionLevel: state.compressionLevel,
    nextChunk: state.nextChunk,
  }
}

function throwIfAborted(signal: AbortSignal | undefined) {
  if (signal?.aborted) {
    throw new DOMException('Compression aborted', 'AbortError')
  }
}
