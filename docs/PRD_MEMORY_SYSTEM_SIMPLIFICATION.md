[PRD]
# PRD: NineClaw 记忆系统简化与三层收敛

## Overview

当前 NineClaw 的记忆系统已经具备 raw source、daily log、category memory、review queue、wiki、lint/index 等多层结构，但实际运行路径与文件结构之间存在明显脱节：

- 自动 ingest 主要写入 `WORKING.md`、`PITFALLS.md`、`memory/REVIEW_QUEUE.md`、`memory/YYYY-MM-DD.md`、`memory/raw/`、`memory/SOURCE_INDEX.md`
- `DECISIONS.md`、多数 `memory/categories/*.md`、`memory/LINT.md`、`memory/LOG.md` 更偏治理或备用层，并非高频主路径
- 记忆路由、模板脚手架、前端 workspace 展示都围绕现有复杂结构展开，导致维护面过大

本次重构目标是参考“三层记忆”思路，在尽量保留现有命名习惯的前提下，大幅减少模块数量，明确主路径，降低维护复杂度，并同步收敛后端写入逻辑、prompt 路由和前端 workspace 展示。

## Goals

- 将当前记忆系统收敛为少数高频、职责明确的核心层
- 减少重复文件与重复写入逻辑，降低长期维护成本
- 保留对话证据追溯能力与历史检索能力
- 让 prompt 路由与实际存储结构一致，避免“设计层多于使用层”
- 同步调整 Agent Workspace UI，使文件分组与新的记忆模型保持一致
- 为后续实现分阶段迁移提供明确边界与验收标准

## Quality Gates

这些门槛适用于每个用户故事：

- `npm run lint`
- `cargo test --manifest-path src-tauri/Cargo.toml agent_workspace`

对于涉及 UI 或工作区交互的故事，还必须完成以下手动验收：

- 在 Agent Workspace 弹窗中确认文件分组与新架构一致
- 确认可编辑文件、lazy load 行为和保存行为未回归
- 确认记忆相关系统提示只引用保留层，不再提示已下线层

对于最终集成阶段，还应补充一次端到端手动验收：

- 桌面对话触发一次 ingest，确认写入路径、检索路径与 workspace 展示一致

## User Stories

### US-001: 明确目标记忆架构
**Description:** As a maintainer, I want a single documented target architecture so that all later code changes converge to the same simplified model.

**Acceptance Criteria:**
- [ ] 文档中明确列出保留层、合并层、删除层
- [ ] 文档中明确每一层的职责、读写入口和是否自动维护
- [ ] 文档中明确说明新架构仍沿用现有主要命名还是新增命名
- [ ] 文档中明确说明哪些旧文件属于兼容期存在，哪些会被彻底下线

### US-002: 收敛后端记忆写入主路径
**Description:** As a developer, I want ingest to write only the retained core layers so that memory writes become predictable and easier to maintain.

**Acceptance Criteria:**
- [ ] `append_agent_memory_entry()` 只写入保留层或兼容层
- [ ] 不再自动生成或刷新已决定下线的文件
- [ ] 附件导入路径与对话 ingest 路径使用同一套简化后的记忆模型
- [ ] 相关 Rust 测试覆盖新的写入目标和下线路径

### US-003: 合并重叠的执行态与待复查结构
**Description:** As a maintainer, I want open loops、commitments、review queue 的职责收敛 so that execution memory is not split across multiple overlapping files.

**Acceptance Criteria:**
- [ ] `WORKING.md`、`memory/REVIEW_QUEUE.md`、`memory/categories/commitments.md` 的职责被重新划分并形成单一路径或清晰主从关系
- [ ] 自动 ingest 只向一个主执行态区域追加待办/阻塞/复查信息
- [ ] prompt 路由不再同时指向多个表达同类状态的文件
- [ ] 兼容期策略明确：旧文件保留只读、迁移一次性写入，或直接停止生成

### US-004: 合并稳定记忆与规则层
**Description:** As a maintainer, I want stable profile and rules files to stop overlapping so that user preference, relationships, decisions, and pitfalls each have a clear home.

**Acceptance Criteria:**
- [ ] `MEMORY.md`、`USER_MODEL.md`、`RELATIONSHIP_MAP.md`、`DECISIONS.md`、`PITFALLS.md` 与 category shards 的边界被重新定义
- [ ] 已决定下线的 category shards 不再出现在 scaffold、prompt、workspace 分组中
- [ ] 稳定档案层只保留高频、长期、可复用信息
- [ ] 规则层只保留明确决策、限制和失败模式

