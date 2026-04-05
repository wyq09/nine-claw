import type { ChatAttachmentKind, ChatAttachmentUpload, PersistedChatAttachment } from '../types'
import { decodeLocalPathSource, getPathFileName } from './inlineMedia'

function encodeAttachmentPath(path: string): string {
  return encodeURI(path)
}

function escapeDirectiveValue(value: string): string {
  return encodeURI(value)
}

function getAttachmentDirectiveLabel(kind: PersistedChatAttachment['kind']): string {
  return kind === 'image' ? '图片' : kind === 'video' ? '视频' : kind === 'audio' ? '语音' : '文件'
}

export function buildPromptWithAttachments(
  prompt: string,
  attachments: PersistedChatAttachment[],
): string {
  if (attachments.length === 0) {
    return prompt
  }

  const attachmentLines = attachments.map((attachment) => {
    const encodedPath = encodeAttachmentPath(attachment.filePath)
    const encodedName = escapeDirectiveValue(attachment.fileName || '附件')
    return `::nc-media{type="${attachment.kind}" path="${encodedPath}" name="${encodedName}" label="${getAttachmentDirectiveLabel(attachment.kind)}"}`
  })
  const trimmedPrompt = prompt.trim()
  if (!trimmedPrompt) {
    return attachmentLines.join('\n')
  }

  return `${trimmedPrompt}\n\n${attachmentLines.join('\n')}`.trim()
}

export function inferAttachmentKindFromReference(reference: string, mimeType = ''): ChatAttachmentKind {
  const normalized = `${reference.toLowerCase()} ${mimeType.toLowerCase()}`
  if (normalized.includes('image/') || /\.(png|jpe?g|gif|webp|bmp|svg)(?:[?#].*)?$/.test(normalized)) {
    return 'image'
  }
  if (normalized.includes('video/') || /\.(mp4|webm|mov|m4v|avi|mkv)(?:[?#].*)?$/.test(normalized)) {
    return 'video'
  }
  if (normalized.includes('audio/') || /\.(mp3|wav|m4a|aac|ogg|opus|amr|silk)(?:[?#].*)?$/.test(normalized)) {
    return 'audio'
  }
  return 'file'
}

function inferClipboardFileName(type: string): string {
  const extension =
    type === 'image/png'
      ? 'png'
      : type === 'image/jpeg'
        ? 'jpg'
        : type === 'image/webp'
          ? 'webp'
          : type === 'video/mp4'
            ? 'mp4'
            : type === 'audio/mpeg'
              ? 'mp3'
              : 'bin'
  return `pasted-${Date.now()}.${extension}`
}

function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer)
  const chunkSize = 0x8000
  let binary = ''

  for (let index = 0; index < bytes.length; index += chunkSize) {
    const chunk = bytes.subarray(index, index + chunkSize)
    binary += String.fromCharCode(...chunk)
  }

  return btoa(binary)
}

export async function buildAttachmentUploadFromFile(file: File): Promise<ChatAttachmentUpload> {
  const dataBase64 = arrayBufferToBase64(await file.arrayBuffer())
  return {
    fileName: file.name?.trim() || inferClipboardFileName(file.type),
    mimeType: file.type || null,
    dataBase64,
  }
}

function normalizeClipboardPathCandidate(value: string): string | null {
  const trimmed = value.trim()
  if (!trimmed) {
    return null
  }

  if (/^file:\/\//i.test(trimmed)) {
    return decodeLocalPathSource(trimmed.replace(/^file:\/\//i, ''))
  }

  if (trimmed.startsWith('/') || /^[A-Za-z]:[\\/]/.test(trimmed)) {
    return decodeLocalPathSource(trimmed)
  }

  return null
}

function parsePathLines(raw: string): string[] {
  const lines = raw
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
  if (lines.length === 0) {
    return []
  }

  const normalized = lines
    .map(normalizeClipboardPathCandidate)
    .filter((value): value is string => Boolean(value))

  return normalized.length === lines.length ? normalized : []
}

export function extractClipboardFilePathCandidates(dataTransfer: DataTransfer | null): string[] {
  if (!dataTransfer) {
    return []
  }

  const candidates = [
    ...parsePathLines(dataTransfer.getData('text/uri-list') || ''),
    ...parsePathLines(dataTransfer.getData('text/plain') || ''),
  ]

  return [...new Set(candidates)]
}

export function buildClipboardPathUpload(path: string): ChatAttachmentUpload {
  return {
    fileName: getPathFileName(path) || 'attachment',
    sourcePath: path,
    mimeType: null,
  }
}
