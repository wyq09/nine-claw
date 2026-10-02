import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

/**
 * Tauri v2 事件名只允许字母数字与 `-` `/` `:` `_`（`.` 非法）。
 * 曾有 `session.llm_log.updated` / `workspace.llm_trace` 两个带点事件名，
 * emit 与 listen 双侧都被拒，实时刷新从未送达前端。此测试扫描全部
 * `listen('...')` 字面量，防止再次引入非法事件名。
 */
const VALID_EVENT_NAME = /^[A-Za-z0-9\-/:_]+$/

function walkSourceFiles(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name)
    if (statSync(full).isDirectory()) {
      if (name === '__tests__' || name === 'node_modules') continue
      walkSourceFiles(full, out)
    } else if (/\.(ts|tsx)$/.test(name)) {
      out.push(full)
    }
  }
  return out
}

function extractListenEventNames(source: string): string[] {
  const names: string[] = []
  // listen('name' / listen<Payload>('name' / listen<Payload<'x'>>('name'
  const pattern = /\blisten(?:<[^>(]*>)?\(\s*['"]([^'"]+)['"]/g
  let match: RegExpExecArray | null
  while ((match = pattern.exec(source)) !== null) {
    names.push(match[1])
  }
  return names
}

describe('tauri event names', () => {
  it('all listen() event names are valid for Tauri v2 (no dots)', () => {
    const files = walkSourceFiles(join(process.cwd(), 'src'))
    expect(files.length).toBeGreaterThan(50)

    const invalid: Array<{ file: string; name: string }> = []
    let checked = 0
    for (const file of files) {
      for (const name of extractListenEventNames(readFileSync(file, 'utf8'))) {
        checked += 1
        if (!VALID_EVENT_NAME.test(name)) {
          invalid.push({ file: file.replace(process.cwd() + '/', ''), name })
        }
      }
    }
    expect(checked).toBeGreaterThan(10)
    expect(invalid).toEqual([])
  })
})
