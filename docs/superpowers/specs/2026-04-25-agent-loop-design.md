# Agent Loop Design

## Overview

NineClaw 的全新、独立的 ReAct 式多 Agent Loop 引擎。主 Agent 在循环中自主分析任务、委派子 Agent、收集结果、决策下一步，直到任务完成。与现有 supervisor/worker 委派系统完全独立，互不影响。

## 设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 执行模式 | ReAct 式多 Agent 循环 | 主 Agent 自主决策下一步，灵活处理复杂任务 |
| 与现有委派的关系 | 全新独立机制 | 不影响已验证的 team 委派系统，独立演进 |
| 子 Agent 来源 | 用户预创建，主 Agent 选择 | 用户控制 Agent 能力边界，主 Agent 负责选择 |
| 上下文传递 | 最小化传递 | 只传任务描述和必要信息，节省 token |
| 并发支持 | 批量标记 + tokio::JoinSet | batch 标记算 1 次迭代，高效利用轮次 |
| 并行控制 | 主 Agent prompt 驱动，不在子 Agent 加配置 | 并行与否取决于任务关系，非 Agent 属性 |
| 前端展示 | 可折叠，子 Agent 流式输出 | 用户感知到子 Agent 在工作，不会觉得卡住 |
| 循环上限默认值 | 50 轮，可配置 | 覆盖大多数复杂任务，支持动态扩容 |

## 标记协议

### NC_AGENT_LOOP_CALL_JSON — 单次委派

主 Agent 输出，委派一个子 Agent 执行任务。

```json
{
  "agent_id": "string — 目标子 Agent ID",
  "task": "string — 任务描述",
  "params": { "key": "value — 结构化参数" },
  "context_injection": "string? — 额外传递的上下文片段",
  "expect_structured_output": "boolean — 是否要求结构化输出",
  "output_format_hint": "string? — 输出格式说明",
  "pause_for_review": "boolean — 是否暂停等待人工确认"
}
```

后端解析后：
- `pause_for_review: true` → 暂停 loop，前端弹出确认卡片
- `params` 序列化后注入子 Agent prompt 前缀

### NC_AGENT_LOOP_BATCH_JSON — 批量并发委派

主 Agent 需要同时委派多个子 Agent 时输出。

```json
{
  "calls": [
    {
      "agent_id": "string",
      "task": "string",
      "params": { "key": "value" },
      "context_injection": "string?",
      "expect_structured_output": "boolean",
      "output_format_hint": "string?"
    }
  ],
  "pause_for_review": "boolean — 整个 batch 是否需要人工确认"
}
```

一个 batch 算 1 次迭代。后端用 `tokio::JoinSet` 并发执行，上限 `max_concurrent`。

### NC_AGENT_LOOP_RESULT_JSON — 子 Agent 结果回注

后端执行完子 Agent 后生成，注入主 Agent 对话上下文。

单次委派结果：
```json
{
  "agent_id": "string",
  "agent_name": "string",
  "task": "string — 原始任务",
  "status": "success | error | cancelled",
  "output": "string — 子 Agent 最终文本回复",
  "tool_calls_count": "number",
  "duration_ms": "number"
}
```

批量委派结果：
```json
{
  "batch_id": "string",
  "results": [
    {
      "agent_id": "string",
      "agent_name": "string",
      "task": "string",
      "status": "success | error | cancelled",
      "output": "string",
      "tool_calls_count": "number",
      "duration_ms": "number"
    }
  ],
  "total_duration_ms": "number"
}
```

### NC_AGENT_LOOP_EXTEND_JSON — 申请扩大循环数

主 Agent 判断剩余迭代不够时输出。

```json
{
  "current_iteration": "number",
  "max_iterations": "number",
  "reason": "string — 为什么需要更多迭代",
  "requested_extra": "number — 请求额外增加的迭代数"
}
```

后端暂停 loop，前端弹出确认卡片。用户批准则更新本次 loop 的 max_iterations，拒绝则 loop 以当前结果正常结束。

### NC_AGENT_LOOP_FINAL — 显式结束（可选）

主 Agent 输出此标记表示 loop 结束。如果回复中无任何 CALL/BATCH 标记，loop 也自然结束。

## 循环控制参数

