# Implementation Plan: SQLite Unified Storage

## Objective

按 [PRD_SQLITE_UNIFIED_STORAGE.md](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/docs/PRD_SQLITE_UNIFIED_STORAGE.md:1) 将 NineClaw 的聊天历史、agent 记忆、共享用户记忆、runtime session、wiki 统一纳入 SQLite 主存储，并将 Markdown 文件降级为投影层 / 兼容层 / 手工编辑视图。

配套细则见：

- [DB_SCHEMA_SQLITE_UNIFIED_STORAGE.md](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/docs/DB_SCHEMA_SQLITE_UNIFIED_STORAGE.md:1)
- [RUST_BREAKDOWN_SQLITE_UNIFIED_STORAGE.md](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/docs/RUST_BREAKDOWN_SQLITE_UNIFIED_STORAGE.md:1)
- [FRONTEND_BREAKDOWN_SQLITE_UNIFIED_STORAGE.md](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/docs/FRONTEND_BREAKDOWN_SQLITE_UNIFIED_STORAGE.md:1)

本实施计划默认接受以下前提：

- `history_v1` 直接废弃
- SQLite 是唯一主真源
- `--session <jsonl path>` 仍是当前 PI runtime 外部接口
- 文件系统短期保留，但不再是产品主读写路径

## Non-Negotiable Rules

- 不修改受保护文件 [src-tauri/resources/pi-runtime/macos/pi](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/resources/pi-runtime/macos/pi:1)
- 不重新引入旧的 `memory/categories/*.md` 主路径
- 不在第一阶段要求“完全无文件运行”
- 所有 SQLite 写入必须有明确 schema 与迁移，不允许继续堆 JSON blob 当主表

## Target Architecture

### Core Storage Domains

1. Chat
- `chat_sessions`
- `chat_turns`

2. Shared User Memory
- `user_profiles`
- `user_profile_aliases`
- `user_memory_documents`

3. Agent Private Memory / Wiki
- `memory_documents`
- `memory_events`
- `memory_sources`
- `memory_source_links`

4. Runtime Cache
- `runtime_sessions`
- `runtime_session_messages`
- `runtime_multimodal_summaries`

### Domain Boundaries

- `chat_*`
  - 会话与轮次主数据
- `user_*`
  - 跨 agent 共享的用户长期记忆
- `memory_documents`
  - agent 私有稳定文档与 wiki 页面
- `memory_events`
  - ingest、daily、open loop、review 等事件流
- `memory_sources`
  - 原始来源与附件证据
- `runtime_*`
  - PI 运行时缓存；不是长期业务真相

### File Projection Policy

下列文件保留，但改为 SQLite 投影：

- `MEMORY.md`
- `USER_MODEL.md`
- `RELATIONSHIP_MAP.md`
- `WORKING.md`
- `DECISIONS.md`
- `PITFALLS.md`
- `PUBLIC_CONTEXT.md`
- `memory/YYYY-MM-DD.md`
- `memory/DAILY_INDEX.md`
- `memory/SOURCE_INDEX.md`
- `wiki/INDEX.md`
- `wiki/*.md`

短期策略：

- 读：SQLite-first，文件兜底
- 写：SQLite-first，文件回写
- 修复：支持从 SQLite 重建投影

## Schema Design

### Phase 1 Required Tables

#### `chat_sessions`

- `id TEXT PRIMARY KEY`
- `title TEXT NOT NULL`
- `status TEXT NOT NULL`
- `created_at INTEGER NOT NULL`
- `updated_at INTEGER NOT NULL`
- `agent_id TEXT`
- `agent_snapshot_json TEXT`
- `bot_target_json TEXT`
- `session_llm_provider_id TEXT`
- `session_llm_model TEXT`

索引：

- `idx_chat_sessions_updated_at`
- `idx_chat_sessions_agent_id`

#### `chat_turns`

- `id TEXT PRIMARY KEY`
- `session_id TEXT NOT NULL`
- `turn_index INTEGER NOT NULL`
- `prompt TEXT NOT NULL`
- `answer TEXT NOT NULL`
- `thinking TEXT NOT NULL DEFAULT ''`
- `status TEXT NOT NULL`
- `created_at INTEGER NOT NULL`
- `completed_at INTEGER`
- `usage_json TEXT`
- `response_segments_json TEXT`
- `tool_calls_json TEXT`
- `activity_json TEXT`

索引：

- `idx_chat_turns_session_id_turn_index`
- `idx_chat_turns_created_at`

约束：

- `(session_id, turn_index)` 唯一
- `session_id` 外键指向 `chat_sessions.id`

