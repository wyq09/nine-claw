import { convertFileSrc } from '@tauri-apps/api/core'

export type InlineMediaAttachment = {
  kind: 'image' | 'video' | 'audio' | 'file'
  label: string
  path: string
  src: string
  fileName: string
  transcript?: string
}

function isAbsoluteLocalPath(value: string): boolean {
  return value.startsWith('/') || /^[A-Za-z]:[\\/]/.test(value)
}

export function decodeLocalPathSource(source: string): string {
  try {
    return decodeURIComponent(source)
  } catch {
    return source
  }
}

function isResolvableAssetReference(value: string): boolean {
  const trimmed = value.trim()
  if (!trimmed) {
    return false
  }
  return /^(https?:|data:|asset:|file:\/\/)/i.test(trimmed) || isAbsoluteLocalPath(trimmed)
}

function inferAttachmentKind(value: string, hint = ''): InlineMediaAttachment['kind'] {
  const normalized = `${hint} ${value}`.toLowerCase()
  if (/\b(image|img|photo|picture)\b/.test(normalized) || /\.(png|jpe?g|gif|webp|bmp|svg)(?:[?#].*)?$/.test(normalized)) {
    return 'image'
  }
  if (/\b(video|movie|clip)\b/.test(normalized) || /\.(mp4|webm|mov|m4v|avi|mkv)(?:[?#].*)?$/.test(normalized)) {
    return 'video'
  }
  if (/\b(audio|voice|speech|tts)\b/.test(normalized) || /\.(mp3|wav|m4a|aac|ogg|opus|amr|silk)(?:[?#].*)?$/.test(normalized)) {
    return 'audio'
  }
  return 'file'
}

export function normalizeLocalAssetSource(source: string): string {
  const trimmed = source.trim()
  if (!trimmed) {
    return source
  }

  if (/^(https?:|data:|asset:)/i.test(trimmed)) {
    return trimmed
  }

  if (/^file:\/\//i.test(trimmed)) {
    const filePath = decodeLocalPathSource(trimmed.replace(/^file:\/\//i, ''))
    return convertFileSrc(filePath)
  }

  if (isAbsoluteLocalPath(trimmed)) {
    return convertFileSrc(decodeLocalPathSource(trimmed))
  }

  return trimmed
}

function normalizeMarkdownImageSource(source: string): string {
  return normalizeLocalAssetSource(source)
}

export function normalizeMarkdownImageSources(content: string): string {
  return content
    .replace(/!\[([^\]]*)\]\(([^)\s]+)(\s+"[^"]*")?\)/g, (_match, alt: string, src: string, title = '') => {
      return `![${alt}](${normalizeMarkdownImageSource(src)}${title})`
    })
    .replace(/<img([^>]*?)src=(['"])(.*?)\2([^>]*)>/gi, (_match, before: string, quote: string, src: string, after: string) => {
      return `<img${before}src=${quote}${normalizeMarkdownImageSource(src)}${quote}${after}>`
    })
}

export function getPathFileName(path: string): string {
  const normalized = path.replace(/[?#].*$/, '').trim()
  if (!normalized) {
    return ''
  }
  const parts = normalized.split(/[\\/]/)
  return parts[parts.length - 1] ?? ''
}

function parseStandaloneMarkdownMediaLine(line: string): InlineMediaAttachment | null {
  const match = line.trim().match(/^\[([^\]]+)\]\(([^)]+)\)$/)
  if (!match) {
    return null
  }

  const [, label, rawPath] = match
  const path = decodeLocalPathSource(rawPath.trim())
  if (!path || !isResolvableAssetReference(path)) {
    return null
  }

  const kind = inferAttachmentKind(path, label)

  return {
    kind,
    label: label.trim() || getPathFileName(path) || '附件',
    path,
    src: normalizeLocalAssetSource(path),
    fileName: getPathFileName(path) || 'attachment',
  }
}

function parseDirectiveMediaToken(token: string): InlineMediaAttachment | null {
  const trimmed = token.trim()
  if (!trimmed.startsWith('::nc-media{') || !trimmed.endsWith('}')) {
    return null
  }

  const body = trimmed.slice('::nc-media{'.length, -1)
  let rawType = ''
  let rawPath = ''
  let rawName = ''
  let rawLabel = ''
  for (const match of body.matchAll(/(\w+)=("([^"]*)"|'([^']*)'|([^\s]+))/g)) {
    const key = match[1]
    const normalized = (match[3] ?? match[4] ?? match[5] ?? '').trim()
    if (key === 'type') {
      rawType = normalized
    } else if (key === 'path') {
      rawPath = decodeLocalPathSource(normalized)
    } else if (key === 'name') {
      rawName = decodeLocalPathSource(normalized)
    } else if (key === 'label') {
      rawLabel = decodeLocalPathSource(normalized)
    }
  }

  const salvageCandidates = [rawPath, rawName, rawLabel, decodeLocalPathSource(trimmed)].filter(Boolean)
  for (const candidate of salvageCandidates) {
    for (const nestedMatch of candidate.matchAll(/::nc-media\{[^}]*\}/g)) {
      const nestedDirective = nestedMatch[0]?.trim()
      if (!nestedDirective || nestedDirective === trimmed) {
        continue
      }
      const nestedAttachment = parseDirectiveMediaToken(nestedDirective)
      if (nestedAttachment) {
        return nestedAttachment
      }
    }
  }

  if (!rawPath || !isResolvableAssetReference(rawPath)) {
    return null
  }

  const kind = inferAttachmentKind(rawPath, rawType)
  const fileName = rawName || getPathFileName(rawPath) || 'attachment'
  const label =
    rawLabel ||
    (kind === 'image'
      ? '图片'
      : kind === 'video'
        ? '视频'
        : kind === 'audio'
          ? '语音'
          : '文件')

  return {
    kind,
    label,
    path: rawPath,
    src: normalizeLocalAssetSource(rawPath),
    fileName,
  }
}

function parseDirectiveMediaLine(line: string): InlineMediaAttachment | null {
  return parseDirectiveMediaToken(line)
}

function parseInboundAttachmentLine(line: string): InlineMediaAttachment | null {
  const trimmed = line.trim()
  const transcriptMarker = ' | 转写: '
  const transcriptIndex = trimmed.indexOf(transcriptMarker)
  const transcript =
    transcriptIndex >= 0 ? trimmed.slice(transcriptIndex + transcriptMarker.length).trim() : undefined
  const main = transcriptIndex >= 0 ? trimmed.slice(0, transcriptIndex).trim() : trimmed
  const match = main.match(/^\[收到(图片|视频|语音|文件)\](?:\s+(.+))?$/)
  if (!match) {
    return null
  }

  const [, kindLabel, rawPath = ''] = match
  const kind =
    kindLabel === '图片'
      ? 'image'
      : kindLabel === '视频'
        ? 'video'
        : kindLabel === '语音'
          ? 'audio'
          : 'file'
  const path = decodeLocalPathSource(rawPath.trim())

  return {
    kind,
    label: kindLabel,
    path,
    src: path ? normalizeLocalAssetSource(path) : '',
    fileName: getPathFileName(path) || `${kindLabel}附件`,
    ...(transcript ? { transcript } : {}),
  }
}

function extractAttachmentsFromJsonValue(
  value: unknown,
  pathHint = '',
  seen = new Set<string>(),
): InlineMediaAttachment[] {
  if (typeof value === 'string') {
    const candidate = decodeLocalPathSource(value.trim())
    if (!isResolvableAssetReference(candidate) || seen.has(candidate)) {
      return []
    }
    seen.add(candidate)
    const kind = inferAttachmentKind(candidate, pathHint)
    return [
      {
        kind,
        label:
          kind === 'image'
            ? '图片'
            : kind === 'video'
              ? '视频'
              : kind === 'audio'
                ? '语音'
                : '文件',
        path: candidate,
        src: normalizeLocalAssetSource(candidate),
        fileName: getPathFileName(candidate) || 'attachment',
      },
    ]
  }

  if (Array.isArray(value)) {
    return value.flatMap((item, index) => extractAttachmentsFromJsonValue(item, `${pathHint}[${index}]`, seen))
  }

  if (typeof value === 'object' && value !== null) {
    return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
      extractAttachmentsFromJsonValue(child, pathHint ? `${pathHint}.${key}` : key, seen),
    )
  }

  return []
}

export function extractInlineMediaAttachments(content: string): {
  contentWithoutAttachments: string
  attachments: InlineMediaAttachment[]
} {
  const attachments: InlineMediaAttachment[] = []
  const seen = new Set<string>()
  const textLines: string[] = []

  const pushAttachment = (attachment: InlineMediaAttachment) => {
    if (!attachment.path || !seen.has(attachment.path)) {
      if (attachment.path) {
        seen.add(attachment.path)
      }
      attachments.push(attachment)
    }
  }

  for (const line of content.split('\n')) {
    const lineWithoutInlineDirectives = line.replace(/::nc-media\{[^}]*\}/g, (match) => {
      const attachment = parseDirectiveMediaToken(match)
      if (attachment) {
        pushAttachment(attachment)
        return ' '
      }
      return match
    })

    const attachment =
      parseInboundAttachmentLine(lineWithoutInlineDirectives) ??
      parseDirectiveMediaLine(lineWithoutInlineDirectives) ??
      parseStandaloneMarkdownMediaLine(lineWithoutInlineDirectives)
    if (attachment) {
      pushAttachment(attachment)
      continue
    }

    if (lineWithoutInlineDirectives.trim()) {
      textLines.push(lineWithoutInlineDirectives)
    } else if (line.trim() === '') {
      textLines.push('')
    }
  }

  try {
    const parsed = JSON.parse(content)
    for (const attachment of extractAttachmentsFromJsonValue(parsed, '', seen)) {
      attachments.push(attachment)
    }
  } catch {
    // Ignore non-JSON content.
  }

  return {
    contentWithoutAttachments: textLines.join('\n').trim(),
    attachments,
  }
}

/** 是否包含「文件」类附件（与打开/下载/复制同一工具行） */
export function turnHasInlineFileAttachment(turn: {
  answer: string
  responseSegments?: Array<{ type: string; text?: string }>
}): boolean {
  const hasFile = (text: string) =>
    extractInlineMediaAttachments(text).attachments.some((a) => a.kind === 'file')
  if (hasFile(turn.answer || '')) {
    return true
  }
  const segments = turn.responseSegments
  if (!segments) {
    return false
  }
  for (const seg of segments) {
    if (seg.type === 'text' && seg.text && hasFile(seg.text)) {
      return true
    }
  }
  return false
}

/** 判断本轮回复正文中是否包含可展示的附件（用于与 Token 等元信息同一行排版） */
export function turnHasInlineAttachments(turn: {
  answer: string
  responseSegments?: Array<{ type: string; text?: string }>
}): boolean {
  if (extractInlineMediaAttachments(turn.answer || '').attachments.length > 0) {
    return true
  }
  const segments = turn.responseSegments
  if (!segments) {
    return false
  }
  for (const seg of segments) {
    if (seg.type === 'text' && seg.text && extractInlineMediaAttachments(seg.text).attachments.length > 0) {
      return true
    }
  }
  return false
}