### US-005: 精简索引与治理文件
**Description:** As a maintainer, I want non-essential governance files removed or merged so that the system does not spend effort maintaining low-value artifacts.

**Acceptance Criteria:**
- [ ] `memory/LINT.md` 被删除或改为显式按需生成，不再作为常规运行时产物
- [ ] `memory/LOG.md` 被删除，或与 `memory/SOURCE_INDEX.md` 合并成单一登记册
- [ ] `memory/categories/INDEX.md` 如不再提供运行时价值，则不再 scaffold 或展示
- [ ] 相关模板、读取逻辑、前端分组和测试同步更新

### US-006: 收敛记忆检索与系统提示
**Description:** As an agent runtime, I want prompt routing to reflect the simplified architecture so that retrieval cost and reasoning confusion both decrease.

**Acceptance Criteria:**
- [ ] `build_workspace_system_prompt_for_query()` 只引用保留层
- [ ] query snapshot 构建逻辑不再读取已下线文件
- [ ] daily retrieval 仍可通过 `memory/DAILY_INDEX.md` + `memory/YYYY-MM-DD.md` 工作
- [ ] evidence retrieval 仍可通过 `memory/SOURCE_INDEX.md` + `memory/raw/` 工作

### US-007: 同步简化 Agent Workspace UI
**Description:** As a user editing agent memory, I want the workspace dialog to show the simplified memory model so that file navigation matches actual system behavior.

**Acceptance Criteria:**
- [ ] 前端文件 section 分组与新架构一致
- [ ] 已下线或不再推荐的文件不再作为默认显著入口展示
- [ ] 默认选中文件策略与新主路径一致
- [ ] 文案中不再使用过时的“分类记忆 / lint / log”心智模型

### US-008: 提供迁移与兼容策略
**Description:** As a maintainer, I want an explicit migration path so that existing agent workspaces do not silently break during the simplification rollout.

**Acceptance Criteria:**
- [ ] 定义旧文件迁移策略：保留、只读、合并、删除
- [ ] 明确是否需要一次性迁移脚本，或在读取时惰性兼容
- [ ] 明确 agent template 如何更新，以及旧 workspace 如何处理
- [ ] 明确测试覆盖新建 agent 与已有 agent 两种路径

## Functional Requirements

1. FR-1: 系统必须定义一个简化后的目标记忆架构，并将其作为唯一主路径。
2. FR-2: 系统必须保留“稳定档案层、执行态层、历史检索层、证据层”四类核心能力。
3. FR-3: 系统必须减少重叠文件，避免同一类信息同时写入多个 markdown 文件。
4. FR-4: 系统必须继续支持通过 `memory/raw/` 和 `memory/SOURCE_INDEX.md` 追溯附件与原始对话证据。
5. FR-5: 系统必须继续支持通过 `memory/DAILY_INDEX.md` 快速检索历史摘要，再下钻到 `memory/YYYY-MM-DD.md`。
6. FR-6: 系统必须停止将低价值治理文件作为运行时默认产物，除非其仍承担明确主路径责任。
7. FR-7: 系统必须让模板脚手架、后端读写逻辑、prompt 路由、前端 workspace 展示使用同一套记忆分层定义。
8. FR-8: 系统必须支持现有 agent workspace 在迁移期内继续被读取，不因文件下线而直接损坏。
9. FR-9: 系统必须为 UI 展示提供与新架构匹配的 section 标签和默认打开策略。
10. FR-10: 系统必须为下线层定义兼容策略，避免系统提示仍然引用已废弃文件。
11. FR-11: 系统必须为“用户记忆”定义固定 schema，而不是允许稳定档案层长期自由发散。
12. FR-12: 系统必须确保“身份画像 / 工作方式 / 写作风格”三类信息可被稳定存储、编辑、检索和迁移。
13. FR-13: 任一记忆 markdown 文件不得超过 500 行；接近上限时必须拆分、归档或迁移到更合适的层。

## Non-Goals

- 本次不重做 embedding、向量数据库或外部检索基础设施
- 本次不引入新的远程存储方案
- 本次不改变多 agent 隔离规则
- 本次不重构整个 workspace 系统为数据库或非 markdown 方案
- 本次不处理与记忆系统无直接关系的 channel、scheduler、runtime 打包问题

## Technical Considerations

