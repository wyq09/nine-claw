#!/usr/bin/env node
/**
 * 构建产物清理脚本 —— 打包完成后清掉「没必要留存的过程文件」。
 *
 * 背景：一次完整打包（npm run tauri build）会在仓库里留下 ~21GB 过程产物：
 *   - src-tauri/target/            cargo 增量编译缓存（debug ~12G + release ~3G）
 *   - .cache/node-binaries/        prepare-pi-runtime 下载的官方 node 完整二进制（~250M/版本）
 *   - src-tauri/resources/pi-runtime/<platform>/  pi runtime 暂存目录（~378M，构建时重建）
 *   - dist/                        vite 前端产物（tauri.conf.json frontendDist 引用）
 *
 * 设计原则：
 *   1. 默认保守 —— 只删「下次构建一定会重建」的内容，绝不碰源码/配置/最终安装包。
 *   2. 路径白名单 —— 删除目标写死在 collectCleanupTargets 里，不接受任意路径参数。
 *   3. 保护 git 跟踪文件 —— pi-runtime 平台目录里的 pi launcher / .gitkeep 是 git 跟踪的
 *      （AGENTS.md 明确保护 macOS pi launcher），因此这些目录只清空「未跟踪内容」，不删目录本身。
 *   4. --keep 逃生门 —— 默认保留 .app/.dmg 安装包；--keep=no 连安装包一起清。
 *   5. --dry-run —— 只打印将要删除的内容和大小，不动文件系统。
 *
 * 用法：
 *   node scripts/cleanup-build-artifacts.mjs                # 清理过程文件，保留安装包
 *   node scripts/cleanup-build-artifacts.mjs --dry-run      # 预览
 *   node scripts/cleanup-build-artifacts.mjs --keep=no      # 连 .app/.dmg 一起清
 *   npm run cleanup:build / npm run cleanup:build:dry
 */
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

/** 真实实现：列出某目录下 git 跟踪的文件名（用于 pi-runtime 目录保护）。*/
function listTrackedFilesReal(dir) {
  const result = spawnSync('git', ['ls-files', dir], { cwd: repoRoot, encoding: 'utf8' })
  if (result.status !== 0) return []
  return result.stdout
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => path.basename(line))
}

/**
 * 收集要清理的目标。返回 [{ path, kind, reason, preserve, exists, bytes }]。
 * kind: 'dir' 整目录删除；'dir-contents' 只删目录内未跟踪内容（preserve = 要保留的文件名）。
 * fsImpl/listTrackedFiles 仅测试注入用。
 */
export function collectCleanupTargets({
  fsImpl = fs,
  root = repoRoot,
  keepBundles = true,
  listTrackedFiles = listTrackedFilesReal,
} = {}) {
  const targets = []

  const measure = (target) => {
    try {
      target.exists = fsImpl.existsSync(target.path)
      target.bytes = target.exists ? directorySizeSafe(fsImpl, target.path, new Set(target.preserve ?? [])) : 0
    } catch {
      target.exists = false
      target.bytes = 0
    }
    return target
  }

  // 1. cargo 编译缓存（最大头）。cargo build 下次自动重建。整目录删除。
  targets.push(
    measure({
      path: path.join(root, 'src-tauri/target'),
      kind: 'dir',
      preserve: [],
      reason: 'cargo incremental build cache, rebuilt on next build',
    })
  )

  // 2. 官方 node 二进制下载缓存。删了下次构建重新下载。
  targets.push(
    measure({
      path: path.join(root, '.cache/node-binaries'),
      kind: 'dir',
      preserve: [],
      reason: 'downloaded official node binaries, re-downloadable',
    })
  )

  // 3. pi runtime 三平台暂存目录。只清空未跟踪内容 ——
  //    macOS 的 pi launcher 和各平台 .gitkeep 是 git 跟踪的保护文件，必须原地保留。
  for (const platform of ['macos', 'windows', 'linux']) {
    const dirPath = path.join(root, 'src-tauri/resources/pi-runtime', platform)
    targets.push(
      measure({
        path: dirPath,
        kind: 'dir-contents',
        preserve: listTrackedFiles(dirPath),
        reason: 'pi-runtime staging contents, rebuilt by prepare-pi-runtime',
      })
    )
  }

  // 4. 前端产物 dist/（gitignore）。tsc -b && vite build 重建。
  targets.push(
    measure({
      path: path.join(root, 'dist'),
      kind: 'dir',
      preserve: [],
      reason: 'vite build output, rebuilt by tsc -b && vite build',
    })
  )

  if (keepBundles) return targets

  // 5. --keep=no：连最终安装包一起清（CI 验证构建用）。注意它在 target/ 里，
  //    已被第 1 项覆盖，这里显式列出只是让输出语义清楚；重复删除是 no-op。
  targets.push(
    measure({
      path: path.join(root, 'src-tauri/target/release/bundle'),
      kind: 'dir',
      preserve: [],
      reason: 'final .app/.dmg bundles',
    })
  )
  return targets
}

