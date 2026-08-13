#!/usr/bin/env node
/**
 * Mechanical 800-line guard for hand-written source.
 *
 * The repository already contains files over the limit; those are recorded in
 * scripts/file-size-baseline.json as a ratchet. This script fails when:
 *   - a file over 800 lines is not in the baseline, or
 *   - a baselined file grows beyond its recorded line count.
 *
 * Refresh the baseline deliberately with:
 *   node scripts/check-file-size.mjs --update-baseline
 */
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(__dirname, '..')
const limit = 800
const baselinePath = path.join(__dirname, 'file-size-baseline.json')

const groups = [
  {
    label: 'frontend',
    roots: [path.join(root, 'src')],
    extensions: new Set(['.ts', '.tsx', '.css']),
  },
  {
    label: 'rust',
    roots: [path.join(root, 'src-tauri', 'src')],
    extensions: new Set(['.rs']),
  },
]

function walk(dir, extensions, output) {
  if (!fs.existsSync(dir)) return
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      walk(fullPath, extensions, output)
    } else if (extensions.has(path.extname(entry.name))) {
      output.push(fullPath)
    }
  }
}

function collectFiles() {
  const files = []
  for (const group of groups) {
    for (const dir of group.roots) walk(dir, group.extensions, files)
  }
  return files
}

function rel(file) {
  return path.relative(root, file).split(path.sep).join('/')
}

function lineCounts(files) {
  const result = new Map()
  for (const file of files) {
    result.set(file, fs.readFileSync(file, 'utf8').split('\n').length)
  }
  return result
}

function loadBaseline() {
  try {
    const value = JSON.parse(fs.readFileSync(baselinePath, 'utf8'))
    return value?.files ?? {}
  } catch {
    return {}
  }
}

function updateBaseline(counts) {
  const files = {}
  for (const [file, count] of [...counts.entries()].sort(([a], [b]) => rel(a).localeCompare(rel(b)))) {
    if (count > limit) files[rel(file)] = count
  }
  const payload = {
    generatedBy: 'scripts/check-file-size.mjs --update-baseline',
    limit,
    note: 'Ratchet baseline for pre-existing files over the 800-line hand-written source limit. Split files before raising these values.',
    files,
  }
  fs.writeFileSync(baselinePath, `${JSON.stringify(payload, null, 2)}\n`)
  return files
}

function main() {
  const update = process.argv.includes('--update-baseline')
  const files = collectFiles()
  const counts = lineCounts(files)

  if (update) {
    const files = updateBaseline(counts)
    console.log(`Updated baseline with ${Object.keys(files).length} files over ${limit} lines.`)
    return
  }

  const baseline = loadBaseline()
  const failures = []
  for (const [file, count] of [...counts.entries()].sort(([a], [b]) => rel(a).localeCompare(rel(b)))) {
    if (count <= limit) continue
    const key = rel(file)
    const baselineCount = baseline[key]
    if (baselineCount === undefined) {
      failures.push(`${key}: ${count} lines (new file over ${limit}-line limit)`)
    } else if (count > baselineCount) {
      failures.push(`${key}: ${count} lines (baseline was ${baselineCount}; split before extending the ratchet)`)
    }
  }

  if (failures.length > 0) {
    console.error(`800-line guard failed with ${failures.length} violation(s):`)
    for (const failure of failures) console.error(`  - ${failure}`)
    process.exitCode = 1
    return
  }

  console.log(`800-line guard passed (${files.length} files scanned, limit ${limit}).`)
}

main()
