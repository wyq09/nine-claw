import { act, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { TurnPreparingIndicator } from '../TurnPreparingIndicator'
import { TurnResponseBody, hasRenderableTurnContent } from '../TurnAndTools'
import { WidgetSegmentsContext } from '../../widgets/WidgetSegmentsContext'
import type { ConversationTurn } from '../../../types'

vi.mock('../../../components/MarkdownRenderer', () => ({
  default: ({ content }: { content: string }) => <div>{content}</div>,
}))

function buildTurn(overrides: Partial<ConversationTurn> = {}): ConversationTurn {
  return {
    id: 'turn-1',
    prompt: 'prompt',
    answer: '',
    status: 'running',
    createdAt: 1,
    thinking: '',
    activity: [],
    toolCalls: [],
    responseSegments: [],
    ...overrides,
  }
}

describe('TurnResponseBody widgets', () => {
  it('renders ask_user inline with later response text instead of pinning it to the bottom', () => {
    const turn = buildTurn({
      responseSegments: [
        { type: 'text', text: '先确认一个信息。' },
        {
          type: 'widget',
          widget: {
            kind: 'ask_user',
            widgetId: 'ask-inline-1',
            version: 1,
            title: '请选择部署方式',
            status: 'pending',
            questions: [
              {
                id: 'deploy',
                type: 'single_select',
                label: '部署方式',
                required: true,
                recommendedOptionId: 'cloud',
                options: [
                  { id: 'cloud', label: '云端' },
                  { id: 'local', label: '本地' },
                  { id: 'other', label: '其他' },
                ],
              },
            ],
          },
        },
        { type: 'text', text: '拿到答案后我会继续生成配置。' },
      ],
    })

    render(
      <WidgetSegmentsContext.Provider value={{}}>
        <TurnResponseBody
          turn={turn}
          agentBuilderActionBusyId=""
          agentBuilderActionError=""
          agentBuilderActionNotice=""
          agentBuilderActionTargetId=""
          streamLive={false}
          activeTurnId=""
          onCreateAgentDraft={vi.fn()}
          showExecutionRail
          showThinkingProcess={false}
          onImageClick={vi.fn()}
        />
      </WidgetSegmentsContext.Provider>,
    )

    const responseBlocks = document.querySelector('.turn-response-blocks')
    expect(responseBlocks).not.toBeNull()
    const children = Array.from(responseBlocks?.children ?? [])
    expect(children).toHaveLength(3)
    expect(children[0]?.textContent).toContain('先确认一个信息。')
    expect(children[1]?.textContent).toContain('Ask User')
    expect(children[2]?.textContent).toContain('拿到答案后我会继续生成配置。')
    expect(screen.getByText('请选择部署方式')).toBeInTheDocument()
  })

  it('treats widget-only turns as renderable content', () => {
    const turn = buildTurn({
      responseSegments: [
        {
          type: 'widget',
          widget: {
            kind: 'ask_user',
            widgetId: 'ask-only-1',
            version: 1,
            title: '补充一下目标',
            status: 'pending',
            questions: [
              {
                id: 'goal',
                type: 'single_select',
                label: '目标',
                required: true,
                recommendedOptionId: 'speed',
                options: [
                  { id: 'speed', label: '优先速度' },
                  { id: 'quality', label: '优先质量' },
                  { id: 'other', label: '其他' },
                ],
              },
            ],
          },
        },
      ],
    })

    expect(hasRenderableTurnContent(turn, true, false)).toBe(true)
  })
})

describe('TurnPreparingIndicator', () => {
  it('waits briefly before showing the preparing copy', async () => {
    vi.useFakeTimers()

    render(<TurnPreparingIndicator />)
    expect(screen.queryByText('准备本轮回复')).not.toBeInTheDocument()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(280)
    })

    expect(screen.getByText('准备本轮回复')).toBeInTheDocument()
    expect(screen.getByText('正在整理上下文并启动本轮任务…')).toBeInTheDocument()
    expect(screen.queryByText('正在连接模型…')).not.toBeInTheDocument()
  })
})
