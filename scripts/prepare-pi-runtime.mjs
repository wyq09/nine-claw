import fs from 'node:fs'
import path from 'node:path'
import { spawnSync } from 'node:child_process'

const runtimePackageName = '@mariozechner/pi-coding-agent'
const publishedMonoPackageNames = [
  '@mariozechner/pi-coding-agent',
  '@mariozechner/pi-agent-core',
  '@mariozechner/pi-ai',
  '@mariozechner/pi-tui',
  '@mariozechner/pi-web-ui',
  '@mariozechner/pi-mom',
]
const repoOnlyMonoPackageNames = ['@mariozechner/pi-pods']
const projectRoot = process.cwd()
const localNodeModulesRoot = path.resolve(projectRoot, 'node_modules')
const resourceRoot = path.resolve(projectRoot, 'src-tauri', 'resources', 'pi-runtime')
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
      } else if (rawDependency.startsWith('/')) {
        resolvedDependency = resolveRealPathIfExists(rawDependency)
      }

      if (!resolvedDependency || copied.has(resolvedDependency)) continue

      fs.mkdirSync(libDir, { recursive: true })
      const targetPath = path.join(libDir, path.basename(resolvedDependency))
      fs.copyFileSync(resolvedDependency, targetPath)
      fs.chmodSync(targetPath, 0o755)
      copied.add(resolvedDependency)
      queue.push(resolvedDependency)
    }
  }

  return Array.from(copied)
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
    fs.rmSync(fullPath, { recursive: true, force: true })
  }
  return targetDir
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

function resolveNodeExecutable() {
  const envPath = process.env.PI_RUNTIME_NODE_PATH?.trim()
  const executable = envPath ? path.resolve(envPath) : fs.realpathSync(process.execPath)
  if (!fs.existsSync(executable)) {
    throw new Error(`Node executable not found: ${executable}`)
  }
  return executable
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
  const content = [
    '#!/bin/sh',
    '# Bundled pi launcher generated during build.',
    'set -eu',
    'SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"',
    'if [ -d "$SCRIPT_DIR/lib" ]; then',
    '  export DYLD_LIBRARY_PATH="$SCRIPT_DIR/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"',
    '  export LD_LIBRARY_PATH="$SCRIPT_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"',
    'fi',
    'exec "$SCRIPT_DIR/node" "$SCRIPT_DIR/pi-package/dist/cli.js" "$@"',
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
  const nodeExecutable = resolveNodeExecutable()
  const nodeTargetName = platformDir === 'windows' ? 'node.exe' : 'node'
  const nodeTargetPath = path.join(targetDir, nodeTargetName)
  const packageTargetPath = path.join(targetDir, 'pi-package')

  fs.copyFileSync(nodeExecutable, nodeTargetPath)
  if (platformDir !== 'windows') {
    fs.chmodSync(nodeTargetPath, 0o755)
  }
  const copiedLibraries = copyBundledLibraries(platformDir, nodeExecutable, targetDir)

  copyRecursive(packageDir, packageTargetPath)
  rewritePackageSymlinks(packageTargetPath, packageDir)
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

  console.log(`Bundled pi runtime prepared for ${platformDir}: ${targetDir}`)
}

main()
