# PRD: Session Context Window Guard for Chat Sessions

## Overview

为 NineClaw 桌面端聊天会话增加“上下文窗口占用感知”能力。系统需要为每个 session 独立维护当前模型的最大上下文窗口与已使用上下文占比，并在任务输入区展示一个轻量 badge。用户鼠标悬停时，可以看到类似参考图的详细上下文面板，包括当前占比、状态等级、阈值说明、输入/输出 token 明细。

该能力的核心目标不是改变主线问答流程，而是在不干扰流式问答的前提下，为用户提供上下文风险预警，并在占比过高时触发界面层级的紧凑/折叠策略，以及可选的自动压缩能力。

本需求有两个明确约束：

1. 如果无法自动获取模型最大上下文窗口，必须允许用户在配置模型时手动填写。
2. token 统计、上下文比例计算、状态刷新、自动压缩都不得阻塞主线问答流程。

## Goals

- 为每个 chat session 提供独立的上下文占用状态，不与全局状态混淆。
- 在输入区显式展示“当前 session 已使用上下文比例”。
- 在 hover 面板中提供可理解的明细与阈值说明。
- 支持四级阈值策略：
  - 60%: Snip
  - 75%: Compact
  - 85%: Collapse
  - 95%: Auto Compact
- 最大上下文窗口优先自动获取，失败时允许手动配置。
- 所有上下文统计与压缩逻辑必须异步旁路执行，不影响消息发送、流式输出、停止生成等主线能力。
- 为前端、Rust 后端、状态机和关键交互补齐测试。

## Quality Gates

These commands must pass for every user story:

- `npm run lint` - 前端静态检查
- `npm run build` - TypeScript 编译与 Vite 构建
- `cargo test --manifest-path src-tauri/Cargo.toml` - Rust 单元测试
- 对涉及输入区与 hover 面板的 UI story，需补充桌面端人工验证
- 对涉及自动压缩的 story，需补充“发送消息不中断主链路”的手工回归验证

说明：

- 当前仓库未见成型的前端单测脚手架，若故事涉及前端单测，将把“补齐 Vitest 测试基建”纳入范围。
- 自动压缩相关验收必须验证“统计失败不阻塞发送”“压缩失败不吞输入”。

## User Stories

### US-001: 为 Provider 配置增加最大上下文窗口手动字段
**Description:** As a 配置模型的用户, I want to manually fill in the model's max context tokens so that the system can still compute session context usage when auto detection is unavailable.

**Acceptance Criteria:**
- [ ] Provider 配置界面新增 `最大上下文窗口（tokens）` 输入项。
- [ ] 字段允许为空，表示未手动配置。
- [ ] 字段仅接受正整数。
- [ ] 字段值会持久化到现有 Provider 配置存储中。
- [ ] 已有 Provider 配置在未填写该字段时保持兼容，不出现崩溃或解析失败。
- [ ] 该字段的展示与保存不影响现有 Base URL、API Key、Model 等配置行为。

### US-002: 为会话和模型能力增加上下文窗口元数据结构
**Description:** As a system, I want a dedicated context-state model so that session-level usage can be computed and rendered consistently.

**Acceptance Criteria:**
- [ ] 前端新增独立的 session context state 类型，不复用现有 `TokenUsageRecord` 结构硬塞字段。
- [ ] 状态至少包含 `sessionId`、`model`、`usedTokens`、`contextWindow`、`percent`、`stage`、`source`、`updatedAt`。
- [ ] `source` 能区分 `auto`、`manual`、`estimated`、`unknown`。
- [ ] 当 `contextWindow` 不可用时，状态仍可安全渲染，不抛错。
- [ ] 当 session 切换时，状态按 session 隔离更新，不串会话。

### US-003: 后端支持读取当前 session 的上下文占用统计
**Description:** As a user, I want the app to read the current session context usage in the background so that I can see accurate usage information without affecting chat.

**Acceptance Criteria:**
- [ ] Rust 后端新增独立命令用于获取某个 session 的 context stats。
- [ ] 该命令复用当前 PI session/runtime 组装逻辑，不新建与主流程割裂的配置链路。
- [ ] 优先读取 PI RPC 的 `get_session_stats.contextUsage`。
- [ ] 若 RPC 返回 `contextUsage` 缺失，则回退到模型上下文窗口配置与已知 usage 做降级计算或未知态。
- [ ] 该命令失败时仅返回错误给上下文模块，不影响主会话问答命令。
- [ ] 该命令不会要求主会话流式问答等待它完成。

