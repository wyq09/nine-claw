# Bundled Pi Runtime

NineClaw stages the current machine's Node runtime plus a `pi-mono`-aware runtime bundle
into this directory during build:

```bash
npm run prepare:pi-runtime
```

Generated layout:

- `src-tauri/resources/pi-runtime/windows/`
- `src-tauri/resources/pi-runtime/macos/`
- `src-tauri/resources/pi-runtime/linux/`

Each platform directory contains:

- a bundled Node executable
- a copied `pi` package tree (`pi-package/`)
- a broader `pi-mono` resource bundle (`pi-mono/`)
- a launcher script (`pi` or `pi.cmd`)
- a small `manifest.json`

By default, staging prefers packages from the current project's `node_modules` and only falls back
to a global npm install when needed. If `PI_MONO_REPO_DIR` is set, the bundle also captures a
pi-mono repository snapshot for repo-only resources.

At runtime, NineClaw prefers this bundled launcher and only falls back to a system-wide `pi` when
the bundled runtime is missing.
