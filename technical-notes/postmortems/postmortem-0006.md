# Postmortem-0006: Channel Factory 新增通道流程

- 来源：`docs/DEV_NOTES.md` #6（Channel Factory 模式）

## 1. 执行摘要

新增通道需遵循固定五步：`factory.rs` 枚举加变体 → 实现 `Channel` trait → `create_channel` 加 match arm → `lib.rs` 加 Tauri command → 前端 `piClient.ts` 加 invoke 封装。遗漏任一步会导致新通道不可用。此为流程文档，非事故。

## 2. 时间线

- 抽象：通道统一走 `ChannelFactory` 模式。
- 沉淀：明确新增通道的标准五步清单并文档化。

## 3. 根因

新增通道横跨 Rust 枚举 / trait / factory / command 与前端封装多个文件，缺少机械检查时容易漏步，且漏步往往只在运行时才暴露。

## 4. Guardrails

- 新增护栏：新增通道必须按五步清单逐项落地，缺一不可。
- 为什么流程放过了它：步骤分散在多个文件，无清单/脚本校验完整性，靠人工自觉。
- 新检查：新增通道的 PR 用 checklist 核对五步；枚举变体缺 match arm 可由编译穷尽检查兜底，前端封装缺失由 typecheck 兜底。
