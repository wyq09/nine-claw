import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const projectFile = (path: string) => readFileSync(resolve(process.cwd(), path), 'utf8')

describe('history sidebar menu styles', () => {
  it('keeps group menus theme-aware instead of hard-coded green colors', () => {
    const css = projectFile('src/App.css')

    expect(css).toContain('.history-context-menu {')
    expect(css).toContain('border: 1px solid color-mix(in srgb, rgba(var(--accent-rgb), 0.42) 68%, var(--chrome-border));')
    expect(css).toContain('background:')
    expect(css).toContain('color-mix(in srgb, var(--surface-elevated, var(--surface-base)) 92%, rgba(var(--accent-rgb), 1) 8%);')
    expect(css).toContain('.history-context-menu-item:hover {')
    expect(css).toContain('background: rgba(var(--accent-rgb), 0.12);')
    expect(css).toContain('.history-context-submenu-panel {')
    expect(css).toContain('color-mix(in srgb, var(--surface-elevated, var(--surface-base)) 94%, rgba(var(--accent-rgb), 1) 6%);')
  })
})
