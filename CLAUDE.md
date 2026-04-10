# NineClaw — CLAUDE.md

## 保护文件：禁止修改

### `src-tauri/resources/pi-runtime/macos/pi`

这是 macOS 上的 PI runtime launcher 脚本。**绝对不要修改此文件**。

原因：bundled 的 68KB thin node 依赖 `@loader_path/libnode.141.dylib`（adhoc 签名），在 macOS Darwin 25+ 上被 SIP/AMFI 以 SIGKILL 阻止加载。正确的实现必须在 macOS 上搜索 PATH 时跳过 `SCRIPT_DIR`，确保优先使用系统安装的 node（完全签名）。

pre-commit hook `.githooks/pre-commit` 会自动恢复此文件的正确内容。如果 CI 或 Codex 将其改坏，请检查 `.githooks/pre-commit` 是否正常运行。