- 当前核心逻辑集中在 [src-tauri/src/agent_workspace.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:741) 和 [src-tauri/src/agent_workspace/memory_wiki.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace/memory_wiki.rs:84)，两者体量已过大，重构时应优先按职责拆模块。
- `npm run build` 当前会触发 `prepare:pi-runtime`，构建成本高且与本次需求不直接耦合，因此不适合作为每个故事的基础门槛。
- `DECISIONS.md` 与部分 category shards 当前存在“模板存在、检索存在、自动写入不稳定”的半活跃状态，迁移时需要先明确“主文件”。
- 前端 workspace 分组当前显式展示 `memoryIndex`、`categoryMemory`、`dailyLog`、`wiki`，需要与新分层保持一致。[src/app/agents/AgentChannelDialogs.tsx](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/app/agents/AgentChannelDialogs.tsx:370)
- 文案和 section label 目前绑定旧心智模型，需同步更新。[src/app/lib/appFormatting.tsx](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/app/lib/appFormatting.tsx:580)
- 记忆文件单文件上限为 500 行，模板、自动写入和迁移策略都必须围绕这个上限设计，不能默认无限增长。

## Success Metrics

- 核心记忆相关 markdown 文件数量明显下降
- 默认 ingest 写入目标减少为少数主路径
- prompt 构建逻辑不再依赖已下线层
- Agent Workspace 中用户可见的“记忆类分组”更少、更清楚
- 维护者可以在不阅读全部记忆模块的情况下理解写入与检索主路径

## Proposed Target Shape

在尽量保留现有命名习惯的前提下，目标架构建议为：

- 稳定档案层：`MEMORY.md`、`USER_MODEL.md`、`RELATIONSHIP_MAP.md`
- 执行态层：`WORKING.md`
- 规则层：`DECISIONS.md`、`PITFALLS.md`
- 历史检索层：`memory/YYYY-MM-DD.md`、`memory/DAILY_INDEX.md`
- 证据层：`memory/raw/`、`memory/SOURCE_INDEX.md`
- 外部知识层：`wiki/`（可选保留）

### Stable Profile Schema

稳定档案层必须覆盖以下“用户记忆”结构，并作为模板与迁移的目标 schema：

```md
# 用户身份 / Identity

## 用户身份

### 基本信息
- 称呼：
- 所在地区：

### 职业与背景

### 身边的人

### 日常偏好

## 工作方式

### 开发流程

### 组织与整理

### 调试习惯

### AI 交互偏好

## 写作风格

### 整体调性

### 句式与节奏

### 段落结构

### 表达模式
```

推荐落位如下：

- `MEMORY.md`：保留最稳定、最高频的身份锚点与协作原则
- `USER_MODEL.md`：承载“工作方式”与“写作风格”的主体 schema
- `RELATIONSHIP_MAP.md`：承载“身边的人”，并与人物关系保持独立可维护

如果后续决定进一步收敛为单一 `PROFILE.md`，该 schema 应整体迁入，不允许拆散为任意自由段落。

### Memory File Size Limit

所有记忆 markdown 文件都必须遵守 500 行上限：

- `MEMORY.md`
- `USER_MODEL.md`
- `RELATIONSHIP_MAP.md`
- `WORKING.md`
- `DECISIONS.md`
- `PITFALLS.md`
- `memory/YYYY-MM-DD.md`
- `memory/SOURCE_INDEX.md`
- `memory/DAILY_INDEX.md`

达到或接近上限时，系统必须采用以下策略之一：

- 将历史内容归档到更低频的历史文件
- 将不同职责内容拆到独立文件
- 将已闭环内容从执行态文件迁移到历史层
- 将高频摘要与低频明细分离，避免主文件持续膨胀

建议下线或并入其他层的文件：

- `memory/LINT.md`
- `memory/LOG.md`
- `memory/categories/INDEX.md`
- `memory/categories/preferences.md`
- `memory/categories/relationships.md`
- `memory/categories/pitfalls.md`
- `memory/categories/decisions.md`
- `memory/categories/commitments.md`

`memory/categories/projects.md` 与 `memory/categories/inferences.md` 是否保留，应在实现前最后确认：

- 若希望“项目背景”和“临时推断”继续拥有独立层，可保留
- 若希望结构更彻底简化，可分别并入 `MEMORY.md` / `WORKING.md`

## Open Questions

- `projects.md` 是否保留为独立长期上下文页，还是并入 `MEMORY.md`
- `inferences.md` 是否保留为独立“暂定结论”层，还是并入 `WORKING.md`
- `wiki/` 是否继续作为正式层，还是降级为可选扩展
- 迁移策略是否需要一次性转换已有 workspace 内容，还是只做读兼容与新写入收敛
- 是否需要把 `MEMORY.md` / `USER_MODEL.md` / `RELATIONSHIP_MAP.md` 进一步合并为单一 `PROFILE.md`
[/PRD]
