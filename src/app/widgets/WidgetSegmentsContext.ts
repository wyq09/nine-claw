import { createContext } from 'react'

export type WidgetSegmentsContextValue = {
  onSubmitWidget?: (payload: {
    widgetId: string
    kind: 'ask_user'
    answers: Array<{ questionId: string; value: string | string[]; customValue?: string }>
  }) => Promise<void> | void
  onCancelWidget?: (payload: { widgetId: string; kind: 'ask_user' }) => Promise<void> | void
}

export const DEFAULT_WIDGET_SEGMENTS_CONTEXT: WidgetSegmentsContextValue = {
  onSubmitWidget: async () => {
    console.warn('[WidgetSegmentsBlock] 未接入 onSubmitWidget；提交被忽略')
  },
  onCancelWidget: async () => {
    console.warn('[WidgetSegmentsBlock] 未接入 onCancelWidget；取消被忽略')
  },
}

export const WidgetSegmentsContext = createContext<WidgetSegmentsContextValue>(
  DEFAULT_WIDGET_SEGMENTS_CONTEXT,
)
