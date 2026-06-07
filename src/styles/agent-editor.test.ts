import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

const root = process.cwd()

function readProjectFile(path: string) {
  return readFileSync(join(root, path), 'utf8')
}

describe('agent editor styles', () => {
  it('loads the dedicated three-column editor stylesheet after base app styles', () => {
    const appTsx = readProjectFile('src/App.tsx')

    expect(appTsx).toContain("import './App.css'")
    expect(appTsx).toContain("import './styles/agent-editor.css'")
    expect(appTsx.indexOf("import './styles/agent-editor.css'")).toBeGreaterThan(appTsx.indexOf("import './App.css'"))
  })

  it('keeps input and select controls aligned in the editor layout', () => {
    const css = readProjectFile('src/styles/agent-editor.css')

    expect(css).toContain('grid-template-columns: minmax(220px, 270px) minmax(0, 1fr);')
    expect(css).toContain('.agent-editor-config-layout .input-field input,')
    expect(css).toContain('.agent-editor-config-layout .input-field select')
    expect(css).toContain('height: 44px;')
    expect(css).toContain('.agent-search-input-wrap')
  })
})
