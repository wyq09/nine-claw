# Postmortem-0003: WeChat Channel 自包含架构与 ChannelManager 不一致

- 来源：`docs/DEV_NOTES.md` #3

## 1. 执行摘要

微信通道采用自包含（self-contained）架构，不走 `ChannelManager` 的统一消息处理循环，导致 `ensure_processing()` / `process_incoming_messages()` 对微信不生效、`_tx` 参数未被使用，容易被误当成统一入口。此为架构文档沉淀，非线上事故。

## 2. 时间线

- 设计：微信通道采用自包含 monitor/worker 线程（`getUpdates` 长轮询 + `PiBridge` 调用 pi 子进程）。
- 沉淀：明确 `ChannelManager` 的统一循环对微信通道不生效，相关配置走 `channel.set_ai_config()`。

## 3. 根因

微信通道与 `ChannelManager` 是两套并行处理路径，命名相近但语义不同；没有显式契约说明"哪些通道走哪条路径、哪些统一 API 对谁不生效"，埋下误用风险。

## 4. Guardrails

- 新增护栏（文档契约）：自包含通道必须显式声明"不走 `ChannelManager`"，并列出对自身不生效的统一 API。
- 为什么流程放过了它：两套路径并存却无"通道→处理路径"的归属标记，靠读代码才知 `_tx` 未使用。
- 新检查：新增通道时在文档/注释中声明其处理路径归属；未被 `ChannelManager` 覆盖的通道须在契约中显式列出。
