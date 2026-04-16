# Rust Breakdown: SQLite Unified Storage

## Goal

本文件把 Rust 侧改造拆到模块、函数、调用链和实施顺序，目标是让实现者不需要再自己决定“从哪儿开始拆”。

## Current Rust Call Graph

当前关键入口：

- [src-tauri/src/lib.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/lib.rs:2423)
  - `read_agent_workspace_bundle`
  - `read_agent_workspace_file`
  - `write_agent_workspace_file`
- [src-tauri/src/lib.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/lib.rs:913)
  - `get_session_context_stats`
- [src-tauri/src/lib.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/lib.rs:4318)
  - `stream_pi_prompt`

当前核心实现位置：

- [src-tauri/src/agent_workspace.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:524)
  - prompt 组装、workspace bundle、文件读写、memory ingest
- [src-tauri/src/agent_workspace/memory_wiki.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace/memory_wiki.rs:19)
  - source index / daily index / wiki 脚手架
- [src-tauri/src/lib.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/lib.rs:1688)
  - session / summary 临时文件路径

## Target Module Split

新增目录：

```text
src-tauri/src/storage/
  mod.rs
  db.rs
  migrations.rs
  chat_history.rs
  user_memory.rs
  agent_memory.rs
  runtime_sessions.rs
  projections.rs
  search_docs.rs
```

### `storage/db.rs`

职责：

- 统一打开 SQLite 连接
- 统一 PRAGMA
- 统一 schema ensure / migrate 入口

迁移内容：

- 从当前 `open_history_db()`、`history_db_path()` 抽离
- 保留现有 DB 文件路径 `nineclaw.sqlite3`

### `storage/migrations.rs`

职责：

- 所有新表创建
- `history_v1` 导入
- 版本号管理

要求：

- 使用 `schema_version` 或等价表维护迁移版本
- 禁止继续在 `lib.rs` 内散落 `CREATE TABLE IF NOT EXISTS`

### `storage/chat_history.rs`

职责：

- `chat_sessions` / `chat_turns`
- 聊天增量读写
- 从 `history_v1` 导入

新增函数：

- `list_chat_sessions`
- `get_chat_session`
- `create_chat_session`
- `append_chat_turn`
- `update_chat_turn`
- `delete_chat_session`
- `clear_all_chat_sessions`
- `migrate_legacy_history_v1`

替换点：

- `load_history_state`
- `save_history_state`
- `clear_history_state`
- `session_context_stats_from_history`

### `storage/user_memory.rs`

职责：

- `user_profiles`
- `user_profile_aliases`
- `user_memory_documents`

新增函数：

- `ensure_default_owner_profile`
- `resolve_user_profile_for_source`
- `bind_user_profile_alias`
- `list_user_profiles`
- `get_user_profile_documents`
- `upsert_user_memory_document`

要求：

- 桌面来源必须稳定返回 `owner`
- 不做自动身份猜测

### `storage/agent_memory.rs`

职责：

- `memory_documents`
- `memory_events`
- `memory_sources`
- `memory_source_links`

新增函数：

- `load_agent_document`
- `upsert_agent_document`
- `append_memory_event`
- `register_memory_source`
- `link_memory_source`
- `list_daily_digest_events`
- `list_source_index_entries`

替换点：

- `append_agent_memory_entry`
- `register_agent_attachment_source`
- `build_specialized_memory_snapshot`
- `build_daily_digest_retrieval_snapshot`

### `storage/runtime_sessions.rs`

职责：

- runtime session / summary
- `jsonl` 物化与清理

新增函数：

- `get_or_create_runtime_session`
- `append_runtime_session_message`
- `load_runtime_session_messages`
- `record_runtime_multimodal_summary`
- `load_runtime_multimodal_summaries`
- `materialize_runtime_session_file`
- `clear_runtime_session`

替换点：

- `session_file_path`
- `ephemeral_session_file_path`
- `summary_file_path`
- `record_multimodal_summary`
- `prepend_multimodal_summary_context`
- `clear_pi_session`
- `clear_pi_session_for_id`

说明：

- 路径生成函数可以保留，但只作为物化工具

### `storage/projections.rs`

职责：

- SQLite -> 文件
- 文件 -> SQLite
- 冲突检测

新增函数：

- `render_projection_file`
- `write_projection_file`
- `import_projection_file`
- `verify_projection_integrity`
- `rebuild_files_from_sqlite`
- `rebuild_sqlite_from_files`

