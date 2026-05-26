import { describe, expect, it } from 'vitest'
import type { SessionLlmLogInfo } from '../../../../lib/sessionLlmLogClient'
import {
  filterSessionLogs,
  formatLogSize,
  matchesSessionLogScope,
  parseSessionLlmLogSections,
} from '../sessionLlmLogModel'

function log(overrides: Partial<SessionLlmLogInfo> = {}): SessionLlmLogInfo {
  return {
    workspaceId: null,
    sessionId: 'session-1',
    path: '/tmp/session-1.md',
    size: 123,
    modifiedAt: 1,
    preview: 'hello world',
    ...overrides,
  }
}

describe('sessionLlmLogModel', () => {
  it('matches standalone scope without leaking workspace logs', () => {
    expect(matchesSessionLogScope({ sessionId: 's1' }, { sessionId: 's1' })).toBe(true)
    expect(matchesSessionLogScope({ workspaceId: 'ws-1', sessionId: 's1' }, { sessionId: 's1' })).toBe(false)
    expect(matchesSessionLogScope({ workspaceId: 'ws-1', sessionId: 's1' }, { workspaceId: 'ws-1', sessionId: 's1' })).toBe(true)
  })

  it('filters by session id, path, and preview text', () => {
    const items = [
      log({ sessionId: 'alpha', preview: 'provider error' }),
      log({ sessionId: 'beta', path: '/tmp/tool-output.md', preview: 'done' }),
    ]
    expect(filterSessionLogs(items, 'provider')).toEqual([items[0]])
    expect(filterSessionLogs(items, 'tool-output')).toEqual([items[1]])
    expect(filterSessionLogs(items, 'beta')).toEqual([items[1]])
  })

  it('parses markdown log headings into display sections', () => {
    const sections = parseSessionLlmLogSections(`## LLM Turn Start - now

- source: \`desktop\`

### User Prompt

\`\`\`text
hi
\`\`\`

## LLM Turn Finish - later

done`)

    expect(sections.map((section) => section.title)).toEqual([
      'LLM Turn Start - now',
      'User Prompt',
      'LLM Turn Finish - later',
    ])
    expect(sections[0].tone).toBe('start')
    expect(sections[2].tone).toBe('finish')
  })

  it('formats compact log sizes', () => {
    expect(formatLogSize(0)).toBe('0 B')
    expect(formatLogSize(512)).toBe('512 B')
    expect(formatLogSize(2048)).toBe('2.0 KB')
  })
})
