# Postmortem-0008: 前端事件流订阅需成对清理

- 来源：`docs/DEV_NOTES.md` #7

## 1. 执行摘要

后端通过 Tauri `emit` 推送 4 类事件（`pi://stream`、`bot://message`、`bot://status`、`bot://qr-code`），前端 `listen` 订阅后若不在组件卸载时 `unlisten`，会导致重复回调/内存泄漏。此为事件契约文档 + 一条防御性护栏。

## 2. 时间线

- 建立：后端事件流 → 前端 `listen` 订阅 → 识别未 `unlisten` 的泄漏风险 → 沉淀契约。

## 3. 根因

事件监听是全局的；组件级订阅若不在卸载时清理，组件重建后回调会叠加触发，且早期组件不频繁卸载，泄漏不可见。

## 4. Guardrails

- 新增护栏：所有 `listen` 订阅必须在组件卸载时 `unlisten`。
- 为什么流程放过了它：早期组件不重建/不频繁卸载，泄漏静默且不可见。
- 新检查：React 组件 `useEffect` 返回清理函数调用 `unlisten`；评审检查每个 `listen` 是否有对应 `unlisten`。
