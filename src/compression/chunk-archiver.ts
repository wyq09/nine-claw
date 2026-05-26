import { getToolName } from './message-pair-utils'
import type { ChunkArchiveWriter, ConversationMessage } from './types'

export type ChunkArchiveInput = {
  sessionId: string
  chunk: number
  compressionLevel: number
  archivedAt?: string
  topics?: string | null
  messages: ConversationMessage[]
  rootDir?: string
  writer?: ChunkArchiveWriter
}

export type BuiltChunkArchive = {
  path: string
  content: string
}

export async function archiveCompressedChunk(input: ChunkArchiveInput): Promise<BuiltChunkArchive | null> {
  const messages = archiveableMessages(input.messages)
  if (!messages.length) return null

  const path = chunkPath(input.rootDir ?? '.', input.sessionId, input.chunk)
  const content = buildChunkMarkdown({
    ...input,
    messages,
    archivedAt: input.archivedAt ?? new Date().toISOString(),
  })
  await input.writer?.(path, content)
  return { path, content }
}

export function buildChunkMarkdown(input: Required<Pick<ChunkArchiveInput, 'archivedAt'>> & ChunkArchiveInput): string {
  const messages = archiveableMessages(input.messages)
  const lines = [
    '---',
    `session_id: ${yamlScalar(input.sessionId)}`,
    `chunk: ${input.chunk}`,
    `compression_level: ${input.compressionLevel}`,
    `archived_at: ${input.archivedAt}`,
    `message_count: ${messages.length}`,
  ]
  if (input.topics?.trim()) {
    lines.push(`topics: ${yamlScalar(input.topics)}`)
  }
  lines.push('---', '', `# Session Chunk ${input.chunk}`, '')
  lines.push('> 本文件包含压缩时归档的原始对话。')
  lines.push('> 可通过 file_reader 工具召回特定细节。', '')

  for (const message of messages) {
    appendMessageMarkdown(lines, message)
  }
  return lines.join('\n')
}

export function archiveableMessages(messages: ConversationMessage[]): ConversationMessage[] {
  return messages.filter(
    (message) =>
      message.role !== 'system' &&
      !message.system_injected &&
      !message.compressed_summary,
  )
}

export function chunkPath(rootDir: string, sessionId: string, chunk: number): string {
  const base = rootDir.replace(/\/+$/, '')
  return `${base}/${safeFileStem(sessionId)}-chunk-${String(chunk).padStart(4, '0')}.md`
}

function appendMessageMarkdown(lines: string[], message: ConversationMessage) {
  if (message.role === 'user') {
    lines.push('## User', '', formatContent(message.content), '')
    return
  }
  if (message.role === 'assistant') {
    lines.push('## Assistant', '')
    if (message.tool_calls?.length) {
      const tools = message.tool_calls
        .map((call) => {
          const name = getToolName(call)
          if (!name) return null
          const args = call.function?.arguments ?? call.arguments
          return args == null ? name : `${name} | ${truncateContent(formatContent(args), 240)}`
        })
        .filter(Boolean)
        .join('; ')
      if (tools) lines.push(`_Tool calls: ${tools}_`, '')
    }
    lines.push(formatContent(message.content), '')
    return
  }
  if (message.role === 'tool' || message.role === 'toolResult') {
    lines.push(`### Tool Result: ${message.name ?? 'tool'}`, '', '```')
    lines.push(truncateContent(formatContent(message.content), 500))
    lines.push('```', '')
  }
}

function formatContent(content: unknown): string {
  if (content == null) return ''
  if (typeof content === 'string') return content
  if (Array.isArray(content)) {
    return content
      .map((block) => {
        if (block && typeof block === 'object' && 'text' in block && typeof block.text === 'string') {
          return block.text
        }
        if (block && typeof block === 'object' && 'type' in block) {
          return `[${String(block.type)}]`
        }
        return String(block)
      })
      .join('\n')
  }
  try {
    return JSON.stringify(content)
  } catch {
    return String(content)
  }
}

function truncateContent(text: string, maxChars: number): string {
  if (text.length <= maxChars) return text
  return `${text.slice(0, maxChars)}\n... [truncated, ${text.length} chars total]`
}

function yamlScalar(value: string): string {
  return JSON.stringify(value)
}

function safeFileStem(value: string): string {
  const stem = value
    .trim()
    .replace(/[^a-zA-Z0-9._-]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 120)
  return stem || 'session'
}
