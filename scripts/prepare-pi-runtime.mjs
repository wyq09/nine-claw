import fs from 'node:fs'
import path from 'node:path'
import { spawnSync } from 'node:child_process'

const runtimePackageName = '@earendil-works/pi-coding-agent'
const extraRuntimePackageNames = ['@modelcontextprotocol/sdk']
const publishedMonoPackageNames = [
  '@earendil-works/pi-coding-agent',
  '@earendil-works/pi-agent-core',
  '@earendil-works/pi-ai',
  '@earendil-works/pi-tui',
  '@earendil-works/pi-web-ui',
]
const repoOnlyMonoPackageNames = ['@earendil-works/pi-pods']
const projectRoot = process.cwd()
const localNodeModulesRoot = path.resolve(projectRoot, 'node_modules')
const resourceRoot = path.resolve(projectRoot, 'src-tauri', 'resources', 'pi-runtime')
const archiveRoot = path.resolve(projectRoot, 'src-tauri', 'resources', 'pi-runtime-bundles')
const readmePath = path.join(resourceRoot, 'README.md')
let cachedGlobalNpmRoot = null

function resolvePlatformDir(value) {
  const normalized = (value || process.platform).toLowerCase()
  if (normalized === 'win32' || normalized === 'windows') return 'windows'
  if (normalized === 'darwin' || normalized === 'macos') return 'macos'
  return 'linux'
}

function runCommand(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8' })
  if (result.status !== 0) {
    const detail = result.stderr?.trim() || result.stdout?.trim() || `exit ${result.status ?? 'unknown'}`
    throw new Error(`${command} ${args.join(' ')} failed: ${detail}`)
  }
  return result.stdout.trim()
}

function copyRecursive(sourcePath, targetPath) {
  fs.rmSync(targetPath, { recursive: true, force: true })
  fs.mkdirSync(path.dirname(targetPath), { recursive: true })
  // Dereference symlinks so the bundled runtime does not keep absolute links
  // back to the build machine's global npm install.
  fs.cpSync(sourcePath, targetPath, { recursive: true, dereference: true })
}

function copyEntry(sourcePath, targetPath) {
  fs.rmSync(targetPath, { recursive: true, force: true })
  fs.mkdirSync(path.dirname(targetPath), { recursive: true })
  const stats = fs.statSync(sourcePath)
  if (stats.isDirectory()) {
    fs.cpSync(sourcePath, targetPath, { recursive: true, dereference: true })
    return
  }
  fs.copyFileSync(sourcePath, targetPath)
}

function rewritePackageSymlinks(targetRoot, sourceRoot) {
  const stack = [targetRoot]
  while (stack.length > 0) {
    const current = stack.pop()
    if (!current || !fs.existsSync(current)) continue

    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const targetPath = path.join(current, entry.name)
      if (entry.isDirectory()) {
        stack.push(targetPath)
        continue
      }
      if (!entry.isSymbolicLink()) {
        continue
      }

      const rawTarget = fs.readlinkSync(targetPath)
      const resolvedTarget = path.isAbsolute(rawTarget)
        ? rawTarget.startsWith(sourceRoot)
          ? path.join(targetRoot, path.relative(sourceRoot, rawTarget))
          : rawTarget
        : path.resolve(path.dirname(targetPath), rawTarget)

      fs.rmSync(targetPath, { force: true })
      if (!fs.existsSync(resolvedTarget)) {
        continue
      }

      const stats = fs.statSync(resolvedTarget)
      if (stats.isDirectory()) {
        fs.cpSync(resolvedTarget, targetPath, { recursive: true, dereference: true })
      } else {
        fs.copyFileSync(resolvedTarget, targetPath)
        if ((stats.mode & 0o111) !== 0) {
          fs.chmodSync(targetPath, 0o755)
        }
      }
    }
  }
}

function resolveRealPathIfExists(candidate) {
  if (!candidate) return null
  if (!fs.existsSync(candidate)) return null
  return fs.realpathSync(candidate)
}

function readJsonFile(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'))
}

function packagePathFromNodeModules(rootPath, packageName) {
  return path.join(rootPath, ...packageName.split('/'))
}

function resolveGlobalNpmRoot() {
  if (!cachedGlobalNpmRoot) {
    cachedGlobalNpmRoot = runCommand('npm', ['root', '-g'])
  }
  return cachedGlobalNpmRoot
}

