# 报告 02：工具执行流水线 + 能力 Seam + 错误归因

> 对照基准：dsh 的工具 waterfall（pre/execute/post 三层中间件 + 守卫 + 审批 + finalizeContent）、能力 Seam 三角色、错误归因（拒绝≠runner 失败）。
> **可抄度：🟡 中**——本报告的核心结论是：**dsh 的工具流水线无法直接搬到 Rust，因为工具根本不经过 Rust**。可行的 seam 只有 JS 扩展层。

---

## 0. TL;DR 与可行性结论

| # | 动作 | 改哪 | 抄自 dsh | 可行性 |
|---|------|------|---------|--------|
| ① | 侦察 PI 扩展协议：`pi.on(...)` 还支持哪些事件、返回值能做什么 | 读 PI 源码/文档 | — | **前置必做** |
| ② | JS `tool_call` 单点钩子升级为**可组合的 pre 规则链** | `managed_runtime_extension.rs` | `tools/pre-execute` waterfall | 🟢（受 ① 约束） |
| ③ | 若 PI 支持，加 **post-execute 钩子**（结果 transform / add-context / block-after） | `managed_runtime_extension.rs` | `tools/post-execute` | 🟡（取决于 ①） |
| ④ | Rust 侧工具**错误归因**：denial vs 真失败 vs 非零退出 | `lib.rs` tool 事件转发 | `denialSignatures`/`runnerFailureRules` | 🟢（纯 Rust 观测层） |
| ⑤ | 把 `run_guard_chain`（已有 3 层 + 默认-deny 审批）文档化为**结构范本** | 文档 | waterfall + approval | 🟢（零代码） |
| ⑥ | （未来）`ToolExecutor` trait 空壳，为"工具回吐 Rust / 远程沙箱"留扩展点 | 新 trait | Capability Seam | 🟡（前瞻） |

> **最该抄且现在能抄的**：④（错误归因）和 ⑤（把已有 guard chain 识别为范本）。②③ 需先做 ① 侦察。

---

## 1. 当前态

### 1.1 agent loop：两个不同的 loop，工具流水线在 PI 内

必须区分，否则会把元编排误当工具流水线：

**A. 真正的"每轮工具执行 loop"——跑在 PI 内部，nine-claw 不实现。**
`stream_pi_prompt`（`lib.rs:4523`）是 **stdout 事件转发器**，不是 loop 驱动：
- stdin 写一条 prompt（`send_pi_prompt_command` `lib.rs:1949`）
- `loop{}`（`lib.rs:5225`）逐行读 PI stdout 的 JSON，按 `type` 分发：`tool_execution_start`（`lib.rs:5795`）/`tool_execution_update`（`lib.rs:5864`）/`tool_execution_end`（`lib.rs:5893`）/`turn_end`（`lib.rs:5468`）
- 迭代上限：计数 `turn_end`，到 `max_agent_tool_rounds_per_dialogue`（默认 80，`runtime_parameters.rs:16`）→ stdin 写 `{type:"abort"}`（`runtime_parameters.rs:68`）
- 超时：总运行 + 首包/空闲，靠 `kill_child_with_trace` 杀进程

**B. "Agent Loop" feature（`agent_loop.rs`）——子智能体委派的元编排 loop，非工具流水线。**
4-phase（Observe/Plan/Execute/Verify，`agent_loop_types.rs:237`），LLM 文本 marker 驱动（`NC_AGENT_LOOP_CALL_JSON:` 等 `agent_loop.rs:34`）。Execute 调 `delegate_to_agent`（`agent_loop.rs:503`）→ 为子智能体再起一个 PI。

### 1.2 工具注册与执行：全在 PI

