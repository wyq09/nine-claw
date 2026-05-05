import { useContext } from 'react'
import type { WidgetSegment } from '../../widgetTypes'
import { AskUserCard } from './AskUserCard'
import { WidgetSegmentsContext } from './WidgetSegmentsContext'

export type WidgetSegmentsBlockProps = {
  segments: WidgetSegment[]
  turnId: string
}

export function WidgetSegmentsBlock({ segments, turnId }: WidgetSegmentsBlockProps) {
  const ctx = useContext(WidgetSegmentsContext)
  if (segments.length === 0) {
    return null
  }

  return (
    <div className="widget-segments-block" data-turn-id={turnId}>
      {segments.map((segment, index) => {
        if (segment.widget.kind !== 'ask_user') {
          return null
        }
        return (
          <AskUserCard
            key={`${segment.widget.widgetId}-${index}`}
            widget={segment.widget}
            onSubmit={ctx.onSubmitWidget}
            onCancel={ctx.onCancelWidget}
          />
        )
      })}
    </div>
  )
}
