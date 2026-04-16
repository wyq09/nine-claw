# PRD: SQLite Unified Storage for Chat, Memory, Runtime Session, and Wiki

## Overview

NineClaw 当前已经同时使用了多种持久化与运行时载体：

- 聊天历史保存在 SQLite 的 `app_state.history_v1` JSON blob 中
- Agent 记忆主要写入 `WORKING.md`、`memory/YYYY-MM-DD.md`、`memory/DAILY_INDEX.md`、`memory/SOURCE_INDEX.md`
- Wiki 以 `wiki/*.md` 文件存在
- PI 运行时上下文和多模态摘要保存在临时目录下的 `*.jsonl` / `*.json`

这套方案在功能上可用，但已经出现三个结构性问题：

1. 主数据分散在 JSON blob、Markdown 文件和临时文件之间，难以统一检索、迁移和约束。
2. 聊天历史与记忆体系缺少统一领域模型，运行时逻辑越来越依赖文件路径与文本拼接。
3. `history_v1` 是历史兼容方案，不适合作为后续功能扩展的基础。

本 PRD 的目标是将聊天历史、agent 记忆、临时 session 状态、wiki 内容统一纳入 SQLite 主存储；文件系统不再作为主真源，而退为投影层、兼容层和运行时适配层。

本需求有两个明确约束：

1. `history_v1` 可直接废弃，不需要继续兼容写回。
2. 由于当前 PI 运行时仍依赖 `--session <jsonl path>`，临时 session 文件短期内不能完全消失，但必须降级为由 SQLite 物化出来的运行时适配文件。

## Product Decision Summary

本期直接固定以下产品与架构决策：

- `history_v1` 直接废弃，不保留长期兼容写回
- SQLite 是唯一主存储
- Markdown 文件保留，但只作为投影层、兼容层和手工编辑视图
- 聊天历史、agent 私有记忆、wiki、runtime session 都进入 SQLite
- PI runtime session 文件短期继续存在，但只作为 SQLite 的运行时物化产物
- 检索、system prompt、workspace UI 后续都以 SQLite 为数据主源
- 用户记忆采用“共享用户档案 + agent 局部覆盖”的作用域模型

## Goals

- 用结构化 SQLite 替代 `history_v1` JSON blob，作为聊天历史唯一主存储。
- 用结构化 SQLite 承载 agent 记忆、证据登记、daily digest、wiki 内容和运行时 session 缓存。
- 建立统一检索基础，为后续 FTS5 和 `sqlite-vec` 提供稳定数据源。
- 将当前简化后的 memory system 映射到少数明确的 SQLite 领域表，而不是继续扩大 Markdown 文件树。
- 保留现有 Workspace UI 的可读/可编辑体验，但将 Markdown 降级为 SQLite 的投影层或兼容层。
- 将 PI runtime session 与多模态 summary 纳入 SQLite 管理，并支持在需要时物化为临时文件。
- 为迁移、降级、重建和数据一致性提供明确路径。

## Quality Gates

These commands must pass for every user story:

- `npm run lint`
- `npm run build`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- 对聊天历史迁移、记忆写回、wiki 读写、session runtime 物化需要补充桌面端人工回归
- 对 FTS5 / `sqlite-vec` 接入前的基础结构改造，需要补充迁移库与旧数据导入的手工验证

说明：

- 当前仓库已经使用 `rusqlite`，新增能力必须在现有 SQLite 连接与迁移体系中落地。
- 受保护文件 `src-tauri/resources/pi-runtime/macos/pi` 不在本 PRD 范围内，任何实现不得修改它。

## Current State and Problem Statement

### Chat History

- 前端仍通过 `loadHistoryState()` / `saveHistoryState()` 读写整个历史快照。
- SQLite 中 `app_state.history_v1` 只是一个大 JSON 字符串，不具备结构化查询能力。
- 历史搜索仍以 UI 端字符串拼接和 `includes()` 过滤为主。

### Agent Memory