### US-004: 模型上下文窗口来源采用自动优先、手填回退
**Description:** As a user, I want context-window resolution to prefer auto detection but fall back to my manual value so that usage stays usable across different providers.

**Acceptance Criteria:**
- [ ] 上下文窗口解析优先级为：PI 实际返回值 > 手动填写值 > 可推断默认值/估算值 > unknown。
- [ ] 当自动获取失败但手动值存在时，UI 使用手动值。
- [ ] 当使用手动值时，UI 可标识该值来自手动配置。
- [ ] 当仅能估算时，UI 可标识为估算值。
- [ ] 自定义 Provider、OpenAI 兼容 Provider、内置 Provider 均走同一套优先级规则。

### US-005: 输入区展示当前 session 上下文占比 badge
**Description:** As a chatting user, I want to see the current session context percentage near the task input so that I can notice context pressure before sending more messages.

**Acceptance Criteria:**
- [ ] 输入区工具栏新增 context badge。
- [ ] badge 默认显示当前 session 的上下文占比，如 `14.2%`。
- [ ] 当上下文窗口未知时，badge 显示降级态，如 `--` 或 `未知`。
- [ ] badge 展示不会挤压发送按钮导致布局错位。
- [ ] badge 在首页空会话与已有会话两种状态下都能稳定渲染。
- [ ] badge 刷新失败时不影响输入框编辑、发送、停止生成、附件操作。

### US-006: hover 展示上下文占用明细面板
**Description:** As a chatting user, I want a hover card with details so that I can understand why the current session is in a certain context state.

**Acceptance Criteria:**
- [ ] 鼠标悬停 badge 时展示 hover 面板。
- [ ] 面板包含：标题、进度条、当前百分比、状态标签、阈值列表、输入 tokens、输出 tokens。
- [ ] 面板中的阈值文案为 `60% Snip`、`75% Compact`、`85% Collapse`、`95% Auto Compact`。
- [ ] 面板能显示当前窗口来源，如自动、手动、估算。
- [ ] 面板在上下文未知、统计中、统计失败三种状态下均有合理展示。
- [ ] 面板样式在浅色/深色主题下均可读，不出现溢出或遮挡输入区主要操作。

### US-007: 建立会话上下文阶段状态机
**Description:** As a system, I want a consistent stage classifier so that UI behavior and warnings can be driven by one reliable source of truth.

**Acceptance Criteria:**
- [ ] 根据 percent 将状态划分为 `normal`、`snip`、`compact`、`collapse`、`auto_compact`。
- [ ] 阈值边界行为明确：
  - [ ] `<60` 为 `normal`
  - [ ] `>=60 && <75` 为 `snip`
  - [ ] `>=75 && <85` 为 `compact`
  - [ ] `>=85 && <95` 为 `collapse`
  - [ ] `>=95` 为 `auto_compact`
- [ ] 该分类逻辑有单元测试覆盖边界值。
- [ ] UI 所有文案与样式均使用同一状态机输出，不重复定义阈值。

### US-008: 在高占比阶段启用界面层紧凑/折叠策略
**Description:** As a user, I want older content to become visually more compact when context pressure is high so that I can focus on the latest conversation.

**Acceptance Criteria:**
- [ ] `snip` 阶段支持轻量裁剪策略，例如默认收起旧 thinking 或次要细节。
- [ ] `compact` 阶段支持更紧凑的历史展示样式。
- [ ] `collapse` 阶段支持折叠较老 turn，仅保留必要摘要或标题。
- [ ] 这些策略只影响界面展示，不修改底层会话消息内容。
- [ ] 用户切换 session 后，展示策略基于当前 session 的状态独立生效。
- [ ] 展示层策略失败不会影响消息发送与历史渲染。

### US-009: 95% 阈值支持可选自动压缩
**Description:** As a user, I want the app to optionally auto-compact very full sessions so that the conversation can continue safely.

**Acceptance Criteria:**
- [ ] 自动压缩能力由显式开关控制。
- [ ] 仅在 `>=95%` 且开关开启时尝试自动压缩。
- [ ] 自动压缩调用独立后端命令，不与主发送链路强耦合。
- [ ] 自动压缩开始时，UI 显示明确状态，如“正在压缩上下文”。
- [ ] 自动压缩成功后，会话上下文占比会刷新。
- [ ] 自动压缩失败时，用户输入保留，不吞消息，不造成输入框卡死。
- [ ] 若用户关闭该开关，则即使达到 95% 也仅警示，不自动压缩。

