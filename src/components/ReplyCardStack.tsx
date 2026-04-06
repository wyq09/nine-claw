import { lazy, Suspense, type MouseEvent } from 'react'
import type { ReplyCardItem } from '../lib/replyCardFormat'

const MarkdownRenderer = lazy(() => import('./MarkdownRenderer'))

function MarkdownFallback({ content }: { content: string }) {
  return <div className="markdown-content-fallback">{content || ' '}</div>
}

type ReplyCardStackProps = {
  items: ReplyCardItem[]
  isStreaming: boolean
  onImageClick?: (src: string, alt: string) => void
}

export default function ReplyCardStack({ items, isStreaming, onImageClick }: ReplyCardStackProps) {
  const lastStreamingIndex = isStreaming ? Math.max(0, items.length - 1) : -1

  const handleClick = (event: MouseEvent<HTMLDivElement>) => {
    if (!onImageClick) {
      return
    }
    const target = event.target
    if (!(target instanceof HTMLElement)) {
      return
    }
    const image = target.closest('img')
    if (!(image instanceof HTMLImageElement) || !image.src) {
      return
    }
    event.preventDefault()
    onImageClick(image.src, image.alt)
  }

  if (items.length === 0) {
    return null
  }

  return (
    <div className="reply-card-stack">
      {items.map((item, index) => {
        const tone = item.tone && item.tone !== 'default' ? item.tone : ''
        const toneClass = tone ? ` reply-card-tone-${tone}` : ''
        const streamThis = index === lastStreamingIndex
        return (
          <div key={`reply-card-${index}`} className={`reply-card${toneClass}`}>
            {item.title ? <div className="reply-card-title">{item.title}</div> : null}
            <div className="reply-card-body markdown-content" onClick={handleClick}>
              <Suspense fallback={<MarkdownFallback content={item.body} />}>
                <MarkdownRenderer content={item.body} isStreaming={streamThis} />
              </Suspense>
            </div>
          </div>
        )
      })}
    </div>
  )
}