工具**不在 Rust 注册**，注册表是 PI 的。nine-claw 通过三种产物间接影响：
1. **harness JSON**：`restrict_harness_to_agent_tools`（`managed_runtime.rs:564`）把 agent 的 `active_tools: Vec<String>`（`HarnessDefinition` `managed_runtime.rs:61`）写进 `harness-effective-*.json`，PI 据此启用工具
2. **JS bootstrap 扩展**（`managed_runtime_extension.rs` 生成）：在 PI 进程内 `pi.registerTool(...)` 注册 nine-claw 自带工具（web_search/web_fetch/image_generation/agent_delegate/ask_user/mcp_tool/memory_*/…，`:802-830`）
3. **单工具 `.mjs`**（`include_str!` 嵌入）：mcp_tool/memory_update/memory_search/chat_search（`:50-66`）

**唯一现有的 pre-execute 钩子（跑在 PI 的 JS 里）**：`pi.on("tool_call")`（`managed_runtime_extension.rs:871`）做两件事：
- 连续 3 次相同工具+参数 → block（`TOOL_REPEAT_LIMIT=3` `:472`，逻辑 `:880-885`）
- bash 命令含内联凭证（`authorization: bearer`/`x-api-key`/`api_key=`/`token=`）→ block（`:894-899`）
- 返回 `{block:true, reason}` 或 `undefined`

**无 post-execute 钩子、无 result transform、无 add-context、无单工具超时/重试/指标。**

### 1.3 能力 Seam：不存在三角色抽象

`AgentCapabilityPolicy`（`agent_capabilities.rs:13`）**不是工具清单/seam**，而是**技能选择策略**（`required_skill_ids`/`forbidden_skill_ids`/`max_dynamic_skills`/`forbidden_paths`/`high_risk_actions`）。

全仓搜不到 `ToolProvider`/`ExecutionBackend`/`ServiceDefinition` trait。唯一有真 trait 的子系统是 **embedding**（`ProviderRegistry` + `Provider`，`managed_runtime.rs:2066`），但仅限向量检索。"换执行后端"（如远程沙箱统一搬 Bash/PTY/LSP）**当前不可能**。

### 1.4 错误归因与审批

- **工具失败归因：无。** 只从 `tool_execution_end` 读 `isError`（`lib.rs:5904`）原样转发，不区分"被拦截 / runner 真失败 / 非零退出"。
- **审批：只在子智能体委派层有。** `run_guard_chain`（`agent_loop.rs:1260`）三层：Policy（预算/重试/连续失败 `:1335`）→ Safety（禁路径/禁技能/高风险→需审批 `:1376`）→ Human Approval（oneshot channel，**超时/中止默认 deny** `:1430`，与 dsh"缺失/不可答=deny"一致）。但门控的是 `GuardedAction`（`agent_loop_types.rs:373`，"能否委派给子智能体 X 做 Y"），**不是工具层**。
- **per-call 工具 policy：不存在。**

---

## 2. dsh 做法（对照）

### 2.1 工具执行 waterfall

```
model tool-call block
  → tool/call (落日志)
  → tools/pre-execute  (waterfall: hook/权限/沙箱)
  → 注册的单调守卫      (deny 或 abstain，identity protected)
  → ctx.approval       (一次性 prompt；缺失/不可答 = deny)
  → tools/execute      (around dispatch: 超时/重试/指标)
  → tool body 执行     (fs/write-intent / tool-owned events)
  → tools/post-execute (accept/block/replace/add context)
  → finalizeContent    (内容不变量)
  → tools/result       (冻结的权威结果)
```
- **三层 waterfall 都能 transform 调用**；hook 可跨工具族而不耦合工具到某一策略
- 结果返回前被"无损快照 + 归一化"（pipeline/result snapshot throw → isError）

### 2.2 错误归因（"拒绝 ≠ runner 失败"）

`ConfinedArgv` 带两类正交 stderr 分类器：
- `denialSignatures`：命令被沙箱**正确拦截**（EROFS/EACCES/EPERM）
- `runnerFailureRules`：沙箱 runner 在执行命令**前**就失败（消费者先检查这个，再判 denial）
- **退出码单独永远不能证明 runner 失败**

### 2.3 能力 Seam 三角色

每项能力 = Service Definition（接口）+ Provider（实现）+ Consumer（使用）。fs 和 subprocess Provider 共享执行世界 → 指向远程沙箱则 Bash/PTY/LSP 一并搬走，无需各自 fork。

