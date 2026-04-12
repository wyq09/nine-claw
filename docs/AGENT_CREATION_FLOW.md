# NineClaw Agent Creation Flow

## Design Goal

不要让用户面对一张冷冰冰的“新建智能体表单”。

更自然的方式是：

1. 先让用户说“我想要一个什么 agent”
2. 系统帮他抽取骨架
3. 自动建档
4. 再通过轻量对话把 identity / role / memory 补全

## Recommended Flow

### Step 1: Intent Capture

入口问题只问一件事：

> 你想要这个 agent 主要替你干什么？

用户自然描述即可。

系统从这句话里先提取：

- `name`
- `summary`
- `description`
- 倾向模型
- 是否偏研究 / 执行 / 协调

### Step 2: Quick Draft

立即给用户一个可编辑草案：

- 名字
- 一句话简介
- 详细职责
- 推荐默认模型
- 推荐执行模式

这一步只要用户点头，不要追着盘问。

### Step 3: Create Agent Record

用户确认后立刻：

1. 写入数据库
2. 创建 `agents/<agent-id>/`
3. 从 `agents/_template/` 生成初始 `IDENTITY.md`、`ROLE.md`、`MEMORY.md`、`USER_MODEL.md`、`RELATIONSHIP_MAP.md`、`PITFALLS.md`、`TOOLS.md`、`WORKING.md`、`DECISIONS.md`、`memory/INDEX.md`、`memory/REVIEW_QUEUE.md`、`wiki/INDEX.md`
5. 更新 `AGENT_REGISTRY.md` 生成索引

这一步的目标是“先出生”。

注意：

- `AGENT_REGISTRY.md` 是系统生成文件，不应该承载人工维护的详细角色说明
- 角色说明应进入 `ROLE.md`、`IDENTITY.md`、`MEMORY.md`

### Step 4: Guided Onboarding

创建完成后，不要把用户弹回管理页。

直接进入 agent onboarding，会话里只做 4 轮以内的补全：

1. 这个 agent 你希望我怎么称呼它
2. 它最该做的 3 类事是什么
3. 它最不该做的事是什么
4. 你希望它说话是什么风格

系统把答案分别写回：

- `IDENTITY.md`
- `ROLE.md`
- `PUBLIC_CONTEXT.md`
- `MEMORY.md`
- `USER_MODEL.md`
- `RELATIONSHIP_MAP.md`
- `PITFALLS.md`
- `memory/categories/*.md` 或 `DECISIONS.md`（如果内容已经稳定）

### Step 5: First Real Task

onboarding 完成后，立刻给用户一个过渡句：

> 这个 agent 已经建好了。你现在可以直接给它第一件事。

第一件真实任务的上下文写入：

- `WORKING.md`
- `WORKING.md` 的 `OPEN_LOOPS`
- `memory/YYYY-MM-DD.md`

## UI Recommendation

创建入口分成两种：

### Mode A: Quick Create

适合大多数用户：

- 一个输入框
- 一句自然语言描述
- 系统自动生成草案

### Mode B: Advanced Create

适合明确知道自己要什么的用户：

- 保留现在的表单
- 但在保存后仍自动生成 workspace 和 onboarding

## Important Constraint

不要要求用户一开始就理解：

- system prompt
- memory layers
- shared context policy
- long-term vs working memory

这些属于系统内部实现，不属于用户的首轮认知负担。

用户只该感受到：

- agent 很快出生
- agent 很快有个性
- agent 很快能干活

## Suggested Copy

### Create Button

- `新建 agent`
- `造个新代理`

### First Prompt

- `你想要这个 agent 主要替你干什么？`

### Draft Confirmation

- `我先给你起了个草案，你看下对不对。`

### Onboarding Transition

- `它先别急着上工，我把身份和边界补齐。`

### Finish State

- `建好了。现在直接给它第一件事。`
