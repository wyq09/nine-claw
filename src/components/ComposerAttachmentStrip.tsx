import { X } from 'lucide-react'
import { useLocalMediaPreview } from '../hooks/useLocalMediaPreview'
import { normalizeLocalAssetSource } from '../lib/inlineMedia'
import type { PersistedChatAttachment } from '../types'

const KIND_LABELS: Record<PersistedChatAttachment['kind'], string> = {
  image: '图片',
  video: '视频',
  audio: '语音',
  file: '文件',
}

export function ComposerAttachmentStrip({
  attachments,
  uploading,
  onRemove,
  onClear,
}: {
  attachments: PersistedChatAttachment[]
  uploading: boolean
  onRemove: (attachmentId: string) => void
  onClear: () => void
}) {
  if (attachments.length === 0) {
    return null
  }

  return (
    <div className="composer-attachment-strip">
      <div className="composer-attachment-strip-head">
        <span>已附带 {attachments.length} 个附件</span>
        <button type="button" className="link-button" onClick={onClear} disabled={uploading}>
          清空
        </button>
      </div>
      <div className="composer-attachment-list">
        {attachments.map((attachment) => (
          <ComposerAttachmentChip
            key={attachment.id}
            attachment={attachment}
            uploading={uploading}
            onRemove={onRemove}
          />
        ))}
      </div>
    </div>
  )
}

function ComposerAttachmentChip({
  attachment,
  uploading,
  onRemove,
}: {
  attachment: PersistedChatAttachment
  uploading: boolean
  onRemove: (attachmentId: string) => void
}) {
  const previewSrc = useLocalMediaPreview({
    path: attachment.filePath,
    mimeType: attachment.mimeType,
    fallbackSrc: normalizeLocalAssetSource(attachment.filePath),
    enabled: attachment.kind === 'image',
  })

  return (
    <div className={`composer-attachment-chip ${attachment.kind}`}>
      {attachment.kind === 'image' ? (
        <img className="composer-attachment-thumb" src={previewSrc} alt={attachment.fileName} />
      ) : (
        <span className="composer-attachment-kind">{KIND_LABELS[attachment.kind]}</span>
      )}
      <span className="composer-attachment-name" title={attachment.fileName}>
        {attachment.fileName}
      </span>
      <button
        type="button"
        className="composer-attachment-remove"
        aria-label={`移除附件 ${attachment.fileName}`}
        onClick={() => onRemove(attachment.id)}
        disabled={uploading}
      >
        <X size={14} />
      </button>
    </div>
  )
}
