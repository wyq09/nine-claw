# SQLite Unified Storage: Schema Specification

## Goal

本文件是 [PRD_SQLITE_UNIFIED_STORAGE.md](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/docs/PRD_SQLITE_UNIFIED_STORAGE.md:1) 和 [IMPLEMENTATION_PLAN_SQLITE_UNIFIED_STORAGE.md](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/docs/IMPLEMENTATION_PLAN_SQLITE_UNIFIED_STORAGE.md:1) 的数据库细则，直接定义：

- 表
- 字段
- 索引
- 唯一键
- 数据来源
- 投影策略

该文档默认：

- `history_v1` 废弃
- SQLite 是唯一主真源
- Markdown 文件是 projection

## Naming Rules

- 表名采用 snake_case 复数
- 主键统一使用 `TEXT`
- 时间统一使用 `INTEGER` 毫秒时间戳
- 状态字段统一使用 `TEXT`
- 可变结构统一存 `*_json TEXT`
- 投影内容哈希统一存 `content_hash TEXT`

## Chat Tables

### `chat_sessions`

用途：

- 聊天会话主表

字段：

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

约束：

- `status` 只允许：
  - `running`
  - `done`
  - `error`
  - `aborted_user`
  - `aborted_model`

索引：

- `idx_chat_sessions_updated_at` on `(updated_at DESC)`
- `idx_chat_sessions_agent_id` on `(agent_id)`

来源：

- 当前 `HistoryItem`

### `chat_turns`

用途：

- 聊天轮次主表

字段：

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

约束：

- 外键：`session_id -> chat_sessions.id`
- 唯一键：`(session_id, turn_index)`

索引：

- `idx_chat_turns_session_id_turn_index`
- `idx_chat_turns_created_at`

来源：

- 当前 `ConversationTurn`

### `chat_turn_attachments`

用途：

- 会话内附件与 turn 的关联

字段：

- `id TEXT PRIMARY KEY`
- `turn_id TEXT NOT NULL`
- `attachment_id TEXT NOT NULL`
- `created_at INTEGER NOT NULL`

外键：

- `turn_id -> chat_turns.id`
- `attachment_id -> memory_sources.id`

## Shared User Memory Tables

### `user_profiles`

用途：

- 共享用户档案主表

字段：

- `id TEXT PRIMARY KEY`
- `display_name TEXT NOT NULL`
- `kind TEXT NOT NULL`
- `status TEXT NOT NULL DEFAULT 'active'`
- `created_at INTEGER NOT NULL`
- `updated_at INTEGER NOT NULL`

固定初始记录：

- `id='owner'`

### `user_profile_aliases`

用途：

- 不同来源身份到 profile 的映射

字段：

- `id TEXT PRIMARY KEY`
- `profile_id TEXT NOT NULL`
- `source_kind TEXT NOT NULL`
- `source_user_key TEXT NOT NULL`
- `confidence TEXT NOT NULL DEFAULT 'confirmed'`
- `notes TEXT NOT NULL DEFAULT ''`
- `created_at INTEGER NOT NULL`
- `updated_at INTEGER NOT NULL`

唯一键：

- `(source_kind, source_user_key)`

索引：

- `idx_user_profile_aliases_profile_id`

默认记录：

- `source_kind='desktop'`
- `source_user_key='owner'`
- `profile_id='owner'`

### `user_memory_documents`

用途：

- 共享用户长期文档

字段：

- `id TEXT PRIMARY KEY`
- `profile_id TEXT NOT NULL`
- `doc_type TEXT NOT NULL`
- `title TEXT NOT NULL`
- `content_md TEXT NOT NULL`
- `content_hash TEXT NOT NULL`
- `version INTEGER NOT NULL`
- `projection_path TEXT`
- `projection_enabled INTEGER NOT NULL DEFAULT 0`
- `created_at INTEGER NOT NULL`
- `updated_at INTEGER NOT NULL`

`doc_type` 固定值：

- `profile`
- `preferences`
- `relationships`
- `collaboration`
- `inferences`

唯一键：

- `(profile_id, doc_type)`

## Agent Private Memory Tables

### `memory_documents`

用途：

- agent 私有稳定文档与 wiki 文档

字段：

- `id TEXT PRIMARY KEY`
- `agent_id TEXT NOT NULL`
- `doc_type TEXT NOT NULL`
- `title TEXT NOT NULL`
- `relative_path TEXT`
- `content_md TEXT NOT NULL`
- `content_hash TEXT NOT NULL`
- `version INTEGER NOT NULL`
- `projection_enabled INTEGER NOT NULL DEFAULT 1`
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

索引：

- `idx_memory_documents_agent_id`
- `idx_memory_documents_doc_type`

