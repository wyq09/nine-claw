import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const projectFile = (path: string) => readFileSync(resolve(process.cwd(), path), 'utf8')

describe('global scrollbar styles', () => {
  it('loads scrollbar rules from the global stylesheet entry', () => {
    const indexCss = projectFile('src/index.css')

    expect(indexCss).toContain("@import './styles/global-scrollbars.css';")
  })

  it('uses thin cross-browser scrollbars with theme-aware colors', () => {
    const scrollbarCss = projectFile('src/styles/global-scrollbars.css')

    expect(scrollbarCss).toContain('--global-scrollbar-size: 6px;')
    expect(scrollbarCss).toContain('scrollbar-width: thin;')
    expect(scrollbarCss).toContain('scrollbar-color: var(--global-scrollbar-thumb) var(--global-scrollbar-track);')
    expect(scrollbarCss).toContain('*::-webkit-scrollbar')
    expect(scrollbarCss).toContain('width: var(--global-scrollbar-size);')
    expect(scrollbarCss).toContain('height: var(--global-scrollbar-size);')
    expect(scrollbarCss).toContain('var(--chrome-muted')
  })
})
