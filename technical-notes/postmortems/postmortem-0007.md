# Postmortem-0007: PiBridge 会话隔离与临时文件路径

- 来源：`docs/DEV_NOTES.md` #6（PiBridge 会话隔离）

## 1. 执行摘要

`PiBridge` 为每个 `(channel_id, user_id)` 生成独立 session 文件，原路径 `/tmp/nineclaw-bot-session-{hash}.jsonl`；pi 子进程一次性运行。该共享 /tmp 固定前缀路径已按 ADR-0002 迁入用户私有目录（0700/0600、拒 symlink、legacy 迁移一次）。

## 2. 时间线

- 设计：按 `(channel_id, user_id)` 生成独立 session 文件，pi 子进程 prompt→response→退出。
- 隐患：早期实现用共享 `/tmp` + 固定前缀，未考虑多用户/daemon 泄露风险。
- 决策：按 ADR-0002 迁入应用数据目录下的私有运行时目录。

## 3. 根因

早期实现为省事用共享 `/tmp` + 固定前缀；单用户桌面机下泄露风险不明显，缺少权限/路径约束测试。

## 4. Guardrails

- 新增护栏：session / summary / runtime 文件必须落在用户私有目录（目录 0700、文件 0600）、拒绝 symlink、旧 /tmp 迁移一次（见 ADR-0002）。
- 为什么流程放过了它：共享 /tmp 固定前缀在单用户场景无可见影响，无权限/路径相关测试或契约。
- 新检查：写临时文件的测试断言文件 mode=0600、目录 mode=0700、路径含私有段、symlink 被拒。