### `memory_events`

用途：

- ingest 和执行态事件流

字段：

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
- `idx_memory_events_session_id`
- `idx_memory_events_turn_id`
- `idx_memory_events_day_key`
- `idx_memory_events_profile_id`

说明：

- `memory/YYYY-MM-DD.md` 和 `memory/DAILY_INDEX.md` 都由这里投影
- `WORKING.md` 的 open loops / review 由这里派生更新

### `memory_sources`

用途：

- 原始来源与附件主表

字段：

- `id TEXT PRIMARY KEY`
- `agent_id TEXT NOT NULL`
- `source_type TEXT NOT NULL`
- `title TEXT NOT NULL`
- `file_path TEXT NOT NULL`
- `mime_type TEXT`
- `summary TEXT NOT NULL`
- `source_hash TEXT`
- `created_at INTEGER NOT NULL`

`source_type` 固定值：

- `conversation_raw`
- `attachment`
- `external_file`
- `generated_source`

索引：

- `idx_memory_sources_agent_id_created_at`
- `idx_memory_sources_source_type`

### `memory_source_links`

用途：

- 原始来源到业务对象的关联

字段：

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

索引：

- `idx_memory_source_links_source_id`
- `idx_memory_source_links_target_id`

## Runtime Tables

### `runtime_sessions`

用途：

- PI runtime session 主表

字段：

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

`channel_kind` 典型值：

- `desktop`
- `wechat`
- `lark`
- `peer`

### `runtime_session_messages`

用途：

- 运行态消息序列

字段：

- `id TEXT PRIMARY KEY`
- `runtime_session_id TEXT NOT NULL`
- `seq INTEGER NOT NULL`
- `role TEXT NOT NULL`
- `payload_json TEXT NOT NULL`
- `created_at INTEGER NOT NULL`

唯一键：

- `(runtime_session_id, seq)`

### `runtime_multimodal_summaries`

用途：

- 多模态摘要缓存

字段：

- `id TEXT PRIMARY KEY`
- `runtime_session_id TEXT NOT NULL`
- `timestamp_ms INTEGER NOT NULL`
- `user_prompt TEXT NOT NULL`
- `assistant_response TEXT NOT NULL`
- `created_at INTEGER NOT NULL`

索引：

- `idx_runtime_multimodal_summaries_runtime_session_id_timestamp`

## Projection Metadata

### `document_projections`

用途：

- 跟踪 SQLite 文档与文件投影之间的一致性

字段：

- `id TEXT PRIMARY KEY`
- `scope_kind TEXT NOT NULL`
- `scope_id TEXT NOT NULL`
- `document_id TEXT NOT NULL`
- `file_path TEXT NOT NULL`
- `sqlite_hash TEXT NOT NULL`
- `file_hash TEXT`
- `last_sqlite_write_at INTEGER NOT NULL`
- `last_file_write_at INTEGER`
- `status TEXT NOT NULL`

`status` 固定值：

- `in_sync`
- `sqlite_ahead`
- `file_ahead`
- `missing_file`
- `conflict`

## Search Foundation Tables

第一阶段可不启用，但 schema 预留：

### `search_documents`

- `id TEXT PRIMARY KEY`
- `source_kind TEXT NOT NULL`
- `source_id TEXT NOT NULL`
- `scope_kind TEXT NOT NULL`
- `scope_id TEXT`
- `title TEXT NOT NULL`
- `body TEXT NOT NULL`
- `path TEXT`
- `anchor TEXT`
- `created_at INTEGER NOT NULL`
- `updated_at INTEGER NOT NULL`

### `search_embeddings`

- `id TEXT PRIMARY KEY`
- `document_id TEXT NOT NULL`
- `provider_id TEXT NOT NULL`
- `model TEXT NOT NULL`
- `dims INTEGER NOT NULL`
- `embedding BLOB NOT NULL`
- `content_hash TEXT NOT NULL`
- `updated_at INTEGER NOT NULL`

## Migration Notes

### Drop-In Compatibility

- `token_usage_records` 暂时保留
- `app_state` 暂时保留给 provider preferences 等非历史字段
- `history_v1` 迁移完成后可删除其数据行

### Import Order

严格顺序：

1. `chat_sessions` / `chat_turns`
2. `user_profiles` / aliases
3. `memory_documents`
4. `memory_events`
5. `memory_sources`
6. `runtime_*`
7. `document_projections`
8. `search_*`

## Exit Criteria

本 schema 视为可实施的标准：

- 没有任何核心域继续依赖单个 JSON blob 做主存储
- 没有任何核心域仍然只能存在于文件系统
- runtime session 与多模态 summary 已有明确结构化承载位置
