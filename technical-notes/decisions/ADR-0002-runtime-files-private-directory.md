# ADR-0002: PI 运行时文件迁入用户私有目录

- 状态：Accepted
- 日期：2026-08-14

## Context

PI 的 session / summary / runtime 文件原本写在共享临时目录 `/tmp`，文件名带固定前缀（如 `nineclaw-bot-session-`）。在单用户桌面机上风险不明显，但多用户主机或 daemon（`--scheduler-daemon`）模式下，这些文件可能被同机其他用户读取，存在信息泄露风险。

另一处硬约束：对共享临时目录做 `chmod` 收紧权限，在 macOS 上会触发 `EPERM`，无法可靠完成。

## Decision

把 PI session / summary / runtime 文件从共享 `/tmp` 固定前缀，迁到应用数据目录下的**用户私有运行时目录**：

- 目录权限 `0700`，文件权限 `0600`；
- 写入前用 `lstat` **拒绝符号链接**，避免 symlink 攻击或误链到共享路径；
- 首次启动时把 `/tmp` 下旧前缀文件**迁移一次**（legacy 迁移），之后不再依赖共享 /tmp。

**硬规则**：只 chmod 应用自有 runtime 树内的目录；绝不 chmod runtime 树之外的目录（含共享临时目录），以规避 macOS `EPERM` 回退场景。

## Alternatives considered

- **保留 /tmp，仅换随机文件名**：实现最省，但随机名不解决"目录本身共享、其他用户可枚举"的根本问题，且随机性不提供权限隔离。
- **每 session 一个子目录**：隔离粒度更细，但目录散落、清理成本高，且仍需先解决父目录权限问题。
- **O_TMPFILE / 匿名临时文件**：文件不落名、更隐蔽，但 pi 子进程需按路径读写，跨进程不适用，且平台支持不一致。

## Consequences

- 正面：文件权限与路径都受控，多用户/daemon 泄露风险消除；不再依赖共享 /tmp 的可预测路径。
- 负面：需一次性 legacy 迁移逻辑与配套测试；调用方需从"拼 `/tmp` 路径"改为"向 runtime 模块取路径"。
- 中性：最常见的单用户桌面场景行为基本不变，仅路径变化。

## Rejected options

见 Alternatives considered：随机名、每 session 子目录、O_TMPFILE 均被否决，理由如上。
