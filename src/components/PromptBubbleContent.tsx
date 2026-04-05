import { useMemo } from 'react'
import { extractInlineMediaAttachments } from '../lib/inlineMedia'
import { InlineMediaAttachmentList } from './InlineMediaAttachmentList'

export function PromptBubbleContent({
  content,
  onImageClick,
}: {
  content: string
  onImageClick?: (src: string, alt: string) => void
}) {
  const { contentWithoutAttachments, attachments } = useMemo(
    () => extractInlineMediaAttachments(content),
    [content],
  )

  return (
    <div className="prompt-bubble">
      {contentWithoutAttachments ? (
        <div className="prompt-bubble-text">{contentWithoutAttachments}</div>
      ) : null}
      {attachments.length > 0 ? <InlineMediaAttachmentList attachments={attachments} onImageClick={onImageClick} /> : null}
      {!contentWithoutAttachments && attachments.length === 0 ? <div className="prompt-bubble-text"> </div> : null}
    </div>
  )
}
