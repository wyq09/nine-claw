# Implementation Plan: Memory System Simplification

## Objective

按 [PRD_MEMORY_SYSTEM_SIMPLIFICATION.md](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/docs/PRD_MEMORY_SYSTEM_SIMPLIFICATION.md:1) 将 NineClaw 现有记忆系统收敛为少数主路径，并确保：

- 用户记忆有固定 schema
- 运行时只依赖保留层
- Agent Workspace UI 与后端分层一致
- 任一记忆 markdown 文件不超过 500 行

## Target Architecture

### 保留层

- `MEMORY.md`
  只放最稳定的身份锚点、核心原则、长期主题摘要
- `USER_MODEL.md`
  承载工作方式、AI 交互偏好、写作风格 schema
- `RELATIONSHIP_MAP.md`
  承载“身边的人”和关键关系
- `WORKING.md`
  承载当前 focus、open loops、pending review、最近上下文
- `DECISIONS.md`
  承载明确规则、确认过的约定、长期有效决策
- `PITFALLS.md`
  承载高风险错误模式与纠正动作
- `memory/YYYY-MM-DD.md`
  历史摘要正文
- `memory/DAILY_INDEX.md`
  历史摘要检索索引
- `memory/raw/`
  原始证据层
- `memory/SOURCE_INDEX.md`
  证据登记册
- `wiki/`
  暂时保留为可选外部知识层

### 下线层

- `memory/LINT.md`
- `memory/LOG.md`
- `memory/categories/INDEX.md`
- `memory/categories/preferences.md`
- `memory/categories/relationships.md`
- `memory/categories/pitfalls.md`
- `memory/categories/decisions.md`
- `memory/categories/commitments.md`

### 待定层

- `memory/categories/projects.md`
- `memory/categories/inferences.md`

默认建议：

- `projects.md` 并入 `MEMORY.md`
- `inferences.md` 并入 `WORKING.md`

如果实现期发现这两类信息确实持续高频且会逼近 500 行，再恢复为独立层。

## User Memory Schema

### `MEMORY.md`

- 用户身份 / Identity
  - 基本信息
  - 职业与背景
  - 日常偏好
- 核心原则
- 当前长期主题

### `USER_MODEL.md`

- 工作方式
  - 开发流程
  - 组织与整理
  - 调试习惯
  - AI 交互偏好
- 写作风格
  - 整体调性
  - 句式与节奏
  - 段落结构
  - 表达模式

### `RELATIONSHIP_MAP.md`

- 身边的人
- 团队与角色
- 关系备注

## 500-Line Rule

所有记忆 markdown 文件必须小于等于 500 行。

### 执行规则

- `WORKING.md`
  只保留当前 focus、最近 open loops、最近 review items、最近上下文
- `DECISIONS.md`
  只保留仍然有效的规则，不保留过期决策流水
- `PITFALLS.md`
  只保留仍活跃的坑点
- `memory/YYYY-MM-DD.md`
  单天文件接近 500 行时切分为当天多段文件，或将明细写入 raw，只保留摘要
- `memory/SOURCE_INDEX.md`
  接近上限时按月份或季度滚动归档
- `memory/DAILY_INDEX.md`
  接近上限时按月份或季度滚动归档，同时保留最新总入口

### 工程要求

- 新增统一的行数检查工具函数
- 每次写入前检查目标文件行数
- 对会增长的文件统一采用“截断 / 归档 / 滚动”策略，禁止无界追加

## Execution Phases

## Phase 1: Freeze Target Shape

### Goal

先让模板、常量、前端分组、prompt 文案统一到同一套心智模型。

### Changes

- 更新 [src-tauri/src/agent_workspace.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:20)
  - 调整 `AGENT_TEMPLATE_FILES`
  - 去掉计划下线的 scaffold 文件
  - 更新 `fallback_template()`
- 更新 [src-tauri/src/agent_workspace/memory_wiki.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace/memory_wiki.rs:22)
  - 停止把 `LOG.md`、`LINT.md` 视为核心记忆文件
- 更新 [src/app/agents/AgentChannelDialogs.tsx](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/app/agents/AgentChannelDialogs.tsx:370)
  - 简化 section 分组
- 更新 [src/app/lib/appFormatting.tsx](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/app/lib/appFormatting.tsx:580)
  - 同步 section label

### Acceptance

- 新建 agent 不再 scaffold 已下线文件
- Workspace UI 不再显式突出已下线层
- Prompt 文案不再把已下线层描述为主路径

## Phase 2: Simplify Write Path

### Goal

让 ingest 和附件登记只写保留层。

### Changes

- 重构 [append_agent_memory_entry()](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:741)
  - 保留 raw source 写入
  - 保留 daily log 写入
  - 保留 source index 写入
  - 保留 `WORKING.md` 写入
  - 保留 `PITFALLS.md` 写入
  - 停止写 `REVIEW_QUEUE.md` 或将其逻辑内联进 `WORKING.md`
  - 停止 category shards 自动写入
- 重构附件入口
  - [register_agent_attachment_source()](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:849)
  - [persist_agent_inbound_artifact()](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:910)

### Acceptance

- 一次 ingest 后只生成保留层内容
- 不再刷新 `LINT.md`、`LOG.md`
- 不再依赖环境变量恢复旧 category 自动追加

## Phase 3: Simplify Retrieval Path

### Goal

让 runtime prompt 和 query snapshots 只走保留层。

### Changes

