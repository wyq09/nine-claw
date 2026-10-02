import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

import { listen } from '@tauri-apps/api/event'
import {
  appendToolSelectionActivity,
  formatToolSelectionActivity,
  subscribeToolSelectionEvent,
  type ToolSelectionEventPayload,
} from '../piAgent/toolSelectionActivity'

function makePayload(overrides: Partial<ToolSelectionEventPayload> = {}): ToolSelectionEventPayload {
  return {
    sessionId: 's1',
    source: 'jev',
    mode: 'jev',
    requestedModel: 'jev-1.13.0',
    resolvedModel: 'jev-1.13.0',
    profile: 'code',
    profileSource: 'jev',
    enabledTools: ['read', 'write', 'bash'],
    prunedTools: [],
    reason: '',
    latencyMs: 230,
    usage: { inputTokens: 120, outputTokens: 40 },
    ...overrides,
  }
}

describe('formatToolSelectionActivity', () => {
  it('formats a jev decision with profile, tool count and latency', () => {
    const { title, detail } = formatToolSelectionActivity(makePayload())

    expect(title).toBe('本轮工具路由')
    expect(detail).toBe('模式：jev；画像：code（jev）；启用 3 个工具；耗时 230ms')
  })

  it('appends pruned tools for a jev decision that trimmed tools', () => {
    const { detail } = formatToolSelectionActivity(
      makePayload({ prunedTools: ['web_search', 'image_gen'] }),
    )

    expect(detail).toBe('模式：jev；画像：code（jev）；启用 3 个工具；裁剪：web_search、image_gen；耗时 230ms')
  })

  it('appends the fallback reason when source is fallback', () => {
    const { detail } = formatToolSelectionActivity(
      makePayload({ source: 'fallback', reason: '请求超时' }),
    )

    expect(detail).toContain('回退：请求超时')
    expect(detail).toContain('模式：jev')
  })

  it('keeps legacy decisions with nothing pruned brief', () => {
    const { detail } = formatToolSelectionActivity(
      makePayload({ source: 'legacy', mode: 'legacy' }),
    )

    expect(detail).toBe('模式：原有')
  })

  it('still reports pruned tools for legacy source', () => {
    const { detail } = formatToolSelectionActivity(
      makePayload({ source: 'legacy', mode: 'legacy', prunedTools: ['bash'], profile: null, profileSource: null }),
    )

    expect(detail).toBe('模式：原有；启用 3 个工具；裁剪：bash；耗时 230ms')
  })

  it('omits the profile segment when profile is null', () => {
    const { detail } = formatToolSelectionActivity(makePayload({ profile: null, profileSource: null }))

    expect(detail).toBe('模式：jev；启用 3 个工具；耗时 230ms')
  })

  it('omits the profile source when only profileSource is null', () => {
    const { detail } = formatToolSelectionActivity(makePayload({ profileSource: null }))

    expect(detail).toBe('模式：jev；画像：code；启用 3 个工具；耗时 230ms')
  })

  it('omits the fallback segment when reason is blank', () => {
    const { detail } = formatToolSelectionActivity(makePayload({ source: 'fallback', reason: '  ' }))

    expect(detail).not.toContain('回退')
  })
})

describe('appendToolSelectionActivity', () => {
  const appendActivity = vi.fn()

  beforeEach(() => {
    appendActivity.mockClear()
  })

  it('routes the event to the session turn and appends a done activity', () => {
    appendToolSelectionActivity(makePayload(), 'active', new Map([['s1', 't1']]), appendActivity)

    expect(appendActivity).toHaveBeenCalledWith('s1', 't1', '本轮工具路由', expect.any(String), 'done')
  })

  it('falls back to the active history when sessionId is null', () => {
    appendToolSelectionActivity(makePayload({ sessionId: null }), 'active', new Map([['active', 't9']]), appendActivity)

    expect(appendActivity).toHaveBeenCalledWith('active', 't9', '本轮工具路由', expect.any(String), 'done')
  })

  it('drops events that cannot be mapped to a turn', () => {
    appendToolSelectionActivity(makePayload({ sessionId: null }), '', new Map(), appendActivity)
    appendToolSelectionActivity(makePayload(), 'active', new Map(), appendActivity)

    expect(appendActivity).not.toHaveBeenCalled()
  })
})

describe('subscribeToolSelectionEvent', () => {
  it('listens on pi://tool-selection and forwards the payload', async () => {
    const unlisten = vi.fn()
    const onEvent = vi.fn()
    vi.mocked(listen).mockImplementation(async (_name, handler) => {
      handler({ payload: makePayload() } as Parameters<typeof handler>[0])
      return unlisten
    })

    const dispose = await subscribeToolSelectionEvent(onEvent)
    dispose()

    expect(listen).toHaveBeenCalledWith('pi://tool-selection', expect.any(Function))
    expect(onEvent).toHaveBeenCalledWith(makePayload())
    expect(unlisten).toHaveBeenCalled()
  })
})