- 当前记忆系统已经简化，不再以大量 `memory/categories/*.md` 作为主路径。
- 运行时主要依赖：
  - `MEMORY.md`
  - `USER_MODEL.md`
  - `RELATIONSHIP_MAP.md`
  - `WORKING.md`
  - `DECISIONS.md`
  - `PITFALLS.md`
  - `memory/DAILY_INDEX.md`
  - `memory/SOURCE_INDEX.md`
  - `memory/YYYY-MM-DD.md`
- `wiki/*.md`
- ingest 流程仍直接写文件，运行时 prompt 组装仍按文件路由。

### User Memory Scope

- 当前用户相关信息主要散落在 agent 私有空间内。
- 不同 agent 会重复维护用户偏好、人物关系和上下文判断，存在分叉风险。
- 本期不再按“每个 agent 独立一份用户画像”设计，而采用共享用户档案模型。

### Runtime Session

- 桌面聊天和 IM / peer 通道都依赖 session 文件持有 PI 上下文。
- 多模态摘要单独保存在临时目录 JSON 文件中。
- 这些文件本质上是运行时缓存，不应继续被视为长期持久化真源。

### Why Now

- `history_v1` 已经明确可以废弃，迁移成本显著降低。
- memory system 已经完成一轮简化，适合趁当前心智模型尚清晰时收敛到 SQLite。
- 后续统一检索、上下文窗口治理、跨层记忆路由都需要结构化主存储。

## Target Architecture

### Core Principle

统一采用：

- `SQLite-first`
- `Files-compatible`
- `Runtime-session-materialized`

解释：

- SQLite 是主存储、主检索、主迁移入口
- Markdown 作为可编辑投影或兼容视图，而不是唯一真源
- runtime session 文件只在 PI 进程需要时由 SQLite 物化生成

### Memory Scope Model

统一采用双层记忆作用域：

- Shared user memory
  - 记录“这个用户本身是谁、长期偏好什么、与哪些关键人物相关”
- Agent private memory
  - 记录“这个用户与该 agent 如何协作、该 agent 当前任务如何推进”

边界如下：

- 共享用户层
  - `PROFILE`
  - `PREFERENCES`
  - `RELATIONSHIPS`
  - `COLLABORATION`
  - `INFERENCES`
- Agent 私有层
  - `MEMORY`
  - `USER_MODEL`
  - `RELATIONSHIP_MAP`
  - `WORKING`
  - `DECISIONS`
  - `PITFALLS`
  - `PUBLIC_CONTEXT`
  - `wiki`

桌面默认身份策略：

- 桌面对话默认绑定固定 `owner` user profile
- IM / peer 身份通过显式 alias 绑定归并到共享用户档案
- 不做文本启发式自动合并

### Storage Layers

#### 1. Chat Data Layer

聊天历史主存储。

建议表：

- `chat_sessions`
- `chat_turns`

#### 2. Shared User Memory Layer

共享用户档案与长期用户模型。

建议表：

- `user_profiles`
- `user_profile_aliases`
- `user_memory_documents`

文档类型至少包括：

- `profile`
- `preferences`
- `relationships`
- `collaboration`
- `inferences`

#### 3. Memory Document Layer

稳定档案与可编辑知识文档。

建议表：

- `memory_documents`

文档类型至少包括：

- `memory`
- `user_model`
- `relationship_map`
- `working`
- `decisions`
- `pitfalls`
- `public_context`
- `wiki_index`
- `wiki_page`

#### 4. Memory Event Layer

时间序列事件，不与稳定档案混表。

建议表：

- `memory_events`

事件类型至少包括：

- `conversation_ingest`
- `attachment_registered`
- `daily_digest_entry`
- `open_loop_added`
- `review_item_added`
- `pitfall_promoted`

#### 5. Evidence Layer

原始来源和附件登记。

建议表：

- `memory_sources`
- `memory_source_links`

#### 6. Runtime Cache Layer

PI 运行时 session 与多模态摘要缓存。

建议表：

- `runtime_sessions`
- `runtime_session_messages`
- `runtime_multimodal_summaries`

### Suggested Table Ownership

