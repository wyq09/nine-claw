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
  const canPreview = Boolean(attachment.src)

  return (
    <div className={`inline-media-card ${attachment.kind}`}>
      <div className="inline-media-card-head">
        <span className="inline-media-kicker">{attachment.label}</span>
        <strong>{title}</strong>
      </div>
      {attachment.kind === 'image' && canPreview ? (
        <button
          type="button"
          className="inline-media-image-button"
          onClick={() => onImageClick?.(attachment.src, title)}
        >
          <img src={attachment.src} alt={title} className="inline-media-image" />
        </button>
      ) : null}
      {attachment.kind === 'video' ? (
        canPreview ? (
          <video className="inline-media-video" controls preload="metadata" src={attachment.src} />
        ) : (
          <div className="inline-media-empty">视频文件已接收，但当前没有可用路径。</div>
        )
      ) : null}
      {attachment.kind === 'audio' ? (
        canPreview ? (
          <audio className="inline-media-audio" controls preload="metadata" src={attachment.src} />
        ) : (
          <div className="inline-media-empty">语音已接收，但当前没有可用路径。</div>
        )
      ) : null}
      {attachment.kind === 'file' ? (
        canPreview ? (
          <a
            className="inline-media-file-link"
            href={attachment.src}
            target="_blank"
            rel="noreferrer"
            download={attachment.fileName}
          >
            打开文件
          </a>
        ) : (
          <div className="inline-media-empty">文件已接收，但当前没有可用路径。</div>
        )
      ) : null}
      {attachment.path ? <div className="inline-media-path">{attachment.path}</div> : null}
      {attachment.transcript ? <div className="inline-media-transcript">转写：{attachment.transcript}</div> : null}
    </div>
  )
}