### `storage/search_docs.rs`

职责：

- 统一导出 search documents
- 不要求第一阶段就启用 FTS5 / `sqlite-vec`

## Existing Function Rewrite Plan

### 1. `append_agent_memory_entry()`

当前位置：

- [src-tauri/src/agent_workspace.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:706)

改造顺序：

1. 计算 `summary` / `categories`
2. 解析来源 profile
3. 写 `memory_events`
4. 如需，更新 `memory_documents` 中的 `working`
5. 如命中坑点，更新 `pitfalls`
6. 如存在 raw source，写 `memory_sources`
7. 最后调用 projection 层回写 `WORKING.md` / daily / source index

禁止：

- 先写文件再回推状态

### 2. `register_agent_attachment_source()`

当前位置：

- [src-tauri/src/agent_workspace.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:806)

改造顺序：

1. 写 `memory_sources`
2. 写 `memory_events` 的 `attachment_registered`
3. 建立 source links
4. 更新 daily digest projection
5. 更新 `SOURCE_INDEX` projection

### 3. `read_agent_workspace_bundle()`

当前位置：

- [src-tauri/src/agent_workspace.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:634)

改造目标：

- 先从 SQLite 组装文档列表
- `exists` 表示 projection 是否存在
- `content` 优先来自 SQLite
- `lazy_fetch` 保留

### 4. `write_agent_workspace_file()`

当前位置：

- [src-tauri/src/agent_workspace.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:688)

改造目标：

1. 校验可写 scope
2. 写 SQLite 文档
3. 更新 projection metadata
4. 回写 projection 文件
5. 返回新的 bundle

### 5. `build_workspace_system_prompt_for_query()`

当前位置：

- [src-tauri/src/agent_workspace.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:524)

改造目标：

- 文案结构保留
- 内容来源改为 SQLite-first

实施步骤：

1. 先替换 specialized snapshot 数据来源
2. 再替换 daily digest snapshot 数据来源
3. 最后替换 legacy workspace memory snapshot 数据来源

### 6. `stream_pi_prompt()`

当前位置：

- [src-tauri/src/lib.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/lib.rs:4318)

改造目标：

- session 文件不再被视为真实存储
- 调用前通过 `materialize_runtime_session_file()` 获取路径

实施步骤：

1. 建 runtime session
2. 读取 runtime messages
3. 物化 `jsonl`
4. 把路径传给 PI
5. 流式期间持续把消息或 summary 同步回 SQLite

## File-Level Change Order

### Step 1

新增：

- `src-tauri/src/storage/*`

仅引入，不改调用方。

### Step 2

修改：

- [src-tauri/src/lib.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/lib.rs:1)

动作：

- 命令入口改调新 storage 模块
- 删除旧 history blob API

### Step 3

修改：

- [src-tauri/src/agent_workspace.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:1)
- [src-tauri/src/agent_workspace/memory_wiki.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace/memory_wiki.rs:1)

动作：

- 变成 storage + projection facade
- 不再直接承担主存储职责

### Step 4

修改：

- `heartbeat.rs`
- `scheduler/mod.rs`
- `channels/lark/mod.rs`
- `channels/wechat/mod.rs`
- `peer_gateway.rs`

动作：

- 这些地方对 `append_agent_memory_entry()` 的调用保持不变
- 但底层语义变成 SQLite-first

## Tests to Add

### Migration

- `history_v1` 导入结构化表
- 重复运行迁移不重复插入

### User Profiles

- 桌面来源解析到 `owner`
- alias 映射正确
- 未绑定来源不会误归并

### Agent Memory

- ingest 写入 `memory_events`
- `working` / `pitfalls` 文档能同步更新
- `source index` 投影可重建

### Runtime Sessions

- 运行态消息能写入 SQLite
- `jsonl` 能从 SQLite 物化
- 清理后 SQLite 和文件都删除

### Workspace

- 没有 projection 文件时 bundle 仍可读
- 编辑 workspace 文件后 SQLite 内容更新

## Completion Criteria

Rust 侧视为完成的标准：

- 主逻辑不再依赖 `history_v1`
- `agent_workspace.rs` 不再承担主存储角色
- `stream_pi_prompt()` 不再把临时文件当真源
- Workspace / prompt / runtime / ingest 都能通过 storage 层闭合
