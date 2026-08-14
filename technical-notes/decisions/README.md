# Decisions (ADR)

本目录存放架构决策记录（Architecture Decision Records）。模板见 `ADR-0001-adr-template.md`。

## 编号约定

- 文件名：`ADR-NNNN-kebab-case.md`，`NNNN` 为 4 位零填充的连续递增序号。
- 序号从 `0001` 开始，**严格连续、绝不重排、绝不复用**。
- 已废弃的 ADR **不改编号**：把状态改为 `Superseded`，并在文末指向取代它的新 ADR。
- 一条 ADR 只记一个决策；决策范围变大时新开一条，而不是回头改写旧条目。

## 状态含义

| 状态 | 含义 |
|------|------|
| Proposed | 提议中，尚未定论 |
| Accepted | 已采纳，正在生效 |
| Rejected | 已否决（仍保留记录，供后人避免重走弯路） |
| Superseded | 被更新的 ADR 取代 |

## 索引

| 编号 | 标题 | 状态 | 日期 |
|------|------|------|------|
| ADR-0001 | 采用 ADR 记录关键决策（含标准结构） | Accepted | 2026-08-14 |
| ADR-0002 | PI 运行时文件迁入用户私有目录 | Accepted | 2026-08-14 |
