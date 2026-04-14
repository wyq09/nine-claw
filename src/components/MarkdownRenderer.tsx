import { Streamdown } from 'streamdown'
import 'streamdown/styles.css'

type MarkdownRendererProps = {
  content: string
  isStreaming: boolean
}

const STREAMING_MARKDOWN_FALLBACK_CHAR_THRESHOLD = 12_000
const STREAMING_MARKDOWN_FALLBACK_LINE_THRESHOLD = 220

function shouldUsePlainTextFallback(content: string, isStreaming: boolean): boolean {
  if (!isStreaming) {
    return false
  }
  if (content.length >= STREAMING_MARKDOWN_FALLBACK_CHAR_THRESHOLD) {
    return true
  }
  let lineCount = 1
  for (let i = 0; i < content.length; i += 1) {
    if (content.charCodeAt(i) === 10) {
      lineCount += 1
      if (lineCount >= STREAMING_MARKDOWN_FALLBACK_LINE_THRESHOLD) {
        return true
      }
    }
  }
  return false
}

export default function MarkdownRenderer({ content, isStreaming }: MarkdownRendererProps) {
  if (shouldUsePlainTextFallback(content, isStreaming)) {
    return <div className="markdown-content-fallback">{content}</div>
  }

  return (
    <Streamdown
      mode={isStreaming ? 'streaming' : 'static'}
      isAnimating={isStreaming}
      linkSafety={{ enabled: false }}
    >
      {content}
    </Streamdown>
  )
}