function resolveInstalledPackageDir(packageName, options = {}) {
  const roots = []
  if (options.localFirst !== false) {
    roots.push(localNodeModulesRoot)
  }
  if (options.includeGlobal !== false) {
    roots.push(resolveGlobalNpmRoot())
  }

  for (const rootPath of roots) {
    const candidate = packagePathFromNodeModules(rootPath, packageName)
    if (fs.existsSync(candidate)) {
      return fs.realpathSync(candidate)
    }
  }

  return null
}

function copyBundledLibraries(platformDir, sourceExecutable, targetDir) {
  if (platformDir === 'macos') {
    return copyMacLibraries(sourceExecutable, targetDir)
  }
  if (platformDir === 'linux') {
    return copyLinuxLibraries(sourceExecutable, targetDir)
  }
  return []
}

function copyMacLibraries(sourceExecutable, targetDir) {
  const libDir = path.join(targetDir, 'lib')
  const queue = [fs.realpathSync(sourceExecutable)]
  const visited = new Set()
  const copied = new Set()
  const aliasPaths = new Set()

  while (queue.length > 0) {
    const current = queue.pop()
    if (!current || visited.has(current)) continue
    visited.add(current)

    const output = runCommand('otool', ['-L', current])
    for (const line of output.split('\n').slice(1)) {
      const rawDependency = line.trim().split(' ')[0]
      if (!rawDependency) continue
      if (rawDependency.startsWith('/usr/lib/') || rawDependency.startsWith('/System/')) continue

      let resolvedDependency = null
      if (rawDependency.startsWith('@rpath/')) {
        resolvedDependency = resolveRealPathIfExists(
          path.resolve(path.dirname(current), '..', 'lib', path.basename(rawDependency)),
        )
      } else if (rawDependency.startsWith('@loader_path/')) {
        resolvedDependency = resolveRealPathIfExists(
          path.resolve(path.dirname(current), rawDependency.replace('@loader_path/', '')),
        )
      } else if (rawDependency.startsWith('@executable_path/')) {
        resolvedDependency = resolveRealPathIfExists(
          path.resolve(path.dirname(sourceExecutable), rawDependency.replace('@executable_path/', '')),
        )
      } else if (rawDependency.startsWith('/')) {
        resolvedDependency = resolveRealPathIfExists(rawDependency)
      }

      if (!resolvedDependency) continue

      fs.mkdirSync(libDir, { recursive: true })
      const targetPath = path.join(libDir, path.basename(resolvedDependency))
      if (!copied.has(resolvedDependency)) {
        fs.copyFileSync(resolvedDependency, targetPath)
        fs.chmodSync(targetPath, 0o755)
        copied.add(resolvedDependency)
        queue.push(resolvedDependency)
      }

      const aliasName = path.basename(rawDependency)
      if (aliasName && aliasName !== path.basename(resolvedDependency)) {
        const aliasPath = path.join(libDir, aliasName)
        if (!aliasPaths.has(aliasPath) && !fs.existsSync(aliasPath)) {
          fs.symlinkSync(path.basename(resolvedDependency), aliasPath)
          aliasPaths.add(aliasPath)
        }
      }
    }
  }

  return Array.from(copied)
}

function listMacDependencies(binaryPath) {
  const output = runCommand('otool', ['-L', binaryPath])
  return output
    .split('\n')
    .slice(1)
    .map((line) => line.trim().split(' ')[0])
    .filter(Boolean)
}

