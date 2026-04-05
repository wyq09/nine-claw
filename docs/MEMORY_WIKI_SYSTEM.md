# NineClaw Memory Wiki

## Goal

把现有的 agent memory 从“主 `MEMORY.md` + daily log”升级成更接近 LLM Wiki 的结构，让知识能持续编译、索引和维护，而不是每轮都从原始来源重新拼。

## Layers

### Raw Sources

- `agents/<agent-id>/memory/raw/`
- `agents/<agent-id>/inbox/`
- 对话原文、导入附件、渠道收件副本都在这里
- 这些文件只追加和登记，不做摘要式覆盖

### Curated Wiki

- `MEMORY.md`
- `WORKING.md`
- `DECISIONS.md`
- `memory/categories/*.md`
- `memory/YYYY-MM-DD.md`

这一层是 LLM 维护过的知识层，允许持续改写和整理。

### Schema And Operations

- `AGENTS.md`
- 当前 agent 私有 markdown
- `memory/WIKI_INDEX.md`
- `memory/SOURCE_INDEX.md`
- `memory/LOG.md`
- `memory/LINT.md`

这一层告诉模型应该怎么 ingest / query / lint。

## Runtime Flow

1. 运行前先注入当前 agent 的核心私有文件。
2. 再注入 `WIKI_INDEX.md`、`SOURCE_INDEX.md`、`LOG.md`、`LINT.md`。
3. 再按当前问题抽取相关分类记忆。
4. 最后补最近两天的 daily log。

## Write Flow

### Ingest

每次新的桌面聊天、Bot 对话或附件导入都会：

- 把原始内容记入 `memory/raw/` 或 `inbox/`
- 更新 `SOURCE_INDEX.md`
- 追加 `LOG.md`
- 刷新 `WIKI_INDEX.md`
- 同步沉淀到 `MEMORY.md` / `WORKING.md` / 分类记忆

### Query

模型回答历史问题时，先读 `WIKI_INDEX.md` 再决定进具体页，避免把所有 memory 全量塞进上下文。

### Lint

`LINT.md` 提供维护检查表，要求模型定期注意：

- 冲突或过期结论
- 缺失交叉引用
- 没有被索引覆盖的高频概念
- 还缺哪些 source

## Why This Helps

- raw sources 和 wiki synthesis 分层后，来源和结论不再混在一起
- `SOURCE_INDEX.md` / `LOG.md` 给模型一个轻量可导航的历史面板
- 桌面聊天、Bot、附件现在都走同一套记忆沉淀路径
