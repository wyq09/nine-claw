import type { SessionLlmLogInfo } from '../../../lib/sessionLlmLogClient'

export type SessionLlmLogSection = {
  id: string
  title: string
  body: string
  tone: 'start' | 'finish' | 'tool' | 'plain'
}

export function matchesSessionLogScope(
  event: { workspaceId?: string | null; sessionId?: string | null },
  scope: { workspaceId?: string | null; sessionId?: string | null },
): boolean {
  const desiredSession = scope.sessionId?.trim() ?? ''
  if (desiredSession && event.sessionId?.trim() !== desiredSession) return false

  const desiredWorkspace = scope.workspaceId?.trim() ?? ''
  const eventWorkspace = event.workspaceId?.trim() ?? ''
  if (desiredWorkspace) return eventWorkspace === desiredWorkspace
  return eventWorkspace === ''
}

export function filterSessionLogs(
  logs: SessionLlmLogInfo[],
  query: string,
): SessionLlmLogInfo[] {
  const q = query.trim().toLowerCase()
  if (!q) return logs
  return logs.filter((item) => {
    const haystack = `${item.sessionId}\n${item.path}\n${item.preview}`.toLowerCase()
    return haystack.includes(q)
  })
}

export function parseSessionLlmLogSections(content: string): SessionLlmLogSection[] {
  const lines = content.split(/\r?\n/)
  const sections: SessionLlmLogSection[] = []
  let current: SessionLlmLogSection | null = null

  const flush = () => {
    if (!current) return
    sections.push({ ...current, body: current.body.trim() })
  }

  for (const line of lines) {
    const match = /^(##|###)\s+(.+?)\s*$/.exec(line)
    if (match) {
      flush()
      const title = match[2]
      current = {
        id: `${sections.length}-${title}`,
        title,
        body: '',
        tone: title.includes('Start')
          ? 'start'
          : title.includes('Finish')
            ? 'finish'
            : title.includes('Tool')
              ? 'tool'
              : 'plain',
      }
      continue
    }
    if (!current) {
      current = { id: '0-log', title: 'Session Log', body: '', tone: 'plain' }
    }
    current.body += `${line}\n`
  }
  flush()
  return sections.filter((section) => section.title.trim() || section.body.trim())
}

export function formatLogSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}
