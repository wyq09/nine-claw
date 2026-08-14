# ADR-0001: 采用 ADR 记录本仓库关键决策（含标准结构）

- 状态：Accepted
- 日期：2026-08-14

## Context

仓库在演进中产生过若干"为什么当时这样做"的隐性决策（如 PI 运行时文件路径、共享 /tmp 固定前缀），只散落在代码和 `docs/DEV_NOTES.md` 里，后人难以判断哪些是刻意的、哪些是可推翻的历史包袱。

## Decision

对会长期影响后续改动的决策，一律写成一条 ADR 存入本目录 `technical-notes/decisions/`。每条 ADR 必须包含以下标准结构：

| 段落 | 说明 |
|------|------|
| 标题 | 一句话决策，文件名 `ADR-NNNN-kebab-case.md` |
| 状态 | `Proposed` / `Accepted` / `Rejected` / `Superseded` |
| 日期 | 决策日期 |
| Context | 背景与要解决的问题 |
| Decision | 做出的决定（含硬规则） |
| Alternatives considered | 考虑过的备选方案及其取舍 |
| Consequences | 正面/负面/中性的结果 |
| Rejected options | 被否决的方案与原因 |

编号规则见本目录 `README.md`（连续递增、不重排、不复用）。

## Alternatives considered

- 继续靠 `docs/DEV_NOTES.md` 承载决策：信息已存在但无状态、无"被否方案"维度，难以追溯。
- 用代码注释承载：靠近代码但分散，且无法表达跨模块决策与时间线。

## Consequences

- 正面：关键决策可追溯、可推翻（标 `Superseded`）、有明确的被否方案记录。
- 负面：新增一条轻量维护义务（写 ADR 的纪律）。
- 中性：不改任何产品逻辑。

## Rejected options

- 不落地任何决策记录（现状）：否决，因为"为什么这样做"的知识随人流失。
- 单独维护一个 wiki/外部文档：否决，决策应随仓库版本走、随 PR 一起评审。
