import type { ResponseSegment } from '../../types'
import type { WidgetSegment } from '../../widgetTypes'

export function appendOrReplaceWidgetSegment(
  segments: ResponseSegment[] | undefined,
  nextWidget: WidgetSegment,
): ResponseSegment[] {
  const current = segments ?? []
  let replaced = false
  const next = current.map((segment) => {
    if (
      segment.type === 'widget' &&
      segment.widget.widgetId === nextWidget.widget.widgetId
    ) {
      replaced = true
      return nextWidget
    }
    return segment
  })

  return replaced ? next : [...next, nextWidget]
}
