# dsh 对照基准快照

本目录 4 份调研报告共同依赖的基准信息。dsh 代码演进后，报告中的结论应以这里的快照为准进行复核。

| 项 | 值 |
|----|----|
| 上游仓库 | https://github.com/deepseek-ai/deepseek-harness |
| dsh commit | 待补（首次生成报告时未固定，复核时请填具体 commit hash） |
| nine-claw commit | `ca4bb62e766ef2a80e48d7d5b52ebe80b445b756` |
| 报告生成日期 | 2026-08-13 |
| 报告最后修订 | 2026-08-13（实施第一批 P0 后） |

## 使用约定

- 所有 `file:line` 引用以 nine-claw commit 为准；文件变更后优先跑对应测试，而不是手工改行号。
- 复核对 dsh 的结论前，先在 dsh 仓库 checkout 到上表 commit。
- 若无法确认 dsh commit，至少记录查阅时的默认分支和日期。

## 变更记录

| 日期 | 变更 |
|------|------|
| 2026-08-13 | 修正 01 报告中 `hasBrokenToolPairs` 的调用方描述；修正 04 报告中 `npm run build` 流水线描述 |
| 2026-08-13 | 落地第一批：Unicode 码点裁剪、配对平衡、CI 骨架、800 行 ratchet；状态见 `ROADMAP.md` |