function listMacRpaths(binaryPath) {
  const output = runCommand('otool', ['-l', binaryPath])
  const lines = output.split('\n')
  const rpaths = []

  for (let index = 0; index < lines.length; index += 1) {
    if (!lines[index]?.includes('cmd LC_RPATH')) continue
    const pathLine = lines
      .slice(index, index + 6)
      .find((line) => line.trim().startsWith('path '))
    if (!pathLine) continue
    const match = pathLine.trim().match(/^path\s+(.+?)\s+\(offset/)
    if (match?.[1]) {
      rpaths.push(match[1])
    }
  }

  return rpaths
}

function rewriteMacBinary(binaryPath, libDir) {
  const bundledLibTargets = new Map()
  for (const entry of fs.readdirSync(libDir, { withFileTypes: true })) {
    if (!entry.isFile() && !entry.isSymbolicLink()) continue
    bundledLibTargets.set(entry.name, path.join(libDir, entry.name))
  }

  const binaryDir = path.dirname(binaryPath)
  const binaryInLibDir = binaryDir === libDir

  for (const rawDependency of listMacDependencies(binaryPath)) {
    if (rawDependency.startsWith('/usr/lib/') || rawDependency.startsWith('/System/')) {
      continue
    }

    let targetName = null
    if (rawDependency.startsWith('@rpath/')) {
      const rpathName = path.basename(rawDependency)
      if (bundledLibTargets.has(rpathName)) {
        targetName = rpathName
      }
    } else if (rawDependency.startsWith('@loader_path/') || rawDependency.startsWith('@executable_path/')) {
      const loaderName = path.basename(rawDependency)
      if (bundledLibTargets.has(loaderName)) {
        targetName = loaderName
      }
    } else if (rawDependency.startsWith('/')) {
      const resolvedDependency = resolveRealPathIfExists(rawDependency)
      const resolvedName = resolvedDependency ? path.basename(resolvedDependency) : null
      const rawName = path.basename(rawDependency)
      if (rawName && bundledLibTargets.has(rawName)) {
        targetName = rawName
      } else if (resolvedName && bundledLibTargets.has(resolvedName)) {
        targetName = resolvedName
      }
    }

    if (!targetName) continue

    const rewrittenPath = binaryInLibDir
      ? `@loader_path/${targetName}`
      : `@loader_path/lib/${targetName}`
    runCommand('install_name_tool', ['-change', rawDependency, rewrittenPath, binaryPath])
  }

  if (binaryInLibDir) {
    runCommand('install_name_tool', ['-id', `@loader_path/${path.basename(binaryPath)}`, binaryPath])
    return
  }

  const desiredRpath = '@loader_path/lib'
  if (!listMacRpaths(binaryPath).includes(desiredRpath)) {
    runCommand('install_name_tool', ['-add_rpath', desiredRpath, binaryPath])
  }

  const legacyRpath = '@loader_path/../lib'
  if (listMacRpaths(binaryPath).includes(legacyRpath)) {
    runCommand('install_name_tool', ['-delete_rpath', legacyRpath, binaryPath])
  }
}

function finalizeMacRuntimeBundle(targetDir, nodeTargetPath) {
  const libDir = path.join(targetDir, 'lib')
  if (!fs.existsSync(libDir)) {
    return
  }

  for (const entry of fs.readdirSync(libDir, { withFileTypes: true })) {
    if (!entry.isFile()) continue
    rewriteMacBinary(path.join(libDir, entry.name), libDir)
  }
  for (const entry of fs.readdirSync(libDir, { withFileTypes: true })) {
    if (!entry.isFile()) continue
    signMacBinary(path.join(libDir, entry.name))
  }
  rewriteMacBinary(nodeTargetPath, libDir)
  signMacBinary(nodeTargetPath)
}

function signMacBinary(binaryPath) {
  runCommand('codesign', ['--force', '--sign', '-', '--timestamp=none', binaryPath])
}

function copyLinuxLibraries(sourceExecutable, targetDir) {
  const libDir = path.join(targetDir, 'lib')
  const output = runCommand('ldd', [sourceExecutable])
  const copied = []

  for (const line of output.split('\n')) {
    const match = line.match(/=>\s+(\/[^\s]+)\s+\(/)
    if (!match) continue
    const libraryPath = match[1]
    if (libraryPath.startsWith('/lib/') || libraryPath.startsWith('/usr/lib/')) {
      continue
    }
    const resolvedPath = resolveRealPathIfExists(libraryPath)
    if (!resolvedPath) continue

    fs.mkdirSync(libDir, { recursive: true })
    const targetPath = path.join(libDir, path.basename(resolvedPath))
    fs.copyFileSync(resolvedPath, targetPath)
    fs.chmodSync(targetPath, 0o755)
    copied.push(resolvedPath)
  }

  return copied
}

function cleanPlatformDir(platformDir) {
  const targetDir = path.join(resourceRoot, platformDir)
  fs.mkdirSync(targetDir, { recursive: true })
  for (const entry of fs.readdirSync(targetDir)) {
    const fullPath = path.join(targetDir, entry)
    // maxRetries: on macOS, Spotlight/backup scanners briefly hold freshly written
    // runtime files, which makes rmSync fail with ENOTEMPTY/EBUSY on first attempt.
    fs.rmSync(fullPath, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
  }
  return targetDir
}

function ensureArchiveRoot() {
  fs.mkdirSync(archiveRoot, { recursive: true })
}

function removeMacExtendedAttributes(targetPath) {
  if (process.platform !== 'darwin') return
  spawnSync('xattr', ['-cr', targetPath], { stdio: 'ignore' })
}

function createRuntimeArchiveWithPython(platformDir, archivePath) {
  const script = `
import os
import tarfile

resource_root = ${JSON.stringify(resourceRoot)}
archive_path = ${JSON.stringify(archivePath)}
platform_dir = ${JSON.stringify(platformDir)}
source_path = os.path.join(resource_root, platform_dir)

with tarfile.open(archive_path, "w:gz", dereference=True) as archive:
    archive.add(source_path, arcname=platform_dir, recursive=True)
`
  const result = spawnSync('python3', ['-c', script], { encoding: 'utf8' })
  if (result.status !== 0) {
    const detail = result.stderr?.trim() || result.stdout?.trim() || `exit ${result.status ?? 'unknown'}`
    throw new Error(`python3 tarfile fallback failed: ${detail}`)
  }
}

function createRuntimeArchive(platformDir) {
  ensureArchiveRoot()
  const archivePath = path.join(archiveRoot, `${platformDir}.tar.gz`)
  fs.rmSync(archivePath, { force: true })
  let result = spawnSync(
    'tar',
    ['-czf', archivePath, '-C', resourceRoot, platformDir],
    { encoding: 'utf8' },
  )
  if (result.status !== 0) {
    const detail = result.stderr?.trim() || result.stdout?.trim() || `exit ${result.status ?? 'unknown'}`
    console.warn(`[prepare-pi-runtime] system tar failed, falling back to python3 tarfile: ${detail}`)
    fs.rmSync(archivePath, { force: true })
    createRuntimeArchiveWithPython(platformDir, archivePath)
  }
  removeMacExtendedAttributes(archivePath)
  return archivePath
}

function resolveRuntimePackageDir() {
  const envPath = process.env.PI_RUNTIME_PACKAGE_DIR?.trim()
  if (envPath) {
    const candidate = path.resolve(envPath)
    if (!fs.existsSync(candidate)) {
      throw new Error(`PI_RUNTIME_PACKAGE_DIR does not exist: ${candidate}`)
    }
    return candidate
  }

  const candidate = resolveInstalledPackageDir(runtimePackageName)
  if (!candidate) {
    throw new Error(
      `Runtime package not found: ${runtimePackageName}. Run npm install or set PI_RUNTIME_PACKAGE_DIR.`,
    )
  }
  return candidate
}

function resolveNodeExecutable(platformDir) {
  const envPath = process.env.PI_RUNTIME_NODE_PATH?.trim()
  if (envPath) {
    const executable = path.resolve(envPath)
    if (!fs.existsSync(executable)) {
      throw new Error(`Node executable not found: ${executable}`)
    }
    return executable
  }

  // Try to download an official, properly-signed Node.js binary.
  // On macOS the bundled thin node (68KB @loader_path wrapper) gets SIGKILL'd
  // by SIP/AMFI, so we need a real node from nodejs.org.
  const officialNode = downloadOfficialNode(platformDir)
  if (officialNode) return officialNode

  // Fallback: use the node running this build script
  console.warn('[prepare-pi-runtime] Falling back to current process node (may not work on macOS)')
  const executable = fs.realpathSync(process.execPath)
  if (!fs.existsSync(executable)) {
    throw new Error(`Node executable not found: ${executable}`)
  }
  return executable
}

// ---------------------------------------------------------------------------
// Official Node.js binary download
// ---------------------------------------------------------------------------

const NODE_DIST_BASE = 'https://nodejs.org/dist'

function getNodePlatformTriplet(platformDir) {
  if (platformDir === 'macos') return `darwin-${process.arch}`
  if (platformDir === 'windows') return 'win-x64'
  return `linux-${process.arch}`
}

function getNodeVersion() {
  return process.version.replace(/^v/, '') // e.g. "22.18.0"
}

function officialNodeCacheDir() {
  return path.join(projectRoot, '.cache', 'node-binaries')
}

function downloadOfficialNode(platformDir) {
  const version = getNodeVersion()
  const triplet = getNodePlatformTriplet(platformDir)
  const nodeExeName = platformDir === 'windows' ? 'node.exe' : 'node'
  const cacheKey = `node-v${version}-${triplet}`
  const cachedExePath = path.join(officialNodeCacheDir(), cacheKey, 'bin', nodeExeName)
  // Windows zip layout: node-v22.x.x-win-x64/node.exe
  const cachedWinExePath = path.join(officialNodeCacheDir(), cacheKey, nodeExeName)

  if (fs.existsSync(cachedExePath)) {
    console.log(`[prepare-pi-runtime] Using cached official Node.js v${version} (${triplet})`)
    return cachedExePath
  }
  if (platformDir === 'windows' && fs.existsSync(cachedWinExePath)) {
    console.log(`[prepare-pi-runtime] Using cached official Node.js v${version} (${triplet})`)
    return cachedWinExePath
  }

  const archiveExt = platformDir === 'windows' ? 'zip' : 'tar.gz'
  const archiveName = `${cacheKey}.${archiveExt}`
  const url = `${NODE_DIST_BASE}/v${version}/${archiveName}`

  console.log(`[prepare-pi-runtime] Downloading official Node.js v${version} (${triplet})...`)
  console.log(`[prepare-pi-runtime] URL: ${url}`)

  const tmpDir = path.join(officialNodeCacheDir(), '.tmp', cacheKey)
  fs.rmSync(tmpDir, { recursive: true, force: true })
  fs.mkdirSync(tmpDir, { recursive: true })

  const archivePath = path.join(tmpDir, archiveName)

  // Download (retry up to 3 times)
  let curlResult = null
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    // Remove stale partial download before retry
    if (fs.existsSync(archivePath)) fs.rmSync(archivePath, { force: true })
    curlResult = spawnSync('curl', [
      '--fail', '--location', '--progress-bar',
      '--output', archivePath,
      '--retry', '2',
      '--connect-timeout', '30',
      url,
    ], { encoding: 'utf8', stdio: ['pipe', 'inherit', 'inherit'] })
    if (curlResult.status === 0) break
    console.warn(`[prepare-pi-runtime] Download attempt ${attempt} failed (exit ${curlResult.status})`)
  }

  if (curlResult.status !== 0 || !fs.existsSync(archivePath)) {
    console.warn(`[prepare-pi-runtime] Failed to download official Node.js: curl exit ${curlResult.status}`)
    fs.rmSync(tmpDir, { recursive: true, force: true })
    return null
  }

  // Extract
  const extractDir = path.join(officialNodeCacheDir(), cacheKey)
  fs.rmSync(extractDir, { recursive: true, force: true })
  fs.mkdirSync(extractDir, { recursive: true })

  if (platformDir === 'windows') {
    // Use python to extract zip (available on macOS/Linux for cross-build)
    const script = `
import zipfile, sys
with zipfile.ZipFile(${JSON.stringify(archivePath)}) as zf:
    zf.extractall(${JSON.stringify(extractDir)})
`
    const pyResult = spawnSync('python3', ['-c', script], { encoding: 'utf8' })
    if (pyResult.status !== 0) {
      console.warn('[prepare-pi-runtime] Failed to extract Node.js zip')
      fs.rmSync(tmpDir, { recursive: true, force: true })
      return null
    }
  } else {
    const tarResult = spawnSync('tar', ['-xzf', archivePath, '-C', extractDir], { encoding: 'utf8' })
    if (tarResult.status !== 0) {
      console.warn('[prepare-pi-runtime] Failed to extract Node.js tarball')
      fs.rmSync(tmpDir, { recursive: true, force: true })
      return null
    }
  }

  // Cleanup temp download
  fs.rmSync(tmpDir, { recursive: true, force: true })

  // Verify extraction — tar creates a versioned subdirectory
  let exePath = platformDir === 'windows' ? cachedWinExePath : cachedExePath
  if (!fs.existsSync(exePath)) {
    // tar archives contain a top-level versioned directory, e.g. node-v22.18.0-darwin-arm64/
    const versionedDir = path.join(extractDir, cacheKey)
    const altPath = path.join(versionedDir, 'bin', nodeExeName)
    const altWinPath = path.join(versionedDir, nodeExeName)
    if (fs.existsSync(altPath)) {
      exePath = altPath
    } else if (platformDir === 'windows' && fs.existsSync(altWinPath)) {
      exePath = altWinPath
    } else {
      console.warn(`[prepare-pi-runtime] Extracted node binary not found at ${exePath}`)
      return null
    }
  }

  // macOS: ad-hoc sign the binary so SIP/AMFI doesn't kill it
  if (platformDir === 'macos') {
    try {
      signMacBinary(exePath)
    } catch (error) {
      console.warn(`[prepare-pi-runtime] Warning: could not sign downloaded node: ${error.message}`)
    }
  }

  console.log(`[prepare-pi-runtime] Official Node.js v${version} (${triplet}) ready at ${exePath}`)
  return exePath
}

/**
 * npm hoists most dependencies to the top-level node_modules/, leaving only
 * a subset in the package's own node_modules/.  When we copy the package
 * directory, hoisted deps are missing.  This function reads the package's
 * declared dependencies and copies any that exist in the top-level
 * node_modules/ but are absent from the target bundle.
 */
/**
 * Recursively copy all dependencies (including transitive) that are hoisted
 * to the top-level node_modules/ and missing from the target bundle.
 *
 * npm hoists most packages to the project root node_modules/. When we copy
 * just the package directory, hoisted deps (and their deps, etc.) are missing.
 * This walks the full dependency graph and copies anything not yet present.
 */
function copyHoistedDependencies(sourcePackageDir, targetPackageDir) {
  const targetNodeModules = path.join(targetPackageDir, 'node_modules')
  fs.mkdirSync(targetNodeModules, { recursive: true })

  const copied = new Set()
  const queue = []

  // Seed: direct dependencies of the root package
  const rootPkgJson = readPackageJson(sourcePackageDir)
  if (rootPkgJson) {
    enqueueDeps(queue, rootPkgJson, sourcePackageDir)
  }

  while (queue.length > 0) {
    const { depName, contextDir } = queue.shift()
    if (copied.has(depName)) continue

    const targetDepPath = path.join(targetNodeModules, depName)
    if (fs.existsSync(targetDepPath)) {
      // Already present (came with the package or copied earlier) — still
      // need to process its dependencies in case they're also hoisted.
      const existingPkgJson = readPackageJson(targetDepPath)
      if (existingPkgJson) {
        enqueueDeps(queue, existingPkgJson, targetDepPath)
      }
      copied.add(depName)
      continue
    }

    // Find the dep in the build machine's node_modules hierarchy
    const sourceDepPath = resolveInstalledPackageDir(depName, { localFirst: true, includeGlobal: false })
    if (!sourceDepPath) {
      // Some deps are optional or platform-specific — don't fail the build
      continue
    }

    copyRecursive(sourceDepPath, targetDepPath)
    rewritePackageSymlinks(targetDepPath, sourceDepPath)
    copied.add(depName)

    // Enqueue this package's own dependencies for processing
    const depPkgJson = readPackageJson(sourceDepPath)
    if (depPkgJson) {
      enqueueDeps(queue, depPkgJson, sourceDepPath)
    }
  }

  if (copied.size > 0) {
    console.log(`[prepare-pi-runtime] Copied ${copied.size} deps (including transitive)`)
  }
}

function copyRuntimePackageToNodeModules(packageName, targetNodeModulesRoot) {
  const sourceDir = resolveInstalledPackageDir(packageName, { localFirst: true, includeGlobal: false })
  if (!sourceDir) {
    throw new Error(`Runtime support package not found: ${packageName}. Run npm install first.`)
  }

  const targetPath = packagePathFromNodeModules(targetNodeModulesRoot, packageName)
  copyRecursive(sourceDir, targetPath)
  rewritePackageSymlinks(targetPath, sourceDir)
  copyHoistedDependencies(sourceDir, targetPath)
}

function readPackageJson(pkgDir) {
  const p = path.join(pkgDir, 'package.json')
  if (!fs.existsSync(p)) return null
  return JSON.parse(fs.readFileSync(p, 'utf8'))
}

function enqueueDeps(queue, pkgJson, contextDir) {
  const depFields = ['dependencies', 'optionalDependencies']
  for (const field of depFields) {
    const deps = pkgJson[field]
    if (!deps) continue
    for (const depName of Object.keys(deps)) {
      queue.push({ depName, contextDir })
    }
  }
}

function writeLauncher(platformDir, targetDir) {
  if (platformDir === 'windows') {
    const launcherPath = path.join(targetDir, 'pi.cmd')
    const content = [
      '@echo off',
      'setlocal',
      'set SCRIPT_DIR=%~dp0',
      '"%SCRIPT_DIR%node.exe" "%SCRIPT_DIR%pi-package\\dist\\cli.js" %*',
      '',
    ].join('\r\n')
    fs.writeFileSync(launcherPath, content)
    return launcherPath
  }

  const launcherPath = path.join(targetDir, 'pi')
  // IMPORTANT: On macOS, the bundled thin node (68KB @loader_path wrapper) is
  // killed by SIP/AMFI because its dylibs are ad-hoc signed.  We MUST skip
  // SCRIPT_DIR when searching PATH for node, so the system-installed node is
  // preferred.  Do NOT simplify this to `command -v node` — see AGENTS.md.
  const content = [
    '#!/bin/sh',
    '# Bundled pi launcher generated during build.',
    'set -eu',
    'SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"',
    'NODE_BIN="$SCRIPT_DIR/node"',
    'if [ "$(uname -s)" = "Darwin" ]; then',
    '  # On macOS the bundled thin node launcher may be blocked by SIP / AMFI',
    '  # when its @loader_path dylibs are unsigned.  Prefer a system-installed',
    '  # node if one exists (search PATH *without* SCRIPT_DIR so the bundled',
    '  # node is not found ahead of the real one).',
    '  _saved_IFS="$IFS"',
    '  IFS=\':\'',
    '  _found=',
    '  for _dir in $PATH; do',
    '    case "$_dir" in',
    '      "$SCRIPT_DIR") continue ;;',
    '    esac',
    '    if [ -x "$_dir/node" ]; then',
    '      _found="$_dir/node"',
    '      break',
    '    fi',
    '  done',
    '  IFS="$_saved_IFS"',
    '  if [ -n "$_found" ]; then',
    '    NODE_BIN="$_found"',
    '  fi',
    'fi',
    'if [ -d "$SCRIPT_DIR/lib" ]; then',
    '  export DYLD_LIBRARY_PATH="$SCRIPT_DIR/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"',
    '  export LD_LIBRARY_PATH="$SCRIPT_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"',
    'fi',
    'exec "$NODE_BIN" "$SCRIPT_DIR/pi-package/dist/cli.js" "$@"',
    '',
  ].join('\n')
  fs.writeFileSync(launcherPath, content)
  fs.chmodSync(launcherPath, 0o755)
  return launcherPath
}

function writeManifest(targetDir, payload) {
  fs.writeFileSync(path.join(targetDir, 'manifest.json'), JSON.stringify(payload, null, 2))
}

function resolveMonoRepoDir() {
  const envPath = process.env.PI_MONO_REPO_DIR?.trim()
  if (!envPath) {
    return null
  }

  const candidate = path.resolve(envPath)
  if (!fs.existsSync(candidate)) {
    throw new Error(`PI_MONO_REPO_DIR does not exist: ${candidate}`)
  }
  const packageJsonPath = path.join(candidate, 'package.json')
  if (!fs.existsSync(packageJsonPath)) {
    throw new Error(`PI_MONO_REPO_DIR is missing package.json: ${candidate}`)
  }
  return candidate
}

function listRepoPackages(repoDir) {
  const packagesDir = path.join(repoDir, 'packages')
  if (!fs.existsSync(packagesDir)) {
    return []
  }

  const result = []
  for (const entry of fs.readdirSync(packagesDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue
    const packageDir = path.join(packagesDir, entry.name)
    const packageJsonPath = path.join(packageDir, 'package.json')
    if (!fs.existsSync(packageJsonPath)) continue

    const packageJson = readJsonFile(packageJsonPath)
    result.push({
      name: packageJson.name ?? entry.name,
      version: packageJson.version ?? null,
      path: path.relative(repoDir, packageDir),
    })
  }

  return result.sort((left, right) => left.name.localeCompare(right.name))
}

function writeMonoBundleReadme(targetPath, lines) {
  fs.writeFileSync(targetPath, `${lines.join('\n')}\n`)
}

function stageRepoMonoBundle(targetDir, repoDir) {
  const monoTargetDir = path.join(targetDir, 'pi-mono')
  fs.rmSync(monoTargetDir, { recursive: true, force: true })
  fs.mkdirSync(monoTargetDir, { recursive: true })

  const rootEntries = [
    'README.md',
    'AGENTS.md',
    'CONTRIBUTING.md',
    'LICENSE',
    'package.json',
    'package-lock.json',
    'biome.json',
    'tsconfig.json',
    'tsconfig.base.json',
    '.pi',
    'docs',
    'examples',
    'scripts',
    'packages',
  ]

  for (const entry of rootEntries) {
    const sourcePath = path.join(repoDir, entry)
    if (!fs.existsSync(sourcePath)) continue
    copyEntry(sourcePath, path.join(monoTargetDir, entry))
  }

  const packages = listRepoPackages(repoDir)
  writeMonoBundleReadme(path.join(monoTargetDir, 'BUNDLE_README.md'), [
    '# Pi Mono Bundle',
    '',
    'This directory is a snapshot of a pi-mono repository used for runtime bundling.',
    `Source: ${repoDir}`,
    '',
    'It is staged so the desktop client ships not only the CLI runtime entrypoint,',
    'but also the broader pi-mono package tree and its docs/scripts assets.',
  ])

  return {
    mode: 'repo-snapshot',
    source: repoDir,
    packages,
    missingPackages: [],
  }
}

function stageInstalledMonoBundle(targetDir) {
  const monoTargetDir = path.join(targetDir, 'pi-mono')
  const monoNodeModulesDir = path.join(monoTargetDir, 'node_modules')
  fs.rmSync(monoTargetDir, { recursive: true, force: true })
  fs.mkdirSync(monoNodeModulesDir, { recursive: true })

  const packages = []
  const missingPackages = []

  for (const packageName of publishedMonoPackageNames) {
    const sourceDir = resolveInstalledPackageDir(packageName)
    if (!sourceDir) {
      missingPackages.push(packageName)
      continue
    }

    const targetPath = packagePathFromNodeModules(monoNodeModulesDir, packageName)
    copyRecursive(sourceDir, targetPath)
    rewritePackageSymlinks(targetPath, sourceDir)

    const packageJson = readJsonFile(path.join(sourceDir, 'package.json'))
    packages.push({
      name: packageJson.name ?? packageName,
      version: packageJson.version ?? null,
      path: path.relative(monoTargetDir, targetPath),
      source: sourceDir,
    })
  }

  missingPackages.push(...repoOnlyMonoPackageNames)
  writeMonoBundleReadme(path.join(monoTargetDir, 'BUNDLE_README.md'), [
    '# Pi Mono Bundle',
    '',
    'This directory contains the pi-mono package set staged from installed npm packages.',
    'NineClaw uses the coding-agent launcher as the runtime entrypoint, and bundles the',
    'rest of the pi-mono core packages alongside it for future integration and inspection.',
    '',
    `Local node_modules root: ${localNodeModulesRoot}`,
    `Global npm root fallback: ${resolveGlobalNpmRoot()}`,
    '',
    'Repo-only packages still require PI_MONO_REPO_DIR during staging.',
  ])

  return {
    mode: 'installed-packages',
    source: 'local-node_modules-or-global-fallback',
    packages,
    missingPackages,
  }
}

function stageMonoBundle(targetDir) {
  const repoDir = resolveMonoRepoDir()
  if (repoDir) {
    return stageRepoMonoBundle(targetDir, repoDir)
  }
  return stageInstalledMonoBundle(targetDir)
}

function main() {
  const platformDir = resolvePlatformDir(process.argv[2] ?? process.env.PI_RUNTIME_PLATFORM)
  const targetDir = cleanPlatformDir(platformDir)
  const packageDir = resolveRuntimePackageDir()
  const nodeExecutable = resolveNodeExecutable(platformDir)
  const nodeTargetName = platformDir === 'windows' ? 'node.exe' : 'node'
  const nodeTargetPath = path.join(targetDir, nodeTargetName)
  const packageTargetPath = path.join(targetDir, 'pi-package')

  fs.copyFileSync(nodeExecutable, nodeTargetPath)
  if (platformDir !== 'windows') {
    fs.chmodSync(nodeTargetPath, 0o755)
  }
  const copiedLibraries = copyBundledLibraries(platformDir, nodeExecutable, targetDir)
  if (platformDir === 'macos') {
    finalizeMacRuntimeBundle(targetDir, nodeTargetPath)
  }

  copyRecursive(packageDir, packageTargetPath)
  rewritePackageSymlinks(packageTargetPath, packageDir)
  copyHoistedDependencies(packageDir, packageTargetPath)
  const runtimeNodeModulesRoot = path.join(targetDir, 'node_modules')
  fs.mkdirSync(runtimeNodeModulesRoot, { recursive: true })
  for (const packageName of extraRuntimePackageNames) {
    copyRuntimePackageToNodeModules(packageName, runtimeNodeModulesRoot)
  }
  const launcherPath = writeLauncher(platformDir, targetDir)
  const monoBundle = stageMonoBundle(targetDir)

  writeManifest(targetDir, {
    platform: platformDir,
    launcher: path.basename(launcherPath),
    nodeExecutable: path.basename(nodeTargetPath),
    sourceNodeExecutable: nodeExecutable,
    sourcePackageDir: packageDir,
    packageName: runtimePackageName,
    copiedLibraries,
    monoBundle,
  })

  if (fs.existsSync(readmePath)) {
    fs.copyFileSync(readmePath, path.join(targetDir, 'README.md'))
  }

  removeMacExtendedAttributes(targetDir)
  const archivePath = createRuntimeArchive(platformDir)

  console.log(`Bundled pi runtime prepared for ${platformDir}: ${targetDir}`)
  console.log(`Bundled pi runtime archive prepared: ${archivePath}`)
}

main()
