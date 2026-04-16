# Frontend Breakdown: SQLite Unified Storage

## Goal

本文件定义前端从“blob history + workspace file API 心智模型”迁移到“SQLite-first 数据模型”的具体改造点。

## Current Frontend Dependency Map

### Chat History

当前依赖：

- [src/hooks/usePiAgent.ts](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/hooks/usePiAgent.ts:300)
  - `loadHistoryState()`
  - `saveHistoryState()`
  - `clearHistoryState()`

当前问题：

- 整份历史快照读写
- 删除 / 更新都要回写整包 JSON
- `historySearch` 仍是纯前端字符串过滤

### Workspace

当前依赖：

- [src/lib/piClient.ts](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/lib/piClient.ts:253)
  - `readAgentWorkspaceBundle`
  - `readAgentWorkspaceFile`
  - `writeAgentWorkspaceFile`
- [src/app/shell/NineClawApp.tsx](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/app/shell/NineClawApp.tsx:1318)
  - workspace dialog 加载、lazy fetch、保存
- [src/app/agents/AgentChannelDialogs.tsx](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/app/agents/AgentChannelDialogs.tsx:346)
  - workspace file UI

### Sidebar History Search

当前依赖：

- [src/app/shell/NineClawApp.tsx](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/app/shell/NineClawApp.tsx:342)
- [src/app/shell/NineClawAppChrome.tsx](/Users/yiqunwu/wuyiqun/power_project/ai-x/ai_coding/nine-claw/src/app/shell/NineClawAppChrome.tsx:363)

当前问题：

- 搜索只过滤当前已加载 history
- 无法接入统一 SQLite 检索

## Frontend Target State

### 1. Chat History Becomes Incremental

前端不再维护“整体历史快照持久化”模型。

目标：

- 历史列表：独立 API
- 会话详情：独立 API
- 轮次追加：独立 API
- 删除会话：独立 API

### 2. Workspace Remains File-Shaped but SQLite-Backed

前端仍接收 `AgentWorkspaceBundle` / `AgentWorkspaceFile` 这类 UI 友好结构，但其内容由 SQLite 生成。

目标：

- UI 不需要知道底层已经不是直接读磁盘
- `lazy_fetch` 保留
- 保存工作流不变

### 3. Search Moves to Backend

阶段划分：

- 第一阶段：历史列表仍先显示最近会话，但不再依赖 `history_v1`
- 第二阶段：侧边栏 search 改为后端统一检索

## API Change Plan

### Remove History Snapshot APIs

前端下线：

- `loadHistoryState`
- `saveHistoryState`
- `clearHistoryState`

`src/lib/piClient.ts` 新增：

- `listChatSessions()`
- `getChatSession(sessionId)`
- `createChatSession()`
- `appendChatTurn()`
- `updateChatTurn()`
- `deleteChatSession()`
- `clearAllChatSessions()`

### Add User Memory / Search / Runtime APIs

后续新增：

- `listUserProfiles()`
- `getUserProfile(profileId)`
- `searchKnowledge(query)`
- `getRuntimeSessionStatus(sessionId)`

第一阶段允许这些接口先只在后端落地，不强制马上接完整 UI。

## Hook Refactor Plan

### `usePiAgent`

当前是最大改造点。

要做的事：

1. 移除 hydration 时从 blob 拉全量数据
2. 改成：
   - mount 时 `listChatSessions`
   - 选中会话时 `getChatSession`
3. 提交消息时：
   - 若无 session，新建 session
   - 追加 turn
   - streaming 期间增量更新本地 state
   - 完成后调用 `updateChatTurn`
4. 删除 / 清空：
   - 直接调结构化 API

注意：

- 本地 state 仍保留，以支持流式 UI
- 但不能再在 `useEffect` 中把整个 `history` 序列化存回数据库

### `useComposerAttachments`

当前逻辑基本可保留，但附件对象未来应带：

- `sourceId`
- `linkedTurnId?`

这样后续 UI 才能和 `memory_sources` 对齐。

## Component Change Plan

### `NineClawApp`

修改点：

- `history` 加载改成：
  - 列表单独拉
  - active session 按需拉
- `visibleHistory` 不再依赖全量 turn 已在内存中
- workspace dialog 调用保持不变，但背后数据已变 SQLite-first

### `NineClawAppChrome`

第一阶段：

- 搜索框保留
- 输入后仍然过滤当前已加载列表，或直接只搜 session 标题

第二阶段：

- 搜索框接 `searchKnowledge()`
- 返回聊天 / wiki / memory 混合结果

### `AgentWorkspaceDialog`

保持以下 UI 语义不变：

- 文件列表
- 选中文件
- lazy fetch
- 编辑保存

允许调整：

- `exists` 字段解释成 projection 是否存在
- 元信息增加 `sourceOfTruth: sqlite`

## Type Changes

### History Types

保留：

- `HistoryItem`
- `ConversationTurn`

但增加：

- `historyLoaded: boolean`
- `sessionDetailLoaded: boolean`

### Workspace Types

在 `AgentWorkspaceFile` 上新增可选字段：

- `sourceOfTruth?: 'sqlite' | 'projection'`
- `projectionStatus?: 'in_sync' | 'missing' | 'conflict' | 'sqlite_ahead' | 'file_ahead'`

## Step-by-Step Frontend Delivery

### Step 1: Chat API Flip

- 改 `src/lib/piClient.ts`
- 改 `usePiAgent`
- 保证 UI 看起来不变

### Step 2: Workspace Metadata Enrichment

- bundle / file 返回 projection 状态
- dialog 展示更准确的状态

### Step 3: Unified Search Hook

- 新增 `useKnowledgeSearch`
- 先不接 UI

### Step 4: Sidebar Search Upgrade

- `historySearch` 改接后端
- 结果混合展示

## Tests to Add

### `usePiAgent`

- mount 时加载 sessions
- 选中 session 时加载详情
- 新建会话不再调用 `saveHistoryState`
- 删除会话后列表刷新

### Workspace

- `readAgentWorkspaceBundle` 结果仍能正确渲染
- lazy fetch 文件能正确加载
- 保存后状态更新

### Search

- search hook 正常发起请求
- 空 query 不触发后端

## Completion Criteria

前端视为完成的标准：

- `usePiAgent` 不再依赖 blob history API
- Workspace UI 不感知底层从文件切到 SQLite
- 侧边栏历史列表不再依赖一次性载入全量 turn
