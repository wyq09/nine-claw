import { describe, expect, it } from 'vitest'
import { getHistorySidebarCardMeta } from '../historySidebarCardMeta'

describe('getHistorySidebarCardMeta', () => {
  it('uses relative label while running', () => {
    const meta = getHistorySidebarCardMeta('running', Date.now())
    expect(meta.icon).toBe('refresh')
    expect(meta.tone).toBe('running')
    expect(meta.label).toBe('思考中')
  })

  it('shows calendar-style time when done', () => {
    const ts = new Date(2026, 4, 13, 9, 15, 0).getTime()
    const meta = getHistorySidebarCardMeta('done', ts)
    expect(meta.icon).toBe('check')
    expect(meta.label).toBe('05/13(三) 09:15')
  })
})