### US-010: 上下文统计刷新完全旁路，不阻塞主线问答
**Description:** As a user, I want context tracking to be asynchronous so that my main chat flow stays fast and uninterrupted.

**Acceptance Criteria:**
- [ ] 发送消息时，不等待 context stats 请求完成。
- [ ] 流式输出过程中，context stats 刷新失败不会打断 stream。
- [ ] 停止生成时，不依赖 context stats 任务完成。
- [ ] session 切换时，上下文统计采用后台异步刷新。
- [ ] 模型切换时，上下文状态可异步重算，不阻塞 UI 切换。
- [ ] 所有旁路请求均有超时与错误处理，不在前端形成无限 loading。

### US-011: 为上下文模块补齐前端测试基建
**Description:** As a developer, I want frontend test infrastructure so that context-window UI and state logic can be safely verified.

**Acceptance Criteria:**
- [ ] 项目新增可执行的前端单测方案，如 Vitest + Testing Library。
- [ ] 能对纯函数状态机进行单元测试。
- [ ] 能对 badge/hover 组件进行渲染测试。
- [ ] 能 mock context stats 返回值与失败场景。
- [ ] 新测试基建不破坏现有 `npm run build` 与 `npm run lint`。

### US-012: 为后端上下文统计与自动压缩补齐 Rust 测试
**Description:** As a developer, I want Rust tests around context stats and compaction commands so that session-context behavior is stable.

**Acceptance Criteria:**
- [ ] `get_session_stats` 解析逻辑覆盖正常值、缺失值、null 值。
- [ ] 模型窗口来源优先级逻辑有测试覆盖。
- [ ] 自动压缩命令的成功、失败、超时分支有测试覆盖。
- [ ] 不需要真实调用外部模型即可测试关键分支。
- [ ] 测试不修改受保护文件与 PI runtime 启动脚本。

## Functional Requirements

### FR-1: Session 级上下文追踪
系统必须以 session 为粒度维护上下文占用状态，每个 session 独立计算、独立刷新、独立展示。

### FR-2: 最大上下文窗口可配置
系统必须支持从模型能力自动获取最大上下文窗口；若无法获取，则允许用户在 Provider 配置中手动填写。

### FR-3: 多来源优先级解析
系统必须按既定优先级解析 context window，并在结果中保留来源信息。

### FR-4: 输入区 badge 展示
系统必须在聊天输入区展示当前 session 的上下文占比或降级态。

### FR-5: hover 明细卡片
系统必须支持在 badge hover 时显示明细面板，包含进度、状态、阈值和 token 明细。

### FR-6: 阈值状态机
系统必须根据当前占比输出统一的阶段状态，并驱动 UI 文案与展示层策略。

### FR-7: UI 展示层紧凑策略
系统必须在高上下文占比阶段对历史内容进行展示层的收敛，但不得修改真实会话数据。

### FR-8: 自动压缩开关
系统必须提供自动压缩开关，允许用户控制是否在 95% 阈值触发自动压缩。

### FR-9: 异步旁路刷新
系统必须保证 context stats 获取、占比刷新、hover 明细更新都在独立异步链路中执行，不阻塞主线问答。

### FR-10: 自动压缩不吞输入
系统必须保证自动压缩失败时保留用户输入内容，并允许继续发送或重试。

### FR-11: 降级与容错
当 context window 不可获取、PI stats 缺失、后端命令超时或 UI 渲染失败时，系统必须优雅降级，不影响聊天功能。

### FR-12: 测试覆盖
系统必须为状态机、数据来源优先级、后端 stats 解析、自动压缩关键分支、badge 与 hover 交互提供测试覆盖。

## Non-Goals (Out of Scope)

- 不在本期实现基于 token 的精确消息裁剪算法替代 PI 自身 compaction。
- 不在本期修改 PI 内部 compaction 策略或其总结内容格式。
- 不在本期为所有 Provider 自动联网抓取最新上下文窗口元数据并实时同步。
- 不在本期实现跨 session 的全局上下文汇总面板。
- 不在本期重做整个聊天历史展示组件。
- 不在本期引入基于 token 预算的消息编辑/改写建议能力。

