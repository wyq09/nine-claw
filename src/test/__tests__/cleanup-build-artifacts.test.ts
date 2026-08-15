import { describe, expect, it, vi } from 'vitest'

// 测试目标：cleanup-build-artifacts.mjs 的白名单逻辑、git 跟踪文件保护与 keepBundles 分支。
// 通过 fsImpl / listTrackedFiles 注入 mock，不触碰真实仓库目录。
// 被测模块顶层只在直接执行时跑 main()，import 无副作用。

type Dirent = { name: string; isDirectory: () => boolean; isFile: () => boolean }

function mockFs({
  existing = new Set<string>(),
  files = [] as Array<[string, number]>,
  dirs = new Set<string>(),
} = {}) {
  const existsSync = (p: string) => existing.has(p)
  const readdirSync = (dir: string): Dirent[] => {
    const fileNames = new Set<string>()
    const dirNames = new Set<string>()
    for (const [f] of files) if (f.startsWith(dir + '/')) fileNames.add(f.slice(dir.length + 1).split('/')[0])
    for (const d of dirs) if (d.startsWith(dir + '/')) dirNames.add(d.slice(dir.length + 1).split('/')[0])
    for (const n of fileNames) dirNames.delete(n) // 同名时文件优先（如无扩展名的 pi）
    if (fileNames.size + dirNames.size === 0) throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' })
    return [
      ...[...fileNames].map((name) => ({ name, isDirectory: () => false, isFile: () => true })),
      ...[...dirNames].map((name) => ({ name, isDirectory: () => true, isFile: () => false })),
    ]
  }
  const statSync = (p: string) => {
    const hit = files.find(([f]) => f === p)
    if (!hit) throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' })
    return { size: hit[1] }
  }
  const rmSync = vi.fn()
  const renameSync = vi.fn((from: string, to: string) => {
    existing.delete(from)
    existing.add(to)
  })
  const mkdirSync = vi.fn()
  return { existsSync, readdirSync, statSync, rmSync, renameSync, mkdirSync }
}

const ROOT = '/fake/repo'
const noTracked = () => [] as string[]

// 被测模块（.mjs）的 fsImpl 形参默认值为完整 node:fs，tsc 据此推断入参类型；
// mock 只实现用到的子集，调用处统一用该断言收窄。
const asFs = (m: ReturnType<typeof mockFs>) => m as unknown as typeof import('node:fs')

describe('collectCleanupTargets', () => {
  it('returns the fixed whitelist of process-artifact dirs', async () => {
    const { collectCleanupTargets } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    const targets = collectCleanupTargets({ fsImpl: asFs(mockFs()), root: ROOT, listTrackedFiles: noTracked })
    const rels = targets.map((t) => t.path.replace(ROOT + '/', ''))
    expect(rels).toEqual([
      'src-tauri/target',
      '.cache/node-binaries',
      'src-tauri/resources/pi-runtime/macos',
      'src-tauri/resources/pi-runtime/windows',
      'src-tauri/resources/pi-runtime/linux',
      'dist',
    ])
    // 白名单绝不能包含安装包目录（默认 keep bundles）
    expect(rels.some((r) => r.includes('bundle'))).toBe(false)
  })

  it('marks pi-runtime platform dirs as dir-contents and wires tracked files into preserve', async () => {
    const { collectCleanupTargets } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    const targets = collectCleanupTargets({
      fsImpl: asFs(mockFs()),
      root: ROOT,
      listTrackedFiles: (dir: string) => (dir.endsWith('macos') ? ['pi'] : ['.gitkeep']),
    })
    const macos = targets.find((t) => t.path.endsWith('pi-runtime/macos'))
    const linux = targets.find((t) => t.path.endsWith('pi-runtime/linux'))
    expect(macos?.kind).toBe('dir-contents')
    expect(macos?.preserve).toEqual(['pi']) // AGENTS.md 保护文件必须被保留
    expect(linux?.kind).toBe('dir-contents')
    expect(linux?.preserve).toEqual(['.gitkeep'])
    // 全量删除目标（target/.cache/dist）不保留任何东西
    expect(targets.find((t) => t.path.endsWith('src-tauri/target'))?.kind).toBe('dir')
  })

  it('keeps final bundles by default and removes them with keepBundles=false', async () => {
    const { collectCleanupTargets } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    const fsMock = asFs(mockFs())
    const kept = collectCleanupTargets({ fsImpl: fsMock, root: ROOT, listTrackedFiles: noTracked })
    const purged = collectCleanupTargets({ fsImpl: fsMock, root: ROOT, keepBundles: false, listTrackedFiles: noTracked })
    expect(kept.some((t) => t.path.includes('release/bundle'))).toBe(false)
    expect(purged.some((t) => t.path.includes('release/bundle'))).toBe(true)
  })

  it('marks missing dirs as exists=false and zero bytes', async () => {
    const { collectCleanupTargets } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    const targets = collectCleanupTargets({ fsImpl: asFs(mockFs()), root: ROOT, listTrackedFiles: noTracked })
    expect(targets.length).toBeGreaterThan(0)
    for (const t of targets) {
      expect(t.exists).toBe(false)
      expect(t.bytes).toBe(0)
    }
  })
})

