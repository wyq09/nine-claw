import { describe, expect, it } from 'vitest'
import { validateAskUserToolPolicy, type AskUserWidget } from '../widgetTypes'

describe('validateAskUserToolPolicy', () => {
  it('accepts Alice-style ask_user widgets', () => {
    const widget: AskUserWidget = {
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
          recommendedOptionId: 'word',
          options: [
            { id: 'word', label: 'Word 文档' },
            { id: 'ppt', label: 'PPT 演示' },
            { id: 'other', label: '其他' },
          ],
        },
      ],
    }

    expect(validateAskUserToolPolicy(widget)).toEqual([])
  })

  it('rejects widgets that violate ask_user interaction rules', () => {
    const widget: AskUserWidget = {
      kind: 'ask_user',
      widgetId: 'ask-2',
      version: 1,
      title: 'bad',
      status: 'pending',
      questions: [
        {
          id: 'q1',
          type: 'single_select',
          label: '问题一',
          recommendedOptionId: 'b',
          options: [
            { id: 'a', label: 'A' },
            { id: 'b', label: 'B' },
          ],
        },
        {
          id: 'q2',
          type: 'text',
          label: '问题二',
        },
      ],
    }

    expect(validateAskUserToolPolicy(widget)).toEqual([
      'ask_user 每次只能问一个问题。',
      'ask_user 的第一个选项必须是推荐选项。',
      'ask_user 的最后一个选项必须是“其他”。',
    ])
  })
})
