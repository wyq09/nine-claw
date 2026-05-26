import type { SessionWorkspacePreviewKind } from '../../../lib/sessionWorkspaceClient'

const CODE_EXTENSIONS = new Set([
  'php',
  'py',
  'ts',
  'tsx',
  'js',
  'jsx',
  'css',
  'json',
  'rs',
  'go',
  'java',
  'kt',
  'swift',
  'sh',
  'zsh',
  'toml',
  'yaml',
  'yml',
  'xml',
  'sql',
])

export function inferSessionWorkspacePreviewKind(
  fileName: string,
  isDir: boolean,
): SessionWorkspacePreviewKind {
  if (isDir) return 'folder'
  const ext = fileName.split('.').pop()?.toLowerCase() ?? ''
  if (ext === 'md' || ext === 'markdown') return 'markdown'
  if (ext === 'html' || ext === 'htm') return 'html'
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'ico', 'svg'].includes(ext)) return 'image'
  if (CODE_EXTENSIONS.has(ext)) return 'code'
  if (['txt', 'log', 'csv', 'env', 'ini'].includes(ext)) return 'text'
  return 'binary'
}

export function isTextualPreviewKind(kind: SessionWorkspacePreviewKind): boolean {
  return kind === 'markdown' || kind === 'code' || kind === 'html' || kind === 'text'
}

export function formatSessionWorkspaceSize(size: number | null): string {
  if (size == null) return ''
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${(size / 1024 / 1024).toFixed(1)} MB`
}

export function shortenWorkspacePath(path: string): string {
  const normalized = path.replace(/\\/g, '/')
  const parts = normalized.split('/').filter(Boolean)
  if (parts.length <= 3) return path
  return `…/${parts.slice(-3).join('/')}`
}

export function splitCodeLines(content: string): Array<{ n: number; text: string }> {
  const lines = content.split(/\r?\n/)
  return lines.map((text, index) => ({ n: index + 1, text }))
}