核心 ownership 固定如下：

- `chat_sessions` / `chat_turns`
  - 聊天主数据
- `user_profiles` / `user_profile_aliases` / `user_memory_documents`
  - 共享用户记忆
- `memory_documents`
  - agent 私有记忆与 wiki 文档
- `memory_events`
  - ingest、daily、review、open loop 等事件流
- `memory_sources`
  - 原始证据与附件来源
- `runtime_sessions` / `runtime_session_messages` / `runtime_multimodal_summaries`
  - PI 运行态缓存

## Data Model Requirements

### FR-1: Chat History Must Be Fully Structured

系统必须用结构化表替代 `history_v1`：

- `chat_sessions`
  - `id`
  - `title`
  - `status`
  - `created_at`
  - `updated_at`
  - `agent_snapshot_json`
  - `bot_target_json`
  - `session_llm_provider_id`
  - `session_llm_model`
- `chat_turns`
  - `id`
  - `session_id`
  - `turn_index`
  - `prompt`
  - `answer`
  - `thinking`
  - `status`
  - `created_at`
  - `completed_at`
  - `usage_json`
  - `response_segments_json`
  - `tool_calls_json`
  - `activity_json`

### FR-2: `history_v1` Must Be Removed from the Main Path

系统必须：

- 停止继续写入 `app_state.history_v1`
- 启动迁移时读取一次旧数据并导入结构化表
- 迁移成功后不再依赖 `history_v1`
- 新接口不再围绕“整个历史快照字符串”设计

### FR-3: Memory Must Use Domain Tables Instead of File-Tree Semantics

系统不得继续把 SQLite 设计成“文件树镜像表”。

系统必须按职责建模：

- `memory_documents`
  - 稳定文档与可编辑档案
- `memory_events`
  - 流水事件
- `memory_sources`
  - 原始证据

### FR-3a: Shared User Memory Must Be First-Class

系统必须把共享用户记忆建成一级模型，而不是继续隐式散落在 agent 私有记忆中。

要求：

- 桌面对话默认绑定 `owner` profile
- 共享用户记忆与 agent 私有记忆必须分表
- 共享层可被多个 agent 读取
- agent 局部规则不得反向污染共享用户档案

### FR-4: Working State Must Remain Separate from Stable Memory

`WORKING.md` 对应的数据必须与稳定档案分离：

- 当前 focus
- open loops
- pending review
- latest context

这些内容必须保留为“执行态”，不得混入 `MEMORY.md` / `USER_MODEL.md` / `RELATIONSHIP_MAP.md` 的长期层。

### FR-5: Daily Digest Must Become SQLite-Backed

系统必须将 daily digest 和 daily index 纳入 SQLite：

- `memory_events` 保存每日 digest 条目
- `memory/DAILY_INDEX.md` 变为可选投影
- `memory/YYYY-MM-DD.md` 变为可选投影或导出视图

### FR-6: Source Index and Raw Evidence Must Be Structured

系统必须为原始来源建立结构化登记：

- 来源类型
- 绝对路径
- MIME type
- 关联 agent
- 关联 session / turn / memory event
- 登记时间
- 摘要

`memory/SOURCE_INDEX.md` 可以保留，但只能作为投影层。

### FR-7: Wiki Must Be SQLite-Backed

系统必须将 wiki 内容纳入 SQLite 主存储：

- `wiki/INDEX.md` 和 `wiki/*.md` 的内容进入 `memory_documents`
- 文档类型区分 `wiki_index` 与 `wiki_page`
- 文件系统 wiki 页面可保留手动编辑体验，但不再是唯一真源

### FR-8: Runtime Session Must Be SQLite-Managed

系统必须将以下内容纳入 SQLite：

- session 元数据
- session 消息序列
- 多模态摘要条目
- 最后更新时间与生存状态

但在当前阶段：

- PI 运行前仍需从 SQLite 物化临时 `jsonl`
- 多模态摘要仍可在进程调用前拼装为文本上下文
- 清理 session 时应先清 SQLite 运行态，再清物化文件