describe('directorySizeSafe', () => {
  it('aggregates file bytes and skips preserved names', async () => {
    const { directorySizeSafe } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    const fsMock = mockFs({
      files: [
        ['/fake/repo/d/pi', 7000], // tracked launcher —— 应被跳过
        ['/fake/repo/d/node_modules', 100], // 无扩展名按目录处理，但内部无文件 → 0
        ['/fake/repo/d/index.html', 100],
        ['/fake/repo/d/assets.js', 50],
      ],
    })
    expect(directorySizeSafe(asFs(fsMock), '/fake/repo/d', new Set(['pi']))).toBe(250)
    // 不传 preserve 时全部计入
    expect(directorySizeSafe(asFs(fsMock), '/fake/repo/d')).toBe(7250)
  })

  it('returns 0 on unreadable dir instead of throwing', async () => {
    const { directorySizeSafe } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    expect(directorySizeSafe(asFs(mockFs()), '/nope')).toBe(0)
  })
})

describe('removeTarget', () => {
  it('removes the whole dir for kind=dir', async () => {
    const { removeTarget } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    const fsMock = mockFs()
    removeTarget(asFs(fsMock), { path: '/fake/repo/dist', kind: 'dir', preserve: [] as string[] })
    expect(fsMock.rmSync).toHaveBeenCalledWith(
      '/fake/repo/dist',
      { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }
    )
  })

  it('deletes only untracked entries for kind=dir-contents', async () => {
    const { removeTarget } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    const fsMock = mockFs({
      files: [
        ['/fake/repo/macos/pi', 7000],
        ['/fake/repo/macos/.DS_Store', 6148],
      ],
      dirs: new Set(['/fake/repo/macos/node_modules']),
    })
    removeTarget(asFs(fsMock), { path: '/fake/repo/macos', kind: 'dir-contents', preserve: ['pi'] })
    const removedPaths = fsMock.rmSync.mock.calls.map((c) => c[0] as string)
    expect(removedPaths).toContain('/fake/repo/macos/.DS_Store')
    expect(removedPaths).toContain('/fake/repo/macos/node_modules')
    expect(removedPaths).not.toContain('/fake/repo/macos/pi') // 保护文件绝不能删
  })

  it('is a no-op when the dir-contents dir no longer exists', async () => {
    const { removeTarget } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    const fsMock = mockFs()
    expect(() =>
      removeTarget(asFs(fsMock), { path: '/gone', kind: 'dir-contents', preserve: [] as string[] })
    ).not.toThrow()
    expect(fsMock.rmSync).not.toHaveBeenCalled()
  })
})

describe('relocateBundles', () => {
  const BUNDLE = `${ROOT}/src-tauri/target/release/bundle`
  const withFS = (existing: string[]) =>
    mockFs({ existing: new Set(existing), dirs: new Set([BUNDLE]) })

  it('moves the bundle dir into release-artifacts/ and returns the new path', async () => {
    const { relocateBundles } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    const fsMock = withFS([BUNDLE])
    const dest = relocateBundles(asFs(fsMock), ROOT)
    expect(dest).toBeTruthy()
    expect(String(dest)).toContain('release-artifacts/bundle-')
    const [from, to] = fsMock.renameSync.mock.calls[0]
    expect(from).toBe(BUNDLE)
    expect(String(to)).toContain('release-artifacts')
  })

  it('returns null when there is no bundle to keep', async () => {
    const { relocateBundles } = await import('../../../scripts/cleanup-build-artifacts.mjs')
    expect(relocateBundles(asFs(withFS([])), ROOT)).toBeNull()
    expect(withFS([]).renameSync).not.toHaveBeenCalled()
  })
})