```rust
struct AgentLoopConfig {
    max_iterations: u32,           // 默认 50，用户可配置
    iteration_timeout_ms: u64,     // 每次子 Agent 超时，默认 120000
    enable_nested: bool,           // 允许嵌套委派，默认 true
    max_depth: u32,                // 嵌套最大深度，默认 3
    allow_extend: bool,            // 允许主 Agent 申请扩大循环数，默认 true
    max_extend_limit: u32,         // 单次 loop 最大允许扩展到的上限，默认 200
    max_concurrent: u32,           // 单次 batch 最大并发数，默认 5
    batch_fail_strategy: BatchFailStrategy,  // 并发失败策略，默认 WaitAll
}

enum BatchFailStrategy {
    FailFast,   // 任一失败 → 取消其余
    WaitAll,    // 等全部完成，失败的标记 error
}
```

存储在 `agents` 表的 `agent_loop_config` JSON 字段中。为 NULL 时不启用 Agent Loop。

## 嵌套委派

当子 Agent 也有 `agent_loop_config` 且 `enable_nested: true` 时，子 Agent 的执行也走同样的 loop 逻辑。递归解析标记，`depth + 1`，超过 `max_depth` 时后端返回错误结果。

## LLM 调用前的防御层

每次迭代调用 PiBridge 前，执行两步：

### 1. 修复孤立 tool_call

扫描对话历史，发现 tool_call 无对应 tool_result（中断/崩溃导致）时，注入合成 tool_result：

```json
{
  "tool_call_id": "<原始 id>",
  "role": "tool",
  "content": "[NineClaw] 此工具调用因会话中断未完成，请根据已有信息继续。"
}
```

### 2. 动态内容注入

将当前时间和权限拒绝摘要追加到最后一条 system 消息末尾（放末尾保护 KV Cache prefix）：

```
[动态上下文]
当前时间：2026-04-25 14:30 CST
权限拒绝：无（或：X 工具被用户拒绝，请换用 Y 方式）
```

封装为 `prepare_loop_iteration()` 函数。

## 两个循环层级

Agent Loop 涉及两个嵌套的循环，职责不同：

**外层循环（Agent Loop）：** 主 Agent ↔ 子 Agent 之间的委派循环。由标记协议驱动，后端 `run_agent_loop()` 控制。每轮迭代 = 主 Agent 输出标记 → 执行子 Agent → 回注结果。

**内层循环（ReAct）：** 单个 Agent 内部的 tool_call 循环。由 PI runtime 原生处理。每轮 = LLM 输出 tool_call → 执行工具 → 回注 tool_result → LLM 继续输出。

Agent Loop Engine 不干预内层 ReAct 循环，只在子 Agent 整体回复完成后才介入。

## Agent 内部 ReAct 循环优化

每个 Agent（主 Agent 和子 Agent）内部的 tool_call 循环遵循以下规则：

### 写入 assistant 消息（token 优化）

`compress_assistant_message()` 函数在写入对话历史前执行：如果本轮有 tool_call 且伴随文字 < 50 字（说明性文字如"让我查一下..."），丢弃文字，只保留 tool_call。文字包含实质性推理则保留。

### 终止判断

本轮无 tool_call → Agent 认为任务完成，内部 ReAct 循环结束，控制权回到外层 Agent Loop。

### 工具执行（并发安全分批）

工具定义带 `isConcurrencySafe` 属性：
- safe 组：tokio::JoinSet 并发执行，上限 max_concurrent
- unsafe 组：按顺序串行执行

所有 tool_result 收集完毕后写回上下文，进入下一轮 ReAct 迭代。

## 循环终止条件与善后

| # | 条件 | 触发者 | 善后逻辑 |
|---|------|--------|----------|
| 1 | 无工具调用（无 CALL/BATCH 标记） | LLM 自然停止 | 正常完成，emit `completed` |
| 2 | 达到 max_iterations | 后端兜底 | 注入系统消息让主 Agent 做一次总结，emit `completed` with reason |
| 3 | AbortSignal | 用户取消 | AtomicBool 穿透所有等待层，已完成结果不丢弃，emit `aborted` |
| 4 | 不可恢复错误 | 系统异常 | 不重试，错误信息回注，emit `error` |

### 达到 max_iterations 的特殊处理

