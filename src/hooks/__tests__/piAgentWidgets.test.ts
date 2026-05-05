import { describe, expect, it } from 'vitest'
import { appendOrReplaceWidgetSegment } from '../piAgent/piAgentWidgets'
import type { ResponseSegment } from '../../types'
import { parseResponseSegments } from '../piAgent/piAgentPure'

describe('appendOrReplaceWidgetSegment', () => {
  it('appends a new widget segment when none exists', () => {
    const next = appendOrReplaceWidgetSegment([], {
      type: 'widget',
      widget: {
        kind: 'ask_user',
        widgetId: 'ask-1',
        version: 1,
        title: '给我一个明示',
        status: 'pending',
        questions: [
          {
            id: 'format',
            type: 'single_select',
            label: '你要 Word 还是 PPT？',
            recommendedOptionId: 'doc',
            options: [
              { id: 'doc', label: 'Word 文档' },
              { id: 'ppt', label: 'PPT 演示' },
              { id: 'other', label: '其他' },
            ],
          },
        ],
      },
    })

    expect(next).toHaveLength(1)
    expect(next[0]).toMatchObject({
      type: 'widget',
      widget: { widgetId: 'ask-1', status: 'pending' },
    })
  })

  it('replaces an existing widget segment with the same widgetId', () => {
    const previous: ResponseSegment[] = [
      {
        type: 'widget',
        widget: {
          kind: 'ask_user',
          widgetId: 'ask-1',
          version: 1,
          title: '给我一个明示',
          status: 'pending',
          questions: [
            {
              id: 'format',
              type: 'single_select',
              label: '你要 Word 还是 PPT？',
              recommendedOptionId: 'doc',
              options: [
                { id: 'doc', label: 'Word 文档' },
                { id: 'ppt', label: 'PPT 演示' },
                { id: 'other', label: '其他' },
              ],
            },
          ],
        },
      },
    ]

    const next = appendOrReplaceWidgetSegment(previous, {
      type: 'widget',
      widget: {
        kind: 'ask_user',
        widgetId: 'ask-1',
        version: 1,
        title: '给我一个明示',
        status: 'submitted',
        questions: [
          {
            id: 'format',
            type: 'single_select',
            label: '你要 Word 还是 PPT？',
            recommendedOptionId: 'doc',
            options: [
              { id: 'doc', label: 'Word 文档' },
              { id: 'ppt', label: 'PPT 演示' },
              { id: 'other', label: '其他' },
            ],
          },
        ],
      },
    })

    expect(next).toHaveLength(1)
    expect(next[0]).toMatchObject({
      type: 'widget',
      widget: { widgetId: 'ask-1', status: 'submitted' },
    })
  })
})

describe('parseResponseSegments widget support', () => {
  it('parses ask_user widget segments', () => {
    const parsed = parseResponseSegments([
      {
        type: 'widget',
        widget: {
          kind: 'ask_user',
          widgetId: 'ask-42',
          version: 1,
          title: '继续前请确认',
          status: 'pending',
          questions: [
            {
              id: 'mode',
              type: 'single_select',
              label: '你想先推进哪部分？',
              recommendedOptionId: 'mvp',
              options: [
                { id: 'mvp', label: 'MVP' },
                { id: 'full', label: '完整系统' },
                { id: 'other', label: '其他' },
              ],
            },
          ],
        },
      },
    ])

    expect(parsed).toEqual([
      {
        type: 'widget',
        widget: {
          kind: 'ask_user',
          widgetId: 'ask-42',
          version: 1,
          title: '继续前请确认',
          status: 'pending',
          questions: [
            {
              id: 'mode',
              type: 'single_select',
              label: '你想先推进哪部分？',
              recommendedOptionId: 'mvp',
              options: [
                { id: 'mvp', label: 'MVP' },
                { id: 'full', label: '完整系统' },
                { id: 'other', label: '其他' },
              ],
            },
          ],
        },
      },
    ])
  })

  it('rejects malformed widget segments', () => {
    const parsed = parseResponseSegments([
      {
        type: 'widget',
        widget: {
          kind: 'ask_user',
          widgetId: 'ask-bad',
          version: 1,
          title: 'bad',
          status: 'pending',
          questions: [],
        },
      },
    ])

    expect(parsed).toBeUndefined()
  })
})
