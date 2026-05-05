import { describe, expect, it, vi } from 'vitest'
import { createAskUserTool, formatAskUserResult, validateAskUserPolicy } from '../ask_user_tool.mjs'

const validInput = {
  title: '继续前确认一下',
  questions: [
    {
      id: 'format',
      type: 'single_select',
      label: '你要哪种格式？',
      options: [
        { id: 'word', label: 'Word 文档' },
        { id: 'ppt', label: 'PPT 演示' },
        { id: 'other', label: '其他' },
      ],
      recommendedOptionId: 'word',
      allowCustomInput: true,
    },
  ],
}

describe('validateAskUserPolicy', () => {
  it('accepts a valid Alice-style ask_user choice question', () => {
    expect(validateAskUserPolicy(validInput)).toEqual([])
  })

  it('rejects a choice question without recommended-first and trailing 其他', () => {
    expect(
      validateAskUserPolicy({
        ...validInput,
        questions: [
          {
            ...validInput.questions[0],
            options: [
              { id: 'ppt', label: 'PPT 演示' },
              { id: 'word', label: 'Word 文档' },
            ],
            recommendedOptionId: 'word',
          },
        ],
      }),
    ).toEqual([
      'ask_user must put the recommended option first.',
      "ask_user must end with an '其他' option.",
    ])
  })
})

describe('formatAskUserResult', () => {
  it('formats a successful answer into continuation-friendly text', () => {
    const text = formatAskUserResult(validInput, {
      widgetId: 'ask_user_1',
      answers: [
        {
          questionId: 'format',
          value: 'word',
          customValue: '发我可编辑版本',
        },
      ],
    })

    expect(text).toContain('The user answered your clarification request.')
    expect(text).toContain("Continue the conversation now and complete the user's request")
    expect(text).toContain('- 你要哪种格式？: word (补充: 发我可编辑版本)')
    expect(text).toContain('"widgetId": "ask_user_1"')
  })
})

describe('createAskUserTool', () => {
  it('returns a continuation-focused tool result after a successful submission', async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        ok: true,
        widgetId: 'ask_user_2',
        answers: [
          {
            questionId: 'format',
            value: 'word',
          },
        ],
      }),
    })

    const tool = createAskUserTool({
      Type: {
        String: (value: unknown) => value,
        Optional: (value: unknown) => value,
        Array: (value: unknown) => value,
        Object: (value: unknown) => value,
        Union: (value: unknown) => value,
        Literal: (value: unknown) => value,
        Boolean: () => ({}),
        Number: () => ({}),
      },
      fetchImpl,
      processApi: {
        env: {
          NINECLAW_PROXY_BASE_URL: 'http://127.0.0.1:4312',
          NINECLAW_PROXY_SESSION_TOKEN: 'token-1',
        },
      },
    })

    const result = await tool.execute('call_1', validInput)
    expect(fetchImpl).toHaveBeenCalledOnce()
    expect(result.details).toMatchObject({
      ok: true,
      widgetId: 'ask_user_2',
    })
    expect(result.content[0].text).toContain('Continue the conversation now and complete the user')
    expect(result.content[0].text).toContain('Structured answers JSON:')
  })
})
