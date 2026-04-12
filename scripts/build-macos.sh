#!/usr/bin/env bash
# NineClaw — macOS 发布包（会执行 beforeBuildCommand：含 prepare:pi-runtime、前端构建等）
#
# 用法（在仓库根目录；参数直接传给 `tauri build`，不要用多余的 `--`）：
#   ./scripts/build-macos.sh
#   ./scripts/build-macos.sh --bundles dmg             # 仅 DMG，通常略快
#   ./scripts/build-macos.sh --bundles app             # 仅 .app
#
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "请在 macOS 上执行本脚本。" >&2
  exit 1
fi

export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

echo "==> 开始 Tauri 发布构建（Rust release + 打包）…"
npm run tauri -- build "$@"

echo ""
echo "==> 常见产物路径（名称随版本/架构可能略有不同）："
echo "    $ROOT/src-tauri/target/release/bundle/macos/"
echo "    $ROOT/src-tauri/target/release/bundle/dmg/"
ls -la "$ROOT/src-tauri/target/release/bundle/macos/" 2>/dev/null || true
ls -la "$ROOT/src-tauri/target/release/bundle/dmg/" 2>/dev/null || true