/** 递归统计目录大小，跳过 preserve 中的名字；任何读失败按 0 计（并发删除是常态）。*/
export function directorySizeSafe(fsImpl, absPath, preserve = new Set()) {
  let total = 0
  let entries
  try {
    entries = fsImpl.readdirSync(absPath, { withFileTypes: true })
  } catch {
    return 0
  }
  for (const entry of entries) {
    if (preserve.has(entry.name)) continue
    const full = path.join(absPath, entry.name)
    try {
      if (entry.isDirectory()) total += directorySizeSafe(fsImpl, full)
      else if (entry.isFile()) total += fsImpl.statSync(full).size
    } catch {
      // 被并发删除/权限问题 —— 跳过
    }
  }
  return total
}

/**
 * 把最终安装包目录挪到仓库根的 release-artifacts/（gitignore 约定位置）。
 * 必须在删除 src-tauri/target 之前调用 —— bundle 就住在 target 里，不挪走就会被连带删除。
 * 返回挪到的新路径；bundle 不存在或挪动失败返回 null（失败时保持原位，清理会跳过警告）。
 */
export function relocateBundles(fsImpl, repoRootDir) {
  const bundleDir = path.join(repoRootDir, 'src-tauri/target/release/bundle')
  if (!fsImpl.existsSync(bundleDir)) return null
  const stamp = new Date().toISOString().replace(/[:T]/g, '-').slice(0, 17)
  const dest = path.join(repoRootDir, 'release-artifacts', `bundle-${stamp}`)
  try {
    fsImpl.mkdirSync(path.dirname(dest), { recursive: true })
    fsImpl.renameSync(bundleDir, dest)
    return dest
  } catch {
    return null
  }
}

/** 执行单个目标的删除。dir-contents 只删未跟踪条目。*/
export function removeTarget(fsImpl, target) {
  const rmOpts = { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }
  if (target.kind === 'dir') {
    fsImpl.rmSync(target.path, rmOpts)
    return
  }
  const preserve = new Set(target.preserve ?? [])
  let entries
  try {
    entries = fsImpl.readdirSync(target.path, { withFileTypes: true })
  } catch {
    return // 目录不存在 —— 视为已清理
  }
  for (const entry of entries) {
    if (preserve.has(entry.name)) continue
    // macOS Spotlight 会抢新写入文件的句柄导致 ENOTEMPTY，统一走带重试的删除
    fsImpl.rmSync(path.join(target.path, entry.name), rmOpts)
  }
}

function formatBytes(bytes) {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${(bytes / 1024 ** i).toFixed(1)} ${units[i]}`
}

function main() {
  const args = process.argv.slice(2)
  const dryRun = args.includes('--dry-run')
  const keepBundles = !args.includes('--keep=no')

  if (dryRun) console.log('[cleanup] DRY RUN —— 只统计，不删除\n')

  // 安装包住在 src-tauri/target 里 —— 删 target 前先挪到 release-artifacts/ 保全
  if (keepBundles) {
    const bundleDir = path.join(repoRoot, 'src-tauri/target/release/bundle')
    if (fs.existsSync(bundleDir)) {
      if (dryRun) {
        console.log('[cleanup] would keep  src-tauri/target/release/bundle (挪到 release-artifacts/)\n')
      } else {
        const relocated = relocateBundles(fs, repoRoot)
        console.log(
          relocated
            ? `[cleanup] kept      安装包已挪到 ${path.relative(repoRoot, relocated)}\n`
            : '[cleanup] WARN      安装包挪动失败，target 清理可能将其删除\n'
        )
      }
    }
  }

  let freed = 0
  let removed = 0
  for (const target of collectCleanupTargets({ keepBundles })) {
    const rel = path.relative(repoRoot, target.path)
    if (!target.exists) {
      console.log(`[cleanup] skip (不存在): ${rel}`)
      continue
    }
    const detail =
      target.kind === 'dir-contents' && target.preserve.length > 0
        ? `(保留 ${target.preserve.length} 个 git 跟踪文件)`
        : ''
    console.log(
      `[cleanup] ${dryRun ? 'would clean' : 'cleaning'} ${rel} ${detail}(${formatBytes(target.bytes)}) — ${target.reason}`
    )
    freed += target.bytes
    removed += 1
    if (!dryRun) {
      try {
        removeTarget(fs, target)
      } catch (error) {
        console.warn(`[cleanup] WARN: ${rel} 清理失败: ${error.code ?? error.message}`)
      }
    }
  }

  console.log(`\n[cleanup] ${dryRun ? '可释放' : '已释放'} ${formatBytes(freed)}（${removed} 个目标）`)
  if (!dryRun && removed > 0) {
    console.log('[cleanup] 提示：下次构建会重新下载 node 二进制并全量重编 Rust（首次会明显变慢）')
  }
}

const invokedDirectly = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
if (invokedDirectly) main()