### FR-8a: Runtime Session Files Are Not Long-Term Source of Truth

系统必须明确：

- 临时 session `jsonl` 不是长期存储
- 多模态 summary 临时 JSON 不是长期存储
- 两者都可由 SQLite 重新生成
- 任何面向产品的统计、搜索、恢复都不得再直接依赖这些临时文件

### FR-9: Workspace UI Must Continue to Work

Agent Workspace UI 仍需支持：

- 查看核心记忆文件
- 编辑核心记忆文件
- 查看 wiki 页面
- 查看 source index / daily index

但其后端数据来源改为 SQLite：

- 读取时优先从 SQLite 生成文档内容
- 写入时优先更新 SQLite，再回写投影文件

### FR-10: Search Must Target SQLite Data

后续检索必须以 SQLite 为基础：

- 聊天历史检索不再扫 `history_v1`
- 记忆检索不再依赖逐个打开 Markdown 文件
- daily index、source index、wiki 都从统一 SQLite 数据源生成检索材料

### FR-10a: Prompt Assembly Must Move to SQLite-First Reads

运行时 system prompt 组装必须逐步从“按文件路径读 Markdown”迁到“先读 SQLite，再按需生成文本片段”。

第一阶段允许：

- 仍调用现有文件路由函数
- 但这些函数的内容来源必须优先来自 SQLite

第二阶段目标：

- prompt 组装不再依赖文件存在

### FR-11: Projection Files Must Be Optional and Rebuildable

系统必须允许以下场景：

- SQLite 为主，文件不存在时仍能工作
- 文件损坏后可从 SQLite 重建
- 用户手工编辑文件后能同步回 SQLite

第一阶段允许：

- 某些文件仍作为兼容层存在
- 但实现中必须明确“SQLite 是主、文件是投影”

### FR-12: Projection Drift Must Be Detectable

系统必须具备投影漂移检测能力：

- SQLite 文档内容版本或哈希
- 文件投影内容版本或哈希
- rebuild 时以 SQLite 为准
- 手工编辑文件时，能检测并回写 SQLite 或提示冲突

## Migration Plan

### Phase 1: Chat History Migration

- 新增 `chat_sessions` / `chat_turns`
- 启动时从 `history_v1` 导入
- 迁移成功后前端改为按 session / turn 读写
- 移除继续写回 `history_v1` 的逻辑

### Phase 2: Memory and Wiki Table Introduction

- 新增 `memory_documents` / `memory_events` / `memory_sources`
- 新增 `user_profiles` / `user_profile_aliases` / `user_memory_documents`
- 将当前核心记忆文件与 wiki 文档导入 SQLite
- 将 `WORKING` / `DAILY_INDEX` / `SOURCE_INDEX` 的写路径迁到 SQLite
- 将桌面对话默认映射到固定 `owner` profile

### Phase 3: Runtime Session SQLite-Backed Cache

- 新增 `runtime_sessions` / `runtime_session_messages` / `runtime_multimodal_summaries`
- 将 session 清理、恢复、summary 读写改为 SQLite-first
- 保留对 `--session <jsonl path>` 的运行时物化

### Phase 4: Projection Rebuild and UI Flip

- Workspace UI 改为 SQLite-first
- 文件写回改为投影生成
- 提供全量 rebuild 命令：
  - rebuild documents to SQLite
  - rebuild projections from SQLite

## User Stories

### US-001: Replace `history_v1` with structured chat tables
**Description:** As a developer, I want to remove `history_v1` from the main path so that chat history can be queried and evolved safely.

**Acceptance Criteria:**
- [ ] `history_v1` 不再作为主读写路径
- [ ] 聊天历史能按 session / turn 查询
- [ ] 旧历史可迁移导入
- [ ] 删除会话只影响结构化表，不再操作 blob 快照

### US-002: Persist agent memory in SQLite
**Description:** As a system, I want memory writes to land in SQLite first so that retrieval and consistency no longer depend on ad hoc markdown updates.