---

## 3. 分步改造方案

### 步骤 ① 侦察 PI 扩展协议（前置，0.5 天）

**这是解锁 ②③ 的前提。** 现状：`pi.on("tool_call")` 只支持返回 `{block}`/`undefined`（`managed_runtime_extension.rs:871`）。要抄 dsh 的 pre/post waterfall，必须先知道：

- `pi.on(...)` 还能监听哪些事件？是否有 `tool_result`/`tool_after`/`message_after`？
- 钩子返回值能做哪些事？除了 `{block:true}`，能否 `replace` 参数 / `add_context` / 改写 result？
- PI 是否支持"工具执行回吐宿主"协议（即 PI 把 tool_call 通过 RPC 交给 nine-claw 执行）？

**做法**：读 PI 包源码（`@earendil-works/pi-coding-agent` / `@mariozechner/pi-coding-agent`，在 `node_modules` 或 pi-package 里）grep `registerTool`/`\.on\(`/hooks/`emit`，整理一份"PI 扩展协议能力清单"存到 `docs/research-dsh/pi-extension-protocol.md`。

### 步骤 ② 可组合的 pre 规则链（P2，1-2 天，受 ① 约束）

把 `managed_runtime_extension.rs:871` 的单点钩子重构成**有序规则链**：

```js
// 伪代码（生成进 JS bootstrap）
const preRules = [
  repeatGuard({ limit: 3 }),           // 现有：连续相同调用
  credentialScan({                     // 现有：bash 内联凭证
    patterns: ['authorization: bearer', 'x-api-key', 'api_key=', 'token='],
  }),
  forbiddenPath({ roots: agentForbiddenPaths }),   // 新：禁路径（复用 capability_policy.forbidden_paths）
  highRiskGate({ actions: highRiskActions }),      // 新：高风险 → ask（若 PI 支持 ask）
];
pi.on('tool_call', (call) => {
  for (const rule of preRules) {
    const verdict = rule(call);        // { allow } | { block, reason } | { ask, prompt }
    if (verdict.block) return { block: true, reason: verdict.reason };
    if (verdict.ask)   return { ask: verdict.prompt };  // 取决于 ①
  }
  return undefined;                    // 全 allow → 委托 PI 执行
});
```

收益：新增策略（如"禁止写 .env"/"禁止 rm -rf 根"）只需加一条规则，不动核心。每条规则是纯函数，可在 JS 侧单测。

**验收测试**（JS 单测 + 一个 Rust 集成测试驱动 PI 跑真工具）：
- repeatGuard 第 4 次相同调用被 block
- credentialScan 拦截 `curl -H "authorization: bearer xxx"`
- forbiddenPath 拦截写 agent `forbidden_paths` 之外的路径
- 全 allow 的正常工具不受影响

### 步骤 ③ post-execute 钩子（P2，取决于 ①）

若 ① 侦察确认 PI 支持 `tool_result`/after 事件：
- 加 `pi.on('tool_result', ...)`：对 result 做 transform（如截断超长输出）/ add-context（如给模型补充"上次同类命令的结果"）/ block-after（如检测到危险副作用后阻止续跑）
- 若不支持，**放弃此步**，记录到 `docs/research-dsh/pi-extension-protocol.md` 作为 PI 能力缺口。

### 步骤 ④ Rust 侧工具错误归因（P1，1 天，纯 Rust 现在就能做）

即使工具在 PI 内执行，nine-claw 仍可在**观测层**改进错误归因，让 trace/日志能区分失败类型。在 `tool_execution_end` 处理（`lib.rs:5893`）：

```rust
#[derive(Debug)]
enum ToolFailureKind {
    BlockedByPolicy,      // JS 钩子返回 {block}（从事件 payload 识别）
    DeniedSandbox,        // stderr 含 denial signatures（若未来有沙箱）
    RunnerFailed,         // runner 在执行命令前失败
    TaskError,            // 工具正常执行但返回错误（普通非零退出/业务错误）
    Success,
}

fn classify_tool_failure(end_payload: &Value, stderr: Option<&str>) -> ToolFailureKind
```

