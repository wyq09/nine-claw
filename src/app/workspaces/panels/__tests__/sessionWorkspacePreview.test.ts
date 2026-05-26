import { describe, expect, it } from 'vitest'
import {
  formatSessionWorkspaceSize,
  inferSessionWorkspacePreviewKind,
  isTextualPreviewKind,
  shortenWorkspacePath,
  splitCodeLines,
} from '../sessionWorkspacePreview'

describe('sessionWorkspacePreview', () => {
  it('classifies common preview types', () => {
    expect(inferSessionWorkspacePreviewKind('README.md', false)).toBe('markdown')
    expect(inferSessionWorkspacePreviewKind('index.html', false)).toBe('html')
    expect(inferSessionWorkspacePreviewKind('script.ts', false)).toBe('code')
    expect(inferSessionWorkspacePreviewKind('photo.png', false)).toBe('image')
    expect(inferSessionWorkspacePreviewKind('notes.txt', false)).toBe('text')
    expect(inferSessionWorkspacePreviewKind('archive.zip', false)).toBe('binary')
    expect(inferSessionWorkspacePreviewKind('src', true)).toBe('folder')
  })

  it('detects textual preview kinds', () => {
    expect(isTextualPreviewKind('markdown')).toBe(true)
    expect(isTextualPreviewKind('code')).toBe(true)
    expect(isTextualPreviewKind('image')).toBe(false)
    expect(isTextualPreviewKind('binary')).toBe(false)
  })

  it('formats sizes and code lines', () => {
    expect(formatSessionWorkspaceSize(null)).toBe('')
    expect(formatSessionWorkspaceSize(32)).toBe('32 B')
    expect(formatSessionWorkspaceSize(2048)).toBe('2.0 KB')
    expect(splitCodeLines('a\nb')).toEqual([
      { n: 1, text: 'a' },
      { n: 2, text: 'b' },
    ])
  })

  it('shortens long paths from the tail', () => {
    expect(shortenWorkspacePath('/Users/me/.nineclaw/workspace/chat-sessions/s1')).toBe(
      '…/workspace/chat-sessions/s1',
    )
    expect(shortenWorkspacePath('/tmp/a')).toBe('/tmp/a')
  })
})
