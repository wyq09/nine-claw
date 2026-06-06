import type { ConversationTurn } from '../../types'

const STREAMING_TEXT_BUCKET_SIZE = 48

function bucketTextLength(value: string | undefined): number {
  return value ? Math.floor(value.length / STREAMING_TEXT_BUCKET_SIZE) : 0
}

export function getStreamingTurnLayoutRevision(turn: ConversationTurn | undefined): number {
  if (!turn) {
    return 0
  }

  let revision = bucketTextLength(turn.answer) + bucketTextLength(turn.thinking)
  revision += turn.toolCalls.length * 4
  revision += turn.activity.length * 2

  for (const segment of turn.responseSegments ?? []) {
    revision += segment.type === 'text' ? bucketTextLength(segment.text) : 1
  }

  return revision
}
