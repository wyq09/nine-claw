# Pi Runtime Bundling

## Goal

NineClaw now treats `pi` as a first-class app runtime instead of a pure external prerequisite:

- packaged app prefers a bundled runtime launcher
- that launcher ships with a bundled Node executable and a copied `pi` npm package
- the same bundle now carries an explicit `pi-mono` resource snapshot instead of only the CLI package
- development app can still fall back to a system `pi` on `PATH`
- Windows keeps the old npm-based auto-install path as a last resort

This keeps packaging predictable while preserving local development convenience.

## Runtime Resolution Order

The Rust runtime layer is centralized in `src-tauri/src/pi_runtime.rs`.

Resolution order:

1. bundled runtime in Tauri resources
2. system `pi` discovered from `PATH`
3. Windows-only npm auto-install fallback

All `pi` entry points now share the same resolver:

- main chat streaming
- IM bot workers
- heartbeat scheduler

## Resource Layout

The build script stages runtime assets into one of these paths:

```text
src-tauri/resources/pi-runtime/windows/
src-tauri/resources/pi-runtime/macos/
src-tauri/resources/pi-runtime/linux/
```

Each platform directory contains:

- `node` or `node.exe`
- `pi-package/` copied from the resolved `@mariozechner/pi-coding-agent`
- `pi-mono/` containing either:
  - a local `pi-mono` repo snapshot when `PI_MONO_REPO_DIR` is provided
  - or a staged bundle of installed `@mariozechner/*` core packages from `node_modules`
- `pi` or `pi.cmd` launcher
- `manifest.json`

Tauri includes `resources/pi-runtime` in the final application package.

## Staging Helper

Use the helper script to stage the current machine's runtime:

```bash
npm run prepare:pi-runtime
```

Optional platform override:

```bash
npm run prepare:pi-runtime -- macos
```

Optional full monorepo snapshot:

```bash
PI_MONO_REPO_DIR=/path/to/pi-mono npm run prepare:pi-runtime -- macos
```

Resolution order for package staging:

1. `PI_RUNTIME_PACKAGE_DIR` for the runtime CLI package
2. local project `node_modules`
3. global npm install as compatibility fallback

For the broader `pi-mono` bundle, `PI_MONO_REPO_DIR` takes precedence. Without it,
NineClaw stages the published `@mariozechner/*` packages it can resolve locally.

## Packaging Recommendations

For release engineering, keep the runtime build separate from the app build, but no longer manual:

1. run `npm install` so the pinned `@mariozechner/*` package set exists locally
2. if you need repo-only packages such as `pi-pods`, expose a built pi-mono checkout via `PI_MONO_REPO_DIR`
3. ensure the build machine's `node` is the one you want to ship
4. run `npm run build` / `npm run tauri build`

That separation is intentional. It keeps the desktop client self-contained while still letting the
runtime bundle grow from a single CLI package into a broader `pi-mono` capability pack.

## Future Extension Points

The current seam is no longer "future-only"; it already bundles a broader `pi-mono` resource set.
Further evolution can build on that without changing the app packaging model:

- replace CLI spawning with embedded SDK sessions
- add version metadata / checksum verification for bundled runtimes
- support multiple agent runtimes side by side
- expose runtime diagnostics in settings for other coding engines
