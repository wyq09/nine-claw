# NineClaw Widget System Plan

## 目标

构建一套可扩展的交互式 widget 系统，让智能体不只“回复文本”，还可以在对话中：

- 向用户提问并等待结构化回答
- 收集多选、单选、自由输入等表单数据
- 对用户动作做状态回写
- 后续扩展审批、配置确认、批量编辑、任务选择等更多 widget

`ask_user` 是第一类 widget，不应被做成一次性的特殊 UI。

## 当前现状

仓库已经具备三块可复用基础：

1. `responseSegments` 作为对话渲染的结构化通道
2. `DelegateSegmentsBlock` 作为“特殊卡片独立渲染”的前例
3. `agent_loop_review` 作为“用户动作回传给运行时”的前例

但目前还缺少：

- 通用 widget segment 类型
- widget 标准 schema
- widget 提交/取消回传协议
- runtime 侧 pending widget 生命周期管理
- `ask_user` 工具与 widget 系统之间的桥接

## 建议分层

### 1. Widget Schema 层

职责：定义所有 widget 的统一 JSON 协议。

建议标准：

```ts
type WidgetSegment = {
  type: 'widget'
  widget: WidgetDefinition
}
```

第一期只支持：

```ts
type AskUserWidget = {
  kind: 'ask_user'
  widgetId: string
  version: number
  title: string
  description?: string
  submitLabel?: string
  cancelLabel?: string
  allowSkip?: boolean
  status: 'pending' | 'submitted' | 'cancelled' | 'expired'
  questions: AskUserQuestion[]
}
```

设计原则：

- `type` 负责顶层路由
- `kind` 负责 widget 种类扩展
- `version` 负责协议演进
- `status` 负责 UI 展示与重复提交控制

### 2. Widget Render 层

职责：把 `responseSegments` 中的 widget 渲染为独立卡片。

建议目录：

- `src/app/widgets/WidgetSegmentsBlock.tsx`
- `src/app/widgets/AskUserCard.tsx`
- `src/styles/widget-cards.css`

设计原则：

- widget 不混入普通 markdown reply card
- 每个 widget 自己维护局部交互状态
- 提交动作通过 context / callback 上抛，不直接绑死后端

### 3. Widget Action 层

职责：把用户点击“提交/取消”的动作回传给 NineClaw runtime。

建议标准接口：

```ts
submitWidgetResponse({
  widgetId,
  kind,
  answers,
})
```

Rust 侧建议命令：

- `widget_submit_response`
- `widget_cancel_response`

建议事件：

- `widget.requested`
- `widget.updated`
- `widget.resolved`

### 4. Runtime Pending State 层

职责：当工具触发 `ask_user` 后，运行时要暂停该工具调用，并等待用户提交结果。

Rust 侧需要新增：

- `PendingWidgetRequest` 存储结构
- `widget_id -> oneshot sender` 映射
- 超时 / 取消 / 会话关闭清理逻辑
- 持久化恢复策略

核心要求：

- 用户没提交前，tool call 不能假装完成
- 同一个 widget 只能消费一次
- 会话恢复后要么继续等待，要么明确标记失效

## ask_user 工具设计

`ask_user` 不应直接输出 markdown，而应返回结构化 widget 请求。

### 来自 Alice 的关键约束

这部分非常值得直接吸收，而且应该上升为 `ask_user` 专属策略，而不是只写在 prompt 里靠模型自觉。

必须遵守：

1. 工具调用前，助手先用一句自然语言交代背景，不能直接甩卡片
2. 每次只问一个问题
3. 选项 2 到 6 个，最后一个固定是“其他”
4. 第一个选项必须是推荐选项
5. 文案口语化，不要像表单或问卷
6. 不允许退化成正文里的编号选项

不应使用：

- 情绪安抚或闲聊场景
- plan 确认场景
- 不需要澄清即可安全继续的普通执行场景

这意味着 `ask_user` 不只是“一个可交互 widget”，而是“一个在信息不足时强制转向澄清的工具策略”。

建议工具入参：

```ts
{
  title: string
  description?: string
  questions: [AskUserQuestion]
  submitLabel?: string
  allowSkip?: boolean
  timeoutMs?: number
}
```

建议工具执行流程：

1. JS runtime tool 收到 `ask_user` 调用
2. 调用宿主桥接接口创建 pending widget request
3. Rust 生成 `widgetId`，向前端发出 widget segment
4. 前端渲染 card widget
5. 用户提交
6. 前端 invoke `widget_submit_response`
7. Rust 唤醒等待中的工具
8. `ask_user` 返回结构化结果给模型继续推理

建议在工具实现里内建 `validateAskUserToolPolicy()` 校验：

- 如果不是单问题，直接拒绝
- 如果没有推荐项，直接拒绝
- 如果最后不是“其他”，直接拒绝
- 如果选项数量不在 2 到 6 个之间，直接拒绝

这样可以把 Alice 这套交互纪律落实成“运行时保证”，而不是单纯靠模型提示词。

建议工具返回结果：

```ts
{
  ok: true,
  widgetId: string,
  answers: [...]
}
```

取消 / 超时：

```ts
{
  ok: false,
  widgetId: string,
  reason: 'cancelled' | 'expired'
}
```

## 建议标准目录结构

```text
src/
  widgetTypes.ts
  app/widgets/
    WidgetSegmentsBlock.tsx
    AskUserCard.tsx
    __tests__/
  styles/
    widget-cards.css

src-tauri/src/
  widget_runtime.rs
  widget_store.rs
  commands_widgets.rs

src/runtime-tools/
  ask_user_tool.mjs
  ask_user_tool.test.ts
```

## 推荐分期

### Phase 1

目标：先把协议和前端承载层稳定下来。

包含：

- `widget` segment 类型
- `ask_user` schema
- 前端卡片渲染
- 提交 callback 标准
- 单测

本次已完成这一阶段。

### Phase 2

目标：让 `ask_user` 真正可执行。

包含：

- Rust pending widget manager
- Tauri submit/cancel commands
- runtime tool `ask_user_tool.mjs`
- session close / timeout / duplicate submit 处理

### Phase 3

目标：抽象成统一 widget 平台。

包含：

- widget registry
- widget schema versioning
- widget analytics / audit trail
- 其他 widget：`approve`, `config_form`, `select_resource`, `bulk_edit`

## 为什么不建议把 ask_user 做成 reply card 扩展字段

原因很直接：

- reply card 本质是静态内容，不适合承载状态机
- ask_user 需要提交、取消、超时、去重、恢复
- 后续更多 widget 会需要不同交互模型
- 把交互协议藏在 markdown/json fence 里，后面会很难维护

所以建议从第一天就把 `widget` 作为独立 segment 类型。