接入：
- `tool_execution_end` 转发时把 `failure_kind` 写进 session_llm_log / llm_trace / 前端事件
- 不改变 PI 行为，只让"为什么失败"在 nine-claw 侧可观测、可统计、可报警

**验收测试**：
- 一个被 repeatGuard block 的调用 → `BlockedByPolicy`
- 一个正常报错的工具（如读不存在文件）→ `TaskError`
- 两者不被混为一类（这正是 dsh"退出码单独不能证明 runner 失败"的精神）

### 步骤 ⑤ 把 `run_guard_chain` 文档化为范本（P0，0.5 天，零代码）

`run_guard_chain`（`agent_loop.rs:1260`）已经是**三层 waterfall + 默认-deny 审批**的优秀实现（Policy→Safety→Approval，超时/中止默认 deny `:1430`）。这与 dsh 的 `tools/pre-execute → 守卫 → approval` 结构同构。

**动作**：在 `docs/research-dsh/` 或代码注释里明确标注：
- 它就是 nine-claw 现有的"工具级 waterfall"范本（虽门控委派而非工具）
- 未来若工具搬进 Rust，照此结构扩展到工具层
- 抽出其"规则链 + verdict 枚举 + 默认 deny"为可复用模式

这一步零代码，但**避免未来重复造轮子**——它是抄 dsh 的现成结构载体。

### 步骤 ⑥（前瞻，P3）`ToolExecutor` trait 空壳

为"工具回吐 Rust / 远程沙箱统一搬迁"留扩展点，定义 trait 但暂不实现：

```rust
/// 能力 Seam 的 Consumer 侧：一个工具的执行后端接口。
/// 现状：唯一实现是 PI 进程内执行（黑盒）。未来可加 RemoteSandboxExecutor。
trait ToolExecutor: Send + Sync {
    fn execute(&self, call: &ToolCall, policy: &ToolPolicy) -> ToolOutcome;
}
struct ToolPolicy { mode: SandboxMode, workspace_root: PathBuf, session_id: SessionId }
enum ToolOutcome { Success(Value), Denied(String), Failed(String), Blocked(String) }
```
仅定义 + 一个 `PiToolExecutor` 占位实现。价值是**让后续"远程沙箱"需求有明确落点**，而非真改架构。配合报告 03 的 Scope 原语，未来 per-agent 换后端才有可能。

---

## 4. 工作量与风险

| 步骤 | 工作量 | 风险 | 阻塞 |
|------|--------|------|------|
| ① 侦察 | 0.5 天 | 低 | 无（只读） |
| ② pre 规则链 | 1-2 天 | 中（JS 生成路径改动需回归现有工具） | ① |
| ③ post 钩子 | 1 天 | 中 | ①（可能做不了） |
| ④ 错误归因 | 1 天 | 低（纯观测层） | 无 |
| ⑤ 文档化范本 | 0.5 天 | 无 | 无 |
| ⑥ trait 空壳 | 0.5 天 | 低 | 无 |

## 5. 边界与不建议（重要）

- **不建议**把 dsh 的工具 waterfall 整条重写到 Rust——工具在 PI 内执行，Rust 层重写只会得到一个**永远不被调用的死代码流水线**。这是本报告最重要的边界判断。
- **不建议**为了抄 Seam 而把 `AgentCapabilityPolicy` 改名/重构——它当前是 skill 选择策略且工作良好，强行套"能力 Seam"会制造术语混淆。Seam 是新需求（远程沙箱）的载体，不是现有代码的重构理由。
- ②③ 高度依赖 PI 扩展协议能力。若 ① 侦察发现 PI 钩子只能 `{block}`，则 ②③ 的天花板就是"更丰富的 block 规则"，**不可强求** transform/add-context/ask。诚实记录 PI 缺口，必要时向 PI 上游提 issue。
- 唯一**无 PI 依赖、现在就能做、且直接对标 dsh** 的是 **④ 错误归因**（观测层）和 **⑤ 文档化已有 guard chain**。建议先落地这两步。
