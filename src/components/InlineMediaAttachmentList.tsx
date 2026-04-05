import { useLocalMediaPreview } from '../hooks/useLocalMediaPreview'
import { normalizeLocalAssetSource } from '../lib/inlineMedia'
import { openLocalFile } from '../lib/piClient'
import type { InlineMediaAttachment } from '../lib/inlineMedia'

export function InlineMediaAttachmentList({
  attachments,
  onImageClick,
}: {
  attachments: InlineMediaAttachment[]
  onImageClick?: (src: string, alt: string) => void
}) {
  return (
    <div className="inline-media-list">
      {attachments.map((attachment, index) => (
        <InlineMediaAttachmentCard
          key={`${attachment.kind}-${attachment.path || attachment.fileName}-${index}`}
          attachment={attachment}
          onImageClick={onImageClick}
        />
      ))}
    </div>
  )
}

function InlineMediaAttachmentCard({
  attachment,
  onImageClick,
}: {
  attachment: InlineMediaAttachment
  onImageClick?: (src: string, alt: string) => void
}) {
  const title = attachment.fileName || attachment.label
  const normalizedLabel = attachment.label.trim()
  const showSecondaryLabel =
    Boolean(normalizedLabel) &&
    normalizedLabel !== title &&
    normalizedLabel !== '图片' &&
    normalizedLabel !== '视频' &&
    normalizedLabel !== '语音'
  const targetPath = attachment.path.trim()
  const targetSrc = attachment.src.trim()
  const canOpen = Boolean(targetPath || targetSrc)
  const previewSrc = useLocalMediaPreview({
    path: targetPath || targetSrc,
    fallbackSrc: targetSrc || (targetPath ? normalizeLocalAssetSource(targetPath) : ''),
    enabled: attachment.kind !== 'file',
  })
  const canPreview = Boolean(previewSrc)
  const extension = title.split('.').pop()?.trim().toLowerCase() ?? ''
  const typeLabel =
    extension === 'md' || extension === 'markdown'
      ? 'Markdown'
      : extension === 'pdf'
        ? 'PDF'
        : extension === 'doc' || extension === 'docx'
          ? 'Word'
          : extension === 'xls' || extension === 'xlsx' || extension === 'csv'
            ? 'Spreadsheet'
            : extension === 'ppt' || extension === 'pptx'
              ? 'Presentation'
              : extension
                ? extension.toUpperCase()
                : attachment.label

  const handleOpen = () => {
    const target = targetPath || targetSrc
    if (!target) {
      return
    }

    if (targetPath) {
      void openLocalFile(targetPath).catch((error) => {
        console.error('打开附件失败', error)
      })
      return
    }

    if (/^(https?:|data:|asset:|file:\/\/)/i.test(targetSrc)) {
      window.open(targetSrc, '_blank', 'noopener,noreferrer')
      return
    }

    void openLocalFile(target).catch((error) => {
      console.error('打开附件失败', error)
    })
  }

  return (
    <div className={`inline-media-card ${attachment.kind}`}>
      {attachment.kind === 'file' ? (
        <button
          type="button"
          className="inline-media-file-card"
          onClick={handleOpen}
          aria-label={`打开附件 ${title}`}
          title={`打开 ${title}`}
          disabled={!canOpen}
        >
          <span className="inline-media-file-kind">{typeLabel}</span>
          <div className="inline-media-file-copy">
            <strong title={title}>{title}</strong>
          </div>
        </button>
      ) : (
        <div className="inline-media-card-head">
          {showSecondaryLabel ? <span className="inline-media-kicker">{attachment.label}</span> : null}
          <strong title={title}>{title}</strong>
        </div>
      )}
      {attachment.kind === 'image' && canPreview ? (
        <button
          type="button"
          className="inline-media-image-button"
          onClick={() => onImageClick?.(previewSrc, title)}
        >
          <img src={previewSrc} alt={title} className="inline-media-image" />
        </button>
      ) : null}
      {attachment.kind === 'video' ? (
        canPreview ? (
          <video className="inline-media-video" controls preload="metadata" src={previewSrc} />
        ) : (
          <div className="inline-media-empty">视频文件已接收，但当前没有可用路径。</div>
        )
      ) : null}
      {attachment.kind === 'audio' ? (
        canPreview ? (
          <audio className="inline-media-audio" controls preload="metadata" src={previewSrc} />
        ) : (
          <div className="inline-media-empty">语音已接收，但当前没有可用路径。</div>
        )
      ) : null}
      {attachment.kind === 'file' && !canOpen ? <div className="inline-media-empty">文件已接收，但当前没有可用路径。</div> : null}
      {attachment.transcript ? <div className="inline-media-transcript">转写：{attachment.transcript}</div> : null}
    </div>
  )
}
