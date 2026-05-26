# macOS Node 启动安全机制

当 macOS 系统尝试运行位于受限目录的可执行文件时（如 `/Applications`、`/System` 等），若该文件依赖未签名的 dylib，会被 SIP 拦截并发送 SIGKILL（signal 9）强制终止进程。

PI 的解决方案是 **严格遵守 `SCRIPT_DIR` 隔离规则**：在 `pi` 启动脚本中，会跳过自身目录下的 `node` 二进制，仅从系统 PATH 中选取 **已签名且非 ad-hoc** 的 node 二进制路径，有效避免崩溃。

相关文件：
- `/src-tauri/resources/pi-runtime/macos/pi`
- `/src-tauri/resources/pi-runtime/macos/node`