### Phase 2 Required Tables

#### `user_profiles`

- `id TEXT PRIMARY KEY`
- `display_name TEXT NOT NULL`
- `kind TEXT NOT NULL`
- `status TEXT NOT NULL DEFAULT 'active'`
- `created_at INTEGER NOT NULL`
- `updated_at INTEGER NOT NULL`

默认数据：

- `owner`

#### `user_profile_aliases`

- `id TEXT PRIMARY KEY`
- `profile_id TEXT NOT NULL`
- `source_kind TEXT NOT NULL`
- `source_user_key TEXT NOT NULL`
- `confidence TEXT NOT NULL DEFAULT 'confirmed'`
- `notes TEXT NOT NULL DEFAULT ''`
- `created_at INTEGER NOT NULL`
- `updated_at INTEGER NOT NULL`

索引与约束：

- `(source_kind, source_user_key)` 唯一
- `idx_user_profile_aliases_profile_id`

#### `user_memory_documents`

- `id TEXT PRIMARY KEY`
- `profile_id TEXT NOT NULL`
- `doc_type TEXT NOT NULL`
- `title TEXT NOT NULL`
- `content_md TEXT NOT NULL`
- `content_hash TEXT NOT NULL`
- `version INTEGER NOT NULL`
- `created_at INTEGER NOT NULL`
- `updated_at INTEGER NOT NULL`

唯一键：

- `(profile_id, doc_type)`

### Phase 3 Required Tables

#### `memory_documents`

- `id TEXT PRIMARY KEY`
- `agent_id TEXT NOT NULL`
- `doc_type TEXT NOT NULL`
- `title TEXT NOT NULL`
- `relative_path TEXT`
- `content_md TEXT NOT NULL`
- `content_hash TEXT NOT NULL`
- `version INTEGER NOT NULL`
- `is_projection_enabled INTEGER NOT NULL DEFAULT 1`
- `created_at INTEGER NOT NULL`
- `updated_at INTEGER NOT NULL`

`doc_type` 固定值：

- `memory`
- `user_model`
- `relationship_map`
- `working`
- `decisions`
- `pitfalls`
- `public_context`
- `wiki_index`
- `wiki_page`

唯一键：

- `(agent_id, doc_type, COALESCE(relative_path, ''))`

#### `memory_events`

- `id TEXT PRIMARY KEY`
- `agent_id TEXT NOT NULL`
- `profile_id TEXT`
- `event_type TEXT NOT NULL`
- `user_id TEXT NOT NULL`
- `session_id TEXT`
- `turn_id TEXT`
- `day_key TEXT`
- `summary TEXT NOT NULL`
- `payload_json TEXT NOT NULL`
- `source_ref TEXT`
- `created_at INTEGER NOT NULL`

`event_type` 固定值：

- `conversation_ingest`
- `attachment_registered`
- `daily_digest_entry`
- `open_loop_added`
- `review_item_added`
- `pitfall_promoted`

索引：

- `idx_memory_events_agent_id_created_at`
- `idx_memory_events_day_key`
- `idx_memory_events_session_id`
- `idx_memory_events_profile_id`

#### `memory_sources`

- `id TEXT PRIMARY KEY`
- `agent_id TEXT NOT NULL`
- `source_type TEXT NOT NULL`
- `title TEXT NOT NULL`
- `file_path TEXT NOT NULL`
- `mime_type TEXT`
- `summary TEXT NOT NULL`
- `source_hash TEXT`
- `created_at INTEGER NOT NULL`

#### `memory_source_links`

- `id TEXT PRIMARY KEY`
- `source_id TEXT NOT NULL`
- `link_type TEXT NOT NULL`
- `target_id TEXT NOT NULL`
- `created_at INTEGER NOT NULL`

`link_type` 固定值：

- `chat_session`
- `chat_turn`
- `memory_event`
- `memory_document`

### Phase 4 Required Tables

#### `runtime_sessions`

- `id TEXT PRIMARY KEY`
- `session_id TEXT NOT NULL`
- `channel_kind TEXT NOT NULL`
- `runtime_key TEXT NOT NULL`
- `status TEXT NOT NULL`
- `session_file_path TEXT`
- `last_materialized_at INTEGER`
- `created_at INTEGER NOT NULL`
- `updated_at INTEGER NOT NULL`

唯一键：

- `(channel_kind, runtime_key)`

#### `runtime_session_messages`