- 重构 [build_workspace_system_prompt_for_query()](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:551)
  - 删掉已下线层提示
  - 聚焦 `MEMORY.md`、`USER_MODEL.md`、`RELATIONSHIP_MAP.md`、`WORKING.md`、`DECISIONS.md`、`PITFALLS.md`
- 删除或合并：
  - `build_categorized_memory_snapshot()`
  - `build_specialized_memory_snapshot()` 中对 category shards 的依赖
  - `build_memory_wiki_snapshot()` 中对已下线文件的路由说明
- 保留：
  - `build_daily_digest_retrieval_snapshot()`
  - `SOURCE_INDEX` 相关 evidence 路由

### Acceptance

- 运行时 system prompt 不再提及 `LOG.md`、`LINT.md`、多数 category shards
- 历史检索仍能通过 daily index 工作
- 附件/原始记录检索仍能通过 source index 工作

## Phase 4: Introduce Stable Schema

### Goal

把用户记忆从“自由文本文件”收敛为“固定结构文件”。

### Changes

- 更新模板：
  - `MEMORY.md`
  - `USER_MODEL.md`
  - `RELATIONSHIP_MAP.md`
- 新增辅助函数：
  - 读取稳定档案 schema
  - 在指定 section 下插入/更新条目
  - 防止重复条目和自由散落内容
- 调整写回策略
  - 自动沉淀只允许写入明确 section
  - 不允许把任意对话摘要直接塞进稳定档案层

### Acceptance

- 新 agent 模板符合 PRD 中的 schema
- 稳定档案层不会持续长成自由流水账
- 身份画像、工作方式、写作风格可稳定维护

## Phase 5: Enforce 500-Line Limit

### Goal

让“单文件不超过 500 行”成为真实机制，而不是文档约定。

### Changes

- 新增通用工具：
  - `count_lines(path)`
  - `count_lines_str(content)`
  - `enforce_memory_line_limit(path, strategy)`
- 为不同文件定义策略：
  - `WORKING.md`
    保留最近 N 条 open loops / review / context
  - `PITFALLS.md`
    仅保留活跃项
  - `DECISIONS.md`
    仅保留有效规则
  - `SOURCE_INDEX.md`
    超限后滚动归档到 `SOURCE_INDEX-YYYY-MM.md`
  - `DAILY_INDEX.md`
    超限后滚动归档到 `DAILY_INDEX-YYYY-MM.md`

### Acceptance

- 自动写入不会让目标文件超过 500 行
- 超限行为可预测、可测试、可回溯

## Phase 6: Cleanup and Migration

### Goal

处理旧 workspace 和历史文件兼容。

### Changes

- 兼容读取旧文件，但不再新写
- 新增一次性迁移或惰性迁移逻辑：
  - `REVIEW_QUEUE.md` -> `WORKING.md`
  - category shards -> 稳定档案层或规则层
  - `LOG.md` -> 合并进 `SOURCE_INDEX.md` 或直接停用
- 清理未使用函数：
  - `select_core_memory_points()`
  - `append_core_memory_points()`
  - 其他 category-only helpers

### Acceptance

- 老 agent workspace 打开不报错
- 迁移后主路径明确
- 死代码和无主文件被删除

## File-Level Worklist

### Backend

- [src-tauri/src/agent_workspace.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace.rs:1)
  - 拆成多个子模块
  - 优先拆 `templates`、`ingest`、`retrieval`、`migration`
- [src-tauri/src/agent_workspace/memory_wiki.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agent_workspace/memory_wiki.rs:1)
  - 收缩为 evidence + daily index 支撑模块
- [src-tauri/src/agents.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/agents.rs:304)
  - 确认 bundle 输出与新分组一致
- [src-tauri/src/lib.rs](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src-tauri/src/lib.rs:2144)
  - 确认 tauri 命令行为不变

### Frontend

- [src/app/agents/AgentChannelDialogs.tsx](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/app/agents/AgentChannelDialogs.tsx:346)
- [src/app/lib/appFormatting.tsx](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/app/lib/appFormatting.tsx:580)

### Docs

- [docs/PRD_MEMORY_SYSTEM_SIMPLIFICATION.md](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/docs/PRD_MEMORY_SYSTEM_SIMPLIFICATION.md:1)
- [docs/MEMORY_WIKI_SYSTEM.md](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/docs/MEMORY_WIKI_SYSTEM.md:1)
- [docs/AGENT_WORKSPACE_ARCHITECTURE.md](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/docs/AGENT_WORKSPACE_ARCHITECTURE.md:1)

## Testing Plan

## Automated

- `npm run lint`
- `cargo test --manifest-path src-tauri/Cargo.toml agent_workspace`

### Add / Update Rust Tests

- 新 agent scaffold 不再生成已下线文件
- ingest 只写保留层
- retrieval prompt 不再引用已下线层
- 500 行限制在各类文件上的行为
- 旧 workspace 的兼容读取

## Manual

- 打开 Agent Workspace
- 检查 section 分组
- 检查默认打开文件
- 手动编辑并保存稳定档案层文件
- 发起一轮桌面对话，确认 ingest 写入路径
- 导入一个附件，确认 evidence 路径

## Risks

- 旧 workspace 中已有数据分散在已下线 shards，迁移不完整会导致“记忆丢失感”
- prompt 路由收得太快，可能短期降低召回率
- 500 行限制如果只做截断、不做归档，会导致信息静默丢失
- UI 若仍显示旧分组，会让用户心智混乱

## Recommended Delivery Order

1. Phase 1
2. Phase 2
3. Phase 3
4. Phase 4
5. Phase 5
6. Phase 6

不要先做迁移，再做目标架构；否则会迁移到一套很快又要变的结构里。