不直接硬截断，向主 Agent 追加系统消息：「已达循环上限，请根据已有结果给出最终回复」，给一次收尾机会（不计入迭代次数）。

### AbortSignal 穿透机制

`loop_state.abort.store(true, Relaxed)` → PiBridge SIGTERM/SIGKILL → JoinSet CancellationToken → 嵌套 loop 递归检查父 abort flag。

### 不可恢复错误

```rust
enum UnrecoverableError {
    ContextCorrupted,
    InvalidModelOutput,
    SafetyViolation,
    AgentNotFound(String),
    ProviderAuthFailed,
    NestedDepthExceeded,
}
```

## 后端架构

### 新增文件

```
src-tauri/src/
├── agent_loop.rs           — Agent Loop 引擎核心，~300-400 行
├── agent_loop_types.rs     — 标记协议、配置、事件类型
└── lib.rs                  — 修改 stream_pi_prompt 末尾
```

### 核心流程

```
stream_pi_prompt（现有入口）
    ↓ 主 Agent 回复完成
    ↓ 检查 agent_loop_config
    ↓ 无 config → 现有逻辑不变
    ↓ 有 config →
    ↓
run_agent_loop()
    while iteration < max_iterations && !abort {
        prepare_loop_iteration()     // 防御层
        调用主 Agent（PiBridge）
        解析回复中的标记
        match 标记类型 {
            CALL → execute_single_delegate()
            BATCH → execute_batch_delegates()   // tokio::JoinSet
            EXTEND → emit_review_card() → 等用户
            无标记 → break
        }
        回注结果到主 Agent 上下文
        iteration++
    }
    善后处理
```

### 运行时状态

```rust
struct AgentLoopState {
    loop_id: String,
    agent_id: String,
    session_id: String,
    iteration: u32,
    max_iterations: u32,
    depth: u32,
    started_at: Instant,
    history: Vec<LoopIteration>,
}
```

不持久化到数据库。loop 是运行时临时状态，崩溃后用户重新发起即可。对话历史中有 RESULT 标记记录，不丢失上下文。

### 全局状态管理

```rust
pub struct ActiveLoops {
    loops: Mutex<HashMap<String, ActiveLoopHandle>>,
}

pub struct ActiveLoopHandle {
    abort_flag: Arc<AtomicBool>,
    review_sender: Option<Sender<ReviewResponse>>,
    state: Arc<Mutex<AgentLoopState>>,
}
```

挂在 Tauri managed state 上，loop 完成后自动移除。

## 数据存储

### SQLite 变更

```sql
ALTER TABLE agents ADD COLUMN agent_loop_config TEXT;
```

JSON 字符串，与现有 `capability_policy` 等字段风格一致。

`chat_turns` 表无需新增字段。Agent Loop 迭代结果以 RESULT 标记嵌入 assistant 消息文本中，随对话历史自然持久化。

不新建独立表。

### Rust AgentRecord 扩展

```rust
// agents.rs
pub agent_loop_config: Option<AgentLoopConfig>,
```

## Tauri API

### 现有命令扩展

无新命令触发 loop。`stream_pi_prompt` 检测到 `agent_loop_config` 后自动进入 loop 模式。

### 新增命令

```rust
#[tauri::command]
async fn agent_loop_respond_review(
    loop_id: String,
    approved: bool,
    extend_to: Option<u32>,
    app: AppHandle,
) -> Result<(), String>;

#[tauri::command]
async fn agent_loop_abort(
    loop_id: String,
    app: AppHandle,
) -> Result<(), String>;
```

### 事件清单

| 事件 | Payload | 前端动作 |
|------|---------|----------|
| `agent-loop://started` | `{ loop_id, config }` | 创建 AgentLoopBlock |
| `agent-loop://delegate/chunk` | `{ loop_id, iteration, agent_id, delta }` | 子 Agent 卡片追加文字 |
| `agent-loop://delegate/tool_start` | `{ loop_id, iteration, agent_id, tool_name }` | 工具行 ⏳ |
| `agent-loop://delegate/tool_end` | `{ loop_id, iteration, agent_id, tool_name, status }` | 工具行 ✅/❌ |
| `agent-loop://iteration/end` | `{ loop_id, iteration, result }` | 卡片收缩为摘要 |
| `agent-loop://review/request` | `{ loop_id, review_type, ... }` | 弹出审核卡片 |
| `agent-loop://completed` | `{ loop_id, reason, total_iterations, duration_ms }` | 显示底部统计 |
| `agent-loop://aborted` | `{ loop_id, iterations_completed }` | 标记已取消 |
| `agent-loop://error` | `{ loop_id, error }` | 显示错误信息 |

