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
- `WORKING.md`（短期上下文，ingest 会更新）
- `DECISIONS.md`、`memory/categories/*.md`（**默认由人/模型整理**，不从每轮对话自动灌条目）
- `memory/YYYY-MM-DD.md`（按日流水，ingest 追加摘要行）

### Schema And Operations

- `AGENTS.md`、agent 私有 markdown
- `memory/WIKI_INDEX.md`（含 **Topic map**：从核心页 `##` 标题生成的导航）
- `memory/SOURCE_INDEX.md`（每条含 `Index: type=… ts=… cats=…` 便于 `rg` 过滤）
- `memory/LOG.md`、`memory/LINT.md`

## Runtime Flow

1. 注入核心私有文件。
2. 注入 `WIKI_INDEX.md`、`SOURCE_INDEX.md`、`LOG.md`、`LINT.md`。
3. 按问题抽取相关分类记忆（若对应 shard 存在且有内容）。
4. 补最近两天的 daily log。

## Write Flow

### Ingest

每次桌面聊天 / Bot / 附件会：

- 写入 `memory/raw/`（或 inbox）原文或登记
- 更新 `SOURCE_INDEX.md`（含结构化 `Index` 行）
- 追加 `LOG.md`
- 刷新 `WIKI_INDEX.md`
- 更新 `WORKING.md` 与当日 `memory/YYYY-MM-DD.md`
- **默认不再**向 `memory/categories/*.md` 追加（避免分类文件变成流水账）。若需恢复旧行为，启动前设置环境变量 `NINECLAW_APPEND_CATEGORY_MEMORY_ON_INGEST=1`（或 `true` / `yes`）。

### Query

先读 `WIKI_INDEX.md`（含 Topic map），再下钻具体页；涉及附件/来源时查 `SOURCE_INDEX.md` 中的 `Path` / `Index` 行。

### Lint

`LINT.md` 要求定期检查：把 daily / raw 中的重要结论合并进 `DECISIONS.md`、`MEMORY.md` 或 category shards，并处理矛盾与过期结论。

### 自动提炼到 MEMORY.md

当前产品选择：**不**在后台静默写 `MEMORY.md`。推荐由模型在「整理任务」中根据 daily + SOURCE_INDEX 提出修改稿，人工确认后写入。

## Why This Helps

- Raw 与 synthesis 分离，结论可追溯
- `Index` 行支持按类型/时间/分类键快速过滤，为后续向量检索留接口
- 分类文件默认人工/批量整理，可读性优于每轮自动 bullet
