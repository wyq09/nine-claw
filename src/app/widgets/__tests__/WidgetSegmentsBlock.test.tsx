import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { WidgetSegmentsBlock } from '../WidgetSegmentsBlock'
import { WidgetSegmentsContext } from '../WidgetSegmentsContext'
import type { WidgetSegment } from '../../../widgetTypes'

const askUserSegment: WidgetSegment = {
  type: 'widget',
  widget: {
    kind: 'ask_user',
    widgetId: 'ask-1',
    version: 1,
    title: '补充需求',
    description: '请选择范围并补充你的输入。',
    submitLabel: '继续',
    cancelLabel: '先跳过',
    allowSkip: true,
    status: 'pending',
    questions: [
      {
        id: 'scope',
        type: 'multi_select',
        label: '你想先做哪些部分？',
        required: true,
        allowCustomInput: true,
        recommendedOptionId: 'tool',
        options: [
          { id: 'tool', label: '工具设计' },
          { id: 'ui', label: '交互卡片' },
          { id: 'other', label: '其他' },
        ],
      },
      {
        id: 'notes',
        type: 'textarea',
        label: '补充要求',
        required: true,
      },
    ],
  },
}

describe('WidgetSegmentsBlock', () => {
  it('renders ask_user widget and submits collected answers', () => {
    const onSubmitWidget = vi.fn(async () => {})

    render(
      <WidgetSegmentsContext.Provider value={{ onSubmitWidget }}>
        <WidgetSegmentsBlock segments={[askUserSegment]} turnId="turn-1" />
      </WidgetSegmentsContext.Provider>,
    )

    fireEvent.click(screen.getByLabelText('工具设计'))
    fireEvent.change(screen.getByPlaceholderText('补充说明'), {
      target: { value: '还要支持用户自定义输入' },
    })
    fireEvent.change(screen.getByLabelText('补充要求'), {
      target: { value: '优先做标准协议，不要先写死 UI。' },
    })
    fireEvent.click(screen.getByText('继续'))

    expect(onSubmitWidget).toHaveBeenCalledWith({
      widgetId: 'ask-1',
      kind: 'ask_user',
      answers: [
        {
          questionId: 'scope',
          value: ['tool'],
          customValue: '还要支持用户自定义输入',
        },
        {
          questionId: 'notes',
          value: '优先做标准协议，不要先写死 UI。',
          customValue: '',
        },
      ],
    })
  })

  it('blocks submit when required answers are missing', () => {
    const onSubmitWidget = vi.fn(async () => {})

    render(
      <WidgetSegmentsContext.Provider value={{ onSubmitWidget }}>
        <WidgetSegmentsBlock segments={[askUserSegment]} turnId="turn-2" />
      </WidgetSegmentsContext.Provider>,
    )

    expect(screen.getByText('继续')).toBeDisabled()
    expect(onSubmitWidget).not.toHaveBeenCalled()
  })

  it('shows the recommended badge on the first preferred option', () => {
    render(<WidgetSegmentsBlock segments={[askUserSegment]} turnId="turn-3" />)

    expect(screen.getByText('推荐')).toBeInTheDocument()
  })
})