**Acceptance Criteria:**
- [ ] `append_agent_memory_entry()` 改为写 SQLite 主表
- [ ] `WORKING`、daily、source registration 由 SQLite 驱动
- [ ] 现有记忆简化后的层次不被重新打散成 category 表

### US-002a: Introduce shared user memory
**Description:** As a system, I want a shared user-memory scope so that multiple agents can reuse one stable user profile without duplicating or diverging it.

**Acceptance Criteria:**
- [ ] 存在共享 user profile 主表
- [ ] 桌面对话默认归到 `owner`
- [ ] alias 绑定可把 IM / peer 身份显式归并到同一 profile
- [ ] 共享用户记忆与 agent 私有记忆检索时可并存但不混写

### US-003: Persist wiki in SQLite
**Description:** As a user, I want wiki pages stored in SQLite so that they can participate in the same storage and search system as memory.

**Acceptance Criteria:**
- [ ] wiki index 与 wiki pages 都进入 SQLite
- [ ] 仍可在 Workspace UI 中查看和编辑 wiki 内容
- [ ] 文件系统 wiki 页面可由 SQLite 重建

### US-004: Persist runtime session state in SQLite
**Description:** As a developer, I want runtime session and multimodal summaries managed in SQLite so that temp files stop being the primary record.

**Acceptance Criteria:**
- [ ] session metadata 和消息缓存在 SQLite 中
- [ ] 多模态 summary 进入 SQLite
- [ ] PI 调用前可从 SQLite 物化所需 `jsonl`
- [ ] 清理 session 时 SQLite 与物化文件都能同步清理
- [ ] 产品逻辑不再直接依赖 `jsonl` / summary 临时文件

### US-005: Keep Workspace UI compatible during transition
**Description:** As a user, I want agent workspace pages to keep working while storage moves under the hood.

**Acceptance Criteria:**
- [ ] 核心记忆文件和 wiki 页面仍可展示
- [ ] 编辑操作不会因为 SQLite 化而丢失
- [ ] UI 不需要感知底层是否来自 SQLite 还是投影文件

### US-006: Rebuild and repair storage
**Description:** As an operator, I want rebuild commands so that SQLite and projection files can be repaired if they drift.

**Acceptance Criteria:**
- [ ] 支持从文件重建 SQLite
- [ ] 支持从 SQLite 重建文件投影
- [ ] rebuild 过程可重复执行且不产生重复脏数据

### US-007: Move prompt assembly to SQLite-first data access
**Description:** As a developer, I want workspace prompt assembly to read from SQLite-first sources so that runtime memory loading no longer depends on physical markdown files.

**Acceptance Criteria:**
- [ ] `WORKING`、`DECISIONS`、`PITFALLS`、`USER_MODEL`、`RELATIONSHIP_MAP` 的读取可由 SQLite 生成
- [ ] `DAILY_INDEX` 与 `SOURCE_INDEX` 的摘要快照可由 SQLite 生成
- [ ] 缺少投影文件时 system prompt 仍可工作

## Non-Goals (Out of Scope)

- 本期不修改受保护的 `src-tauri/resources/pi-runtime/macos/pi`
- 本期不强制删除所有 Markdown 文件
- 本期不要求 PI runtime 原生支持 SQLite session 输入
- 本期不在一个版本里完成所有投影文件删除
- 本期不重新引入旧的 category memory 主路径
- 本期不做启发式自动用户身份合并
- 本期不要求一次性把全部 workspace 读取逻辑改成无文件模式

## Open Implementation Notes

- 当前 memory system 已经简化，SQLite 建模应围绕现有主路径展开，而不是复活旧 category shards。
- `history_v1` 直接废弃是本次方案的重要前提，不再为它设计长期兼容接口。
- runtime session 文件是当前架构的外部接口适配层，迁移时应明确其“缓存/物化”定位，避免误判为长期存储。
- 共享用户记忆不是 agent 私有记忆的替代，而是它的上层复用层；实现时必须防止两层相互污染。
- 第一阶段的成功标准不是“没有文件”，而是“即使没有文件，SQLite 主路径也能恢复核心能力”。
