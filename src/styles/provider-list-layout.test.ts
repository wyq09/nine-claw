import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const projectFile = (path: string) => readFileSync(resolve(process.cwd(), path), 'utf8')

describe('provider list layout styles', () => {
  it('keeps provider cards in a stable horizontal layout with a scrollable list body', () => {
    const css = projectFile('src/App.css')

    expect(css).toContain('.provider-list-items')
    expect(css).toContain('overflow-y: auto;')
    expect(css).toContain('display: flex;')
    expect(css).toContain('align-items: center;')
    expect(css).toContain('justify-content: space-between;')
    expect(css).toContain('flex-shrink: 0;')
    expect(css).toContain('.bot-channel-model')
    expect(css).toContain('text-overflow: ellipsis;')
    expect(css).toContain('.provider-card-end')
    expect(css).toContain('.bot-channel-icon')
    expect(css).toContain('.provider-card-action-button')
    expect(css).toContain('.provider-custom-shortcut')
  })
})