- `id TEXT PRIMARY KEY`
- `runtime_session_id TEXT NOT NULL`
- `seq INTEGER NOT NULL`
- `role TEXT NOT NULL`
- `payload_json TEXT NOT NULL`
- `created_at INTEGER NOT NULL`

#### `runtime_multimodal_summaries`

- `id TEXT PRIMARY KEY`
- `runtime_session_id TEXT NOT NULL`
- `timestamp_ms INTEGER NOT NULL`
- `user_prompt TEXT NOT NULL`
- `assistant_response TEXT NOT NULL`
- `created_at INTEGER NOT NULL`

## API and Module Changes

### Rust Storage Split

把当前 `src-tauri/src/lib.rs` 中与历史数据库相关的逻辑拆成独立模块：

- `storage/chat_history.rs`
- `storage/user_memory.rs`
- `storage/agent_memory.rs`
- `storage/runtime_sessions.rs`
- `storage/projection.rs`
- `storage/migrations.rs`

目标：

- `lib.rs` 只保留命令入口
- schema 与查询逻辑不再继续堆在主文件

### Replace Existing History APIs

下线：

- `load_history_state`
- `save_history_state`
- `clear_history_state`

新增：

- `list_chat_sessions`
- `get_chat_session`
- `create_chat_session`
- `append_chat_turn`
- `update_chat_turn`
- `delete_chat_session`
- `clear_all_chat_sessions`

### Add Memory APIs

新增：

- `get_agent_memory_bundle`
- `update_memory_document`
- `append_memory_event`
- `register_memory_source`
- `rebuild_memory_projections`
- `import_memory_projections`

### Add Shared User Memory APIs

新增：

- `list_user_profiles`
- `get_user_profile`
- `upsert_user_profile_document`
- `bind_user_profile_alias`
- `resolve_user_profile_for_source`

默认策略：

- 桌面来源统一映射 `desktop:owner -> owner`

### Add Runtime Session APIs

新增：

- `get_or_create_runtime_session`
- `append_runtime_session_message`
- `load_runtime_session_messages`
- `materialize_runtime_session_file`
- `clear_runtime_session`
- `record_runtime_multimodal_summary`
- `load_runtime_multimodal_summaries`

## Implementation Phases

## Phase 1: Introduce Structured Chat History

### Goal

彻底替换 `history_v1`，让聊天历史先结构化落地。

### Changes

- 增加 `chat_sessions` / `chat_turns`
- 写迁移逻辑：从 `app_state.history_v1` 导入
- 前端 `usePiAgent` 改为 session / turn 增量读写
- 删除继续写回 `history_v1` 的逻辑
- `token_usage_records` 保持兼容，但从 `chat_turns.usage_json` 派生更新

### Acceptance

- 启动后可自动迁移旧历史
- 新会话不再进入 `history_v1`
- 历史列表与会话详情均从结构化表读取
- `clear` / `delete session` 不再操作 blob 快照

## Phase 2: Shared User Memory Introduction

### Goal

建立共享用户记忆层，避免多 agent 用户画像分叉。

### Changes

- 增加 `user_profiles` / `user_profile_aliases` / `user_memory_documents`
- 初始化默认 `owner` profile
- 桌面对话默认映射到 `owner`
- 为 IM / peer 预留 alias 显式绑定能力
- 先只落库，不强制前端暴露完整管理 UI

### Acceptance

- 桌面来源能稳定解析到 `owner`
- 共享用户记忆可独立查询和更新
- agent 私有记忆与共享记忆仍分层

## Phase 3: Agent Memory and Wiki Move to SQLite

### Goal

把当前简化后的 memory system 迁到 SQLite-first。

### Changes

- 增加 `memory_documents` / `memory_events` / `memory_sources` / `memory_source_links`
- 将现有核心文档导入 `memory_documents`
- 将 `append_agent_memory_entry()` 改为：
  - 先写 `memory_events`
  - 再更新 `working` / `pitfalls` 对应 document
  - 再登记 `memory_sources`
  - 最后按需回写投影文件
- `register_agent_attachment_source()` 改为 SQLite-first
- `wiki/*.md` 导入 `memory_documents`

### Acceptance

- ingest 不再先改文件再推断状态
- 文件不存在时核心记忆仍可从 SQLite 读出
- wiki 页面可从 SQLite 读写

## Phase 4: Runtime Session SQLite-Backed Cache

### Goal

让 session `jsonl` 和多模态摘要文件退出主存储角色。

### Changes

- 增加 `runtime_sessions` / `runtime_session_messages` / `runtime_multimodal_summaries`
- 当前 `session_file_path()` / `summary_file_path()` 继续保留，但只负责物化路径
- `stream_pi_prompt` 前：
  - 从 SQLite 取 runtime session
  - 物化 `jsonl`
  - 传给 PI