## Technical Considerations

### 1. 现有前端落点
聊天输入区主结构位于：
- `src/app/chat/ChatWorkspace.tsx`

建议将 context badge 与 hover 卡片接入输入区右侧工具栏区域，而非标题栏模型选择器区域，以满足“任务输入框里面要有一个地方放当前 session 已使用的上下文大小比例”的要求。

### 2. 现有后端能力
当前桌面端通过 `pi --mode rpc --session ...` 运行会话，PI RPC 文档中已存在：
- `get_session_stats`
- `compact`

因此建议优先复用现有 PI RPC 能力，而不是自行重新估算一整套上下文大小。

### 3. 当前模型窗口元数据不足
当前 Rust 侧写入 `models.json` 时只写入基础模型字段，没有稳定写入 `contextWindow`。如果上下文窗口需要更准确的 PI 行为支撑，需要补齐 runtime 配置输出中的 `contextWindow`。

### 4. Provider 配置兼容性
当前 Provider 配置结构较轻，新增 `maxContextTokens` 字段时需保证：
- 老配置可正常反序列化
- 自定义 Provider 与内置 Provider 行为一致
- 不破坏现有设置页保存逻辑

### 5. 异步旁路原则
需要显式避免以下反模式：
- 发送消息前同步等待 context 统计
- 流式输出中串行刷新 stats
- 因 context stats 失败阻断 `stream_pi_prompt`
- 自动压缩失败后丢失用户输入

### 6. 前端测试现状
当前仓库未见成熟前端单测体系，需要将测试基建作为功能交付的一部分，而不是事后补充。

### 7. 受保护文件约束
不得修改：
- `src-tauri/resources/pi-runtime/macos/pi`

所有上下文能力必须通过前端、Rust 命令、runtime 配置生成、PI RPC 调用完成，不能通过修改受保护启动脚本实现。

## Success Metrics

- 90% 以上使用已配置模型的聊天 session 能显示有效 context badge。
- 无论是否拿到自动上下文窗口，用户都可以通过手填配置获得稳定展示。
- 上下文统计失败不会导致聊天发送失败。
- 自动压缩开启时，95% 阈值后的继续对话成功率高于未压缩场景。
- 输入区 badge 与 hover 面板在浅色/深色主题下都具备可读性和稳定布局。
- 新增测试覆盖状态机边界、后端 stats 解析、关键 UI 交互和自动压缩容错分支。

## Open Questions

- 自动压缩开关应放在全局设置、Provider 设置，还是 session 级设置？
- `Snip / Compact / Collapse` 三个阶段的 UI 收敛范围是否需要提供用户自定义偏好？
- 当 PI 返回 `contextUsage.percent = null` 且刚完成 compaction 时，是否需要展示“压缩后待刷新”专用状态？
- 对于 OpenAI 兼容代理平台，一个模型是否需要支持“手填 context window 覆盖自动值”的强制优先选项？
- 自动压缩是否应在首次版本默认关闭，以降低行为变化风险？
- hover 面板是否需要支持点击固定，便于用户长时间查看细节？
- 是否需要在历史列表或 session 标题旁同步展示高风险上下文状态点位？

## Suggested Story Order

1. US-001 手动配置最大上下文窗口
2. US-002 上下文状态模型
3. US-003 后端 session context stats
4. US-004 多来源优先级解析
5. US-005 输入区 badge
6. US-006 hover 明细卡片
7. US-007 阈值状态机
8. US-010 旁路刷新与容错
9. US-011 前端测试基建
10. US-012 Rust 测试
11. US-008 展示层紧凑/折叠策略
12. US-009 95% 自动压缩

## Implementation Notes for This Repo

建议优先关注这些文件与模块：

- `src/app/chat/ChatWorkspace.tsx`
- `src/app/shell/NineClawRouteOutlet.tsx`
- `src/types.ts`
- `src/app/lib/appProviderLlm.ts`
- `src/lib/piClient.ts`
- `src-tauri/src/lib.rs`

建议新增模块示例：

- `src/hooks/useSessionContextWindow.ts`
- `src/components/SessionContextBadge.tsx`
- `src/components/SessionContextPopover.tsx`
- `src/app/lib/sessionContext.ts`
- `src-tauri/src/session_context.rs` 或在现有 `lib.rs` 中拆分对应逻辑
- `src/test/...` 或项目约定的前端测试目录
