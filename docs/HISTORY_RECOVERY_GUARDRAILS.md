# History Recovery Guardrails

- `chat_sessions` / `chat_turns` 是历史会话的第一数据源。
- `app_state.history_v1` 只做备份，不得作为唯一真相。
- 关键边沿必须先写结构化 SQLite：空会话创建、首条用户消息、bot inbound 首条、bot done/error 收尾，都不能只留在内存里等下一次大快照。
- “先内存后异步保存”只允许用于补充性 snapshot flush，且窗口必须保持高频；当前前端窗口基线是 idle `10ms`、running `20ms`，后续若要调大必须先补崩溃恢复验证。
- 前端 `save_history_state` 只能 merge/upsert，不能整表替换。
- 启动恢复必须优先回源结构化 SQLite；`history_v1` 仅在结构化数据不可用时兜底。
- 启动恢复必须容忍单源失败：`structured sqlite`、`history_v1 snapshot`、`legacy localStorage` 任意一路成功，都必须先把历史会话拉起来；禁止因为其中一路报错就把整个历史列表置空。
- 恢复到前端内存态时，禁止做“只保留最近 N 条会话”的硬截断。分页、虚拟列表、折叠桶只能发生在展示层，不能让已持久化的历史会话在恢复后直接从状态树里消失。
- 当 `history_v1` 与结构化 SQLite 不一致时，必须以结构化 SQLite 为主；`history_v1` 只能补回结构化里完全缺失的旧会话，绝不能覆盖结构化里更新的会话状态、标题或轮次。
- 崩溃/闪退场景要按“structured 已先落盘、snapshot 可能滞后”来设计恢复逻辑，不能假设 blob 一定比结构化更新。
- scheduler daemon 不得自动拉起微信/飞书等 IM 机器人；IM 通道只允许主交互进程接管，避免后台进程和前台进程争抢导致“像重启一样”的重复自动启动与状态污染。
- 主 UI 进程必须保持单实例。禁止让打包版 `NineClaw.app` 与 `tauri dev` 下的桌面实例同时存活并共用同一份历史库，否则用户会把实例切换误判成“中途重启”或“会话丢失”。
- 任何清空/删除历史都必须同步更新结构化表和备份。
- 新增历史恢复或落盘逻辑时，必须补测试，覆盖崩溃重启后的恢复场景。
- 在 `tauri dev` 下，代码改动触发的 app reload 属于正常现象；reload 之后历史列表仍必须从持久化层完整恢复，不能出现“数据库还在但侧边栏空了”的假丢失。
