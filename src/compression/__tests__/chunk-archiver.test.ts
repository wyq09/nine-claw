import { describe, expect, it, vi } from 'vitest'
import { archiveCompressedChunk, buildChunkMarkdown } from '../chunk-archiver'
import type { ConversationMessage } from '../types'

describe('chunk archiver', () => {
  it('A9 writes markdown with front matter and truncates tool results', async () => {
    const writer = vi.fn()
    const messages: ConversationMessage[] = [
      { role: 'system', content: 'sys' },
      { role: 'user', content: 'please fix it' },
      {
        role: 'assistant',
        content: 'I will read',
        tool_calls: [{ id: 'call-1', function: { name: 'read_file', arguments: '{"path":"a.ts"}' } }],
      },
      { role: 'tool', name: 'read_file', tool_call_id: 'call-1', content: 'x'.repeat(700) },
      { role: 'user', content: 'internal', system_injected: true },
    ]

    const archive = await archiveCompressedChunk({
      sessionId: 'session/one',
      chunk: 1,
      compressionLevel: 1,
      archivedAt: '2026-05-20T12:00:00Z',
      topics: '文件编辑,测试',
      messages,
      rootDir: '/tmp/chunks',
      writer,
    })

    expect(archive?.path).toBe('/tmp/chunks/session_one-chunk-0001.md')
    expect(writer).toHaveBeenCalledWith(archive?.path, archive?.content)
    expect(archive?.content).toContain('session_id: "session/one"')
    expect(archive?.content).toContain('compression_level: 1')
    expect(archive?.content).toContain('topics: "文件编辑,测试"')
    expect(archive?.content).toContain('## User')
    expect(archive?.content).toContain('### Tool Result: read_file')
    expect(archive?.content).toContain('[truncated, 700 chars total]')
    expect(archive?.content).not.toContain('internal')
  })

  it('returns deterministic markdown without touching disk when no writer is provided', () => {
    const content = buildChunkMarkdown({
      sessionId: 's1',
      chunk: 2,
      compressionLevel: 3,
      archivedAt: '2026-05-20T12:00:00Z',
      topics: null,
      messages: [{ role: 'user', content: 'hello' }],
    })

    expect(content).toContain('# Session Chunk 2')
    expect(content).toContain('message_count: 1')
  })
})
