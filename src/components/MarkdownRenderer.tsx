import { Streamdown } from 'streamdown'
import 'streamdown/styles.css'

type MarkdownRendererProps = {
  content: string
  isStreaming: boolean
}

export default function MarkdownRenderer({ content, isStreaming }: MarkdownRendererProps) {
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
