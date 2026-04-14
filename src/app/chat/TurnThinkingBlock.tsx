import { LazyDetails } from './LazyDetails'

function summarizeThinking(thinking: string): string {
  const trimmed = thinking.trim()
  if (!trimmed) {
    return '思考过程'
  }
  let lineCount = 1
  for (let i = 0; i < thinking.length; i += 1) {
    if (thinking.charCodeAt(i) === 10) {
      lineCount += 1
    }
  }
  return `思考过程 · ${lineCount} 行 · ${thinking.length} 字`
}

export function TurnThinkingBlock({ thinking, isStreaming }: { thinking: string; isStreaming: boolean }) {
  return (
    <LazyDetails
      className="turn-thinking-block"
      summaryClassName="turn-thinking-summary"
      defaultOpen={isStreaming}
      data-streaming={isStreaming ? '' : undefined}
      summary={summarizeThinking(thinking)}
    >
      {(open) => (open ? <pre className="turn-thinking-pre">{thinking}</pre> : null)}
    </LazyDetails>
  )
}
