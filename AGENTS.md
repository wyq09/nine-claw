# AGENTS.md — Instructions for AI Coding Agents (Codex, Claude, etc.)

## CRITICAL: Protected Files — DO NOT MODIFY

### `src-tauri/resources/pi-runtime/macos/pi`

**This file is PROTECTED. You MUST NOT modify it under any circumstances.**

#### Why This File Matters

On macOS (Darwin 25+), the bundled "thin node" binary (`src-tauri/resources/pi-runtime/macos/node`) is a ~68KB wrapper that depends on `@loader_path/libnode.141.dylib`. This dylib is ad-hoc signed, and macOS SIP/AMFI will SIGKILL any process that tries to load it.

The `pi` launcher script works around this by searching the system PATH for a properly-signed system node, **explicitly skipping its own directory** (`SCRIPT_DIR`) during the search. This ensures the bundled broken node is never selected on macOS.

#### The Broken Pattern (NEVER write this)

```sh
# BROKEN — will find the bundled thin node and get SIGKILL'd
if [ "$(uname -s)" = "Darwin" ] && command -v node >/dev/null 2>&1; then
  NODE_BIN="$(command -v node)"
fi
```

The problem: `command -v node` searches the full PATH, which includes SCRIPT_DIR. The bundled `node` binary in SCRIPT_DIR is found first, selected, and immediately killed by SIP/AMFI.

#### The Correct Pattern (already in the file, do not change)

```sh
if [ "$(uname -s)" = "Darwin" ]; then
  _saved_IFS="$IFS"
  IFS=':'
  _found=
  for _dir in $PATH; do
    case "$_dir" in
      "$SCRIPT_DIR") continue ;;  # Skip bundled node directory
    esac
    if [ -x "$_dir/node" ]; then
      _found="$_dir/node"
      break
    fi
  done
  IFS="$_saved_IFS"
  if [ -n "$_found" ]; then
    NODE_BIN="$_found"
  fi
fi
```

#### Enforcement

- A pre-commit hook (`.githooks/pre-commit`) automatically restores the correct content on every commit.
- If you modify this file, the hook will silently overwrite your changes.
- **Do not attempt to "simplify", "modernize", or "improve" this file.** The complexity is intentional and necessary.

#### If You See SIGKILL (signal 9)

If PI is getting killed immediately on macOS, it means this file was modified incorrectly. Run:
```sh
cat src-tauri/resources/pi-runtime/macos/pi | head -10
```
If you see `command -v node`, the file has been corrupted. Restore it from git:
```sh
git checkout HEAD -- src-tauri/resources/pi-runtime/macos/pi
```

## Build & Packaging Guide

### Prerequisites

- Node.js v22+ (构建脚本会用当前 node 版本下载对应的官方二进制)
- Rust toolchain (`rustup`)
- macOS: Xcode Command Line Tools

### Build Command

```sh
# macOS (arm64)
npm run tauri build

# 只打 DMG（跳过 .app）
npm run tauri build -- --bundles dmg
```

产物位置：
- `.app`: `src-tauri/target/release/bundle/macos/NineClaw.app`
- `.dmg`: `src-tauri/target/release/bundle/dmg/NineClaw_0.1.0_aarch64.dmg`
- Windows 需要 Windows 机器或 GitHub Actions，无法在 macOS 交叉编译

### Build Pipeline（`npm run build`）

1. `build:lark-helper` — esbuild 打包飞书 bot helper
2. `prepare:pi-runtime` — **核心步骤**，准备 PI 运行时（见下文）
3. `tsc -b && vite build` — 前端 TypeScript 编译 + Vite 打包
4. `cargo build --release` — Rust 后端编译
5. `tauri bundle` — 打包 .app/.dmg

### PI Runtime 打包细节（`scripts/prepare-pi-runtime.mjs`）

这是最关键的构建步骤，负责把 PI agent 运行所需的一切打包进应用。

#### 打包内容

