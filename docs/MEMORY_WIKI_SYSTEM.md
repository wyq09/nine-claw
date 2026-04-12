# NineClaw Memory Wiki

## Goal

把 agent memory 从「单文件 + daily log」升级成接近 Wiki 的分层：原始来源可审计，整理层可编辑，索引可导航。

## Layers

### Raw Sources

- `agents/<agent-id>/memory/raw/`
- `agents/<agent-id>/inbox/`
- 对话原文、附件副本：只追加登记，不做摘要式覆盖

### Curated Wiki

- `MEMORY.md`（人设与核心原则，**不**由 ingest 自动改写）
- `USER_MODEL.md`（用户长期画像、隐含意图、测试模式）
- `RELATIONSHIP_MAP.md`（重要人物与关系）
- `PITFALLS.md`（高风险坑点、明确纠正）
- `WORKING.md`（短期上下文，含 `OPEN_LOOPS`，ingest 会更新）
- `DECISIONS.md`、`memory/categories/*.md`（**默认由人/模型整理**，不从每轮对话自动灌条目）
- `memory/REVIEW_QUEUE.md`（待复查、待闭环、易过期项目状态）
- `memory/YYYY-MM-DD.md`（按日流水，ingest 追加摘要行）

### Schema And Operations

- `AGENTS.md`、agent 私有 markdown
- `memory/INDEX.md`（根入口：告诉运行时去哪读，不是简单文件清单）
- `memory/SOURCE_INDEX.md`（每条含 `Index: type=… ts=… cats=…` 便于 `rg` 过滤）
- `memory/LOG.md`、`memory/LINT.md`
- `wiki/INDEX.md`（外部知识与方法论入口）

## Runtime Flow

1. 注入核心私有文件。
2. 注入 `memory/INDEX.md`、`SOURCE_INDEX.md`、`LOG.md`、`LINT.md`、`REVIEW_QUEUE.md`。
3. 按问题抽取相关分类记忆（若对应 shard 存在且有内容）。
4. 补最近两天的 daily log。

## Write Flow

### Ingest

每次桌面聊天 / Bot / 附件会：

- 写入 `memory/raw/`（或 inbox）原文或登记
- 更新 `SOURCE_INDEX.md`（含结构化 `Index` 行）
- 追加 `LOG.md`
- 刷新 `memory/INDEX.md` 和 `LINT.md`
- 更新 `WORKING.md` 与当日 `memory/YYYY-MM-DD.md`
- 对“别再这样”“不要误判”这类明确纠正，保守沉淀到 `PITFALLS.md`
- 对承诺、阻塞、时效性项目状态自动补 `REVIEW_QUEUE.md`
- **默认不再**向 `memory/categories/*.md` 追加（避免分类文件变成流水账）。若需恢复旧行为，启动前设置环境变量 `NINECLAW_APPEND_CATEGORY_MEMORY_ON_INGEST=1`（或 `true` / `yes`）。

### Query

先读 `memory/INDEX.md`，再按路由下钻：任务推进优先 `WORKING.md` 的 `OPEN_LOOPS`，规则优先 `DECISIONS.md` / `PITFALLS.md`，用户风格优先 `USER_MODEL.md`，人物关系优先 `RELATIONSHIP_MAP.md`，涉及附件/来源时查 `SOURCE_INDEX.md`，涉及外部知识时查 `wiki/INDEX.md`。

### Lint

`LINT.md` 现在会记录最近一次自动体检结果与分数：检查入口是否齐全、`OPEN_LOOPS` 是否存在、schema 是否完整、`commitments` 是否账本化、`inferences` 是否与事实分层、`USER_MODEL/PITFALLS/RELATIONSHIP_MAP` 是否存在、legacy `BOOTSTRAP/WIKI_INDEX` 是否已清理。

### 自动提炼到 MEMORY.md

当前产品选择：**不**在后台静默写 `MEMORY.md`。推荐由模型在「整理任务」中根据 daily + SOURCE_INDEX 提出修改稿，人工确认后写入。

## Why This Helps

- Raw 与 synthesis 分离，结论可追溯
- `Index` 行支持按类型/时间/分类键快速过滤，为后续向量检索留接口
- 分类文件默认人工/批量整理，可读性优于每轮自动 bullet