- session 清理时：
  - 先删 `runtime_*` 记录
  - 再删物化文件

### Acceptance

- runtime 恢复和清理可通过 SQLite 驱动
- 临时文件损坏时可重新物化
- 产品逻辑不再直接依赖临时文件作为真源

## Phase 5: Projection Layer and Workspace UI Flip

### Goal

让 Workspace UI 和 system prompt 读取切到 SQLite-first。

### Changes

- `read_agent_workspace_bundle()` 改为优先从 SQLite 生成内容
- `read_agent_workspace_file()` 改为优先读 SQLite 文档，再决定是否回退文件
- `write_agent_workspace_file()` 改为优先更新 SQLite，再写投影
- `build_workspace_system_prompt_for_query()` 改为 SQLite-first 组装
- `build_daily_digest_retrieval_snapshot()` 改为查 `memory_events`
- `build_specialized_memory_snapshot()` 改为查 `memory_documents`

### Acceptance

- 缺少投影文件时，Workspace UI 仍能工作
- system prompt 不再依赖文件存在
- 手工编辑文件后可同步回 SQLite 或提示冲突

## Phase 6: Search Foundation

### Goal

为后续 FTS5 / `sqlite-vec` 奠定稳定主数据源。

### Changes

- 增加统一 search document build hooks
- 把聊天、共享用户记忆、agent 文档、wiki、daily 事件映射成可索引材料
- 本阶段只要求准备统一数据出口，不要求立即做完整搜索 UI

### Acceptance

- 所有核心域都能导出为一致的检索文档模型
- 后续引入 FTS5 / `sqlite-vec` 时无需再回头清洗主存储

## Migration Mechanics

### One-Time Import Order

严格按以下顺序执行：

1. 创建新表
2. 导入 `history_v1`
3. 导入 agent 核心记忆文档
4. 导入 wiki 页面
5. 导入 `memory/YYYY-MM-DD.md` 与 `DAILY_INDEX.md`
6. 导入 `SOURCE_INDEX.md` 与 raw source metadata
7. 初始化 `owner` profile
8. 建立桌面来源 alias
9. 切换读路径
10. 切换写路径
11. 停止写 `history_v1`

### Projection Rebuild Commands

必须提供：

- `rebuild_sqlite_from_files`
- `rebuild_files_from_sqlite`
- `verify_projection_integrity`

### Conflict Policy

第一版默认策略：

- SQLite 与文件冲突时，以 SQLite 为准
- 若文件修改时间晚于 SQLite 且内容 hash 不同，记录 warning，并要求显式导入

## Frontend Changes

### History Flow

- `usePiAgent` 去掉“整份 history 快照”心智模型
- 历史列表单独加载
- 会话详情按需加载
- 删除 / 清空直接调用结构化 API

### Workspace Flow

- workspace 面板继续展示文件形态，但数据来自 SQLite
- `lazy_fetch` 保留
- UI 不暴露“这是数据库文档还是投影文件”的差异

### User Memory Visibility

第一阶段前端只要求：

- 系统内部能解析到 `owner`
- 不强制开放完整 shared user memory 管理页

## Test Plan

### Rust Tests

- 历史迁移：`history_v1 -> chat_sessions/chat_turns`
- 文档读写：`memory_documents`
- 事件写入：`memory_events`
- 运行态 session 物化与清理
- user profile alias 解析
- projection rebuild 幂等性

### Frontend Tests

- 历史列表与会话详情改为增量加载后不回归
- 删除会话 / 清空历史行为不回归
- workspace 核心文档读取不依赖本地文件存在

### Manual Verification

- 旧历史自动迁移
- 现有 agent workspace 能正常打开
- 新聊天、附件、wiki 编辑都能落到 SQLite
- 删除 session 后 runtime 物化文件同步清理
- 删除投影文件后系统仍可恢复显示

## Rollout Order

推荐按以下顺序合并：

1. 聊天历史结构化
2. 共享用户记忆骨架
3. agent memory / wiki SQLite-first
4. runtime session SQLite-backed
5. Workspace / prompt SQLite-first
6. search foundation

## Exit Criteria

本项目视为完成的标准：

- `history_v1` 已废弃
- 新写入全部先入 SQLite
- runtime session 文件只作为物化产物
- Workspace UI 与 system prompt 均能在“文件缺失”情况下工作
- 共享用户记忆与 agent 私有记忆均有明确表结构和读写边界
