import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

const root = process.cwd()

function readProjectFile(path: string) {
  return readFileSync(join(root, path), 'utf8')
}

describe('application log styles', () => {
  it('loads the dedicated log stylesheet after base app styles', () => {
    const appTsx = readProjectFile('src/App.tsx')

    expect(appTsx).toContain("import './styles/application-logs.css'")
    expect(appTsx.indexOf("import './styles/application-logs.css'")).toBeGreaterThan(appTsx.indexOf("import './App.css'"))
  })

  it('uses a sidebar and viewport layout for logs', () => {
    const css = readProjectFile('src/styles/application-logs.css')

    expect(css).toContain('grid-template-columns: minmax(230px, 280px) minmax(0, 1fr);')
    expect(css).toContain('.app-log-sidebar')
    expect(css).toContain('.app-log-viewport')
    expect(css).toContain('.settings-logs-tab')
    expect(css).toContain('height: clamp(300px')
  })
})