| 内容 | 来源 | 说明 |
|------|------|------|
| `node` (105MB) | `nodejs.org/dist` 下载的官方二进制 | 解决 macOS SIP/AMFI SIGKILL 问题 |
| `pi-package/` | `node_modules/@mariozechner/pi-coding-agent` | PI agent 主程序 |
| `pi-package/node_modules/` | 递归依赖树（239 个包） | 包括直接和传递依赖 |
| `pi-mono/` | pi-mono 仓库快照或已安装包 | PI 生态核心包 |
| `pi` (launcher) | `writeLauncher()` 生成 | 启动脚本，macOS 跳过 SCRIPT_DIR |
| `lib/` | node 的动态链接库 | macOS 上 codesign + install_name_tool 处理 |

#### 关键逻辑

1. **官方 Node.js 下载** — `downloadOfficialNode()` 从 `nodejs.org` 下载对应平台和架构的完整二进制（非 thin wrapper），缓存到 `.cache/node-binaries/`
2. **递归依赖复制** — `copyHoistedDependencies()` 递归解析 `package.json` 的 `dependencies` + `optionalDependencies`，把 npm hoist 到顶层 `node_modules/` 的包全部复制到 bundle 的 `pi-package/node_modules/`
3. **macOS 库签名** — `finalizeMacRuntimeBundle()` 对所有 dylib 做 `install_name_tool` 重写 + `codesign --force --sign -`
4. **tar.gz 打包** — 生成 `src-tauri/resources/pi-runtime-bundles/macos.tar.gz`，app 启动时解压到 `~/Library/Application Support/com.wuyq.nineclaw/pi-runtime-extracted/`

#### 已知的坑

- **npm hoist 问题**：npm 把大部分依赖提升到项目根 `node_modules/`，不在 `pi-coding-agent/node_modules/` 下。必须递归复制，否则运行时报 `ERR_MODULE_NOT_FOUND`
- **macOS SIP/AMFI**：bundled 的 thin node (68KB @loader_path wrapper) 会被 SIGKILL。`prepare-pi-runtime.mjs` 的 `writeLauncher()` 必须用 PATH 遍历跳过 SCRIPT_DIR，不能用 `command -v node`
- **pi-ai 路径查找**：Rust 端 `resolve_pi_ai_import_path()` 需要搜索 `pi-package/node_modules/` 路径（不仅是 `node_modules/`），因为打包后 pi-ai 在 `pi-package/` 子目录下
- **DMG 打包超时**：runtime 体积大（~378MB 源文件），首次 DMG 打包可能超时，重试通常成功
- **旧提取目录缓存**：app 会比对 archive 路径决定是否重新解压。修改构建后需要手动删除 `~/Library/Application Support/com.wuyq.nineclaw/pi-runtime-extracted/`

#### 环境变量覆盖

| 变量 | 用途 |
|------|------|
| `PI_RUNTIME_NODE_PATH` | 指定 node 二进制路径（跳过下载） |
| `PI_RUNTIME_PACKAGE_DIR` | 指定 pi-coding-agent 包路径 |
| `PI_RUNTIME_PLATFORM` | 指定目标平台（macos/windows/linux） |
| `PI_MONO_REPO_DIR` | 指定 pi-mono 仓库路径（用于完整快照） |

## Source file size — 800 lines max

- **Hand-written source**（本仓库内由我们编写、维护的 `.ts` / `.tsx` / `.rs` / `.css` 等）**单文件不宜超过 800 行**；**非必要不得**新增或把文件撑到 800 行以上。
- 接近或超过上限时：**拆模块**（按领域/功能分文件）、**抽 React 组件或 hook**、**把纯函数挪到 `*.ts` 工具文件**，而不是继续在同一文件堆叠逻辑。
- **例外**（不强制套用 800 行）：第三方 vendored 源码、构建/锁文件、明确标注为机器生成的文件、以及仅含数据/配置的极长静态表（仍应优先考虑单独数据文件）。

## Code Review Rubrics

When reviewing plans or code changes, evaluate on these dimensions (1-10 scale):

| Dimension | Description |
|-----------|-------------|
| Correctness | Does it work as intended? Are edge cases handled? |
| Security | No injection, credential leaks, or unsafe operations? |
| Performance | No unnecessary allocations, O(n²) where O(n) exists? |
| Maintainability | Clear naming, appropriate abstractions, not over-engineered? |
| Compatibility | Works across platforms (macOS, Linux, Windows)? |

**Pass**: overall >= 7.0 AND no dimension <= 3.

## 所有的功能开发前必须设计单元测试，覆盖所以修改到的功能，全部测试通过了，才算完成任务
