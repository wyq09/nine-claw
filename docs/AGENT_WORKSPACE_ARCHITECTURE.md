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
    _templates/
    <agent-id>/
      BOOTSTRAP.md
      IDENTITY.md
      ROLE.md
      MEMORY.md
      TOOLS.md
      HEARTBEAT.md
      WORKING.md
      DECISIONS.md
      PUBLIC_CONTEXT.md
      memory/
        YYYY-MM-DD.md
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
- `TOOLS.md`: 私有工具偏好
- `WORKING.md`: 短期上下文
- `DECISIONS.md`: 决策日志
- `PUBLIC_CONTEXT.md`: 外部安全上下文
- `memory/YYYY-MM-DD.md`: 每日日志

## Runtime Integration

NineClaw 在以下时机自动接入这套结构：

### 1. Agent 创建

- 数据库创建 agent 记录
- 自动在 `agents/<agent-id>/` 下生成私有 md 文件
- 对新建自定义 agent 额外生成 `BOOTSTRAP.md`

### 2. Agent 更新

- 保持既有私有文件不被覆盖
- 缺失文件自动补齐

### 3. Agent 运行

运行时 system prompt 会注入：

- 当前 workspace 根目录
- 当前 agent home 路径
- 读取顺序
- 共享 / 私有记忆边界
- 写入规则
- `BOOTSTRAP.md` 首次引导规则

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
- 然后通过 `BOOTSTRAP.md` 或向导问题，逐步补齐身份和边界
- 用户自然聊几轮，就能把 agent 建出来
