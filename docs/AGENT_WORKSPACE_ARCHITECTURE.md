# NineClaw Agent Workspace Architecture

## Goal

把 NineClaw 当前“数据库里的 agent 配置”升级成“两层结构”：

1. 数据库负责 agent 索引、模型、技能、颜色、执行模式
2. `md` 工作区负责 agent 的身份、私有记忆、工具偏好、短期状态和引导流程

数据库是索引层。Markdown 是记忆层。

## Directory Model

默认工作区根目录：

```text
~/.nineclaw/workspace/
```

结构：

```text
~/.nineclaw/workspace/
  AGENTS.md
  SOUL.md
  USER.md
  MEMORY.md
  TOOLS.md
  HEARTBEAT.md
  AGENT_REGISTRY.md
  agents/
    _template/
    <agent-id>/
      IDENTITY.md
      ROLE.md
      MEMORY.md
      USER_MODEL.md
      RELATIONSHIP_MAP.md
      PITFALLS.md
      TOOLS.md
      HEARTBEAT.md
      WORKING.md
      DECISIONS.md
      PUBLIC_CONTEXT.md
      memory/
        YYYY-MM-DD.md
        INDEX.md
        REVIEW_QUEUE.md
        SOURCE_INDEX.md
        LOG.md
        LINT.md
        categories/
          INDEX.md
          general.md
          user_profile.md
          preferences.md
          projects.md
          commitments.md
          decisions.md
          relationships.md
          pitfalls.md
          inferences.md
        raw/
          YYYY-MM-DD/
            <timestamp>-<source>.md
      wiki/
        INDEX.md
      inbox/
        YYYY-MM-DD/
          <timestamp>-<scope>-<file>
```

## Memory Layers

### Root Shared Layer

用于所有 agent 共享：

- `AGENTS.md`: 运行协议
- `SOUL.md`: 共享行为底色
- `USER.md`: 共享用户画像
- `MEMORY.md`: system memory
- `TOOLS.md`: system tool notes
- `HEARTBEAT.md`: system-level periodic checks
- `AGENT_REGISTRY.md`: app 生成的 agent 索引文件，不适合手写长说明

### Agent Private Layer

用于单个 agent：

- `IDENTITY.md`: 自我身份
- `ROLE.md`: 责任边界
- `MEMORY.md`: 私有长期记忆
- `USER_MODEL.md`: 用户长期画像与交互模型
- `RELATIONSHIP_MAP.md`: 重要人物关系图
- `PITFALLS.md`: 高风险坑点与明确纠正
- `TOOLS.md`: 私有工具偏好
- `WORKING.md`: 短期上下文，含 `Current Focus` / `OPEN_LOOPS`
- `DECISIONS.md`: 决策日志
- `PUBLIC_CONTEXT.md`: 外部安全上下文
- `memory/YYYY-MM-DD.md`: 每日日志
- `memory/INDEX.md`: 当前 agent memory 的总入口，运行时先读它再决定看哪些页
- `memory/REVIEW_QUEUE.md`: 复查队列，记录会过期或要闭环的事项
- `memory/SOURCE_INDEX.md`: raw source / 附件来源注册表，不直接改源文件
- `memory/LOG.md`: append-only 的 ingest/query/source 操作日志
- `memory/LINT.md`: wiki 健康检查标准
- `memory/categories/*.md`: 长期整理层，带统一 schema
- `memory/raw/YYYY-MM-DD/*.md`: 原始对话/来源快照，作为不可变 source-of-truth
- `wiki/INDEX.md`: 外部知识、研究笔记、方法论入口
- `inbox/YYYY-MM-DD/*`: 用户或渠道发来的附件副本，供后续工具读取

## LLM Wiki Layering

这版工作区已经不是“单个 `MEMORY.md`”模式，而是借鉴了 Karpathy 的 LLM Wiki 思路，拆成三层：

1. Raw sources
   - `memory/raw/` 与 `inbox/`
   - 只追加，不改写
   - 保留原始对话、附件和来源路径
2. Curated wiki
   - `MEMORY.md`、`USER_MODEL.md`、`RELATIONSHIP_MAP.md`、`PITFALLS.md`、`WORKING.md`、`memory/categories/*.md`、`memory/REVIEW_QUEUE.md`、决策、daily log
   - 对话 ingest 默认只写 raw、daily、`WORKING.md`、`REVIEW_QUEUE.md`、`PITFALLS.md` 与索引；`memory/categories/*.md` 不自动追加（避免流水账），由整理任务或环境变量 `NINECLAW_APPEND_CATEGORY_MEMORY_ON_INGEST=1` 控制旧行为
   - LLM 与用户维护整理层：总结、交叉引用、沉淀到 `MEMORY.md` / `DECISIONS.md` 等
3. Schema
   - `AGENTS.md`、当前 agent 私有 markdown、`LINT.md`
   - 定义 ingest / query / lint 的规则

## 和「系统指令」怎么分工

你在应用里为每个智能体填的**系统指令**（长约束、风格）是主入口；本目录下的 md 是记忆与持续身份。详见 [AGENT_SYSTEM_PROMPT.md](./AGENT_SYSTEM_PROMPT.md)。

## Runtime Integration

NineClaw 在以下时机自动接入这套结构：

### 1. Agent 创建

- 数据库创建 agent 记录
- 自动在 `agents/<agent-id>/` 下生成私有 md 文件
- 默认从 `agents/_template/` 复制新的记忆骨架，不再额外生成 `BOOTSTRAP.md`

### 2. Agent 更新

- 保持既有私有文件不被覆盖
- 缺失文件自动补齐

### 3. Agent 运行

运行时 system prompt 会注入：

- 当前 workspace 根目录
- 当前 agent home 路径
- 读取顺序
- `memory/INDEX.md` / `SOURCE_INDEX.md` / `LOG.md` / `LINT.md` / `REVIEW_QUEUE.md`
- `USER_MODEL.md` / `RELATIONSHIP_MAP.md` / `PITFALLS.md`
- 共享 / 私有记忆边界
- 写入规则
- memory / raw / daily / wiki 的分层规则

## Why This Split

好处有四个：

1. agent 有持续身份，不只是一次性 prompt
2. 记忆有边界，避免所有 agent 抢同一份 `MEMORY.md`
3. 后续可以做 agent onboarding，而不是让用户一次性填一大坨表单
4. UI、数据库、文件系统三层职责更清楚

## Current Default Roles

当前已预置的 agent home：

- `main`: 默认主代理
- `researcher`: 研究 / 检索 / 核验
- `operator`: 执行 / 落地 / 维护

## Next Iteration

下一步应该做的是对话式创建流程，而不是继续堆字段：

- 先生成 agent 基础记录
- 再通过轻量向导逐步补齐身份和边界
- 用户自然聊几轮，就能把 agent 建出来