## 前端架构

### 类型扩展（src/types.ts）

```typescript
interface AgentLoopSegment {
  type: "agent_loop";
  loop_id: string;
  status: "running" | "completed" | "aborted" | "error";
  reason?: "max_iterations" | "abort" | "error";
  total_iterations: number;
  current_depth: number;
  iterations: AgentLoopIteration[];
  started_at: number;
  completed_at?: number;
}

interface AgentLoopIteration {
  iteration: number;
  marker_type: "call" | "batch" | "extend";
  status: "pending" | "running" | "reviewing" | "completed";
  delegate?: {
    agent_id: string;
    agent_name: string;
    task: string;
    params?: Record<string, unknown>;
    output?: string;
    tool_calls_count?: number;
    duration_ms?: number;
  };
  batch?: {
    delegates: Array<{
      agent_id: string;
      agent_name: string;
      task: string;
      status: "running" | "completed" | "error" | "cancelled";
      output?: string;
      duration_ms?: number;
    }>;
  };
}

interface AgentLoopReviewSegment {
  type: "agent_loop_review";
  loop_id: string;
  review_type: "pause_for_review" | "extend";
  iteration: number;
  delegate_info?: { agent_name: string; task: string };
  extend_info?: { current_max: number; requested_max: number; reason: string };
  status: "pending" | "approved" | "rejected";
}
```

### 组件结构

```
src/components/agent-loop/
├── AgentLoopBlock.tsx         — 容器组件
├── AgentResultCard.tsx        — 子 Agent 结果卡片（收起/展开）
├── AgentResultDetail.tsx      — 展开后的详细执行过程
├── BatchResultCard.tsx        — 并发卡片（tabs 切换）
├── ReviewCard.tsx             — 审核确认 + 扩容请求
└── LoopStatsLine.tsx          — 底部统计小字
```

### 前端展示行为

**子 Agent 执行中：** 卡片实时流式展示文字输出 + 工具调用状态（单行缩写），用户能看到 Agent 在工作。

**子 Agent 完成后：** 卡片自动收缩为摘要（名称 + 状态 + 一行摘要），点击可展开完整输出。

**轮次显示：** 每个卡片带 #N 序号。不显示进度比（如 3/50），除非触达上限需要扩容。

**扩容请求：** 弹出确认卡片，用用户能理解的语言（不暴露"迭代次数"等技术概念）。

**批量并发：** 并排展示所有子 Agent 的流式输出，窄屏自动堆叠。

### piClient 扩展

```typescript
export async function agentLoopRespondReview(
  loopId: string, approved: boolean, extendTo?: number
) {
  return invoke("agent_loop_respond_review", { loopId, approved, extendTo });
}

export async function agentLoopAbort(loopId: string) {
  return invoke("agent_loop_abort", { loopId });
}
```

### usePiAgent hook 扩展

订阅 `agent-loop://*` 事件，维护 AgentLoopSegment 状态，驱动 UI 更新。

## 改动范围总结

| 模块 | 文件 | 改动类型 |
|------|------|----------|
| 类型定义 | `src-tauri/src/agent_loop_types.rs` | 新增 |
| 核心引擎 | `src-tauri/src/agent_loop.rs` | 新增，~300-400 行 |
| 集成入口 | `src-tauri/src/lib.rs` | 修改 stream_pi_prompt 末尾 |
| 数据模型 | `src-tauri/src/agents.rs` | AgentRecord 新增字段 + migration |
| 前端组件 | `src/components/agent-loop/*.tsx` | 新增 6 个组件 |
| 前端类型 | `src/types.ts` | 新增 segment 类型 |
| 前端 API | `src/lib/piClient.ts` | 新增 2 个调用 |
| Hook | `src/hooks/usePiAgent.ts` | 订阅新事件 |
| Agent 编辑 | `src/app/agents/AgentDialogsBundle.tsx` | 新增 loop config 编辑区 |
